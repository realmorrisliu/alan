struct BoundWalkFs {
    inner: StaticFileFs,
    first: tokio::sync::Mutex<Option<bool>>,
    reached: Notify,
    race_clunk: bool,
    clunk_calls: AtomicUsize,
    second_clunk: Notify,
}

#[async_trait::async_trait]
impl FileServer for BoundWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        let first = self.first.lock().await.take();
        if let Some(held) = first {
            self.reached.notify_one();
            if held {
                std::future::pending::<()>().await;
            }
            return Err(ErrorCode::Io);
        }
        Ok(qid)
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        let first = self.clunk_calls.fetch_add(1, Ordering::SeqCst) == 0;
        if self.race_clunk && first {
            // Force pending-Walk cleanup to beat the explicit client clunk.
            self.second_clunk.notified().await;
        }
        let removed = self.inner.fids.lock().await.remove(&fid).is_some();
        if self.race_clunk && !first {
            self.second_clunk.notify_one();
        }
        if removed {
            Ok(())
        } else {
            Err(ErrorCode::NotFound)
        }
    }
    async fn write(&self, _: Fid, _: Offset, _: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
}

#[tokio::test]
async fn backing_walk_bound_before_response_is_cleaned_on_cancel_or_failure() {
    use std::time::Duration;
    for held in [true, false] {
        let backing = Arc::new(BoundWalkFs {
            inner: StaticFileFs {
                content: b"exact",
                fids: tokio::sync::Mutex::new(HashMap::new()),
            },
            first: tokio::sync::Mutex::new(Some(held)),
            reached: Notify::new(),
            race_clunk: false,
            clunk_calls: AtomicUsize::new(0),
            second_clunk: Notify::new(),
        });
        let mut namespace = Namespace::new();
        namespace.mount(
            "/data",
            InProcessTransport::new(backing.clone()),
            Access::ReadWrite,
        );
        let fs = Arc::new(MountFs::new(namespace));
        let owned = fs.clone();
        let job = tokio::spawn(async move {
            owned
                .walk(Fid::ROOT, Fid(100), &["data".into(), "value".into()])
                .await
        });
        let reached =
            tokio::time::timeout(Duration::from_secs(1), backing.reached.notified()).await;
        // Always stop and await the caller before asserting, even if the gate fails.
        if held || reached.is_err() {
            job.abort();
        }
        let joined = tokio::time::timeout(Duration::from_secs(2), job).await;
        reached.expect("backing fid bound before response");
        let joined = joined.expect("walk phase bounded");
        if held {
            assert!(joined.unwrap_err().is_cancelled());
        } else {
            assert!(joined.unwrap().is_err());
        }
        tokio::time::timeout(Duration::from_secs(1), async {
            while !backing.inner.fids.lock().await.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("unpublished backing fid released");
        // A subsequent successful walk transfers ownership rather than clunking
        // its backing fid early, and ordinary outer clunk still releases it.
        fs.walk(Fid::ROOT, Fid(100), &["data".into(), "value".into()])
            .await
            .unwrap();
        fs.open(Fid(100), OpenMode::Read).await.unwrap();
        assert_eq!(fs.read(Fid(100), 0, 100).await.unwrap(), b"exact");
        assert_eq!(backing.inner.fids.lock().await.len(), 1);
        fs.clunk(Fid(100)).await.unwrap();
        assert!(backing.inner.fids.lock().await.is_empty());
    }
}

#[tokio::test]
async fn imported_client_clunk_cancels_pending_walk_without_resurrection() {
    use std::time::Duration;
    use tokio::io::BufReader;
    for synthetic_fallback in [false, true] {
        let backing = Arc::new(BoundWalkFs {
            inner: StaticFileFs {
                content: b"exact",
                fids: tokio::sync::Mutex::new(HashMap::new()),
            },
            first: tokio::sync::Mutex::new(Some(true)),
            reached: Notify::new(),
            race_clunk: true,
            clunk_calls: AtomicUsize::new(0),
            second_clunk: Notify::new(),
        });
        let mut namespace = Namespace::new();
        let names = if synthetic_fallback {
            namespace.mount(
                "/",
                InProcessTransport::new(backing.clone()),
                Access::ReadWrite,
            );
            namespace.mount("/value/child", memfs(), Access::ReadWrite);
            vec!["value".into()]
        } else {
            namespace.mount(
                "/data",
                InProcessTransport::new(backing.clone()),
                Access::ReadWrite,
            );
            vec!["data".into(), "value".into()]
        };
        let fs = Arc::new(MountFs::new(namespace));
        let (client, server) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let exported = fs.clone();
        let export = tokio::spawn(async move {
            alan_ap::export_file_server(exported, BufReader::new(server_read), server_write).await
        });
        let imported = Arc::new(alan_ap::ImportedFileServer::new(
            BufReader::new(client_read),
            client_write,
        ));
        let client = imported.clone();
        let old_names = names.clone();
        let mut caller = Some(tokio::spawn(async move {
            client.walk(Fid::ROOT, Fid(100), &old_names).await
        }));
        let result = tokio::time::timeout(Duration::from_secs(3), async {
            backing.reached.notified().await;
            let bound = backing.inner.fids.lock().await.len();
            let old = caller.take().expect("owned client call");
            old.abort();
            let cancelled = old.await.is_err_and(|error| error.is_cancelled());
            // Dropping the imported waiter alone doesn't cancel its exported
            // server task. The explicit fid clunk must cancel that acquisition.
            imported.clunk(Fid(100)).await?;
            tokio::task::yield_now().await;
            let removed = fs.stat(Fid(100)).await == Err(ErrorCode::NotFound);
            imported.walk(Fid::ROOT, Fid(100), &names).await?;
            imported.open(Fid(100), OpenMode::Read).await?;
            let bytes = imported.read(Fid(100), 0, 100).await?;
            tokio::task::yield_now().await;
            let still_live = fs.stat(Fid(100)).await.is_ok();
            imported.clunk(Fid(100)).await?;
            Ok::<_, ErrorCode>((bound, cancelled, removed, bytes, still_live))
        })
        .await;
        // Await all owned tasks before checking the phase, including failures.
        if let Some(caller) = caller {
            caller.abort();
            let _ = caller.await;
        }
        export.abort();
        let _ = export.await;
        drop(imported);
        tokio::time::timeout(Duration::from_secs(1), async {
            while !backing.inner.fids.lock().await.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("wire backing descriptors released");
        let (bound, cancelled, removed, bytes, still_live) =
            result.expect("wire phase bounded").unwrap();
        assert_eq!(bound, 1);
        assert!(
            cancelled && removed && still_live,
            "no old owner resurrection; synthetic={synthetic_fallback}"
        );
        assert_eq!(bytes, b"exact");
    }
}

#[tokio::test]
async fn completed_walk_clunk_still_propagates_backing_not_found() {
    let backing = Arc::new(BoundWalkFs {
        inner: StaticFileFs {
            content: b"exact",
            fids: tokio::sync::Mutex::new(HashMap::new()),
        },
        first: tokio::sync::Mutex::new(None),
        reached: Notify::new(),
        race_clunk: false,
        clunk_calls: AtomicUsize::new(0),
        second_clunk: Notify::new(),
    });
    let mut namespace = Namespace::new();
    namespace.mount(
        "/data",
        InProcessTransport::new(backing.clone()),
        Access::ReadWrite,
    );
    let fs = MountFs::new(namespace);
    fs.walk(Fid::ROOT, Fid(100), &["data".into(), "value".into()])
        .await
        .unwrap();
    backing.inner.fids.lock().await.clear();
    assert_eq!(fs.clunk(Fid(100)).await, Err(ErrorCode::NotFound));
}
