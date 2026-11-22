use std::sync::Arc;
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, LiveNamespace, MountFs, Namespace};
use alan_routefs::RouteFs;
use tokio::sync::oneshot;

use super::backend::{
    GhCliGithubBackend, GithubBackend, GithubEvent, GithubIssueMeta, IssueRecord,
    MemoryGithubBackend, NewIssueRequest, RepoSlug, SyncRepoOutcome, redact_github_tokens,
    validate_label,
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

#[cfg(unix)]
#[tokio::test]
async fn list_only_sync_hydrates_details_on_read_preserves_unchanged_and_revalidates_on_change() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let sync_phase = dir.path().join("phase.txt");
    std::fs::write(&sync_phase, "1").unwrap();
    let gh_script = dir.path().join("gh");

    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${{1:-}}" == "auth" && "${{2:-}}" == "token" ]]; then
  echo "ghp_sync_hydration_token"
  exit 0
fi
phase="$(cat "{phase_file}")"
if [[ "${{1:-}}" == "api" ]]; then
  all_args="$*"
  case "$all_args" in
    *"graphql"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n{{"data":{{"repository":{{"pullRequest":{{"reviewThreads":{{"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[{{"id":"PRRT_resolved","isResolved":true,"path":"ignored.rs","line":1,"originalLine":1,"comments":{{"nodes":[{{"author":{{"login":"codex"}},"body":"already resolved"}}]}}}},{{"id":"PRRT_1","isResolved":false,"path":"crates/service-manager/src/githubfs/gh_cli.rs","line":null,"originalLine":42,"comments":{{"nodes":[{{"author":{{"login":"codex"}},"body":"thread-body-%s"}}]}}}}]}}}}}}}}}}' "$phase"
      ;;
    *"repos/acme/alan/events"*)
      if [[ "$phase" == "2b" ]]; then
        printf 'HTTP/1.1 304 Not Modified\r\n\r\n'
      else
        printf 'HTTP/1.1 200 OK\r\nETag: W/"sync-%s"\r\n\r\n[]' "$phase"
      fi
      ;;
    *"repos/acme/alan/issues/10/comments"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n[{{"id":101,"user":{{"login":"morris"}},"body":"issue-comment-%s","created_at":"2026-11-22T01:00:00Z"}}]' "$phase"
      ;;
    *"repos/acme/alan/issues/20/comments"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n[]'
      ;;
    *"repos/acme/alan/issues/10"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n{{"number":10,"title":"Bug","user":{{"login":"morris"}},"state":"open","labels":[],"updated_at":"2026-11-22T01:00:00Z","body":"b","html_url":"https://github.com/acme/alan/issues/10"}}'
      ;;
    *"repos/acme/alan/issues"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n[{{"number":10,"title":"Bug","user":{{"login":"morris"}},"state":"open","labels":[],"updated_at":"2026-11-22T01:00:00Z","body":"b","html_url":"https://github.com/acme/alan/issues/10"}}]'
      ;;
    *"repos/acme/alan/pulls/20/reviews"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n[{{"id":201,"user":{{"login":"codex"}},"state":"COMMENTED","body":"review-%s","submitted_at":"2026-11-22T01:05:00Z"}}]' "$phase"
      ;;
    *"repos/acme/alan/pulls/20"*)
      sha="sha-1"; updated="2026-11-22T01:00:00Z"
      if [[ "$phase" == "3" ]]; then sha="sha-2"; updated="2026-11-22T02:00:00Z"; fi
      printf 'HTTP/1.1 200 OK\r\n\r\n{{"number":20,"title":"PR","user":{{"login":"morris"}},"state":"open","draft":false,"mergeable":true,"head":{{"ref":"fix","sha":"%s"}},"base":{{"ref":"main"}},"updated_at":"%s","html_url":"https://github.com/acme/alan/pull/20"}}' "$sha" "$updated"
      ;;
    *"repos/acme/alan/pulls"*)
      sha="sha-1"; updated="2026-11-22T01:00:00Z"
      if [[ "$phase" == "3" ]]; then sha="sha-2"; updated="2026-11-22T02:00:00Z"; fi
      printf 'HTTP/1.1 200 OK\r\n\r\n[{{"number":20,"title":"PR","user":{{"login":"morris"}},"state":"open","draft":false,"mergeable":true,"head":{{"ref":"fix","sha":"%s"}},"base":{{"ref":"main"}},"updated_at":"%s","html_url":"https://github.com/acme/alan/pull/20"}}]' "$sha" "$updated"
      ;;
    *"repos/acme/alan/commits/sha-1/check-runs"*)
      if [[ "$phase" == "1" ]]; then
        printf 'HTTP/1.1 200 OK\r\n\r\n{{"check_runs":[{{"name":"ci-1","status":"in_progress","conclusion":null,"details_url":"https://ci/1"}}]}}'
      elif [[ "$phase" == "2a" ]]; then
        printf 'HTTP/1.1 200 OK\r\n\r\n{{"check_runs":[{{"name":"ci-1","status":"completed","conclusion":"failure","details_url":"https://ci/1"}}]}}'
      else
        printf 'HTTP/1.1 200 OK\r\n\r\n{{"check_runs":[{{"name":"ci-1","status":"completed","conclusion":"success","details_url":"https://ci/1"}}]}}'
      fi
      ;;
    *"repos/acme/alan/commits/sha-2/check-runs"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n{{"check_runs":[{{"name":"ci-2","status":"completed","conclusion":"success","details_url":"https://ci/2"}}]}}'
      ;;
    *)
      exit 1
      ;;
  esac
  exit 0
fi
exit 1
"#,
        phase_file = sync_phase.display()
    );
    std::fs::write(&gh_script, script).unwrap();
    let mut perms = std::fs::metadata(&gh_script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&gh_script, perms).unwrap();

    let backend = Arc::new(GhCliGithubBackend::with_binary(&gh_script));
    let slug = RepoSlug::parse("acme/alan").unwrap();
    let githubfs = Arc::new(
        GithubFs::with_watched_repos(backend, [slug.clone()])
            .await
            .unwrap(),
    );
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/github",
        InProcessTransport::new(githubfs),
        Access::ReadWrite,
    );
    let shell = alan_shell::Shell::new(InProcessTransport::new(Arc::new(MountFs::new(ns))));

    // Phase 1: list-only sync must still hydrate checks.json (pending), reviews.json + unresolved GraphQL threads, and issue comments.
    shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    let checks_1 = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/checks.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(checks_1.contains("ci-1"), "{checks_1}");
    assert!(checks_1.contains("in_progress"), "{checks_1}");
    let reviews_1 = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/reviews.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(reviews_1.contains("review-1"), "{reviews_1}");
    assert!(reviews_1.contains("PRRT_1"), "{reviews_1}");
    assert!(reviews_1.contains("thread-body-1"), "{reviews_1}");
    assert!(reviews_1.contains("\"line\": 42"), "{reviews_1}");
    assert!(!reviews_1.contains("PRRT_resolved"), "{reviews_1}");
    let comments_1 = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/issues/10/comments")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(comments_1.contains("issue-comment-1"), "{comments_1}");

    // Phase 2: repeated checks.json read WITHOUT sync observes completion for the same sha/updated_at while reviews stay cached.
    std::fs::write(&sync_phase, "2").unwrap();
    let checks_2_nosync = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/checks.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(checks_2_nosync.contains("completed"), "{checks_2_nosync}");
    assert!(checks_2_nosync.contains("success"), "{checks_2_nosync}");
    let reviews_2_nosync = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/reviews.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(reviews_2_nosync.contains("review-1"), "{reviews_2_nosync}");
    assert!(
        reviews_2_nosync.contains("thread-body-1"),
        "{reviews_2_nosync}"
    );

    // Phase 2a: 200 OK list-only sync with unchanged sha/updated_at still lets checks.json observe fresh check state, while preserving reviews cache.
    std::fs::write(&sync_phase, "2a").unwrap();
    shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    let checks_2a = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/checks.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(checks_2a.contains("failure"), "{checks_2a}");
    let reviews_2a = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/reviews.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(reviews_2a.contains("review-1"), "{reviews_2a}");

    // Phase 2b: 304 Not Modified sync also allows checks.json to observe completed check runs.
    std::fs::write(&sync_phase, "2b").unwrap();
    shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    let checks_2b = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/checks.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(checks_2b.contains("success"), "{checks_2b}");

    // Phase 3: new head_sha/updated_at invalidates full hydration so reviews and threads refresh too.
    std::fs::write(&sync_phase, "3").unwrap();
    shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    let checks_3 = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/checks.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(checks_3.contains("ci-2"), "{checks_3}");
    let reviews_3 = String::from_utf8(
        shell
            .cat("/mnt/github/repos/acme/alan/pulls/20/reviews.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(reviews_3.contains("review-3"), "{reviews_3}");
    assert!(reviews_3.contains("thread-body-3"), "{reviews_3}");
}

#[cfg(unix)]
#[tokio::test]
async fn gh_cli_sync_defers_event_dedup_on_failure_and_ignores_fast_fail_stdin_epipe() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let fail_pulls = dir.path().join("fail_pulls.txt");
    std::fs::write(&fail_pulls, "1").unwrap();
    let gh_script = dir.path().join("gh");

    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${{1:-}}" == "auth" && "${{2:-}}" == "token" ]]; then
  echo "ghp_transient_token"
  exit 0
fi
if [[ "${{1:-}}" == "api" ]]; then
  all_args="$*"
  if [[ "$all_args" == *"-X POST"* && "$all_args" == *"repos/acme/alan/issues"* ]]; then
    # Exit immediately without reading stdin to force EPIPE on large request bodies.
    printf 'HTTP/1.1 422 Unprocessable Entity\r\n\r\n{{"message":"validation failed"}}'
    echo "validation failed" >&2
    exit 1
  fi
  if [[ "$all_args" == *"repos/acme/alan/events"* ]]; then
    printf 'HTTP/1.1 200 OK\r\nETag: W/"e-1"\r\n\r\n[{{"id":"ev-transient-1","type":"IssuesEvent","actor":{{"login":"morris"}},"created_at":"2026-11-22T01:00:00Z","payload":{{"action":"labeled","issue":{{"number":42,"labels":[{{"name":"alan:auto"}}]}}}}}}]'
    exit 0
  fi
  if [[ "$all_args" == *"repos/acme/alan/issues"* ]]; then
    printf 'HTTP/1.1 200 OK\r\n\r\n[]'
    exit 0
  fi
  if [[ "$all_args" == *"repos/acme/alan/pulls"* ]]; then
    if [[ "$(cat "{fail_flag}")" == "1" ]]; then
      echo "transient upstream 500" >&2
      exit 1
    fi
    printf 'HTTP/1.1 200 OK\r\n\r\n[]'
    exit 0
  fi
fi
exit 1
"#,
        fail_flag = fail_pulls.display()
    );
    std::fs::write(&gh_script, script).unwrap();
    let mut perms = std::fs::metadata(&gh_script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&gh_script, perms).unwrap();

    let backend = GhCliGithubBackend::with_binary(&gh_script);
    let slug = RepoSlug::parse("acme/alan").unwrap();

    // First sync fails on pulls: must return Err and MUST NOT lose ev-transient-1.
    assert!(backend.sync_repo(&slug, None).await.is_err());

    // Second sync recovers: ev-transient-1 must still be delivered.
    std::fs::write(&fail_pulls, "0").unwrap();
    let SyncRepoOutcome::Updated(second) = backend.sync_repo(&slug, None).await.unwrap() else {
        panic!("expected Updated on retry");
    };
    assert_eq!(second.events.len(), 1);
    assert_eq!(second.events[0].event_id, "ev-transient-1");

    // Third sync deduplicates already-committed ev-transient-1.
    let SyncRepoOutcome::Updated(third) = backend.sync_repo(&slug, None).await.unwrap() else {
        panic!("expected Updated without ETag");
    };
    assert!(third.events.is_empty(), "{:?}", third.events);

    // Large payload POST to a fast-exiting gh script surfaces API error instead of BrokenPipe.
    let err = backend
        .create_issue(
            &slug,
            NewIssueRequest {
                title: "Big".to_string(),
                body: "y".repeat(256 * 1024),
                labels: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(err.message.contains("HTTP 422"), "{err}");
}

#[cfg(unix)]
#[tokio::test]
async fn gh_cli_hydrates_review_event_labels_and_fails_closed_on_graphql_errors() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let fail_pr99 = dir.path().join("fail_pr99.txt");
    let pr20_fetched = dir.path().join("pr20_fetched.flag");
    std::fs::write(&fail_pr99, "1").unwrap();
    let gh_script = dir.path().join("gh");

    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${{1:-}}" == "auth" && "${{2:-}}" == "token" ]]; then
  echo "ghp_review_label_token"
  exit 0
fi
if [[ "${{1:-}}" == "api" ]]; then
  all_args="$*"
  case "$all_args" in
    *"graphql"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n{{"errors":[{{"type":"FORBIDDEN","message":"forbidden"}}]}}'
      ;;
    *"repos/acme/alan/events"*)
      printf 'HTTP/1.1 200 OK\r\nETag: W/"rev-1"\r\n\r\n[{{"id":"ev-review-20","type":"PullRequestReviewEvent","actor":{{"login":"morris"}},"created_at":"2026-11-22T01:05:00Z","payload":{{"action":"created","pull_request":{{"number":20,"head":{{"sha":"sha-20","ref":"feat/pr-20"}}}}}}}},{{"id":"ev-review-comment-99","type":"PullRequestReviewCommentEvent","actor":{{"login":"morris"}},"created_at":"2026-11-22T01:04:00Z","payload":{{"action":"created","pull_request":{{"number":99,"head":{{"sha":"sha-99","ref":"feat/pr-99"}}}}}}}}]'
      ;;
    *"repos/acme/alan/issues/20/comments"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n[]'
      ;;
    *"repos/acme/alan/issues"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n[]'
      ;;
    *"repos/acme/alan/pulls/99"*)
      if [[ "$(cat "{fail_flag}")" == "1" ]]; then
        echo "upstream 500 fetching PR 99" >&2
        exit 1
      fi
      printf 'HTTP/1.1 200 OK\r\n\r\n{{"number":99,"title":"Older PR","user":{{"login":"morris"}},"state":"open","draft":false,"mergeable":true,"labels":[{{"name":"alan:auto"}}],"head":{{"ref":"feat/pr-99","sha":"sha-99"}},"base":{{"ref":"main"}},"updated_at":"2026-11-22T01:00:00Z","html_url":"https://github.com/acme/alan/pull/99"}}'
      ;;
    *"repos/acme/alan/pulls/20/reviews"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n[]'
      ;;
    *"repos/acme/alan/pulls/20"*)
      touch "{pr20_flag}"
      printf 'HTTP/1.1 200 OK\r\n\r\n{{"number":20,"title":"Synced PR","user":{{"login":"morris"}},"state":"open","draft":false,"mergeable":true,"labels":[{{"name":"alan:auto"}}],"head":{{"ref":"feat/pr-20","sha":"sha-20"}},"base":{{"ref":"main"}},"updated_at":"2026-11-22T01:00:00Z","html_url":"https://github.com/acme/alan/pull/20"}}'
      ;;
    *"repos/acme/alan/commits/sha-20/check-runs"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n{{"check_runs":[]}}'
      ;;
    *"repos/acme/alan/pulls"*)
      printf 'HTTP/1.1 200 OK\r\n\r\n[{{"number":20,"title":"Synced PR","user":{{"login":"morris"}},"state":"open","draft":false,"mergeable":true,"labels":[{{"name":"alan:auto"}}],"head":{{"ref":"feat/pr-20","sha":"sha-20"}},"base":{{"ref":"main"}},"updated_at":"2026-11-22T01:00:00Z","html_url":"https://github.com/acme/alan/pull/20"}}]'
      ;;
    *)
      exit 1
      ;;
  esac
  exit 0
