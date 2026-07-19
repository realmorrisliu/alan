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

use crate::{
    agent_machine::{AgentMachine, DeferredRuntimeAction, NormalizedToolCall, TurnActivityState},
    approval::replays_tool_calls,
    config::Config,
    runtime::RuntimeConfig,
};

use super::loop_guard::ToolLoopGuard;
use super::steering_queue::handle_queued_steering_inputs;
use super::submission_handlers::{RuntimeOpAction, handle_runtime_op_with_cancel};
use super::tool_authorization::{
    ToolAuthorizationOutcome, ToolAuthorizationRequest, authorize_tool_call,
};
use super::tool_batch::{
    ToolBatchOrchestratorOutcome, ToolOrchestratorInputs, ToolOrchestratorOutcome,
    approved_replay_call_index,
};
use super::tool_execution::{
    ToolExecutionOutcome, ToolExecutionRequest, execute_allowed_tool_call,
};
use super::tool_resolution::{ToolResolutionOutcome, ToolResolutionRequest, resolve_tool_call};
use super::turn_driver::{TurnInputBroker, drive_turn_submission_with_cancel};
pub(super) use super::turn_executor::run_turn_with_cancel;
use super::turn_executor::{TurnExecutionOutcome, TurnRunKind};
use super::turn_memory::{FinalizeTurnMemoryRequest, finalize_turn_memory_best_effort};
#[allow(
    unused_imports,
    reason = "these helpers are imported here for the adjacent white-box test module"
)]
use super::turn_support::{
    cancel_current_task, emit_streaming_chunks, normalize_tool_calls, split_text_for_typing,
};
use super::virtual_tool::VirtualToolOutcome;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeferredRuntimeActionExit {
    Completed,
    Cancelled,
}

/// Process-loop aggregate for one Agent Machine and its stable transition dependencies.
///
/// Transition-local execution state belongs exclusively to `machine`; the remaining fields are
/// configuration, namespace capability sources, and derived prompt inputs.
pub(super) struct RuntimeLoopState {
    pub(super) machine: AgentMachine,
    pub(super) environment: NamespaceRuntimeEnvironment,
    pub(super) core_config: Config,
    pub(super) runtime_config: RuntimeConfig,
    pub(super) definition_persona_dirs: Vec<std::path::PathBuf>,
    pub(super) prompt_cache: super::prompt_cache::PromptAssemblyCache,
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
}

pub(super) fn compaction_runtime(
    state: &mut RuntimeLoopState,
) -> super::compaction::CompactionRuntime<'_> {
    let generation = state.namespace_generation();
    let agent_files = state.agent_files();
    let process_path = state.process_path();
    let settings = super::compaction::CompactionSettings::new(
        state.runtime_config.compaction_trigger_messages,
        state.runtime_config.compaction_keep_last,
        state.runtime_config.context_window_tokens,
        state.runtime_config.compaction_soft_trigger_ratio,
        state.runtime_config.compaction_hard_trigger_ratio,
    );
    let memory = super::compaction::CompactionMemory::new(
        state.core_config.memory.enabled,
        state.core_config.memory.store_dir.clone(),
        process_path,
    );
    super::compaction::CompactionRuntime::new(
        &mut state.machine,
        generation,
        agent_files,
        settings,
        memory,
    )
}

#[cfg(test)]
pub(super) fn child_launch_runtime(
    state: &RuntimeLoopState,
    spec: &alan_agent_protocol::SpawnSpec,
) -> super::child_agents::ChildLaunchRuntime {
    let base_agent_config = child_launch_base_agent_config(state);
    let (plan_explanation, plan_items) = match state.machine.plan_snapshot() {
        Some(snapshot) => (snapshot.explanation.as_deref(), snapshot.items.as_slice()),
        None => (None, &[][..]),
    };
    let task_context = super::child_agents::project_child_task_context(
        state.machine.tape_summary(),
        state.machine.messages(),
        plan_explanation,
        plan_items,
        spec,
    );
    super::child_agents::ChildLaunchRuntime::new(
        base_agent_config,
        state.child_launch(),
        state.tool_execution(),
        state.child_run_registry().clone(),
        state.process_path(),
        state.prompt_cache.capability_view().cloned(),
        task_context,
    )
}

pub(super) fn delegated_skill_runtime(
    state: &mut RuntimeLoopState,
) -> super::delegated_skill_tool::DelegatedSkillRuntime<'_> {
    let agent_files = state.agent_files();
    let child_run_registry = state.child_run_registry().clone();
    let child_launch = state.child_launch();
    let base_agent_config = child_launch_base_agent_config(state);
    let tool_execution = state.tool_execution();
    let parent_process_path = state.process_path();
    let child_runtime_inputs = super::delegated_skill_tool::DelegatedChildRuntimeInputs::new(
        base_agent_config,
        child_launch,
        tool_execution,
        child_run_registry,
        parent_process_path,
    );
    super::delegated_skill_tool::DelegatedSkillRuntime::new(
        &mut state.machine,
        &mut state.prompt_cache,
        agent_files,
        child_runtime_inputs,
    )
}

