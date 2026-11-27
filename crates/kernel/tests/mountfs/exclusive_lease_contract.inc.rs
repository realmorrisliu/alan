// Protocol-layer `OpenMode::WriteExclusive` lease enforcement across namespace
// interposition, `clunk`, `clunk_all`, and transport drop (ADR-0063 / Task 2.1, 2.4).

struct ExclusiveTestStreamFs {
    stream: Stream,
    fids: tokio::sync::Mutex<std::collections::HashMap<Fid, (bool, Option<OpenMode>)>>,
    fail_open: std::sync::atomic::AtomicBool,
    open_calls: std::sync::atomic::AtomicUsize,
}

impl ExclusiveTestStreamFs {
    fn new() -> Self {
        Self {
            stream: Stream::new(),
            fids: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            fail_open: std::sync::atomic::AtomicBool::new(false),
            open_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn set_fail_open(&self, fail: bool) {
        self.fail_open
            .store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    fn open_calls(&self) -> usize {
        self.open_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl FileServer for ExclusiveTestStreamFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let is_dir = matches!(
            names.last().map(String::as_str),
            None | Some("1") | Some("machine") | Some("io")
        );
        self.fids.lock().await.insert(newfid, (is_dir, None));
        Ok(Qid {
            kind: if is_dir {
                FileKind::Dir
            } else {
                FileKind::Stream
            },
            version: 0,
            path: if is_dir { 1 } else { 2 },
        })
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.open_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.fail_open.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ErrorCode::Io);
        }
        let mut fids = self.fids.lock().await;
        let (is_dir, open_mode) = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        *open_mode = Some(mode);
        Ok(Qid {
            kind: if *is_dir {
                FileKind::Dir
            } else {
                FileKind::Stream
            },
            version: 0,
            path: if *is_dir { 1 } else { 2 },
        })
    }

    async fn read(&self, _fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(self.stream.read(offset, count).await)
    }

    async fn write(&self, _fid: Fid, _offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.stream.append(data).await;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let is_dir = self
            .fids
            .lock()
            .await
            .get(&fid)
            .map(|(is_dir, _)| *is_dir)
            .unwrap_or(false);
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind: if is_dir {
                    FileKind::Dir
                } else {
                    FileKind::Stream
                },
                version: 0,
                path: if is_dir { 1 } else { 2 },
            },
            length: self.stream.len().await,
            executable: false,
            writable: !is_dir,
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        newfid: Fid,
        _name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let is_dir = kind == FileKind::Dir;
        self.fids.lock().await.insert(newfid, (is_dir, None));
        Ok(Qid {
            kind,
            version: 0,
            path: 3,
        })
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids.lock().await.remove(&fid);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids.lock().await.remove(&fid);
        Ok(())
    }
}

