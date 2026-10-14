//! Submission action dispatch, retaining the existing Machine and Tape lease owners.

use super::*;

pub(crate) async fn handle_submission_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    submission: Submission,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    handle_submission_with_cancel_and_steering(state, submission, emit, cancel, None).await
}

pub(crate) async fn handle_submission_with_cancel_and_steering<E, F>(
    state: &mut RuntimeLoopState,
    submission: Submission,
    emit: &mut E,
    cancel: &CancellationToken,
    steering_broker: Option<&TurnInputBroker>,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let explicit_interrupt = matches!(submission.op, Op::Interrupt);
    let computational = matches!(
        submission.op,
        Op::Turn { .. }
            | Op::CompactWithOptions { .. }
            | Op::Input {
                mode: alan_agent_protocol::InputMode::FollowUp
                    | alan_agent_protocol::InputMode::Steer,
                ..
            }
    );
    let budget_resume = matches!(&submission.op, Op::Resume {request_id, ..}
        if state.machine.pending_yield(request_id).is_some_and(|pending|
            matches!(pending, crate::agent_machine::PendingYield::Confirmation(pending)
                if pending.checkpoint_type == crate::approval::TASK_BUDGET_CHECKPOINT_TYPE)));
    let other_resume = !budget_resume
        && matches!(&submission.op, Op::Resume {request_id, ..}
        if state.machine.pending_yield(request_id).is_some());
    if (computational && !matches!(submission.op, Op::Turn { .. })) || other_resume {
        crate::runtime::task_budget::begin(state).await?;
    }
    let result = handle_submission_inner(state, submission, emit, cancel, steering_broker).await;
    let result = match result {
        Err(error)
            if error.is::<NamespaceEvaluationUncertainty>()
                || error.is::<owner_work::FallbackUncertainty>() =>
        {
            Err(error)
        }
        Err(error) if error.is::<crate::agent_machine::task_budget::Exhausted>() => {
            if cancel.is_cancelled() {
                crate::runtime::turn_support::cancel_current_task(
                    &mut state.machine,
                    &state.environment.agent_files(),
                    &state.environment.host_mount_requests(),
                    emit,
                )
                .await?;
                if let Some(budget) = &state.machine.task_budget {
                    budget.settle(true).await?;
                }
                state.machine.task_budget_wait = None;
                return Ok(());
            }
            if state.machine.has_pending_interaction() {
                crate::runtime::task_budget::settle_foreground(state).await?;
                return Ok(());
            }
            crate::runtime::task_budget::pause(
                state,
                error
                    .downcast_ref::<crate::agent_machine::task_budget::Exhausted>()
                    .expect("typed exhaustion"),
                None,
                emit,
            )
            .await
        }
        result => result,
    };
    if result.is_ok() && (explicit_interrupt || state.machine.submission_was_cancelled()) {
        if let Some(budget) = &state.machine.task_budget {
            if !explicit_interrupt && state.machine.has_deferred_runtime_actions() {
                budget.finish_foreground().await?;
            } else {
                budget.settle(true).await?;
            }
        }
        state.machine.task_budget_wait = None;
    } else if computational || other_resume || budget_resume {
        if result.is_err() {
            // An uncertain execution/publication error is not a terminal task.
            // Keep its spent allowance until explicit cancellation or recovery.
            if let Some(budget) = &state.machine.task_budget {
                budget.settle(false).await?;
            }
        } else {
            crate::runtime::task_budget::settle_foreground(state).await?;
        }
    }
    result
}

