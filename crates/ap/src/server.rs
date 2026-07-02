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
use tokio::io::{AsyncBufRead, AsyncWrite};
use tokio::sync::Mutex;

use crate::wire::{
    read_request_frame, read_response_frame, write_request_frame, write_response_frame,
};
use crate::{ErrorCode, Fid, FileKind, Offset, OpenMode, Qid, Request, Response, Stat, WireError};

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

/// Process IO event direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessIoEventKind {
    Input,
    Output,
}

/// Receiver for ordered process IO append notifications.
///
/// This is the ordered form of the input/output-specific sinks above. A
/// subscriber that binds after IO has already happened can replay retained
/// `/proc/<pid>/io/events` history without grouping records by direction.
#[async_trait]
pub trait ProcessIoEventSink: Send + Sync {
    async fn io_appended(&self, pid: &str, kind: ProcessIoEventKind, count: u32);
}

/// Optional ordered event source implemented by file servers that own process IO.
#[async_trait]
pub trait ProcessIoEventSource: Send + Sync {
    async fn subscribe_process_io(
        &self,
        pid: &str,
        sink: Arc<dyn ProcessIoEventSink>,
    ) -> Result<(), ErrorCode>;
}

/// Ordered process event retained by `/proc` and projected into aggregate views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEvent {
    Input { count: u32 },
    Output { count: u32 },
    Status { status: String },
}

/// Receiver for ordered process lifecycle and IO notifications.
#[async_trait]
pub trait ProcessEventSink: Send + Sync {
    async fn process_event(&self, pid: &str, event: ProcessEvent);
}

/// Optional ordered event source implemented by file servers that own process state.
#[async_trait]
pub trait ProcessEventSource: Send + Sync {
    async fn subscribe_process_events(
        &self,
        pid: &str,
        sink: Arc<dyn ProcessEventSink>,
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

/// Export a [`FileServer`] over the aP byte transport.
///
/// The loop is intentionally simple in v1: one connection carries a serialized
/// sequence of request frames and response-result frames. Multiplexing can be
/// added above this without changing the [`FileServer`] contract.
pub async fn export_file_server<R, W>(
    server: Arc<dyn FileServer>,
    mut reader: R,
    mut writer: W,
) -> Result<(), WireError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let transport = InProcessTransport::new(server);
    while let Some(request) = read_request_frame(&mut reader).await? {
        let result = transport.call(request).await;
        write_response_frame(&mut writer, &result).await?;
    }
    Ok(())
}

/// Client side of one aP wire connection.
pub struct WireTransportClient<R, W> {
    reader: R,
    writer: W,
}

impl<R, W> WireTransportClient<R, W>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    /// Send one request and return the exact remote operation result.
    pub async fn call_result(
        &mut self,
        request: Request,
    ) -> Result<Result<Response, ErrorCode>, WireError> {
        write_request_frame(&mut self.writer, &request).await?;
        read_response_frame(&mut self.reader)
            .await?
            .ok_or(WireError::Closed)
    }

    /// Send one request and map transport failures back into aP error space.
    pub async fn call(&mut self, request: Request) -> Result<Response, ErrorCode> {
        self.call_result(request)
            .await
            .map_err(|error| error.to_error_code())?
    }
}

/// A remote aP tree imported as a normal [`FileServer`].
///
/// V1 serializes requests through one connection. This preserves aP semantics and
/// keeps request IDs out of the first wire slice; a later transport can add
/// multiplexing without changing clients.
pub struct ImportedFileServer<R, W> {
    client: Mutex<WireTransportClient<R, W>>,
}

impl<R, W> ImportedFileServer<R, W>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            client: Mutex::new(WireTransportClient::new(reader, writer)),
        }
    }

    async fn remote_call(&self, request: Request) -> Result<Response, ErrorCode> {
        self.client.lock().await.call(request).await
    }
}

#[async_trait]
impl<R, W> FileServer for ImportedFileServer<R, W>
where
    R: AsyncBufRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        match self
            .remote_call(Request::Walk {
                fid,
                newfid,
                names: names.to_vec(),
            })
            .await?
        {
            Response::Walk { qid } => Ok(qid),
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        match self.remote_call(Request::Open { fid, mode }).await? {
            Response::Open { qid } => Ok(qid),
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        match self
            .remote_call(Request::Read { fid, offset, count })
            .await?
        {
            Response::Read { data } => Ok(data),
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        match self
            .remote_call(Request::Write {
                fid,
                offset,
                data: data.to_vec(),
            })
            .await?
        {
            Response::Write { count } => Ok(count),
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        match self.remote_call(Request::Stat { fid }).await? {
            Response::Stat { stat } => Ok(stat),
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        match self
            .remote_call(Request::Create {
                fid,
                newfid,
                name: name.to_string(),
                kind,
            })
            .await?
        {
            Response::Create { qid } => Ok(qid),
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        match self.remote_call(Request::Remove { fid }).await? {
            Response::Remove => Ok(()),
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        match self.remote_call(Request::Clunk { fid }).await? {
            Response::Clunk => Ok(()),
            _ => Err(ErrorCode::BadRequest),
        }
    }
}