#[tokio::test]
async fn interposing_file_server_cannot_bypass_active_exclusive_lease() {
    let agent_backing = Arc::new(ExclusiveTestStreamFs::new());
    let mut base_ns = Namespace::new();
    base_ns.mount(
        "/agent",
        InProcessTransport::new(agent_backing.clone()),
        Access::ReadWrite,
    );
    let live = LiveNamespace::new(base_ns);
    let engine_fs = MountFs::from_live_namespace(live.clone());

    let tape_path = [
        "agent".to_string(),
        "1".to_string(),
        "machine".to_string(),
        "tape".to_string(),
    ];

    // Owning engine acquires an exclusive write lease on /agent/1/machine/tape.
    engine_fs
        .walk(Fid::ROOT, Fid(1), &tape_path)
        .await
        .unwrap();
    engine_fs
        .open(Fid(1), OpenMode::WriteExclusive)
        .await
        .unwrap();
    engine_fs.write(Fid(1), 0, b"entry-1\n").await.unwrap();

    // Mount a naive interposing FileServer ahead of /agent on the shared live
    // namespace AND another deeper interposer on a derived child namespace.
    let live_interposer = Arc::new(ExclusiveTestStreamFs::new());
    live.mount(
        "/agent",
        InProcessTransport::new(live_interposer.clone()),
        Access::ReadWrite,
    );
    let live_contender = MountFs::from_live_namespace(live.clone());

    let mut child_ns = live.snapshot().child();
    let deep_interposer = Arc::new(ExclusiveTestStreamFs::new());
    child_ns.mount(
        "/agent/1/machine",
        InProcessTransport::new(deep_interposer.clone()),
        Access::ReadWrite,
    );
    let child_contender = MountFs::new(child_ns);

    for (contender, interposer) in [
        (&live_contender, &live_interposer),
        (&child_contender, &deep_interposer),
    ] {
        for (idx, mode) in [
            OpenMode::Write,
            OpenMode::WriteExclusive,
            OpenMode::ReadWrite,
        ]
        .into_iter()
        .enumerate()
        {
            let fid = Fid(10 + idx as u64);
            contender.walk(Fid::ROOT, fid, &tape_path).await.unwrap();
            assert_eq!(
                contender.open(fid, mode).await,
                Err(ErrorCode::NoAccess),
                "interposer must not grant {mode:?} while exclusive lease is active"
            );
            contender.clunk(fid).await.unwrap();
        }
        assert_eq!(
            interposer.open_calls(),
            0,
            "conflicting write open must be rejected at the kernel boundary before reaching interposer"
        );
    }

    // Concurrent readers continue tailing newly appended bytes without blocking.
    // Use a direct view sharing the same namespace lineage so it observes the
    // underlying stream while the exclusive lease is held.
    let mut reader_direct_ns = live.snapshot().child();
    reader_direct_ns.unmount("/agent");
    reader_direct_ns.mount(
        "/agent",
        InProcessTransport::new(agent_backing.clone()),
        Access::ReadWrite,
    );
    let reader_fs = MountFs::new(reader_direct_ns);
    reader_fs
        .walk(Fid::ROOT, Fid(3), &tape_path)
        .await
        .unwrap();
    reader_fs.open(Fid(3), OpenMode::Read).await.unwrap();
    assert_eq!(reader_fs.read(Fid(3), 0, 64).await.unwrap(), b"entry-1\n");

    let tail = tokio::spawn(async move { reader_fs.read(Fid(3), 8, 64).await.unwrap() });
    tokio::task::yield_now().await;
    engine_fs.write(Fid(1), 8, b"entry-2\n").await.unwrap();
    let tailed = tokio::time::timeout(std::time::Duration::from_secs(1), tail)
        .await
        .expect("reader tail must not block under active exclusive lease")
        .unwrap();
    assert_eq!(tailed, b"entry-2\n");

    // Once the holder clunks its fid, a new exclusive open succeeds.
    engine_fs.clunk(Fid(1)).await.unwrap();
    live_contender
        .walk(Fid::ROOT, Fid(30), &tape_path)
        .await
        .unwrap();
    live_contender
        .open(Fid(30), OpenMode::WriteExclusive)
        .await
        .unwrap();
    live_contender.clunk(Fid(30)).await.unwrap();
}