pub(super) fn child_run_termination_runtime(
    state: &mut RuntimeLoopState,
) -> super::child_run_termination_tool::ChildRunTerminationRuntime<'_> {
    let agent_files = state.agent_files();
    let tool_execution = state.tool_execution();
    let child_run_registry = state.child_run_registry().clone();
    let parent_process_path = state.process_path();
    super::child_run_termination_tool::ChildRunTerminationRuntime::new(
        &mut state.machine,
        &state.runtime_config.policy_engine,
        &state.runtime_config.governance,
        agent_files,
        tool_execution,
        child_run_registry,
        parent_process_path,
    )
}

pub(super) fn mount_request_runtime(
    state: &mut RuntimeLoopState,
) -> super::mount_request_tool::MountRequestRuntime<'_> {
    let agent_files = state.agent_files();
    let tool_execution = state.tool_execution();
    super::mount_request_tool::MountRequestRuntime::new(
        &mut state.machine,
        &state.runtime_config.policy_engine,
        &state.runtime_config.governance,
        agent_files,
        tool_execution,
    )
}

pub(super) fn agent_interaction_runtime(
    state: &mut RuntimeLoopState,
) -> super::interaction_tools::AgentInteractionRuntime<'_> {
    let agent_files = state.agent_files();
    super::interaction_tools::AgentInteractionRuntime::new(&mut state.machine, agent_files)
}

pub(super) async fn dispatch_virtual_tool_call<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<super::virtual_tool::VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let agent_files = state.agent_files();
    if cancel.is_cancelled()
        && super::turn_support::check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel)
            .await?
    {
        return Ok(super::virtual_tool::VirtualToolOutcome::EndTurn);
    }

    match tool_call.name.as_str() {
        "request_confirmation" => {
            let runtime = agent_interaction_runtime(state);
            super::interaction_tools::handle_request_confirmation(
                runtime,
                tool_call,
                tool_arguments,
                emit,
            )
            .await
        }
        "request_mount" => {
            let runtime = mount_request_runtime(state);
            super::mount_request_tool::handle_request_mount(
                runtime,
                tool_call,
                tool_arguments,
                emit,
            )
            .await
        }
        "request_user_input" => {
            let runtime = agent_interaction_runtime(state);
            super::interaction_tools::handle_request_user_input(
                runtime,
                tool_call,
                tool_arguments,
                emit,
            )
            .await
        }
        "update_plan" => {
            let runtime = agent_interaction_runtime(state);
            super::interaction_tools::handle_update_plan(runtime, tool_call, tool_arguments, emit)
                .await
        }
        "invoke_delegated_skill" => {
            let runtime = delegated_skill_runtime(state);
            super::delegated_skill_tool::handle_invoke_delegated_skill(
                runtime,
                tool_call,
                tool_arguments,
                cancel,
                emit,
            )
            .await
        }
        "terminate_child_run" => {
            let runtime = child_run_termination_runtime(state);
            super::child_run_termination_tool::handle_terminate_child_run(
                runtime,
                tool_call,
                tool_arguments,
                allow_approved_tool_escalation_execution,
                emit,
            )
            .await
        }
        _ => Ok(super::virtual_tool::VirtualToolOutcome::NotVirtual),
    }
}

pub(super) fn tool_resolution_runtime(
    state: &mut RuntimeLoopState,
) -> super::tool_resolution::ToolResolutionRuntime<'_> {
    let tool_execution = state.tool_execution();
    super::tool_resolution::ToolResolutionRuntime::new(&mut state.machine, tool_execution)
}

pub(super) fn tool_authorization_runtime(
    state: &mut RuntimeLoopState,
) -> super::tool_authorization::ToolAuthorizationRuntime<'_> {
    let generation = state.namespace_generation();
    let agent_files = state.agent_files();
    let llm_request_timeout_secs = state.runtime_config.llm_request_timeout_secs;
    super::tool_authorization::ToolAuthorizationRuntime::new(
        &mut state.machine,
        &state.runtime_config.policy_engine,
        &state.runtime_config.governance,
        generation,
        agent_files,
        llm_request_timeout_secs,
    )
}

pub(super) fn tool_execution_runtime(
    state: &mut RuntimeLoopState,
) -> super::tool_execution::ToolExecutionRuntime<'_> {
    let agent_files = state.agent_files();
    let tool_execution = state.tool_execution();
    let process_path = state.process_path();
    super::tool_execution::ToolExecutionRuntime::new(
        &mut state.machine,
        agent_files,
        tool_execution,
        process_path,
    )
}

