//! Test-only rejecting writer probe for owning persistence boundary tests.
use super::*;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

struct FlushFailFile(tokio::fs::File);

impl tokio::io::AsyncWrite for FlushFailFile {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, bytes)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.0).poll_flush(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => Poll::Ready(Err(io::Error::other("synthetic flush failure"))),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

impl RolloutRecorder {
    /// Exercise the production writer with real writes and a failed file flush.
    pub(crate) async fn flush_failure_probe(&self) -> Self {
        self.flush().await.unwrap();
        let file = OpenOptions::new()
            .append(true)
            .open(&self.rollout_path)
            .await
            .unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(super::writer::run_writer(
            tokio::io::BufWriter::new(FlushFailFile(file)),
            rx,
        ));
        Self {
            writer: Arc::new(RolloutWriter::new(tx, task)),
            rollout_id: self.rollout_id.clone(),
            rollout_path: self.rollout_path.clone(),
        }
    }

    /// Inject failed acknowledgement after writes reach the real rollout file.
    pub(crate) fn batch_failure_probe(
        &self,
        persist_before_failure: bool,
    ) -> (Self, mpsc::UnboundedReceiver<Vec<RolloutItem>>) {
        let backing = self.clone();
        let path = self.rollout_path.clone();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (observed_tx, observed_rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    RolloutCmd::PersistBatch { items, ack } => {
                        if persist_before_failure {
                            backing.flush().await.unwrap();
                            let file = OpenOptions::new().append(true).open(&path).await.unwrap();
                            let mut writer = FlushFailFile(file);
                            let error = Self::persist_items_and_flush(&mut writer, &items)
                                .await
                                .unwrap_err();
                            assert!(error.to_string().contains("synthetic flush failure"));
                        }
                        let _ = observed_tx.send(items);
                        let _ = ack.send(Err(anyhow!("injected batch writer failure")));
                    }
                    RolloutCmd::Close { ack } => {
                        let _ = ack.send(Ok(()));
                        break;
                    }
                    _ => panic!("unexpected command in rejecting batch probe"),
                }
            }
        });
        (
            Self {
                writer: Arc::new(RolloutWriter::new(tx, task)),
                rollout_id: self.rollout_id.clone(),
                rollout_path: self.rollout_path.clone(),
            },
            observed_rx,
        )
    }
}
