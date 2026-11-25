//! Action evidence and tool-tape completion helpers for subagent and delegated-skill slots.

use std::future::Future;

use alan_agent_protocol::Event;
use anyhow::Result;
use serde_json::{Value, json};

use crate::agent_machine::NormalizedToolCall;
use crate::evidence::redact_durable_evidence_text;
use crate::skills::{DelegatedSkillResult, DelegatedSkillResultStatus};

use super::super::async_workstreams::AsyncWorkstreamInfo;
use super::super::delegated_child_run::{ChildRuntimeResult, DelegatedChildRunReference};
use super::super::delegated_skill_evidence::{
    attach_workstream_action_metadata, build_bounded_delegated_invocation_persistence,
    build_subagent_action_metadata, resolve_action_output_reference,
};
use super::super::delegated_skill_tool::{DelegatedSkillInvocationRequest, DelegatedSkillRuntime};
use super::super::project_workstreams::{WorkstreamPersistence, WorkstreamRecord, WorkstreamState};
use super::super::transition::{NamespaceActionRecord, NamespaceAgentFiles};
use super::super::turn_support::tool_result_preview;
use super::super::virtual_tool::VirtualToolOutcome;
use super::SubagentSpawnRequest;
use super::batch::PreparedChildKind;

pub(super) async fn allocate_running_subagent_action(
    agent_files: &NamespaceAgentFiles,
    action_tool: &str,
    label: &str,
    stream_name: Option<&str>,
    persistence: Option<WorkstreamPersistence>,
    background: bool,
) -> Result<Option<String>> {
    let mut doc = build_subagent_action_metadata(label, None, None, "running", None, None);
    attach_workstream_action_metadata(&mut doc, stream_name, persistence, background);
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
    if let PreparedChildKind::AdHocSubagent { request, .. } = kind {
        attach_workstream_action_metadata(
            &mut doc,
            request.stream_name.as_deref(),
            request
                .stream_name
                .as_ref()
                .map(|_| request.effective_persistence()),
            request.background,
        );
    }
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
pub(super) async fn record_slot_completion<E, F>(
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

pub(super) fn subagent_arguments_json(request: &SubagentSpawnRequest) -> Value {
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
    if request.background {
        arguments["background"] = json!(true);
    }
    arguments
}

pub(super) async fn commit_background_dispatched_slot<E, F>(
    runtime: &mut DelegatedSkillRuntime<'_>,
    tool_call: &NormalizedToolCall,
    kind: &PreparedChildKind,
    info: AsyncWorkstreamInfo,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: Future<Output = ()>,
{
    let (persisted_arguments, read_only, task) = match kind {
        PreparedChildKind::AdHocSubagent {
            request,
            persisted_arguments,
            ..
        } => (
            persisted_arguments.clone(),
            request.read_only,
            request.task.clone(),
        ),
        PreparedChildKind::DelegatedSkill { request } => {
            (tool_call.arguments.clone(), false, request.task.clone())
        }
    };
    let stream_label = info.stream_name.as_deref().unwrap_or(info.role.as_str());
    let summary = format!(
        "Dispatched background workstream '{}' at {} ({}).",
        stream_label, info.agent_path, info.process_path
    );
    let mut payload = json!({
        "status": "running",
        "background": true,
        "role": info.role,
        "task": task,
        "read_only": read_only,
        "workstream_state": "running",
        "persistence": match info.persistence {
            WorkstreamPersistence::Persistent => "persistent",
            WorkstreamPersistence::OneOff => "one_off",
        },
        "child_pid": info.child_pid,
        "child_run_id": info.child_run_id,
        "child_process_path": info.process_path,
        "child_agent_path": info.agent_path,
        "summary": summary,
    });
    if let Some(stream_name) = info.stream_name.as_deref() {
        payload["stream_name"] = json!(stream_name);
    }
    if let Some(action_id) = info.action_id.as_deref() {
        payload["action_id"] = json!(action_id);
    }
    let preview = tool_result_preview(&json!(summary));
    emit(Event::ToolCallCompleted {
        presentation: None,
        id: tool_call.id.clone(),
        name: Some(tool_call.name.clone()),
        success: Some(true),
        result_preview: preview,
        audit: None,
    })
    .await;
    runtime
        .machine
        .record_tool_call(&tool_call.name, persisted_arguments, payload.clone(), true);
    runtime
        .machine
        .add_tool_message(&tool_call.id, &tool_call.name, payload);
    Ok(VirtualToolOutcome::Continue {
        refresh_context: true,
    })
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
