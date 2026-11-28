//! Single-owner actor adapter for in-process [`FileServer`] implementations.
//!
//! [`FileServerActor`] lets an in-process file server handle all aP requests
//! synchronously with `&mut self` inside a dedicated Tokio task, avoiding shared
//! `tokio::sync::Mutex` lock hazards across `.await` boundaries and isolating
//! panics to the failing server's owned state (ADR-0063).
//!
//! Blocking [`FileKind::Stream`] reads return [`ActorOutcome::ReadStream`] so
//! the proxy awaits future bytes outside the actor mailbox without stalling
//! concurrent `walk`, `stat`, `write`, or `clunk` calls.

use std::any::Any;
use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot, watch};

use crate::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream};

/// Outcome returned by [`FileServerActor::read`].
///
/// Ordinary byte files and directory listings return [`ActorOutcome::Ready`]
/// directly from the synchronous actor loop. [`FileKind::Stream`] files return
/// [`ActorOutcome::ReadStream`] so the proxy awaits future bytes outside the
/// actor mailbox without stalling concurrent callers.
pub enum ActorOutcome {
    /// Bytes are already available; reply to the caller immediately.
    Ready(Vec<u8>),
    /// Offload a potentially blocking [`Stream::read`] outside the actor loop.
    ReadStream {
        stream: Stream,
        offset: Offset,
        count: u32,
    },
}

impl fmt::Debug for ActorOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready(bytes) => f.debug_tuple("Ready").field(&bytes.len()).finish(),
            Self::ReadStream { offset, count, .. } => f
                .debug_struct("ReadStream")
                .field("offset", offset)
                .field("count", count)
                .finish_non_exhaustive(),
        }
    }
}

