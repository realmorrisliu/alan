//! Pure JSON, GraphQL, and GitHub Events API response parsers for `GhCliGithubBackend`.

use alan_ap::ErrorCode;
use serde_json::Value;

use super::super::{
    GithubBackendError, GithubComment, GithubEvent, GithubIssueMeta, GithubPullMeta,
    GithubReviewThread, RepoSlug,
};

pub(super) fn parse_label_names(v: &Value) -> Vec<String> {
    v["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["name"].as_str().map(ToString::to_string))
        .collect()
}

pub(super) fn parse_gh_issue_meta(v: &Value) -> GithubIssueMeta {
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

pub(super) fn parse_gh_comment(v: &Value) -> GithubComment {
    GithubComment {
        id: v["id"].as_u64().unwrap_or(0),
        author: v["user"]["login"].as_str().unwrap_or_default().to_string(),
        body: v["body"].as_str().unwrap_or_default().to_string(),
        created_at: v["created_at"].as_str().unwrap_or_default().to_string(),
    }
}

pub(super) fn parse_gh_pull_meta(v: &Value) -> GithubPullMeta {
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

pub(super) fn parse_graphql_review_threads_page(
    body: &Value,
) -> Result<(Vec<GithubReviewThread>, Option<String>), GithubBackendError> {
    if let Some(errs) = body["errors"].as_array().filter(|e| !e.is_empty()) {
        return Err(classify_graphql_error(errs));
    }
    let pr_val = &body["data"]["repository"]["pullRequest"];
    let Some(threads_obj) = pr_val["reviewThreads"].as_object() else {
        if body["data"]["repository"].is_object() && pr_val.is_null() {
            return Err(GithubBackendError::new(
                ErrorCode::NotFound,
                "pull request not found",
            ));
        }
        return Err(GithubBackendError::new(
            ErrorCode::Io,
            "malformed GraphQL reviewThreads response",
        ));
    };
    let Some(nodes) = threads_obj.get("nodes").and_then(Value::as_array) else {
        return Err(GithubBackendError::new(
            ErrorCode::Io,
            "malformed GraphQL reviewThreads nodes array",
        ));
    };

    let mut page_threads = Vec::new();
    for node in nodes {
        if node["isResolved"].as_bool().unwrap_or(false) {
            continue;
        }
        let first_comment = node["comments"]["nodes"].as_array().and_then(|c| c.first());
        page_threads.push(GithubReviewThread {
            id: node["id"].as_str().unwrap_or_default().to_string(),
            path: node["path"].as_str().map(ToString::to_string),
            line: node["line"]
                .as_u64()
                .or_else(|| node["originalLine"].as_u64()),
            resolved: false,
            author: first_comment
                .and_then(|c| c["author"]["login"].as_str())
                .unwrap_or_default()
                .to_string(),
            body: first_comment
                .and_then(|c| c["body"].as_str())
                .unwrap_or_default()
                .to_string(),
        });
    }

    let has_next = threads_obj
        .get("pageInfo")
        .and_then(|p| p["hasNextPage"].as_bool())
        .unwrap_or(false);
    let next_cursor = has_next
        .then(|| {
            threads_obj
                .get("pageInfo")
                .and_then(|p| p["endCursor"].as_str())
                .filter(|c| !c.is_empty())
                .map(ToString::to_string)
        })
        .flatten();
    Ok((page_threads, next_cursor))
}

fn classify_graphql_error(errors: &[Value]) -> GithubBackendError {
    let first = errors.first();
    let err_type = first
        .and_then(|e| e["type"].as_str())
        .unwrap_or_default()
        .to_ascii_uppercase();
    let msg = first
        .and_then(|e| e["message"].as_str())
        .unwrap_or("GitHub GraphQL error");
    let lowered = msg.to_ascii_lowercase();
    let code = if err_type == "NOT_FOUND" || lowered.contains("not found") {
        ErrorCode::NotFound
    } else if err_type == "RATE_LIMITED" || lowered.contains("rate limit") {
        ErrorCode::RateLimited
    } else if matches!(err_type.as_str(), "FORBIDDEN" | "UNAUTHORIZED")
        || lowered.contains("forbidden")
        || lowered.contains("unauthorized")
    {
        ErrorCode::NoAccess
    } else {
        ErrorCode::Io
    };
    GithubBackendError::new(code, format!("GitHub GraphQL error: {msg}"))
}

pub(super) fn apply_pull_meta_to_event(event: &mut GithubEvent, meta: &GithubPullMeta) {
    if event.labels.is_empty() {
        event.labels = meta.labels.clone();
    }
    if event.head_sha.is_none() && !meta.head_sha.is_empty() {
        event.head_sha = Some(meta.head_sha.clone());
    }
    if event.branch.is_none() && !meta.head_ref.is_empty() {
        event.branch = Some(meta.head_ref.clone());
    }
}

fn parse_pull_event_fields(
    payload: &Value,
) -> (Option<u64>, Vec<String>, Option<String>, Option<String>) {
    let pr = &payload["pull_request"];
    (
        pr["number"].as_u64().or_else(|| payload["number"].as_u64()),
        parse_label_names(pr),
        pr["head"]["sha"].as_str().map(ToString::to_string),
        pr["head"]["ref"].as_str().map(ToString::to_string),
    )
}

pub(super) fn parse_gh_events(repo: &RepoSlug, items: &[Value]) -> Vec<GithubEvent> {
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
                "PullRequestEvent" => {
                    let (num, labels, sha, br) = parse_pull_event_fields(payload);
                    (
                        format!("pull_{action}"),
                        Some("pull".to_string()),
                        num,
                        labels,
                        sha,
                        br,
                    )
                }
                "PullRequestReviewEvent" => {
                    let (num, labels, sha, br) = parse_pull_event_fields(payload);
                    (
                        "pull_review".to_string(),
                        Some("pull".to_string()),
                        num,
                        labels,
                        sha,
                        br,
                    )
                }
                "PullRequestReviewCommentEvent" => {
                    let (num, labels, sha, br) = parse_pull_event_fields(payload);
                    (
                        "pull_review_comment".to_string(),
                        Some("pull".to_string()),
                        num,
                        labels,
                        sha,
                        br,
                    )
                }
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
