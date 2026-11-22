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
