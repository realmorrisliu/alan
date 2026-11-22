use std::sync::Arc;

use alan_ap::{ErrorCode, InProcessTransport};
use alan_kernel::{Access, MountFs, Namespace};
use alan_routefs::RouteFs;

use super::GithubFs;
use super::backend::{
    GhCliGithubBackend, GithubBackend, NewIssueRequest, RepoSlug, SyncRepoOutcome,
};
use super::dispatch::{GithubEventDispatcher, TriggerAllowlist};

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
async fn gh_cli_paginates_check_runs_reviews_and_repo_listings_across_multiple_pages() {
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
  *"repos/acme/alan/events"*)
    printf 'HTTP/1.1 200 OK\r\n\r\n[]'
    ;;
  *"repos/acme/alan/issues/30/comments"*)
    printf 'HTTP/1.1 200 OK\r\n\r\n[]'
    ;;
  *"repos/acme/alan/issues?"*)
    if [[ "$all_args" == *"page=2"* ]]; then
      printf 'HTTP/1.1 200 OK\r\n\r\n[{"number":31,"title":"Issue 31","user":{"login":"morris"},"state":"open","labels":[],"updated_at":"2026-11-22T01:00:00Z","body":"b","html_url":"https://github.com/acme/alan/issues/31"}]'
    else
      printf 'HTTP/1.1 200 OK\r\nLink: <repos/acme/alan/issues?state=all&per_page=100&page=2>; rel="next"\r\n\r\n[{"number":1,"title":"Issue 1","user":{"login":"morris"},"state":"open","labels":[],"updated_at":"2026-11-22T01:00:00Z","body":"b","html_url":"https://github.com/acme/alan/issues/1"}]'
    fi
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
  *"repos/acme/alan/pulls?"*)
    if [[ "$all_args" == *"page=2"* ]]; then
      printf 'HTTP/1.1 200 OK\r\n\r\n[{"number":32,"title":"PR 32","user":{"login":"morris"},"state":"open","draft":false,"mergeable":true,"head":{"ref":"fix-32","sha":"sha-32"},"base":{"ref":"main"},"updated_at":"2026-11-22T01:00:00Z","html_url":"https://github.com/acme/alan/pull/32"}]'
    else
      printf 'HTTP/1.1 200 OK\r\nLink: <repos/acme/alan/pulls?state=all&per_page=100&page=2>; rel="next"\r\n\r\n[{"number":30,"title":"Multi-page","user":{"login":"morris"},"state":"open","draft":false,"mergeable":true,"head":{"ref":"fix","sha":"sha-multi"},"base":{"ref":"main"},"updated_at":"2026-11-22T01:00:00Z","html_url":"https://github.com/acme/alan/pull/30"}]'
    fi
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

    assert_eq!(
        backend.list_issue_numbers(&slug).await.unwrap(),
        vec![1, 31]
    );
    assert_eq!(
        backend.list_pull_numbers(&slug).await.unwrap(),
        vec![30, 32]
    );
    let SyncRepoOutcome::Updated(synced) = backend.sync_repo(&slug, None).await.unwrap() else {
        panic!("expected Updated");
    };
    let issue_nums: Vec<u64> = synced.issues.iter().map(|i| i.meta.number).collect();
    assert_eq!(issue_nums, vec![1, 31]);
    let pull_nums: Vec<u64> = synced.pulls.iter().map(|p| p.meta.number).collect();
    assert_eq!(pull_nums, vec![30, 32]);
}
