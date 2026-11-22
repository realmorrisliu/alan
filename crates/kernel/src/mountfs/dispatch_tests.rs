//! Real tagged export acceptance versus fid reservation and clunk.
use super::*;
use std::{collections::HashSet, time::Duration};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    sync::Notify,
};

struct ProbeFs {
    inner: Arc<dyn FileServer>,
    walked: Notify,
    clunked: Notify,
    live: std::sync::Mutex<HashSet<Fid>>,
    walk_calls: AtomicU64,
}
impl ProbeFs {
    fn new(inner: Arc<dyn FileServer>) -> Self {
        Self {
            inner,
            walked: Notify::new(),
            clunked: Notify::new(),
            live: Default::default(),
            walk_calls: AtomicU64::new(0),
        }
    }
}
#[async_trait]
impl FileServer for ProbeFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.walked.notify_one();
        self.walk_calls.fetch_add(1, Ordering::SeqCst);
        let qid = self.inner.walk(fid, newfid, names).await?;
        self.live.lock().unwrap().insert(newfid);
        Ok(qid)
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.clunked.notify_one();
        self.inner.clunk(fid).await?;
        self.live.lock().unwrap().remove(&fid);
        Ok(())
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data).await
    }
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid).await
    }
}

#[tokio::test]
async fn exported_walk_clunk_cannot_precede_accepted_reservation() {
    for synthetic in [false, true] {
        let backing = Arc::new(ProbeFs::new(Arc::new(alan_ap::reference::MemFs::new())));
        let mut ns = Namespace::new();
        ns.mount(
            if synthetic { "/data/child" } else { "/data" },
            InProcessTransport::new(backing.clone()),
            crate::namespace::Access::ReadWrite,
        );
        let fs = Arc::new(MountFs::new(ns));
        let accepted = Arc::new(ProbeFs::new(fs.clone()));
        let (client, server) = tokio::io::duplex(4096);
        let (reader, mut writer) = tokio::io::split(client);
        let (server_reader, server_writer) = tokio::io::split(server);
        let exported = accepted.clone();
        let export = tokio::spawn(async move {
            alan_ap::export_file_server(exported, BufReader::new(server_reader), server_writer)
                .await
        });
        let mut reader = BufReader::new(reader);
        let state = fs.state.lock().await;
        let mut frames = alan_ap::encode_tagged_request_frame(
            1,
            &Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(100),
                names: vec!["data".into()],
            },
        )
        .unwrap();
        frames.extend(
            alan_ap::encode_tagged_request_frame(2, &Request::Clunk { fid: Fid(100) }).unwrap(),
        );
        writer.write_all(&frames).await.unwrap();
        let started = tokio::time::timeout(Duration::from_secs(2), async {
            accepted.walked.notified().await;
            accepted.clunked.notified().await;
        })
        .await;
        // Both real tagged calls reached the owner but cannot yet acquire its lock.
        drop(state);
        let replies = tokio::time::timeout(Duration::from_secs(2), async {
            let mut result = HashMap::new();
            for _ in 0..2 {
                let (tag, response) = alan_ap::read_tagged_response_frame(&mut reader)
                    .await
                    .unwrap()
                    .unwrap();
                result.insert(tag, response);
            }
            result
        })
        .await;
        let installed = fs.state.lock().await.fids.contains_key(&Fid(100));
        // Clean up even the OLD leaking implementation before asserting the RED.
        let _ = fs.clunk(Fid(100)).await;
        export.abort();
        let _ = export.await;
        started.expect("both exported requests must enter without head-of-line blocking");
        let replies = replies.expect("both terminal tagged responses must be returned");
        assert_eq!(
            replies[&2],
            Ok(Response::Clunk),
            "accepted later clunk must see the earlier walk reservation"
        );
        assert!(
            !installed,
            "cancelled fid must not be published after clunk"
        );
        assert!(
            backing.live.lock().unwrap().is_empty(),
            "backing fid must be released"
        );
        fs.walk(Fid::ROOT, Fid(100), &["data".into()])
            .await
            .unwrap();
        fs.clunk(Fid(100)).await.unwrap();
        assert!(
            backing.live.lock().unwrap().is_empty(),
            "legitimate fid reuse must work"
        );
    }
}

#[tokio::test]
async fn ready_walk_cancellation_never_sends_a_late_backing_walk() {
    let backing = Arc::new(ProbeFs::new(Arc::new(alan_ap::reference::MemFs::new())));
    let mut ns = Namespace::new();
    ns.mount(
        "/data",
        InProcessTransport::new(backing.clone()),
        crate::namespace::Access::ReadWrite,
    );
    let resolved = ns.resolve_candidates("/data").remove(0);
    let fs = MountFs::new(ns);
    let mut owner = PendingWalk::new(fs.state.clone(), Fid(100), resolved, Fid(200));
    let (cancel, mut cancelled) = tokio::sync::oneshot::channel();
    cancel.send(()).unwrap();
    let result = owner.wait(&mut cancelled).await;
    owner.close().await;
    assert!(
        result.is_none(),
        "already ready cancellation must win over immediate backing success"
    );
    assert_eq!(
        backing.walk_calls.load(Ordering::SeqCst),
        0,
        "cleanup must not be followed by a fresh backing acquisition"
    );
    assert!(backing.live.lock().unwrap().is_empty());
}