pub(super) fn turn_memory_runtime(
    state: &mut RuntimeLoopState,
) -> super::turn_memory::TurnMemoryRuntime<'_> {
    let memory_dir = state
        .core_config
        .memory
        .enabled
        .then(|| state.core_config.memory.store_dir.clone())
        .flatten();
    let process_path = state.process_path();
    let llm_request_timeout_secs = state.runtime_config.llm_request_timeout_secs;
    super::turn_memory::TurnMemoryRuntime::new(
        &mut state.machine,
        memory_dir,
        process_path,
        llm_request_timeout_secs,
    )
}

pub(super) async fn orchestrate_tool_batch<E, F>(
    loop_guard: &mut ToolLoopGuard,
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    orchestrate_tool_batch_internal(loop_guard, state, tool_calls, inputs, None, None, emit).await
}

pub(super) async fn replay_approved_tool_call_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    approved_unknown_effect_call_id: Option<&str>,
    approved_tool_escalation_call_id: Option<&str>,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    replay_approved_tool_batch_with_cancel(
        state,
        std::slice::from_ref(tool_call),
        approved_unknown_effect_call_id,
        approved_tool_escalation_call_id,
        inputs,
        emit,
    )
    .await
}

pub(super) async fn replay_approved_tool_batch_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    approved_unknown_effect_call_id: Option<&str>,
    approved_tool_escalation_call_id: Option<&str>,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let max_tool_loops =
        (state.runtime_config.max_tool_loops != 0).then_some(state.runtime_config.max_tool_loops);
    let approved_unknown_effect_call_index =
        approved_replay_call_index(tool_calls, approved_unknown_effect_call_id);
    let approved_tool_escalation_call_index =
        approved_replay_call_index(tool_calls, approved_tool_escalation_call_id);
    let mut loop_guard = ToolLoopGuard::new(max_tool_loops, state.runtime_config.tool_repeat_limit);
    orchestrate_tool_batch_internal(
        &mut loop_guard,
        state,
        tool_calls,
        inputs,
        approved_unknown_effect_call_index,
        approved_tool_escalation_call_index,
        emit,
    )
    .await
}

async fn orchestrate_tool_call<E, F>(
    loop_guard: &mut ToolLoopGuard,
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    inputs: ToolOrchestratorInputs<'_>,
    allow_approved_unknown_effect_execution: bool,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<ToolOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let tool_arguments = tool_call.arguments.clone();

    if let Some(msg) = loop_guard.before_tool_call(&tool_call.name, &tool_arguments) {
        emit(Event::Error {
            message: msg.clone(),
            recoverable: true,
        })
        .await;
        emit(Event::TextDelta {
            chunk: msg,
            is_final: true,
        })
        .await;
        return Ok(ToolOrchestratorOutcome::EndTurn);
    }

    match dispatch_virtual_tool_call(
        state,
        tool_call,
        &tool_arguments,
        inputs.cancel,
        allow_approved_tool_escalation_execution,
        emit,
    )
    .await?
    {
        VirtualToolOutcome::NotVirtual => {}
        VirtualToolOutcome::Continue { refresh_context } => {
            return Ok(ToolOrchestratorOutcome::ContinueToolBatch { refresh_context });
        }
        VirtualToolOutcome::PauseTurn => return Ok(ToolOrchestratorOutcome::PauseTurn),
        VirtualToolOutcome::EndTurn => return Ok(ToolOrchestratorOutcome::EndTurn),
    }

    let resolution_runtime = tool_resolution_runtime(state);
    let resolution_request = ToolResolutionRequest {
        tool_call,
        tool_arguments: &tool_arguments,
    };
    let resolved_tool =
        match resolve_tool_call(resolution_runtime, resolution_request, emit).await? {
            ToolResolutionOutcome::Resolved(resolved) => resolved,
            ToolResolutionOutcome::Unavailable => {
                return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                    refresh_context: false,
                });
            }
        };

    let authorization_runtime = tool_authorization_runtime(state);
    let authorization_request = ToolAuthorizationRequest {
        tool_call,
        tool_arguments: &tool_arguments,
        tool_capability: resolved_tool.capability,
        current_tool_cwd: resolved_tool.current_cwd.as_deref(),
        allow_approved_tool_escalation_execution,
        cancel: inputs.cancel,
    };
    let tool_audit =
        match authorize_tool_call(authorization_runtime, authorization_request, emit).await? {
            ToolAuthorizationOutcome::Authorized { audit } => Some(audit),
            ToolAuthorizationOutcome::Completed => {
                return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                    refresh_context: false,
                });
            }
            ToolAuthorizationOutcome::PauseTurn => {
                return Ok(ToolOrchestratorOutcome::PauseTurn);
            }
        };

    let execution_runtime = tool_execution_runtime(state);
    let execution_request = ToolExecutionRequest {
        tool_call,
        tool_arguments: &tool_arguments,
        tool_timeout_secs: resolved_tool.timeout_secs,
        tool_capability: resolved_tool.capability,
        tool_audit,
        allow_approved_unknown_effect_execution,
        cancel: inputs.cancel,
    };
    match execute_allowed_tool_call(execution_runtime, execution_request, emit).await? {
        ToolExecutionOutcome::Completed => Ok(ToolOrchestratorOutcome::ContinueToolBatch {
            refresh_context: false,
        }),
        ToolExecutionOutcome::PauseTurn => Ok(ToolOrchestratorOutcome::PauseTurn),
        ToolExecutionOutcome::EndTurn => Ok(ToolOrchestratorOutcome::EndTurn),
    }
}

