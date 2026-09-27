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
}

impl RolloutWriter {
    pub(super) fn new(tx: mpsc::UnboundedSender<RolloutCmd>, task: JoinHandle<()>) -> Self {
        Self {
            tx: Mutex::new(Some(tx)),
            task: tokio::sync::Mutex::new(Some(task)),
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
            return Ok(());
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

        ack_result?;
        task_result?;
        Ok(())
    }
}
