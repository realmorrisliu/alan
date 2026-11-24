//! Internal fid, state cache, node identity, and pure formatting helpers for `GithubFs`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use alan_ap::{ErrorCode, Fid, FileKind, Offset, OpenMode, Qid, Stream, VersionTable};
use chrono::Utc;

use super::backend::{
    CreatedIssue, GithubStatusSnapshot, IssueRecord, MAX_CTL_BYTES, MAX_DOC_BYTES, PullRecord,
    RateLimitStatus, RateLimitUpdate, RepoSlug, SyncRepoData, redact_github_tokens,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Node {
    Root,
    Ctl,
    Status,
    Events,
    ReposDir,
    RepoOwnerDir(String),
    RepoDir(RepoSlug),
    IssuesDir(RepoSlug),
    IssuesNew(RepoSlug),
    IssueDir(RepoSlug, u64),
    IssueMeta(RepoSlug, u64),
    IssueBody(RepoSlug, u64),
    IssueComments(RepoSlug, u64),
    IssueCtl(RepoSlug, u64),
    PullsDir(RepoSlug),
    PullDir(RepoSlug, u64),
    PullMeta(RepoSlug, u64),
    PullChecks(RepoSlug, u64),
    PullReviews(RepoSlug, u64),
    PullCtl(RepoSlug, u64),
}

impl Node {
    pub(super) fn static_bytes(&self) -> Option<&'static [u8]> {
        match self {
            Node::Root => Some(b"ctl\nstatus\nevents\nrepos"),
            Node::Ctl => Some(b"watch <owner/repo>\nunwatch <owner/repo>\nsync\n"),
            Node::RepoDir(_) => Some(b"issues\npulls"),
            Node::IssueDir(_, _) => Some(b"meta.json\nbody.md\ncomments\nctl"),
            Node::IssueCtl(_, _) => Some(b"label add <name>\nlabel remove <name>\nclose\n"),
            Node::PullDir(_, _) => Some(b"meta.json\nchecks.json\nreviews.json\nctl"),
            Node::PullCtl(_, _) => Some(b"comment <body>\n"),
            _ => None,
        }
    }

    pub(super) fn is_writable(&self) -> bool {
        matches!(
            self,
            Node::Ctl
                | Node::IssuesNew(_)
                | Node::IssueComments(_, _)
                | Node::IssueCtl(_, _)
                | Node::PullCtl(_, _)
        )
    }

    pub(super) fn max_write_bytes(&self) -> usize {
        match self {
            Node::Ctl | Node::IssueCtl(_, _) | Node::PullCtl(_, _) => MAX_CTL_BYTES,
            _ => MAX_DOC_BYTES,
        }
    }
}

pub(super) struct GithubFid {
    pub(super) node: Node,
    pub(super) mode: Option<OpenMode>,
    pub(super) write_buf: Vec<u8>,
    pub(super) wrote: bool,
    pub(super) write_failed: bool,
    pub(super) read_snapshot: Option<Vec<u8>>,
}

