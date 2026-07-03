//! The [`FileServer`] trait — the aP contract every server implements — and the
//! [`InProcessTransport`] that carries calls to it.
//!
//! Servers implement typed async methods (ergonomic, owned returns); the
//! transport is the seam that turns wire [`Request`]s into method calls and
//! method results into [`Response`]s. v1's transport is in-process and does no
//! serialization (§5.6); a future wire transport substitutes here without
//! changing servers or clients.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{ErrorCode, Fid, FileKind, Offset, OpenMode, Qid, Request, Response, Stat};

/// A file server: the backing implementation of one mountable tree.
///
/// Methods take fids, name components, byte buffers, offsets, and counts and
/// return owned, serializable values or an [`ErrorCode`] (§5.1) — nothing
/// borrowed from server-internal memory, so the same calls can later cross a
/// process boundary. Fid lifecycle is the caller's: `walk`/`create` bind a
/// caller-chosen `newfid`, `open` acts on an existing fid, `clunk` releases one
/// (§5.2).
#[async_trait]
pub trait FileServer: Send + Sync {
    /// Walk `names` from `fid`, binding `newfid` to the destination file.
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode>;

    /// Open `fid` with the given access intent. On a [`FileKind::Clone`] file
    /// this allocates a new resource as a side effect (§5.4); the caller then
    /// `read`s `fid` to learn the allocated name. A denied/absent/rate-limited
    /// open is a dial-time failure (§5.5).
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode>;

    /// Read up to `count` bytes from `offset`. On a [`FileKind::Stream`] this
    /// blocks until bytes at or after `offset` exist (§5.3).
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode>;

    /// Write `data` at `offset`, returning the byte count accepted. Document
    /// entry points commit on `clunk`, never on a partial write.
    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode>;

    /// Return metadata for `fid`.
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode>;

    /// Create child `name` of `kind` under directory `fid`, binding `newfid`.
    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode>;

    /// Remove the file `fid` refers to, then release `fid`.
    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode>;

    /// Release `fid`. For commit-on-clunk document writes this is the commit
    /// point, and MAY return a commit-time [`ErrorCode::BadRequest`] (§5.5).
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode>;
}

/// Receiver for process-output append notifications.
///
/// This is intentionally generic aP-adjacent plumbing: the kernel can publish
/// `/proc/<pid>/io/output` stream changes without knowing which user-space file
/// server, if any, projects those changes into a higher-level view.
#[async_trait]
pub trait ProcessOutputEventSink: Send + Sync {
    async fn output_appended(&self, pid: &str, count: u32);
}

/// Optional event source implemented by file servers that own process output.
#[async_trait]
pub trait ProcessOutputEventSource: Send + Sync {
    async fn subscribe_process_output(
        &self,
        pid: &str,
        sink: Arc<dyn ProcessOutputEventSink>,
    ) -> Result<(), ErrorCode>;
}

/// Receiver for process-input append notifications.
///
/// This mirrors process-output notifications so higher-level views can observe
/// input delivered directly through `/proc/<pid>/io/input`.
#[async_trait]
pub trait ProcessInputEventSink: Send + Sync {
    async fn input_appended(&self, pid: &str, count: u32);
}

/// Optional event source implemented by file servers that own process input.
#[async_trait]
pub trait ProcessInputEventSource: Send + Sync {
    async fn subscribe_process_input(
        &self,
        pid: &str,
        sink: Arc<dyn ProcessInputEventSink>,
    ) -> Result<(), ErrorCode>;
}

/// The in-process fast path: dispatches a wire [`Request`] to a [`FileServer`]
/// and returns its [`Response`] with no serialization (§5.6).
#[derive(Clone)]
pub struct InProcessTransport {
    server: Arc<dyn FileServer>,
}

impl InProcessTransport {
    pub fn new(server: Arc<dyn FileServer>) -> Self {
        Self { server }
    }

    /// Carry one operation to the server. This is the single dispatch point that
    /// maps each `Request` variant onto the matching typed method; a future wire
    /// transport offers the same `call` shape over serialized bytes.
    pub async fn call(&self, request: Request) -> Result<Response, ErrorCode> {
        let s = &self.server;
        match request {
            Request::Walk { fid, newfid, names } => s
                .walk(fid, newfid, &names)
                .await
                .map(|qid| Response::Walk { qid }),
            Request::Open { fid, mode } => {
                s.open(fid, mode).await.map(|qid| Response::Open { qid })
            }
            Request::Read { fid, offset, count } => s
                .read(fid, offset, count)
                .await
                .map(|data| Response::Read { data }),
            Request::Write { fid, offset, data } => s
                .write(fid, offset, &data)
                .await
                .map(|count| Response::Write { count }),
            Request::Stat { fid } => s.stat(fid).await.map(|stat| Response::Stat { stat }),
            Request::Create {
                fid,
                newfid,
                name,
                kind,
            } => s
                .create(fid, newfid, &name, kind)
                .await
                .map(|qid| Response::Create { qid }),
            Request::Remove { fid } => s.remove(fid).await.map(|()| Response::Remove),
            Request::Clunk { fid } => s.clunk(fid).await.map(|()| Response::Clunk),
        }
    }
}
