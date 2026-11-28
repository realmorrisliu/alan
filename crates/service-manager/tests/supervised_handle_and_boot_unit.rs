use std::sync::Arc;
use std::time::Duration;

use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Request,
    Response, Stat,
};
use alan_kernel::{Access, MountFs, Namespace, SrvFs};
use alan_service_manager::{
    BootManifest, BootUnit, RestartPolicy, SupervisedHandle, SupervisionStrategy,
};
use alan_shell::Shell;
use async_trait::async_trait;
use tokio::sync::Notify;

fn boot_unit_toml(extra_fields: &str) -> String {
    format!(
        r#"name = "sample-unit"
executable = "/bin/sample-unit"
after = []
required = true
timeout_ms = 5000
restart_limit = 3
initial_backoff_ms = 50
max_backoff_ms = 500
stable_reset_ms = 5000
published_handles = ["sample"]
{extra_fields}"#
    )
}

struct BlockingReadFs {
    entered_read: Arc<Notify>,
    release_read: Arc<Notify>,
}

#[async_trait]
impl FileServer for BlockingReadFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, _names: &[String]) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Stream,
            version: 0,
            path: newfid.0,
        })
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Stream,
            version: 0,
            path: fid.0,
        })
    }

    async fn read(&self, _fid: Fid, _offset: Offset, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.entered_read.notify_one();
        self.release_read.notified().await;
        Ok(b"event\n".to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: "stream".to_string(),
            qid: Qid {
                kind: FileKind::Stream,
                version: 0,
                path: fid.0,
            },
            length: 0,
            executable: false,
            writable: false,
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}

#[tokio::test]
async fn supervised_handle_starts_unbound_and_rejects_ops_with_io() {
    let handle = SupervisedHandle::new();
    assert_eq!(handle.generation(), 0);
    assert!(!handle.is_bound());
    assert_eq!(
        handle.walk(Fid::ROOT, Fid(1), &[]).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(handle.stat(Fid::ROOT).await, Err(ErrorCode::Io));
    assert_eq!(handle.clunk(Fid::ROOT).await, Err(ErrorCode::Io));
    let err = handle
        .while_bound_async("Test Service is unavailable", || async { Ok(()) })
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Test Service is unavailable");
}

#[tokio::test]
async fn supervised_handle_stale_generation_fids_fail_with_io_across_all_ops() {
    let server1 = Arc::new(MemFs::writable());
    let handle = SupervisedHandle::bound(server1);
    assert_eq!(handle.generation(), 1);
    assert!(handle.is_bound());

    // Create a file and a subdirectory in generation 1, plus walk a separate fid to each.
    handle
        .create(Fid::ROOT, Fid(1), "doc", FileKind::File)
        .await
        .unwrap();
    handle.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    assert_eq!(handle.write(Fid(1), 0, b"gen1-data").await.unwrap(), 9);

    handle
        .create(Fid::ROOT, Fid(2), "dir", FileKind::Dir)
        .await
        .unwrap();

    // Also hold an unopened fid walked in generation 1 to test stale `open`.
    handle
        .walk(Fid::ROOT, Fid(3), &["doc".to_string()])
        .await
        .unwrap();

    // Hold a commit-on-clunk style write-only buffered fid in generation 1.
    handle
        .walk(Fid::ROOT, Fid(4), &["doc".to_string()])
        .await
        .unwrap();
    handle.open(Fid(4), OpenMode::Write).await.unwrap();
    assert_eq!(handle.write(Fid(4), 0, b"stale-write").await.unwrap(), 11);

    // Simulate restart: deactivate then rebind to a fresh generation 2 server.
    handle.deactivate();
    assert!(!handle.is_bound());
    assert_eq!(handle.generation(), 1);

    let server2 = Arc::new(MemFs::writable());
    assert_eq!(handle.bind(server2), 2);
    assert_eq!(handle.generation(), 2);
    assert!(handle.is_bound());

    // Pre-restart fids must fail cleanly with ErrorCode::Io across every FileServer op.
    assert_eq!(
        handle.open(Fid(3), OpenMode::Read).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(handle.read(Fid(1), 0, 64).await, Err(ErrorCode::Io));
    assert_eq!(handle.write(Fid(1), 0, b"mutate").await, Err(ErrorCode::Io));
    assert_eq!(handle.stat(Fid(1)).await, Err(ErrorCode::Io));
    assert_eq!(handle.walk(Fid(2), Fid(50), &[]).await, Err(ErrorCode::Io));
    assert_eq!(
        handle
            .create(Fid(2), Fid(51), "child", FileKind::File)
            .await,
        Err(ErrorCode::Io)
    );
    assert_eq!(handle.remove(Fid(1)).await, Err(ErrorCode::Io));
    assert_eq!(handle.clunk(Fid(1)).await, Err(ErrorCode::Io));
    assert_eq!(handle.clunk(Fid(2)).await, Err(ErrorCode::Io));
    assert_eq!(handle.clunk(Fid(3)).await, Err(ErrorCode::Io));
    // Stale write-buffered fid clunk must return Io and never mutate generation 2.
    assert_eq!(handle.clunk(Fid(4)).await, Err(ErrorCode::Io));

    // Generation 2 server remains untouched (no "doc" or "dir" from gen 1).
    assert_eq!(
        handle.walk(Fid::ROOT, Fid(10), &["doc".to_string()]).await,
        Err(ErrorCode::NotFound)
    );

    // Fresh walk/create from Fid::ROOT binds transparently to generation 2.
    handle
        .create(Fid::ROOT, Fid(11), "fresh", FileKind::File)
        .await
        .unwrap();
    handle.open(Fid(11), OpenMode::ReadWrite).await.unwrap();
    assert_eq!(handle.write(Fid(11), 0, b"gen2").await.unwrap(), 4);
    assert_eq!(handle.read(Fid(11), 0, 16).await.unwrap(), b"gen2");
    handle.clunk(Fid(11)).await.unwrap();
}

#[tokio::test]
async fn supervised_handle_rebind_reused_server_isolates_fids_and_allows_fresh_walk() {
    let shared_server = Arc::new(MemFs::with_read_only_file("banner", b"hello".to_vec()));
    let handle = SupervisedHandle::new();
    assert_eq!(handle.bind(shared_server.clone()), 1);

    handle
        .walk(Fid::ROOT, Fid(1), &["banner".to_string()])
        .await
        .unwrap();
    handle.open(Fid(1), OpenMode::Read).await.unwrap();
    assert_eq!(handle.read(Fid(1), 0, 32).await.unwrap(), b"hello");

    // Direct rebind without deactivate still increments generation monotonically.
    assert_eq!(handle.bind(shared_server.clone()), 2);
    assert_eq!(handle.generation(), 2);

    // Old Fid(1) is rejected even though the backing Arc is the same FileServer.
    assert_eq!(handle.read(Fid(1), 0, 32).await, Err(ErrorCode::Io));
    assert_eq!(handle.stat(Fid(1)).await, Err(ErrorCode::Io));

    // A fresh walk reusing client Fid(1) succeeds transparently on the new generation.
    handle
        .walk(Fid::ROOT, Fid(1), &["banner".to_string()])
        .await
        .unwrap();
    handle.open(Fid(1), OpenMode::Read).await.unwrap();
    assert_eq!(handle.read(Fid(1), 0, 32).await.unwrap(), b"hello");
    handle.clunk(Fid(1)).await.unwrap();

    // Third bind advances generation to 3.
    assert_eq!(handle.bind(shared_server), 3);
    assert_eq!(handle.generation(), 3);
}

#[tokio::test]
async fn supervised_handle_deactivate_clears_active_server_without_deadlocking_in_flight_read() {
    let entered_read = Arc::new(Notify::new());
    let release_read = Arc::new(Notify::new());
    let blocking = Arc::new(BlockingReadFs {
        entered_read: entered_read.clone(),
        release_read: release_read.clone(),
    });
    let handle = SupervisedHandle::bound(blocking);
    handle
        .walk(Fid::ROOT, Fid(1), &["events".to_string()])
        .await
        .unwrap();
    handle.open(Fid(1), OpenMode::Read).await.unwrap();

    let reader_handle = handle.clone();
    let reader = tokio::spawn(async move { reader_handle.read(Fid(1), 0, 64).await });
    entered_read.notified().await;

    // Deactivate and rebind must NOT deadlock while the gen-1 stream read is awaiting.
    handle.deactivate();
    assert!(!handle.is_bound());
    assert_eq!(handle.generation(), 1);
    assert_eq!(
        handle.walk(Fid::ROOT, Fid(2), &[]).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(handle.stat(Fid(1)).await, Err(ErrorCode::Io));

    let replacement = Arc::new(MemFs::with_read_only_file(
        "status",
        b"restarted\n".to_vec(),
    ));
    assert_eq!(handle.bind(replacement), 2);

    // Fresh walk immediately works against generation 2 while the old reader is still parked.
    handle
        .walk(Fid::ROOT, Fid(2), &["status".to_string()])
        .await
        .unwrap();
    handle.open(Fid(2), OpenMode::Read).await.unwrap();
    assert_eq!(handle.read(Fid(2), 0, 64).await.unwrap(), b"restarted\n");
    handle.clunk(Fid(2)).await.unwrap();

    release_read.notify_one();
    assert_eq!(reader.await.unwrap(), Ok(b"event\n".to_vec()));
    // The pre-restart fid is still stale on subsequent ops.
    assert_eq!(handle.clunk(Fid(1)).await, Err(ErrorCode::Io));
}

#[tokio::test]
async fn supervised_handle_through_srvfs_and_mountfs_rejects_stale_fids_and_rebinds_fresh_walks() {
    let srvfs = Arc::new(SrvFs::new());
    let handle = SupervisedHandle::bound(Arc::new(MemFs::with_read_only_file(
        "version",
        b"v1\n".to_vec(),
    )));
    srvfs
        .post(
            "demo",
            InProcessTransport::new(handle.clone()),
            Access::ReadWrite,
        )
        .await;

    let (tree, access) = srvfs.lookup("demo").await.unwrap();
    let mut ns = Namespace::new();
    ns.mount("/mnt/demo", tree, access);
    let mount_fs = Arc::new(MountFs::new(ns));
    let transport = InProcessTransport::new(mount_fs);

    // Walk a fid through MountFs before restart.
    assert!(matches!(
        transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(10),
                names: vec!["mnt".into(), "demo".into(), "version".into()],
            })
            .await,
        Ok(Response::Walk { .. })
    ));

    // Restart the backing service behind SupervisedHandle.
    handle.deactivate();
    handle.bind(Arc::new(MemFs::with_read_only_file(
        "version",
        b"v2\n".to_vec(),
    )));

    // Pre-restart MountFs fid fails cleanly with ErrorCode::Io.
    assert_eq!(
        transport.call(Request::Stat { fid: Fid(10) }).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        transport
            .call(Request::Open {
                fid: Fid(10),
                mode: OpenMode::Read,
            })
            .await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        transport.call(Request::Clunk { fid: Fid(10) }).await,
        Err(ErrorCode::Io)
    );

    // Fresh walk via Shell over the same MountFs transparently observes generation 2.
    let shell = Shell::new(transport);
    assert_eq!(shell.cat("/mnt/demo/version").await.unwrap(), b"v2\n");
}

#[test]
fn boot_unit_supervision_defaults_to_one_for_one() {
    let unit = BootUnit::parse(&boot_unit_toml("restart = \"on-failure\"\n")).unwrap();
    assert_eq!(unit.supervision, SupervisionStrategy::OneForOne);
    assert_eq!(
        SupervisionStrategy::default(),
        SupervisionStrategy::OneForOne
    );
}

#[test]
fn boot_unit_supervision_parses_and_round_trips_both_variants() {
    let one_for_one = BootUnit::parse(&boot_unit_toml(
        "restart = \"on-failure\"\nsupervision = \"one-for-one\"\n",
    ))
    .unwrap();
    assert_eq!(one_for_one.supervision, SupervisionStrategy::OneForOne);
    let serialized = toml::to_string(&one_for_one).unwrap();
    assert!(serialized.contains("supervision = \"one-for-one\""));
    assert_eq!(BootUnit::parse(&serialized).unwrap(), one_for_one);

    let rest_for_one = BootUnit::parse(&boot_unit_toml(
        "restart = \"on-failure\"\nsupervision = \"rest-for-one\"\n",
    ))
    .unwrap();
    assert_eq!(rest_for_one.supervision, SupervisionStrategy::RestForOne);
    let serialized = toml::to_string(&rest_for_one).unwrap();
    assert!(serialized.contains("supervision = \"rest-for-one\""));
    assert_eq!(BootUnit::parse(&serialized).unwrap(), rest_for_one);

    let always_rest = BootUnit::parse(&boot_unit_toml(
        "restart = \"always\"\nsupervision = \"rest-for-one\"\n",
    ))
    .unwrap();
    assert_eq!(always_rest.restart, RestartPolicy::Always);
    assert_eq!(always_rest.supervision, SupervisionStrategy::RestForOne);
}

#[test]
fn boot_unit_supervision_rejects_invalid_values() {
    for invalid in ["all-for-one", "OneForOne", "RestForOne", "one_for_one", ""] {
        let doc = boot_unit_toml(&format!(
            "restart = \"on-failure\"\nsupervision = \"{invalid}\"\n"
        ));
        assert!(
            BootUnit::parse(&doc).is_err(),
            "expected invalid supervision value `{invalid}` to fail parsing"
        );
    }
}

#[test]
fn boot_unit_rejects_rest_for_one_with_restart_never() {
    let doc = boot_unit_toml("restart = \"never\"\nsupervision = \"rest-for-one\"\n");
    let err = BootUnit::parse(&doc).unwrap_err();
    assert!(
        err.to_string().contains("rest-for-one"),
        "unexpected error: {err:#}"
    );

    // Default one-for-one with restart = "never" remains valid.
    let one_for_one_never = BootUnit::parse(&boot_unit_toml("restart = \"never\"\n")).unwrap();
    assert_eq!(one_for_one_never.restart, RestartPolicy::Never);
    assert_eq!(
        one_for_one_never.supervision,
        SupervisionStrategy::OneForOne
    );
}

#[test]
fn system_boot_manifest_defaults_all_units_to_one_for_one() {
    let manifest = BootManifest::system().unwrap();
    for unit in manifest.ordered() {
        assert_eq!(
            unit.supervision,
            SupervisionStrategy::OneForOne,
            "system unit `{}` should default to OneForOne",
            unit.name
        );
    }
    assert!(
        Duration::from_millis(manifest.get("agent-runtime").unwrap().timeout_ms)
            > Duration::from_millis(0)
    );
}
