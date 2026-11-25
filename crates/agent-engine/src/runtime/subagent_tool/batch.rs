//! Preparation, concurrent execution, and deterministic commit helpers for subagent/delegated calls.

use std::future::Future;

use alan_agent_protocol::Event;
use anyhow::Result;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::agent_machine::NormalizedToolCall;
use crate::agent_machine::pending_resume::ResumeTarget;
use crate::agent_machine::task_budget::{Dimension, Exhausted, InheritedExhaustion};
use crate::skills::DelegatedSkillResult;

use super::super::async_workstreams::{
    AsyncChildLaunchFn, AsyncWorkstreamInfo, AsyncWorkstreamRegistry,
    launch_background_workstream_slot,
};
use super::super::child_agents::ChildLaunchRuntime;
use super::super::child_runs::ChildRunRegistry;
use super::super::delegated_child_run::ChildRuntimeResult;
use super::super::delegated_skill_tool::{
    DelegatedSkillInvocationRequest, DelegatedSkillRuntime,
    parse_delegated_skill_invocation_request, resolve_delegated_skill_invocation,
};
use super::super::transition::NamespaceAgentFiles;
use super::super::turn_support::{check_turn_cancelled, tool_result_preview};
use super::super::virtual_tool::VirtualToolOutcome;
pub(in crate::runtime) use super::commit::finalize_action_record;
use super::commit::{
    allocate_running_subagent_action, commit_background_dispatched_slot, record_slot_completion,
    subagent_arguments_json,
};
use super::workstream::{
    TrackedWorkstream, finalize_tracked_workstream, prepare_workstream_launch,
};
use super::{
    ActionProgressTracker, SubagentSpawnFn, SubagentSpawnRequest, resolve_subagent_spawn_spec,
};

/// Either an ad-hoc subagent launch or a skill-delegated child launch in a batch.
#[derive(Debug, Clone)]
pub(in crate::runtime) enum PreparedChildKind {
    AdHocSubagent {
        request: SubagentSpawnRequest,
        persisted_arguments: Value,
        workstream: Option<TrackedWorkstream>,
    },
    DelegatedSkill {
        request: DelegatedSkillInvocationRequest,
    },
}

/// Immediate validation/resolution failure before spawning a child Process.
#[derive(Debug, Clone)]
pub(in crate::runtime) struct PrelaunchFailure {
    pub(in crate::runtime) status: String,
    pub(in crate::runtime) error_kind: String,
    pub(in crate::runtime) message: String,
    pub(in crate::runtime) delegated_result: Option<DelegatedSkillResult>,
}

/// One prepared subagent or delegated-skill item ready for concurrent execution.
pub(in crate::runtime) struct PreparedChildSlot {
    pub(in crate::runtime) index: usize,
    pub(in crate::runtime) tool_call: NormalizedToolCall,
    pub(in crate::runtime) kind: PreparedChildKind,
    pub(in crate::runtime) label: String,
    pub(in crate::runtime) action_id: Option<String>,
    pub(in crate::runtime) launch: std::result::Result<
        (
            ChildLaunchRuntime,
            alan_agent_protocol::SpawnSpec,
            Option<ResumeTarget>,
        ),
        PrelaunchFailure,
    >,
}

/// Outcome of executing one prepared slot in `JoinSet`.
pub(in crate::runtime) enum ExecutedSlotOutcome {
    Completed(Box<ChildRuntimeResult>),
    BackgroundDispatched(AsyncWorkstreamInfo),
}

/// Result of executing one prepared slot in `JoinSet` (pure data, no `&mut AgentMachine`).
pub(in crate::runtime) struct ExecutedChildSlot {
    pub(in crate::runtime) index: usize,
    pub(in crate::runtime) tool_call: NormalizedToolCall,
    pub(in crate::runtime) kind: PreparedChildKind,
    pub(in crate::runtime) label: String,
    pub(in crate::runtime) action_id: Option<String>,
    pub(in crate::runtime) outcome: std::result::Result<ExecutedSlotOutcome, SlotExecutionError>,
}