/// Synchronous single-owner file server implementation executed inside an
/// isolated Tokio actor task via [`spawn_file_actor`].
///
/// Every method takes `&mut self` without any interior locking, eliminating
/// cross-`.await` lock hazards and shared-mutex poisoning on panic. Blocking
/// [`FileKind::Stream`] reads return [`ActorOutcome::ReadStream`] so the proxy
/// awaits future bytes outside the mailbox loop.
pub trait FileServerActor: Send {
    /// Walk `names` from `fid`, binding `newfid` to the destination file.
    fn walk(&mut self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode>;

    /// Open `fid` with the given access intent.
    fn open(&mut self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode>;

    /// Read up to `count` bytes from `offset`.
    ///
    /// Non-streaming files return [`ActorOutcome::Ready`]; [`FileKind::Stream`]
    /// files return [`ActorOutcome::ReadStream`] so waiting on future bytes
    /// happens outside the actor mailbox.
    fn read(&mut self, fid: Fid, offset: Offset, count: u32) -> Result<ActorOutcome, ErrorCode>;

    /// Write `data` at `offset`, returning the byte count accepted.
    fn write(&mut self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode>;

    /// Return metadata for `fid`.
    fn stat(&mut self, fid: Fid) -> Result<Stat, ErrorCode>;

    /// Create child `name` of `kind` under directory `fid`, binding `newfid`.
    fn create(
        &mut self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode>;

    /// Remove the file `fid` refers to, then release `fid`.
    fn remove(&mut self, fid: Fid) -> Result<(), ErrorCode>;

    /// Release `fid` (and commit buffered writes for commit-on-clunk files).
    fn clunk(&mut self, fid: Fid) -> Result<(), ErrorCode>;
}

/// Terminal reason explaining why a spawned [`FileServerActor`] task stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorExitReason {
    /// The actor mailbox closed cleanly after all [`FileServer`] handles dropped.
    Exited,
    /// The actor task panicked; carries the extracted panic payload message.
    Panicked(String),
}

/// Handle for observing when a spawned [`FileServerActor`] terminates.
#[derive(Debug, Clone)]
pub struct ActorExitHandle {
    rx: watch::Receiver<Option<ActorExitReason>>,
}

impl ActorExitHandle {
    /// Wait until the actor task exits or panics and return its terminal reason.
    ///
    /// If the actor has already terminated, returns immediately with the
    /// recorded [`ActorExitReason`].
    pub async fn wait(&self) -> ActorExitReason {
        let mut rx = self.rx.clone();
        wait_for_exit(&mut rx).await
    }

    /// Return the terminal exit reason if the actor has already terminated.
    pub fn try_reason(&self) -> Option<ActorExitReason> {
        self.rx.borrow().clone()
    }

    /// Whether the actor task has already exited or panicked.
    pub fn is_finished(&self) -> bool {
        self.rx.borrow().is_some()
    }
}

/// Bounded-mailbox [`FileServer`] proxy forwarding operations to a spawned
/// [`FileServerActor`] task.
#[derive(Clone)]
pub struct ActorFileServer {
    tx: mpsc::Sender<Envelope>,
    exit_rx: watch::Receiver<Option<ActorExitReason>>,
}

impl ActorFileServer {
    /// Spawn `actor` in an isolated Tokio task with a bounded mailbox of
    /// `mailbox_cap` entries (clamped to at least `1`).
    pub fn spawn<A>(actor: A, mailbox_cap: usize) -> (Self, ActorExitHandle)
    where
        A: FileServerActor + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(mailbox_cap.max(1));
        let (exit_tx, exit_rx) = watch::channel(None);
        tokio::spawn(run_actor(actor, rx, exit_tx));
        let exit_handle = ActorExitHandle {
            rx: exit_rx.clone(),
        };
        (Self { tx, exit_rx }, exit_handle)
    }

    async fn call(&self, op: ActorOp) -> Result<ActorOutput, ErrorCode> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Envelope {
                op,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ErrorCode::Io)?;
        reply_rx.await.map_err(|_| ErrorCode::Io)
    }
}

/// Spawn a [`FileServerActor`] in an isolated Tokio task behind a bounded
/// mailbox proxy, returning both the [`FileServer`] handle and its
/// [`ActorExitHandle`].
///
/// `mailbox_cap` configures bounded `mpsc` async backpressure (`send().await`)
/// and is clamped to at least `1`.
pub fn spawn_file_actor<A>(actor: A, mailbox_cap: usize) -> (Arc<dyn FileServer>, ActorExitHandle)
where
    A: FileServerActor + Send + 'static,
{
    let (proxy, exit_handle) = ActorFileServer::spawn(actor, mailbox_cap);
    (Arc::new(proxy), exit_handle)
}

#[async_trait]
impl FileServer for ActorFileServer {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        match self
            .call(ActorOp::Walk {
                fid,
                newfid,
                names: names.to_vec(),
            })
            .await?
        {
            ActorOutput::Qid(res) => res,
            _ => Err(ErrorCode::Io),
        }
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        match self.call(ActorOp::Open { fid, mode }).await? {
            ActorOutput::Qid(res) => res,
            _ => Err(ErrorCode::Io),
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let outcome = match self.call(ActorOp::Read { fid, offset, count }).await? {
            ActorOutput::Read(res) => res?,
            _ => return Err(ErrorCode::Io),
        };
        match outcome {
            ActorOutcome::Ready(bytes) => Ok(bytes),
            ActorOutcome::ReadStream {
                stream,
                offset,
                count,
            } => {
                let mut exit_rx = self.exit_rx.clone();
                tokio::select! {
                    biased;
                    bytes = stream.read(offset, count) => Ok(bytes),
                    _ = wait_for_exit(&mut exit_rx) => Err(ErrorCode::Io),
                }
            }
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        match self
            .call(ActorOp::Write {
                fid,
                offset,
                data: data.to_vec(),
            })
            .await?
        {
            ActorOutput::Write(res) => res,
            _ => Err(ErrorCode::Io),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        match self.call(ActorOp::Stat { fid }).await? {
            ActorOutput::Stat(res) => res,
            _ => Err(ErrorCode::Io),
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
            .call(ActorOp::Create {
                fid,
                newfid,
                name: name.to_string(),
                kind,
            })
            .await?
        {
            ActorOutput::Qid(res) => res,
            _ => Err(ErrorCode::Io),
        }
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        match self.call(ActorOp::Remove { fid }).await? {
            ActorOutput::Unit(res) => res,
            _ => Err(ErrorCode::Io),
        }
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        match self.call(ActorOp::Clunk { fid }).await? {
            ActorOutput::Unit(res) => res,
            _ => Err(ErrorCode::Io),
        }
    }
}

struct Envelope {
    op: ActorOp,
    reply: oneshot::Sender<ActorOutput>,
}

enum ActorOp {
    Walk {
        fid: Fid,
        newfid: Fid,
        names: Vec<String>,
    },
    Open {
        fid: Fid,
        mode: OpenMode,
    },
    Read {
        fid: Fid,
        offset: Offset,
        count: u32,
    },
    Write {
        fid: Fid,
        offset: Offset,
        data: Vec<u8>,
    },
    Stat {
        fid: Fid,
    },
    Create {
        fid: Fid,
        newfid: Fid,
        name: String,
        kind: FileKind,
    },
    Remove {
        fid: Fid,
    },
    Clunk {
        fid: Fid,
    },
}

enum ActorOutput {
    Qid(Result<Qid, ErrorCode>),
    Read(Result<ActorOutcome, ErrorCode>),
    Write(Result<u32, ErrorCode>),
    Stat(Result<Stat, ErrorCode>),
    Unit(Result<(), ErrorCode>),
}

fn dispatch<A: FileServerActor>(actor: &mut A, op: ActorOp) -> ActorOutput {
    match op {
        ActorOp::Walk { fid, newfid, names } => ActorOutput::Qid(actor.walk(fid, newfid, &names)),
        ActorOp::Open { fid, mode } => ActorOutput::Qid(actor.open(fid, mode)),
        ActorOp::Read { fid, offset, count } => ActorOutput::Read(actor.read(fid, offset, count)),
        ActorOp::Write { fid, offset, data } => ActorOutput::Write(actor.write(fid, offset, &data)),
        ActorOp::Stat { fid } => ActorOutput::Stat(actor.stat(fid)),
        ActorOp::Create {
            fid,
            newfid,
            name,
            kind,
        } => ActorOutput::Qid(actor.create(fid, newfid, &name, kind)),
        ActorOp::Remove { fid } => ActorOutput::Unit(actor.remove(fid)),
        ActorOp::Clunk { fid } => ActorOutput::Unit(actor.clunk(fid)),
    }
}

async fn run_actor<A>(
    actor: A,
    mut rx: mpsc::Receiver<Envelope>,
    exit_tx: watch::Sender<Option<ActorExitReason>>,
) where
    A: FileServerActor + Send + 'static,
{
    let guard = ExitGuard::new(exit_tx);
    let mut actor = Some(actor);

    while let Some(Envelope { op, reply }) = rx.recv().await {
        let call_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let actor_ref = actor.as_mut().expect("actor present while loop runs");
            dispatch(actor_ref, op)
        }));
        match call_result {
            Ok(output) => {
                let _ = reply.send(output);
            }
            Err(payload) => {
                let reason = extract_panic_message(payload);
                // Reject any future sends immediately while owned state drops.
                rx.close();
                if let Some(owned) = actor.take() {
                    let _ = std::panic::catch_unwind(AssertUnwindSafe(move || drop(owned)));
                }
                guard.finish(ActorExitReason::Panicked(reason));
                // Drop queued envelopes and the panicking caller's reply channel so
                // every in-flight caller observes ErrorCode::Io.
                drop(rx);
                drop(reply);
                return;
            }
        }
    }