impl GithubFid {
    pub(super) fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            write_buf: Vec::new(),
            wrote: false,
            write_failed: false,
            read_snapshot: None,
        }
    }

    pub(super) fn buffer_write(
        &mut self,
        offset: Offset,
        data: &[u8],
        max_bytes: usize,
    ) -> Result<u32, ErrorCode> {
        if self.write_failed {
            return Err(ErrorCode::BadRequest);
        }
        let Ok(start) = usize::try_from(offset) else {
            self.write_failed = true;
            return Err(ErrorCode::BadRequest);
        };
        let Some(end) = start.checked_add(data.len()).filter(|e| *e <= max_bytes) else {
            self.write_failed = true;
            return Err(ErrorCode::BadRequest);
        };
        if self.write_buf.len() < end {
            self.write_buf.resize(end, 0);
        }
        self.write_buf[start..end].copy_from_slice(data);
        self.wrote = true;
        Ok(data.len() as u32)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CachedIssue {
    pub(super) record: IssueRecord,
    pub(super) hydrated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CachedPull {
    pub(super) record: PullRecord,
    pub(super) hydrated: bool,
}

#[derive(Default)]
pub(super) struct RepoCache {
    pub(super) synced: bool,
    pub(super) etag: Option<String>,
    pub(super) issues: BTreeMap<u64, CachedIssue>,
    pub(super) pulls: BTreeMap<u64, CachedPull>,
    pub(super) last_created_issue: Option<CreatedIssue>,
}

pub(super) struct FsState {
    pub(super) watched_repos: BTreeSet<RepoSlug>,
    pub(super) repos: BTreeMap<RepoSlug, RepoCache>,
    pub(super) next_cursor: u64,
    pub(super) last_sync_at: Option<String>,
    pub(super) rate_limit: RateLimitStatus,
    pub(super) last_error: Option<String>,
    pub(super) events: Stream,
    versions: VersionTable,
    pub(super) fids: HashMap<Fid, GithubFid>,
}

impl FsState {
    pub(super) fn new() -> Self {
        Self {
            watched_repos: BTreeSet::new(),
            repos: BTreeMap::new(),
            next_cursor: 0,
            last_sync_at: None,
            rate_limit: RateLimitStatus::default(),
            last_error: None,
            events: Stream::new(),
            versions: VersionTable::new(),
            fids: HashMap::new(),
        }
    }

    pub(super) fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .get(&fid)
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    pub(super) fn qid(&self, node: &Node) -> Qid {
        let (kind, path) = node_identity(node);
        Qid {
            kind,
            version: self.versions.get(path),
            path,
        }
    }

    pub(super) fn bump(&mut self, node: &Node) {
        self.versions.bump(node_identity(node).1);
    }

    pub(super) fn status_snapshot(&self) -> GithubStatusSnapshot {
        let etags = self
            .repos
            .iter()
            .filter_map(|(slug, cache)| {
                cache
                    .etag
                    .as_ref()
                    .map(|etag| (slug.as_slug(), etag.clone()))
            })
            .collect();
        GithubStatusSnapshot {
            watched_repos: self.watched_repos.iter().map(RepoSlug::as_slug).collect(),
            cursor: self.next_cursor,
            last_sync_at: self.last_sync_at.clone(),
            etags,
            rate_limit: self.rate_limit.clone(),
            last_error: self.last_error.clone(),
        }
    }

    fn apply_rate_limit(&mut self, update: Option<RateLimitUpdate>) {
        if let Some(update) = update {
            self.rate_limit.limit = update.limit.or(self.rate_limit.limit);
            self.rate_limit.remaining = update.remaining.or(self.rate_limit.remaining);
            self.rate_limit.reset_at = update.reset_at.or_else(|| self.rate_limit.reset_at.clone());
        }
    }

    pub(super) fn record_not_modified(&mut self, rate_limit: Option<RateLimitUpdate>) {
        self.rate_limit.api_requests = self.rate_limit.api_requests.saturating_add(1);
        self.rate_limit.conditional_hits_304 =
            self.rate_limit.conditional_hits_304.saturating_add(1);
        self.apply_rate_limit(rate_limit);
        self.last_sync_at = Some(Utc::now().to_rfc3339());
        self.last_error = None;
        self.bump(&Node::Status);
    }

    pub(super) fn record_sync_update(
        &mut self,
        slug: &RepoSlug,
        data: SyncRepoData,
    ) -> (Vec<u8>, Stream) {
        self.rate_limit.api_requests = self.rate_limit.api_requests.saturating_add(1);
        self.apply_rate_limit(data.rate_limit);
        self.last_sync_at = Some(Utc::now().to_rfc3339());
        self.last_error = None;

        let cache = self.repos.entry(slug.clone()).or_default();
        cache.synced = true;
        if let Some(new_etag) = data.etag {
            cache.etag = Some(new_etag);
        }
        for issue in data.issues {
            let num = issue.meta.number;
            let entry = if data.details_hydrated {
                CachedIssue {
                    record: issue,
                    hydrated: true,
                }
            } else if let Some(prev) = cache.issues.get(&num)
                && prev.hydrated
                && prev.record.meta.updated_at == issue.meta.updated_at
            {
                CachedIssue {
                    record: IssueRecord {
                        meta: issue.meta,
                        body: issue.body,
                        comments: prev.record.comments.clone(),
                    },
                    hydrated: true,
                }
            } else {
                CachedIssue {
                    record: issue,
                    hydrated: false,
                }
            };
            cache.issues.insert(num, entry);
        }
        for pull in data.pulls {
            let num = pull.meta.number;
            let entry = if data.details_hydrated {
                CachedPull {
                    record: pull,
                    hydrated: true,
                }
            } else if let Some(prev) = cache.pulls.get(&num)
                && prev.hydrated
                && prev.record.meta.head_sha == pull.meta.head_sha
                && prev.record.meta.updated_at == pull.meta.updated_at
            {
                CachedPull {
                    record: PullRecord {
                        meta: pull.meta,
                        checks: prev.record.checks.clone(),
                        reviews: prev.record.reviews.clone(),
                        comments: prev.record.comments.clone(),
                    },
                    hydrated: true,
                }
            } else {
                CachedPull {
                    record: pull,
                    hydrated: false,
                }
            };
            cache.pulls.insert(num, entry);
        }

        let mut lines = Vec::new();
        for mut event in data.events {
            self.next_cursor = self.next_cursor.saturating_add(1);
            event.cursor = self.next_cursor;
            if event.repo.is_empty() {
                event.repo = slug.as_slug();
            }
            if let Ok(mut encoded) = serde_json::to_vec(&event) {
                encoded.push(b'\n');
                lines.extend_from_slice(&encoded);
            }
        }
        self.bump(&Node::IssuesDir(slug.clone()));
        self.bump(&Node::PullsDir(slug.clone()));
        self.bump(&Node::Status);
        (lines, self.events.clone())
    }

    pub(super) fn record_sync_error(&mut self, message: &str) {
        self.rate_limit.api_requests = self.rate_limit.api_requests.saturating_add(1);
        self.last_error = Some(redact_github_tokens(message, None));
        self.bump(&Node::Status);
    }
}

pub(super) fn parse_positive_u64(raw: &str) -> Option<u64> {
    if raw.is_empty() || raw.starts_with('0') || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<u64>().ok().filter(|n| *n > 0)
}

pub(super) fn parse_single_line_ctl(bytes: &[u8]) -> Result<String, ErrorCode> {
    let text = std::str::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
    if text.contains('\0') {
        return Err(ErrorCode::BadRequest);
    }
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(ErrorCode::BadRequest);
    }
    Ok(trimmed.to_string())
}

pub(super) fn join_lines(lines: impl IntoIterator<Item = String>) -> Vec<u8> {
    lines
        .into_iter()
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

pub(super) fn encode_pretty_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, ErrorCode> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| ErrorCode::Io)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn slice_bytes(bytes: &[u8], offset: Offset, count: u32) -> Vec<u8> {
    let start = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let end = start.saturating_add(count as usize).min(bytes.len());
    bytes[start..end].to_vec()
}

fn node_identity(node: &Node) -> (FileKind, u64) {
    let kind = match node {
        Node::Root
        | Node::ReposDir
        | Node::RepoOwnerDir(_)
        | Node::RepoDir(_)
        | Node::IssuesDir(_)
        | Node::IssueDir(_, _)
        | Node::PullsDir(_)
        | Node::PullDir(_, _) => FileKind::Dir,
        Node::Events => FileKind::Stream,
        Node::Ctl
        | Node::Status
        | Node::IssuesNew(_)
        | Node::IssueMeta(_, _)
        | Node::IssueBody(_, _)
        | Node::IssueComments(_, _)
        | Node::IssueCtl(_, _)
        | Node::PullMeta(_, _)
        | Node::PullChecks(_, _)
        | Node::PullReviews(_, _)
        | Node::PullCtl(_, _) => FileKind::File,
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    node.hash(&mut hasher);
    (kind, hasher.finish())
}
