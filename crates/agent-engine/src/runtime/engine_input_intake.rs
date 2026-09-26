use super::*;

/// Retains the acknowledged stream position, including frames in the reader channel.
pub(super) struct NamespaceInputIntake {
    receiver: mpsc::Receiver<(Result<Submission>, u64)>,
    task: tokio::task::JoinHandle<()>,
    admitted: u64,
}

impl NamespaceInputIntake {
    pub(super) fn new(files: NamespaceAgentFiles) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let admitted = files.input_read_position();
        let task = tokio::spawn(async move {
            loop {
                let input = files.read_next_input_submission(InputMode::FollowUp).await;
                let position = files.input_read_position();
                if sender.send((input, position)).await.is_err() {
                    break;
                }
            }
        });
        Self {
            receiver,
            task,
            admitted,
        }
    }

    pub(super) async fn recv(&mut self) -> Option<Result<Submission>> {
        let (input, position) = self.receiver.recv().await?;
        self.admitted = position;
        Some(input)
    }

    pub(super) async fn admit_before_discard(
        &mut self,
        files: &NamespaceAgentFiles,
        api: &mut mpsc::Receiver<Submission>,
        queues: &mut RuntimeSubmissionQueues,
    ) -> Result<()> {
        // Snapshot a finite boundary; concurrent writes after it belong to later work.
        let api_count = api.len();
        let committed = files.input_committed_length().await?;
        for _ in 0..api_count {
            if let Ok(input) = api.try_recv() {
                queues.push_outer_submission(input);
            }
        }
        while self.admitted < committed {
            let input = self
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("input reader closed"))??;
            queues.push_outer_submission(input);
        }
        Ok(())
    }

    pub(super) async fn stop(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}
