use super::*;

impl RuntimeSubmissionQueues {
    pub(super) fn pause(&self) {
        self.outer_queue
            .lock()
            .expect("input queue poisoned")
            .paused = true;
    }

    pub(super) fn is_paused(&self) -> bool {
        self.outer_queue
            .lock()
            .expect("input queue poisoned")
            .paused
    }

    pub(super) async fn handle_control(
        &mut self,
        submission: &Submission,
        files: &NamespaceAgentFiles,
        cancel: Option<&CancellationToken>,
    ) -> bool {
        use alan_agent_protocol::Op;
        if matches!(submission.op, Op::Interrupt) {
            self.pause();
            if let Some(cancel) = cancel {
                cancel.cancel();
            } else {
                return false; // The idle transition still clears any pending request.
            }
            let _ = crate::runtime::ui_surfaces::warning(
                files,
                "Input queue paused; use /continue or /discard for queued work".to_owned(),
            )
            .await;
            return true;
        }
        if !matches!(submission.op, Op::ContinueQueue | Op::DiscardQueue) {
            return false;
        }
        let result = (|| -> Result<Vec<Submission>> {
            anyhow::ensure!(cancel.is_none(), "active input has not settled yet");
            let mut queue = self.outer_queue.lock().expect("input queue poisoned");
            anyhow::ensure!(queue.paused, "input queue is not paused");
            let mut discarded = Vec::new();
            if matches!(submission.op, Op::DiscardQueue) {
                queue.pending.retain(|item| match item {
                    QueuedRuntimeItem::Submission(input) => {
                        discarded.push(input.clone());
                        false
                    }
                    QueuedRuntimeItem::Deferred(_) => true,
                });
            }
            queue.paused = false;
            Ok(discarded)
        })();
        let notice = match result {
            Ok(discarded) => {
                let mut unpublished = 0;
                for input in &discarded {
                    let message = "Queued input discarded without execution";
                    let record = crate::runtime::transition::NamespaceActionRecord::new("input", "failed")
                        .with_output(message)
                        .with_result(serde_json::json!({"call_id":input.id,"submission_id":input.id,"exit_code":1,"outcome":{"success":false,"error":message}}).to_string());
                    if let Err(error) = files.write_action(record).await {
                        unpublished += 1;
                        warn!(%error, submission_id=%input.id, "Failed to publish discarded input evidence");
                    }
                }
                let _ = crate::runtime::ui_surfaces::turn_completed(files, false).await;
                if unpublished > 0 {
                    format!(
                        "Discarded {} queued inputs; could not publish {unpublished} result records",
                        discarded.len()
                    )
                } else if matches!(submission.op, Op::DiscardQueue) {
                    format!(
                        "Discarded {} queued inputs without execution",
                        discarded.len()
                    )
                } else {
                    "Input queue resumed".to_owned()
                }
            }
            Err(error) => format!("Queue control rejected: {error}"),
        };
        if let Err(error) = crate::runtime::ui_surfaces::warning(files, notice).await {
            warn!(%error, "Failed to publish queue control notice");
        }
        true
    }
}