#[tokio::test]
async fn read_only_prefixes_clamp_system_mounts_while_tracking_live_data_mounts() {
    let ctl_tree = Arc::new(alan_ap::reference::MemFs::writable());
    ctl_tree
        .create(Fid::ROOT, Fid(1), "ctl", FileKind::File)
        .await
        .unwrap();
    ctl_tree.clunk(Fid(1)).await.unwrap();

    let sibling_tree = Arc::new(alan_ap::reference::MemFs::writable());
    let live = LiveNamespace::new(Namespace::new());
    live.mount(
        "/mnt/connections",
        InProcessTransport::new(ctl_tree),
        crate::Access::ReadWrite,
    );
    live.mount(
        "/mnt/connections-app",
        InProcessTransport::new(sibling_tree),
        crate::Access::ReadWrite,
    );

    let view = MountFs::from_live_namespace(live.clone())
        .with_read_only_prefixes(["/proc", "/mnt/connections"]);

    // Clamped mount is readable, reports writable=false, and refuses write/create/remove.
    view.walk(
        Fid::ROOT,
        Fid(10),
        &["mnt".into(), "connections".into(), "ctl".into()],
    )
    .await
    .unwrap();
    assert!(!view.stat(Fid(10)).await.unwrap().writable);
    assert_eq!(
        view.open(Fid(10), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        view.open(Fid(10), OpenMode::ReadWrite).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(view.remove(Fid(10)).await, Err(ErrorCode::NoAccess));

    // Non-canonical Walk segments (leading empty segment or slash-joined segment) must stay clamped.
    for (fid_num, names) in [
        (
            15,
            vec![
                "".to_string(),
                "mnt".into(),
                "connections".into(),
                "ctl".into(),
            ],
        ),
        (16, vec!["mnt/connections/ctl".to_string()]),
    ] {
        let fid = Fid(fid_num);
        view.walk(Fid::ROOT, fid, &names).await.unwrap();
        assert!(!view.stat(fid).await.unwrap().writable, "{names:?}");
        assert_eq!(
            view.open(fid, OpenMode::Write).await,
            Err(ErrorCode::NoAccess),
            "{names:?}"
        );
        view.clunk(fid).await.unwrap();
    }

    view.walk(Fid::ROOT, Fid(11), &["mnt".into(), "connections".into()])
        .await
        .unwrap();
    assert_eq!(
        view.create(Fid(11), Fid(12), "new", FileKind::File).await,
        Err(ErrorCode::NoAccess)
    );
    view.clunk(Fid(11)).await.unwrap();

    // Sibling prefix sharing only a string prefix (not component boundary) remains writable.
    view.walk(
        Fid::ROOT,
        Fid(13),
        &["mnt".into(), "connections-app".into()],
    )
    .await
    .unwrap();
    view.create(Fid(13), Fid(14), "note.md", FileKind::File)
        .await
        .unwrap();
    assert!(view.stat(Fid(14)).await.unwrap().writable);
    view.open(Fid(14), OpenMode::Write).await.unwrap();
    view.write(Fid(14), 0, b"ok").await.unwrap();
    view.clunk(Fid(14)).await.unwrap();
    view.clunk(Fid(13)).await.unwrap();

    // Live mount/unmount updates are visible immediately through the same view.
    let dynamic_tree = Arc::new(alan_ap::reference::MemFs::writable());
    live.mount(
        "/mnt/project-live",
        InProcessTransport::new(dynamic_tree),
        crate::Access::ReadWrite,
    );
    view.walk(Fid::ROOT, Fid(20), &["mnt".into(), "project-live".into()])
        .await
        .unwrap();
    view.create(Fid(20), Fid(21), "hello.txt", FileKind::File)
        .await
        .unwrap();
    view.open(Fid(21), OpenMode::Write).await.unwrap();
    view.write(Fid(21), 0, b"live").await.unwrap();
    view.clunk(Fid(21)).await.unwrap();
    view.clunk(Fid(20)).await.unwrap();

    live.unmount("/mnt/project-live");
    assert_eq!(
        view.walk(
            Fid::ROOT,
            Fid(22),
            &["mnt".into(), "project-live".into(), "hello.txt".into()]
        )
        .await,
        Err(ErrorCode::NotFound)
    );
}
