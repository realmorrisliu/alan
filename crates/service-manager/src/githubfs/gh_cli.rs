//! Host `gh` CLI adapter implementing [`super::GithubBackend`] with `If-None-Match` ETags.
//!
//! Auth stays strictly inside the Host `gh` CLI / ephemeral in-memory `SecretToken`
//! passed only via `GH_TOKEN` env to child `gh` processes. Tokens are never serialized
//! into any namespace file, error message, or debug formatter.

mod parse;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use alan_ap::ErrorCode;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{
    GithubBackend, GithubBackendError, GithubCheckRun, GithubComment, GithubEvent, GithubIssueMeta,
    GithubPullChecks, GithubPullMeta, GithubPullReviews, GithubReview, GithubReviewThread,
    IssueRecord, NewIssueRequest, PullRecord, RateLimitUpdate, RepoSlug, SyncRepoData,
    SyncRepoOutcome,
};
use parse::{
    apply_pull_meta_to_event, parse_gh_comment, parse_gh_events, parse_gh_issue_meta,
    parse_gh_pull_meta, parse_graphql_review_threads_page, parse_next_link,
};

const ACCEPT_JSON_HEADER: &str = "Accept: application/vnd.github+json";
const PR_REVIEW_THREADS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!,$after:String){\
repository(owner:$owner,name:$repo){\
pullRequest(number:$number){\
reviewThreads(first:100,after:$after){\
pageInfo{hasNextPage endCursor}\
nodes{id isResolved path line originalLine comments(first:1){nodes{author{login} body}}}\
}}}}";
const MAX_REVIEW_THREAD_PAGES: usize = 10;
const MAX_REST_PAGES: usize = 10;
const REST_PAGE_SIZE: usize = 100;

/// Non-leaking wrapper holding an ephemeral Host token resolved via `gh auth token`.
#[derive(Clone)]
struct SecretToken(String);

impl fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Host `gh` CLI adapter implementing [`GithubBackend`] with `If-None-Match` ETags.
#[derive(Debug, Clone)]
pub struct GhCliGithubBackend {
    gh_binary: PathBuf,
    seen_event_ids: Arc<Mutex<HashSet<String>>>,
}

impl Default for GhCliGithubBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GhCliGithubBackend {
    /// Construct a Host `gh` adapter resolving `"gh"` from Host `PATH`.
    pub fn new() -> Self {
        Self::with_binary("gh")
    }

