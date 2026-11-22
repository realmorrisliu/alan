#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alan_agent_engine::tools::{ReifiedMountAccess, SandboxHostMount, ToolRegistry};
use alan_agent_engine::{AgentProcessConfig, LlmClient};
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Request};
use alan_kernel::{Access, LiveNamespace, MountFs, Namespace, SrvFs};
use alan_llm::MockLlmProvider;
use alan_service_manager::{
    CLI_IMPORT_MOUNT_PATH, FileserverLaunchGrant, FileserverManager, PackageCommand,
    PackageExportKind, PackageService, PackageState, ProcessLaunchContext, ServiceManager,
    ServiceManagerConfig, run_q_command_with_package_service,
};
use alan_shell::Shell;
use tokio::net::TcpListener;

const EXTERNAL_AP_FIXTURE: &str = r#"#!/usr/bin/env python3
import json
import os
import socket
import sys
import threading

LOCK = threading.Lock()
EVENTS_COND = threading.Condition(LOCK)
OUT_LOCK = threading.Lock()
STATE = {
    "items": [],
    "commits": 0,
    "has_runtime_cred": bool(os.environ.get("SCOPED_RUNTIME_TOKEN")),
}
EVENTS = bytearray()
FIDS = {}

def append_event(payload):
    line = (json.dumps(payload) + "\n").encode("utf-8")
    EVENTS.extend(line)
    EVENTS_COND.notify_all()

def stat_for(node, writable=True):
    if node == "root":
        return {"name": "", "qid": {"kind": "dir", "version": 0, "path": 1}, "length": 18, "executable": False, "writable": False}
    if node == "ctl":
        return {"name": "ctl", "qid": {"kind": "file", "version": 0, "path": 2}, "length": 0, "executable": False, "writable": writable}
    if node == "status":
        data = json.dumps(STATE).encode("utf-8")
        return {"name": "status", "qid": {"kind": "file", "version": STATE["commits"], "path": 3}, "length": len(data), "executable": False, "writable": False}
    if node == "events":
        return {"name": "events", "qid": {"kind": "stream", "version": 0, "path": 4}, "length": len(EVENTS), "executable": False, "writable": False}
    if node == "huge":
        return {"name": "huge", "qid": {"kind": "file", "version": 0, "path": 5}, "length": 2048, "executable": False, "writable": False}
    if node == "slow":
        return {"name": "slow", "qid": {"kind": "file", "version": 0, "path": 6}, "length": 2, "executable": False, "writable": False}
    return None

