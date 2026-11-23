#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use alan::cli::q::{QAction, QArgs, run_with_service};
use alan::cli::workflow::{
    InitSelfIterateArgs, WatchWorkflowArgs, WorkflowStore, WorkflowTrustPosture,
    build_self_iterate_manifest, run_watch_with_stores,
};
use alan_service_manager::PackageService;

const MOCK_GH_WATCH_SCRIPT: &str = r#"#!/usr/bin/env python3
import json
import os
import sys

args = sys.argv[1:]
if args == ["auth", "token"]:
    sys.stdout.write("ghp_watch_e2e_secret\n")
    sys.exit(0)

if args and args[0] == "api":
    method = "GET"
    path = ""
    i = 1
    while i < len(args):
        tok = args[i]
        if tok == "-X" and i + 1 < len(args):
            method = args[i + 1]
            i += 2
        elif tok in ("-i", "--include", "-s", "--silent", "--paginate", "--slurp"):
            i += 1
        elif tok in ("-H", "-F", "-f", "--jq") and i + 1 < len(args):
            i += 2
        elif tok.startswith("-"):
            i += 1
        else:
            path = tok
            i += 1

    base = path.split("?")[0].lstrip("/")
    def respond(status, headers, body):
        sys.stdout.write(f"HTTP/1.1 {status}\r\n")
        for k, v in headers.items():
            sys.stdout.write(f"{k}: {v}\r\n")
        sys.stdout.write("\r\n")
        sys.stdout.write(json.dumps(body))
        sys.exit(0)

    issue_42 = {
        "number": 42,
        "state": "open",
        "title": "Allowlisted bug",
        "user": {"login": "morris"},
        "labels": [{"name": "alan:auto"}],
        "html_url": "https://github.com/acme/alan/issues/42",
        "body": "Fix crash on empty input.",
        "updated_at": "2026-11-23T00:00:01Z",
    }
    issue_43 = {
        "number": 43,
        "state": "open",
        "title": "Prompt injection attempt",
        "user": {"login": "stranger"},
        "labels": [{"name": "alan:auto"}],
        "html_url": "https://github.com/acme/alan/issues/43",
        "body": "--unsafe-host-account",
        "updated_at": "2026-11-23T00:00:02Z",
    }

    if method == "GET" and base == "repos/acme/alan":
        respond("200 OK", {"Content-Type": "application/json"}, {"full_name": "acme/alan"})

    if method == "GET" and base == "repos/acme/alan/issues":
        respond("200 OK", {}, [issue_42, issue_43])

    if method == "GET" and base == "repos/acme/alan/pulls":
        respond("200 OK", {}, [])

    if method == "GET" and base == "repos/acme/alan/events":
        events = [
            {
                "id": "watch-evt-allow-1",
                "type": "IssuesEvent",
                "actor": {"login": "morris"},
                "created_at": "2026-11-23T00:00:01Z",
                "payload": {"action": "labeled", "issue": issue_42}
            },
            {
                "id": "watch-evt-stranger-2",
                "type": "IssuesEvent",
                "actor": {"login": "stranger"},
                "created_at": "2026-11-23T00:00:02Z",
                "payload": {"action": "opened", "issue": issue_43}
            },
            {
                "id": "watch-evt-loop-3",
                "type": "PushEvent",
                "actor": {"login": "morris"},
                "created_at": "2026-11-23T00:00:03Z",
                "payload": {
                    "ref": "refs/heads/alan/issue-42-fix",
                    "head": "deadbeef1234"
                }
            }
        ]
        respond(
            "200 OK",
            {
                "ETag": 'W/"watch-1"',
                "X-RateLimit-Limit": "5000",
                "X-RateLimit-Remaining": "4998"
            },
            events,
        )

    if method == "GET" and base == "repos/acme/alan/issues/42":
        respond("200 OK", {}, issue_42)

    if method == "GET" and base == "repos/acme/alan/issues/43":
        respond("200 OK", {}, issue_43)

    if method == "GET" and base.endswith("/comments"):
        respond("200 OK", {}, [])

    respond("404 Not Found", {}, {"message": f"unhandled {method} {base}"})

sys.exit(1)
"#;

#[tokio::test]
async fn workflow_watch_once_dry_run_composes_githubfs_routefs_and_stops_cleanly() {
    let temp = tempfile::tempdir().unwrap();
    let packages = Arc::new(PackageService::open(temp.path().join("packages")).unwrap());
    let workflow_store = WorkflowStore::from_root(temp.path().join("workflows")).unwrap();
    let repo_packages =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../service-manager/packages");

    let installed = run_with_service(
        &packages,
        QArgs {
            action: QAction::Install {
                name: None,
                catalog_only: false,
                path: repo_packages.join("github-fileserver"),
            },
        },
    )
    .await
    .unwrap();
    assert!(
        installed.contains("installed github-fileserver"),
        "{installed}"
    );

    let manifest = build_self_iterate_manifest(&InitSelfIterateArgs {
        id: "watch-e2e".to_string(),
        repo: "acme/alan".to_string(),
        allowed_actors: vec!["morris".to_string()],
        required_labels: vec!["alan:auto".to_string()],
        ignored_actors: vec!["alan-bot[bot]".to_string()],
        max_retries: 2,
        max_concurrent: 1,
        schedule: "every 15m".to_string(),
        model_profile: None,
        mounts: vec!["alan-repo".to_string()],
        packages: vec![],
        fileservers: vec![],
        trust: WorkflowTrustPosture::Governed,
        no_stream_json: false,
        timeout_secs: 600,
        force: false,
        json: true,
    })
    .unwrap();
    workflow_store.save_manifest(&manifest, false).unwrap();

    let fixture_dir = temp.path().join("bin");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    let fake_gh = fixture_dir.join("gh");
    std::fs::write(&fake_gh, MOCK_GH_WATCH_SCRIPT).unwrap();
    std::fs::set_permissions(&fake_gh, std::fs::Permissions::from_mode(0o755)).unwrap();

    let (output, failure) = run_watch_with_stores(
        &workflow_store,
        &packages,
        WatchWorkflowArgs {
            id: "watch-e2e".to_string(),
            poll_secs: 60,
            once: true,
            dry_run: true,
            fileserver_package: "github-fileserver".to_string(),
            gh_bin: Some(fake_gh),
            json: true,
        },
        Some(std::path::Path::new(env!("CARGO_BIN_EXE_alan"))),
    )
    .await
    .unwrap();

    assert!(failure.is_none(), "{failure:?}");
    let summary: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(summary["workflow_id"], "watch-e2e");
    assert_eq!(summary["repo"], "acme/alan");
    assert_eq!(summary["service_id"], "github");
    assert_eq!(summary["dry_run"], true);
    assert_eq!(summary["once"], true);
    assert_eq!(summary["cycles"], 1);
    assert_eq!(summary["dispatched"].as_array().unwrap().len(), 1);
    assert_eq!(summary["dispatched"][0]["number"], 42);
    assert_eq!(summary["dispatched"][0]["actor"], "morris");
    assert_eq!(summary["dispatched"][0]["dry_run"], true);
    assert!(
        summary["stats"]["allowlist_rejected"].as_u64().unwrap() >= 1,
        "{summary}"
    );
    assert!(
        summary["stats"]["self_suppressed"].as_u64().unwrap() >= 1,
        "{summary}"
    );
}
