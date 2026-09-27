use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CommitFailure {
    inner: alan_agentfs::AgentFs,
    commit_before_error: bool,
    writes: AtomicUsize,
    commits: AtomicUsize,
    written: Mutex<BTreeSet<Fid>>,
}

#[async_trait]
impl FileServer for CommitFailure {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.inner.walk(fid, newfid, names).await
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let count = self.inner.write(fid, offset, data).await?;
        self.writes.fetch_add(1, Ordering::Relaxed);
        self.written.lock().unwrap().insert(fid);
        Ok(count)
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
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if !self.written.lock().unwrap().remove(&fid) {
            return self.inner.clunk(fid).await;
        }
        self.commits.fetch_add(1, Ordering::Relaxed);
        if self.commit_before_error {
            self.inner.clunk(fid).await?;
            // Model a committed operation whose acknowledgement was lost.
            Err(ErrorCode::Io)
        } else {
            // Reject the close before a buffered input document is committed.
            // Control streams can already have published an effect at write time.
            Err(ErrorCode::BadRequest)
        }
    }
}

#[tokio::test]
async fn commit_failure_never_returns_acceptance_or_replays_unknown_work() {
    for commit_before_error in [false, true] {
        for args in [vec!["submit", "7", "one task"], vec!["continue", "7"]] {
            let fs = Arc::new(CommitFailure {
                inner: alan_agentfs::AgentFs::new(),
                commit_before_error,
                writes: AtomicUsize::new(0),
                commits: AtomicUsize::new(0),
                written: Mutex::new(BTreeSet::new()),
            });
            let mut namespace = Namespace::new();
            namespace.mount(
                "/agent/7",
                InProcessTransport::new(fs.clone()),
                Access::ReadWrite,
            );
            let outcome = AgentWorkProcessRunner
                .run(invocation(namespace, &args))
                .await;
            assert_eq!(outcome.exit_code, 1);
            let receipt: Value = serde_json::from_slice(&outcome.output).unwrap();
            assert_eq!(receipt["success"], false);
            assert!(receipt.get("status").is_none());
            let error = receipt["error"].as_str().unwrap();
            assert!(error.contains("delivery may be unknown"));
            assert_eq!(fs.writes.load(Ordering::Relaxed), 1);
            assert_eq!(fs.commits.load(Ordering::Relaxed), 1);
            let shell = Shell::new(InProcessTransport::new(fs.clone()));
            if args[0] == "submit" {
                let id = error.split_whitespace().nth(1).unwrap();
                uuid::Uuid::parse_str(id).expect("uncertain submission ID remains inspectable");
                let bytes = shell.cat("/io/input").await.unwrap();
                let input = String::from_utf8(bytes).unwrap();
                assert_eq!(input.matches(id).count(), usize::from(commit_before_error));
            } else {
                let bytes = shell.cat("/events").await.unwrap();
                let events = String::from_utf8(bytes).unwrap();
                // Control writes take effect before clunk; failure cannot imply rollback.
                assert_eq!(events.matches("ctl:queue-v1 continue").count(), 1);
            }
        }
    }
}
