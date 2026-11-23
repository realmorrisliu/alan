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

const MOCK_GH_TWO_ELIGIBLE_SCRIPT: &str = r#"#!/usr/bin/env python3
import json
import sys

args = sys.argv[1:]
if args == ["auth", "token"]:
    sys.stdout.write("ghp_watch_sigterm_secret\n")
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
        "title": "First queued issue",
        "user": {"login": "morris"},
        "labels": [{"name": "alan:auto"}],
        "html_url": "https://github.com/acme/alan/issues/42",
        "body": "First eligible work item.",
        "updated_at": "2026-11-23T00:00:01Z",
    }
    issue_43 = {
        "number": 43,
        "state": "open",
        "title": "Second queued issue",
        "user": {"login": "morris"},
        "labels": [{"name": "alan:auto"}],
        "html_url": "https://github.com/acme/alan/issues/43",
        "body": "Must never be spawned after SIGTERM.",
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
                "id": "watch-sigterm-evt-1",
                "type": "IssuesEvent",
                "actor": {"login": "morris"},
                "created_at": "2026-11-23T00:00:01Z",
                "payload": {"action": "labeled", "issue": issue_42}
            },
            {
                "id": "watch-sigterm-evt-2",
                "type": "IssuesEvent",
                "actor": {"login": "morris"},
                "created_at": "2026-11-23T00:00:02Z",
                "payload": {"action": "labeled", "issue": issue_43}
            }
        ]
        respond(
            "200 OK",
            {
                "ETag": 'W/"watch-sigterm-1"',
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
async fn workflow_watch_sigterm_cancels_active_worker_reaps_pid_and_stops_cleanly() {
    let temp = tempfile::tempdir().unwrap();
    let home_dir = temp.path().join("home");
    let data_home = home_dir.join(".local/share");
    let config_home = home_dir.join(".config");
    std::fs::create_dir_all(&data_home).unwrap();
    std::fs::create_dir_all(&config_home).unwrap();

    let alan_bin = PathBuf::from(env!("CARGO_BIN_EXE_alan"));
    let repo_packages =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../service-manager/packages");

    let run_alan_cli = |args: &[&str]| {
        let mut cmd = std::process::Command::new(&alan_bin);
        cmd.args(args)
            .env_remove("ALAN_INSTALL_CHANNEL")
            .env("HOME", &home_dir)
            .env("XDG_DATA_HOME", &data_home)
            .env("XDG_CONFIG_HOME", &config_home);
        cmd.output().unwrap()
    };

    let pkg_arg = repo_packages.join("github-fileserver");
    let install_out = run_alan_cli(&["q", "install", pkg_arg.to_str().unwrap()]);
    assert!(
        install_out.status.success(),
        "q install failed: {}",
        String::from_utf8_lossy(&install_out.stderr)
    );

    let init_out = run_alan_cli(&[
        "workflow",
        "init-self-iterate",
        "watch-sigterm",
        "--repo",
        "acme/alan",
        "--allowed-actor",
        "morris",
        "--required-label",
        "alan:auto",
        "--grant",
        "alan-repo",
        "--json",
    ]);
    assert!(
        init_out.status.success(),
        "init-self-iterate failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );

    let fixture_dir = temp.path().join("bin");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    let fake_gh = fixture_dir.join("gh");
    std::fs::write(&fake_gh, MOCK_GH_TWO_ELIGIBLE_SCRIPT).unwrap();
    std::fs::set_permissions(&fake_gh, std::fs::Permissions::from_mode(0o755)).unwrap();

    let spawn_count_file = temp.path().join("spawn_count.txt");
    let worker_pid_file = temp.path().join("worker.pid");
    let mock_worker = fixture_dir.join("mock-worker.sh");
    let mock_worker_body = format!(
        "#!/bin/sh\nset -eu\nprintf 'spawned\\n' >> \"{}\"\nsleep 60 &\nsleep_pid=$!\nprintf '%s\\n' \"$$\" > \"{}.tmp\"\nmv \"{}.tmp\" \"{}\"\nwait \"$sleep_pid\"\n",
        spawn_count_file.display(),
        worker_pid_file.display(),
        worker_pid_file.display(),
        worker_pid_file.display(),
    );
    std::fs::write(&mock_worker, mock_worker_body).unwrap();
    std::fs::set_permissions(&mock_worker, std::fs::Permissions::from_mode(0o755)).unwrap();

    let watcher = tokio::process::Command::new(&alan_bin)
        .args([
            "workflow",
            "watch",
            "watch-sigterm",
            "--poll-secs",
            "60",
            "--json",
            "--gh-bin",
            fake_gh.to_str().unwrap(),
        ])
        .env_remove("ALAN_INSTALL_CHANNEL")
        .env("HOME", &home_dir)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("ALAN_WORKFLOW_ALAN_BIN", &mock_worker)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let watcher_pid = libc::pid_t::try_from(watcher.id().unwrap()).unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    let worker_pid = loop {
        if let Ok(raw) = std::fs::read_to_string(&worker_pid_file)
            && let Some(line) = raw.lines().next()
            && let Ok(pid) = line.trim().parse::<libc::pid_t>()
            && pid > 0
        {
            break pid;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for first dispatched worker PID"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };

    // SAFETY: `watcher_pid` is the live spawned `alan workflow watch` child not yet awaited.
    let rc = unsafe { libc::kill(watcher_pid, libc::SIGTERM) };
    assert_eq!(rc, 0, "failed to deliver SIGTERM to watcher {watcher_pid}");

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        watcher.wait_with_output(),
    )
    .await
    .expect("watcher must shut down promptly after SIGTERM")
    .unwrap();
    assert!(
        output.status.success(),
        "watcher exited unsuccessfully after SIGTERM: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let reap_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        // SAFETY: Querying process existence with signal 0 checks worker termination without signaling.
        let check = unsafe { libc::kill(worker_pid, 0) };
        if check == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < reap_deadline,
            "active worker PID {worker_pid} survived watcher SIGTERM"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let spawn_lines = std::fs::read_to_string(&spawn_count_file)
        .unwrap()
        .lines()
        .count();
    assert_eq!(
        spawn_lines, 1,
        "second queued issue must never spawn after SIGTERM"
    );

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["workflow_id"], "watch-sigterm");
    assert_eq!(summary["dispatched"].as_array().unwrap().len(), 1);
    assert_eq!(summary["dispatched"][0]["status"], "cancelled");
    assert_eq!(summary["dispatched"][0]["exit_code"], 130);
}
