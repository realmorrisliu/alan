//! The byte/offset stream primitive (§5.3): an append-only byte log with
//! retained history that backs every aP stream file (a process's `io/output`,
//! an agent's `events`, an `llmfs` generation's `data`).
//!
//! Reads are offset-addressed and resumable, history is retained so a reader
//! that opens late still reads from offset 0, and a read at the live edge blocks
//! until new bytes arrive — observation is a blocking read, not a separate
//! notification primitive (ADR-0024). Record typing (e.g. one JSON object per
//! line) is a consumer convention layered on top, not part of this primitive.

use std::sync::Arc;

use tokio::sync::{Mutex, watch};

use crate::Offset;

/// A shared, cheaply cloneable handle to one append-only byte stream. Clones
/// observe the same underlying log; each reader supplies its own offset, so many
/// consumers watch one stream independently (§5.3).
#[derive(Clone)]
pub struct Stream {
    inner: Arc<Inner>,
}

struct Inner {
    buf: Mutex<Vec<u8>>,
    /// Carries the current length; bumped on every append. Readers waiting at
    /// the live edge wake on a change. `watch` retains the latest value, so an
    /// append between a reader's length check and its await is never lost.
    len_tx: watch::Sender<u64>,
}

impl Default for Stream {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream {
    pub fn new() -> Self {
        let (len_tx, _len_rx) = watch::channel(0u64);
        Self {
            inner: Arc::new(Inner {
                buf: Mutex::new(Vec::new()),
                len_tx,
            }),
        }
    }

    /// Append `bytes` to the log and wake any readers parked at the live edge.
    pub async fn append(&self, bytes: &[u8]) {
        let new_len = {
            let mut buf = self.inner.buf.lock().await;
            buf.extend_from_slice(bytes);
            buf.len() as u64
        };
        // Ignored if there are no receivers; the retained value still updates.
        let _ = self.inner.len_tx.send(new_len);
    }

    /// The number of bytes retained so far.
    pub async fn len(&self) -> u64 {
        self.inner.buf.lock().await.len() as u64
    }

    /// Whether no bytes have been appended yet.
    pub async fn is_empty(&self) -> bool {
        self.inner.buf.lock().await.is_empty()
    }

    /// Read up to `count` bytes starting at `offset`. If `offset` is at or beyond
    /// the live edge, block until bytes at `offset` exist, then return them
    /// (§5.3). Subscribe to length changes *before* the first check so an append
    /// racing the check is observed rather than missed.
    pub async fn read(&self, offset: Offset, count: u32) -> Vec<u8> {
        let mut len_rx = self.inner.len_tx.subscribe();
        loop {
            {
                let buf = self.inner.buf.lock().await;
                let start = offset as usize;
                if start < buf.len() {
                    let end = buf.len().min(start + count as usize);
                    return buf[start..end].to_vec();
                }
            }
            // No bytes at `offset` yet; wait for the next append. A closed
            // channel (stream dropped) ends the wait with no more bytes.
            if len_rx.changed().await.is_err() {
                return Vec::new();
            }
        }
    }
}
