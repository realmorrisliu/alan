//! Loopback TCP listener, mount options, dead-PID GC, and live macOS `/sbin/mount_nfs`
//! + Seatbelt `(deny network*)` verification tests.

use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use alan_ap::reference::MemFs;
use tokio::net::TcpStream;

use crate::paths::SystemStorePaths;

use super::mount::{ReifiedMountManager, gc_stale_reified_mounts, mount_nfs_options};
use super::nfs_server::NfsLoopbackServer;
use super::rpc::{
    NFS_PROGRAM, NFS_VERSION, NfsProc, RpcAuth, encode_rpc_call, read_rpc_record, write_rpc_record,
};
use super::tests::parse_accepted_reply;

#[tokio::test]
async fn loopback_tcp_server_binds_strictly_to_127_0_0_1_and_serves_rpc() {
    let server = NfsLoopbackServer::start(Arc::new(MemFs::with_read_only_file("hi.txt", b"ok")))
        .await
        .unwrap();
    assert_eq!(server.local_addr().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_ne!(server.port(), 0);

    let mut stream = TcpStream::connect(server.local_addr()).await.unwrap();
    let call = encode_rpc_call(
        55,
        NFS_PROGRAM,
        NFS_VERSION,
        NfsProc::Null as u32,
        &RpcAuth::None,
        &[],
    )
    .unwrap();
    write_rpc_record(&mut stream, &call).await.unwrap();
    let reply = read_rpc_record(&mut stream).await.unwrap().unwrap();
    assert_eq!(parse_accepted_reply(&reply, 55).remaining(), 0);

    server.shutdown().await;
}

#[test]
fn mount_options_and_gc_stale_reified_mounts_work() {
    assert_eq!(
        mount_nfs_options(12345),
        "vers=3,tcp,port=12345,mountport=12345,rdonly,locallocks,nfc,noatime,actimeo=1,rsize=65536,wsize=65536,soft,intr,dumbtimer,timeo=300,retrans=6,mutejukebox,nobrowse"
    );

    let tmp = tempfile::TempDir::new().unwrap();
    let store = SystemStorePaths::from_root(&tmp.path().join("System Store")).unwrap();
    let root = store.reified_mounts().unwrap();
    fs::create_dir_all(&root).unwrap();

    // Dead PID instance with two stale mount dirs.
    let dead_dir = store.instance_reified_mounts("alan-dead").unwrap();
    fs::create_dir_all(dead_dir.join("proc")).unwrap();
    fs::create_dir_all(dead_dir.join("agent")).unwrap();
    fs::write(
        dead_dir.join("instance.json"),
        serde_json::json!({
            "instance_id": "alan-dead",
            "pid": 999_999_999u32,
            "created_at_ms": 1u64
        })
        .to_string(),
    )
    .unwrap();

    // Live PID instance (current process) under a different instance id.
    let live_dir = store.instance_reified_mounts("alan-live").unwrap();
    fs::create_dir_all(live_dir.join("root")).unwrap();
    fs::write(
        live_dir.join("instance.json"),
        serde_json::json!({
            "instance_id": "alan-live",
            "pid": std::process::id(),
            "created_at_ms": 1u64
        })
        .to_string(),
    )
    .unwrap();

    // Unknown directory lacking instance.json must be preserved.
    let unknown_dir = root.join("foreign-dir");
    fs::create_dir_all(&unknown_dir).unwrap();

    let (manager, report) = ReifiedMountManager::new(store.clone(), "alan-current").unwrap();
    assert_eq!(report.swept_instances, vec!["alan-dead".to_string()]);
    assert_eq!(report.skipped_live_instances, vec!["alan-live".to_string()]);
    assert_eq!(report.removed_mount_dirs, 2);
    assert!(!dead_dir.exists());
    assert!(live_dir.exists());
    assert!(unknown_dir.exists());
    assert!(manager.instance_dir().join("instance.json").is_file());

    // Direct gc_stale_reified_mounts call is idempotent.
    let second = gc_stale_reified_mounts(&store, "alan-current").unwrap();
    assert!(second.swept_instances.is_empty());
    assert_eq!(second.skipped_live_instances, vec!["alan-live".to_string()]);
}

#[tokio::test]
async fn reified_mount_manager_validates_names_and_cleans_instance_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = SystemStorePaths::from_root(&tmp.path().join("System Store")).unwrap();
    let (manager, _) = ReifiedMountManager::new(store, "alan-unit").unwrap();
    let instance_dir = manager.instance_dir().to_path_buf();
    assert!(instance_dir.is_dir());
    assert!(manager.active_mount_points().is_empty());

    let fs_server = Arc::new(MemFs::empty());
    assert!(manager.mount("../escape", fs_server.clone()).await.is_err());
    assert!(manager.mount("", fs_server).await.is_err());
    assert!(!manager.unmount("missing").await.unwrap());
    assert!(manager.gc_stale().unwrap().swept_instances.is_empty());

    manager.unmount_all().await.unwrap();
    assert!(!instance_dir.exists());
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn macos_live_zero_root_nfs_mount_read_sandbox_exec_and_rofs() {
    use std::process::Command;

    use alan_ap::FileServer;

    use super::mount::ReifiedMountHandle;
    use super::tests::DynamicTestFs;

    let tmp = tempfile::TempDir::new().unwrap();
    let mount_point = tmp.path().join("reified-ap");
    let fs_server: Arc<dyn FileServer> = Arc::new(DynamicTestFs::new().await);

    let handle = ReifiedMountHandle::mount_at(mount_point.clone(), fs_server)
        .await
        .expect("zero-root /sbin/mount_nfs should mount loopback NFSv3");
    assert!(handle.is_mounted());

    // 1. Standard POSIX utilities: ls, cat, grep.
    let ls_out = Command::new("/bin/ls")
        .arg(&mount_point)
        .output()
        .expect("run ls");
    assert!(ls_out.status.success(), "ls failed: {:?}", ls_out);
    let ls_text = String::from_utf8_lossy(&ls_out.stdout);
    assert!(ls_text.contains("hello.txt"), "ls output: {ls_text}");
    assert!(ls_text.contains("sub"), "ls output: {ls_text}");
    assert!(ls_text.contains("events"), "ls output: {ls_text}");
    assert!(ls_text.contains("status"), "ls output: {ls_text}");

    let cat_hello = Command::new("/bin/cat")
        .arg(mount_point.join("hello.txt"))
        .output()
        .expect("run cat hello.txt");
    assert!(cat_hello.status.success());
    assert_eq!(cat_hello.stdout, b"hello alan9 nfs\nline two needle\n");

    let cat_nested = Command::new("/bin/cat")
        .arg(mount_point.join("sub/nested.txt"))
        .output()
        .expect("run cat sub/nested.txt");
    assert!(cat_nested.status.success());
    assert_eq!(cat_nested.stdout, b"deep nested content\n");

    // Dynamic Stream and zero-stat status nodes are readable via plain cat without blocking!
    let cat_stream = Command::new("/bin/cat")
        .arg(mount_point.join("events"))
        .output()
        .expect("run cat events");
    assert!(cat_stream.status.success());
    assert_eq!(cat_stream.stdout, b"event-1: ready\nevent-2: ok\n");

    let cat_status = Command::new("/bin/cat")
        .arg(mount_point.join("status"))
        .output()
        .expect("run cat status");
    assert!(cat_status.status.success());
    assert_eq!(cat_status.stdout, b"state=healthy\nmode=ro\n");

    let grep_out = Command::new("/usr/bin/grep")
        .arg("needle")
        .arg(mount_point.join("hello.txt"))
        .output()
        .expect("run grep");
    assert!(grep_out.status.success());
    assert_eq!(grep_out.stdout, b"line two needle\n");

    // 2. Seatbelt sandbox-exec with `(deny network*)` still reads transparently through kernel NFSv3!
    let sb_profile = "(version 1)\n(allow default)\n(deny network*)\n";
    let sb_cat = Command::new("/usr/bin/sandbox-exec")
        .arg("-p")
        .arg(sb_profile)
        .arg("/bin/cat")
        .arg(mount_point.join("hello.txt"))
        .output()
        .expect("run sandbox-exec cat");
    assert!(
        sb_cat.status.success(),
        "sandbox-exec (deny network*) cat failed: stderr={}",
        String::from_utf8_lossy(&sb_cat.stderr)
    );
    assert_eq!(sb_cat.stdout, b"hello alan9 nfs\nline two needle\n");

    let sb_grep = Command::new("/usr/bin/sandbox-exec")
        .arg("-p")
        .arg(sb_profile)
        .arg("/usr/bin/grep")
        .arg("healthy")
        .arg(mount_point.join("status"))
        .output()
        .expect("run sandbox-exec grep");
    assert!(
        sb_grep.status.success(),
        "sandbox-exec (deny network*) grep failed: stderr={}",
        String::from_utf8_lossy(&sb_grep.stderr)
    );
    assert_eq!(sb_grep.stdout, b"state=healthy\n");

    // 3. Writes fail with Read-only file system (EROFS).
    let write_err = fs::write(mount_point.join("new.txt"), b"nope")
        .expect_err("write to read-only NFS mount must fail");
    assert_eq!(
        write_err.raw_os_error(),
        Some(libc::EROFS),
        "expected EROFS, got {write_err:?}"
    );
    let mkdir_err = fs::create_dir(mount_point.join("newdir"))
        .expect_err("mkdir on read-only NFS mount must fail");
    assert_eq!(
        mkdir_err.raw_os_error(),
        Some(libc::EROFS),
        "expected EROFS, got {mkdir_err:?}"
    );
    let remove_err = fs::remove_file(mount_point.join("hello.txt"))
        .expect_err("remove on read-only NFS mount must fail");
    assert_eq!(
        remove_err.raw_os_error(),
        Some(libc::EROFS),
        "expected EROFS, got {remove_err:?}"
    );

    // 4. Clean unmount removes mount point cleanly.
    handle
        .unmount()
        .await
        .expect("clean unmount should succeed");
    assert!(!mount_point.exists());
}
