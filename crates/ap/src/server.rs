//! The [`FileServer`] trait — the aP contract every server implements — and the
//! [`InProcessTransport`] that carries calls to it.
//!
//! Servers implement typed async methods (ergonomic, owned returns); the
//! transport is the seam that turns wire [`Request`]s into method calls and
//! method results into [`Response`]s. v1's transport is in-process and does no
//! serialization (§5.6); a future wire transport substitutes here without
//! changing servers or clients.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use tokio::io::{AsyncBufRead, AsyncWrite};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

use crate::wire::{
    MAX_WIRE_FRAME_BYTES, WireTag, read_tagged_request_frame, read_tagged_response_frame,
    write_tagged_request_frame, write_tagged_response_frame,
};
use crate::{ErrorCode, Fid, FileKind, Offset, OpenMode, Qid, Request, Response, Stat, WireError};

type PendingResponses =
    Arc<StdMutex<HashMap<WireTag, oneshot::Sender<Result<Response, ErrorCode>>>>>;

struct PendingResponseGuard {
    pending: PendingResponses,
    tag: WireTag,
}

impl Drop for PendingResponseGuard {
    fn drop(&mut self) {
        self.pending
            .lock()
            .expect("pending response mutex poisoned")
            .remove(&self.tag);
    }
}

struct ConnectionWriteGuard {
    closed: Arc<AtomicBool>,
    pending: PendingResponses,
    completed: bool,
}

impl Drop for ConnectionWriteGuard {
    fn drop(&mut self) {
        if !self.completed {
            close_imported_connection(&self.closed, &self.pending);
        }
    }
}

fn close_imported_connection(closed: &AtomicBool, pending: &PendingResponses) {
    closed.store(true, Ordering::Release);
    for (_, waiter) in pending
        .lock()
        .expect("pending response mutex poisoned")
        .drain()
    {
        let _ = waiter.send(Err(ErrorCode::Io));
    }
}

const MAX_WIRE_PAYLOAD_CHUNK_BYTES: usize = MAX_WIRE_FRAME_BYTES / 8;

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
/// One connection carries tagged request/response frames. Requests execute
/// independently so a blocking stream read does not prevent unrelated fids from
/// making progress on the same attachment.
pub async fn export_file_server<R, W>(
    server: Arc<dyn FileServer>,
    reader: R,
    mut writer: W,
) -> Result<(), WireError>
where
    R: AsyncBufRead + Send + Unpin + 'static,
    W: AsyncWrite + Unpin,
{
    let transport = InProcessTransport::new(server);
    let (request_tx, mut request_rx) = mpsc::channel(32);
    let reader_task = AbortOnDrop::new(tokio::spawn(read_export_requests(reader, request_tx)));
    let mut calls = JoinSet::new();

    let result = async {
        loop {
            tokio::select! {
                message = request_rx.recv() => {
                    match message {
                        Some(Ok(ExportReaderMessage::Request { tag, request })) => {
                            let transport = transport.clone();
                            calls.spawn(async move { (tag, transport.call(request).await) });
                        }
                        Some(Err(error)) => break Err(error),
                        None => break Ok(()),
                    }
                }
                result = calls.join_next(), if !calls.is_empty() => {
                    let (tag, result) = result
                        .expect("aP export call set was non-empty")
                        .map_err(|error| WireError::Io(std::io::Error::other(error)))?;
                    write_tagged_response_frame(&mut writer, tag, &result).await?;
                }
            }
        }
    }
    .await;

    reader_task.abort_and_join().await;
    result
}

struct AbortOnDrop {
    handle: Option<JoinHandle<()>>,
}