def handle(req):
    op = req["op"]
    with LOCK:
        if op == "walk":
            fid, newfid, names = req["fid"], req["newfid"], req["names"]
            if newfid == 0 or newfid in FIDS:
                return False, "bad_request"
            base = "root" if fid == 0 else FIDS.get(fid, {}).get("node")
            if base is None:
                return False, "not_found"
            if not names:
                FIDS[newfid] = {"node": base, "mode": None, "buf": bytearray()}
                return True, {"op": "walk", "qid": stat_for(base)["qid"]}
            if base == "root" and len(names) == 1 and names[0] in ("ctl", "status", "events", "huge", "slow"):
                target = names[0]
                FIDS[newfid] = {"node": target, "mode": None, "buf": bytearray()}
                return True, {"op": "walk", "qid": stat_for(target)["qid"]}
            return False, "not_found"
        if op == "open":
            fid, mode = req["fid"], req["mode"]
            if fid == 0:
                return True, {"op": "open", "qid": stat_for("root")["qid"]}
            st = FIDS.get(fid)
            if st is None:
                return False, "not_found"
            if st["node"] in ("status", "events", "huge", "slow") and mode != "read":
                return False, "no_access"
            st["mode"] = mode
            return True, {"op": "open", "qid": stat_for(st["node"])["qid"]}
        if op == "stat":
            fid = req["fid"]
            node = "root" if fid == 0 else FIDS.get(fid, {}).get("node")
            if node is None:
                return False, "not_found"
            return True, {"op": "stat", "stat": stat_for(node)}
        if op == "read":
            fid, off, cnt = req["fid"], req["offset"], req["count"]
            node = "root" if fid == 0 else FIDS.get(fid, {}).get("node")
            if node is None:
                return False, "not_found"
            if node == "root":
                raw = b"ctl\nstatus\nevents\nhuge\nslow"
            elif node == "status":
                raw = json.dumps(STATE).encode("utf-8")
            elif node == "events":
                if off >= len(EVENTS):
                    EVENTS_COND.wait(timeout=5.0)
                raw = bytes(EVENTS)
            elif node == "huge":
                raw = b"h" * 2048
            elif node == "slow":
                pass
            elif node == "ctl":
                raw = b"add <item> | probe_net <host> <port> | probe_write <path> | probe_read <path>\n"
            else:
                return False, "not_found"
            if node != "slow":
                return True, {"op": "read", "data": list(raw[off:off + cnt])}
        if op == "write":
            fid, off, data = req["fid"], req["offset"], req["data"]
            st = FIDS.get(fid)
            if st is None:
                return False, "not_found"
            if st["node"] != "ctl" or st["mode"] not in ("write", "read_write"):
                return False, "no_access"
            b = bytes(data)
            end = off + len(b)
            if end > len(st["buf"]):
                st["buf"].extend(b"\x00" * (end - len(st["buf"])))
            st["buf"][off:end] = b
            return True, {"op": "write", "count": len(b)}
        if op == "clunk":
            fid = req["fid"]
            if fid == 0:
                return True, {"op": "clunk"}
            st = FIDS.pop(fid, None)
            if st is None:
                return False, "not_found"
            if st["node"] == "ctl" and st["mode"] in ("write", "read_write"):
                text = st["buf"].decode("utf-8", errors="strict").strip()
                parts = text.split(" ", 2)
                verb = parts[0] if parts else ""
                if verb == "add" and len(parts) >= 2 and parts[1].strip():
                    item = text[4:].strip()
                    STATE["items"].append(item)
                    STATE["commits"] += 1
                    append_event({"seq": STATE["commits"], "item": item})
                    return True, {"op": "clunk"}
                if verb == "probe_net" and len(parts) == 3:
                    host, port = parts[1], int(parts[2])
                    try:
                        s = socket.create_connection((host, port), timeout=2)
                        s.close()
                        return True, {"op": "clunk"}
                    except OSError:
                        return False, "no_access"
                if verb == "probe_write" and len(parts) >= 2:
                    target = text[len("probe_write "):].strip()
                    try:
                        with open(target, "w", encoding="utf-8") as f:
                            f.write("escaped")
                        return True, {"op": "clunk"}
                    except OSError:
                        return False, "no_access"
                if verb == "probe_read" and len(parts) >= 2:
                    target = text[len("probe_read "):].strip()
                    try:
                        with open(target, "r", encoding="utf-8") as f:
                            _ = f.read(32)
                        return True, {"op": "clunk"}
                    except OSError:
                        return False, "no_access"
                return False, "bad_request"
            return True, {"op": "clunk"}
        return False, "unsupported"

import time

def run_slow_sleep():
    time.sleep(2.2)
    return True, {"op": "read", "data": list(b"ok")}

# handle("slow") runs outside LOCK
def handle_or_slow(req):
    if req.get("op") == "read":
        with LOCK:
            fid = req["fid"]
            node = "root" if fid == 0 else FIDS.get(fid, {}).get("node")
        if node == "slow":
            return run_slow_sleep()
    return handle(req)

if os.environ.get("BOOT_SENTINEL"):
    with open(os.environ["BOOT_SENTINEL"], "w", encoding="utf-8") as f:
        f.write("started\n")

