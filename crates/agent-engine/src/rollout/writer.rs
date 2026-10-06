use super::RolloutItem;
use anyhow::{Context, Result, anyhow};
use std::sync::Mutex;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub(super) enum RolloutCmd {
    Record(Box<RolloutItem>),
    PersistBatch {
        items: Vec<RolloutItem>,
        ack: oneshot::Sender<Result<()>>,
    },
    Flush {
        ack: Option<oneshot::Sender<Result<()>>>,
    },
    Close {
        ack: oneshot::Sender<Result<()>>,
    },
}

pub(super) async fn run_writer<W: tokio::io::AsyncWrite + Unpin>(
    mut writer: W,
    mut rx: mpsc::UnboundedReceiver<RolloutCmd>,
) {
    use super::RolloutRecorder;
    let mut failed_write = None::<String>;
    while let Some(command) = rx.recv().await {
        let blocked = || {
            anyhow!(
                "Rollout writer failed closed after uncertain write or flush: {}",
                failed_write.as_deref().unwrap_or("unknown")
            )
        };
        match command {
            RolloutCmd::Record(item) => {
                if failed_write.is_some() {
                    tracing::error!(error = %blocked(), "Rollout record rejected by failed-closed writer");
                } else if let Err(error) = RolloutRecorder::write_item(&mut writer, &item).await {
                    tracing::error!(%error, "Rollout record write failed");
                    failed_write = Some(error.to_string());
                }
            }
            RolloutCmd::PersistBatch { items, ack } => {
                let result = if failed_write.is_some() {
                    Err(blocked())
                } else {
                    RolloutRecorder::persist_items_and_flush(&mut writer, &items).await
                };
                if let Err(error) = &result {
                    tracing::error!(%error, "Rollout persistence failed; writer closed to further writes");
                    failed_write = Some(error.to_string());
                }
                let _ = ack.send(result);
            }
            RolloutCmd::Flush { ack } => {
                let result = if failed_write.is_some() {
                    Err(blocked())
                } else {
                    RolloutRecorder::flush_writer(&mut writer).await
                };
                if let Err(error) = &result {
                    tracing::error!(%error, "Rollout flush failed; writer closed to further writes");
                    failed_write = Some(error.to_string());
                }
                if let Some(ack) = ack {
                    let _ = ack.send(result);
                }
            }
            RolloutCmd::Close { ack } => {
                let result = if failed_write.is_some() {
                    Err(blocked())
                } else {
                    RolloutRecorder::flush_writer(&mut writer).await
                };
                if let Err(error) = &result {
                    tracing::error!(%error, "Rollout close flush failed");
                }
                let _ = ack.send(result);
                return;
            }
        }
    }
    if failed_write.is_none()
        && let Err(error) = RolloutRecorder::flush_writer(&mut writer).await
    {
        tracing::error!(%error, "Rollout final flush failed");
    }
}

#[derive(Debug)]
pub(super) struct RolloutWriter {
    tx: Mutex<Option<mpsc::UnboundedSender<RolloutCmd>>>,
    task: tokio::sync::Mutex<Option<JoinHandle<()>>>,
    close_error: Mutex<Option<String>>,
}

impl RolloutWriter {
    pub(super) fn new(tx: mpsc::UnboundedSender<RolloutCmd>, task: JoinHandle<()>) -> Self {
        Self {
            tx: Mutex::new(Some(tx)),
            task: tokio::sync::Mutex::new(Some(task)),
            close_error: Mutex::new(None),
        }
    }

    pub(super) fn send(&self, command: RolloutCmd) -> Result<()> {
        let sender = self
            .tx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sender
            .as_ref()
            .ok_or_else(|| anyhow!("Rollout writer is closed"))?
            .send(command)
            .map_err(|_| anyhow!("Rollout writer task has stopped"))
    }

    pub(super) async fn close(&self) -> Result<()> {
        let mut task = self.task.lock().await;
        let Some(task) = task.take() else {
            let close_error = self
                .close_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            return close_error
                .as_ref()
                .map_or(Ok(()), |error| Err(anyhow!(error.clone())));
        };

        let (ack_tx, ack_rx) = oneshot::channel();
        let close_result = {
            let mut sender = self
                .tx
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sender.take().map_or_else(
                || Err(anyhow!("Rollout writer is already closed")),
                |sender| {
                    sender
                        .send(RolloutCmd::Close { ack: ack_tx })
                        .map_err(|_| anyhow!("Rollout writer task has stopped"))
                },
            )
        };

        let ack_result = match close_result {
            Ok(()) => match ack_rx.await {
                Ok(result) => result,
                Err(_) => Err(anyhow!(
                    "Rollout writer stopped before close acknowledgement"
                )),
            },
            Err(error) => Err(error),
        };
        let task_result = task.await.context("Rollout writer task failed");

        let result = ack_result.and(task_result);
        if let Err(error) = &result {
            *self
                .close_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(format!("{error:#}"));
        }
        result
    }
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod io_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn close_failure_is_preserved_for_every_waiter() {
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx);
        let task = tokio::spawn(async {});
        let writer = RolloutWriter::new(tx, task);

        let first_error = writer.close().await.unwrap_err().to_string();
        let second_error = writer.close().await.unwrap_err().to_string();

        assert_eq!(first_error, second_error);
    }
}
