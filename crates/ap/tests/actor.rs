//! Behavioral contract tests for [`FileServerActor`] and [`spawn_file_actor`].
//!
//! Verifies:
//! - Full aP operation dispatch and clean [`ActorExitReason::Exited`] shutdown.
//! - Zero mailbox starvation when `FileKind::Stream` readers block outside the
//!   actor loop via [`ActorOutcome::ReadStream`].
//! - Bounded `mpsc` async backpressure (`send().await`) without speculative
//!   rejections.
//! - Isolated panic recovery: owned actor state drops cleanly, in-flight,
//!   blocked-stream, and subsequent calls fail with [`ErrorCode::Io`], and
//!   [`ActorExitHandle`] reports [`ActorExitReason::Panicked`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, mpsc as std_mpsc};
use std::time::Duration;

use alan_ap::{
    ActorExitReason, ActorOutcome, ErrorCode, Fid, FileKind, FileServerActor, Offset, OpenMode,
    Qid, Stat, Stream, spawn_file_actor,
};
use tokio::time::timeout;

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Node {
    Root,
    Note,
    Events,
    Dynamic(u64),
}

struct TestActor {
    fids: HashMap<Fid, (Node, Option<OpenMode>)>,
    note: Vec<u8>,
    dynamic: HashMap<String, (u64, Vec<u8>)>,
    next_path: u64,
    events: Stream,
    _drop_flag: Option<DropFlag>,
}

impl TestActor {
    fn new(drop_flag: Option<Arc<AtomicBool>>) -> Self {
        let mut fids = HashMap::new();
        fids.insert(Fid::ROOT, (Node::Root, None));
        Self {
            fids,
            note: b"initial".to_vec(),
            dynamic: HashMap::new(),
            next_path: 10,
            events: Stream::new(),
            _drop_flag: drop_flag.map(DropFlag),
        }
    }

    fn qid_for(node: Node) -> Qid {
        match node {
            Node::Root => Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 0,
            },
            Node::Note => Qid {
                kind: FileKind::File,
                version: 0,
                path: 1,
            },
            Node::Events => Qid {
                kind: FileKind::Stream,
                version: 0,
                path: 2,
            },
            Node::Dynamic(path) => Qid {
                kind: FileKind::File,
                version: 0,
                path,
            },
        }
    }
}

impl FileServerActor for TestActor {
    fn walk(&mut self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if newfid == Fid::ROOT || self.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = self.fids.get(&fid).ok_or(ErrorCode::NotFound)?.0;
        for name in names {
            node = match (node, name.as_str()) {
                (Node::Root, "note") => Node::Note,
                (Node::Root, "events") => Node::Events,
                (Node::Root, other) => self
                    .dynamic
                    .get(other)
                    .map(|(path, _)| Node::Dynamic(*path))
                    .ok_or(ErrorCode::NotFound)?,
                _ => return Err(ErrorCode::NotDirectory),
            };
        }
        self.fids.insert(newfid, (node, None));
        Ok(Self::qid_for(node))
    }

    fn open(&mut self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let entry = self.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        entry.1 = Some(mode);
        Ok(Self::qid_for(entry.0))
    }

