#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use alan::cli::q::{QAction, QArgs, run_with_service};
use alan_agent_engine::tools::{ReifiedMountAccess, SandboxHostMount, ToolRegistry};
use alan_agent_engine::{AgentProcessConfig, LlmClient};
use alan_ap::{ErrorCode, InProcessTransport};
use alan_kernel::{Access, LiveNamespace, MountFs, Namespace, SrvFs};
use alan_llm::MockLlmProvider;
use alan_service_manager::githubfs::{
    CreatedIssue, DispatchOutcome, GITHUB_MOUNT_PATH, GITHUB_SRV_HANDLE, GithubEventDispatcher,
    GithubIssueMeta, GithubStatusSnapshot, HUMAN_REVIEW_PORT, IssueKey, SkipReason,
    TriggerAllowlist, new_routefs_transport,
};
use alan_service_manager::{
    FileserverLaunchGrant, FileserverManager, PackageExportKind, PackageService,
    ProcessLaunchContext, ServiceManager, ServiceManagerConfig,
};
use alan_shell::Shell;

const SECRET_FIXTURE_TOKEN: &str = "ghp_e2e_secret_token_do_not_leak";

const FAKE_GH_SCRIPT: &str = r#"#!/usr/bin/env python3
import json
import os
import sys
import urllib.parse

STATE_PATH = os.path.join(os.environ.get("TMPDIR", "/tmp"), "gh_e2e_state.json")
TOKEN = "ghp_e2e_secret_token_do_not_leak"

def load_state():
    if os.path.exists(STATE_PATH):
        with open(STATE_PATH, "r", encoding="utf-8") as f:
            return json.load(f)
    return {
        "next_issue": 1,
        "next_comment": 100,
        "issues": {},
        "pulls": {
            "2": {
                "number": 2,
                "title": "Existing PR",
                "user": {"login": "morris"},
                "head": {"ref": "fix/ns-walk", "sha": "sha-2"},
                "base": {"ref": "main"},
                "state": "open",
                "mergeable": True,
                "labels": [{"name": "alan:auto"}],
                "updated_at": "2026-11-22T00:00:00Z",
                "html_url": "https://github.com/acme/alan/pull/2"
            }
        },
        "events_emitted": False,
    }

def save_state(state):
    with open(STATE_PATH, "w", encoding="utf-8") as f:
        json.dump(state, f)

def respond(status, headers, body_obj):
    out = [f"HTTP/1.1 {status}"]
    for k, v in headers.items():
        out.append(f"{k}: {v}")
    out.append("")
    out.append(json.dumps(body_obj))
    sys.stdout.write("\r\n".join(out))
    sys.exit(0)

if sys.argv[1:3] == ["auth", "token"]:
    sys.stdout.write(TOKEN + "\n")
    sys.exit(0)