fi
exit 1
"#,
        fail_flag = fail_pr99.display(),
        pr20_flag = pr20_fetched.display(),
    );
    std::fs::write(&gh_script, script).unwrap();
    let mut perms = std::fs::metadata(&gh_script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&gh_script, perms).unwrap();

    let backend = Arc::new(GhCliGithubBackend::with_binary(&gh_script));
    let slug = RepoSlug::parse("acme/alan").unwrap();

    // If metadata fallback fails transiently during sync_repo, event dedup state must not be committed.
    assert!(backend.sync_repo(&slug, None).await.is_err());

    std::fs::write(&fail_pr99, "0").unwrap();
    let githubfs = Arc::new(
        GithubFs::with_watched_repos(backend.clone(), [slug.clone()])
            .await
            .unwrap(),
    );
    let routefs = Arc::new(RouteFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/github",
        InProcessTransport::new(githubfs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/route",
        InProcessTransport::new(routefs),
        Access::ReadWrite,
    );
    let transport = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = alan_shell::Shell::new(transport.clone());

    shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    // PR #20 is in synced pulls, so label hydration must NOT issue a redundant GET /pulls/20 call.
    assert!(!pr20_fetched.exists());

    let allowlist = TriggerAllowlist {
        allowed_repos: vec!["acme/alan".to_string()],
        allowed_actors: vec!["morris".to_string()],
        required_labels: vec!["alan:auto".to_string()],
        ignored_actors: vec![],
        ignored_branch_prefixes: vec![],
        max_concurrent: 4,
        max_retries: 3,
    };
    let mut dispatcher = GithubEventDispatcher::new(transport, allowlist).unwrap();
    let outcomes = dispatcher.process_available_events().await.unwrap();
    assert_eq!(outcomes.len(), 2, "{outcomes:?}");
    assert!(matches!(
        &outcomes[0],
        super::dispatch::DispatchOutcome::DispatchedWorker { event_id, .. }
            if event_id == "ev-review-comment-99"
    ));
    assert!(matches!(
        &outcomes[1],
        super::dispatch::DispatchOutcome::DispatchedWorker { event_id, .. }
            if event_id == "ev-review-20"
    ));

    // GraphQL errors must fail closed rather than returning unresolved_threads: [].
    let err = backend.get_pull(&slug, 20).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::NoAccess, "{err}");
}

