use std::sync::Arc;
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, LiveNamespace, MountFs, Namespace};
use alan_routefs::RouteFs;
use tokio::sync::oneshot;

use super::backend::{
    GhCliGithubBackend, GithubBackend, GithubBackendError, GithubComment, GithubEvent,
    GithubIssueMeta, GithubPullChecks, GithubPullMeta, GithubPullReviews, IssueRecord,
    MemoryGithubBackend, NewIssueRequest, PullRecord, RepoSlug, SyncRepoOutcome,
    redact_github_tokens, validate_label,
};
use super::dispatch::{
    CoalesceKey, DispatchOutcome, GithubEventDispatcher, SkipReason, TriggerAllowlist,
};
use super::{GithubFs, MAX_CTL_BYTES};

#[test]
fn slug_and_payload_validation_fail_closed() {
    assert!(RepoSlug::parse("realmorrisliu/alan").is_ok());
    assert!(RepoSlug::parse("org.name/repo_1-2").is_ok());
    assert!(RepoSlug::parse("../alan").is_err());
    assert!(RepoSlug::parse("owner/..").is_err());
    assert!(RepoSlug::parse("owner/repo/extra").is_err());
    assert!(RepoSlug::parse("owner/").is_err());
    assert!(RepoSlug::parse("/repo").is_err());

    assert_eq!(validate_label("alan:auto").unwrap(), "alan:auto");
    assert!(validate_label("   ").is_err());
    assert!(validate_label("bad\nlabel").is_err());
    assert!(validate_label("bad\0label").is_err());

    let bad_title = NewIssueRequest {
        title: "   ".to_string(),
        body: "details".to_string(),
        labels: vec![],
    };
    assert_eq!(bad_title.validate(), Err(ErrorCode::BadRequest));

    let bad_nul = NewIssueRequest {
        title: "ok".to_string(),
        body: "bad\0body".to_string(),
        labels: vec![],
    };
    assert_eq!(bad_nul.validate(), Err(ErrorCode::BadRequest));
}

#[test]
fn token_redaction_scrubs_prefixes_and_bearer_headers() {
    let raw = "auth failed for ghp_secretToken12345 and github_pat_11AA_BBcc with Bearer my.jwt.token and token raw_tok_99";
    let scrubbed = redact_github_tokens(raw, Some("explicit-secret-xyz"));
    assert!(!scrubbed.contains("ghp_secretToken12345"), "{scrubbed}");
    assert!(!scrubbed.contains("github_pat_11AA_BBcc"), "{scrubbed}");
    assert!(!scrubbed.contains("my.jwt.token"), "{scrubbed}");
    assert!(!scrubbed.contains("raw_tok_99"), "{scrubbed}");
    assert!(scrubbed.contains("[REDACTED]"), "{scrubbed}");

    let explicit = redact_github_tokens(
        "leaked explicit-secret-xyz here",
        Some("explicit-secret-xyz"),
    );
    assert!(!explicit.contains("explicit-secret-xyz"));
    assert!(explicit.contains("[REDACTED]"));
}

#[test]
fn trigger_allowlist_validation_fails_closed() {
    let valid = TriggerAllowlist {
        allowed_repos: vec!["acme/alan".to_string()],
        allowed_actors: vec!["morris".to_string()],
        required_labels: vec!["alan:auto".to_string()],
        ignored_actors: vec!["alan-bot".to_string()],
        ignored_branch_prefixes: vec!["alan/".to_string()],
        max_concurrent: 2,
        max_retries: 3,
    };
    assert!(valid.validate().is_ok());

    let mut empty_repos = valid.clone();
    empty_repos.allowed_repos.clear();
    assert!(empty_repos.validate().is_err());

    let mut empty_actors = valid.clone();
    empty_actors.allowed_actors.clear();
    assert!(empty_actors.validate().is_err());

    let mut empty_labels = valid.clone();
    empty_labels.required_labels.clear();
    assert!(empty_labels.validate().is_err());

    let mut zero_conc = valid.clone();
    zero_conc.max_concurrent = 0;
    assert!(zero_conc.validate().is_err());

    let mut zero_retries = valid;
    zero_retries.max_retries = 0;
    assert!(zero_retries.validate().is_err());
}