if len(sys.argv) >= 2 and sys.argv[1] == "api":
    method = "GET"
    has_stdin = False
    target = None
    i = 2
    while i < len(sys.argv):
        arg = sys.argv[i]
        if arg == "-i":
            i += 1
        elif arg == "-X":
            method = sys.argv[i + 1]
            i += 2
        elif arg == "-H":
            i += 2
        elif arg == "--input":
            has_stdin = True
            i += 2
        else:
            target = arg
            i += 1
    if not os.environ.get("GH_TOKEN"):
        sys.stderr.write("missing GH_TOKEN\n")
        sys.exit(1)

    parsed = urllib.parse.urlparse(target or "")
    path = parsed.path
    payload = json.loads(sys.stdin.read()) if has_stdin else {}
    state = load_state()

    if path == "repos/acme/alan" and method == "GET":
        respond("200 OK", {}, {"full_name": "acme/alan"})

    if path == "repos/acme/alan/issues" and method == "POST":
        num = state["next_issue"]
        state["next_issue"] += 1
        issue = {
            "number": num,
            "title": payload["title"],
            "body": payload.get("body", ""),
            "user": {"login": "morris"},
            "state": "open",
            "labels": [{"name": l} for l in payload.get("labels", [])],
            "updated_at": "2026-11-22T01:00:00Z",
            "html_url": f"https://github.com/acme/alan/issues/{num}",
        }
        state["issues"][str(num)] = issue
        save_state(state)
        respond("201 Created", {}, issue)

    if path == "repos/acme/alan/issues" and method == "GET":
        respond("200 OK", {}, list(state["issues"].values()))

    if path.startswith("repos/acme/alan/issues/") and path.endswith("/comments") and method == "GET":
        respond("200 OK", {}, [])

    if path.startswith("repos/acme/alan/issues/") and path.endswith("/comments") and method == "POST":
        cid = state["next_comment"]
        state["next_comment"] += 1
        save_state(state)
        respond(
            "201 Created",
            {},
            {
                "id": cid,
                "user": {"login": "morris"},
                "body": payload.get("body", ""),
                "created_at": "2026-11-22T01:05:00Z",
            },
        )

    if path.startswith("repos/acme/alan/issues/") and method == "GET":
        num = path.rsplit("/", 1)[-1]
        issue = state["issues"].get(num)
        if issue is None:
            respond("404 Not Found", {}, {"message": "Not Found"})
        respond("200 OK", {}, issue)

    if path == "repos/acme/alan/pulls" and method == "GET":
        respond("200 OK", {}, list(state["pulls"].values()))

    if path == "repos/acme/alan/pulls/2" and method == "GET":
        respond("200 OK", {}, state["pulls"]["2"])

    if path == "repos/acme/alan/pulls/2/reviews" and method == "GET":
        respond("200 OK", {}, [])

    if path == "repos/acme/alan/commits/sha-2/check-runs" and method == "GET":
        respond("200 OK", {}, {"check_runs": []})

    if path == "graphql" and method == "POST":
        respond(
            "200 OK",
            {},
            {
                "data": {
                    "repository": {
                        "pullRequest": {
                            "reviewThreads": {
                                "pageInfo": {"hasNextPage": False, "endCursor": None},
                                "nodes": [],
                            }
                        }
                    }
                }
            },
        )

    if path == "repos/acme/alan/events" and method == "GET":
        # Newest-first as returned by GitHub REST events API; GhCliGithubBackend reverses.
        sync_round = state.get("sync_round", 0) + 1
        state["sync_round"] = sync_round
        if sync_round == 1:
            events = [
                {
                    "id": "gh-evt-pr-queued-2",
                    "type": "PullRequestEvent",
                    "actor": {"login": "morris"},
                    "created_at": "2026-11-22T02:02:00Z",
                    "payload": {
                        "action": "labeled",
                        "number": 2,
                        "pull_request": {
                            "number": 2,
                            "labels": [{"name": "alan:auto"}],
                            "head": {"sha": "sha-2", "ref": "fix/ns-walk"},
                        },
                    },
                },
                {
                    "id": "gh-evt-coalesce-1b",
                    "type": "IssuesEvent",
                    "actor": {"login": "morris"},
                    "created_at": "2026-11-22T02:01:30Z",
                    "payload": {
                        "action": "labeled",
                        "issue": {
                            "number": 1,
                            "labels": [{"name": "alan:auto"}],
                        },
                    },
                },
                {
                    "id": "gh-evt-allow-1",
                    "type": "IssuesEvent",
                    "actor": {"login": "morris"},
                    "created_at": "2026-11-22T02:01:00Z",
                    "payload": {
                        "action": "labeled",
                        "issue": {
                            "number": 1,
                            "labels": [{"name": "alan:auto"}],
                        },
                    },
                },
                {
                    "id": "gh-evt-untrusted-0",
                    "type": "IssuesEvent",
                    "actor": {"login": "stranger"},
                    "created_at": "2026-11-22T02:00:00Z",
                    "payload": {
                        "action": "opened",
                        "issue": {
                            "number": 99,
                            "labels": [],
                        },
                    },
                },
            ]
            etag = 'W/"e2e-1"'
        else:
            events = [
                {
                    "id": "gh-evt-retry-1",
                    "type": "IssuesEvent",
                    "actor": {"login": "morris"},
                    "created_at": "2026-11-22T02:03:00Z",
                    "payload": {
                        "action": "labeled",
                        "issue": {
                            "number": 1,
                            "labels": [{"name": "alan:auto"}],
                        },
                    },
                }
            ]
            etag = f'W/"e2e-{sync_round}"'
        state["events_emitted"] = True
        save_state(state)
        respond(
            "200 OK",
            {
                "ETag": etag,
                "X-RateLimit-Limit": "5000",
                "X-RateLimit-Remaining": "4999",
            },
            events,
        )

    respond("404 Not Found", {}, {"message": f"unhandled {method} {path}"})