def dispatch(frame, writer, out_lock):
    ok, payload = handle_or_slow(frame["request"])
    out = {"status": "ok", "tag": frame["tag"], "response": payload} if ok else {"status": "error", "tag": frame["tag"], "code": payload}
    with out_lock:
        writer.write((json.dumps(out) + "\n").encode("utf-8"))
        writer.flush()

def serve_stream(reader, writer):
    out_lock = threading.Lock()
    while True:
        line = reader.readline()
        if not line:
            break
        if not line.strip():
            continue
        frame = json.loads(line)
        threading.Thread(target=dispatch, args=(frame, writer, out_lock), daemon=True).start()

sock_path = os.environ.get("ALAN_FILESERVER_SOCKET")
if sock_path:
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(sock_path)
    srv.listen(8)
    while True:
        conn, _ = srv.accept()
        threading.Thread(
            target=lambda c: serve_stream(c.makefile("rb"), c.makefile("wb")),
            args=(conn,),
            daemon=True,
        ).start()
else:
    serve_stream(sys.stdin.buffer, sys.stdout.buffer)
"#;

fn write_fileserver_package_dir(dir: &std::path::Path, sentinel_path: Option<&std::path::Path>) {
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    let sentinel_block = sentinel_path
        .map(|path| format!("env = {{ BOOT_SENTINEL = \"{}\" }}\n", path.display()))
        .unwrap_or_default();
    std::fs::write(
        dir.join("alan-package.toml"),
        format!(
            r#"
schema_version = 1
kind = "fileserver"
version = "0.1.0"
description = "Integration test external aP fileserver"

[fileserver]
id = "extfs"
description = "External aP test fixture"
entrypoint = "bin/extfs.py"
allow_network = true
network_domains = ["127.0.0.1"]
timeout_secs = 15
max_message_bytes = 131072

[fileserver.transport]
type = "stdio"
command = "bin/extfs.py"
{sentinel_block}"#
        ),
    )
    .unwrap();
    std::fs::write(dir.join("bin/extfs.py"), EXTERNAL_AP_FIXTURE).unwrap();
}