#[derive(Debug)]
pub(in crate::runtime) enum SlotExecutionError {
    Prelaunch(Box<PrelaunchFailure>),
    SpawnOrJoin(anyhow::Error),
}

pub(in crate::runtime) async fn prepare_subagent_slot(
    runtime: &mut DelegatedSkillRuntime<'_>,
    subagent_spawn_enabled: bool,
    index: usize,
    tool_call: &NormalizedToolCall,
    tool_arguments: &Value,
) -> Result<PreparedChildSlot> {
    let Some(request) = super::parse_subagent_spawn_request(tool_arguments) else {
        return Ok(PreparedChildSlot {
            index,
            tool_call: tool_call.clone(),
            kind: PreparedChildKind::AdHocSubagent {
                request: SubagentSpawnRequest {
                    role: "invalid".to_string(),
                    task: String::new(),
                    model: None,
                    read_only: false,
                    cwd: None,
                    host_mounts: Vec::new(),
                    timeout_secs: None,
                    stream_name: None,
                    persistence: None,
                    scope: None,
                    background: false,
                },
                persisted_arguments: tool_arguments.clone(),
                workstream: None,
            },
            label: "subagent".to_string(),
            action_id: None,
            launch: Err(PrelaunchFailure {
                status: "invalid_request".to_string(),
                error_kind: "invalid_request".to_string(),
                message: "Invalid spawn_subagent payload.".to_string(),
                delegated_result: None,
            }),
        });
    };

    let label = request.role.clone();
    let persisted_arguments = subagent_arguments_json(&request);
    if !(subagent_spawn_enabled && runtime.prompt_cache.supports_delegated_skill_invocation()) {
        return Ok(PreparedChildSlot {
            index,
            tool_call: tool_call.clone(),
            kind: PreparedChildKind::AdHocSubagent {
                request,
                persisted_arguments,
                workstream: None,
            },
            label,
            action_id: None,
            launch: Err(PrelaunchFailure {
                status: "subagent_spawn_unavailable".to_string(),
                error_kind: "subagent_spawn_unavailable".to_string(),
                message: "Ad-hoc subagent spawning is only available on Root Agent when child launch is configured."
                    .to_string(),
                delegated_result: None,
            }),
        });
    }

    let action_id = allocate_running_subagent_action(
        &runtime.agent_files,
        &format!("subagent:{}", request.role),
        &label,
        request.stream_name.as_deref(),
        request
            .stream_name
            .as_ref()
            .map(|_| request.effective_persistence()),
        request.background,
    )
    .await?;

    let mut tracked_workstream = None;
    let launch = match resolve_subagent_spawn_spec(runtime.child_namespace_cwd(), &request) {
        Ok(mut spec) => {
            if request.background && runtime.async_workstreams().is_closed() {
                Err(PrelaunchFailure {
                    status: "runtime_shutting_down".to_string(),
                    error_kind: "runtime_shutting_down".to_string(),
                    message:
                        "Foreground instance is shutting down; cannot launch async workstream."
                            .to_string(),
                    delegated_result: None,
                })
            } else {
                match prepare_workstream_launch(runtime, &request).await {
                    Ok(prepared_ws) => {
                        if let Some(model) = prepared_ws.effective_model {
                            spec.runtime_overrides.model = Some(model);
                        }
                        let lease = if request.background {
                            match runtime.authorize_async_workstream().await {
                                Ok(lease) => Ok(lease),
                                Err(err) => Err(PrelaunchFailure {
                                    status: "task_budget_exhausted".to_string(),
                                    error_kind: "task_budget_exhausted".to_string(),
                                    message: err.to_string(),
                                    delegated_result: None,
                                }),
                            }
                        } else {
                            Ok(None)
                        };
                        match lease {
                            Ok(lease) => {
                                let child_runtime =
                                    runtime.child_launch_runtime_with_lease(&spec, lease);
                                let resume =
                                    crate::agent_machine::pending_resume::resume_target_for(
                                        &runtime.machine.pending_delegated_resumes,
                                        &tool_call.id,
                                    )
                                    .or(prepared_ws.checkpoint_resume);
                                tracked_workstream = prepared_ws.tracked;
                                Ok((child_runtime, spec, resume))
                            }
                            Err(failure) => Err(failure),
                        }
                    }
                    Err(failure) => Err(*failure),
                }
            }
        }
        Err(failure) => Err(*failure),
    };

    Ok(PreparedChildSlot {
        index,
        tool_call: tool_call.clone(),
        kind: PreparedChildKind::AdHocSubagent {
            request,
            persisted_arguments,
            workstream: tracked_workstream,
        },
        label,
        action_id,
        launch,
    })
}

