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
