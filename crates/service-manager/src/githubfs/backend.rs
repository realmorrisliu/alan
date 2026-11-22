//! GitHub backend trait, DTOs, and deterministic in-memory test backend.
//!
//! Credentials remain strictly behind [`GhCliGithubBackend`] (resolved via the Host `gh` CLI
//! adapter) and are never serialized into namespace files, events, status, or debug output.
//! Destructive merge, tag, release, and publish operations are intentionally absent from
//! [`GithubBackend`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use alan_ap::ErrorCode;
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub use super::gh_cli::GhCliGithubBackend;

/// Maximum commit-on-clunk document size accepted by `githubfs` (1 MiB).
pub const MAX_DOC_BYTES: usize = 1 << 20;
/// Maximum commit-on-clunk control command size accepted by `ctl` files (64 KiB).
pub const MAX_CTL_BYTES: usize = 64 * 1024;
const MAX_TITLE_BYTES: usize = 512;
const MAX_LABEL_BYTES: usize = 128;

/// Validated `<owner>/<repo>` GitHub repository slug.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoSlug {
    /// Repository owner/organization name.
    pub owner: String,
    /// Repository name.
    pub repo: String,
}

impl RepoSlug {
    /// Construct and validate a repository slug from separate `owner` and `repo` components.
    pub fn new(owner: &str, repo: &str) -> Result<Self, ErrorCode> {
        if !valid_slug_part(owner) || !valid_slug_part(repo) {
            return Err(ErrorCode::BadRequest);
        }
        Ok(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    /// Parse and validate `<owner>/<repo>` text.
    pub fn parse(raw: &str) -> Result<Self, ErrorCode> {
        let trimmed = raw.trim();
        let Some((owner, repo)) = trimmed.split_once('/') else {
            return Err(ErrorCode::BadRequest);
        };
        if repo.contains('/') {
            return Err(ErrorCode::BadRequest);
        }
        Self::new(owner, repo)
    }

    /// Return canonical `<owner>/<repo>` string.
    pub fn as_slug(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

impl fmt::Display for RepoSlug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.repo)
    }
}

fn valid_slug_part(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 100
        && part != "."
        && part != ".."
        && part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Validate a GitHub label string.
pub(crate) fn validate_label(raw: &str) -> Result<String, ErrorCode> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_LABEL_BYTES
        || trimmed.bytes().any(|b| matches!(b, b'\n' | b'\r' | b'\0'))
    {
        return Err(ErrorCode::BadRequest);
    }
    Ok(trimmed.to_string())
}

/// Metadata projected at `repos/<owner>/<repo>/issues/<n>/meta.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubIssueMeta {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<String>,
    pub updated_at: String,
    pub url: String,
}

/// Issue or pull request comment entry projected in `comments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubComment {
    pub id: u64,
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// Complete cached/fetched issue record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueRecord {
    pub meta: GithubIssueMeta,
    pub body: String,
    #[serde(default)]
    pub comments: Vec<GithubComment>,
}

/// Metadata projected at `repos/<owner>/<repo>/pulls/<n>/meta.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubPullMeta {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_ref: String,
    pub head_sha: String,
    pub base_ref: String,
    pub state: String,
    #[serde(default)]
    pub mergeable: Option<bool>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub updated_at: String,
    pub url: String,
}

/// Single CI check run summary projected inside `pulls/<n>/checks.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubCheckRun {
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details_url: Option<String>,
}

/// Check runs projection at `pulls/<n>/checks.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubPullChecks {
    pub head_sha: String,
    #[serde(default)]
    pub checks: Vec<GithubCheckRun>,
}

/// Pull request review summary inside `pulls/<n>/reviews.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubReview {
    pub id: u64,
    pub author: String,
    pub state: String,
    pub body: String,
    pub submitted_at: String,
}

/// Unresolved review comment thread inside `pulls/<n>/reviews.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubReviewThread {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    pub resolved: bool,
    pub author: String,
    pub body: String,
}

/// Reviews projection at `pulls/<n>/reviews.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubPullReviews {
    #[serde(default)]
    pub reviews: Vec<GithubReview>,
    #[serde(default)]
    pub unresolved_threads: Vec<GithubReviewThread>,
}

/// Complete cached/fetched pull request record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRecord {
    pub meta: GithubPullMeta,
    pub checks: GithubPullChecks,
    pub reviews: GithubPullReviews,
    #[serde(default)]
    pub comments: Vec<GithubComment>,
}

