//! The production writer must not reuse a stream after uncertain buffered I/O.
use super::*;
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::io::{AsyncWrite, BufWriter};

#[derive(Default)]
struct State {
    accepted: Vec<u8>,
    committed: Vec<u8>,
    writes: usize,
    flushes: usize,
}

struct DeferredFailure(Arc<Mutex<State>>);
impl AsyncWrite for DeferredFailure {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut state = self.0.lock().unwrap();
        state.writes += 1;
        state.accepted.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut state = self.0.lock().unwrap();
        state.flushes += 1;
        let accepted = std::mem::take(&mut state.accepted);
        if state.flushes == 1 {
            state
                .committed
                .extend_from_slice(&accepted[..accepted.len() / 2]);
            Poll::Ready(Err(io::Error::other("deferred partial write failure")))
        } else {
            state.committed.extend_from_slice(&accepted);
            Poll::Ready(Ok(()))
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

fn item(label: &str) -> RolloutItem {
    RolloutItem::Event(super::super::EventRecord {
        event_type: label.into(),
        payload: serde_json::json!({"submission_ids": ["one", "two"]}),
        timestamp: "test".into(),
    })
}

async fn batch(tx: &mpsc::UnboundedSender<RolloutCmd>, label: &str) -> Result<()> {
    let (ack, rx) = oneshot::channel();
    tx.send(RolloutCmd::PersistBatch {
        items: vec![item(label)],
        ack,
    })
    .unwrap();
    rx.await.unwrap()
}

#[tokio::test]
async fn buffered_deferred_flush_failure_blocks_next_batch_and_record() {
    let state = Arc::new(Mutex::new(State::default()));
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(run_writer(
        BufWriter::new(DeferredFailure(state.clone())),
        rx,
    ));
    assert!(
        batch(&tx, "first")
            .await
            .unwrap_err()
            .to_string()
            .contains("deferred")
    );
    let prefix = state.lock().unwrap().committed.clone();
    assert!(!prefix.is_empty());
    tx.send(RolloutCmd::Record(Box::new(item("blocked-record"))))
        .unwrap();
    assert!(
        batch(&tx, "second")
            .await
            .unwrap_err()
            .to_string()
            .contains("failed closed")
    );
    drop(tx);
    task.await.unwrap();
    let state = state.lock().unwrap();
    assert_eq!(state.committed, prefix, "no second append or final retry");
    assert_eq!(state.writes, 1);
    assert_eq!(state.flushes, 1);
}

#[tokio::test]
async fn enqueue_only_record_and_unacked_flush_poison_the_same_writer() {
    let state = Arc::new(Mutex::new(State::default()));
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(run_writer(
        BufWriter::new(DeferredFailure(state.clone())),
        rx,
    ));
    tx.send(RolloutCmd::Record(Box::new(item("unacked-record"))))
        .unwrap();
    tx.send(RolloutCmd::Flush { ack: None }).unwrap();
    assert!(batch(&tx, "second").await.is_err());
    let (ack, result) = oneshot::channel();
    tx.send(RolloutCmd::Flush { ack: Some(ack) }).unwrap();
    assert!(result.await.unwrap().is_err());
    let (ack, result) = oneshot::channel();
    tx.send(RolloutCmd::Close { ack }).unwrap();
    assert!(result.await.unwrap().is_err());
    task.await.unwrap();
    let state = state.lock().unwrap();
    assert_eq!(state.writes, 1);
    assert_eq!(state.flushes, 1);
}
