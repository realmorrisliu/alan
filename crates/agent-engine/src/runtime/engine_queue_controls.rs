use super::*;

impl RuntimeSubmissionQueues {
    pub(super) async fn admit_input(&self, input: &Submission) -> Result<()> {
        crate::agent_machine::input_queue::admit_input(
            &self.outer_queue,
            self.recorder.as_ref(),
            input,
        )
        .await
    }

    pub(super) async fn reject_admission(&mut self, input: &Submission, error: &anyhow::Error) {
        warn!(%error, submission_id=%input.id, "Input admission not acknowledged; input remains unaccepted");
        if let Some(environment) = &self.environment {
            if let Err(publish_error) = environment
                .agent_files()
                .append_ui_event(&alan_agent_protocol::UiEvent::InputCompleted {
                    submission_ids: vec![input.id.clone()],
                    status: alan_agent_protocol::UiInputStatus::Failed,
                    error: Some(format!(
                        "Input admission persistence failed; acceptance is uncertain: {error}"
                    )),
                })
                .await
            {
                warn!(%publish_error, submission_id=%input.id, "Failed to publish admission uncertainty");
                self.pause();
                self.push_outer_submission(input.clone());
            }
        } else {
            // Without a visible surface, retain unaccepted work behind a pause.
            self.pause();
            self.push_outer_submission(input.clone());
        }
    }