    fn read(&mut self, fid: Fid, offset: Offset, count: u32) -> Result<ActorOutcome, ErrorCode> {
        let (node, mode) = self.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        if fid != Fid::ROOT && !matches!(mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        match *node {
            Node::Events => Ok(ActorOutcome::ReadStream {
                stream: self.events.clone(),
                offset,
                count,
            }),
            Node::Note => {
                let start = (offset as usize).min(self.note.len());
                let end = self.note.len().min(start + count as usize);
                Ok(ActorOutcome::Ready(self.note[start..end].to_vec()))
            }
            Node::Root => {
                let listing = b"events\nnote".to_vec();
                let start = (offset as usize).min(listing.len());
                let end = listing.len().min(start + count as usize);
                Ok(ActorOutcome::Ready(listing[start..end].to_vec()))
            }
            Node::Dynamic(path) => {
                let bytes = self
                    .dynamic
                    .values()
                    .find(|(p, _)| *p == path)
                    .map(|(_, bytes)| bytes.as_slice())
                    .ok_or(ErrorCode::NotFound)?;
                let start = (offset as usize).min(bytes.len());
                let end = bytes.len().min(start + count as usize);
                Ok(ActorOutcome::Ready(bytes[start..end].to_vec()))
            }
        }
    }

    fn write(&mut self, fid: Fid, _offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let (node, mode) = self.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(
            mode,
            Some(OpenMode::Write | OpenMode::WriteExclusive | OpenMode::ReadWrite)
        ) {
            return Err(ErrorCode::NoAccess);
        }
        match *node {
            Node::Note => {
                self.note = data.to_vec();
                Ok(data.len() as u32)
            }
            Node::Events => {
                self.events.append_sync(data);
                Ok(data.len() as u32)
            }
            Node::Dynamic(path) => {
                let (_, bytes) = self
                    .dynamic
                    .values_mut()
                    .find(|(p, _)| *p == path)
                    .ok_or(ErrorCode::NotFound)?;
                *bytes = data.to_vec();
                Ok(data.len() as u32)
            }
            Node::Root => Err(ErrorCode::IsDirectory),
        }
    }

    fn stat(&mut self, fid: Fid) -> Result<Stat, ErrorCode> {
        let (node, _) = self.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        let (name, length, writable) = match *node {
            Node::Root => (String::new(), 0, false),
            Node::Note => ("note".to_string(), self.note.len() as u64, true),
            Node::Events => ("events".to_string(), self.events.len_sync(), true),
            Node::Dynamic(path) => self
                .dynamic
                .iter()
                .find(|(_, (p, _))| *p == path)
                .map(|(name, (_, bytes))| (name.clone(), bytes.len() as u64, true))
                .ok_or(ErrorCode::NotFound)?,
        };
        Ok(Stat {
            name,
            qid: Self::qid_for(*node),
            length,
            executable: false,
            writable,
        })
    }

    fn create(
        &mut self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if newfid == Fid::ROOT || self.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        if !matches!(self.fids.get(&fid).map(|(n, _)| *n), Some(Node::Root)) {
            return Err(ErrorCode::NotDirectory);
        }
        if kind != FileKind::File {
            return Err(ErrorCode::Unsupported);
        }
        let path = self.next_path;
        self.next_path += 1;
        self.dynamic.insert(name.to_string(), (path, Vec::new()));
        let node = Node::Dynamic(path);
        self.fids.insert(newfid, (node, None));
        Ok(Self::qid_for(node))
    }

    fn remove(&mut self, fid: Fid) -> Result<(), ErrorCode> {
        let (node, _) = self.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        match node {
            Node::Dynamic(path) => {
                self.dynamic.retain(|_, (p, _)| *p != path);
                Ok(())
            }
            _ => Err(ErrorCode::Unsupported),
        }
    }

    fn clunk(&mut self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Err(ErrorCode::BadRequest);
        }
        self.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        Ok(())
    }
}

