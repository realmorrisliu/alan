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