#[cfg(unix)]
#[tokio::test]
async fn gh_cli_paginates_check_runs_and_reviews_across_multiple_pages() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let gh_script = dir.path().join("gh");
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "auth" && "${2:-}" == "token" ]]; then
  echo "ghp_pagination_token"
  exit 0
fi
all_args="$*"
case "$all_args" in
  *"graphql"*)
    printf 'HTTP/1.1 200 OK\r\n\r\n{"data":{"repository":{"pullRequest":{"reviewThreads":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]}}}}}'
    ;;
  *"repos/acme/alan/issues/30/comments"*)
    printf 'HTTP/1.1 200 OK\r\n\r\n[]'
    ;;
  *"repos/acme/alan/pulls/30/reviews"*)
    if [[ "$all_args" == *"page=2"* ]]; then
      printf 'HTTP/1.1 200 OK\r\n\r\n[{"id":302,"user":{"login":"morris"},"state":"APPROVED","body":"review-page-2","submitted_at":"2026-11-22T01:06:00Z"}]'
    else
      printf 'HTTP/1.1 200 OK\r\nLink: <repos/acme/alan/pulls/30/reviews?per_page=100&page=2>; rel="next"\r\n\r\n[{"id":301,"user":{"login":"codex"},"state":"COMMENTED","body":"review-page-1","submitted_at":"2026-11-22T01:05:00Z"}]'
    fi
    ;;
  *"repos/acme/alan/pulls/30"*)
    printf 'HTTP/1.1 200 OK\r\n\r\n{"number":30,"title":"Multi-page","user":{"login":"morris"},"state":"open","draft":false,"mergeable":true,"head":{"ref":"fix","sha":"sha-multi"},"base":{"ref":"main"},"updated_at":"2026-11-22T01:00:00Z","html_url":"https://github.com/acme/alan/pull/30"}'
    ;;
  *"repos/acme/alan/commits/sha-multi/check-runs"*)
    if [[ "$all_args" == *"page=2"* ]]; then
      printf 'HTTP/1.1 200 OK\r\n\r\n{"total_count":2,"check_runs":[{"name":"test-suite","status":"completed","conclusion":"failure"}]}'
    else
      printf 'HTTP/1.1 200 OK\r\nLink: <repos/acme/alan/commits/sha-multi/check-runs?per_page=100&page=2>; rel="next"\r\n\r\n{"total_count":2,"check_runs":[{"name":"lint","status":"completed","conclusion":"success"}]}'
    fi
    ;;
  *"repos/acme/alan/commits/sha-total/check-runs"*)
    if [[ "$all_args" == *"page=2"* ]]; then
      printf 'HTTP/1.1 200 OK\r\n\r\n{"total_count":2,"check_runs":[{"name":"total-2","status":"completed","conclusion":"failure"}]}'
    else
      printf 'HTTP/1.1 200 OK\r\n\r\n{"total_count":2,"check_runs":[{"name":"total-1","status":"completed","conclusion":"success"}]}'
    fi
    ;;
  *)
    exit 1
    ;;
esac
"#;
    std::fs::write(&gh_script, script).unwrap();
    let mut perms = std::fs::metadata(&gh_script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&gh_script, perms).unwrap();

    let backend = GhCliGithubBackend::with_binary(&gh_script);
    let slug = RepoSlug::parse("acme/alan").unwrap();
    let pull = backend.get_pull(&slug, 30).await.unwrap();
    let names: Vec<&str> = pull.checks.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["lint", "test-suite"]);
    assert_eq!(pull.checks.checks[1].conclusion.as_deref(), Some("failure"));
    let review_ids: Vec<u64> = pull.reviews.reviews.iter().map(|r| r.id).collect();
    assert_eq!(review_ids, vec![301, 302]);

    let total_checks = backend
        .get_pull_checks(&slug, 30, "sha-total")
        .await
        .unwrap();
    let total_names: Vec<&str> = total_checks
        .checks
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(total_names, vec!["total-1", "total-2"]);
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
    let mut dispatcher = GithubEventDispatcher::new(transport, allowlist).unwrap();
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