impl AbortOnDrop {
    fn new(handle: JoinHandle<()>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    async fn abort_and_join(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}

enum ExportReaderMessage {
    Request { tag: WireTag, request: Request },
}

async fn read_export_requests<R>(
    mut reader: R,
    request_tx: mpsc::Sender<Result<ExportReaderMessage, WireError>>,
) where
    R: AsyncBufRead + Unpin,
{
    loop {
        match read_tagged_request_frame(&mut reader).await {
            Ok(Some((tag, request))) => {
                if request_tx
                    .send(Ok(ExportReaderMessage::Request { tag, request }))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Ok(None) => break,
            Err(error) => {
                let _ = request_tx.send(Err(error)).await;
                break;
            }
        }
    }
}

/// Client side of one aP wire connection.
pub struct WireTransportClient<R, W> {
    reader: R,
    writer: W,
    in_flight: bool,
    next_tag: WireTag,
}

impl<R, W> WireTransportClient<R, W>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            in_flight: false,
            next_tag: 1,
        }
    }

    /// Send one request and return the exact remote operation result.
    pub async fn call_result(
        &mut self,
        request: Request,
    ) -> Result<Result<Response, ErrorCode>, WireError> {
        if self.in_flight {
            return Err(WireError::Unsynchronized);
        }
        self.in_flight = true;
        let result = self.call_result_in_flight(request).await;
        if result.is_ok() {
            self.in_flight = false;
        }
        result
    }

    async fn call_result_in_flight(
        &mut self,
        request: Request,
    ) -> Result<Result<Response, ErrorCode>, WireError> {
        let tag = self.next_tag;
        self.next_tag = self.next_tag.wrapping_add(1).max(1);
        write_tagged_request_frame(&mut self.writer, tag, &request).await?;
        let (response_tag, result) = read_tagged_response_frame(&mut self.reader)
            .await?
            .ok_or(WireError::Closed)?;
        if response_tag != tag {
            return Err(WireError::Unsynchronized);
        }
        Ok(result)
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
/// Calls are multiplexed through one connection. A background response router
/// pairs tagged results with callers, while a short writer lock keeps frames from
/// interleaving. Waiting on one response never holds the writer or blocks another
/// call.
pub struct ImportedFileServer<R, W> {
    writer: Mutex<W>,
    pending: PendingResponses,
    next_tag: AtomicU64,
    closed: Arc<AtomicBool>,
    reader_task: JoinHandle<()>,
    reader: PhantomData<fn() -> R>,
}

impl<R, W> ImportedFileServer<R, W>
where
    R: AsyncBufRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    pub fn new(reader: R, writer: W) -> Self {
        let pending = Arc::new(StdMutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let reader_task = tokio::spawn(route_imported_responses(
            reader,
            Arc::clone(&pending),
            Arc::clone(&closed),
        ));
        Self {
            writer: Mutex::new(writer),
            pending,
            next_tag: AtomicU64::new(1),
            closed,
            reader_task,
            reader: PhantomData,
        }
    }

    async fn remote_call(&self, request: Request) -> Result<Response, ErrorCode> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ErrorCode::Io);
        }

        let tag = self.next_tag.fetch_add(1, Ordering::Relaxed);
        let (result_tx, result_rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("pending response mutex poisoned")
            .insert(tag, result_tx);
        let _pending_guard = PendingResponseGuard {
            pending: Arc::clone(&self.pending),
            tag,
        };

        let write_result = {
            let mut writer = self.writer.lock().await;
            if self.closed.load(Ordering::Acquire) {
                Err(WireError::Closed)
            } else {
                let mut guard = ConnectionWriteGuard {
                    closed: Arc::clone(&self.closed),
                    pending: Arc::clone(&self.pending),
                    completed: false,
                };
                let result = write_tagged_request_frame(&mut *writer, tag, &request).await;
                guard.completed = result.is_ok();
                result
            }
        };
        if let Err(error) = write_result {
            return Err(error.to_error_code());
        }
        result_rx.await.map_err(|_| ErrorCode::Io)?
    }
}

impl<R, W> Drop for ImportedFileServer<R, W> {
    fn drop(&mut self) {
        self.reader_task.abort();
    }
}