    /// Construct a Host `gh` adapter pointing to an explicit `gh` executable path.
    pub fn with_binary(gh_binary: impl Into<PathBuf>) -> Self {
        Self {
            gh_binary: gh_binary.into(),
            seen_event_ids: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    async fn resolve_host_token(&self) -> Result<Option<SecretToken>, GithubBackendError> {
        let output = Command::new(&self.gh_binary)
            .args(["auth", "token"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|err| {
                GithubBackendError::new(
                    ErrorCode::NoAccess,
                    format!("failed executing Host `gh` adapter: {err}"),
                )
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GithubBackendError::new(
                ErrorCode::NoAccess,
                format!("Host `gh auth token` is unavailable: {}", stderr.trim()),
            ));
        }
        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if token.is_empty() {
            return Err(GithubBackendError::new(
                ErrorCode::NoAccess,
                "Host `gh auth token` returned an empty credential",
            ));
        }
        Ok(Some(SecretToken(token)))
    }

    async fn run_gh_api(
        &self,
        args: &[&str],
        stdin_json: Option<&Value>,
    ) -> Result<GhHttpResponse, GithubBackendError> {
        let secret = self.resolve_host_token().await?;
        let secret_ref = secret.as_ref().map(|s| s.0.as_str());
        let mut cmd = Command::new(&self.gh_binary);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if stdin_json.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        if let Some(ref token) = secret {
            cmd.env("GH_TOKEN", &token.0);
        }
        let mut child = cmd.spawn().map_err(|err| {
            GithubBackendError::redacted(
                ErrorCode::Io,
                format!("failed spawning Host `gh` adapter: {err}"),
                secret_ref,
            )
        })?;
        if let Some(payload) = stdin_json
            && let Some(mut stdin) = child.stdin.take()
        {
            let bytes = serde_json::to_vec(payload).map_err(|err| {
                GithubBackendError::redacted(ErrorCode::BadRequest, err.to_string(), secret_ref)
            })?;
            match stdin.write_all(&bytes).await {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {}
                Err(err) => {
                    return Err(GithubBackendError::redacted(
                        ErrorCode::Io,
                        err.to_string(),
                        secret_ref,
                    ));
                }
            }
            drop(stdin);
        }
        let output = child.wait_with_output().await.map_err(|err| {
            GithubBackendError::redacted(ErrorCode::Io, err.to_string(), secret_ref)
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(parsed) = parse_http_response(&stdout) {
            return validate_http_response(parsed, output.status.success(), &stderr, secret_ref);
        }
        if !output.status.success() {
            return Err(classify_cli_error(&stderr, secret_ref));
        }
        Ok(GhHttpResponse {
            status: 200,
            etag: None,
            next_link: None,
            rate_limit: None,
            body: stdout.to_string(),
        })
    }

    async fn gh_get_json<T: DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<(T, GhHttpResponse), GithubBackendError> {
        let resp = self
            .run_gh_api(&["api", "-i", "-H", ACCEPT_JSON_HEADER, path], None)
            .await?;
        let parsed = serde_json::from_str(&resp.body)
            .map_err(|err| GithubBackendError::new(ErrorCode::Io, err.to_string()))?;
        Ok((parsed, resp))
    }

    async fn gh_get_paginated_array(
        &self,
        first_path: &str,
    ) -> Result<Vec<Value>, GithubBackendError> {
        let mut all_items = Vec::new();
        let mut path = first_path.to_string();
        for _ in 0..MAX_REST_PAGES {
            let (page_items, resp) = self.gh_get_json::<Vec<Value>>(&path).await?;
            all_items.extend(page_items);
            if let Some(next_url) = resp.next_link {
                path = next_url;
            } else {
                break;
            }
        }
        Ok(all_items)
    }

    async fn gh_send_json<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        payload: Option<&Value>,
    ) -> Result<T, GithubBackendError> {
        let mut args = vec!["api", "-i", "-X", method, "-H", ACCEPT_JSON_HEADER, path];
        if payload.is_some() {
            args.extend(["--input", "-"]);
        }
        let resp = self.run_gh_api(&args, payload).await?;
        if resp.body.trim().is_empty() {
            return serde_json::from_str("null")
                .map_err(|err| GithubBackendError::new(ErrorCode::Io, err.to_string()));
        }
        serde_json::from_str(&resp.body)
            .map_err(|err| GithubBackendError::new(ErrorCode::Io, err.to_string()))
    }

    async fn fetch_issue_comments(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<Vec<GithubComment>, GithubBackendError> {
        let path = format!("repos/{repo}/issues/{number}/comments?per_page={REST_PAGE_SIZE}");
        let items = self.gh_get_paginated_array(&path).await?;
        Ok(items.iter().map(parse_gh_comment).collect())
    }

    async fn fetch_pull_checks(
        &self,
        repo: &RepoSlug,
        head_sha: &str,
    ) -> Result<GithubPullChecks, GithubBackendError> {
        if head_sha.trim().is_empty() {
            return Ok(GithubPullChecks {
                head_sha: String::new(),
                checks: Vec::new(),
            });
        }
        let mut runs = Vec::new();
        let mut path =
            format!("repos/{repo}/commits/{head_sha}/check-runs?per_page={REST_PAGE_SIZE}&page=1");
        for page in 1..=MAX_REST_PAGES {
            let (checks_json, resp) = self.gh_get_json::<Value>(&path).await?;
            let page_runs: Vec<GithubCheckRun> = checks_json["check_runs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| GithubCheckRun {
                    name: c["name"].as_str().unwrap_or_default().to_string(),
                    status: c["status"].as_str().unwrap_or_default().to_string(),
                    conclusion: c["conclusion"].as_str().map(ToString::to_string),
                    details_url: c["details_url"].as_str().map(ToString::to_string),
                })
                .collect();
            let page_count = page_runs.len();
            runs.extend(page_runs);
            if let Some(next_url) = resp.next_link {
                path = next_url;
            } else if let Some(total) = checks_json["total_count"].as_u64()
                && page_count > 0
                && (runs.len() as u64) < total
            {
                path = format!(
                    "repos/{repo}/commits/{head_sha}/check-runs?per_page={REST_PAGE_SIZE}&page={}",
                    page + 1
                );
            } else {
                break;
            }
        }
        Ok(GithubPullChecks {
            head_sha: head_sha.to_string(),
            checks: runs,
        })
    }

    async fn fetch_unresolved_review_threads(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<Vec<GithubReviewThread>, GithubBackendError> {
        let mut threads = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..MAX_REVIEW_THREAD_PAGES {
            let payload = serde_json::json!({
                "query": PR_REVIEW_THREADS_QUERY,
                "variables": {
                    "owner": repo.owner,
                    "repo": repo.repo,
                    "number": number,
                    "after": after,
                },
            });
            let body: Value = self.gh_send_json("POST", "graphql", Some(&payload)).await?;
            let (page_threads, next_cursor) = parse_graphql_review_threads_page(&body)?;
            threads.extend(page_threads);
            if let Some(cursor) = next_cursor {
                after = Some(cursor);
            } else {
                break;
            }
        }
        Ok(threads)
    }

    async fn fetch_optional_meta<T>(
        &self,
        path: &str,
        parse: fn(&Value) -> T,
    ) -> Result<Option<T>, GithubBackendError> {
        match self.gh_get_json::<Value>(path).await {
            Ok((v, _)) => Ok(Some(parse(&v))),
            Err(err) if err.code == ErrorCode::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn hydrate_event_metadata(
        &self,
        repo: &RepoSlug,
        events: &mut [GithubEvent],
        issues: &[IssueRecord],
        pulls: &[PullRecord],
    ) -> Result<(), GithubBackendError> {
        use std::collections::hash_map::Entry;

        let issues_by_num: HashMap<u64, &GithubIssueMeta> =
            issues.iter().map(|i| (i.meta.number, &i.meta)).collect();
        let pulls_by_num: HashMap<u64, &GithubPullMeta> =
            pulls.iter().map(|p| (p.meta.number, &p.meta)).collect();
        let mut fetched_pulls: HashMap<u64, Option<GithubPullMeta>> = HashMap::new();
        let mut fetched_issues: HashMap<u64, Option<GithubIssueMeta>> = HashMap::new();

        for event in events {
            let Some(num) = event.number else {
                continue;
            };
            match event.target_kind.as_deref() {
                Some("pull") => {
                    let needs_fill = event.labels.is_empty()
                        || event.head_sha.is_none()
                        || event.branch.is_none();
                    if !needs_fill {
                        continue;
                    }
                    if let Some(meta) = pulls_by_num.get(&num).copied() {
                        apply_pull_meta_to_event(event, meta);
                        continue;
                    }
                    let meta = match fetched_pulls.entry(num) {
                        Entry::Occupied(entry) => entry.into_mut().as_ref(),
                        Entry::Vacant(entry) => {
                            let fetched = self
                                .fetch_optional_meta(
                                    &format!("repos/{repo}/pulls/{num}"),
                                    parse_gh_pull_meta,
                                )
                                .await?;
                            entry.insert(fetched).as_ref()
                        }
                    };
                    if let Some(meta) = meta {
                        apply_pull_meta_to_event(event, meta);
                    }
                }
                Some("issue") if event.labels.is_empty() => {
                    if let Some(meta) = issues_by_num.get(&num).copied() {
                        event.labels = meta.labels.clone();
                        continue;
                    }
                    let meta = match fetched_issues.entry(num) {
                        Entry::Occupied(entry) => entry.into_mut().as_ref(),
                        Entry::Vacant(entry) => {
                            let fetched = self
                                .fetch_optional_meta(
                                    &format!("repos/{repo}/issues/{num}"),
                                    parse_gh_issue_meta,
                                )
                                .await?;
                            entry.insert(fetched).as_ref()
                        }
                    };
                    if let Some(meta) = meta {
                        event.labels = meta.labels.clone();
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct GhHttpResponse {
    status: u16,
    etag: Option<String>,
    next_link: Option<String>,
    rate_limit: Option<RateLimitUpdate>,
    body: String,
}

fn validate_http_response(
    parsed: GhHttpResponse,
    exit_ok: bool,
    stderr: &str,
    secret_ref: Option<&str>,
) -> Result<GhHttpResponse, GithubBackendError> {
    if parsed.status == 304 {
        return Ok(parsed);
    }
    if parsed.status == 404 {
        return Err(GithubBackendError::redacted(
            ErrorCode::NotFound,
            "GitHub resource not found",
            secret_ref,
        ));
    }
    if parsed.status == 429
        || (parsed.status == 403 && parsed.body.to_ascii_lowercase().contains("rate limit"))
    {
        return Err(GithubBackendError::redacted(
            ErrorCode::RateLimited,
            "GitHub API rate limit reached",
            secret_ref,
        ));
    }
    if parsed.status == 401 || parsed.status == 403 {
        return Err(GithubBackendError::redacted(
            ErrorCode::NoAccess,
            format!("GitHub API access denied (HTTP {})", parsed.status),
            secret_ref,
        ));
    }
    if !exit_ok || parsed.status >= 400 {
        return Err(GithubBackendError::redacted(
            ErrorCode::Io,
            format!(
                "GitHub API request failed (HTTP {}): {}",
                parsed.status,
                stderr.trim()
            ),
            secret_ref,
        ));
    }
    Ok(parsed)
}

fn classify_cli_error(stderr: &str, secret_ref: Option<&str>) -> GithubBackendError {
    let lowered = stderr.to_ascii_lowercase();
    let code = if lowered.contains("404") || lowered.contains("not found") {
        ErrorCode::NotFound
    } else if lowered.contains("rate limit") || lowered.contains("429") {
        ErrorCode::RateLimited
    } else if lowered.contains("401") || lowered.contains("403") || lowered.contains("auth") {
        ErrorCode::NoAccess
    } else {
        ErrorCode::Io
    };
    GithubBackendError::redacted(code, format!("gh api error: {}", stderr.trim()), secret_ref)
}

fn parse_http_response(raw: &str) -> Option<GhHttpResponse> {
    let normalized = raw.replace("\r\n", "\n");
    if !normalized.starts_with("HTTP/") {
        return None;
    }
    let (headers_block, body) = normalized.split_once("\n\n").unwrap_or((&normalized, ""));
    let mut lines = headers_block.lines();
    let status_line = lines.next()?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())?;
    let (mut etag, mut next_link, mut limit, mut remaining, mut reset_at) =
        (None, None, None, None, None);
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let val = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "etag" if !val.is_empty() => etag = Some(val.to_string()),
            "link" if !val.is_empty() => next_link = next_link.or_else(|| parse_next_link(val)),
            "x-ratelimit-limit" => limit = val.parse::<u64>().ok(),
            "x-ratelimit-remaining" => remaining = val.parse::<u64>().ok(),
            "x-ratelimit-reset" => {
                reset_at = val
                    .parse::<i64>()
                    .ok()
                    .and_then(|s| chrono::DateTime::from_timestamp(s, 0).map(|dt| dt.to_rfc3339()))
                    .or_else(|| (!val.is_empty()).then(|| val.to_string()));
            }
            _ => {}
        }
    }
    let rate_limit =
        (limit.is_some() || remaining.is_some() || reset_at.is_some()).then_some(RateLimitUpdate {
            limit,
            remaining,
            reset_at,
        });
    Some(GhHttpResponse {
        status,
        etag,
        next_link,
        rate_limit,
        body: body.to_string(),
    })
}

#[async_trait]
impl GithubBackend for GhCliGithubBackend {
    async fn list_repos(&self) -> Result<Vec<RepoSlug>, GithubBackendError> {
        Ok(Vec::new())
    }

    async fn has_repo(&self, repo: &RepoSlug) -> Result<bool, GithubBackendError> {
        let path = format!("repos/{repo}");
        match self
            .run_gh_api(&["api", "-i", "-H", ACCEPT_JSON_HEADER, &path], None)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.code == ErrorCode::NotFound => Ok(false),
            Err(err) => Err(err),
        }
    }

    async fn sync_repo(
        &self,
        repo: &RepoSlug,
        if_none_match: Option<&str>,
    ) -> Result<SyncRepoOutcome, GithubBackendError> {
        let events_path = format!("repos/{repo}/events?per_page=30");
        let mut args = vec!["api", "-i", "-H", ACCEPT_JSON_HEADER];
        let header;
        if let Some(etag) = if_none_match {
            header = format!("If-None-Match: {etag}");
            args.extend(["-H", &header]);
        }
        args.push(&events_path);
        let resp = self.run_gh_api(&args, None).await?;
        if resp.status == 304 {
            return Ok(SyncRepoOutcome::NotModified {
                rate_limit: resp.rate_limit,
            });
        }

        let raw_events: Vec<Value> = serde_json::from_str(&resp.body)
            .map_err(|err| GithubBackendError::new(ErrorCode::Io, err.to_string()))?;
        let mut candidate_events: Vec<GithubEvent> = {
            let seen = self
                .seen_event_ids
                .lock()
                .expect("seen events lock poisoned");
            let mut batch_seen = std::collections::HashSet::new();
            parse_gh_events(repo, &raw_events)
                .into_iter()
                .filter(|event| {
                    !seen.contains(&event.event_id) && batch_seen.insert(event.event_id.clone())
                })
                .collect()
        };

        let (raw_issues, _) = self
            .gh_get_json::<Vec<Value>>(&format!("repos/{repo}/issues?state=all&per_page=30"))
            .await?;
        let issues: Vec<IssueRecord> = raw_issues
            .iter()
            .filter(|item| !item["pull_request"].is_object())
            .map(|item| IssueRecord {
                meta: parse_gh_issue_meta(item),
                body: item["body"].as_str().unwrap_or_default().to_string(),
                comments: Vec::new(),
            })
            .collect();

        let (raw_pulls, pulls_resp) = self
            .gh_get_json::<Vec<Value>>(&format!("repos/{repo}/pulls?state=all&per_page=30"))
            .await?;
        let pulls: Vec<PullRecord> = raw_pulls
            .iter()
            .map(|item| {
                let meta = parse_gh_pull_meta(item);
                let head_sha = meta.head_sha.clone();
                PullRecord {
                    meta,
                    checks: GithubPullChecks {
                        head_sha,
                        checks: Vec::new(),
                    },
                    reviews: GithubPullReviews {
                        reviews: Vec::new(),
                        unresolved_threads: Vec::new(),
                    },
                    comments: Vec::new(),
                }
            })
            .collect();

        self.hydrate_event_metadata(repo, &mut candidate_events, &issues, &pulls)
            .await?;

        // Commit deduplication state only after every fallible API call in sync succeeds.
        let new_events = {
            let mut seen = self
                .seen_event_ids
                .lock()
                .expect("seen events lock poisoned");
            candidate_events
                .into_iter()
                .filter(|event| seen.insert(event.event_id.clone()))
                .collect()
        };

        Ok(SyncRepoOutcome::Updated(SyncRepoData {
            etag: resp.etag,
            rate_limit: pulls_resp.rate_limit.or(resp.rate_limit),
            issues,
            pulls,
            events: new_events,
            details_hydrated: false,
        }))
    }

    async fn list_issue_numbers(&self, repo: &RepoSlug) -> Result<Vec<u64>, GithubBackendError> {
        let (items, _) = self
            .gh_get_json::<Vec<Value>>(&format!("repos/{repo}/issues?state=all&per_page=30"))
            .await?;
        Ok(items
            .iter()
            .filter(|v| !v["pull_request"].is_object())
            .filter_map(|v| v["number"].as_u64())
            .collect())
    }

    async fn get_issue(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<IssueRecord, GithubBackendError> {
        let (item, _) = self
            .gh_get_json::<Value>(&format!("repos/{repo}/issues/{number}"))
            .await?;
        let comments = self.fetch_issue_comments(repo, number).await?;
        Ok(IssueRecord {
            meta: parse_gh_issue_meta(&item),
            body: item["body"].as_str().unwrap_or_default().to_string(),
            comments,
        })
    }

    async fn create_issue(
        &self,
        repo: &RepoSlug,
        req: NewIssueRequest,
    ) -> Result<IssueRecord, GithubBackendError> {
        req.validate()
            .map_err(|code| GithubBackendError::new(code, "invalid new issue payload"))?;
        let payload = serde_json::json!({
            "title": req.title.trim(),
            "body": req.body,
            "labels": req.labels,
        });
        let item: Value = self
            .gh_send_json("POST", &format!("repos/{repo}/issues"), Some(&payload))
            .await?;
        Ok(IssueRecord {
            meta: parse_gh_issue_meta(&item),
            body: item["body"].as_str().unwrap_or_default().to_string(),
            comments: Vec::new(),
        })
    }

    async fn add_issue_comment(
        &self,
        repo: &RepoSlug,
        number: u64,
        body: String,
    ) -> Result<GithubComment, GithubBackendError> {
        let payload = serde_json::json!({ "body": body });
        let item: Value = self
            .gh_send_json(
                "POST",
                &format!("repos/{repo}/issues/{number}/comments"),
                Some(&payload),
            )
            .await?;
        Ok(parse_gh_comment(&item))
    }

    async fn add_issue_label(
        &self,
        repo: &RepoSlug,
        number: u64,
        label: String,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        let payload = serde_json::json!({ "labels": [label] });
        let _: Value = self
            .gh_send_json(
                "POST",
                &format!("repos/{repo}/issues/{number}/labels"),
                Some(&payload),
            )
            .await?;
        self.get_issue(repo, number).await.map(|issue| issue.meta)
    }

    async fn remove_issue_label(
        &self,
        repo: &RepoSlug,
        number: u64,
        label: String,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        let encoded = percent_encode_component(&label);
        let _: Value = self
            .gh_send_json(
                "DELETE",
                &format!("repos/{repo}/issues/{number}/labels/{encoded}"),
                None,
            )
            .await?;
        self.get_issue(repo, number).await.map(|issue| issue.meta)
    }

    async fn close_issue(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        let payload = serde_json::json!({ "state": "closed" });
        let item: Value = self
            .gh_send_json(
                "PATCH",
                &format!("repos/{repo}/issues/{number}"),
                Some(&payload),
            )
            .await?;
        Ok(parse_gh_issue_meta(&item))
    }

    async fn list_pull_numbers(&self, repo: &RepoSlug) -> Result<Vec<u64>, GithubBackendError> {
        let (items, _) = self
            .gh_get_json::<Vec<Value>>(&format!("repos/{repo}/pulls?state=all&per_page=30"))
            .await?;
        Ok(items.iter().filter_map(|v| v["number"].as_u64()).collect())
    }

    async fn get_pull(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<PullRecord, GithubBackendError> {
        let (item, _) = self
            .gh_get_json::<Value>(&format!("repos/{repo}/pulls/{number}"))
            .await?;
        let meta = parse_gh_pull_meta(&item);
        let checks = self.fetch_pull_checks(repo, &meta.head_sha).await?;
        let raw_reviews = self
            .gh_get_paginated_array(&format!(
                "repos/{repo}/pulls/{number}/reviews?per_page={REST_PAGE_SIZE}"
            ))
            .await?;
        let reviews = raw_reviews
            .iter()
            .map(|r| GithubReview {
                id: r["id"].as_u64().unwrap_or(0),
                author: r["user"]["login"].as_str().unwrap_or_default().to_string(),
                state: r["state"].as_str().unwrap_or_default().to_string(),
                body: r["body"].as_str().unwrap_or_default().to_string(),
                submitted_at: r["submitted_at"].as_str().unwrap_or_default().to_string(),
            })
            .collect();
        let unresolved_threads = self.fetch_unresolved_review_threads(repo, number).await?;
        let comments = self.fetch_issue_comments(repo, number).await?;
        Ok(PullRecord {
            meta,
            checks,
            reviews: GithubPullReviews {
                reviews,
                unresolved_threads,
            },
            comments,
        })
    }

    async fn get_pull_checks(
        &self,
        repo: &RepoSlug,
        number: u64,
        head_sha: &str,
    ) -> Result<GithubPullChecks, GithubBackendError> {
        let resolved_sha = if head_sha.trim().is_empty() {
            let (item, _) = self
                .gh_get_json::<Value>(&format!("repos/{repo}/pulls/{number}"))
                .await?;
            parse_gh_pull_meta(&item).head_sha
        } else {
            head_sha.to_string()
        };
        self.fetch_pull_checks(repo, &resolved_sha).await
    }

    async fn add_pull_comment(
        &self,
        repo: &RepoSlug,
        number: u64,
        body: String,
    ) -> Result<GithubComment, GithubBackendError> {
        self.add_issue_comment(repo, number, body).await
    }
}

fn percent_encode_component(raw: &str) -> String {
    let mut encoded = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}