/// Structured NDJSON event record emitted on `/mnt/github/events`.
///
/// Contains strictly structured metadata (never issue `body.md` or comment prose).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubEvent {
    pub cursor: u64,
    pub event_id: String,
    pub kind: String,
    pub actor: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// JSON request payload written to `repos/<owner>/<repo>/issues/new`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewIssueRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub labels: Vec<String>,
}

impl NewIssueRequest {
    /// Validate payload invariants before committing issue creation.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        let title = self.title.trim();
        if title.is_empty()
            || title.len() > MAX_TITLE_BYTES
            || self.title.contains('\0')
            || self.body.len() > MAX_DOC_BYTES
            || self.body.contains('\0')
        {
            return Err(ErrorCode::BadRequest);
        }
        for label in &self.labels {
            validate_label(label)?;
        }
        Ok(())
    }
}

/// JSON summary returned after `issues/new` commits on clunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedIssue {
    pub number: u64,
    pub url: String,
}

/// Rate limit metadata observed from GitHub API responses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<String>,
}

/// Rate-limit and conditional request counters exposed in `/status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<String>,
    pub conditional_hits_304: u64,
    pub api_requests: u64,
}

/// Read-only status document projected at `/status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubStatusSnapshot {
    pub watched_repos: Vec<String>,
    pub cursor: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<String>,
    pub etags: BTreeMap<String, String>,
    pub rate_limit: RateLimitStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Repository snapshot & events returned by `sync_repo` when ETag changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncRepoData {
    pub etag: Option<String>,
    pub rate_limit: Option<RateLimitUpdate>,
    pub issues: Vec<IssueRecord>,
    pub pulls: Vec<PullRecord>,
    pub events: Vec<GithubEvent>,
    /// `true` when `issues`/`pulls` include full comments/checks/reviews; `false` for list-only syncs.
    pub details_hydrated: bool,
}

/// Result of conditional `If-None-Match` repository sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncRepoOutcome {
    NotModified { rate_limit: Option<RateLimitUpdate> },
    Updated(SyncRepoData),
}

/// Backend failure carrying an aP [`ErrorCode`] and token-redacted diagnostic text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubBackendError {
    pub code: ErrorCode,
    pub message: String,
}

impl GithubBackendError {
    /// Construct an error after scrubbing any embedded GitHub credential tokens.
    pub fn new(code: ErrorCode, message: impl AsRef<str>) -> Self {
        Self {
            code,
            message: redact_github_tokens(message.as_ref(), None),
        }
    }

    pub(crate) fn redacted(
        code: ErrorCode,
        message: impl AsRef<str>,
        secret: Option<&str>,
    ) -> Self {
        Self {
            code,
            message: redact_github_tokens(message.as_ref(), secret),
        }
    }
}

impl fmt::Display for GithubBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for GithubBackendError {}

/// Scrubs GitHub tokens (`ghp_`, `github_pat_`, `Bearer ...`, explicit secrets) from text.
pub fn redact_github_tokens(input: &str, extra_secret: Option<&str>) -> String {
    let mut scrubbed = if let Some(secret) = extra_secret.filter(|s| !s.is_empty()) {
        input.replace(secret, "[REDACTED]")
    } else {
        input.to_string()
    };

    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"] {
        while let Some(idx) = scrubbed.find(prefix) {
            let tail = &scrubbed[idx + prefix.len()..];
            let len = tail
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
            scrubbed.replace_range(idx..idx + prefix.len() + len, "[REDACTED]");
        }
    }

    for marker in ["Bearer ", "bearer ", "token "] {
        let mut search_from = 0usize;
        while let Some(rel) = scrubbed[search_from..].find(marker) {
            let token_start = search_from + rel + marker.len();
            let token_len = scrubbed[token_start..]
                .bytes()
                .take_while(|b| {
                    !b.is_ascii_whitespace() && !matches!(b, b'"' | b'\'' | b',' | b';')
                })
                .count();
            if token_len > 0 && &scrubbed[token_start..token_start + token_len] != "[REDACTED]" {
                scrubbed.replace_range(token_start..token_start + token_len, "[REDACTED]");
                search_from = token_start + "[REDACTED]".len();
            } else {
                search_from = token_start;
            }
        }
    }
    scrubbed
}