    pub(super) async fn admit_api_before_dispatch(
        &mut self,
        receiver: &mut mpsc::Receiver<Submission>,
    ) -> Option<Submission> {
        use alan_agent_protocol::Op;
        for _ in 0..receiver.len() {
            let Ok(input) = receiver.try_recv() else {
                break;
            };
            if matches!(
                input.op,
                Op::Interrupt
                    | Op::InterruptSubmission { .. }
                    | Op::ContinueQueue
                    | Op::DiscardQueue
                    | Op::Resume { .. }
                    | Op::SelectProjectDirectory { .. }
            ) {
                return Some(input);
            }
            if let Err(error) = self.admit_input(&input).await {
                self.reject_admission(&input, &error).await;
                continue;
            }
            self.push_outer_submission(input);
        }
        None
    }

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
        if matches!(submission.op, Op::SelectProjectDirectory { .. }) && cancel.is_some() {
            let result = async {
                let environment = self
                    .environment
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing Process directory owner"))?;
                crate::runtime::transition::directory_control::write_cd_action(
                    files,
                    &submission.id,
                    "Select Process directory",
                    Err(anyhow::anyhow!("directory selection requires settled work")),
                    environment.process_files().process_path()?,
                )
                .await
            }
            .await;
            if let Err(error) = result {
                warn!(%error, "Failed to record active directory rejection");
            }
            return true;
        }
        if submission.intent == alan_agent_protocol::InputIntent::Command
            && !matches!(submission.op, Op::Input { .. })
        {
            if let Err(error) = files
                .write_rejected_command(
                    &submission.id,
                    "command intent requires an input operation",
                )
                .await
            {
                warn!(%error, submission_id=%submission.id, "Failed to publish rejected command evidence");
            }
            return true;
        }
        if let Op::InterruptSubmission { submission_id } = &submission.op {
            return self
                .interrupt_submission(submission_id, files, cancel)
                .await;
        }
        if matches!(submission.op, Op::Interrupt) {
            if let Some(cancel) = cancel {
                self.pause();
                cancel.cancel();
            } else {
                let mut queue = self.outer_queue.lock().expect("input queue poisoned");
                if queue.pending.iter().any(|item| matches!(item,
                    QueuedRuntimeItem::Submission(input) if matches!(input.op, Op::Turn { .. } | Op::Input { .. }))) {
                    queue.paused = true;
                }
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
        let preflight = async {
            let (recovered, removed) = {
                let queue = self.outer_queue.lock().expect("input queue poisoned");
                let removed = if matches!(submission.op, Op::DiscardQueue) {
                    queue
                        .pending
                        .iter()
                        .filter_map(|item| match item {
                            QueuedRuntimeItem::Submission(input)
                                if matches!(input.op, Op::Turn { .. } | Op::Input { .. }) =>
                            {
                                Some(input.id.clone())
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                anyhow::ensure!(cancel.is_none(), "active input has not settled yet");
                anyhow::ensure!(queue.paused, "input queue is not paused");
                (queue.recovered, removed)
            };
            if recovered && matches!(submission.op, Op::ContinueQueue) {
                let environment = self
                    .environment
                    .as_ref()
                    .context("recovered input requires current explicit project authority")?;
                let execution = environment.tool_execution();
                let cwd = execution
                    .default_cwd()
                    .context("recovered input requires current explicit project authority")?;
                execution.change_process_directory(&cwd)?;
                anyhow::ensure!(
                    execution
                        .execution_binding()
                        .is_some_and(|binding| binding.cwd_grant_id.is_some()),
                    "recovered input requires current explicit project authority"
                );
            }
            crate::agent_machine::input_queue::persist_input_removals(
                self.recorder.as_ref(),
                &removed,
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let result = preflight.and_then(|()| {
            (|| -> Result<Vec<Submission>> {
                anyhow::ensure!(cancel.is_none(), "active input has not settled yet");
                let mut queue = self.outer_queue.lock().expect("input queue poisoned");
                anyhow::ensure!(queue.paused, "input queue is not paused");
                let mut discarded = Vec::new();
                if matches!(submission.op, Op::DiscardQueue) {
                    queue.pending.retain(|item| match item {
                        QueuedRuntimeItem::Submission(input)
                            if matches!(input.op, Op::Turn { .. } | Op::Input { .. }) =>
                        {
                            discarded.push(input.clone());
                            false
                        }
                        _ => true,
                    });
                }
                queue.paused = false;
                Ok(discarded)
            })()
        });
        let notice = match result {
            Ok(discarded) => {
                let mut unpublished = 0;
                for input in &discarded {
                    if let Err(error) = publish_cancelled_input(
                        files,
                        &input.id,
                        "Queued input discarded without execution",
                    )
                    .await
                    {
                        unpublished += 1;
                        warn!(%error, submission_id=%input.id, "Failed to publish discarded input evidence");
                    }
                }
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
    /// Return false only when a suspended Machine must run its idle interrupt transition.
    async fn interrupt_submission(
        &mut self,
        submission_id: &str,
        files: &NamespaceAgentFiles,
        cancel: Option<&CancellationToken>,
    ) -> bool {
        // A cancellation must be acknowledged durably before removing pending work.
        let queued = {
            let queue = self.outer_queue.lock().expect("input queue poisoned");
            queue.pending.iter().any(|item| {
                matches!(item,
                QueuedRuntimeItem::Submission(input) if input.id == submission_id)
            }) || queue
                .inband
                .iter()
                .chain(queue.buffered_inband_submissions.iter())
                .any(|input| input.id == submission_id)
                || queue
                    .queued_next_turn_inputs
                    .iter()
                    .any(|(id, _)| id.as_deref() == Some(submission_id))
        };
        if queued
            && let Err(error) = crate::agent_machine::input_queue::persist_input_removals(
                self.recorder.as_ref(),
                &[submission_id.to_owned()],
            )
            .await
        {
            let _ = crate::runtime::ui_surfaces::warning(
                files,
                format!("Queue cancellation rejected: {error}"),
            )
            .await;
            return true;
        }
        let (removed, active) = {
            let mut queue = self.outer_queue.lock().expect("input queue poisoned");
            let mut removed = false;
            queue.pending.retain(|item| {
                let matches = matches!(item, QueuedRuntimeItem::Submission(input)
                    if input.id == submission_id && matches!(input.op, alan_agent_protocol::Op::Turn { .. } | alan_agent_protocol::Op::Input { .. }));
                removed |= matches;
                !matches
            });
            let MachineInputQueue {
                inband,
                buffered_inband_submissions,
                ..
            } = &mut *queue;
            for inputs in [inband, buffered_inband_submissions] {
                inputs.retain(|input| {
                    let matches = input.id == submission_id;
                    removed |= matches;
                    !matches
                });
            }
            queue.queued_next_turn_inputs.retain(|(id, _)| {
                let matches = id.as_deref() == Some(submission_id);
                removed |= matches;
                !matches
            });
            let active = queue
                .active_submission_ids
                .iter()
                .any(|id| id == submission_id);
            if removed || active {
                queue.paused = true;
            }
            if active {
                queue.active_cancel_requested = true;
            }
            (removed, active)
        };
        if removed && !active {
            if let Err(error) = publish_cancelled_input(
                files,
                submission_id,
                "Queued input cancelled without execution",
            )
            .await
            {
                warn!(%error, submission_id, "Failed to publish queued input cancellation");
            }
        } else if active {
            if let Some(cancel) = cancel {
                cancel.cancel();
            } else {
                return false;
            }
        } else {
            let _ = crate::runtime::ui_surfaces::warning(
                files,
                format!("Unknown or settled input: {submission_id}"),
            )
            .await;
            return true;
        }
        let _ = crate::runtime::ui_surfaces::warning(
            files,
            "Input queue paused; use /continue or /discard for queued work",
        )
        .await;
        true
    }
}

async fn publish_cancelled_input(
    files: &NamespaceAgentFiles,
    id: &str,
    message: &str,
) -> Result<()> {
    let record = crate::runtime::transition::NamespaceActionRecord::new("input", "failed")
        .with_output(message)
        .with_result(serde_json::json!({"call_id":id,"submission_id":id,"exit_code":1,"outcome":{"success":false,"error":message}}).to_string());
    let action_result = files.write_action(record).await;
    let event_result = files
        .append_ui_event(&alan_agent_protocol::UiEvent::InputCompleted {
            submission_ids: vec![id.to_owned()],
            status: alan_agent_protocol::UiInputStatus::Cancelled,
            error: Some(message.into()),
        })
        .await;
    action_result.map(|_| ()).and(event_result)
}