    if let Some(owned) = actor.take()
        && let Err(payload) = std::panic::catch_unwind(AssertUnwindSafe(move || drop(owned)))
    {
        guard.finish(ActorExitReason::Panicked(extract_panic_message(payload)));
        return;
    }

    guard.finish(ActorExitReason::Exited);
}

struct ExitGuard {
    tx: Option<watch::Sender<Option<ActorExitReason>>>,
}

impl ExitGuard {
    fn new(tx: watch::Sender<Option<ActorExitReason>>) -> Self {
        Self { tx: Some(tx) }
    }

    fn finish(mut self, reason: ActorExitReason) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Some(reason));
        }
    }
}

impl Drop for ExitGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let reason = if std::thread::panicking() {
                ActorExitReason::Panicked("actor panicked during cleanup".to_string())
            } else {
                ActorExitReason::Exited
            };
            let _ = tx.send(Some(reason));
        }
    }
}

async fn wait_for_exit(rx: &mut watch::Receiver<Option<ActorExitReason>>) -> ActorExitReason {
    loop {
        if let Some(reason) = rx.borrow().clone() {
            return reason;
        }
        if rx.changed().await.is_err() {
            return rx.borrow().clone().unwrap_or(ActorExitReason::Exited);
        }
    }
}

fn extract_panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "actor panicked".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::extract_panic_message;

    #[test]
    fn extract_panic_message_handles_str_string_and_unknown_payloads() {
        assert_eq!(extract_panic_message(Box::new("boom")), "boom");
        assert_eq!(
            extract_panic_message(Box::new(String::from("formatted boom"))),
            "formatted boom"
        );
        assert_eq!(extract_panic_message(Box::new(42u32)), "actor panicked");
    }
}
