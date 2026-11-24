use alan_ap::{ErrorCode, FileKind, Offset, OpenMode, Qid, Stat};
use async_trait::async_trait;

use super::*;

struct MockTarget(StdMutex<Vec<usize>>);

fn qid(kind: FileKind, path: u64) -> Qid {
    Qid {
        kind,
        version: 0,
        path,
    }
}

#[async_trait]
impl FileServer for MockTarget {
    async fn walk(&self, _: Fid, nf: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let k = match names.first().map(String::as_str) {
            Some("events") => FileKind::Stream,
            Some(_) => FileKind::File,
            None => FileKind::Dir,
        };
        Ok(qid(k, nf.0))
    }
    async fn open(&self, fid: Fid, _: OpenMode) -> Result<Qid, ErrorCode> {
        let k = if fid.0 == 2 {
            FileKind::Stream
        } else {
            FileKind::File
        };
        Ok(qid(k, fid.0))
    }
    async fn read(&self, fid: Fid, _: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if fid.0 == 2 || fid.0 == 3 {
            tokio::time::sleep(Duration::from_millis(85)).await;
            return Ok(if fid.0 == 2 { b"ev".to_vec() } else { vec![] });
        }
        Ok(vec![b'x'; count as usize])
    }
    async fn write(&self, _: Fid, _: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.0.lock().unwrap().push(data.len());
        Ok(data.len() as u32)
    }
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: "m".into(),
            qid: qid(FileKind::File, fid.0),
            length: 0,
            executable: false,
            writable: false,
        })
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn clunk(&self, _: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}

#[test]
fn runtime_env_and_seatbelt_rules_validate_boundaries() {
    for &bad in BAD_ENV {
        assert!(validate_runtime_env(&BTreeMap::from([(bad.into(), "x".into())])).is_err());
    }
    assert!(validate_runtime_env(&BTreeMap::from([("T".into(), "a\0b".into())])).is_err());
    assert!(validate_runtime_env(&BTreeMap::from([("T".into(), "ok".into())])).is_ok());

    let d = tempfile::tempdir().unwrap();
    let rw_dir = d.path().join("rw");
    std::fs::create_dir(&rw_dir).unwrap();
    let ro = host_mount("/ro", d.path(), ReifiedMountAccess::ReadOnly);
    let rw = host_mount("/rw", &rw_dir, ReifiedMountAccess::ReadWrite);
    let rules = seatbelt_read_only_rules(&SandboxSpec::from_host_mounts(&[ro, rw]));
    assert!(rules.contains("deny file-write*") && rules.contains("require-not (subpath"));
}

#[tokio::test]
async fn bounded_line_reader_and_supervised_proxy_enforce_limits() {
    let mut ok_r = BoundedLineReader::new(BufReader::new(&b"a\nb\n"[..]), 8);
    assert_eq!(ok_r.read_line(&mut String::new()).await.unwrap(), 2);
    let bad = format!("{}\n", "a".repeat(200));
    let mut err_r = BoundedLineReader::new(BufReader::new(bad.as_bytes()), 128);
    let err = err_r.read_line(&mut String::new()).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

    let mock = Arc::new(MockTarget(StdMutex::new(Vec::new())));
    let p = SupervisedFileserver::new(mock.clone(), Duration::from_millis(50), 1024);
    let ev_q = p.walk(Fid::ROOT, Fid(2), &["events".into()]).await.unwrap();
    let sl_q = p.walk(Fid::ROOT, Fid(3), &["slow".into()]).await.unwrap();
    assert_eq!((ev_q.kind, sl_q.kind), (FileKind::Stream, FileKind::File));
    assert_eq!(p.read(Fid(2), 0, 64).await.unwrap(), b"ev");
    assert_eq!(p.read(Fid(3), 0, 64).await, Err(ErrorCode::Io));
    assert_eq!(p.write(Fid(1), 0, &[b'a'; 500]).await.unwrap(), 500);
    assert!(*mock.0.lock().unwrap().iter().max().unwrap() <= p.max_payload_chunk);
    let n = p.read(Fid(1), 0, 500).await.unwrap().len();
    assert_eq!(n, p.max_payload_chunk);
}

