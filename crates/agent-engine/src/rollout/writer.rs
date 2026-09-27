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
