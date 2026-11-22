//! Reference `githubfs` aP file server and `routefs` event dispatcher.
//!
//! Exports the canonical `/srv/github` -> `/mnt/github` tree:
//! - `ctl`: commit-on-clunk control (`watch <owner/repo>`, `unwatch <owner/repo>`, `sync`)
//! - `status`: read-only JSON summary (watched repos, cursor, rate-limit budget, ETags)
//! - `events`: read-only append-only NDJSON `alan_ap::Stream`
//! - `repos/<owner>/<repo>/issues/new`: commit-on-clunk JSON issue create (`{number, url}`)
//! - `repos/<owner>/<repo>/issues/<n>/{meta.json,body.md,comments,ctl}`
//! - `repos/<owner>/<repo>/pulls/<n>/{meta.json,checks.json,reviews.json,ctl}`

pub mod backend;
pub mod dispatch;
mod gh_cli;
#[cfg(test)]
mod tests;
mod tree;

use std::collections::BTreeSet;
use std::sync::Arc;

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, WireError,
    export_file_server,
};
use async_trait::async_trait;
use tokio::io::{AsyncBufRead, AsyncWrite, BufReader};
use tokio::sync::Mutex;

pub use backend::{
    CreatedIssue, GhCliGithubBackend, GithubBackend, GithubBackendError, GithubCheckRun,
    GithubComment, GithubEvent, GithubIssueMeta, GithubPullChecks, GithubPullMeta,
    GithubPullReviews, GithubReview, GithubReviewThread, GithubStatusSnapshot, IssueRecord,
    MAX_CTL_BYTES, MAX_DOC_BYTES, MemoryGithubBackend, NewIssueRequest, PullRecord,
    RateLimitStatus, RateLimitUpdate, RepoSlug, SyncRepoData, SyncRepoOutcome,
    redact_github_tokens,
};
pub use dispatch::{
    CoalesceKey, DispatchOutcome, DispatchStats, GithubEventDispatcher, HUMAN_REVIEW_PORT,
    IssueKey, SkipReason, TriggerAllowlist, WORKER_PORT, install_github_dispatch_rules,
    new_routefs_transport,
};
use tree::{
    CachedIssue, CachedPull, FsState, GithubFid, Node, encode_pretty_json, join_lines,
    parse_positive_u64, parse_single_line_ctl, slice_bytes,
};

/// Canonical `/srv` handle name for `githubfs`.
pub const GITHUB_SRV_HANDLE: &str = "github";
/// Canonical namespace mount path for `githubfs`.
pub const GITHUB_MOUNT_PATH: &str = "/mnt/github";

/// GitHub aP [`FileServer`] backed by a [`GithubBackend`].
pub struct GithubFs<B: GithubBackend> {
    backend: Arc<B>,
    state: Mutex<FsState>,
    sync_lock: Mutex<()>,
}