/// Abstract GitHub repository/issue/pull backend.
///
/// Intentionally omits merge, release, tag, and publish operations so destructive
/// repository transitions remain structurally unreachable through `githubfs`.
#[async_trait]
pub trait GithubBackend: Send + Sync {
    async fn list_repos(&self) -> Result<Vec<RepoSlug>, GithubBackendError>;
    async fn has_repo(&self, repo: &RepoSlug) -> Result<bool, GithubBackendError>;
    async fn sync_repo(
        &self,
        repo: &RepoSlug,
        if_none_match: Option<&str>,
    ) -> Result<SyncRepoOutcome, GithubBackendError>;
    async fn list_issue_numbers(&self, repo: &RepoSlug) -> Result<Vec<u64>, GithubBackendError>;
    async fn get_issue(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<IssueRecord, GithubBackendError>;
    async fn create_issue(
        &self,
        repo: &RepoSlug,
        req: NewIssueRequest,
    ) -> Result<IssueRecord, GithubBackendError>;
    async fn add_issue_comment(
        &self,
        repo: &RepoSlug,
        number: u64,
        body: String,
    ) -> Result<GithubComment, GithubBackendError>;
    async fn add_issue_label(
        &self,
        repo: &RepoSlug,
        number: u64,
        label: String,
    ) -> Result<GithubIssueMeta, GithubBackendError>;
    async fn remove_issue_label(
        &self,
        repo: &RepoSlug,
        number: u64,
        label: String,
    ) -> Result<GithubIssueMeta, GithubBackendError>;
    async fn close_issue(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<GithubIssueMeta, GithubBackendError>;
    async fn list_pull_numbers(&self, repo: &RepoSlug) -> Result<Vec<u64>, GithubBackendError>;
    async fn get_pull(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<PullRecord, GithubBackendError>;
    async fn add_pull_comment(
        &self,
        repo: &RepoSlug,
        number: u64,
        body: String,
    ) -> Result<GithubComment, GithubBackendError>;
}

#[derive(Default)]
struct MemoryRepoState {
    next_number: u64,
    next_comment_id: u64,
    etag: Option<String>,
    issues: BTreeMap<u64, IssueRecord>,
    pulls: BTreeMap<u64, PullRecord>,
    pending_events: Vec<GithubEvent>,
    last_if_none_match: Option<String>,
}

struct MemoryState {
    actor: String,
    repos: BTreeMap<RepoSlug, MemoryRepoState>,
    rate_limit: Option<RateLimitUpdate>,
}

/// Deterministic in-memory [`GithubBackend`] for unit and integration tests.
#[derive(Clone)]
pub struct MemoryGithubBackend {
    state: Arc<Mutex<MemoryState>>,
}

impl Default for MemoryGithubBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryGithubBackend {
    /// Create an empty in-memory backend with default actor `"alan-test-user"`.
    pub fn new() -> Self {
        Self::with_actor("alan-test-user")
    }

    /// Create an empty in-memory backend attributing local mutations to `actor`.
    pub fn with_actor(actor: impl Into<String>) -> Self {
        Self {
            state: Arc::new(Mutex::new(MemoryState {
                actor: actor.into(),
                repos: BTreeMap::new(),
                rate_limit: Some(RateLimitUpdate {
                    limit: Some(5000),
                    remaining: Some(4999),
                    reset_at: Some("2099-01-01T00:00:00Z".to_string()),
                }),
            })),
        }
    }

    /// Ensure `repo` exists in the in-memory store.
    pub fn ensure_repo(&self, repo: &RepoSlug) {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        state
            .repos
            .entry(repo.clone())
            .or_insert_with(|| MemoryRepoState {
                next_number: 1,
                next_comment_id: 1,
                etag: Some("W/\"mem-0\"".to_string()),
                ..Default::default()
            });
    }

    /// Insert or update an issue fixture in `repo`.
    pub fn seed_issue(&self, repo: &RepoSlug, issue: IssueRecord) {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let entry = state
            .repos
            .entry(repo.clone())
            .or_insert_with(|| MemoryRepoState {
                next_number: 1,
                next_comment_id: 1,
                ..Default::default()
            });
        entry.next_number = entry.next_number.max(issue.meta.number.saturating_add(1));
        for comment in &issue.comments {
            entry.next_comment_id = entry.next_comment_id.max(comment.id.saturating_add(1));
        }
        entry.issues.insert(issue.meta.number, issue);
        bump_mem_etag(entry);
    }

    /// Insert or update a pull request fixture in `repo`.
    pub fn seed_pull(&self, repo: &RepoSlug, pull: PullRecord) {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let entry = state
            .repos
            .entry(repo.clone())
            .or_insert_with(|| MemoryRepoState {
                next_number: 1,
                next_comment_id: 1,
                ..Default::default()
            });
        entry.next_number = entry.next_number.max(pull.meta.number.saturating_add(1));
        for comment in &pull.comments {
            entry.next_comment_id = entry.next_comment_id.max(comment.id.saturating_add(1));
        }
        entry.pulls.insert(pull.meta.number, pull);
        bump_mem_etag(entry);
    }

    /// Queue an external event to be emitted on the next `sync` of `repo`.
    pub fn push_event(&self, repo: &RepoSlug, mut event: GithubEvent) {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let entry = state
            .repos
            .entry(repo.clone())
            .or_insert_with(|| MemoryRepoState {
                next_number: 1,
                next_comment_id: 1,
                ..Default::default()
            });
        if event.repo.is_empty() {
            event.repo = repo.as_slug();
        }
        entry.pending_events.push(event);
        bump_mem_etag(entry);
    }

    /// Override the current ETag value returned for `repo`.
    pub fn set_repo_etag(&self, repo: &RepoSlug, etag: impl Into<String>) {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        if let Some(entry) = state.repos.get_mut(repo) {
            entry.etag = Some(etag.into());
        }
    }

    /// Configure the rate-limit snapshot returned by `sync_repo`.
    pub fn set_rate_limit(&self, rate_limit: Option<RateLimitUpdate>) {
        self.state
            .lock()
            .expect("memory github lock poisoned")
            .rate_limit = rate_limit;
    }

    /// Inspect the last `If-None-Match` header value observed by `sync_repo`.
    pub fn last_if_none_match(&self, repo: &RepoSlug) -> Option<String> {
        self.state
            .lock()
            .expect("memory github lock poisoned")
            .repos
            .get(repo)
            .and_then(|entry| entry.last_if_none_match.clone())
    }
}

fn bump_mem_etag(entry: &mut MemoryRepoState) {
    let seq = entry
        .etag
        .as_deref()
        .and_then(|tag| {
            tag.trim_matches(|c| c == 'W' || c == '/' || c == '"')
                .strip_prefix("mem-")
        })
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_add(1);
    entry.etag = Some(format!("W/\"mem-{seq}\""));
}

#[async_trait]
impl GithubBackend for MemoryGithubBackend {
    async fn list_repos(&self) -> Result<Vec<RepoSlug>, GithubBackendError> {
        let state = self.state.lock().expect("memory github lock poisoned");
        Ok(state.repos.keys().cloned().collect())
    }

    async fn has_repo(&self, repo: &RepoSlug) -> Result<bool, GithubBackendError> {
        let state = self.state.lock().expect("memory github lock poisoned");
        Ok(state.repos.contains_key(repo))
    }

    async fn sync_repo(
        &self,
        repo: &RepoSlug,
        if_none_match: Option<&str>,
    ) -> Result<SyncRepoOutcome, GithubBackendError> {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let rate_limit = state.rate_limit.clone();
        let entry = state
            .repos
            .entry(repo.clone())
            .or_insert_with(|| MemoryRepoState {
                next_number: 1,
                next_comment_id: 1,
                etag: Some("W/\"mem-0\"".to_string()),
                ..Default::default()
            });
        entry.last_if_none_match = if_none_match.map(ToString::to_string);
        if if_none_match.is_some()
            && if_none_match == entry.etag.as_deref()
            && entry.pending_events.is_empty()
        {
            return Ok(SyncRepoOutcome::NotModified { rate_limit });
        }
        let events = std::mem::take(&mut entry.pending_events);
        Ok(SyncRepoOutcome::Updated(SyncRepoData {
            etag: entry.etag.clone(),
            rate_limit,
            issues: entry.issues.values().cloned().collect(),
            pulls: entry.pulls.values().cloned().collect(),
            events,
            details_hydrated: true,
        }))
    }

    async fn list_issue_numbers(&self, repo: &RepoSlug) -> Result<Vec<u64>, GithubBackendError> {
        let state = self.state.lock().expect("memory github lock poisoned");
        Ok(state
            .repos
            .get(repo)
            .map(|r| r.issues.keys().copied().collect())
            .unwrap_or_default())
    }

    async fn get_issue(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<IssueRecord, GithubBackendError> {
        let state = self.state.lock().expect("memory github lock poisoned");
        state
            .repos
            .get(repo)
            .and_then(|r| r.issues.get(&number))
            .cloned()
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "issue not found"))
    }

    async fn create_issue(
        &self,
        repo: &RepoSlug,
        req: NewIssueRequest,
    ) -> Result<IssueRecord, GithubBackendError> {
        req.validate()
            .map_err(|code| GithubBackendError::new(code, "invalid new issue payload"))?;
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let actor = state.actor.clone();
        let entry = state
            .repos
            .entry(repo.clone())
            .or_insert_with(|| MemoryRepoState {
                next_number: 1,
                next_comment_id: 1,
                ..Default::default()
            });
        let number = entry.next_number;
        entry.next_number = entry.next_number.saturating_add(1);
        let now = Utc::now().to_rfc3339();
        let mut labels = req.labels;
        labels.sort();
        labels.dedup();
        let record = IssueRecord {
            meta: GithubIssueMeta {
                number,
                title: req.title.trim().to_string(),
                author: actor,
                state: "open".to_string(),
                labels,
                updated_at: now,
                url: format!("https://github.com/{repo}/issues/{number}"),
            },
            body: req.body,
            comments: Vec::new(),
        };
        entry.issues.insert(number, record.clone());
        bump_mem_etag(entry);
        Ok(record)
    }

    async fn add_issue_comment(
        &self,
        repo: &RepoSlug,
        number: u64,
        body: String,
    ) -> Result<GithubComment, GithubBackendError> {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let actor = state.actor.clone();
        let entry = state
            .repos
            .get_mut(repo)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "repo not found"))?;
        let comment_id = entry.next_comment_id;
        entry.next_comment_id = entry.next_comment_id.saturating_add(1);
        let issue = entry
            .issues
            .get_mut(&number)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "issue not found"))?;
        let now = Utc::now().to_rfc3339();
        let comment = GithubComment {
            id: comment_id,
            author: actor,
            body,
            created_at: now.clone(),
        };
        issue.comments.push(comment.clone());
        issue.meta.updated_at = now;
        bump_mem_etag(entry);
        Ok(comment)
    }

    async fn add_issue_label(
        &self,
        repo: &RepoSlug,
        number: u64,
        label: String,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let entry = state
            .repos
            .get_mut(repo)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "repo not found"))?;
        let issue = entry
            .issues
            .get_mut(&number)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "issue not found"))?;
        if !issue.meta.labels.iter().any(|existing| existing == &label) {
            issue.meta.labels.push(label);
            issue.meta.labels.sort();
        }
        issue.meta.updated_at = Utc::now().to_rfc3339();
        let meta = issue.meta.clone();
        bump_mem_etag(entry);
        Ok(meta)
    }

    async fn remove_issue_label(
        &self,
        repo: &RepoSlug,
        number: u64,
        label: String,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let entry = state
            .repos
            .get_mut(repo)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "repo not found"))?;
        let issue = entry
            .issues
            .get_mut(&number)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "issue not found"))?;
        issue.meta.labels.retain(|existing| existing != &label);
        issue.meta.updated_at = Utc::now().to_rfc3339();
        let meta = issue.meta.clone();
        bump_mem_etag(entry);
        Ok(meta)
    }

    async fn close_issue(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let entry = state
            .repos
            .get_mut(repo)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "repo not found"))?;
        let issue = entry
            .issues
            .get_mut(&number)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "issue not found"))?;
        issue.meta.state = "closed".to_string();
        issue.meta.updated_at = Utc::now().to_rfc3339();
        let meta = issue.meta.clone();
        bump_mem_etag(entry);
        Ok(meta)
    }

    async fn list_pull_numbers(&self, repo: &RepoSlug) -> Result<Vec<u64>, GithubBackendError> {
        let state = self.state.lock().expect("memory github lock poisoned");
        Ok(state
            .repos
            .get(repo)
            .map(|r| r.pulls.keys().copied().collect())
            .unwrap_or_default())
    }

    async fn get_pull(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<PullRecord, GithubBackendError> {
        let state = self.state.lock().expect("memory github lock poisoned");
        state
            .repos
            .get(repo)
            .and_then(|r| r.pulls.get(&number))
            .cloned()
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "pull request not found"))
    }

    async fn add_pull_comment(
        &self,
        repo: &RepoSlug,
        number: u64,
        body: String,
    ) -> Result<GithubComment, GithubBackendError> {
        let mut state = self.state.lock().expect("memory github lock poisoned");
        let actor = state.actor.clone();
        let entry = state
            .repos
            .get_mut(repo)
            .ok_or_else(|| GithubBackendError::new(ErrorCode::NotFound, "repo not found"))?;
        let comment_id = entry.next_comment_id;
        entry.next_comment_id = entry.next_comment_id.saturating_add(1);
        let pull = entry.pulls.get_mut(&number).ok_or_else(|| {
            GithubBackendError::new(ErrorCode::NotFound, "pull request not found")
        })?;
        let now = Utc::now().to_rfc3339();
        let comment = GithubComment {
            id: comment_id,
            author: actor,
            body,
            created_at: now.clone(),
        };
        pull.comments.push(comment.clone());
        pull.meta.updated_at = now;
        bump_mem_etag(entry);
        Ok(comment)
    }
}