async fn route_imported_responses<R>(
    mut reader: R,
    pending: PendingResponses,
    closed: Arc<AtomicBool>,
) where
    R: AsyncBufRead + Unpin,
{
    while let Ok(Some((tag, result))) = read_tagged_response_frame(&mut reader).await {
        if let Some(waiter) = pending
            .lock()
            .expect("pending response mutex poisoned")
            .remove(&tag)
        {
            let _ = waiter.send(result);
        }
    }

    close_imported_connection(&closed, &pending);
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
        let count = count.min(MAX_WIRE_PAYLOAD_CHUNK_BYTES as u32);
        match self
            .remote_call(Request::Read { fid, offset, count })
            .await?
        {
            Response::Read { data } => Ok(data),
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        if data.is_empty() {
            return match self
                .remote_call(Request::Write {
                    fid,
                    offset,
                    data: Vec::new(),
                })
                .await?
            {
                Response::Write { count } => Ok(count),
                _ => Err(ErrorCode::BadRequest),
            };
        }

        let mut accepted_total = 0usize;
        let accepted_count = |accepted_total: usize| {
            u32::try_from(accepted_total).map_err(|_| ErrorCode::BadRequest)
        };
        for chunk in data.chunks(MAX_WIRE_PAYLOAD_CHUNK_BYTES) {
            let chunk_offset = offset
                .checked_add(accepted_total as u64)
                .ok_or(ErrorCode::BadRequest)?;
            let response = match self
                .remote_call(Request::Write {
                    fid,
                    offset: chunk_offset,
                    data: chunk.to_vec(),
                })
                .await
            {
                Ok(response) => response,
                Err(_) if accepted_total > 0 => return accepted_count(accepted_total),
                Err(error) => return Err(error),
            };
            let accepted = match response {
                Response::Write { count } => count as usize,
                _ if accepted_total > 0 => return accepted_count(accepted_total),
                _ => return Err(ErrorCode::BadRequest),
            };
            if accepted > chunk.len() {
                return if accepted_total > 0 {
                    accepted_count(accepted_total)
                } else {
                    Err(ErrorCode::BadRequest)
                };
            }
            accepted_total = accepted_total
                .checked_add(accepted)
                .ok_or(ErrorCode::BadRequest)?;
            if accepted < chunk.len() {
                break;
            }
        }
        accepted_count(accepted_total)
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

#[cfg(test)]
mod imported_file_server_tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncWrite, BufReader, duplex};
    use tokio::sync::Notify;

    struct PartialThenPendingWriter {
        wrote_prefix: Arc<Notify>,
        wrote_once: bool,
    }

    impl AsyncWrite for PartialThenPendingWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            if self.wrote_once {
                return Poll::Pending;
            }
            self.wrote_once = true;
            self.wrote_prefix.notify_one();
            Poll::Ready(Ok(bytes.len().min(1)))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn cancelled_remote_call_removes_pending_response() {
        let (client_stream, server_stream) = duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_stream);
        let (server_read, _server_write) = tokio::io::split(server_stream);
        let imported = Arc::new(ImportedFileServer::new(
            BufReader::new(client_read),
            client_write,
        ));

        let caller = Arc::clone(&imported);
        let call =
            tokio::spawn(async move { caller.remote_call(Request::Stat { fid: Fid(1) }).await });
        read_tagged_request_frame(&mut BufReader::new(server_read))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            imported
                .pending
                .lock()
                .expect("pending response mutex poisoned")
                .len(),
            1
        );

        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
        assert!(
            imported
                .pending
                .lock()
                .expect("pending response mutex poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn cancelled_partial_frame_write_closes_the_connection() {
        let (client_read, server_stream) = duplex(4096);
        let wrote_prefix = Arc::new(Notify::new());
        let imported = Arc::new(ImportedFileServer::new(
            BufReader::new(client_read),
            PartialThenPendingWriter {
                wrote_prefix: Arc::clone(&wrote_prefix),
                wrote_once: false,
            },
        ));

        let caller = Arc::clone(&imported);
        let call =
            tokio::spawn(async move { caller.remote_call(Request::Stat { fid: Fid(1) }).await });
        wrote_prefix.notified().await;
        call.abort();
        assert!(call.await.unwrap_err().is_cancelled());
        assert_eq!(
            imported.remote_call(Request::Stat { fid: Fid(2) }).await,
            Err(ErrorCode::Io)
        );

        drop(server_stream);
    }
}