pub(in crate::runtime) async fn prepare_delegated_skill_slot(
    runtime: &mut DelegatedSkillRuntime<'_>,
    index: usize,
    tool_call: &NormalizedToolCall,
    tool_arguments: &Value,
) -> Result<PreparedChildSlot> {
    let Some(request) = parse_delegated_skill_invocation_request(tool_arguments) else {
        return Ok(PreparedChildSlot {
            index,
            tool_call: tool_call.clone(),
            kind: PreparedChildKind::DelegatedSkill {
                request: DelegatedSkillInvocationRequest {
                    skill_id: String::new(),
                    target: String::new(),
                    task: String::new(),
                    cwd: None,
                    host_mounts: Vec::new(),
                    timeout_secs: None,
                },
            },
            label: "delegate".to_string(),
            action_id: None,
            launch: Err(PrelaunchFailure {
                status: "invalid_request".to_string(),
                error_kind: "invalid_request".to_string(),
                message: "Invalid delegated skill invocation payload.".to_string(),
                delegated_result: None,
            }),
        });
    };

    let label = request.skill_id.clone();
    if !runtime.prompt_cache.supports_delegated_skill_invocation() {
        return Ok(PreparedChildSlot {
            index,
            tool_call: tool_call.clone(),
            kind: PreparedChildKind::DelegatedSkill { request },
            label,
            action_id: None,
            launch: Err(PrelaunchFailure {
                status: "delegated_invocation_unavailable".to_string(),
                error_kind: "delegated_invocation_unavailable".to_string(),
                message: "Delegated skill invocation is not available in this runtime.".to_string(),
                delegated_result: None,
            }),
        });
    }

    let resolution = resolve_delegated_skill_invocation(runtime, &request);
    runtime.publish_ensured_skills().await;
    let action_id = allocate_running_subagent_action(
        &runtime.agent_files,
        &format!("delegate:{}", request.skill_id),
        &label,
        None,
        None,
        false,
    )
    .await?;

    let (updated_request, launch) = match resolution {
        Ok(spec) => {
            let updated_request = request
                .with_effective_launch_inputs(spec.launch.cwd.clone(), spec.launch.timeout_secs);
            let child_runtime = runtime.child_launch_runtime(&spec);
            let resume = crate::agent_machine::pending_resume::resume_target_for(
                &runtime.machine.pending_delegated_resumes,
                &tool_call.id,
            );
            (updated_request, Ok((child_runtime, spec, resume)))
        }
        Err(result) => (
            request,
            Err(PrelaunchFailure {
                status: "failed".to_string(),
                error_kind: result
                    .error_kind
                    .clone()
                    .unwrap_or_else(|| "skill_resolution_failed".to_string()),
                message: result.summary.clone(),
                delegated_result: Some(*result),
            }),
        ),
    };

    Ok(PreparedChildSlot {
        index,
        tool_call: tool_call.clone(),
        kind: PreparedChildKind::DelegatedSkill {
            request: updated_request,
        },
        label,
        action_id,
        launch,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Slot execution threads async workstream registry and launcher alongside sync spawn hook"
)]
pub(in crate::runtime) async fn execute_prepared_slot_with_async(
    slot: PreparedChildSlot,
    agent_files: NamespaceAgentFiles,
    async_workstreams: AsyncWorkstreamRegistry,
    rollouts_dir: Option<std::path::PathBuf>,
    child_run_registry: ChildRunRegistry,
    cancel: CancellationToken,
    spawn_fn: SubagentSpawnFn,
    async_launch_fn: AsyncChildLaunchFn,
) -> ExecutedChildSlot {
    let is_background = matches!(
        &slot.kind,
        PreparedChildKind::AdHocSubagent { request, .. } if request.background
    );
    let outcome = match slot.launch {
        Err(prelaunch) => Err(SlotExecutionError::Prelaunch(Box::new(prelaunch))),
        Ok((child_runtime, spec, resume)) if is_background => {
            let tracked = tracked_workstream(&slot.kind).cloned();
            launch_background_workstream_slot(
                &async_workstreams,
                rollouts_dir,
                agent_files,
                child_run_registry,
                slot.kind.clone(),
                slot.label.clone(),
                slot.action_id.clone(),
                tracked,
                child_runtime,
                spec,
                resume,
                &cancel,
                async_launch_fn,
            )
            .await
            .map(ExecutedSlotOutcome::BackgroundDispatched)
        }
        Ok((child_runtime, spec, resume)) => {
            if cancel.is_cancelled() {
                Ok(ExecutedSlotOutcome::Completed(Box::new(
                    ChildRuntimeResult::cancelled_before_launch(),
                )))
            } else {
                let tracker = slot.action_id.clone().map(|action_id| {
                    let (stream_name, persistence) = match &slot.kind {
                        PreparedChildKind::AdHocSubagent { request, .. } => (
                            request.stream_name.clone(),
                            request
                                .stream_name
                                .as_ref()
                                .map(|_| request.effective_persistence()),
                        ),
                        PreparedChildKind::DelegatedSkill { .. } => (None, None),
                    };
                    ActionProgressTracker::new(agent_files, action_id, slot.label.clone())
                        .with_workstream_metadata(stream_name, persistence, false)
                });
                spawn_fn(child_runtime, spec, cancel, resume, tracker)
                    .await
                    .map(|res| ExecutedSlotOutcome::Completed(Box::new(res)))
                    .map_err(SlotExecutionError::SpawnOrJoin)
            }
        }
    };
    ExecutedChildSlot {
        index: slot.index,
        tool_call: slot.tool_call,
        kind: slot.kind,
        label: slot.label,
        action_id: slot.action_id,
        outcome,
    }
}

pub(in crate::runtime) async fn commit_executed_slot<E, F>(
    runtime: &mut DelegatedSkillRuntime<'_>,
    slot: ExecutedChildSlot,
    _cancel: &CancellationToken,
    task_cancel: &CancellationToken,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: Future<Output = ()>,
{
    match slot.outcome {
        Ok(ExecutedSlotOutcome::BackgroundDispatched(info)) => {
            commit_background_dispatched_slot(runtime, &slot.tool_call, &slot.kind, info, emit)
                .await
        }
        Ok(ExecutedSlotOutcome::Completed(boxed_result)) => {
            let mut child_result = *boxed_result;
            let tracked = tracked_workstream(&slot.kind);
            if let Some(pause) = child_result.pause.as_ref()
                && let Some(inherited) = pause.inherited.clone()
            {
                let _ = finalize_tracked_workstream(
                    tracked,
                    runtime.rollouts_dir(),
                    &child_result,
                    None,
                )
                .await;
                let dimension = serde_json::from_value::<Dimension>(Value::String(
                    inherited.dimension.clone(),
                ))?;
                let owns_allowance = match runtime.machine.task_budget.as_ref() {
                    Some(budget) => budget.snapshot().await.task_id == inherited.ancestor_task_id,
                    None => false,
                };
                crate::agent_machine::pending_resume::record(
                    &mut *runtime.machine,
                    crate::agent_machine::pending_resume::PendingResume {
                        tool_call_id: slot.tool_call.id.clone(),
                        request_id: inherited.checkpoint_id.clone(),
                        ancestor_task_id: inherited.ancestor_task_id.clone(),
                        dimension: inherited.dimension.clone(),
                        rollout_path: inherited.rollout_path.clone(),
                        child_pid: child_result
                            .process_path
                            .rsplit('/')
                            .next()
                            .unwrap_or_default()
                            .to_string(),
                    },
                )
                .await?;
                if !owns_allowance {
                    return Err(InheritedExhaustion {
                        ancestor_task_id: inherited.ancestor_task_id,
                        dimension,
                    }
                    .into());
                }
                return Err(Exhausted { dimension }.into());
            }

            if child_result.pause.is_none() {
                runtime
                    .machine
                    .pending_delegated_resumes
                    .retain(|pending| pending.tool_call_id != slot.tool_call.id);
            }
            if task_cancel.is_cancelled()
                && child_result.is_cancelled()
                && check_turn_cancelled(
                    &mut *runtime.machine,
                    &runtime.agent_files,
                    &runtime.host_mount_requests,
                    emit,
                    task_cancel,
                )
                .await?
            {
                let _ = finalize_tracked_workstream(
                    tracked,
                    runtime.rollouts_dir(),
                    &child_result,
                    None,
                )
                .await;
                finalize_action_record(
                    &runtime.agent_files,
                    slot.action_id.as_deref(),
                    &slot.kind,
                    &slot.label,
                    &child_result,
                )
                .await;
                return Ok(VirtualToolOutcome::EndTurn);
            }

            let output_reference = finalize_action_record(
                &runtime.agent_files,
                slot.action_id.as_deref(),
                &slot.kind,
                &slot.label,
                &child_result,
            )
            .await;
            let updated_workstream = finalize_tracked_workstream(
                tracked,
                runtime.rollouts_dir(),
                &child_result,
                output_reference.as_ref(),
            )
            .await;
            if let (Some(child_run_id), Some(reference)) = (
                child_result.child_run_id.as_deref(),
                output_reference.as_ref(),
            ) {
                runtime
                    .child_run_registry()
                    .set_state_ref(child_run_id, reference.clone());
                child_result.child_run = runtime.child_run_registry().get(child_run_id);
            }

            let delegated_result = child_result.delegated_result(output_reference);
            let child_run_ref = Some(child_result.reference());
            record_slot_completion(
                runtime,
                &slot.tool_call,
                &slot.kind,
                delegated_result,
                child_run_ref,
                updated_workstream.as_ref(),
                false,
                None,
                emit,
            )
            .await?;
            Ok(VirtualToolOutcome::Continue {
                refresh_context: true,
            })
        }
        Err(SlotExecutionError::SpawnOrJoin(err)) => {
            let tracked = tracked_workstream(&slot.kind);
            if task_cancel.is_cancelled()
                && check_turn_cancelled(
                    &mut *runtime.machine,
                    &runtime.agent_files,
                    &runtime.host_mount_requests,
                    emit,
                    task_cancel,
                )
                .await?
            {
                let cancelled = ChildRuntimeResult::cancelled_before_launch();
                let _ =
                    finalize_tracked_workstream(tracked, runtime.rollouts_dir(), &cancelled, None)
                        .await;
                return Ok(VirtualToolOutcome::EndTurn);
            }

            let capability_decision = err
                .downcast_ref::<super::super::delegation_capabilities::DelegatedSpawnRejected>()
                .map(|rejection| rejection.decision.clone());
            let error_kind = if capability_decision.is_some() {
                "delegated_capability_mismatch"
            } else {
                "child_launch_failed"
            };
            let summary = match &slot.kind {
                PreparedChildKind::AdHocSubagent { request, .. } => {
                    format!("Failed to launch subagent '{}': {err}", request.role)
                }
                PreparedChildKind::DelegatedSkill { request } => format!(
                    "Failed to launch delegated runtime for skill '{}': {err}",
                    request.skill_id
                ),
            };
            let mut failed_child = ChildRuntimeResult::cancelled_before_launch();
            failed_child.status = super::super::delegated_child_run::ChildRuntimeStatus::Failed;
            failed_child.error_message = Some(summary.clone());
            let output_reference = finalize_action_record(
                &runtime.agent_files,
                slot.action_id.as_deref(),
                &slot.kind,
                &slot.label,
                &failed_child,
            )
            .await;
            let updated_workstream = finalize_tracked_workstream(
                tracked,
                runtime.rollouts_dir(),
                &failed_child,
                output_reference.as_ref(),
            )
            .await;

            let mut result =
                DelegatedSkillResult::failed(summary, Some(json!({ "error_kind": error_kind })));
            result.error_kind = Some(error_kind.to_string());
            result.capability_decision = capability_decision;
            record_slot_completion(
                runtime,
                &slot.tool_call,
                &slot.kind,
                result,
                None,
                updated_workstream.as_ref(),
                true,
                Some(error_kind),
                emit,
            )
            .await?;
            Ok(VirtualToolOutcome::Continue {
                refresh_context: true,
            })
        }
        Err(SlotExecutionError::Prelaunch(prelaunch)) => {
            if let Some(action_id) = slot.action_id.as_deref() {
                let mut failed = ChildRuntimeResult::cancelled_before_launch();
                failed.status = super::super::delegated_child_run::ChildRuntimeStatus::Failed;
                failed.error_message = Some(prelaunch.message.clone());
                let _ = finalize_action_record(
                    &runtime.agent_files,
                    Some(action_id),
                    &slot.kind,
                    &slot.label,
                    &failed,
                )
                .await;
            }
            if let Some(delegated_result) = prelaunch.delegated_result {
                record_slot_completion(
                    runtime,
                    &slot.tool_call,
                    &slot.kind,
                    delegated_result,
                    None,
                    None,
                    true,
                    Some(&prelaunch.error_kind),
                    emit,
                )
                .await?;
            } else {
                let error_payload = json!({
                    "status": prelaunch.status,
                    "error_kind": prelaunch.error_kind,
                    "error": prelaunch.message,
                });
                let preview = tool_result_preview(&error_payload);
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: slot.tool_call.id.clone(),
                    name: Some(slot.tool_call.name.clone()),
                    success: Some(false),
                    result_preview: preview,
                    audit: None,
                })
                .await;
                let persisted_args = match &slot.kind {
                    PreparedChildKind::AdHocSubagent {
                        persisted_arguments,
                        ..
                    } => persisted_arguments.clone(),
                    PreparedChildKind::DelegatedSkill { .. } => slot.tool_call.arguments.clone(),
                };
                runtime.machine.record_tool_call(
                    &slot.tool_call.name,
                    persisted_args,
                    error_payload.clone(),
                    false,
                );
                runtime.machine.add_tool_message(
                    &slot.tool_call.id,
                    &slot.tool_call.name,
                    error_payload,
                );
                runtime
                    .publish_prelaunch_rejection(&prelaunch.message)
                    .await;
                emit(Event::Error {
                    message: prelaunch.message,
                    recoverable: true,
                })
                .await;
            }
            Ok(VirtualToolOutcome::Continue {
                refresh_context: true,
            })
        }
    }
}

fn tracked_workstream(kind: &PreparedChildKind) -> Option<&TrackedWorkstream> {
    match kind {
        PreparedChildKind::AdHocSubagent { workstream, .. } => workstream.as_ref(),
        PreparedChildKind::DelegatedSkill { .. } => None,
    }
}

pub(in crate::runtime) async fn finalize_uncommitted_slot(
    slot: &ExecutedChildSlot,
    rollouts_dir: Option<&std::path::Path>,
    fallback_result: &ChildRuntimeResult,
) {
    let tracked = tracked_workstream(&slot.kind);
    let result = match &slot.outcome {
        Ok(ExecutedSlotOutcome::Completed(result)) => result.as_ref(),
        Ok(ExecutedSlotOutcome::BackgroundDispatched(_)) => return,
        Err(_) => fallback_result,
    };
    let _ = finalize_tracked_workstream(tracked, rollouts_dir, result, None).await;
}