async fn orchestrate_tool_batch_internal<E, F>(
    loop_guard: &mut ToolLoopGuard,
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    inputs: ToolOrchestratorInputs<'_>,
    approved_unknown_effect_call_index: Option<usize>,
    approved_tool_escalation_call_index: Option<usize>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let mut refresh_context = false;

    for (idx, tool_call) in tool_calls.iter().enumerate() {
        let allow_approved_unknown_effect_execution =
            approved_unknown_effect_call_index.is_some_and(|approved_index| approved_index == idx);
        let allow_approved_tool_escalation_execution =
            approved_tool_escalation_call_index.is_some_and(|approved_index| approved_index == idx);
        match orchestrate_tool_call(
            loop_guard,
            state,
            tool_call,
            inputs,
            allow_approved_unknown_effect_execution,
            allow_approved_tool_escalation_execution,
            emit,
        )
        .await?
        {
            ToolOrchestratorOutcome::ContinueToolBatch {
                refresh_context: call_refresh,
            } => {
                refresh_context |= call_refresh;
                if handle_queued_steering_inputs(
                    &mut state.machine,
                    tool_calls,
                    idx + 1,
                    inputs.steering_broker,
                    emit,
                )
                .await?
                {
                    return Ok(ToolBatchOrchestratorOutcome::ContinueTurnLoop {
                        refresh_context: true,
                    });
                }
            }
            ToolOrchestratorOutcome::PauseTurn => {
                if let Some(pending) = state.machine.pending_confirmation()
                    && replays_tool_calls(&pending.checkpoint_type)
                {
                    state
                        .machine
                        .set_tool_replay_batch(pending.checkpoint_id, tool_calls[idx..].to_vec());
                }
                return Ok(ToolBatchOrchestratorOutcome::PauseTurn);
            }
            ToolOrchestratorOutcome::EndTurn => {
                return Ok(ToolBatchOrchestratorOutcome::EndTurn {
                    surfaces_refreshed: false,
                });
            }
        }
    }

    if let Some(msg) = loop_guard.after_tool_batch() {
        emit(Event::Error {
            message: msg.clone(),
            recoverable: true,
        })
        .await;
        emit(Event::TextDelta {
            chunk: msg,
            is_final: true,
        })
        .await;
        let memory_dir = state
            .core_config
            .memory
            .enabled
            .then_some(state.core_config.memory.store_dir.as_deref())
            .flatten();
        let process_path = state.process_path();
        super::memory_surfaces::refresh_active_turn_memory_surfaces_best_effort(
            &state.machine,
            memory_dir,
            &process_path,
            "tool-loop-guard-ended-turn",
        )
        .await;
        emit(Event::TurnCompleted {
            summary: Some("Tool loop stopped by loop guard".to_string()),
        })
        .await;
        return Ok(ToolBatchOrchestratorOutcome::EndTurn {
            surfaces_refreshed: true,
        });
    }

    Ok(ToolBatchOrchestratorOutcome::ContinueTurnLoop { refresh_context })
}

fn child_launch_base_agent_config(state: &RuntimeLoopState) -> super::launch_config::AgentConfig {
    let mut config = super::launch_config::AgentConfig::from(state.core_config.clone());
    config.runtime_config = state.runtime_config.clone();
    config
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
        let memory_runtime = turn_memory_runtime(state);
        finalize_turn_memory_best_effort(
            memory_runtime,
            FinalizeTurnMemoryRequest {
                surfaces_refreshed,
                surfaces_context,
                promotion_context,
            },
        )
        .await;
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
            let generation = state.namespace_generation();
            match super::memory_promotion::run_turn_memory_promotion_job_for_runtime_with_cancel(
                &generation,
                &job,
                cancel,
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
