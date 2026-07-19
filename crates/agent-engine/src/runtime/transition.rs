//! Accepted-submission transition for one Agent Machine.
//!
//! The outer Process loop owns input transport and lifecycle control. Once it accepts a
//! submission, this module advances Machine state and returns only the control outcome the outer
//! loop needs.

mod namespace_environment;

#[cfg(test)]
pub(super) use namespace_environment::NamespaceRequestRecord;
pub use namespace_environment::{
    ApprovedMountGrant, ApprovedMountGrantAccess, MountGrantApplicator,
    MountGrantApplicatorFactory, NamespaceActionRecord, NamespaceMountApplication,
    NamespaceMountControl, NamespaceRuntimeEnvironment, NamespaceToolActionOutput,
    NamespaceTurnOutput, NamespaceTurnRuntime, NamespaceTurnRuntimeConfig,
};
pub(crate) use namespace_environment::{
    NamespaceAgentFiles, NamespaceChildLaunch, NamespaceGeneration, NamespaceProcessFiles,
    NamespaceToolExecution,
};

use std::collections::VecDeque;

use alan_agent_protocol::{Event, InputMode, Op, Submission};
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::warn;

#[cfg(test)]
use crate::agent_machine::NormalizedToolCall;
use crate::{
    agent_machine::{AgentMachine, DeferredRuntimeAction, TurnActivityState},
    config::Config,
    retry,
    runtime::RuntimeConfig,
};