#[tokio::test]
async fn concurrent_start_reserves_service_id_and_preserves_winner_srv_handle() {
    use crate::{CLI_IMPORT_MOUNT_PATH, run_q_command_with_package_service};
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let pkg_dir = tmp.path().join("pkg");
    std::fs::create_dir_all(pkg_dir.join("bin")).unwrap();
    let manifest = "\
schema_version = 1
kind = \"fileserver\"

[fileserver]
id = \"racefs\"
entrypoint = \"bin/server.py\"

[fileserver.transport]
type = \"stdio\"
command = \"bin/server.py\"
";
    let script = r#"#!/usr/bin/env python3
import json, sys, time
time.sleep(0.05)
for line in sys.stdin:
    if not line.strip():
        continue
    frame = json.loads(line)
    req = frame["request"]
    op = req.get("op")
    if op == "stat":
        resp = {
            "status": "ok",
            "tag": frame["tag"],
            "response": {
                "op": "stat",
                "stat": {
                    "name": "",
                    "qid": {"kind": "dir", "version": 0, "path": 1},
                    "length": 0,
                    "executable": False,
                    "writable": False,
                },
            },
        }
    elif op == "clunk":
        resp = {"status": "ok", "tag": frame["tag"], "response": {"op": "clunk"}}
    else:
        resp = {"status": "error", "tag": frame["tag"], "code": "unsupported"}
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#;
    std::fs::write(pkg_dir.join("alan-package.toml"), manifest).unwrap();
    std::fs::write(pkg_dir.join("bin/server.py"), script).unwrap();
    std::fs::set_permissions(
        pkg_dir.join("bin/server.py"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    let packages = PackageService::open(tmp.path().join("store")).unwrap();
    let installed = run_q_command_with_package_service(
        &packages,
        Some(pkg_dir.as_path()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "race-pack".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(installed.exit_code, 0);

    let srvfs = Arc::new(SrvFs::new());
    let mgr = Arc::new(FileserverManager::new(packages, srvfs.clone()));
    let g1 = FileserverLaunchGrant::new("race-pack");
    let g2 = FileserverLaunchGrant::new("race-pack");
    let (r1, r2) = tokio::join!(mgr.start(g1), mgr.start(g2));

    let successes = [r1.is_ok(), r2.is_ok()]
        .into_iter()
        .filter(|ok| *ok)
        .count();
    assert_eq!(successes, 1, "exactly one concurrent start must win");
    assert!(
        srvfs.lookup("racefs").await.is_some(),
        "losing concurrent start must never unpost winner's /srv/racefs"
    );
    let mut ns = Namespace::new();
    mgr.mount_into_namespace("racefs", &mut ns, Access::ReadOnly)
        .await
        .unwrap();
    assert_eq!(mgr.list().await.len(), 1);
    mgr.stop_all().await.unwrap();
    assert!(srvfs.lookup("racefs").await.is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn unix_socket_startup_failure_kills_helper_process_group_and_aborts_tasks() {
    use crate::{CLI_IMPORT_MOUNT_PATH, run_q_command_with_package_service};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    struct AbortObserver(Arc<AtomicBool>);
    impl Drop for AbortObserver {
        fn drop(&mut self) {
            self.0.store(true, AtomicOrdering::SeqCst);
        }
    }

    // 1. Direct guard test: dropping an early FileserverProcessGuard aborts registered tasks.
    let task_aborted = Arc::new(AtomicBool::new(false));
    let observer = AbortObserver(Arc::clone(&task_aborted));
    let mut dummy_cmd = Command::new("/bin/sh");
    dummy_cmd.args(["-c", "sleep 30"]).process_group(0);
    let dummy_child = dummy_cmd.spawn().unwrap();
    let early_guard = FileserverProcessGuard::new(dummy_child);
    early_guard
        .push_task(tokio::spawn(async move {
            let _keep = observer;
            std::future::pending::<()>().await;
        }))
        .await;
    drop(early_guard);
    for _ in 0..40 {
        if task_aborted.load(AtomicOrdering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        task_aborted.load(AtomicOrdering::SeqCst),
        "dropping FileserverProcessGuard must abort registered drain tasks"
    );

    // 2. End-to-end launch failures (timeout + early exit) must kill spawned helper process groups.
    let tmp = tempfile::tempdir().unwrap();
    let pid_dir = fs_canonical_or_owned(&tmp.path().join("pids"));
    std::fs::create_dir_all(&pid_dir).unwrap();
    let pkg_dir = tmp.path().join("pkg");
    std::fs::create_dir_all(pkg_dir.join("bin")).unwrap();
    let manifest = "\
schema_version = 1
kind = \"fileserver\"

[fileserver]
id = \"fail-sock\"
entrypoint = \"bin/server.py\"
timeout_secs = 1

[fileserver.transport]
type = \"unix_socket\"
socket_name = \"fail.sock\"
";
    let script = r#"#!/usr/bin/env python3
import os, subprocess, sys, time

helper = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
pid_file = os.environ["HELPER_PID_FILE"]
tmp_file = pid_file + ".tmp"
with open(tmp_file, "w", encoding="utf-8") as f:
    f.write(str(helper.pid))
    f.flush()
    os.fsync(f.fileno())
os.replace(tmp_file, pid_file)

if os.environ.get("FAIL_MODE") == "exit":
    sys.exit(1)
time.sleep(30)
"#;
    std::fs::write(pkg_dir.join("alan-package.toml"), manifest).unwrap();
    std::fs::write(pkg_dir.join("bin/server.py"), script).unwrap();
    std::fs::set_permissions(
        pkg_dir.join("bin/server.py"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    let packages = PackageService::open(tmp.path().join("store")).unwrap();
    let installed = run_q_command_with_package_service(
        &packages,
        Some(pkg_dir.as_path()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "fail-sock-pack".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(installed.exit_code, 0);

    let srvfs = Arc::new(SrvFs::new());
    let mgr = FileserverManager::new(packages, srvfs.clone());

    for (mode, filename) in [
        ("timeout", "helper_timeout.pid"),
        ("exit", "helper_exit.pid"),
    ] {
        let pid_file = pid_dir.join(filename);
        let mut env = BTreeMap::new();
        env.insert("FAIL_MODE".to_string(), mode.to_string());
        env.insert(
            "HELPER_PID_FILE".to_string(),
            pid_file.display().to_string(),
        );
        let grant = FileserverLaunchGrant::new("fail-sock-pack")
            .with_host_mounts(vec![host_mount(
                "/mnt/pids",
                &pid_dir,
                ReifiedMountAccess::ReadWrite,
            )])
            .with_runtime_env(env);
        assert!(
            mgr.start(grant).await.is_err(),
            "mode `{mode}` should fail startup"
        );

        let helper_pid: i32 = std::fs::read_to_string(&pid_file)
            .expect("helper pid file written before failure")
            .trim()
            .parse()
            .expect("valid numeric helper pid");
        assert!(helper_pid > 0, "expected positive helper pid");
        let mut reaped = false;
        for _ in 0..40 {
            // SAFETY: `helper_pid` is a validated positive PID; signal `0` probes process liveness without delivering a signal.
            let rc = unsafe { libc::kill(helper_pid, 0) };
            let err = std::io::Error::last_os_error().raw_os_error();
            if rc == -1 && err == Some(libc::ESRCH) {
                reaped = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !reaped {
            // SAFETY: `helper_pid` is a validated positive PID leaked by the failed test launch.
            unsafe { libc::kill(helper_pid, libc::SIGKILL) };
            panic!("helper pid {helper_pid} survived failed `{mode}` unix_socket startup");
        }
        assert!(mgr.list().await.is_empty());
        assert!(srvfs.lookup("fail-sock").await.is_none());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn github_fileserver_injects_host_gh_token_and_falls_back_gracefully() {
    use crate::{CLI_IMPORT_MOUNT_PATH, run_q_command_with_package_service};
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let pkg_dir = tmp.path().join("pkg");
    std::fs::create_dir_all(pkg_dir.join("bin")).unwrap();
    let manifest = "\
schema_version = 1
kind = \"fileserver\"

[fileserver]
id = \"github\"
entrypoint = \"bin/server.py\"
allow_network = true

[fileserver.transport]
type = \"stdio\"
command = \"bin/server.py\"
";
    let server_script = r#"#!/usr/bin/env python3
import json, os, sys

PAYLOAD = json.dumps({
    "gh_token": os.environ.get("GH_TOKEN"),
    "github_token": os.environ.get("GITHUB_TOKEN"),
}).encode("utf-8")

def qid(kind, path):
    return {"kind": kind, "version": 0, "path": path}

fids = {0: "root"}

for line in sys.stdin:
    if not line.strip():
        continue
    frame = json.loads(line)
    req = frame["request"]
    op = req.get("op")
    if op == "stat":
        is_env = fids.get(req["fid"]) == "env"
        resp = {
            "status": "ok",
            "tag": frame["tag"],
            "response": {
                "op": "stat",
                "stat": {
                    "name": "env" if is_env else "",
                    "qid": qid("file" if is_env else "dir", 2 if is_env else 1),
                    "length": len(PAYLOAD) if is_env else 0,
                    "executable": False,
                    "writable": False,
                },
            },
        }
    elif op == "walk":
        names = req.get("names", [])
        base = fids.get(req["fid"], "root")
        if names == ["env"] and base == "root":
            fids[req["newfid"]] = "env"
            resp = {"status": "ok", "tag": frame["tag"], "response": {"op": "walk", "qid": qid("file", 2)}}
        elif not names:
            fids[req["newfid"]] = base
            is_env = base == "env"
            resp = {"status": "ok", "tag": frame["tag"], "response": {"op": "walk", "qid": qid("file" if is_env else "dir", 2 if is_env else 1)}}
        else:
            resp = {"status": "error", "tag": frame["tag"], "code": "not_found"}
    elif op == "open":
        is_env = fids.get(req["fid"]) == "env"
        resp = {"status": "ok", "tag": frame["tag"], "response": {"op": "open", "qid": qid("file" if is_env else "dir", 2 if is_env else 1)}}
    elif op == "read":
        off = req["offset"]
        cnt = req["count"]
        chunk = PAYLOAD[off : off + cnt] if fids.get(req["fid"]) == "env" else b""
        resp = {"status": "ok", "tag": frame["tag"], "response": {"op": "read", "data": list(chunk)}}
    elif op == "clunk":
        fids.pop(req["fid"], None)
        resp = {"status": "ok", "tag": frame["tag"], "response": {"op": "clunk"}}
    else:
        resp = {"status": "error", "tag": frame["tag"], "code": "unsupported"}
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#;
    std::fs::write(pkg_dir.join("alan-package.toml"), manifest).unwrap();
    std::fs::write(pkg_dir.join("bin/server.py"), server_script).unwrap();
    std::fs::set_permissions(
        pkg_dir.join("bin/server.py"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    let packages = PackageService::open(tmp.path().join("store")).unwrap();
    let installed = run_q_command_with_package_service(
        &packages,
        Some(pkg_dir.as_path()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "github-test-pack".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(installed.exit_code, 0);

    let srvfs = Arc::new(SrvFs::new());
    let mgr = FileserverManager::new(packages, srvfs.clone());

    let write_fake_gh = |name: &str, body: &str| {
        let p = tmp.path().join(name);
        std::fs::write(&p, body).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    };

    async fn read_child_env(mgr: &FileserverManager) -> serde_json::Value {
        let live = alan_kernel::LiveNamespace::new(Namespace::new());
        mgr.mount_into_live_namespace("github", &live, Access::ReadOnly)
            .await
            .unwrap();
        let shell = alan_shell::Shell::new(InProcessTransport::new(Arc::new(
            alan_kernel::MountFs::from_live_namespace(live),
        )));
        serde_json::from_slice::<serde_json::Value>(&shell.cat("/mnt/github/env").await.unwrap())
            .unwrap()
    }

    // 1. allow_network = true WITHOUT allow_host_gh_token must NOT invoke `gh auth token` or inject GH_TOKEN.
    let marker = tmp.path().join("gh-called.marker");
    let tracking_gh = write_fake_gh(
        "gh-track.sh",
        &format!(
            "#!/bin/sh\ntouch \"{}\"\nprintf 'should-not-inject\\n'\n",
            marker.display()
        ),
    );
    let mut env_ungranted = BTreeMap::new();
    env_ungranted.insert(
        "ALAN_GITHUBFS_GH_BIN".into(),
        tracking_gh.display().to_string(),
    );
    let grant_ungranted = FileserverLaunchGrant::new("github-test-pack")
        .with_allow_network(true)
        .with_runtime_env(env_ungranted);
    assert!(!grant_ungranted.allow_host_gh_token);
    mgr.start(grant_ungranted).await.unwrap();
    assert!(
        !marker.exists(),
        "gh must not run when allow_host_gh_token=false"
    );
    let child_env = read_child_env(&mgr).await;
    assert!(child_env["gh_token"].is_null());
    mgr.stop("github").await.unwrap();

    // 2. allow_network = true WITH allow_host_gh_token = true and no explicit token injects token from `gh auth token`.
    let token_gh = write_fake_gh(
        "gh-ok.sh",
        "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"token\" ]; then printf 'fixture-keychain-token-123\\n'; exit 0; fi\nexit 1\n",
    );
    let mut env_ok = BTreeMap::new();
    env_ok.insert(
        "ALAN_GITHUBFS_GH_BIN".into(),
        token_gh.display().to_string(),
    );
    let grant_ok = FileserverLaunchGrant::new("github-test-pack")
        .with_allow_network(true)
        .with_allow_host_gh_token(true)
        .with_runtime_env(env_ok);
    mgr.start(grant_ok).await.unwrap();
    let child_env = read_child_env(&mgr).await;
    assert_eq!(child_env["gh_token"], "fixture-keychain-token-123");
    assert!(child_env["github_token"].is_null());
    assert!(
        !mgr.recent_stderr("github")
            .await
            .unwrap_or_default()
            .contains("fixture-keychain-token-123")
    );
    mgr.stop("github").await.unwrap();

    // 3. allow_network = false does NOT invoke `gh auth token` and does NOT inject GH_TOKEN even if allow_host_gh_token = true.
    let mut env_no_net = BTreeMap::new();
    env_no_net.insert(
        "ALAN_GITHUBFS_GH_BIN".into(),
        tracking_gh.display().to_string(),
    );
    mgr.start(
        FileserverLaunchGrant::new("github-test-pack")
            .with_allow_network(false)
            .with_allow_host_gh_token(true)
            .with_runtime_env(env_no_net),
    )
    .await
    .unwrap();
    assert!(!marker.exists(), "gh must not run when allow_network=false");
    let child_env = read_child_env(&mgr).await;
    assert!(child_env["gh_token"].is_null());
    mgr.stop("github").await.unwrap();

    // 4. Explicit GH_TOKEN or GITHUB_TOKEN skips host `gh auth token` lookup.
    for (var, val, expected_gh, expected_github) in [
        ("GH_TOKEN", "explicit-gh", Some("explicit-gh"), None),
        (
            "GITHUB_TOKEN",
            "explicit-github",
            None,
            Some("explicit-github"),
        ),
    ] {
        let mut env_supplied = BTreeMap::new();
        env_supplied.insert(
            "ALAN_GITHUBFS_GH_BIN".into(),
            tracking_gh.display().to_string(),
        );
        env_supplied.insert(var.into(), val.into());
        mgr.start(
            FileserverLaunchGrant::new("github-test-pack")
                .with_allow_network(true)
                .with_allow_host_gh_token(true)
                .with_runtime_env(env_supplied),
        )
        .await
        .unwrap();
        assert!(
            !marker.exists(),
            "gh must not run when {var} is already supplied"
        );
        let child_env = read_child_env(&mgr).await;
        assert_eq!(child_env["gh_token"].as_str(), expected_gh);
        assert_eq!(child_env["github_token"].as_str(), expected_github);
        mgr.stop("github").await.unwrap();
    }

    // 5. Missing binary, non-zero exit, and multi-line stdout all fall back silently without failing startup.
    let failing_gh = write_fake_gh("gh-fail.sh", "#!/bin/sh\nexit 1\n");
    let multiline_gh = write_fake_gh("gh-multi.sh", "#!/bin/sh\nprintf 'tok1\\ntok2\\n'\n");
    let missing_gh = tmp.path().join("does-not-exist-gh");
    for gh_path in [&missing_gh, &failing_gh, &multiline_gh] {
        let mut env_fb = BTreeMap::new();
        env_fb.insert("ALAN_GITHUBFS_GH_BIN".into(), gh_path.display().to_string());
        mgr.start(
            FileserverLaunchGrant::new("github-test-pack")
                .with_allow_network(true)
                .with_allow_host_gh_token(true)
                .with_runtime_env(env_fb),
        )
        .await
        .unwrap();
        let child_env = read_child_env(&mgr).await;
        assert!(
            child_env["gh_token"].is_null(),
            "unexpected token for {}",
            gh_path.display()
        );
        mgr.stop("github").await.unwrap();
    }

    // 6. Bounded timeout falls back cleanly to None without failing or hanging.
    let slow_gh = write_fake_gh("gh-slow.sh", "#!/bin/sh\nsleep 10\n");
    let export = FileserverPackageExport {
        service_id: "github".into(),
        description: None,
        entrypoint: "bin/server.py".into(),
        allow_network: true,
        network_domains: Vec::new(),
        timeout_secs: 5,
        max_message_bytes: 65536,
        descriptor_path: "fileservers/github/server.json".into(),
        transport: FileserverTransportSpec::Stdio {
            command: "bin/server.py".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
        },
    };
    let mut env_slow = BTreeMap::new();
    env_slow.insert("ALAN_GITHUBFS_GH_BIN".into(), slow_gh.display().to_string());
    let grant_slow = FileserverLaunchGrant::new("github-test-pack")
        .with_allow_network(true)
        .with_allow_host_gh_token(true)
        .with_runtime_env(env_slow);
    let timed_out = resolve_github_fileserver_token_with_timeout(
        &export,
        &grant_slow,
        Duration::from_millis(100),
    )
    .await;
    assert_eq!(timed_out, None);
}
