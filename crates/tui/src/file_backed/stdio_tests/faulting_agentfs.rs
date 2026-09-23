use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{oneshot, watch};

struct WalkPause {
    suffix: String,
    reached: oneshot::Sender<()>,
    resume: oneshot::Receiver<()>,
}

struct ReadPause {
    suffix: String,
    remaining_matches: usize,
    reached: oneshot::Sender<()>,
    resume: oneshot::Receiver<()>,
}

pub(crate) struct FaultingFileServer {
    inner: Arc<dyn FileServer>,
    paths: Mutex<HashMap<Fid, String>>,
    closed_pid: watch::Sender<Option<u64>>,
    walk_pause: Mutex<Option<WalkPause>>,
    read_pause: Mutex<Option<ReadPause>>,
}

impl FaultingFileServer {
    pub(crate) fn new(inner: Arc<dyn FileServer>) -> Self {
        let (closed_pid, _) = watch::channel(None);
        Self {
            inner,
            paths: Mutex::new(HashMap::new()),
            closed_pid,
            walk_pause: Mutex::new(None),
            read_pause: Mutex::new(None),
        }
    }

    pub(crate) fn pause_next_walk_with_suffix(
        &self,
        suffix: &str,
    ) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (reached_tx, reached_rx) = oneshot::channel();
        let (resume_tx, resume_rx) = oneshot::channel();
        *self.walk_pause.lock().unwrap() = Some(WalkPause {
            suffix: suffix.to_string(),
            reached: reached_tx,
            resume: resume_rx,
        });
        (reached_rx, resume_tx)
    }

    pub(crate) fn pause_read_after_matching_reads(
        &self,
        suffix: &str,
        matching_reads: usize,
    ) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (reached_tx, reached_rx) = oneshot::channel();
        let (resume_tx, resume_rx) = oneshot::channel();
        *self.read_pause.lock().unwrap() = Some(ReadPause {
            suffix: suffix.to_string(),
            remaining_matches: matching_reads.max(1),
            reached: reached_tx,
            resume: resume_rx,
        });
        (reached_rx, resume_tx)
    }

    pub(crate) fn close(&self, pid: u64) {
        self.closed_pid.send_replace(Some(pid));
    }

    async fn pause_matching_read(&self, path: &str) {
        let pause = {
            let mut pending = self.read_pause.lock().unwrap();
            let should_pause = pending.as_mut().is_some_and(|pause| {
                if path.ends_with(&pause.suffix) {
                    pause.remaining_matches -= 1;
                    pause.remaining_matches == 0
                } else {
                    false
                }
            });
            should_pause.then(|| pending.take()).flatten()
        };
        if let Some(pause) = pause {
            let _ = pause.reached.send(());
            let _ = pause.resume.await;
        }
    }

    fn path_after(&self, fid: Fid, names: &[String]) -> String {
        let mut path = self
            .paths
            .lock()
            .unwrap()
            .get(&fid)
            .cloned()
            .unwrap_or_default();
        for name in names {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(name);
        }
        path
    }
}

#[async_trait::async_trait]
impl FileServer for FaultingFileServer {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        let path = self.path_after(fid, names);
        self.paths.lock().unwrap().insert(newfid, path.clone());
        let pause = {
            let mut pending = self.walk_pause.lock().unwrap();
            if pending
                .as_ref()
                .is_some_and(|pause| path.ends_with(&pause.suffix))
            {
                pending.take()
            } else {
                None
            }
        };
        if let Some(pause) = pause {
            let _ = pause.reached.send(());
            let _ = pause.resume.await;
        }
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let path = self
            .paths
            .lock()
            .unwrap()
            .get(&fid)
            .cloned()
            .unwrap_or_default();
        let is_agent_tail = path.ends_with("/machine/tape") || path.ends_with("/machine/ui/events");
        let Some(pid) = is_agent_tail
            .then(|| path.split('/').next()?.parse::<u64>().ok())
            .flatten()
        else {
            let bytes = self.inner.read(fid, offset, count).await?;
            self.pause_matching_read(&path).await;
            return Ok(bytes);
        };
        let mut closed_pid = self.closed_pid.subscribe();
        loop {
            if *closed_pid.borrow() == Some(pid) {
                return Err(ErrorCode::Io);
            }
            tokio::select! {
                result = self.inner.read(fid, offset, count) => {
                    let bytes = result?;
                    self.pause_matching_read(&path).await;
                    return Ok(bytes);
                },
                changed = closed_pid.changed() => {
                    if changed.is_err() {
                        return Err(ErrorCode::Io);
                    }
                }
            }
        }
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
        let qid = self.inner.create(fid, newfid, name, kind).await?;
        let path = self.path_after(fid, &[name.to_string()]);
        self.paths.lock().unwrap().insert(newfid, path);
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let result = self.inner.remove(fid).await;
        self.paths.lock().unwrap().remove(&fid);
        result
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        let result = self.inner.clunk(fid).await;
        self.paths.lock().unwrap().remove(&fid);
        result
    }
}