#[tokio::test]
async fn basic_operations_and_clean_exit_on_handle_drop() {
    let dropped = Arc::new(AtomicBool::new(false));
    let (server, exit_handle) = spawn_file_actor(TestActor::new(Some(Arc::clone(&dropped))), 8);

    assert!(!exit_handle.is_finished());
    assert_eq!(exit_handle.try_reason(), None);

    // Walk, open, read, write, stat, clunk on existing file.
    server
        .walk(Fid::ROOT, Fid(1), &["note".to_string()])
        .await
        .expect("walk note");
    server
        .open(Fid(1), OpenMode::ReadWrite)
        .await
        .expect("open note");
    assert_eq!(server.read(Fid(1), 0, 64).await.unwrap(), b"initial");
    assert_eq!(server.write(Fid(1), 0, b"updated").await.unwrap(), 7);
    assert_eq!(server.read(Fid(1), 0, 64).await.unwrap(), b"updated");
    assert_eq!(server.stat(Fid(1)).await.unwrap().length, 7);
    server.clunk(Fid(1)).await.expect("clunk note");

    // Create, write, read, remove dynamic file.
    server
        .create(Fid::ROOT, Fid(2), "temp", FileKind::File)
        .await
        .expect("create temp");
    server
        .open(Fid(2), OpenMode::ReadWrite)
        .await
        .expect("open temp");
    server.write(Fid(2), 0, b"ephemeral").await.unwrap();
    assert_eq!(server.read(Fid(2), 0, 64).await.unwrap(), b"ephemeral");
    server.remove(Fid(2)).await.expect("remove temp");

    // Dropping the last FileServer handle stops the actor cleanly.
    drop(server);
    let reason = timeout(Duration::from_secs(1), exit_handle.wait())
        .await
        .expect("actor should exit promptly");
    assert_eq!(reason, ActorExitReason::Exited);
    assert!(exit_handle.is_finished());
    assert_eq!(exit_handle.try_reason(), Some(ActorExitReason::Exited));
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn blocked_stream_readers_do_not_starve_actor_mailbox() {
    let (server, exit_handle) = spawn_file_actor(TestActor::new(None), 4);

    // Open two independent stream readers before any bytes exist.
    server
        .walk(Fid::ROOT, Fid(1), &["events".to_string()])
        .await
        .unwrap();
    server.open(Fid(1), OpenMode::Read).await.unwrap();

    server
        .walk(Fid::ROOT, Fid(2), &["events".to_string()])
        .await
        .unwrap();
    server.open(Fid(2), OpenMode::Read).await.unwrap();

    let s1 = Arc::clone(&server);
    let reader1 = tokio::spawn(async move { s1.read(Fid(1), 0, 1024).await });
    let s2 = Arc::clone(&server);
    let reader2 = tokio::spawn(async move { s2.read(Fid(2), 0, 1024).await });

    // Let both readers execute through the actor and park on Stream::read outside the loop.
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!reader1.is_finished());
    assert!(!reader2.is_finished());

    // Concurrent walk, stat, and write must complete with zero starvation.
    timeout(Duration::from_secs(1), async {
        server
            .walk(Fid::ROOT, Fid(3), &["note".to_string()])
            .await
            .unwrap();
        let stat = server.stat(Fid(3)).await.unwrap();
        assert_eq!(stat.name, "note");
        server.open(Fid(3), OpenMode::Write).await.unwrap();
        server.write(Fid(3), 0, b"still responsive").await.unwrap();
        server.clunk(Fid(3)).await.unwrap();

        server
            .walk(Fid::ROOT, Fid(4), &["events".to_string()])
            .await
            .unwrap();
        server.open(Fid(4), OpenMode::Write).await.unwrap();
        server.write(Fid(4), 0, b"first-event\n").await.unwrap();
    })
    .await
    .expect("actor mailbox must not starve under blocked stream readers");

    let bytes1 = timeout(Duration::from_secs(1), reader1)
        .await
        .expect("reader1 wakes")
        .unwrap()
        .unwrap();
    let bytes2 = timeout(Duration::from_secs(1), reader2)
        .await
        .expect("reader2 wakes")
        .unwrap()
        .unwrap();
    assert_eq!(bytes1, b"first-event\n");
    assert_eq!(bytes2, b"first-event\n");

    // Resume from offset at live edge and append a second chunk.
    let s1 = Arc::clone(&server);
    let next_reader = tokio::spawn(async move { s1.read(Fid(1), bytes1.len() as u64, 1024).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!next_reader.is_finished());

    server.write(Fid(4), 0, b"second-event\n").await.unwrap();
    let bytes_next = timeout(Duration::from_secs(1), next_reader)
        .await
        .expect("second read wakes")
        .unwrap()
        .unwrap();
    assert_eq!(bytes_next, b"second-event\n");

    drop(server);
    assert_eq!(exit_handle.wait().await, ActorExitReason::Exited);
}

struct GateActor {
    hold_fid: Fid,
    panic_on_release: bool,
    entered_tx: Option<std_mpsc::Sender<()>>,
    release_rx: Option<std_mpsc::Receiver<()>>,
    processed: Arc<StdMutex<Vec<Fid>>>,
    events: Stream,
    _drop_flag: Option<DropFlag>,
}

impl FileServerActor for GateActor {
    fn walk(&mut self, _fid: Fid, _newfid: Fid, _names: &[String]) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Stream,
            version: 0,
            path: 1,
        })
    }

    fn open(&mut self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Stream,
            version: 0,
            path: 1,
        })
    }

    fn read(&mut self, _fid: Fid, offset: Offset, count: u32) -> Result<ActorOutcome, ErrorCode> {
        Ok(ActorOutcome::ReadStream {
            stream: self.events.clone(),
            offset,
            count,
        })
    }

    fn write(&mut self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Ok(0)
    }

    fn stat(&mut self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.processed.lock().unwrap().push(fid);
        if fid == self.hold_fid {
            if let Some(tx) = self.entered_tx.take() {
                let _ = tx.send(());
            }
            if let Some(rx) = self.release_rx.take() {
                let _ = rx.recv();
            }
            if self.panic_on_release {
                panic!("synthetic actor failure on fid {}", fid.0);
            }
        }
        Ok(Stat {
            name: format!("fid-{}", fid.0),
            qid: Qid {
                kind: FileKind::File,
                version: 0,
                path: fid.0,
            },
            length: 0,
            executable: false,
            writable: false,
        })
    }

    fn create(
        &mut self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::File,
            version: 0,
            path: 99,
        })
    }

    fn remove(&mut self, _fid: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }

    fn clunk(&mut self, _fid: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_mailbox_applies_async_backpressure_without_rejecting() {
    let (entered_tx, entered_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let processed = Arc::new(StdMutex::new(Vec::new()));

    let actor = GateActor {
        hold_fid: Fid(1),
        panic_on_release: false,
        entered_tx: Some(entered_tx),
        release_rx: Some(release_rx),
        processed: Arc::clone(&processed),
        events: Stream::new(),
        _drop_flag: None,
    };

    // Mailbox capacity of 1: at most one request may buffer while another executes.
    let (server, exit_handle) = spawn_file_actor(actor, 1);

    let s1 = Arc::clone(&server);
    let req1 = tokio::spawn(async move { s1.stat(Fid(1)).await });
    tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(1)))
        .await
        .unwrap()
        .expect("req1 entered actor");

    // Fill the single bounded mailbox slot.
    let s2 = Arc::clone(&server);
    let req2 = tokio::spawn(async move { s2.stat(Fid(2)).await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!req2.is_finished());

    // A third concurrent caller must apply async backpressure at `send().await`
    // rather than being rejected or overflowing capacity.
    let s3 = Arc::clone(&server);
    let req3 = tokio::spawn(async move { s3.stat(Fid(3)).await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!req3.is_finished());

    // Cancelling req3 while parked on `send().await` guarantees it never enters
    // the actor mailbox.
    req3.abort();
    let _ = req3.await;

    // Release req1; req1 and buffered req2 both complete cleanly.
    release_tx.send(()).unwrap();
    assert_eq!(req1.await.unwrap().unwrap().name, "fid-1");
    assert_eq!(req2.await.unwrap().unwrap().name, "fid-2");

    // Now send a burst larger than `mailbox_cap = 1`; all callers complete via
    // async backpressure without `RateLimited` or `Io` rejections.
    let mut burst = Vec::new();
    for id in 10..22 {
        let s = Arc::clone(&server);
        burst.push(tokio::spawn(async move { s.stat(Fid(id)).await }));
    }
    for (idx, handle) in burst.into_iter().enumerate() {
        let stat = timeout(Duration::from_secs(1), handle)
            .await
            .expect("burst request completes")
            .unwrap()
            .expect("backpressure never rejects");
        assert_eq!(stat.name, format!("fid-{}", 10 + idx));
    }

    let seen = processed.lock().unwrap().clone();
    assert_eq!(seen[0], Fid(1));
    assert_eq!(seen[1], Fid(2));
    assert!(
        !seen.contains(&Fid(3)),
        "cancelled backpressured caller must not have entered mailbox"
    );
    assert_eq!(seen.len(), 2 + 12);

    drop(server);
    assert_eq!(exit_handle.wait().await, ActorExitReason::Exited);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actor_panic_isolates_state_fails_inflight_and_future_calls_and_notifies_exit_handle() {
    let (entered_tx, entered_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let processed = Arc::new(StdMutex::new(Vec::new()));
    let dropped = Arc::new(AtomicBool::new(false));

    let actor = GateActor {
        hold_fid: Fid(10),
        panic_on_release: true,
        entered_tx: Some(entered_tx),
        release_rx: Some(release_rx),
        processed: Arc::clone(&processed),
        events: Stream::new(),
        _drop_flag: Some(DropFlag(Arc::clone(&dropped))),
    };

    let (server, exit_handle) = spawn_file_actor(actor, 4);

    // Park a stream reader outside the actor loop before the panic occurs.
    let s_stream = Arc::clone(&server);
    let stream_reader = tokio::spawn(async move { s_stream.read(Fid(1), 0, 1024).await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!stream_reader.is_finished());

    // Enter the actor on Fid(10), hold it, and queue an in-flight request Fid(11).
    let s1 = Arc::clone(&server);
    let panicking_req = tokio::spawn(async move { s1.stat(Fid(10)).await });
    tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(1)))
        .await
        .unwrap()
        .expect("panicking request entered actor");

    let s2 = Arc::clone(&server);
    let queued_req = tokio::spawn(async move { s2.stat(Fid(11)).await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!queued_req.is_finished());

    // Trigger the panic inside the isolated actor task.
    release_tx.send(()).unwrap();

    // The panicking request, the queued in-flight request, and the offloaded
    // stream reader must all receive ErrorCode::Io.
    assert_eq!(
        timeout(Duration::from_secs(1), panicking_req)
            .await
            .expect("panicking req resolves")
            .unwrap(),
        Err(ErrorCode::Io)
    );
    assert_eq!(
        timeout(Duration::from_secs(1), queued_req)
            .await
            .expect("queued req resolves")
            .unwrap(),
        Err(ErrorCode::Io)
    );
    assert_eq!(
        timeout(Duration::from_secs(1), stream_reader)
            .await
            .expect("blocked stream reader wakes on panic")
            .unwrap(),
        Err(ErrorCode::Io)
    );

    // Exit handle reports Panicked with the extracted panic message, and owned
    // actor state has already been dropped cleanly.
    let reason = timeout(Duration::from_secs(1), exit_handle.wait())
        .await
        .expect("exit handle notifies");
    match reason {
        ActorExitReason::Panicked(message) => {
            assert!(
                message.contains("synthetic actor failure on fid 10"),
                "unexpected panic reason: {message}"
            );
        }
        ActorExitReason::Exited => panic!("expected ActorExitReason::Panicked"),
    }
    assert!(exit_handle.is_finished());
    assert!(matches!(
        exit_handle.try_reason(),
        Some(ActorExitReason::Panicked(_))
    ));
    assert!(
        dropped.load(Ordering::SeqCst),
        "owned actor state must be dropped cleanly on panic"
    );

    // Every subsequent FileServer operation must fail cleanly with ErrorCode::Io.
    assert_eq!(
        server.walk(Fid::ROOT, Fid(20), &[]).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        server.open(Fid::ROOT, OpenMode::Read).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(server.read(Fid::ROOT, 0, 16).await, Err(ErrorCode::Io));
    assert_eq!(server.write(Fid::ROOT, 0, b"x").await, Err(ErrorCode::Io));
    assert_eq!(server.stat(Fid::ROOT).await, Err(ErrorCode::Io));
    assert_eq!(
        server.create(Fid::ROOT, Fid(21), "x", FileKind::File).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(server.remove(Fid(20)).await, Err(ErrorCode::Io));
    assert_eq!(server.clunk(Fid(20)).await, Err(ErrorCode::Io));
}
