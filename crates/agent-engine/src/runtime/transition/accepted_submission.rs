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

pub(crate) async fn advance_accepted_submission(
    state: &mut RuntimeLoopState,
    submission: Submission,
    broker: &TurnInputBroker,
    cancel: &CancellationToken,
) -> AcceptedSubmissionOutcome {
    let completes_input = matches!(
        submission.op,
        Op::Turn { .. }
            | Op::Input {
                mode: InputMode::FollowUp | InputMode::Steer,
                ..
            }
    );
    let requeue_inband_submissions = accepts_inband_submissions(&submission.op);
    state.machine.accept_submission(submission.id.clone());
    let mut emit = |_event: Event| async {};

    let mut result = if requeue_inband_submissions {
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

    if completes_input && !matches!(result, Ok(TransitionCompletion::Paused)) {
        let mut submission_ids = state.machine.related_submission_ids().to_vec();
        if let Some(id) = state.machine.current_submission_id() {
            submission_ids.push(id.to_owned());
        }
        let event = UiEvent::InputCompleted {
            submission_ids,
            status: if cancel.is_cancelled() {
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

    AcceptedSubmissionOutcome {
        result,
        requeue_inband_submissions,
        deferred_actions,
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
    broker.clear().await;
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