impl<B: GithubBackend> GithubFs<B> {
    /// Construct a new `GithubFs` wrapping `backend`.
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            backend,
            state: Mutex::new(FsState::new()),
            sync_lock: Mutex::new(()),
        }
    }

    /// Construct a `GithubFs` with initial watched repository slugs.
    pub async fn with_watched_repos<I>(backend: Arc<B>, repos: I) -> Result<Self, ErrorCode>
    where
        I: IntoIterator<Item = RepoSlug>,
    {
        let fs = Self::new(backend);
        fs.state.lock().await.watched_repos.extend(repos);
        Ok(fs)
    }

    /// Access the underlying backend handle.
    pub fn backend(&self) -> &Arc<B> {
        &self.backend
    }

    /// Trigger one conditional (`If-None-Match`) sync pass across all watched repositories.
    pub async fn sync_watched_repos(&self) -> Result<(), ErrorCode> {
        let _sync_guard = self.sync_lock.lock().await;
        let targets: Vec<(RepoSlug, Option<String>)> = {
            let state = self.state.lock().await;
            state
                .watched_repos
                .iter()
                .map(|slug| {
                    let etag = state.repos.get(slug).and_then(|c| c.etag.clone());
                    (slug.clone(), etag)
                })
                .collect()
        };

        let mut first_error: Option<ErrorCode> = None;
        for (slug, etag) in targets {
            match self.backend.sync_repo(&slug, etag.as_deref()).await {
                Ok(SyncRepoOutcome::NotModified { rate_limit }) => {
                    self.state.lock().await.record_not_modified(rate_limit);
                }
                Ok(SyncRepoOutcome::Updated(data)) => {
                    let (lines, stream) = self.state.lock().await.record_sync_update(&slug, data);
                    if !lines.is_empty() {
                        stream.append(&lines).await;
                    }
                }
                Err(err) => {
                    self.state.lock().await.record_sync_error(&err.message);
                    first_error.get_or_insert(err.code);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    async fn known_repos(&self) -> Result<BTreeSet<RepoSlug>, ErrorCode> {
        let mut repos: BTreeSet<RepoSlug> = {
            let state = self.state.lock().await;
            state
                .watched_repos
                .iter()
                .chain(state.repos.keys())
                .cloned()
                .collect()
        };
        if let Ok(backend_repos) = self.backend.list_repos().await {
            repos.extend(backend_repos);
        }
        Ok(repos)
    }

    async fn repo_exists(&self, slug: &RepoSlug) -> Result<bool, ErrorCode> {
        {
            let state = self.state.lock().await;
            if state.watched_repos.contains(slug) || state.repos.contains_key(slug) {
                return Ok(true);
            }
        }
        self.backend.has_repo(slug).await.map_err(|e| e.code)
    }

    async fn issue_exists(&self, slug: &RepoSlug, number: u64) -> Result<bool, ErrorCode> {
        if number == 0 {
            return Ok(false);
        }
        match self.read_issue_record(slug, number, false).await {
            Ok(_) => Ok(true),
            Err(ErrorCode::NotFound) => Ok(false),
            Err(code) => Err(code),
        }
    }

    async fn pull_exists(&self, slug: &RepoSlug, number: u64) -> Result<bool, ErrorCode> {
        if number == 0 {
            return Ok(false);
        }
        match self.read_pull_record(slug, number, false).await {
            Ok(_) => Ok(true),
            Err(ErrorCode::NotFound) => Ok(false),
            Err(code) => Err(code),
        }
    }

    async fn resolve_child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match name {
                "ctl" => Ok(Node::Ctl),
                "status" => Ok(Node::Status),
                "events" => Ok(Node::Events),
                "repos" => Ok(Node::ReposDir),
                _ => Err(ErrorCode::NotFound),
            },
            Node::ReposDir if self.known_repos().await?.iter().any(|s| s.owner == name) => {
                Ok(Node::RepoOwnerDir(name.to_string()))
            }
            Node::RepoOwnerDir(owner) => {
                let slug = RepoSlug::new(owner, name).map_err(|_| ErrorCode::NotFound)?;
                self.repo_exists(&slug)
                    .await?
                    .then_some(Node::RepoDir(slug))
                    .ok_or(ErrorCode::NotFound)
            }
            Node::RepoDir(slug) => match name {
                "issues" => Ok(Node::IssuesDir(slug.clone())),
                "pulls" => Ok(Node::PullsDir(slug.clone())),
                _ => Err(ErrorCode::NotFound),
            },
            Node::IssuesDir(slug) if name == "new" => Ok(Node::IssuesNew(slug.clone())),
            Node::IssuesDir(slug) => {
                let number = parse_positive_u64(name).ok_or(ErrorCode::NotFound)?;
                self.issue_exists(slug, number)
                    .await?
                    .then(|| Node::IssueDir(slug.clone(), number))
                    .ok_or(ErrorCode::NotFound)
            }
            Node::IssueDir(slug, n) => match name {
                "meta.json" => Ok(Node::IssueMeta(slug.clone(), *n)),
                "body.md" => Ok(Node::IssueBody(slug.clone(), *n)),
                "comments" => Ok(Node::IssueComments(slug.clone(), *n)),
                "ctl" => Ok(Node::IssueCtl(slug.clone(), *n)),
                _ => Err(ErrorCode::NotFound),
            },
            Node::PullsDir(slug) => {
                let number = parse_positive_u64(name).ok_or(ErrorCode::NotFound)?;
                self.pull_exists(slug, number)
                    .await?
                    .then(|| Node::PullDir(slug.clone(), number))
                    .ok_or(ErrorCode::NotFound)
            }
            Node::PullDir(slug, n) => match name {
                "meta.json" => Ok(Node::PullMeta(slug.clone(), *n)),
                "checks.json" => Ok(Node::PullChecks(slug.clone(), *n)),
                "reviews.json" => Ok(Node::PullReviews(slug.clone(), *n)),
                "ctl" => Ok(Node::PullCtl(slug.clone(), *n)),
                _ => Err(ErrorCode::NotFound),
            },
            Node::ReposDir => Err(ErrorCode::NotFound),
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    async fn read_issue_record(
        &self,
        slug: &RepoSlug,
        n: u64,
        require_details: bool,
    ) -> Result<IssueRecord, ErrorCode> {
        if let Some(cached) = self
            .state
            .lock()
            .await
            .repos
            .get(slug)
            .and_then(|r| r.issues.get(&n))
            .filter(|entry| !require_details || entry.hydrated)
            .cloned()
        {
            return Ok(cached.record);
        }
        let issue = self.backend.get_issue(slug, n).await.map_err(|e| e.code)?;
        self.state
            .lock()
            .await
            .repos
            .entry(slug.clone())
            .or_default()
            .issues
            .insert(
                n,
                CachedIssue {
                    record: issue.clone(),
                    hydrated: true,
                },
            );
        Ok(issue)
    }

    async fn read_pull_record(
        &self,
        slug: &RepoSlug,
        n: u64,
        require_details: bool,
    ) -> Result<PullRecord, ErrorCode> {
        if let Some(cached) = self
            .state
            .lock()
            .await
            .repos
            .get(slug)
            .and_then(|r| r.pulls.get(&n))
            .filter(|entry| !require_details || entry.hydrated)
            .cloned()
        {
            return Ok(cached.record);
        }
        let pull = self.backend.get_pull(slug, n).await.map_err(|e| e.code)?;
        self.state
            .lock()
            .await
            .repos
            .entry(slug.clone())
            .or_default()
            .pulls
            .insert(
                n,
                CachedPull {
                    record: pull.clone(),
                    hydrated: true,
                },
            );
        Ok(pull)
    }

    async fn read_pull_checks(
        &self,
        slug: &RepoSlug,
        n: u64,
    ) -> Result<GithubPullChecks, ErrorCode> {
        let cached_head_sha = self
            .state
            .lock()
            .await
            .repos
            .get(slug)
            .and_then(|r| r.pulls.get(&n))
            .map(|entry| entry.record.meta.head_sha.clone());
        let Some(head_sha) = cached_head_sha else {
            // Cache miss: full get_pull fetches meta + fresh checks in one pass.
            return Ok(self.read_pull_record(slug, n, true).await?.checks);
        };
        let checks = self
            .backend
            .get_pull_checks(slug, n, &head_sha)
            .await
            .map_err(|e| e.code)?;
        if let Some(entry) = self
            .state
            .lock()
            .await
            .repos
            .get_mut(slug)
            .and_then(|r| r.pulls.get_mut(&n))
            && entry.record.meta.head_sha == checks.head_sha
        {
            entry.record.checks = checks.clone();
        }
        Ok(checks)
    }

    async fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        if let Some(static_bytes) = node.static_bytes() {
            return Ok(static_bytes.to_vec());
        }
        match node {
            Node::Status => encode_pretty_json(&self.state.lock().await.status_snapshot()),
            Node::Events => Err(ErrorCode::Unsupported),
            Node::ReposDir => Ok(join_lines(
                self.known_repos()
                    .await?
                    .into_iter()
                    .map(|r| r.owner)
                    .collect::<BTreeSet<_>>(),
            )),
            Node::RepoOwnerDir(owner) => Ok(join_lines(
                self.known_repos()
                    .await?
                    .into_iter()
                    .filter(|r| &r.owner == owner)
                    .map(|r| r.repo),
            )),
            Node::IssuesDir(slug) => {
                let mut nums: BTreeSet<u64> = self
                    .state
                    .lock()
                    .await
                    .repos
                    .get(slug)
                    .map(|r| r.issues.keys().copied().collect())
                    .unwrap_or_default();
                if let Ok(more) = self.backend.list_issue_numbers(slug).await {
                    nums.extend(more);
                }
                Ok(join_lines(
                    std::iter::once("new".to_string())
                        .chain(nums.into_iter().map(|n| n.to_string())),
                ))
            }
            Node::IssuesNew(slug) => {
                let created = self
                    .state
                    .lock()
                    .await
                    .repos
                    .get(slug)
                    .and_then(|r| r.last_created_issue.clone());
                created.map_or_else(|| Ok(Vec::new()), |c| encode_pretty_json(&c))
            }
            Node::IssueMeta(slug, n) => {
                encode_pretty_json(&self.read_issue_record(slug, *n, false).await?.meta)
            }
            Node::IssueBody(slug, n) => Ok(self
                .read_issue_record(slug, *n, false)
                .await?
                .body
                .into_bytes()),
            Node::IssueComments(slug, n) => {
                encode_pretty_json(&self.read_issue_record(slug, *n, true).await?.comments)
            }
            Node::PullsDir(slug) => {
                let mut nums: BTreeSet<u64> = self
                    .state
                    .lock()
                    .await
                    .repos
                    .get(slug)
                    .map(|r| r.pulls.keys().copied().collect())
                    .unwrap_or_default();
                if let Ok(more) = self.backend.list_pull_numbers(slug).await {
                    nums.extend(more);
                }
                Ok(join_lines(nums.into_iter().map(|n| n.to_string())))
            }
            Node::PullMeta(slug, n) => {
                encode_pretty_json(&self.read_pull_record(slug, *n, false).await?.meta)
            }
            Node::PullChecks(slug, n) => {
                encode_pretty_json(&self.read_pull_checks(slug, *n).await?)
            }
            Node::PullReviews(slug, n) => {
                encode_pretty_json(&self.read_pull_record(slug, *n, true).await?.reviews)
            }
            _ => unreachable!("handled by static_bytes"),
        }
    }

    async fn commit_write(&self, node: Node, bytes: Vec<u8>) -> Result<(), ErrorCode> {
        match node {
            Node::Ctl => self.commit_root_ctl(&bytes).await,
            Node::IssuesNew(slug) => self.commit_new_issue(&slug, &bytes).await,
            Node::IssueComments(slug, number) => {
                self.commit_issue_comment(&slug, number, &bytes).await
            }
            Node::IssueCtl(slug, number) => self.commit_issue_ctl(&slug, number, &bytes).await,
            Node::PullCtl(slug, number) => self.commit_pull_ctl(&slug, number, &bytes).await,
            _ => Err(ErrorCode::Unsupported),
        }
    }

    async fn commit_root_ctl(&self, bytes: &[u8]) -> Result<(), ErrorCode> {
        let cmd = parse_single_line_ctl(bytes)?;
        let mut parts = cmd.split_whitespace();
        match (parts.next(), parts.next(), parts.next()) {
            (Some("sync"), None, None) => self.sync_watched_repos().await,
            (Some("watch"), Some(raw_slug), None) => {
                let slug = RepoSlug::parse(raw_slug)?;
                let mut state = self.state.lock().await;
                state.watched_repos.insert(slug.clone());
                state.repos.entry(slug).or_default();
                state.bump(&Node::ReposDir);
                state.bump(&Node::Status);
                Ok(())
            }
            (Some("unwatch"), Some(raw_slug), None) => {
                let slug = RepoSlug::parse(raw_slug)?;
                let mut state = self.state.lock().await;
                state.watched_repos.remove(&slug);
                state.repos.remove(&slug);
                state.bump(&Node::ReposDir);
                state.bump(&Node::Status);
                Ok(())
            }
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn commit_new_issue(&self, slug: &RepoSlug, bytes: &[u8]) -> Result<(), ErrorCode> {
        let req: NewIssueRequest =
            serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
        req.validate()?;
        let record = self
            .backend
            .create_issue(slug, req)
            .await
            .map_err(|err| err.code)?;
        let created = CreatedIssue {
            number: record.meta.number,
            url: record.meta.url.clone(),
        };
        let mut state = self.state.lock().await;
        let cache = state.repos.entry(slug.clone()).or_default();
        cache.issues.insert(
            record.meta.number,
            CachedIssue {
                record,
                hydrated: true,
            },
        );
        cache.last_created_issue = Some(created);
        state.bump(&Node::IssuesDir(slug.clone()));
        state.bump(&Node::IssuesNew(slug.clone()));
        Ok(())
    }

    async fn commit_issue_comment(
        &self,
        slug: &RepoSlug,
        number: u64,
        bytes: &[u8],
    ) -> Result<(), ErrorCode> {
        let text = std::str::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        if text.contains('\0') {
            return Err(ErrorCode::BadRequest);
        }
        let body = text.trim_end_matches(['\r', '\n']).to_string();
        if body.trim().is_empty() {
            return Err(ErrorCode::BadRequest);
        }
        let comment = self
            .backend
            .add_issue_comment(slug, number, body)
            .await
            .map_err(|err| err.code)?;
        let mut state = self.state.lock().await;
        if let Some(entry) = state
            .repos
            .get_mut(slug)
            .and_then(|repo| repo.issues.get_mut(&number))
        {
            if entry.hydrated {
                entry.record.comments.push(comment.clone());
            }
            entry.record.meta.updated_at = comment.created_at;
        }
        state.bump(&Node::IssueComments(slug.clone(), number));
        state.bump(&Node::IssueMeta(slug.clone(), number));
        Ok(())
    }

    async fn commit_issue_ctl(
        &self,
        slug: &RepoSlug,
        number: u64,
        bytes: &[u8],
    ) -> Result<(), ErrorCode> {
        let cmd = parse_single_line_ctl(bytes)?;
        let updated_meta = if cmd == "close" {
            self.backend
                .close_issue(slug, number)
                .await
                .map_err(|err| err.code)?
        } else if let Some(raw_label) = cmd.strip_prefix("label add ") {
            let label = backend::validate_label(raw_label)?;
            self.backend
                .add_issue_label(slug, number, label)
                .await
                .map_err(|err| err.code)?
        } else if let Some(raw_label) = cmd.strip_prefix("label remove ") {
            let label = backend::validate_label(raw_label)?;
            self.backend
                .remove_issue_label(slug, number, label)
                .await
                .map_err(|err| err.code)?
        } else {
            return Err(ErrorCode::BadRequest);
        };
        let mut state = self.state.lock().await;
        if let Some(entry) = state
            .repos
            .get_mut(slug)
            .and_then(|repo| repo.issues.get_mut(&number))
        {
            entry.record.meta = updated_meta;
        }
        state.bump(&Node::IssueMeta(slug.clone(), number));
        Ok(())
    }

    async fn commit_pull_ctl(
        &self,
        slug: &RepoSlug,
        number: u64,
        bytes: &[u8],
    ) -> Result<(), ErrorCode> {
        let raw = std::str::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        if raw.contains('\0') {
            return Err(ErrorCode::BadRequest);
        }
        let trimmed = raw.trim_start().trim_end_matches(['\r', '\n']);
        let first_word = trimmed.split_whitespace().next().unwrap_or("");
        // Fail closed on all destructive or non-comment verbs (merge, release, tag, publish, etc.).
        if first_word != "comment" {
            return Err(ErrorCode::BadRequest);
        }
        let body = trimmed
            .strip_prefix("comment")
            .filter(|rest| rest.starts_with(' ') || rest.starts_with('\t'))
            .map(str::trim)
            .filter(|rest| !rest.is_empty())
            .ok_or(ErrorCode::BadRequest)?
            .to_string();
        let comment = self
            .backend
            .add_pull_comment(slug, number, body)
            .await
            .map_err(|err| err.code)?;
        let mut state = self.state.lock().await;
        if let Some(entry) = state
            .repos
            .get_mut(slug)
            .and_then(|repo| repo.pulls.get_mut(&number))
        {
            if entry.hydrated {
                entry.record.comments.push(comment.clone());
            }
            entry.record.meta.updated_at = comment.created_at;
        }
        state.bump(&Node::PullMeta(slug.clone(), number));
        Ok(())
    }
}

#[async_trait]
impl<B: GithubBackend + 'static> FileServer for GithubFs<B> {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut node = {
            let state = self.state.lock().await;
            if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            state.node_of(fid)?
        };
        for name in names {
            node = self.resolve_child(&node, name).await?;
        }
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let qid = state.qid(&node);
        state.fids.insert(newfid, GithubFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if fid == Fid::ROOT {
            if !matches!(mode, OpenMode::Read) {
                return Err(ErrorCode::NoAccess);
            }
            return Ok(state.qid(&Node::Root));
        }
        if state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        let node = state.node_of(fid)?;
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) && !node.is_writable() {
            return Err(ErrorCode::NoAccess);
        }
        let qid = state.qid(&node);
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        f.mode = Some(mode);
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (node, stream) = {
            let state = self.state.lock().await;
            if fid != Fid::ROOT {
                let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
                if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
            }
            let node = state.node_of(fid)?;
            let stream = matches!(node, Node::Events).then(|| state.events.clone());
            (node, stream)
        };
        if let Some(stream) = stream {
            return Ok(stream.read(offset, count).await);
        }
        if fid == Fid::ROOT {
            let bytes = self.computed_bytes(&node).await?;
            return Ok(slice_bytes(&bytes, offset, count));
        }
        let need_refresh = {
            let state = self.state.lock().await;
            let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            offset == 0 || f.read_snapshot.is_none()
        };
        if need_refresh {
            let bytes = self.computed_bytes(&node).await?;
            let mut state = self.state.lock().await;
            if let Some(f) = state.fids.get_mut(&fid) {
                f.read_snapshot = Some(bytes);
            } else {
                return Err(ErrorCode::NotFound);
            }
        }
        let state = self.state.lock().await;
        let bytes = state
            .fids
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?
            .read_snapshot
            .as_ref()
            .ok_or(ErrorCode::Io)?;
        Ok(slice_bytes(bytes, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        if !node.is_writable() {
            return Err(ErrorCode::Unsupported);
        }
        f.buffer_write(offset, data, node.max_write_bytes())
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let (node, qid, stream) = {
            let state = self.state.lock().await;
            let node = state.node_of(fid)?;
            let qid = state.qid(&node);
            let stream = matches!(node, Node::Events).then(|| state.events.clone());
            (node, qid, stream)
        };
        let length = match stream {
            Some(stream) => stream.len().await,
            None => self.computed_bytes(&node).await?.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid,
            length,
            executable: false,
            writable: node.is_writable(),
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let f = self
            .state
            .lock()
            .await
            .fids
            .remove(&fid)
            .ok_or(ErrorCode::NotFound)?;
        if f.wrote
            && !f.write_failed
            && matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite))
        {
            self.commit_write(f.node, f.write_buf).await?;
        }
        Ok(())
    }
}

/// Export `server` over arbitrary async reader/writer streams using aP wire framing.
pub async fn serve_io<B, R, W>(
    server: Arc<GithubFs<B>>,
    reader: R,
    writer: W,
) -> Result<(), WireError>
where
    B: GithubBackend + 'static,
    R: AsyncBufRead + Send + Unpin + 'static,
    W: AsyncWrite + Unpin,
{
    let file_server: Arc<dyn FileServer> = server;
    export_file_server(file_server, reader, writer).await
}

/// Export `server` over process stdio using framed aP wire transport.
pub async fn serve_stdio<B: GithubBackend + 'static>(
    server: Arc<GithubFs<B>>,
) -> Result<(), WireError> {
    let stdin = BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    serve_io(server, stdin, stdout).await
}
