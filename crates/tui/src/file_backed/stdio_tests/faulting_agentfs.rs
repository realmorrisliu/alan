use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

pub(super) struct CloseTailOnPid {
    inner: Arc<dyn FileServer>,
    paths: Mutex<HashMap<Fid, String>>,
    closed_pid: watch::Sender<Option<u64>>,
}

impl CloseTailOnPid {
    pub(super) fn new(inner: Arc<dyn FileServer>) -> Self {
        let (closed_pid, _) = watch::channel(None);
        Self {
            inner,
            paths: Mutex::new(HashMap::new()),
            closed_pid,
        }
    }

    pub(super) fn close(&self, pid: u64) {
        self.closed_pid.send_replace(Some(pid));
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
impl FileServer for CloseTailOnPid {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        let path = self.path_after(fid, names);
        self.paths.lock().unwrap().insert(newfid, path);
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
            return self.inner.read(fid, offset, count).await;
        };
        let mut closed_pid = self.closed_pid.subscribe();
        loop {
            if *closed_pid.borrow() == Some(pid) {
                return Err(ErrorCode::Io);
            }
            tokio::select! {
                result = self.inner.read(fid, offset, count) => return result,
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