#[tokio::test]
async fn exclusive_lease_releases_on_transport_drop_and_wire_disconnect() {
    let backing = Arc::new(ExclusiveTestStreamFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent",
        InProcessTransport::new(backing.clone()),
        Access::ReadWrite,
    );
    let live = LiveNamespace::new(ns);
    let output_path = [
        "agent".to_string(),
        "1".to_string(),
        "io".to_string(),
        "output".to_string(),
    ];

    // 1. In-process transport drop releases the lease synchronously without explicit clunk.
    let holder_transport = InProcessTransport::new(Arc::new(MountFs::from_live_namespace(
        live.clone(),
    )));
    let peer = MountFs::from_live_namespace(live.clone());

    holder_transport
        .call(alan_ap::Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(1),
            names: output_path.to_vec(),
        })
        .await
        .unwrap();
    holder_transport
        .call(alan_ap::Request::Open {
            fid: Fid(1),
            mode: OpenMode::WriteExclusive,
        })
        .await
        .unwrap();

    peer.walk(Fid::ROOT, Fid(2), &output_path).await.unwrap();
    assert_eq!(
        peer.open(Fid(2), OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );

    drop(holder_transport);

    peer.open(Fid(2), OpenMode::WriteExclusive)
        .await
        .expect("dropping the holding transport must release the exclusive lease");
    peer.clunk(Fid(2)).await.unwrap();

    // 2. Wire disconnect triggers `clunk_all` / server drop and releases the lease.
    let exported_fs = Arc::new(MountFs::from_live_namespace(live.clone()));
    let (client_io, server_io) = tokio::io::duplex(4096);
    let (client_read, client_write) = tokio::io::split(client_io);
    let (server_read, server_write) = tokio::io::split(server_io);
    let server_fs = exported_fs.clone();
    let server_task = tokio::spawn(async move {
        let _ = alan_ap::export_file_server(
            server_fs.clone(),
            tokio::io::BufReader::new(server_read),
            server_write,
        )
        .await;
        server_fs.clunk_all().await;
    });

    let imported = alan_ap::ImportedFileServer::new(
        tokio::io::BufReader::new(client_read),
        client_write,
    );
    imported
        .walk(Fid::ROOT, Fid(5), &output_path)
        .await
        .unwrap();
    imported
        .open(Fid(5), OpenMode::WriteExclusive)
        .await
        .unwrap();

    peer.walk(Fid::ROOT, Fid(6), &output_path).await.unwrap();
    assert_eq!(
        peer.open(Fid(6), OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );

    // Drop the wire client without clunking Fid(5).
    drop(imported);
    server_task.await.unwrap();

    peer.open(Fid(6), OpenMode::WriteExclusive)
        .await
        .expect("wire disconnect must release the exclusive lease");
    peer.clunk(Fid(6)).await.unwrap();
}

#[tokio::test]
async fn exclusive_lease_enforces_aliases_shared_writers_and_read_only_boundaries() {
    let backing = Arc::new(ExclusiveTestStreamFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent",
        InProcessTransport::new(backing.clone()),
        Access::ReadWrite,
    );
    ns.bind("/alias-agent", "/agent");
    let projected = ns
        .project_mounts([("/projected-agent", "/agent", Access::ReadWrite)])
        .unwrap();

    let fs = MountFs::new(ns.clone());
    let projected_fs = MountFs::new(projected);
    let ro_fs = MountFs::new(ns.child()).with_read_only_prefixes(["/agent"]);

    let tape_path = [
        "agent".to_string(),
        "1".to_string(),
        "machine".to_string(),
        "tape".to_string(),
    ];
    let alias_path = [
        "alias-agent".to_string(),
        "1".to_string(),
        "machine".to_string(),
        "tape".to_string(),
    ];
    let projected_path = [
        "projected-agent".to_string(),
        "1".to_string(),
        "machine".to_string(),
        "tape".to_string(),
    ];
    let non_canonical_path = ["".to_string(), "agent/1/./machine/tape".to_string()];

    // Read-only clamped view and synthetic directories reject WriteExclusive at dial time.
    ro_fs.walk(Fid::ROOT, Fid(1), &tape_path).await.unwrap();
    assert_eq!(
        ro_fs.open(Fid(1), OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );
    ro_fs.clunk(Fid(1)).await.unwrap();
    assert_eq!(
        fs.open(Fid::ROOT, OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );

    // Non-exclusive writers coexist, but block WriteExclusive until all clunk.
    fs.walk(Fid::ROOT, Fid(2), &tape_path).await.unwrap();
    fs.walk(Fid::ROOT, Fid(3), &tape_path).await.unwrap();
    fs.walk(Fid::ROOT, Fid(4), &tape_path).await.unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.open(Fid(3), OpenMode::ReadWrite).await.unwrap();
    assert_eq!(
        fs.open(Fid(4), OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(2)).await.unwrap();
    assert_eq!(
        fs.open(Fid(4), OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(3)).await.unwrap();

    // Failed backing open must not leak a lease.
    backing.set_fail_open(true);
    assert_eq!(
        fs.open(Fid(4), OpenMode::WriteExclusive).await,
        Err(ErrorCode::Io)
    );
    backing.set_fail_open(false);

    // Acquiring WriteExclusive blocks bound aliases, projected mounts, and non-canonical walks.
    fs.open(Fid(4), OpenMode::WriteExclusive).await.unwrap();

    fs.walk(Fid::ROOT, Fid(5), &alias_path).await.unwrap();
    assert_eq!(
        fs.open(Fid(5), OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(5)).await.unwrap();

    projected_fs
        .walk(Fid::ROOT, Fid(6), &projected_path)
        .await
        .unwrap();
    assert_eq!(
        projected_fs.open(Fid(6), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    projected_fs.clunk(Fid(6)).await.unwrap();

    fs.walk(Fid::ROOT, Fid(7), &non_canonical_path)
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(7), OpenMode::ReadWrite).await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(7)).await.unwrap();

    fs.clunk(Fid(4)).await.unwrap();

    // After clunk, projected alias can acquire WriteExclusive cleanly.
    projected_fs
        .walk(Fid::ROOT, Fid(8), &projected_path)
        .await
        .unwrap();
    projected_fs
        .open(Fid(8), OpenMode::WriteExclusive)
        .await
        .unwrap();
    projected_fs.clunk(Fid(8)).await.unwrap();
}
