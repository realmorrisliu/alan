//! Host `gh` CLI adapter implementing [`super::GithubBackend`] with `If-None-Match` ETags.
//!
//! Auth stays strictly inside the Host `gh` CLI / ephemeral in-memory `SecretToken`
//! passed only via `GH_TOKEN` env to child `gh` processes. Tokens are never serialized
//! into any namespace file, error message, or debug formatter.

use std::collections::HashSet;
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
    GithubPullChecks, GithubPullMeta, GithubPullReviews, GithubReview, IssueRecord,
    NewIssueRequest, PullRecord, RateLimitUpdate, RepoSlug, SyncRepoData, SyncRepoOutcome,
};

const ACCEPT_JSON_HEADER: &str = "Accept: application/vnd.github+json";

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
        let path = format!("repos/{repo}/issues/{number}/comments?per_page=100");
        let (items, _) = self.gh_get_json::<Vec<Value>>(&path).await?;
        Ok(items.iter().map(parse_gh_comment).collect())
    }
}

#[derive(Debug)]
struct GhHttpResponse {
    status: u16,
    etag: Option<String>,
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
    let (mut etag, mut limit, mut remaining, mut reset_at) = (None, None, None, None);
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let val = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "etag" if !val.is_empty() => etag = Some(val.to_string()),
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
        rate_limit,
        body: body.to_string(),
    })
}

fn parse_label_names(v: &Value) -> Vec<String> {
    v["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["name"].as_str().map(ToString::to_string))
        .collect()
}

fn parse_gh_issue_meta(v: &Value) -> GithubIssueMeta {
    GithubIssueMeta {
        number: v["number"].as_u64().unwrap_or(0),
        title: v["title"].as_str().unwrap_or_default().to_string(),
        author: v["user"]["login"].as_str().unwrap_or_default().to_string(),
        state: v["state"].as_str().unwrap_or("open").to_string(),
        labels: parse_label_names(v),
        updated_at: v["updated_at"].as_str().unwrap_or_default().to_string(),
        url: v["html_url"].as_str().unwrap_or_default().to_string(),
    }
}

fn parse_gh_comment(v: &Value) -> GithubComment {
    GithubComment {
        id: v["id"].as_u64().unwrap_or(0),
        author: v["user"]["login"].as_str().unwrap_or_default().to_string(),
        body: v["body"].as_str().unwrap_or_default().to_string(),
        created_at: v["created_at"].as_str().unwrap_or_default().to_string(),
    }
}

fn parse_gh_pull_meta(v: &Value) -> GithubPullMeta {
    GithubPullMeta {
        number: v["number"].as_u64().unwrap_or(0),
        title: v["title"].as_str().unwrap_or_default().to_string(),
        author: v["user"]["login"].as_str().unwrap_or_default().to_string(),
        head_ref: v["head"]["ref"].as_str().unwrap_or_default().to_string(),
        head_sha: v["head"]["sha"].as_str().unwrap_or_default().to_string(),
        base_ref: v["base"]["ref"].as_str().unwrap_or_default().to_string(),
        state: v["state"].as_str().unwrap_or("open").to_string(),
        mergeable: v["mergeable"].as_bool(),
        labels: parse_label_names(v),
        updated_at: v["updated_at"].as_str().unwrap_or_default().to_string(),
        url: v["html_url"].as_str().unwrap_or_default().to_string(),
    }
}

fn parse_gh_events(repo: &RepoSlug, items: &[Value]) -> Vec<GithubEvent> {
    items
        .iter()
        .rev()
        .filter_map(|item| {
            let event_id = item["id"].as_str()?.to_string();
            let raw_type = item["type"].as_str().unwrap_or("unknown");
            let actor = item["actor"]["login"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let payload = &item["payload"];
            let action = payload["action"].as_str().unwrap_or_default();
            let (kind, target_kind, number, labels, head_sha, branch) = match raw_type {
                "IssuesEvent" => (
                    format!("issue_{action}"),
                    Some("issue".to_string()),
                    payload["issue"]["number"].as_u64(),
                    parse_label_names(&payload["issue"]),
                    None,
                    None,
                ),
                "IssueCommentEvent" => (
                    "issue_commented".to_string(),
                    Some(if payload["issue"]["pull_request"].is_object() {
                        "pull".to_string()
                    } else {
                        "issue".to_string()
                    }),
                    payload["issue"]["number"].as_u64(),
                    parse_label_names(&payload["issue"]),
                    None,
                    None,
                ),
                "PullRequestEvent" => (
                    format!("pull_{action}"),
                    Some("pull".to_string()),
                    payload["pull_request"]["number"]
                        .as_u64()
                        .or_else(|| payload["number"].as_u64()),
                    parse_label_names(&payload["pull_request"]),
                    payload["pull_request"]["head"]["sha"]
                        .as_str()
                        .map(ToString::to_string),
                    payload["pull_request"]["head"]["ref"]
                        .as_str()
                        .map(ToString::to_string),
                ),
                "PullRequestReviewEvent" => (
                    "pull_review".to_string(),
                    Some("pull".to_string()),
                    payload["pull_request"]["number"].as_u64(),
                    Vec::new(),
                    payload["pull_request"]["head"]["sha"]
                        .as_str()
                        .map(ToString::to_string),
                    payload["pull_request"]["head"]["ref"]
                        .as_str()
                        .map(ToString::to_string),
                ),
                _ => (
                    raw_type.to_ascii_lowercase(),
                    None,
                    None,
                    Vec::new(),
                    payload["head"].as_str().map(ToString::to_string),
                    payload["ref"]
                        .as_str()
                        .map(|r| r.strip_prefix("refs/heads/").unwrap_or(r).to_string()),
                ),
            };
            Some(GithubEvent {
                cursor: 0,
                event_id,
                kind,
                actor,
                repo: repo.as_slug(),
                number,
                labels,
                head_sha,
                target_kind,
                branch,
                timestamp: item["created_at"].as_str().map(ToString::to_string),
            })
        })
        .collect()
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
        let candidate_events: Vec<GithubEvent> = {
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
        let issues = raw_issues
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
        let pulls = raw_pulls
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
        let checks = if meta.head_sha.is_empty() {
            GithubPullChecks {
                head_sha: String::new(),
                checks: Vec::new(),
            }
        } else {
            let (checks_json, _) = self
                .gh_get_json::<Value>(&format!(
                    "repos/{repo}/commits/{}/check-runs",
                    meta.head_sha
                ))
                .await?;
            let runs = checks_json["check_runs"]
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
            GithubPullChecks {
                head_sha: meta.head_sha.clone(),
                checks: runs,
            }
        };
        let (raw_reviews, _) = self
            .gh_get_json::<Vec<Value>>(&format!("repos/{repo}/pulls/{number}/reviews?per_page=100"))
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
        let comments = self.fetch_issue_comments(repo, number).await?;
        Ok(PullRecord {
            meta,
            checks,
            reviews: GithubPullReviews {
                reviews,
                unresolved_threads: Vec::new(),
            },
            comments,
        })
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
