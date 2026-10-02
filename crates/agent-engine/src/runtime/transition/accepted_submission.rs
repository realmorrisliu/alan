//! Entry orchestration for one submission already accepted by the Process loop.

use alan_agent_protocol::{Event, InputMode, Op, Submission, UiEvent, UiInputStatus};
use anyhow::Result;
use tokio_util::sync::CancellationToken;

use crate::runtime::turn_input::{TurnInputBroker, next_pending_interaction_submission};

use super::{
    AcceptedSubmissionOutcome, RuntimeLoopState, TransitionCompletion,
    handle_submission_with_cancel, handle_submission_with_cancel_and_steering,
};

pub(crate) fn accepts_inband_submissions(op: &Op) -> bool {
    matches!(
        op,
        Op::Turn { .. }
            | Op::Input {
                mode: InputMode::Steer | InputMode::FollowUp,
                ..
            }
    )
}

pub(crate) fn advance_accepted_submission<'a>(
    state: &'a mut RuntimeLoopState,
    submission: Submission,
    broker: &'a TurnInputBroker,
    cancel: &'a CancellationToken,
) -> impl std::future::Future<Output = AcceptedSubmissionOutcome> + 'a {
    let completes_input = matches!(
        submission.op,
        Op::Turn { .. }
            | Op::Resume { .. }
            | Op::Input {
                mode: InputMode::FollowUp | InputMode::Steer,
                ..
            }
    ) || (matches!(submission.op, Op::Interrupt)
        && state.machine.has_pending_interaction());
    let requeue_inband_submissions = accepts_inband_submissions(&submission.op);
    let reject_compaction = matches!(submission.op, Op::CompactWithOptions { .. })
        && state.machine.has_pending_interaction();
    if !reject_compaction
        && matches!(
            submission.op,
            Op::Turn { .. } | Op::Input { .. } | Op::CompactWithOptions { .. }
        )
    {
        // Publish identity synchronously, before the Process can select a control
        // ahead of the returned future's first poll. Transition ownership stays here.
        state.machine.accept_submission(submission.id.clone());
        if matches!(submission.op, Op::CompactWithOptions { .. }) {
            // Manual compaction has a Tape identity but no cancellation contract.
            state
                .machine
                .input_queue()
                .lock()
                .expect("input queue poisoned")
                .active_submission_ids
                .clear();
        }
    }
    async move {
        if matches!(submission.op, Op::Turn { .. } | Op::Input { .. }) {
            let checkpoint = state.machine.dispatch_input(&submission).await;
            if let Err(error) = checkpoint {
                let event = UiEvent::InputCompleted {
                    submission_ids: vec![submission.id.clone()],
                    status: UiInputStatus::Failed,
                    error: Some(format!(
                        "Input dispatch persistence failed; execution did not start and recovery disposition is uncertain: {error}"
                    )),
                };
                let publish = state.agent_files().append_ui_event(&event).await;
                state.machine.finish_submission();
                return AcceptedSubmissionOutcome {
                    result: Err(match publish {
                        Ok(()) => error,
                        Err(publish_error) => error.context(format!(
                            "also failed to publish correlated dispatch failure: {publish_error}"
                        )),
                    }),
                    requeue_inband_submissions,
                    deferred_actions: Default::default(),
                };
            }
            state
                .environment
                .reconcile_input_captures(&state.machine.input_queue())
                .await;
        }
        if reject_compaction {
            return AcceptedSubmissionOutcome {
                result: Err(anyhow::anyhow!(
                    "Manual compaction must wait for the pending interaction to finish"
                )),
                requeue_inband_submissions,
                deferred_actions: Default::default(),
            };
        }
        let mut emit = |_event: Event| async {};

        let cancelled_before_start =
            cancel.is_cancelled() && matches!(submission.op, Op::Turn { .. } | Op::Input { .. });
        let mut result = if cancelled_before_start {
            state.machine.mark_submission_cancelled();
            if submission.intent == alan_agent_protocol::InputIntent::Command {
                state
                    .agent_files()
                    .write_rejected_command(&submission.id, "command cancelled before execution")
                    .await
            } else {
                Ok(())
            }
        } else if requeue_inband_submissions {
            drive_turn_submission_with_cancel(state, submission, broker, &mut emit, cancel).await
        } else {
            handle_submission_with_cancel(state, submission, &mut emit, cancel).await
        }
        .map(|()| {
            if state.machine.has_pending_interaction() {
                TransitionCompletion::Paused
            } else {
                TransitionCompletion::Completed
            }
        });

        if !state.machine.has_pending_interaction() {
            state
                .machine
                .set_turn_activity(crate::agent_machine::TurnActivityState::Idle);
        }
        state
            .environment
            .reconcile_input_captures(&state.machine.input_queue())
            .await;
        if (completes_input || cancelled_before_start || state.machine.submission_was_cancelled())
            && !state.machine.has_pending_interaction()
            && state.machine.current_submission_id().is_some()
        {
            let mut submission_ids = state.machine.related_submission_ids().to_vec();
            if let Some(id) = state.machine.current_submission_id() {
                submission_ids.push(id.to_owned());
            }
            let event = UiEvent::InputCompleted {
                submission_ids,
                status: if state.machine.submission_was_cancelled() {
                    UiInputStatus::Cancelled
                } else if result.is_err() {
                    UiInputStatus::Failed
                } else {
                    UiInputStatus::Completed
                },
                error: result.as_ref().err().map(ToString::to_string),
            };
            if let Err(error) = state.agent_files().append_ui_event(&event).await {
                result = Err(error.context("publish input completion"));
            }
        }

        let deferred_actions = state.machine.drain_deferred_runtime_actions();
        state.machine.finish_submission();
        crate::runtime::queue_publication::observe(&state.machine.input_queue()).await;

        AcceptedSubmissionOutcome {
            result,
            requeue_inband_submissions,
            deferred_actions,
        }
    }
}

