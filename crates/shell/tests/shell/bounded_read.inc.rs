/// A bound fid remains visible while acquisition or IO is held indefinitely.
struct HeldReadFs {
    inner: EchoFs,
    phase: &'static str,
    reached: tokio::sync::Notify,
}
impl HeldReadFs {
    async fn hold(&self, phase: &str) {
        if self.phase == phase {
            self.reached.notify_one();
            std::future::pending::<()>().await;
        }
    }
}
#[async_trait::async_trait]
impl FileServer for HeldReadFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        self.hold("walk").await;
        Ok(qid)
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.hold("open").await;
        self.inner.open(fid, mode).await
    }
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.hold("stat").await;
        self.inner.stat(fid).await
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.hold("read").await;
        self.inner.read(fid, offset, count).await
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.clunk(fid).await?;
        self.hold("close").await;
        Ok(())
    }
    async fn write(&self, fid: Fid, offset: Offset, bytes: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, bytes).await
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
}

#[tokio::test]
async fn bounded_read_cleans_bound_fids_at_every_held_io_phase() {
    for phase in ["walk", "open", "stat", "read", "close"] {
        let server = Arc::new(HeldReadFs {
            inner: EchoFs::new(),
            phase,
            reached: tokio::sync::Notify::new(),
        });
        server.inner.state.lock().await.buf = b"payload".to_vec();
        let shell = Shell::new(InProcessTransport::new(server.clone()));
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            shell.read_range_bounded("/buf", 0, None, 100, Duration::from_millis(20)),
        )
        .await
        .expect("whole operation and cleanup bounded");
        assert_eq!(result, Err(ErrorCode::Io), "held {phase}");
        assert!(
            server.inner.state.lock().await.fids.is_empty(),
            "{phase} leaked fid"
        );
    }
}

#[tokio::test]
async fn bounded_read_abort_during_walk_still_releases_bound_fid() {
    let server = Arc::new(HeldReadFs {
        inner: EchoFs::new(),
        phase: "walk",
        reached: tokio::sync::Notify::new(),
    });
    let shell = Shell::new(InProcessTransport::new(server.clone()));
    let job = tokio::spawn(async move {
        shell
            .read_range_bounded("/buf", 0, None, 100, Duration::from_secs(5))
            .await
    });
    tokio::time::timeout(Duration::from_secs(1), server.reached.notified())
        .await
        .unwrap();
    assert_eq!(server.inner.state.lock().await.fids.len(), 1);
    job.abort();
    assert!(job.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(1), async {
        while !server.inner.state.lock().await.fids.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("aborted caller cleanup");
}

#[tokio::test]
async fn bounded_read_preserves_exact_range_and_rejects_invalid_bounds() {
    let server = Arc::new(EchoFs::new());
    server.state.lock().await.buf = "α\nexact body".as_bytes().to_vec();
    let shell = Shell::new(InProcessTransport::new(server.clone()));
    let timeout = Duration::from_secs(1);
    assert_eq!(
        shell
            .read_range_bounded("/buf", 0, None, 100, timeout)
            .await
            .unwrap(),
        "α\nexact body".as_bytes()
    );
    assert_eq!(
        shell
            .read_range_bounded("/buf", 3, Some(5), 5, timeout)
            .await
            .unwrap(),
        b"exact"
    );
    for (offset, length, budget, error) in [
        (0, None, 2, ErrorCode::BadRequest),
        (100, None, 100, ErrorCode::NotFound),
        (3, Some(100), 100, ErrorCode::NotFound),
    ] {
        assert_eq!(
            shell
                .read_range_bounded("/buf", offset, length, budget, timeout)
                .await,
            Err(error)
        );
        assert!(server.state.lock().await.fids.is_empty());
    }
}
