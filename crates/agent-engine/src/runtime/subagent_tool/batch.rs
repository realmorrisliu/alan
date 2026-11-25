//! Preparation, concurrent execution, and deterministic commit helpers for subagent/delegated calls.

use std::future::Future;

use alan_agent_protocol::Event;
use anyhow::Result;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::agent_machine::NormalizedToolCall;
use crate::agent_machine::pending_resume::ResumeTarget;
use crate::agent_machine::task_budget::{Dimension, Exhausted, InheritedExhaustion};
use crate::evidence::redact_durable_evidence_text;
use crate::skills::{DelegatedSkillResult, DelegatedSkillResultStatus};

use super::super::child_agents::ChildLaunchRuntime;
use super::super::delegated_child_run::{ChildRuntimeResult, DelegatedChildRunReference};
use super::super::delegated_skill_evidence::{
    build_bounded_delegated_invocation_persistence, build_subagent_action_metadata,
    resolve_action_output_reference,
};
use super::super::delegated_skill_tool::{
    DelegatedSkillInvocationRequest, DelegatedSkillRuntime,
    parse_delegated_skill_invocation_request, resolve_delegated_skill_invocation,
};
use super::super::project_workstreams::{WorkstreamPersistence, WorkstreamRecord, WorkstreamState};
use super::super::transition::{NamespaceActionRecord, NamespaceAgentFiles};
use super::super::turn_support::{check_turn_cancelled, tool_result_preview};
use super::super::virtual_tool::VirtualToolOutcome;
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