#[tokio::test]
async fn q_install_persists_catalog_without_spawning_mounting_or_granting_credentials() {
    let store_dir = tempfile::tempdir().unwrap();
    let pkg_store = store_dir.path().join("packages");
    let pkg_src = tempfile::tempdir().unwrap();
    let sentinel = store_dir.path().join("should-never-exist.sentinel");
    write_fileserver_package_dir(pkg_src.path(), Some(&sentinel));

    let service = PackageService::open(pkg_store.clone()).unwrap();
    let installed = run_q_command_with_package_service(
        &service,
        Some(pkg_src.path()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "extfs-pack".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        installed.exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&installed.output)
    );

    let record = service.resolve("extfs-pack").unwrap();
    assert_eq!(record.export_kind, PackageExportKind::Fileserver);
    assert_eq!(record.reference_count, 0);
    assert!(
        !sentinel.exists(),
        "q install MUST NOT spawn the fileserver executable"
    );

    let list = run_q_command_with_package_service(&service, None, &["list".to_string()])
        .await
        .unwrap();
    let list_text = String::from_utf8(list.output).unwrap();
    assert!(list_text.contains("extfs-pack"), "{list_text}");
    assert!(list_text.contains("export=fileserver"), "{list_text}");
    assert!(list_text.contains("fileserver=extfs(stdio)"), "{list_text}");

    // Booting ServiceManager against this store must NOT auto-start, publish /srv/extfs, or mount /mnt/extfs.
    let mut cfg = ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    cfg.package_store = Some(pkg_store.clone());
    let manager = ServiceManager::boot(cfg).await.unwrap();
    let (_, _, ns) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = Shell::new(InProcessTransport::new(ns));

    let srv_entries = shell.ls("/srv").await.unwrap();
    assert!(
        !srv_entries.iter().any(|entry| entry == "extfs"),
        "install-only fileserver leaked into /srv: {srv_entries:?}"
    );
    assert!(shell.ls("/mnt/extfs").await.is_err());
    assert!(shell.ls("/lib/pkg/extfs-pack").await.is_err());
    assert!(
        !sentinel.exists(),
        "ServiceManager boot MUST NOT auto-start installed fileserver packages"
    );
    let q1 = String::from_utf8(shell.cat("/man/1/q").await.unwrap()).unwrap();
    assert!(q1.contains("fileserver"), "{q1}");

    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn confined_fileserver_lifecycle_enforces_clunk_atomicity_mounts_and_confinement() {
    let store_dir = tempfile::tempdir().unwrap();
    let pkg_store = store_dir.path().join("packages");
    let pkg_src = tempfile::tempdir().unwrap();
    write_fileserver_package_dir(pkg_src.path(), None);

    let packages = PackageService::open(pkg_store.clone()).unwrap();
    let installed = run_q_command_with_package_service(
        &packages,
        Some(pkg_src.path()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "extfs-pack".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(installed.exit_code, 0);

    // Prepare a read-only Host dir outside `/tmp`/`TMPDIR` (Landlock allows `/tmp`
    // scratch writes) and a local TCP listener for OS confinement tests.
    let ro_host_dir = tempdir_outside_tmp();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accept_task =
        tokio::spawn(async move { while let Ok((_sock, _)) = listener.accept().await {} });

    let srvfs = Arc::new(SrvFs::new());
    let fs_manager = FileserverManager::new(packages.clone(), srvfs.clone());

    let mut runtime_env = BTreeMap::new();
    runtime_env.insert(
        "SCOPED_RUNTIME_TOKEN".to_string(),
        "ephemeral-host-secret".to_string(),
    );

    let grant = FileserverLaunchGrant::new("extfs-pack")
        .with_mount_access(Access::ReadWrite)
        .with_allow_network(false) // Manifest declares allow_network=true, but launch grant denies network!
        .with_host_mounts(vec![SandboxHostMount {
            namespace_path: PathBuf::from("/mnt/ro-host"),
            host_path: ro_host_dir.path().to_path_buf(),
            access: ReifiedMountAccess::ReadOnly,
        }])
        .with_runtime_env(runtime_env);

    let status = fs_manager.start(grant).await.unwrap();
    assert!(status.running);
    assert!(!status.allow_network);
    assert_eq!(status.srv_path, "/srv/extfs");
    assert!(srvfs.lookup("extfs").await.is_some());

    // Confirm scoped runtime credential was NEVER persisted into Package Store catalog or manifest!
    let catalog_text = std::fs::read_to_string(pkg_store.join("catalog.json")).unwrap();
    assert!(!catalog_text.contains("ephemeral-host-secret"));

    // Bind explicitly as ReadOnly into one namespace and ReadWrite into another.
    let ro_live = LiveNamespace::new(Namespace::new());
    fs_manager
        .mount_into_live_namespace("extfs", &ro_live, Access::ReadOnly)
        .await
        .unwrap();
    let ro_shell = Shell::new(InProcessTransport::new(Arc::new(
        MountFs::from_live_namespace(ro_live.clone()),
    )));

    let rw_live = LiveNamespace::new(Namespace::new());
    fs_manager
        .mount_into_live_namespace("extfs", &rw_live, Access::ReadWrite)
        .await
        .unwrap();
    let rw_shell = Shell::new(InProcessTransport::new(Arc::new(
        MountFs::from_live_namespace(rw_live.clone()),
    )));

    // ReadOnly mount allows reading status/stat (with writable=false), rejects writes!
    let status_json: serde_json::Value =
        serde_json::from_slice(&ro_shell.cat("/mnt/extfs/status").await.unwrap()).unwrap();
    assert_eq!(status_json["commits"], 0);
    assert_eq!(status_json["has_runtime_cred"], true);
    assert!(!ro_shell.stat("/mnt/extfs/ctl").await.unwrap().writable);
    assert_eq!(
        ro_shell.write("/mnt/extfs/ctl", b"add denied").await,
        Err(ErrorCode::NoAccess)
    );
    let after_ro_reject: serde_json::Value =
        serde_json::from_slice(&ro_shell.cat("/mnt/extfs/status").await.unwrap()).unwrap();
    assert_eq!(after_ro_reject["commits"], 0);

    // Partial write on ReadWrite ctl does NOT commit until clunk!
    let rw_mount_fs = MountFs::from_live_namespace(rw_live.clone());
    let ctl_fid = Fid(101);
    rw_mount_fs
        .walk(
            Fid::ROOT,
            ctl_fid,
            &["mnt".to_string(), "extfs".to_string(), "ctl".to_string()],
        )
        .await
        .unwrap();
    rw_mount_fs.open(ctl_fid, OpenMode::Write).await.unwrap();
    rw_mount_fs.write(ctl_fid, 0, b"add ").await.unwrap();

    let mid_write: serde_json::Value =
        serde_json::from_slice(&rw_shell.cat("/mnt/extfs/status").await.unwrap()).unwrap();
    assert_eq!(
        mid_write["commits"], 0,
        "partial write must not mutate before clunk"
    );

    rw_mount_fs.write(ctl_fid, 4, b"first-item").await.unwrap();
    rw_mount_fs.clunk(ctl_fid).await.unwrap();

    let after_commit: serde_json::Value =
        serde_json::from_slice(&rw_shell.cat("/mnt/extfs/status").await.unwrap()).unwrap();
    assert_eq!(after_commit["commits"], 1);
    assert_eq!(after_commit["items"], serde_json::json!(["first-item"]));

    // Malformed command fails on clunk without mutating state!
    assert_eq!(
        rw_shell.write("/mnt/extfs/ctl", b"bogus-verb").await,
        Err(ErrorCode::BadRequest)
    );
    let after_bad: serde_json::Value =
        serde_json::from_slice(&rw_shell.cat("/mnt/extfs/status").await.unwrap()).unwrap();
    assert_eq!(after_bad["commits"], 1);

    // Stream file `events` recorded the committed event, and blocking stream read wakes on next commit!
    let event_stat = rw_shell.stat("/mnt/extfs/events").await.unwrap();
    assert_eq!(event_stat.qid.kind, FileKind::Stream);
    let events_raw = String::from_utf8(rw_shell.cat("/mnt/extfs/events").await.unwrap()).unwrap();
    assert!(events_raw.contains("first-item"), "{events_raw}");

    let stream_fid = Fid(102);
    rw_mount_fs
        .walk(
            Fid::ROOT,
            stream_fid,
            &["mnt".to_string(), "extfs".to_string(), "events".to_string()],
        )
        .await
        .unwrap();
    rw_mount_fs.open(stream_fid, OpenMode::Read).await.unwrap();
    let initial_len = rw_mount_fs.stat(stream_fid).await.unwrap().length;
    assert_eq!(initial_len as usize, events_raw.len());
    let reader_mount = Arc::new(MountFs::from_live_namespace(rw_live.clone()));
    let reader_fid = Fid(103);
    reader_mount
        .walk(
            Fid::ROOT,
            reader_fid,
            &["mnt".to_string(), "extfs".to_string(), "events".to_string()],
        )
        .await
        .unwrap();
    reader_mount.open(reader_fid, OpenMode::Read).await.unwrap();
    let stream_task = tokio::spawn(async move {
        let out = reader_mount.read(reader_fid, initial_len, 4096).await;
        let _ = reader_mount.clunk(reader_fid).await;
        out
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !stream_task.is_finished(),
        "stream read should block at live edge before new event arrives"
    );
    rw_shell
        .write("/mnt/extfs/ctl", b"add second-item")
        .await
        .unwrap();
    let next_bytes = tokio::time::timeout(Duration::from_secs(2), stream_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let next_text = String::from_utf8(next_bytes).unwrap();
    assert!(next_text.contains("second-item"), "{next_text}");
    rw_mount_fs.clunk(stream_fid).await.unwrap();

    // OS confinement verification (when OS sandbox is active, e.g. Seatbelt on macOS):
    if alan_agent_engine::tools::os_backend_active() {
        // 1. Outbound TCP network must be denied when grant.allow_network == false.
        let net_res = rw_shell
            .write(
                "/mnt/extfs/ctl",
                format!("probe_net 127.0.0.1 {port}").as_bytes(),
            )
            .await;
        assert_eq!(
            net_res,
            Err(ErrorCode::NoAccess),
            "confined fileserver unexpectedly bypassed network denial"
        );

        // 2. Write into ReadOnly Host mount must be denied by OS sandbox.
        let ro_escape = ro_host_dir.path().join("escaped.txt");
        let write_res = rw_shell
            .write(
                "/mnt/extfs/ctl",
                format!("probe_write {}", ro_escape.display()).as_bytes(),
            )
            .await;
        assert_eq!(write_res, Err(ErrorCode::NoAccess));
        assert!(!ro_escape.exists());

        // 3. Restricted Host dot-directory reads under $HOME must be denied by OS sandbox.
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let dotdir = home.join(".alan");
            if let Ok(entries) = std::fs::read_dir(&dotdir)
                && let Some(Ok(first_file)) = entries
                    .into_iter()
                    .find(|entry| entry.as_ref().ok().is_some_and(|e| e.path().is_file()))
            {
                let read_res = rw_shell
                    .write(
                        "/mnt/extfs/ctl",
                        format!("probe_read {}", first_file.path().display()).as_bytes(),
                    )
                    .await;
                assert_eq!(
                    read_res,
                    Err(ErrorCode::NoAccess),
                    "confined fileserver unexpectedly read restricted Host dotfile"
                );
            }
        }
    }

    // Explicitly mounting /mnt/extfs into ServiceManager boot LaunchContext enforces ReadOnly via Kernel!
    let mut boot_ctx = ProcessLaunchContext::root();
    fs_manager
        .mount_into_namespace("extfs", &mut boot_ctx.namespace, Access::ReadOnly)
        .await
        .unwrap();
    let sm = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        boot_ctx,
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    let (_, _, session_ns) = sm.local_entry().create_and_handoff().await.unwrap();
    let session_shell = Shell::new(InProcessTransport::new(session_ns));
    let seen_status =
        String::from_utf8(session_shell.cat("/mnt/extfs/status").await.unwrap()).unwrap();
    assert!(seen_status.contains("first-item"), "{seen_status}");
    assert_eq!(
        session_shell.write("/mnt/extfs/ctl", b"add blocked").await,
        Err(ErrorCode::NoAccess)
    );
    sm.shutdown().await.unwrap();

    // Uninstalling while running retires the package without breaking the active service until stop.
    let uninstall = packages
        .execute(PackageCommand::Uninstall {
            request_id: "retire-running".to_string(),
            package_id: "extfs-pack".to_string(),
        })
        .unwrap();
    assert!(uninstall.success);
    assert_eq!(uninstall.message, "retiring");
    assert_eq!(uninstall.package.unwrap().state, PackageState::Retiring);

    let still_alive = String::from_utf8(rw_shell.cat("/mnt/extfs/status").await.unwrap()).unwrap();
    assert!(still_alive.contains("first-item"));

    // Stopping unposts /srv/extfs, terminates subprocess, and releases lease so retired package GCs.
    fs_manager.stop("extfs").await.unwrap();
    assert!(srvfs.lookup("extfs").await.is_none());
    assert_eq!(
        rw_shell.cat("/mnt/extfs/status").await,
        Err(ErrorCode::NotFound)
    );
    assert!(
        !packages
            .catalog()
            .unwrap()
            .packages
            .contains_key("extfs-pack")
    );

    // Ensure Request is referenced cleanly across explicit clunk tests.
    let _ = std::any::type_name::<Request>();
    accept_task.abort();
}

#[tokio::test]
async fn confined_unix_socket_and_wire_bounds_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let pkg_dir = dir.path().join("sock-pkg");
    std::fs::create_dir_all(pkg_dir.join("bin")).unwrap();
    let manifest = "\
schema_version = 1
kind = \"fileserver\"

[fileserver]
id = \"sockfs\"
entrypoint = \"bin/extfs.py\"
timeout_secs = 1
max_message_bytes = 1024

[fileserver.transport]
type = \"unix_socket\"
socket_name = \"extfs.sock\"
";
    std::fs::write(pkg_dir.join("alan-package.toml"), manifest).unwrap();
    std::fs::write(pkg_dir.join("bin/extfs.py"), EXTERNAL_AP_FIXTURE).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            pkg_dir.join("bin/extfs.py"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    let store_dir = dir.path().join("system-store");
    let packages = PackageService::open(store_dir).unwrap();
    let imported = run_q_command_with_package_service(
        &packages,
        Some(pkg_dir.as_path()),
        &[
            "install".to_string(),
            "--name".to_string(),
            "sock-pkg".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        imported.exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&imported.output)
    );

    let srvfs = Arc::new(SrvFs::new());
    let manager = FileserverManager::new(packages, srvfs.clone());
    let status = manager
        .start(FileserverLaunchGrant::new("sock-pkg"))
        .await
        .unwrap();
    assert!(status.running);
    assert_eq!(status.mount_access, Access::ReadOnly);

    let mut rw_ns = Namespace::new();
    assert!(
        manager
            .mount_into_namespace("sockfs", &mut rw_ns, Access::ReadWrite)
            .await
            .is_err()
    );

    let mut ro_ns = Namespace::new();
    manager
        .mount_into_namespace("sockfs", &mut ro_ns, Access::ReadOnly)
        .await
        .unwrap();
    let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(ro_ns))));
    assert_eq!(shell.cat("/mnt/sockfs/huge").await.unwrap().len(), 2048);
    assert_eq!(shell.cat("/mnt/sockfs/slow").await, Err(ErrorCode::Io));
    assert_eq!(
        shell.write("/mnt/sockfs/ctl", b"add blocked").await,
        Err(ErrorCode::NoAccess)
    );

    let mut wire = manager.connect_wire_client("sockfs").await.unwrap();
    let stat = wire.call(Request::Stat { fid: Fid::ROOT }).await.unwrap();
    assert!(matches!(stat, alan_ap::Response::Stat { .. }));

    manager.stop_all().await.unwrap();
    assert!(srvfs.lookup("sockfs").await.is_none());
}

fn tempdir_outside_tmp() -> tempfile::TempDir {
    use std::path::Path;

    let blocked: Vec<PathBuf> = ["/tmp", "/var/tmp"]
        .into_iter()
        .map(PathBuf::from)
        .chain(std::env::var_os("TMPDIR").map(PathBuf::from))
        .flat_map(|p| [p.clone(), std::fs::canonicalize(&p).unwrap_or(p)])
        .collect();
    let workspace_target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    if workspace_target.is_dir() {
        let canonical = std::fs::canonicalize(&workspace_target).unwrap_or(workspace_target);
        if !blocked.iter().any(|b| canonical.starts_with(b))
            && let Ok(dir) = tempfile::Builder::new()
                .prefix("alan-ro-host-")
                .tempdir_in(&canonical)
        {
            return dir;
        }
    }
    tempfile::tempdir().unwrap()
}
