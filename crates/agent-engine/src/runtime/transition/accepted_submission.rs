//! Entry orchestration for one submission already accepted by the Process loop.

use alan_agent_protocol::{Event, InputMode, Op, Submission};
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
    let requeue_inband_submissions = accepts_inband_submissions(&submission.op);
    track_active_task_submission(&mut state.machine, &submission);
    let mut emit = |_event: Event| async {};

    let result = if requeue_inband_submissions {
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
    handle_submission_with_cancel_and_steering(
        state,
        initial_submission,
        emit,
        cancel,
        Some(broker),
    )
    .await?;

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

        let Some(mut next_submission) = next_submission else {
            break;
        };
        // A command admitted in-band was already eligible to steer the active turn. If that
        // turn ends before another Tool boundary, dispatch it now without requiring a new turn.
        if next_submission.intent == alan_agent_protocol::InputIntent::Command
            && let Op::Input {
                mode: InputMode::Steer,
                parts,
            } = next_submission.op
        {
            next_submission.op = Op::Input {
                parts,
                mode: InputMode::FollowUp,
            };
        }
        track_active_task_submission(&mut state.machine, &next_submission);
        handle_submission_with_cancel_and_steering(
            state,
            next_submission,
            emit,
            cancel,
            Some(broker),
        )
        .await?;
    }

    Ok(())
}

fn track_active_task_submission(
    machine: &mut crate::agent_machine::AgentMachine,
    submission: &Submission,
) {
    if accepts_inband_submissions(&submission.op) {
        machine.accept_submission(submission.id.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_machine::AgentMachine;
    use alan_agent_protocol::{ContentPart, InputIntent};

    #[test]
    fn resume_control_keeps_assistant_output_correlated_to_original_turn() {
        let mut machine = AgentMachine::new();
        let turn = Submission {
            id: "turn-id".into(),
            intent: InputIntent::Agent,
            op: Op::Turn {
                parts: vec![ContentPart::text("request")],
                context: None,
            },
        };
        track_active_task_submission(&mut machine, &turn);
        machine.add_user_message("request");

        let resume = Submission {
            id: "resume-id".into(),
            intent: InputIntent::Agent,
            op: Op::Resume {
                request_id: "confirmation".into(),
                content: Vec::new(),
            },
        };
        track_active_task_submission(&mut machine, &resume);
        machine.add_assistant_message("answer", None);

        assert_eq!(
            machine.messages().last().unwrap().submission_id(),
            Some("turn-id")
        );
    }
}