use super::submission_handlers::{RuntimeOpAction, handle_runtime_op_with_cancel};
use super::tool_orchestrator::{
    ToolBatchOrchestratorOutcome, ToolOrchestratorInputs, replay_approved_tool_batch_with_cancel,
    replay_approved_tool_call_with_cancel,
};
use super::turn_driver::{TurnInputBroker, drive_turn_submission_with_cancel};
pub(super) use super::turn_executor::run_turn_with_cancel;
use super::turn_executor::{TurnExecutionOutcome, TurnRunKind};
#[allow(
    unused_imports,
    reason = "these helpers are imported here for the adjacent white-box test module"
)]
use super::turn_support::{
    cancel_current_task, emit_streaming_chunks, normalize_tool_calls, split_text_for_typing,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeferredRuntimeActionExit {
    Completed,
    Cancelled,
}

/// Agent state for the execution loop
pub(crate) struct RuntimeLoopState {
    pub(crate) machine: AgentMachine,
    pub(crate) environment: NamespaceRuntimeEnvironment,
    pub(crate) core_config: Config,
    pub(crate) runtime_config: RuntimeConfig,
    pub(crate) definition_persona_dirs: Vec<std::path::PathBuf>,
    pub(crate) prompt_cache: super::prompt_cache::PromptAssemblyCache,
}

impl RuntimeLoopState {
    /// Authoritative AgentFS path for the Process that owns this runtime state.
    pub(crate) fn process_path(&self) -> String {
        self.process_files()
            .process_path()
            .expect("runtime namespace was created with a valid /agent/<pid> path")
    }

    /// AgentFS projection path for the owning Process.
    pub(crate) fn agent_path(&self) -> &str {
        self.environment.agent_path()
    }

    pub(crate) fn child_run_registry(&self) -> &super::child_runs::ChildRunRegistry {
        self.environment.child_run_registry()
    }

    pub(crate) fn namespace_generation(&self) -> NamespaceGeneration {
        self.environment.generation()
    }

    pub(crate) fn agent_files(&self) -> NamespaceAgentFiles {
        self.environment.agent_files()
    }

    pub(crate) fn process_files(&self) -> NamespaceProcessFiles {
        self.environment.process_files()
    }

    pub(crate) fn child_launch(&self) -> NamespaceChildLaunch {
        self.environment.child_launch()
    }

    pub(crate) fn mount_control(&mut self) -> NamespaceMountControl<'_> {
        self.environment.mount_control()
    }

    pub(crate) fn tool_execution(&self) -> NamespaceToolExecution {
        self.environment.tool_execution()
    }

    pub(crate) async fn write_namespace_confirmation_request(
        &self,
        pending: &crate::approval::PendingConfirmation,
    ) -> Result<Option<String>> {
        let kind = crate::approval::runtime_confirmation_control_kind(&pending.checkpoint_type)
            .unwrap_or("confirmation");
        let options = serde_json::to_string(&serde_json::json!({
            "checkpoint_id": pending.checkpoint_id.clone(),
            "checkpoint_type": pending.checkpoint_type.clone(),
            "details": pending.details.clone(),
            "options": pending.options.clone(),
        }))?;
        let request_id = self
            .agent_files()
            .write_request(
                namespace_environment::NamespaceRequestRecord::new(kind, pending.summary.clone())
                    .with_options(options),
            )
            .await?;
        Ok(Some(request_id))
    }

    pub(crate) async fn write_namespace_structured_input_request(
        &self,
        pending: &crate::approval::PendingStructuredInputRequest,
    ) -> Result<Option<String>> {
        let options = serde_json::to_string(&serde_json::json!({
            "request_id": pending.request_id.clone(),
            "title": pending.title.clone(),
            "questions": pending.questions.clone(),
        }))?;
        let request_id = self
            .agent_files()
            .write_request(
                namespace_environment::NamespaceRequestRecord::new(
                    "structured_input",
                    pending.prompt.clone(),
                )
                .with_options(options),
            )
            .await?;
        Ok(Some(request_id))
    }

    pub(crate) async fn generate_once_with_cancel(
        &mut self,
        request: crate::llm::GenerationRequest,
        cancel: &CancellationToken,
        cancel_message: &'static str,
    ) -> Result<crate::llm::GenerationResponse> {
        let generation = self.namespace_generation();
        match generation.generate_controlled(&request, 0, cancel).await {
            Err(_) if cancel.is_cancelled() => Err(anyhow::anyhow!(cancel_message)),
            result => result,
        }
    }

    pub(crate) async fn generate_response_with_retry(
        &mut self,
        request: crate::llm::GenerationRequest,
        timeout_secs: u64,
        cancel: &CancellationToken,
    ) -> Result<crate::llm::GenerationResponse> {
        let max_retries = retry::DEFAULT_MAX_RETRIES;
        let mut last_error = None;

        for attempt in 0..=max_retries {
            if cancel.is_cancelled() {
                return Err(anyhow::anyhow!("LLM request cancelled"));
            }

            let generation = self.namespace_generation();
            let attempt_request = request.clone();
            let result = generation
                .generate_controlled(&attempt_request, timeout_secs, cancel)
                .await;

            match result {
                Ok(response) => return Ok(response),
                Err(error) => {
                    if !retry::is_retryable(&error) || attempt >= max_retries {
                        return Err(error);
                    }
                    last_error = Some(error);
                    let delay = retry::backoff_delay(attempt + 1);
                    tokio::select! {
                        _ = cancel.cancelled() => return Err(anyhow::anyhow!("LLM request cancelled")),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded")))
    }

    pub(crate) async fn static_tool_names(&self) -> Result<Vec<String>> {
        Ok(self
            .tool_execution()
            .discover_packages()
            .await?
            .into_iter()
            .map(|manifest| manifest.name)
            .collect())
    }

    pub(crate) fn default_tool_cwd(&self) -> Option<std::path::PathBuf> {
        self.tool_execution()
            .execution_binding()
            .map(|binding| binding.cwd)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransitionCompletion {
    Completed,
    Paused,
}

pub(crate) struct AcceptedSubmissionOutcome {
    pub(crate) result: Result<TransitionCompletion>,
    pub(crate) requeue_inband_submissions: bool,
    pub(crate) deferred_actions: VecDeque<DeferredRuntimeAction>,
}

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
    state.machine.accept_submission(submission.id.clone());
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

/// Handle a single submission
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "submission entrypoint remains available to the adjacent white-box test seam"
    )
)]
pub async fn handle_submission<E, F>(
    state: &mut RuntimeLoopState,
    submission: Submission,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let cancel = CancellationToken::new();
    handle_submission_with_cancel(state, submission, emit, &cancel).await
}

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
    let op = submission.op;

    match handle_runtime_op_with_cancel(state, op, emit, cancel).await? {
        RuntimeOpAction::NoTurn => Ok(()),
        RuntimeOpAction::RunTurn {
            turn_kind,
            user_input,
            activate_task,
        } => {
            state.machine.set_turn_activity(TurnActivityState::Running);
            let turn_outcome = match run_turn_with_cancel(
                state,
                turn_kind,
                user_input,
                emit,
                cancel,
                steering_broker,
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
        RuntimeOpAction::ReplayApprovedToolCall {
            tool_call,
            approved_unknown_effect_call_id,
            approved_tool_escalation_call_id,
        } => {
            state.machine.set_turn_activity(TurnActivityState::Running);
            match replay_approved_tool_call_with_cancel(
                state,
                &tool_call,
                approved_unknown_effect_call_id.as_deref(),
                approved_tool_escalation_call_id.as_deref(),
                ToolOrchestratorInputs {
                    cancel,
                    steering_broker,
                },
                emit,
            )
            .await
            {
                Ok(outcome) => match outcome {
                    ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. } => {
                        let turn_outcome = match run_turn_with_cancel(
                            state,
                            TurnRunKind::ResumeTurn,
                            None,
                            emit,
                            cancel,
                            steering_broker,
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
                            "after approved tool replay call",
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
        RuntimeOpAction::ReplayApprovedToolBatch {
            tool_calls,
            approved_unknown_effect_call_id,
            approved_tool_escalation_call_id,
        } => {
            state.machine.set_turn_activity(TurnActivityState::Running);
            match replay_approved_tool_batch_with_cancel(
                state,
                &tool_calls,
                approved_unknown_effect_call_id.as_deref(),
                approved_tool_escalation_call_id.as_deref(),
                ToolOrchestratorInputs {
                    cancel,
                    steering_broker,
                },
                emit,
            )
            .await
            {
                Ok(outcome) => match outcome {
                    ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. } => {
                        let turn_outcome = match run_turn_with_cancel(
                            state,
                            TurnRunKind::ResumeTurn,
                            None,
                            emit,
                            cancel,
                            steering_broker,
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

async fn finalize_replayed_tool_end_turn_best_effort(
    state: &mut RuntimeLoopState,
    cancel: &CancellationToken,
    surfaces_refreshed: bool,
    surfaces_context: &'static str,
    promotion_context: &'static str,
) {
    if !cancel.is_cancelled() {
        if !surfaces_refreshed {
            super::memory_surfaces::refresh_turn_memory_surfaces_best_effort(
                state,
                surfaces_context,
            )
            .await;
        }
        if let Some(job) =
            super::memory_promotion::build_turn_memory_promotion_job(state, promotion_context)
        {
            state
                .machine
                .push_deferred_runtime_action(DeferredRuntimeAction::TurnMemoryPromotion(job));
        }
    }

    state.machine.set_turn_activity(TurnActivityState::Idle);
}

pub(super) async fn run_deferred_runtime_action_with_cancel(
    state: &mut RuntimeLoopState,
    action: DeferredRuntimeAction,
    cancel: &CancellationToken,
) -> DeferredRuntimeActionExit {
    match action {
        DeferredRuntimeAction::TurnMemoryPromotion(job) => {
            match super::memory_promotion::run_turn_memory_promotion_job_for_runtime_with_cancel(
                state, &job, cancel,
            )
            .await
            {
                Ok(()) => DeferredRuntimeActionExit::Completed,
                Err(_) if cancel.is_cancelled() => DeferredRuntimeActionExit::Cancelled,
                Err(err) => {
                    warn!(
                        error = %err,
                        context = job.warning_context,
                        "Failed to capture confirmed turn memory"
                    );
                    DeferredRuntimeActionExit::Completed
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