async fn drive_turn_submission_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    initial_submission: Submission,
    broker: &TurnInputBroker,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    // The Process may admit steering before this future is first polled.
    // Its completion path already requeues leftovers from the preceding turn.
    let _ = state.machine.clear_buffered_inband_submissions();
    let agent_files = state.agent_files();
    let host_mount_requests = state.environment.host_mount_requests();
    let command = match &initial_submission.op {
        Op::Input { parts, .. }
            if initial_submission.intent == alan_agent_protocol::InputIntent::Command =>
        {
            Some(super::NormalizedToolCall {
                id: initial_submission.id.clone(),
                name: "bash".into(),
                arguments: serde_json::json!({"command": alan_agent_protocol::parts_to_text(parts)}),
            })
        }
        _ => None,
    };
    handle_submission_with_cancel_and_steering(
        state,
        initial_submission,
        emit,
        cancel,
        Some(broker),
    )
    .await?;
    let mut pending_command = command.filter(|_| state.machine.has_pending_interaction());

    loop {
        let next_submission = if state.machine.has_pending_interaction() {
            let RuntimeLoopState { machine, .. } = state;
            next_pending_interaction_submission(
                machine,
                &agent_files,
                &host_mount_requests,
                broker,
                emit,
                cancel,
            )
            .await?
        } else if let Some(buffered) = state.machine.pop_buffered_inband_submission() {
            Some(buffered)
        } else {
            broker.try_recv().await
        };

        let Some(next_submission) = next_submission else {
            if cancel.is_cancelled()
                && let Some(command) = pending_command.as_ref()
            {
                super::explicit_command::finish_failed_explicit_command(
                    state,
                    command,
                    "command cancelled while awaiting approval",
                    Some("cancelled"),
                    emit,
                )
                .await?;
            }
            break;
        };
        if matches!(
            next_submission.op,
            Op::Input {
                mode: InputMode::Steer,
                ..
            }
        ) && !state.machine.is_turn_active()
            && !state.machine.has_pending_interaction()
        {
            state.machine.remove_input(&next_submission).await?;
            state
                .environment
                .reconcile_input_captures(&state.machine.input_queue())
                .await;
            let (status, message) = if cancel.is_cancelled() {
                (
                    UiInputStatus::Cancelled,
                    "Steering input cancelled with the active turn",
                )
            } else {
                (
                    UiInputStatus::Failed,
                    "Steering input arrived after the turn completed; submit a new turn",
                )
            };
            agent_files
                .append_ui_event(&UiEvent::InputCompleted {
                    submission_ids: vec![next_submission.id],
                    status,
                    error: Some(message.into()),
                })
                .await?;
            if status == UiInputStatus::Failed {
                crate::runtime::ui_surfaces::error_notice(&agent_files, message).await?;
            }
            continue;
        }
        if matches!(next_submission.op, Op::Turn { .. } | Op::Input { .. }) {
            state.machine.dispatch_input(&next_submission).await?;
            state
                .environment
                .reconcile_input_captures(&state.machine.input_queue())
                .await;
        }
        // A request response continues the accepted input; its control ID is
        // not the identity of the Agent answer produced after approval.
        match next_submission.op {
            Op::Input {
                mode: InputMode::Steer,
                ..
            } => state
                .machine
                .accept_steering_submission(next_submission.id.clone()),
            Op::Resume { .. } => {}
            _ => state.machine.accept_submission(next_submission.id.clone()),
        }
        handle_submission_with_cancel_and_steering(
            state,
            next_submission,
            emit,
            cancel,
            Some(broker),
        )
        .await?;
        if !state.machine.has_pending_interaction() {
            pending_command = None;
        }
    }

    Ok(())
}