async fn handle_submission_inner<E, F>(
    state: &mut RuntimeLoopState,
    submission: Submission,
    emit: &mut E,
    cancel: &CancellationToken,
    steering_broker: Option<&TurnInputBroker>,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if submission.intent == alan_agent_protocol::InputIntent::Command {
        return explicit_command::handle_explicit_command(
            state,
            submission.id,
            submission.op,
            emit,
            cancel,
            steering_broker,
        )
        .await;
    }
    if matches!(submission.op, Op::Resume { .. }) && state.machine.task_budget_wait.is_none() {
        state.observe_input_shadow(&submission, cancel).await?;
    }
    if owner_work::handle(state, &submission, emit, cancel).await? {
        return Ok(());
    }

    let mut tape_writer = if matches!(submission.op, Op::Turn { .. }) {
        let writer = accepted_submission::begin_turn_dispatch(state, &submission).await?;
        if let Err(error) = crate::runtime::task_budget::begin(state).await {
            writer.finish().await?;
            return Err(error);
        }
        Some(writer)
    } else {
        None
    };
    let op = submission.op;

    let result = async {
        let action = match handle_runtime_op_with_writer(state, op, &mut tape_writer, emit).await? {
            RuntimeOpAction::NoTurn => return Ok(()),
            RuntimeOpAction::ReplayApprovedToolCall {
                tool_call,
                approved_unknown_effect_call_id,
                approved_tool_escalation_call_id,
            } => RuntimeOpAction::ReplayApprovedToolBatch {
                tool_calls: vec![tool_call],
                resume_with_generation: true,
                approved_unknown_effect_call_id,
                approved_tool_escalation_call_id,
            },
            action => action,
        };
        match action {
            RuntimeOpAction::NoTurn => Ok(()),
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                let writer = tape_writer
                    .as_ref()
                    .expect("generation owns the Tape lease");
                state.machine.set_turn_activity(TurnActivityState::Running);
                let turn_outcome = match run_turn_with_writer(
                    state,
                    turn_kind,
                    user_input,
                    emit,
                    cancel,
                    steering_broker,
                    writer,
                )
                .await
                {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        state.machine.set_turn_activity(TurnActivityState::Idle);
                        return Err(err);
                    }
                };
                state.machine.set_turn_activity(
                    if matches!(turn_outcome, TurnExecutionOutcome::Paused) {
                        TurnActivityState::Paused
                    } else {
                        TurnActivityState::Idle
                    },
                );
                if activate_task {
                    state.machine.activate_task();
                }
                Ok(())
            }
            RuntimeOpAction::ReplayApprovedToolCall { .. } => {
                unreachable!("single replay normalized to batch")
            }
            RuntimeOpAction::FinishRejectedExplicitCommand { tool_call } => {
                explicit_command::finish_failed_explicit_command(
                    state,
                    &tool_call,
                    "command was not approved",
                    Some("rejected"),
                    emit,
                )
                .await
            }
            RuntimeOpAction::ReplayApprovedToolBatch {
                tool_calls,
                resume_with_generation,
                approved_unknown_effect_call_id,
                approved_tool_escalation_call_id,
            } => {
                let writer = tape_writer.as_ref().expect("replay owns the Tape lease");
                if !resume_with_generation {
                    return explicit_command::replay_command(
                        state,
                        &tool_calls,
                        approved_unknown_effect_call_id.as_deref(),
                        approved_tool_escalation_call_id.as_deref(),
                        ToolOrchestratorInputs {
                            explicit_command: false,
                            cancel,
                            steering_broker,
                        },
                        writer,
                        emit,
                    )
                    .await;
                }
                state.machine.set_turn_activity(TurnActivityState::Running);
                match replay_approved_tool_batch_with_writer(
                    state,
                    &tool_calls,
                    approved_unknown_effect_call_id.as_deref(),
                    approved_tool_escalation_call_id.as_deref(),
                    ToolOrchestratorInputs {
                        explicit_command: false,
                        cancel,
                        steering_broker,
                    },
                    writer,
                    emit,
                )
                .await
                {
                    Ok(outcome) => match outcome {
                        ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. } => {
                            let turn_outcome = match run_turn_with_writer(
                                state,
                                TurnRunKind::ResumeTurn,
                                None,
                                emit,
                                cancel,
                                steering_broker,
                                writer,
                            )
                            .await
                            {
                                Ok(outcome) => outcome,
                                Err(err) => {
                                    state.machine.set_turn_activity(TurnActivityState::Idle);
                                    return Err(err);
                                }
                            };
                            state.machine.set_turn_activity(
                                if matches!(turn_outcome, TurnExecutionOutcome::Paused) {
                                    TurnActivityState::Paused
                                } else {
                                    TurnActivityState::Idle
                                },
                            );
                        }
                        ToolBatchOrchestratorOutcome::PauseTurn => {
                            state.machine.set_turn_activity(TurnActivityState::Paused);
                        }
                        ToolBatchOrchestratorOutcome::EndTurn { surfaces_refreshed } => {
                            finalize_replayed_tool_end_turn_best_effort(
                                state,
                                cancel,
                                surfaces_refreshed,
                                "approved-tool-replay-ended-turn",
                                "after approved tool replay batch",
                            )
                            .await;
                        }
                    },
                    Err(err) => {
                        state.machine.set_turn_activity(TurnActivityState::Idle);
                        return Err(err);
                    }
                };
                Ok(())
            }
        }
    }
    .await;
    if let Some(writer) = tape_writer {
        let closed = writer.finish().await;
        result.and(closed)
    } else {
        result
    }
}