/// Result of executing one prepared slot in `JoinSet` (pure data, no `&mut AgentMachine`).
pub(in crate::runtime) struct ExecutedChildSlot {
    pub(in crate::runtime) index: usize,
    pub(in crate::runtime) tool_call: NormalizedToolCall,
    pub(in crate::runtime) kind: PreparedChildKind,
    pub(in crate::runtime) label: String,
    pub(in crate::runtime) action_id: Option<String>,
    pub(in crate::runtime) outcome: std::result::Result<ChildRuntimeResult, SlotExecutionError>,
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
    )
    .await?;

    let mut tracked_workstream = None;
    let launch = match resolve_subagent_spawn_spec(runtime.child_namespace_cwd(), &request) {
        Ok(mut spec) => match prepare_workstream_launch(runtime, &request).await {
            Ok(prepared_ws) => {
                if let Some(model) = prepared_ws.effective_model {
                    spec.runtime_overrides.model = Some(model);
                }
                let child_runtime = runtime.child_launch_runtime(&spec);
                let resume = crate::agent_machine::pending_resume::resume_target_for(
                    &runtime.machine.pending_delegated_resumes,
                    &tool_call.id,
                )
                .or(prepared_ws.checkpoint_resume);
                tracked_workstream = prepared_ws.tracked;
                Ok((child_runtime, spec, resume))
            }
            Err(failure) => Err(*failure),
        },
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

pub(in crate::runtime) async fn execute_prepared_slot(
    slot: PreparedChildSlot,
    agent_files: NamespaceAgentFiles,
    cancel: CancellationToken,
    spawn_fn: SubagentSpawnFn,
) -> ExecutedChildSlot {
    let outcome = match slot.launch {
        Err(prelaunch) => Err(SlotExecutionError::Prelaunch(Box::new(prelaunch))),
        Ok((child_runtime, spec, resume)) => {
            if cancel.is_cancelled() {
                Ok(ChildRuntimeResult::cancelled_before_launch())
            } else {
                let tracker = slot.action_id.clone().map(|action_id| {
                    ActionProgressTracker::new(agent_files, action_id, slot.label.clone())
                });
                spawn_fn(child_runtime, spec, cancel, resume, tracker)
                    .await
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
        Ok(mut child_result) => {
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

async fn allocate_running_subagent_action(
    agent_files: &NamespaceAgentFiles,
    action_tool: &str,
    label: &str,
) -> Result<Option<String>> {
    let doc = build_subagent_action_metadata(label, None, None, "running", None, None);
    let record = NamespaceActionRecord::new(action_tool, "running")
        .with_result(doc.to_string())
        .with_approval("not_required");
    Ok(agent_files.start_running_action(record).await.ok())
}

pub(in crate::runtime) async fn finalize_action_record(
    agent_files: &NamespaceAgentFiles,
    action_id: Option<&str>,
    kind: &PreparedChildKind,
    label: &str,
    result: &ChildRuntimeResult,
) -> Option<crate::skills::DelegatedSkillOutputRef> {
    let action_id = action_id?;
    let redacted = redact_durable_evidence_text(&result.output_text);
    let child_pid = result
        .process_path
        .rsplit('/')
        .next()
        .filter(|pid| !pid.is_empty());
    let state_label = result.terminal_status_label();
    let mut doc = build_subagent_action_metadata(
        label,
        child_pid,
        result.child_run_id.as_deref(),
        state_label,
        (!result.process_path.is_empty()).then_some(result.process_path.as_str()),
        Some(&redacted.markers),
    );
    if let Some(agent_path) = result
        .child_run
        .as_ref()
        .and_then(|record| record.agent_path.as_deref())
    {
        doc["child_agent_path"] = json!(agent_path);
    }
    if let Some(error) = result.error_message.as_deref() {
        doc["error"] = json!(error);
    }
    let tool_name = match kind {
        PreparedChildKind::AdHocSubagent { request, .. } => format!("subagent:{}", request.role),
        PreparedChildKind::DelegatedSkill { request } => format!("delegate:{}", request.skill_id),
    };
    let mut record = NamespaceActionRecord::new(tool_name, state_label)
        .with_result(doc.to_string())
        .with_approval("not_required");
    if !redacted.text.trim().is_empty() {
        record = record.with_output(redacted.text.clone());
    }
    if !result.process_path.is_empty() {
        record = record.with_process(result.process_path.clone());
    }
    let _ = agent_files.finish_action(action_id, record).await;
    if !result.output_text.trim().is_empty()
        && result.requires_output_reference(redacted.text.chars().count())
    {
        resolve_action_output_reference(agent_files, action_id, result).await
    } else {
        None
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Batch slot completion threads borrowed runtime, call metadata, child reference, workstream record, and emitter explicitly"
)]
async fn record_slot_completion<E, F>(
    runtime: &mut DelegatedSkillRuntime<'_>,
    tool_call: &NormalizedToolCall,
    kind: &PreparedChildKind,
    result: DelegatedSkillResult,
    child_run_ref: Option<DelegatedChildRunReference>,
    workstream_record: Option<&WorkstreamRecord>,
    prelaunch_rejected: bool,
    status_override: Option<&str>,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: Future<Output = ()>,
{
    match kind {
        PreparedChildKind::DelegatedSkill { request } => {
            let (persisted_arguments, tape_record, rollout_record) =
                build_bounded_delegated_invocation_persistence(request, result, child_run_ref);
            let preview = tool_result_preview(&json!(tape_record.result.summary.clone()));
            let tape_payload = serde_json::to_value(&tape_record).unwrap_or_else(|_| {
                json!({
                    "status": "invalid_result_encoding",
                    "error": "Failed to serialize delegated skill result."
                })
            });
            let rollout_payload =
                serde_json::to_value(&rollout_record).unwrap_or_else(|_| tape_payload.clone());
            let succeeded = matches!(
                tape_record.result.status,
                DelegatedSkillResultStatus::Completed
            );
            emit(Event::ToolCallCompleted {
                presentation: None,
                id: tool_call.id.clone(),
                name: Some(tool_call.name.clone()),
                success: Some(succeeded),
                result_preview: preview,
                audit: None,
            })
            .await;
            runtime.machine.record_tool_call(
                &tool_call.name,
                persisted_arguments,
                rollout_payload,
                succeeded,
            );
            runtime
                .machine
                .add_tool_message(&tool_call.id, &tool_call.name, tape_payload);
            if prelaunch_rejected {
                runtime
                    .publish_prelaunch_rejection(&tape_record.result.summary)
                    .await;
            }
        }
        PreparedChildKind::AdHocSubagent {
            request,
            persisted_arguments,
            ..
        } => {
            let synthetic_req = DelegatedSkillInvocationRequest {
                skill_id: request.role.clone(),
                target: request.role.clone(),
                task: request.task.clone(),
                cwd: request.cwd.clone(),
                host_mounts: request.host_mounts.clone(),
                timeout_secs: request.timeout_secs,
            };
            let (_, tape_record, _) = build_bounded_delegated_invocation_persistence(
                &synthetic_req,
                result,
                child_run_ref.clone(),
            );
            let succeeded = matches!(
                tape_record.result.status,
                DelegatedSkillResultStatus::Completed
            );
            let mut tape_payload = json!({
                "status": status_override.unwrap_or(match tape_record.result.status {
                    DelegatedSkillResultStatus::Completed => "completed",
                    DelegatedSkillResultStatus::Failed => "failed",
                }),
                "role": request.role,
                "task": tape_record.task,
                "read_only": request.read_only,
                "summary": tape_record.result.summary,
            });
            if let Some(model) = request.model.as_deref() {
                tape_payload["model"] = json!(model);
            }
            if let Some(output_text) = tape_record.result.output_text {
                tape_payload["output_text"] = json!(output_text);
            }
            if let Some(preview_text) = tape_record.result.summary_preview {
                tape_payload["summary_preview"] = json!(preview_text);
            }
            if let Some(output_ref) = tape_record.result.output_ref {
                tape_payload["output_ref"] = json!(output_ref);
            }
            if let Some(structured_output) = tape_record.result.structured_output {
                tape_payload["structured_output"] = structured_output;
            }
            if let Some(child_run) = tape_record.result.child_run {
                tape_payload["child_run"] = child_run;
            }
            if !tape_record.result.warnings.is_empty() {
                tape_payload["warnings"] = json!(tape_record.result.warnings);
            }
            if let Some(error_kind) = tape_record.result.error_kind {
                tape_payload["error_kind"] = json!(error_kind);
            }
            if let Some(ws) = workstream_record {
                attach_workstream_fields(&mut tape_payload, ws);
            }
            let rollout_payload = if let Some(child_run_ref) = child_run_ref {
                let mut payload = tape_payload.clone();
                payload["child_run_reference"] = json!(child_run_ref);
                payload
            } else {
                tape_payload.clone()
            };
            let preview = tool_result_preview(&json!(tape_payload["summary"]));
            emit(Event::ToolCallCompleted {
                presentation: None,
                id: tool_call.id.clone(),
                name: Some(tool_call.name.clone()),
                success: Some(succeeded),
                result_preview: preview,
                audit: None,
            })
            .await;
            runtime.machine.record_tool_call(
                &tool_call.name,
                persisted_arguments.clone(),
                rollout_payload,
                succeeded,
            );
            runtime
                .machine
                .add_tool_message(&tool_call.id, &tool_call.name, tape_payload);
            if prelaunch_rejected {
                runtime
                    .publish_prelaunch_rejection(
                        tape_record
                            .result
                            .error_message
                            .as_deref()
                            .unwrap_or(&tape_record.result.summary),
                    )
                    .await;
            }
        }
    }
    Ok(())
}

fn subagent_arguments_json(request: &SubagentSpawnRequest) -> Value {
    let mut arguments = json!({
        "role": request.role,
        "task": request.task,
        "read_only": request.read_only,
    });
    if let Some(model) = request.model.as_deref() {
        arguments["model"] = json!(model);
    }
    if let Some(cwd) = request.cwd.as_ref() {
        arguments["cwd"] = json!(cwd.to_string_lossy());
    }
    if !request.host_mounts.is_empty() {
        arguments["host_mounts"] = json!(request.host_mounts);
    }
    if let Some(timeout_secs) = request.timeout_secs {
        arguments["timeout_secs"] = json!(timeout_secs);
    }
    if let Some(stream_name) = request.stream_name.as_deref() {
        arguments["stream_name"] = json!(stream_name);
    }
    if let Some(persistence) = request.persistence {
        arguments["persistence"] = json!(match persistence {
            WorkstreamPersistence::Persistent => "persistent",
            WorkstreamPersistence::OneOff => "one_off",
        });
    }
    if let Some(scope) = request.scope.as_deref() {
        arguments["scope"] = json!(scope);
    }
    arguments
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
    let result = slot.outcome.as_ref().unwrap_or(fallback_result);
    let _ = finalize_tracked_workstream(tracked, rollouts_dir, result, None).await;
}

fn attach_workstream_fields(payload: &mut Value, ws: &WorkstreamRecord) {
    payload["stream_name"] = json!(ws.name);
    payload["persistence"] = json!(match ws.persistence {
        WorkstreamPersistence::Persistent => "persistent",
        WorkstreamPersistence::OneOff => "one_off",
    });
    payload["workstream_state"] = json!(match ws.state {
        WorkstreamState::Running => "running",
        WorkstreamState::Idle => "idle",
        WorkstreamState::Archived => "archived",
        WorkstreamState::PausedError => "paused_error",
    });
    if let Some(ckpt) = ws.last_checkpoint_ref.as_deref() {
        payload["last_checkpoint_ref"] = json!(ckpt);
    }
    if let Some(rollout) = ws.rollout_ref.as_deref() {
        payload["rollout_ref"] = json!(rollout);
    }
}