#[tokio::test]
async fn ap_fid_lifecycle_and_write_failed_guard_hold() {
    let backend = Arc::new(MemoryGithubBackend::new());
    let slug = RepoSlug::parse("acme/alan").unwrap();
    backend.seed_issue(
        &slug,
        IssueRecord {
            meta: GithubIssueMeta {
                number: 1,
                title: "Initial".to_string(),
                author: "morris".to_string(),
                state: "open".to_string(),
                labels: vec![],
                updated_at: "2026-11-22T00:00:00Z".to_string(),
                url: "https://github.com/acme/alan/issues/1".to_string(),
            },
            body: "hello".to_string(),
            comments: vec![],
        },
    );
    let fs = GithubFs::new(backend);

    // Root cannot be opened for write or overwritten by walk.
    assert_eq!(
        fs.open(Fid::ROOT, OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        fs.walk(Fid::ROOT, Fid::ROOT, &["ctl".into()]).await,
        Err(ErrorCode::BadRequest)
    );

    // Read-only files reject Write/ReadWrite at open.
    fs.walk(Fid::ROOT, Fid(1), &["status".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(1), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        fs.open(Fid(1), OpenMode::ReadWrite).await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(1)).await.unwrap();

    fs.walk(Fid::ROOT, Fid(2), &["events".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(2), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(2)).await.unwrap();

    // Reopening an already-open fid is rejected.
    fs.walk(Fid::ROOT, Fid(3), &["ctl".into()]).await.unwrap();
    fs.open(Fid(3), OpenMode::Write).await.unwrap();
    assert_eq!(
        fs.open(Fid(3), OpenMode::Read).await,
        Err(ErrorCode::BadRequest)
    );
    // Valid write followed by overflowing write marks `write_failed`; clunk must NOT commit.
    fs.write(Fid(3), 0, b"watch other/repo").await.unwrap();
    assert_eq!(
        fs.write(Fid(3), (MAX_CTL_BYTES + 1) as u64, b"x").await,
        Err(ErrorCode::BadRequest)
    );
    fs.clunk(Fid(3)).await.unwrap();

    fs.walk(Fid::ROOT, Fid(4), &["status".into()])
        .await
        .unwrap();
    fs.open(Fid(4), OpenMode::Read).await.unwrap();
    let status_bytes = fs.read(Fid(4), 0, 4096).await.unwrap();
    fs.clunk(Fid(4)).await.unwrap();
    let status_text = String::from_utf8(status_bytes).unwrap();
    assert!(!status_text.contains("other/repo"), "{status_text}");

    // Zero-write open+clunk does nothing, whereas explicit zero-byte write fails closed.
    fs.walk(Fid::ROOT, Fid(5), &["ctl".into()]).await.unwrap();
    fs.open(Fid(5), OpenMode::Write).await.unwrap();
    fs.clunk(Fid(5)).await.unwrap();

    fs.walk(Fid::ROOT, Fid(6), &["ctl".into()]).await.unwrap();
    fs.open(Fid(6), OpenMode::Write).await.unwrap();
    fs.write(Fid(6), 0, b"").await.unwrap();
    assert_eq!(fs.clunk(Fid(6)).await, Err(ErrorCode::BadRequest));

    // Create/remove unsupported.
    assert_eq!(
        fs.create(Fid::ROOT, Fid(7), "foo", FileKind::File).await,
        Err(ErrorCode::Unsupported)
    );
    fs.walk(
        Fid::ROOT,
        Fid(8),
        &[
            "repos".into(),
            "acme".into(),
            "alan".into(),
            "issues".into(),
            "1".into(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(fs.remove(Fid(8)).await, Err(ErrorCode::Unsupported));
    fs.clunk(Fid(8)).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn gh_cli_backend_uses_if_none_match_and_redacts_error_tokens() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let etag_seen_file = dir.path().join("etag_seen.txt");
    let gh_script = dir.path().join("gh");
    let secret = "ghp_UNIT_TEST_SECRET_VALUE_98765";

    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${{1:-}}" == "auth" && "${{2:-}}" == "token" ]]; then
  echo "{secret}"
  exit 0
fi
if [[ "${{1:-}}" == "api" ]]; then
  all_args="$*"
  if [[ "$all_args" == *"repos/acme/leak/events"* ]]; then
    echo "error authenticating with {secret}" >&2
    exit 1
  fi
  if [[ "$all_args" == *"If-None-Match: W/\"gh-1\""* ]]; then
    echo "seen-etag" > "{etag_file}"
    printf 'HTTP/1.1 304 Not Modified\r\nETag: W/"gh-1"\r\nX-RateLimit-Limit: 5000\r\nX-RateLimit-Remaining: 4998\r\n\r\n'
    exit 0
  fi
  if [[ "$all_args" == *"repos/acme/alan/events"* ]]; then
    printf 'HTTP/1.1 200 OK\r\nETag: W/"gh-1"\r\nX-RateLimit-Limit: 5000\r\nX-RateLimit-Remaining: 4999\r\n\r\n[]'
    exit 0
  fi
  if [[ "$all_args" == *"repos/acme/alan/issues"* || "$all_args" == *"repos/acme/alan/pulls"* ]]; then
    printf 'HTTP/1.1 200 OK\r\n\r\n[]'
    exit 0
  fi
fi
exit 1
"#,
        secret = secret,
        etag_file = etag_seen_file.display()
    );
    std::fs::write(&gh_script, script).unwrap();
    let mut perms = std::fs::metadata(&gh_script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&gh_script, perms).unwrap();

    let backend = GhCliGithubBackend::with_binary(&gh_script);
    let slug = RepoSlug::parse("acme/alan").unwrap();
    let first = backend.sync_repo(&slug, None).await.unwrap();
    let SyncRepoOutcome::Updated(data) = first else {
        panic!("expected Updated on first sync");
    };
    assert_eq!(data.etag.as_deref(), Some("W/\"gh-1\""));

    let second = backend
        .sync_repo(&slug, data.etag.as_deref())
        .await
        .unwrap();
    assert!(matches!(second, SyncRepoOutcome::NotModified { .. }));
    assert!(etag_seen_file.exists());

    let leak_slug = RepoSlug::parse("acme/leak").unwrap();
    let err = backend.sync_repo(&leak_slug, None).await.unwrap_err();
    assert!(
        !err.message.contains(secret),
        "token leaked: {}",
        err.message
    );
    assert!(err.message.contains("[REDACTED]"));
}

#[tokio::test]
async fn dispatcher_live_tail_processes_events_and_stops_cleanly_on_shutdown() {
    let backend = Arc::new(MemoryGithubBackend::new());
    let slug = RepoSlug::parse("acme/alan").unwrap();
    backend.ensure_repo(&slug);
    let githubfs = Arc::new(
        GithubFs::with_watched_repos(backend.clone(), [slug.clone()])
            .await
            .unwrap(),
    );
    let routefs = Arc::new(RouteFs::new());

    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/github",
        InProcessTransport::new(githubfs.clone()),
        Access::ReadOnly,
    );
    ns.mount(
        "/mnt/route",
        InProcessTransport::new(routefs.clone()),
        Access::ReadWrite,
    );
    let transport = InProcessTransport::new(Arc::new(MountFs::new(ns)));

    let allowlist = TriggerAllowlist {
        allowed_repos: vec!["acme/alan".to_string()],
        allowed_actors: vec!["morris".to_string()],
        required_labels: vec!["alan:auto".to_string()],
        ignored_actors: vec!["alan-bot".to_string()],
        ignored_branch_prefixes: vec!["alan/".to_string()],
        max_concurrent: 2,
        max_retries: 2,
    };
    let mut dispatcher = GithubEventDispatcher::new(transport.clone(), allowlist).unwrap();
    dispatcher.ensure_route_rules().await.unwrap();

    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move { dispatcher.tail_events_until(rx).await });

    backend.push_event(
        &slug,
        GithubEvent {
            cursor: 0,
            event_id: "ev-live-1".to_string(),
            kind: "issue_labeled".to_string(),
            actor: "morris".to_string(),
            repo: "acme/alan".to_string(),
            number: Some(9),
            labels: vec!["alan:auto".to_string()],
            head_sha: None,
            target_kind: Some("issue".to_string()),
            branch: None,
            timestamp: None,
        },
    );
    backend.push_event(
        &slug,
        GithubEvent {
            cursor: 0,
            event_id: "ev-live-self".to_string(),
            kind: "issue_labeled".to_string(),
            actor: "alan-bot".to_string(),
            repo: "acme/alan".to_string(),
            number: Some(9),
            labels: vec!["alan:auto".to_string()],
            head_sha: None,
            target_kind: Some("issue".to_string()),
            branch: None,
            timestamp: None,
        },
    );
    githubfs.sync_watched_repos().await.unwrap();

    let shell = alan_shell::Shell::new(transport);
    let worker_msg = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let bytes = shell.cat("/mnt/route/ports/worker").await.unwrap();
            if !bytes.is_empty() {
                break String::from_utf8(bytes).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(worker_msg.contains("\"number\":9"), "{worker_msg}");
    assert!(
        worker_msg.contains("/mnt/github/repos/acme/alan/issues/9/meta.json"),
        "{worker_msg}"
    );
    assert!(
        worker_msg.contains("/mnt/github/repos/acme/alan/issues/9/body.md"),
        "{worker_msg}"
    );
    assert!(
        worker_msg.contains("/mnt/github/repos/acme/alan/issues/9/comments"),
        "{worker_msg}"
    );

    let _ = tx.send(());
    let outcomes = handle.await.unwrap().unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(matches!(
        outcomes[1],
        super::dispatch::DispatchOutcome::Skipped {
            reason: SkipReason::SelfActor,
            ..
        }
    ));
}

#[tokio::test]
async fn dispatcher_rolls_back_bookkeeping_and_recovers_after_route_failure() {
    let backend = Arc::new(MemoryGithubBackend::new());
    let slug = RepoSlug::parse("acme/alan").unwrap();
    backend.ensure_repo(&slug);
    let githubfs = Arc::new(
        GithubFs::with_watched_repos(backend.clone(), [slug.clone()])
            .await
            .unwrap(),
    );
    let routefs = Arc::new(RouteFs::new());
    let live_ns = LiveNamespace::new(Namespace::new());
    live_ns.replace_mount(
        "/mnt/github",
        InProcessTransport::new(githubfs.clone()),
        Access::ReadOnly,
    );
    live_ns.replace_mount(
        "/mnt/route",
        InProcessTransport::new(routefs.clone()),
        Access::ReadWrite,
    );
    let transport =
        InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live_ns.clone())));

    let allowlist = TriggerAllowlist {
        allowed_repos: vec!["acme/alan".to_string()],
        allowed_actors: vec!["morris".to_string()],
        required_labels: vec!["alan:auto".to_string()],
        ignored_actors: vec![],
        ignored_branch_prefixes: vec![],
        max_concurrent: 1,
        max_retries: 1,
    };
    let mut dispatcher = GithubEventDispatcher::new(transport.clone(), allowlist).unwrap();
    dispatcher.ensure_route_rules().await.unwrap();

    live_ns.replace_mount(
        "/mnt/route",
        InProcessTransport::new(routefs.clone()),
        Access::ReadOnly,
    );
    let make_event = |id: &str| GithubEvent {
        cursor: 0,
        event_id: id.to_string(),
        kind: "pull_synchronized".to_string(),
        actor: "morris".to_string(),
        repo: "acme/alan".to_string(),
        number: Some(55),
        labels: vec!["alan:auto".to_string()],
        head_sha: Some("sha-55".to_string()),
        target_kind: Some("pull".to_string()),
        branch: Some("feat/55".to_string()),
        timestamp: None,
    };
    backend.push_event(&slug, make_event("ev-recover-1"));
    githubfs.sync_watched_repos().await.unwrap();

    assert!(dispatcher.process_available_events().await.is_err());
    let s = dispatcher.stats();
    assert_eq!(
        (
            s.active_runs,
            s.dispatched_worker,
            s.deduplicated,
            s.events_processed
        ),
        (0, 0, 0, 0)
    );

    live_ns.replace_mount(
        "/mnt/route",
        InProcessTransport::new(routefs.clone()),
        Access::ReadWrite,
    );
    let outcomes = dispatcher.process_available_events().await.unwrap();
    assert!(
        matches!(outcomes.as_slice(), [DispatchOutcome::DispatchedWorker { event_id, attempt: 1, .. }] if event_id == "ev-recover-1")
    );
    let worker_envelope = String::from_utf8(
        alan_shell::Shell::new(transport)
            .cat("/mnt/route/ports/worker")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        worker_envelope.contains("/mnt/github/repos/acme/alan/pulls/55/meta.json"),
        "{worker_envelope}"
    );
    assert!(
        worker_envelope.contains("/mnt/github/repos/acme/alan/pulls/55/checks.json"),
        "{worker_envelope}"
    );
    assert!(
        worker_envelope.contains("/mnt/github/repos/acme/alan/pulls/55/reviews.json"),
        "{worker_envelope}"
    );
    assert!(
        !worker_envelope.contains("/pulls/55/body.md"),
        "{worker_envelope}"
    );

    dispatcher
        .complete_run(&CoalesceKey::new("acme/alan", 55, Some("sha-55")))
        .await
        .unwrap();
    live_ns.replace_mount(
        "/mnt/route",
        InProcessTransport::new(routefs.clone()),
        Access::ReadOnly,
    );
    let ev2 = make_event("ev-escalate-2");
    assert!(dispatcher.process_event(&ev2).await.is_err());
    assert_eq!(dispatcher.stats().escalated_human_review, 0);

    live_ns.replace_mount(
        "/mnt/route",
        InProcessTransport::new(routefs),
        Access::ReadWrite,
    );
    let out2 = dispatcher.process_event(&ev2).await.unwrap();
    assert!(
        matches!(out2, DispatchOutcome::EscalatedHumanReview { event_id, .. } if event_id == "ev-escalate-2")
    );
}

struct CountingGithubBackend {
    inner: MemoryGithubBackend,
    empty_list_repos: bool,
    list_repos_calls: std::sync::atomic::AtomicUsize,
    list_issue_numbers_calls: std::sync::atomic::AtomicUsize,
    list_pull_numbers_calls: std::sync::atomic::AtomicUsize,
}

impl CountingGithubBackend {
    fn new(inner: MemoryGithubBackend) -> Self {
        Self {
            inner,
            empty_list_repos: false,
            list_repos_calls: std::sync::atomic::AtomicUsize::new(0),
            list_issue_numbers_calls: std::sync::atomic::AtomicUsize::new(0),
            list_pull_numbers_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn on_demand_only(inner: MemoryGithubBackend) -> Self {
        Self {
            empty_list_repos: true,
            ..Self::new(inner)
        }
    }

    fn counts(&self) -> (usize, usize, usize) {
        use std::sync::atomic::Ordering::SeqCst;
        (
            self.list_repos_calls.load(SeqCst),
            self.list_issue_numbers_calls.load(SeqCst),
            self.list_pull_numbers_calls.load(SeqCst),
        )
    }
}

#[async_trait::async_trait]
impl GithubBackend for CountingGithubBackend {
    async fn list_repos(&self) -> Result<Vec<RepoSlug>, GithubBackendError> {
        self.list_repos_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.empty_list_repos {
            Ok(Vec::new())
        } else {
            self.inner.list_repos().await
        }
    }

    async fn has_repo(&self, repo: &RepoSlug) -> Result<bool, GithubBackendError> {
        self.inner.has_repo(repo).await
    }

    async fn sync_repo(
        &self,
        repo: &RepoSlug,
        if_none_match: Option<&str>,
    ) -> Result<SyncRepoOutcome, GithubBackendError> {
        self.inner.sync_repo(repo, if_none_match).await
    }

    async fn list_issue_numbers(&self, repo: &RepoSlug) -> Result<Vec<u64>, GithubBackendError> {
        self.list_issue_numbers_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.list_issue_numbers(repo).await
    }

    async fn get_issue(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<IssueRecord, GithubBackendError> {
        self.inner.get_issue(repo, number).await
    }

    async fn create_issue(
        &self,
        repo: &RepoSlug,
        req: NewIssueRequest,
    ) -> Result<IssueRecord, GithubBackendError> {
        self.inner.create_issue(repo, req).await
    }

    async fn add_issue_comment(
        &self,
        repo: &RepoSlug,
        number: u64,
        body: String,
    ) -> Result<GithubComment, GithubBackendError> {
        self.inner.add_issue_comment(repo, number, body).await
    }

    async fn add_issue_label(
        &self,
        repo: &RepoSlug,
        number: u64,
        label: String,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        self.inner.add_issue_label(repo, number, label).await
    }

    async fn remove_issue_label(
        &self,
        repo: &RepoSlug,
        number: u64,
        label: String,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        self.inner.remove_issue_label(repo, number, label).await
    }

    async fn close_issue(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<GithubIssueMeta, GithubBackendError> {
        self.inner.close_issue(repo, number).await
    }

    async fn list_pull_numbers(&self, repo: &RepoSlug) -> Result<Vec<u64>, GithubBackendError> {
        self.list_pull_numbers_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.list_pull_numbers(repo).await
    }

    async fn get_pull(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<PullRecord, GithubBackendError> {
        self.inner.get_pull(repo, number).await
    }

    async fn get_pull_checks(
        &self,
        repo: &RepoSlug,
        number: u64,
        head_sha: &str,
    ) -> Result<GithubPullChecks, GithubBackendError> {
        self.inner.get_pull_checks(repo, number, head_sha).await
    }

    async fn add_pull_comment(
        &self,
        repo: &RepoSlug,
        number: u64,
        body: String,
    ) -> Result<GithubComment, GithubBackendError> {
        self.inner.add_pull_comment(repo, number, body).await
    }
}

#[tokio::test]
async fn on_demand_readonly_mount_walks_and_reads_without_ctl_write() {
    let inner = MemoryGithubBackend::new();
    let slug = RepoSlug::parse("acme/alan").unwrap();
    inner.seed_issue(
        &slug,
        IssueRecord {
            meta: GithubIssueMeta {
                number: 12,
                title: "On-demand read".to_string(),
                author: "morris".to_string(),
                state: "open".to_string(),
                labels: vec!["bug".to_string()],
                updated_at: "2026-11-24T00:00:00Z".to_string(),
                url: "https://github.com/acme/alan/issues/12".to_string(),
            },
            body: "details".to_string(),
            comments: vec![],
        },
    );
    inner.seed_pull(
        &slug,
        PullRecord {
            meta: GithubPullMeta {
                number: 42,
                title: "Lazy PR list".to_string(),
                author: "morris".to_string(),
                head_ref: "feat/lazy".to_string(),
                head_sha: "sha-42".to_string(),
                base_ref: "main".to_string(),
                state: "open".to_string(),
                mergeable: Some(true),
                labels: vec![],
                updated_at: "2026-11-24T00:00:00Z".to_string(),
                url: "https://github.com/acme/alan/pull/42".to_string(),
            },
            checks: GithubPullChecks {
                head_sha: "sha-42".to_string(),
                checks: vec![],
            },
            reviews: GithubPullReviews {
                reviews: vec![],
                unresolved_threads: vec![],
            },
            comments: vec![],
        },
    );

    let fs = Arc::new(GithubFs::new(Arc::new(
        CountingGithubBackend::on_demand_only(inner),
    )));
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/github",
        InProcessTransport::new(fs.clone()),
        Access::ReadOnly,
    );
    let shell = alan_shell::Shell::new(InProcessTransport::new(Arc::new(MountFs::new(ns))));

    // Precondition: list_repos() is empty, and we have NOT written /mnt/github/ctl.
    assert!(shell.ls("/mnt/github/repos").await.unwrap().is_empty());

    // On-demand walk/list/read succeeds on a ReadOnly mount without `watch` in ctl.
    assert_eq!(
        shell
            .ls("/mnt/github/repos/acme/alan/issues")
            .await
            .unwrap(),
        vec!["new", "12"]
    );
    let issue_meta: GithubIssueMeta = serde_json::from_slice(
        &shell
            .cat("/mnt/github/repos/acme/alan/issues/12/meta.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(issue_meta.number, 12);
    assert_eq!(issue_meta.title, "On-demand read");
    assert_eq!(
        shell.ls("/mnt/github/repos/acme/alan/pulls").await.unwrap(),
        vec!["42"]
    );

    // Mutating files still reject write opens with NoAccess under ReadOnly mount.
    assert_eq!(
        shell.write("/mnt/github/ctl", b"watch acme/alan").await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        shell
            .write(
                "/mnt/github/repos/acme/alan/issues/new",
                br#"{"title":"Blocked","body":"ro"}"#,
            )
            .await,
        Err(ErrorCode::NoAccess)
    );

    // Nonexistent repos still return NotFound, and invalid owner slugs are rejected.
    assert_eq!(
        shell.ls("/mnt/github/repos/acme/missing/issues").await,
        Err(ErrorCode::NotFound)
    );
    assert_eq!(
        shell.ls("/mnt/github/repos/unknown-owner/missing").await,
        Err(ErrorCode::NotFound)
    );
    for bad_owner in ["", ".", "..", "bad/owner", "bad owner"] {
        assert_eq!(
            fs.walk(Fid::ROOT, Fid(99), &["repos".into(), bad_owner.into()])
                .await,
            Err(ErrorCode::NotFound),
            "bad owner `{bad_owner}` should fail"
        );
    }
}

#[tokio::test]
async fn directory_stat_skips_backend_listing_calls() {
    let inner = MemoryGithubBackend::new();
    let slug = RepoSlug::parse("acme/alan").unwrap();
    inner.seed_issue(
        &slug,
        IssueRecord {
            meta: GithubIssueMeta {
                number: 1,
                title: "Stat check".to_string(),
                author: "morris".to_string(),
                state: "open".to_string(),
                labels: vec![],
                updated_at: "2026-11-24T00:00:00Z".to_string(),
                url: "https://github.com/acme/alan/issues/1".to_string(),
            },
            body: "body".to_string(),
            comments: vec![],
        },
    );
    inner.push_event(
        &slug,
        GithubEvent {
            cursor: 0,
            event_id: "ev-stat-1".to_string(),
            kind: "issue_opened".to_string(),
            actor: "morris".to_string(),
            repo: "acme/alan".to_string(),
            number: Some(1),
            labels: vec![],
            head_sha: None,
            target_kind: Some("issue".to_string()),
            branch: None,
            timestamp: None,
        },
    );
    let backend = Arc::new(CountingGithubBackend::new(inner));
    let fs = Arc::new(
        GithubFs::with_watched_repos(backend.clone(), [slug])
            .await
            .unwrap(),
    );
    fs.sync_watched_repos().await.unwrap();
    assert_eq!(backend.counts(), (0, 0, 0));

    let dir_paths: &[&[&str]] = &[
        &["repos"],
        &["repos", "acme"],
        &["repos", "acme", "alan"],
        &["repos", "acme", "alan", "issues"],
        &["repos", "acme", "alan", "pulls"],
    ];
    for (idx, path) in dir_paths.iter().enumerate() {
        let fid = Fid((idx + 10) as u64);
        let names: Vec<String> = path.iter().map(|s| (*s).to_string()).collect();
        fs.walk(Fid::ROOT, fid, &names).await.unwrap();
        let st = fs.stat(fid).await.unwrap();
        assert_eq!(st.qid.kind, FileKind::Dir);
        assert_eq!(st.length, 0);
        fs.clunk(fid).await.unwrap();
    }
    assert_eq!(backend.counts(), (0, 0, 0));

    // Events stream and regular files still report computed byte lengths.
    fs.walk(Fid::ROOT, Fid(20), &["events".into()])
        .await
        .unwrap();
    assert!(fs.stat(Fid(20)).await.unwrap().length > 0);
    fs.clunk(Fid(20)).await.unwrap();

    fs.walk(Fid::ROOT, Fid(21), &["status".into()])
        .await
        .unwrap();
    assert!(fs.stat(Fid(21)).await.unwrap().length > 0);
    fs.clunk(Fid(21)).await.unwrap();
    assert_eq!(backend.counts(), (0, 0, 0));
}

#[tokio::test]
async fn synced_repo_dirs_read_cache_first_while_unsynced_repo_queries_on_demand() {
    let inner = MemoryGithubBackend::new();
    let synced_slug = RepoSlug::parse("acme/alan").unwrap();
    let unsynced_slug = RepoSlug::parse("acme/other").unwrap();
    for (slug, issue_num, pull_num) in [(&synced_slug, 10, 20), (&unsynced_slug, 30, 40)] {
        inner.seed_issue(
            slug,
            IssueRecord {
                meta: GithubIssueMeta {
                    number: issue_num,
                    title: format!("Issue {issue_num}"),
                    author: "morris".to_string(),
                    state: "open".to_string(),
                    labels: vec![],
                    updated_at: "2026-11-24T00:00:00Z".to_string(),
                    url: format!("https://github.com/{slug}/issues/{issue_num}"),
                },
                body: "body".to_string(),
                comments: vec![],
            },
        );
        inner.seed_pull(
            slug,
            PullRecord {
                meta: GithubPullMeta {
                    number: pull_num,
                    title: format!("PR {pull_num}"),
                    author: "morris".to_string(),
                    head_ref: "feat/x".to_string(),
                    head_sha: format!("sha-{pull_num}"),
                    base_ref: "main".to_string(),
                    state: "open".to_string(),
                    mergeable: Some(true),
                    labels: vec![],
                    updated_at: "2026-11-24T00:00:00Z".to_string(),
                    url: format!("https://github.com/{slug}/pull/{pull_num}"),
                },
                checks: GithubPullChecks {
                    head_sha: format!("sha-{pull_num}"),
                    checks: vec![],
                },
                reviews: GithubPullReviews {
                    reviews: vec![],
                    unresolved_threads: vec![],
                },
                comments: vec![],
            },
        );
    }

    let backend = Arc::new(CountingGithubBackend::new(inner));
    let fs = Arc::new(
        GithubFs::with_watched_repos(backend.clone(), [synced_slug])
            .await
            .unwrap(),
    );
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/github",
        InProcessTransport::new(fs),
        Access::ReadWrite,
    );
    let shell = alan_shell::Shell::new(InProcessTransport::new(Arc::new(MountFs::new(ns))));

    shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    assert_eq!(
        shell
            .ls("/mnt/github/repos/acme/alan/issues")
            .await
            .unwrap(),
        vec!["new", "10"]
    );
    assert_eq!(
        shell.ls("/mnt/github/repos/acme/alan/pulls").await.unwrap(),
        vec!["20"]
    );
    assert_eq!(backend.counts(), (0, 0, 0));

    // A subsequent 304 Not Modified sync preserves `synced = true`.
    shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    assert_eq!(
        shell
            .ls("/mnt/github/repos/acme/alan/issues")
            .await
            .unwrap(),
        vec!["new", "10"]
    );
    assert_eq!(
        shell.ls("/mnt/github/repos/acme/alan/pulls").await.unwrap(),
        vec!["20"]
    );
    assert_eq!(backend.counts(), (0, 0, 0));

    // Unsynced repo still queries backend on demand for IssuesDir and PullsDir.
    assert_eq!(
        shell
            .ls("/mnt/github/repos/acme/other/issues")
            .await
            .unwrap(),
        vec!["new", "30"]
    );
    assert_eq!(
        shell
            .ls("/mnt/github/repos/acme/other/pulls")
            .await
            .unwrap(),
        vec!["40"]
    );
    assert_eq!(backend.counts(), (0, 1, 1));
}
