//! Bounded wire-framing reader and supervised `FileServer` proxy for external services.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, MAX_WIRE_FRAME_BYTES, Offset, OpenMode, Qid, Stat,
};
use async_trait::async_trait;
use tokio::io::{AsyncBufRead, AsyncRead};
use tokio::sync::{Mutex, RwLock};

pub(super) struct SupervisedFileserver {
    inner: RwLock<Option<Arc<dyn FileServer>>>,
    timeout: Duration,
    pub(super) max_payload_chunk: usize,
    stream_fids: Mutex<HashSet<Fid>>,
}

impl SupervisedFileserver {
    pub(super) fn new(
        inner: Arc<dyn FileServer>,
        timeout: Duration,
        max_message_bytes: usize,
    ) -> Self {
        // JSON byte arrays can take up to 5 bytes per byte ("255, "), plus envelope overhead.
        let max_payload_chunk =
            (max_message_bytes.saturating_sub(256) / 5).clamp(128, MAX_WIRE_FRAME_BYTES / 8);
        Self {
            inner: RwLock::new(Some(inner)),
            timeout,
            max_payload_chunk,
            stream_fids: Mutex::new(HashSet::new()),
        }
    }

    pub(super) async fn deactivate(&self) {
        *self.inner.write().await = None;
        self.stream_fids.lock().await.clear();
    }

    async fn server(&self) -> Result<Arc<dyn FileServer>, ErrorCode> {
        self.inner
            .read()
            .await
            .as_ref()
            .cloned()
            .ok_or(ErrorCode::NoAccess)
    }

    async fn timed<T>(
        &self,
        fut: impl std::future::Future<Output = Result<T, ErrorCode>>,
    ) -> Result<T, ErrorCode> {
        tokio::time::timeout(self.timeout, fut)
            .await
            .map_err(|_| ErrorCode::Io)?
    }

    async fn track_stream_fid(&self, fid: Fid, qid: Qid) {
        if qid.kind == FileKind::Stream {
            self.stream_fids.lock().await.insert(fid);
        }
    }
}

#[async_trait]
impl FileServer for SupervisedFileserver {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self
            .timed(self.server().await?.walk(fid, newfid, names))
            .await?;
        self.track_stream_fid(newfid, qid).await;
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let qid = self.timed(self.server().await?.open(fid, mode)).await?;
        self.track_stream_fid(fid, qid).await;
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let server = self.server().await?;
        let bounded = count.min(self.max_payload_chunk as u32);
        if self.stream_fids.lock().await.contains(&fid) {
            server.read(fid, offset, bounded).await
        } else {
            self.timed(server.read(fid, offset, bounded)).await
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let server = self.server().await?;
        if data.is_empty() {
            return self.timed(server.write(fid, offset, data)).await;
        }
        let mut accepted_total = 0usize;
        for chunk in data.chunks(self.max_payload_chunk) {
            let off = offset
                .checked_add(accepted_total as u64)
                .ok_or(ErrorCode::BadRequest)?;
            let accepted = self.timed(server.write(fid, off, chunk)).await? as usize;
            if accepted > chunk.len() {
                return Err(ErrorCode::BadRequest);
            }
            accepted_total = accepted_total
                .checked_add(accepted)
                .ok_or(ErrorCode::BadRequest)?;
            if accepted < chunk.len() {
                break;
            }
        }
        u32::try_from(accepted_total).map_err(|_| ErrorCode::BadRequest)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.timed(self.server().await?.stat(fid)).await
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let qid = self
            .timed(self.server().await?.create(fid, newfid, name, kind))
            .await?;
        self.track_stream_fid(newfid, qid).await;
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.stream_fids.lock().await.remove(&fid);
        self.timed(self.server().await?.remove(fid)).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.stream_fids.lock().await.remove(&fid);
        self.timed(self.server().await?.clunk(fid)).await
    }
}

/// Enforces `max_message_bytes` per newline-delimited wire response frame.
pub(super) struct BoundedLineReader<R> {
    inner: R,
    max_bytes: usize,
    line_bytes: usize,
    last_nl_offset: Option<usize>,
}

impl<R> BoundedLineReader<R> {
    pub(super) fn new(inner: R, max_bytes: usize) -> Self {
        Self {
            inner,
            max_bytes,
            line_bytes: 0,
            last_nl_offset: None,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for BoundedLineReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<R: AsyncBufRead + Unpin> AsyncBufRead for BoundedLineReader<R> {
    fn poll_fill_buf(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<&[u8]>> {
        let me = self.get_mut();
        let buf = std::task::ready!(Pin::new(&mut me.inner).poll_fill_buf(cx))?;
        let (frame_len, slice, nl_offset) = match buf.iter().position(|b| *b == b'\n') {
            Some(nl) => (nl + 1, &buf[..=nl], Some(nl)),
            None => (buf.len(), buf, None),
        };
        if me.line_bytes.saturating_add(frame_len) > me.max_bytes {
            let msg = format!("response frame exceeded {} bytes", me.max_bytes);
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                msg,
            )));
        }
        me.last_nl_offset = nl_offset;
        Poll::Ready(Ok(slice))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.get_mut();
        let done = me.last_nl_offset.is_some_and(|nl| amt > nl);
        me.line_bytes = if done {
            0
        } else {
            me.line_bytes.saturating_add(amt)
        };
        me.last_nl_offset = None;
        Pin::new(&mut me.inner).consume(amt);
    }
}