sys.exit(1)
"#;

#[tokio::test]
async fn q_install_github_fileserver_start_mount_issue_write_and_routefs_dispatch_e2e() {
    let temp = tempfile::tempdir().unwrap();
    let pkg_store = temp.path().join("packages");
    let packages = PackageService::open(pkg_store.clone()).unwrap();
    let repo_packages =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../service-manager/packages");

    // 1. Install all three self-iteration Quartermaster packages via `alan q install` (`/bin/q`).
    for pkg_id in [
        "github-fileserver",
        "alan-self-iterate",
        "alan-release-prep",
    ] {
        let out = run_with_service(
            &packages,
            QArgs {
                action: QAction::Install {
                    name: None,
                    path: repo_packages.join(pkg_id),
                },
            },
        )
        .await
        .unwrap();
        assert!(out.starts_with(&format!("installed {pkg_id}")), "{out}");
    }

    let inspect_out = run_with_service(
        &packages,
        QArgs {
            action: QAction::Inspect {
                package_id: "github-fileserver".to_string(),
            },
        },
    )
    .await
    .unwrap();
    assert!(
        inspect_out.contains("export_kind: fileserver"),
        "{inspect_out}"
    );
    assert!(inspect_out.contains("service_id: github"), "{inspect_out}");
    assert!(
        inspect_out.contains("srv_handle: /srv/github"),
        "{inspect_out}"
    );
    assert!(
        inspect_out.contains("mount_path: /mnt/github"),
        "{inspect_out}"
    );

    let record = packages.resolve("github-fileserver").unwrap();
    assert_eq!(record.export_kind, PackageExportKind::Fileserver);

    // 2. Zero install-time authority: booting ServiceManager does NOT auto-start or publish /srv/github.
    let mut sm_cfg = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    sm_cfg.package_store = Some(pkg_store.clone());
    let sm = ServiceManager::boot(sm_cfg).await.unwrap();
    let sm_shell = Shell::new(InProcessTransport::new(
        sm.local_entry().namespace_for_local_client(),
    ));
    let srv_entries = sm_shell.ls("/srv").await.unwrap();
    assert!(
        !srv_entries.contains(&GITHUB_SRV_HANDLE.to_string()),
        "q install must not auto-publish /srv/github: {srv_entries:?}"
    );
    assert_eq!(
        sm_shell.ls(GITHUB_MOUNT_PATH).await,
        Err(ErrorCode::NotFound)
    );
    sm.shutdown().await.unwrap();

    // 3. Explicit confined start via FileserverManager exercising `bin/githubfs` -> `alan __fileserver-github`.
    let fixture_dir = temp.path().join("gh-fixture");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    let fake_gh_path = fixture_dir.join("gh");
    std::fs::write(&fake_gh_path, FAKE_GH_SCRIPT).unwrap();
    std::fs::set_permissions(&fake_gh_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let srvfs = Arc::new(SrvFs::new());
    let fs_manager = FileserverManager::new(packages.clone(), srvfs.clone());

    let mut runtime_env = BTreeMap::new();
    runtime_env.insert(
        "ALAN_BIN".to_string(),
        env!("CARGO_BIN_EXE_alan").to_string(),
    );
    runtime_env.insert(
        "ALAN_GITHUBFS_GH_BIN".to_string(),
        fake_gh_path.display().to_string(),
    );
    let grant = FileserverLaunchGrant::new("github-fileserver")
        .with_mount_access(Access::ReadWrite)
        .with_allow_network(false)
        .with_host_mounts(vec![SandboxHostMount {
            namespace_path: PathBuf::from("/opt/gh-fixture"),
            host_path: fixture_dir.clone(),
            access: ReifiedMountAccess::ReadOnly,
        }])
        .with_runtime_env(runtime_env);

    let status = fs_manager.start(grant).await.unwrap();
    assert!(status.running);
    assert_eq!(status.service_id, GITHUB_SRV_HANDLE);
    assert_eq!(status.srv_path, "/srv/github");
    assert_eq!(status.mount_path, GITHUB_MOUNT_PATH);
    assert!(srvfs.lookup(GITHUB_SRV_HANDLE).await.is_some());

    // 4. Mount ReadOnly vs ReadWrite namespaces via FileserverManager.
    let ro_live = LiveNamespace::new(Namespace::new());
    fs_manager
        .mount_into_live_namespace(GITHUB_SRV_HANDLE, &ro_live, Access::ReadOnly)
        .await
        .unwrap();
    let ro_shell = Shell::new(InProcessTransport::new(Arc::new(
        MountFs::from_live_namespace(ro_live),
    )));
    assert!(!ro_shell.stat("/mnt/github/ctl").await.unwrap().writable);
    assert_eq!(
        ro_shell.write("/mnt/github/ctl", b"watch acme/alan").await,
        Err(ErrorCode::NoAccess)
    );
    assert!(ro_shell.cat("/mnt/github/status").await.is_ok());

    let route_transport = new_routefs_transport();
    srvfs
        .post("route", route_transport.clone(), Access::ReadWrite)
        .await;
    let rw_live = LiveNamespace::new(Namespace::new());
    fs_manager
        .mount_into_live_namespace(GITHUB_SRV_HANDLE, &rw_live, Access::ReadWrite)
        .await
        .unwrap();
    rw_live.replace_mount("/mnt/route", route_transport, Access::ReadWrite);
    let rw_transport = InProcessTransport::new(Arc::new(MountFs::from_live_namespace(rw_live)));
    let rw_shell = Shell::new(rw_transport.clone());

    // 5. Watch `acme/alan`, create issue via `/mnt/github/repos/acme/alan/issues/new`, and verify receipt.
    rw_shell
        .write("/mnt/github/ctl", b"watch acme/alan")
        .await
        .unwrap();
    let initial_status: GithubStatusSnapshot =
        serde_json::from_slice(&rw_shell.cat("/mnt/github/status").await.unwrap()).unwrap();
    assert_eq!(initial_status.watched_repos, vec!["acme/alan"]);

    let issue_payload = serde_json::json!({
        "title": "Self-iteration E2E issue",
        "body": "Redacted diagnostics only",
        "labels": ["alan:auto"]
    });
    rw_shell
        .write(
            "/mnt/github/repos/acme/alan/issues/new",
            &serde_json::to_vec(&issue_payload).unwrap(),
        )
        .await
        .unwrap();
    let receipt: CreatedIssue = serde_json::from_slice(
        &rw_shell
            .cat("/mnt/github/repos/acme/alan/issues/new")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(receipt.number, 1);
    assert_eq!(receipt.url, "https://github.com/acme/alan/issues/1");

    let created_meta: GithubIssueMeta = serde_json::from_slice(
        &rw_shell
            .cat("/mnt/github/repos/acme/alan/issues/1/meta.json")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(created_meta.title, "Self-iteration E2E issue");
    let created_body = String::from_utf8(
        rw_shell
            .cat("/mnt/github/repos/acme/alan/issues/1/body.md")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(created_body, "Redacted diagnostics only");

    // Destructive PR merge verb fails closed while comment succeeds.
    assert_eq!(
        rw_shell
            .write("/mnt/github/repos/acme/alan/pulls/2/ctl", b"merge")
            .await,
        Err(ErrorCode::BadRequest)
    );
    rw_shell
        .write(
            "/mnt/github/repos/acme/alan/pulls/2/ctl",
            b"comment Verified without merge rights",
        )
        .await
        .unwrap();

    // 6. Sync events and dispatch `/mnt/github/events` -> `/mnt/route` with loop guards.
    let allowlist = TriggerAllowlist {
        allowed_repos: vec!["acme/alan".to_string()],
        allowed_actors: vec!["morris".to_string()],
        required_labels: vec!["alan:auto".to_string()],
        ignored_actors: vec!["alan-bot".to_string()],
        ignored_branch_prefixes: vec!["alan/".to_string()],
        max_concurrent: 1,
        max_retries: 1,
    };
    let mut dispatcher = GithubEventDispatcher::new(rw_transport, allowlist).unwrap();
    dispatcher.ensure_route_rules().await.unwrap();

    rw_shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    let events_ndjson =
        String::from_utf8(rw_shell.cat("/mnt/github/events").await.unwrap()).unwrap();
    assert!(events_ndjson.contains("gh-evt-allow-1"), "{events_ndjson}");
    assert!(
        !events_ndjson.contains(SECRET_FIXTURE_TOKEN),
        "token leaked in events: {events_ndjson}"
    );

    let first_round = dispatcher.process_available_events().await.unwrap();
    assert_eq!(first_round.len(), 4, "{first_round:?}");
    assert!(matches!(
        first_round[0],
        DispatchOutcome::Skipped {
            reason: SkipReason::ActorNotAllowed,
            ..
        }
    ));
    assert!(matches!(
        first_round[1],
        DispatchOutcome::DispatchedWorker { attempt: 1, .. }
    ));
    assert!(matches!(
        first_round[2],
        DispatchOutcome::Skipped {
            reason: SkipReason::CoalescedActiveRun,
            ..
        }
    ));
    assert!(matches!(
        first_round[3],
        DispatchOutcome::QueuedConcurrencyCap { .. }
    ));

    let worker_port =
        String::from_utf8(rw_shell.cat("/mnt/route/ports/worker").await.unwrap()).unwrap();
    assert!(worker_port.contains("\"number\":1"), "{worker_port}");
    assert!(!worker_port.contains(SECRET_FIXTURE_TOKEN));

    // Completing issue #1 drains queued PR #2 into the worker slot, and a second sync retry
    // on issue #1 exceeds `max_retries = 1` and escalates to `/mnt/route/ports/human-review`.
    let drained = dispatcher
        .complete_run(&IssueKey::new("acme/alan", 1).into_coalesce())
        .await
        .unwrap();
    assert_eq!(drained.len(), 1);
    assert!(matches!(
        drained[0],
        DispatchOutcome::DispatchedWorker { attempt: 1, .. }
    ));
    assert!(
        dispatcher
            .complete_run(&alan_service_manager::githubfs::CoalesceKey::new(
                "acme/alan",
                2,
                Some("sha-2"),
            ))
            .await
            .unwrap()
            .is_empty()
    );

    rw_shell.write("/mnt/github/ctl", b"sync").await.unwrap();
    let second_round = dispatcher.process_available_events().await.unwrap();
    assert_eq!(second_round.len(), 1, "{second_round:?}");
    assert_eq!(
        second_round[0],
        DispatchOutcome::EscalatedHumanReview {
            event_id: "gh-evt-retry-1".to_string(),
            issue_key: IssueKey::new("acme/alan", 1),
            attempts: 1,
            reason: "retry cap exhausted (1/1)".to_string(),
        }
    );
    let human_review = String::from_utf8(
        rw_shell
            .cat(&format!("/mnt/route/ports/{HUMAN_REVIEW_PORT}"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(human_review.contains("\"number\":1"), "{human_review}");

    // Confirm fake Host token never leaked into status or bounded stderr logs.
    let final_status =
        String::from_utf8(rw_shell.cat("/mnt/github/status").await.unwrap()).unwrap();
    assert!(!final_status.contains(SECRET_FIXTURE_TOKEN));
    let stderr = fs_manager
        .recent_stderr(GITHUB_SRV_HANDLE)
        .await
        .unwrap_or_default();
    assert!(!stderr.contains(SECRET_FIXTURE_TOKEN));

    // 7. Stop cleanly and verify /srv/github unposted and /mnt/github removed.
    fs_manager.stop_all().await.unwrap();
    assert!(srvfs.lookup(GITHUB_SRV_HANDLE).await.is_none());
    assert_eq!(
        rw_shell.cat("/mnt/github/status").await,
        Err(ErrorCode::NotFound)
    );
}

trait IssueKeyCoalesceExt {
    fn into_coalesce(self) -> alan_service_manager::githubfs::CoalesceKey;
}

impl IssueKeyCoalesceExt for IssueKey {
    fn into_coalesce(self) -> alan_service_manager::githubfs::CoalesceKey {
        alan_service_manager::githubfs::CoalesceKey::new(&self.repo, self.number, None)
    }
}
