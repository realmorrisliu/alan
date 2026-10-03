//! In-turn input brokering and file-native resume selection.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use alan_agent_protocol::{Event, InputIntent, InputMode, Op, Submission};
use anyhow::Result;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::transition::{NamespaceAgentFiles, NamespaceHostMountRequests};
use super::turn_support::cancel_current_task;
use crate::agent_machine::{AgentMachine, input_queue::MachineInputQueue};

const MAX_BROKERED_INBAND_USER_INPUTS: usize = 16;
pub(super) const MAX_BUFFERED_INBAND_USER_INPUTS: usize = 16;
pub(super) const NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone)]
pub(super) struct TurnInputBroker {
    queue: Arc<Mutex<MachineInputQueue>>,
    notify: Arc<Notify>,
}

impl Default for TurnInputBroker {
    fn default() -> Self {
        Self::from_queue(Default::default())
    }
}

impl TurnInputBroker {
    pub(super) fn from_queue(queue: Arc<Mutex<MachineInputQueue>>) -> Self {
        let notify = queue.lock().expect("input queue poisoned").notify.clone();
        Self { queue, notify }
    }

    pub(super) async fn push(&self, submission: Submission) -> bool {
        let mut state = self.queue.lock().expect("input queue poisoned");
        let guard = &mut state.inband;
        if is_brokered_input(&submission.op)
            && guard
                .iter()
                .filter(|queued| is_brokered_input(&queued.op))
                .count()
                >= MAX_BROKERED_INBAND_USER_INPUTS
        {
            return false;
        }
        guard.push_back(submission);
        drop(state);
        self.notify.notify_one();
        true
    }

    pub(super) async fn recv(&self, cancel: &CancellationToken) -> Option<Submission> {
        loop {
            if let Some(submission) = self.try_pop().await {
                return Some(submission);
            }

            tokio::select! {
                _ = cancel.cancelled() => return None,
                _ = self.notify.notified() => {}
            }
        }
    }

    pub(super) async fn drain(&self) -> VecDeque<Submission> {
        std::mem::take(&mut self.queue.lock().expect("input queue poisoned").inband)
    }

    pub(super) async fn try_recv(&self) -> Option<Submission> {
        self.try_pop().await
    }

    async fn try_pop(&self) -> Option<Submission> {
        self.queue
            .lock()
            .expect("input queue poisoned")
            .inband
            .pop_front()
    }
}

pub(super) fn is_turn_resume_submission(op: &Op) -> bool {
    matches!(op, Op::Resume { .. })
}

// Ordinary follow-ups share the outer FIFO regardless of intent; only explicit
// steering and request responses may enter an active transition.
pub(super) fn is_turn_inband_submission(submission: &Submission) -> bool {
    submission.intent != InputIntent::Command
        && matches!(
            submission.op,
            Op::Resume { .. }
                | Op::Input {
                    mode: InputMode::Steer,
                    ..
                }
        )
}

fn is_brokered_input(op: &Op) -> bool {
    matches!(
        op,
        Op::Input {
            mode: InputMode::Steer | InputMode::FollowUp,
            ..
        }
    )
}

pub(super) async fn next_pending_interaction_submission<E, F>(
    machine: &mut AgentMachine,
    agent_files: &NamespaceAgentFiles,
    host_mount_requests: &NamespaceHostMountRequests,
    broker: &TurnInputBroker,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<Option<Submission>>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    loop {
        if let Some(submission) =
            namespace_pending_resume_submission(machine, agent_files, host_mount_requests).await?
        {
            return Ok(Some(submission));
        }

        tokio::select! {
            incoming = broker.recv(cancel) => {
                let Some(incoming) = incoming else {
                    if cancel.is_cancelled() && machine.has_pending_interaction() {
                        // Report and clear buffered in-turn submissions before cancel_current_task
                        // resets Machine turn state, otherwise the drop count under-reports.
                        emit_dropped_in_turn_submissions(emit, machine, broker).await?;
                        cancel_current_task(
                            machine,
                            agent_files,
                            host_mount_requests,
                            emit,
                        )
                        .await?;
                        return Ok(None);
                    }
                    emit_dropped_in_turn_submissions(emit, machine, broker).await?;
                    return Ok(None);
                };

                if is_turn_resume_submission(&incoming.op) {
                    return Ok(Some(incoming));
                }

                if is_brokered_input(&incoming.op)
                    && machine.buffered_inband_user_input_count() >= MAX_BUFFERED_INBAND_USER_INPUTS
                {
                    machine.remove_input(&incoming).await?;
                    let message = format!("Too many queued in-turn user inputs (limit={MAX_BUFFERED_INBAND_USER_INPUTS}); dropping newest input.");
                    agent_files.append_ui_event(&alan_agent_protocol::UiEvent::InputCompleted {
                        submission_ids: vec![incoming.id],
                        status: alan_agent_protocol::UiInputStatus::Failed,
                        error: Some(message.clone()),
                    }).await?;
                    emit(Event::Error {
                        message,
                        recoverable: true,
                    })
                    .await;
                    continue;
                }
                machine.push_buffered_inband_submission(incoming);
            }
            _ = tokio::time::sleep(NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL) => {}
        }
    }
}

pub(super) async fn namespace_pending_resume_submission(
    machine: &AgentMachine,
    agent_files: &NamespaceAgentFiles,
    host_mount_requests: &NamespaceHostMountRequests,
) -> Result<Option<Submission>> {
    for request_id in machine.pending_request_ids() {
        if machine.pending_host_mount(&request_id).is_some() {
            if host_mount_requests
                .terminal_result(&request_id)
                .await?
                .is_some()
            {
                return Ok(Some(Submission {
                    intent: Default::default(),
                    id: format!("host-mount:{request_id}"),
                    op: Op::Resume {
                        request_id,
                        content: Vec::new(),
                    },
                }));
            }
            continue;
        }
        if let Some(submission) = agent_files
            .resume_submission_from_answered_request(&request_id)
            .await?
        {
            return Ok(Some(submission));
        }
    }

    Ok(None)
}

async fn emit_dropped_in_turn_submissions<E, F>(
    emit: &mut E,
    machine: &mut AgentMachine,
    broker: &TurnInputBroker,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let mut dropped = machine.drain_buffered_inband_submissions();
    dropped.extend(broker.drain().await);
    let dropped_total = dropped.len();
    let ids: Vec<_> = dropped
        .iter()
        .filter(|input| is_brokered_input(&input.op))
        .map(|input| input.id.clone())
        .collect();
    if let Err(error) = crate::agent_machine::input_queue::remove_input_bindings(
        &machine.input_queue(),
        machine.input_recorder().as_ref(),
        &ids,
    )
    .await
    {
        for retained in dropped {
            machine.push_buffered_inband_submission(retained);
        }
        emit(Event::Error {
            message: error.to_string(),
            recoverable: true,
        })
        .await;
        return Err(error);
    }
    for submission in dropped {
        if is_brokered_input(&submission.op) {
            machine.accept_steering_submission(submission.id);
        }
    }
    if dropped_total > 0 {
        emit(Event::Error {
            message: format!(
                "Dropped {dropped_total} in-turn buffered submissions due turn cancellation or shutdown."
            ),
            recoverable: true,
        })
        .await;
    }
    Ok(())
}

#[cfg(test)]
#[path = "turn_input_tests.rs"]
mod tests;
