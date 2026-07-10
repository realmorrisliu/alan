use alan_agent_protocol::{
    AdaptivePresentationHint, ConfirmationYieldPayload, DelegatedSpawnContext, Event, SpawnHandle,
    SpawnLaunchInputs, SpawnRuntimeOverrides, SpawnSpec, SpawnToolProfileOverride,
    StructuredInputKind, StructuredInputOption, StructuredInputQuestion,
    StructuredInputYieldPayload, YieldKind,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

use crate::approval::{
    MOUNT_ESCALATION_CHECKPOINT_PREFIX, MOUNT_ESCALATION_CHECKPOINT_TYPE,
    TOOL_ESCALATION_CHECKPOINT_PREFIX, TOOL_ESCALATION_CHECKPOINT_TYPE,
};
use crate::approval::{PendingConfirmation, append_skill_permission_hints};
use crate::evidence::redact_durable_evidence_text;
use crate::llm::ToolDefinition;
use crate::skills::{
    DelegatedSkillInvocationRecord, DelegatedSkillOutputDebugMetadata, DelegatedSkillOutputRef,
    DelegatedSkillResult, DelegatedSkillResultStatus, DelegatedSkillResultTruncation,
};

use super::agent_loop::{NamespaceActionRecord, NormalizedToolCall, RuntimeLoopState};
use super::child_agents::{
    ChildRuntimeResult, ChildRuntimeStatus, bound_workspace_root, spawn_child_runtime_cancellable,
};
use super::child_runs::{
    ChildRunRegistryError, ChildRunTerminationMode, global_child_run_registry,
};
use super::delegation_capabilities::{
    DelegatedSpawnRejected, classify_delegated_task_requirements,
};
use super::tool_policy::{ToolPolicyDecision, evaluate_tool_policy};
use super::turn_support::{check_turn_cancelled, tool_result_preview};

const MAX_DELEGATED_SKILL_ID_CHARS: usize = 120;
const MAX_DELEGATED_TARGET_CHARS: usize = 120;
const MAX_DELEGATED_TASK_CHARS: usize = 1_000;
const MAX_DELEGATED_PATH_CHARS: usize = 1_000;
const DEFAULT_DELEGATED_TIMEOUT_SECS: u64 = 900;
const MAX_DELEGATED_TIMEOUT_SECS: u64 = 86_400;
const MAX_DELEGATED_RESULT_SUMMARY_CHARS: usize = 320;
const MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS: usize = 4_000;
const MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS: usize = 4_000;
const MAX_DELEGATED_CHILD_RUN_METADATA_CHARS: usize = 2_000;
const MAX_DELEGATED_RESULT_WARNINGS: usize = 16;
const MAX_DELEGATED_RESULT_WARNING_CHARS: usize = 512;
const WORKSPACE_INSPECT_READ_ONLY_TOOLS: [&str; 4] = ["read_file", "grep", "glob", "list_dir"];
const RESERVED_MOUNT_NAMESPACE_ROOTS: &[&str] = &["llm", "mem", "route"];

type DelegatedSkillSpawnResult<T> = std::result::Result<T, Box<DelegatedSkillResult>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VirtualToolOutcome {
    NotVirtual,
    Continue { refresh_context: bool },
    PauseTurn,
    EndTurn,
}

pub(super) fn virtual_tool_definitions(include_delegated_skill: bool) -> Vec<ToolDefinition> {
    let mut defs = vec![
        request_confirmation_tool_definition(),
        request_mount_tool_definition(),
        request_user_input_tool_definition(),
        update_plan_tool_definition(),
    ];
    if include_delegated_skill {
        defs.push(invoke_delegated_skill_tool_definition());
        defs.push(terminate_child_run_tool_definition());
    }
    defs
}

pub(super) async fn try_handle_virtual_tool_call<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if cancel.is_cancelled() && check_turn_cancelled(state, emit, cancel).await? {
        return Ok(VirtualToolOutcome::EndTurn);
    }

    match tool_call.name.as_str() {
        "request_confirmation" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;

            if let Some(mut pending) = parse_confirmation_request(&tool_call.id, tool_arguments) {
                pending.details = append_skill_permission_hints(
                    pending.details,
                    state.turn_state.active_skills(),
                );
                let request_id = state
                    .write_namespace_confirmation_request(&pending)
                    .await?
                    .unwrap_or_else(|| pending.checkpoint_id.clone());
                let pending_payload = json!({
                    "status": "pending_confirmation",
                    "request_id": request_id.clone()
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(true),
                    result_preview: tool_result_preview(&pending_payload),
                    audit: None,
                })
                .await;
                state.session.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    pending_payload,
                    true,
                );
                state
                    .turn_state
                    .set_confirmation_for_request(request_id.clone(), pending.clone());
                emit(Event::Yield {
                    request_id,
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    payload: serde_json::to_value(ConfirmationYieldPayload {
                        checkpoint_type: pending.checkpoint_type.clone(),
                        summary: pending.summary.clone(),
                        details: Some(pending.details.clone()),
                        default_option: pending
                            .options
                            .iter()
                            .find(|option| option.as_str() == "approve")
                            .cloned()
                            .or_else(|| pending.options.first().cloned()),
                        options: pending.options.clone(),
                        presentation_hints: vec![],
                    })
                    .unwrap_or_else(|_| json!({})),
                })
                .await;
            } else {
                let error_payload = json!({
                    "status": "invalid_request",
                    "error": "Invalid confirmation request."
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&error_payload),
                    audit: None,
                })
                .await;
                state.session.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    error_payload,
                    false,
                );
                emit(Event::Error {
                    message: "Invalid confirmation request.".to_string(),
                    recoverable: true,
                })
                .await;
                return Ok(VirtualToolOutcome::EndTurn);
            }
            Ok(VirtualToolOutcome::PauseTurn)
        }
        "request_mount" => handle_request_mount(state, tool_call, tool_arguments, emit).await,
        "request_user_input" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;

            if let Some(request) =
                parse_structured_user_input_request(&tool_call.id, tool_arguments)
            {
                let request_id = state
                    .write_namespace_structured_input_request(&request)
                    .await?
                    .unwrap_or_else(|| request.request_id.clone());
                let pending_payload =
                    json!({"status": "pending_structured_input", "request_id": request_id.clone()});
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(true),
                    result_preview: tool_result_preview(&pending_payload),
                    audit: None,
                })
                .await;
                state.session.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    pending_payload,
                    true,
                );
                state
                    .turn_state
                    .set_structured_input_for_request(request_id.clone(), request.clone());
                emit(Event::Yield {
                    request_id,
                    kind: alan_agent_protocol::YieldKind::StructuredInput,
                    payload: serde_json::to_value(structured_input_yield_payload(
                        &state.session.client_capabilities,
                        request.title.clone(),
                        request.prompt.clone(),
                        request.questions.clone(),
                    ))
                    .unwrap_or_else(|_| json!({})),
                })
                .await;
            } else {
                let error_payload = json!({
                    "status": "invalid_request",
                    "error": "Invalid structured user input request."
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&error_payload),
                    audit: None,
                })
                .await;
                state.session.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    error_payload,
                    false,
                );
                emit(Event::Error {
                    message: "Invalid structured user input request.".to_string(),
                    recoverable: true,
                })
                .await;
                return Ok(VirtualToolOutcome::EndTurn);
            }
            Ok(VirtualToolOutcome::PauseTurn)
        }
        "update_plan" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;
            match parse_plan_update(tool_arguments) {
                Some((explanation, items)) => {
                    state
                        .turn_state
                        .set_plan_snapshot(explanation.clone(), items.clone());
                    let payload = json!({
                        "status": "plan_updated",
                        "explanation": explanation,
                        "items": items.clone(),
                        "items_count": items.len()
                    });
                    emit(Event::ToolCallCompleted {
                        presentation: None,
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        success: Some(true),
                        result_preview: tool_result_preview(&payload),
                        audit: None,
                    })
                    .await;
                    emit(Event::PlanUpdated {
                        explanation: explanation.clone(),
                        items: items.clone(),
                    })
                    .await;
                    state.session.record_tool_call(
                        &tool_call.name,
                        tool_arguments.clone(),
                        payload.clone(),
                        true,
                    );
                    state
                        .session
                        .add_tool_message(&tool_call.id, &tool_call.name, payload);
                    Ok(VirtualToolOutcome::Continue {
                        refresh_context: true,
                    })
                }
                None => {
                    let error_payload = json!({
                        "status": "invalid_request",
                        "error": "Invalid plan update payload."
                    });
                    emit(Event::ToolCallCompleted {
                        presentation: None,
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        success: Some(false),
                        result_preview: tool_result_preview(&error_payload),
                        audit: None,
                    })
                    .await;
                    state.session.record_tool_call(
                        &tool_call.name,
                        tool_arguments.clone(),
                        error_payload,
                        false,
                    );
                    emit(Event::Error {
                        message: "Invalid plan update payload.".to_string(),
                        recoverable: true,
                    })
                    .await;
                    Ok(VirtualToolOutcome::Continue {
                        refresh_context: false,
                    })
                }
            }
        }
        "invoke_delegated_skill" => {
            // A host may provide a real delegated-execution bridge as a dynamic tool.
            // In that case, do not shadow it with the runtime placeholder branch.
            if state
                .session
                .dynamic_tools
                .contains_key("invoke_delegated_skill")
            {
                return Ok(VirtualToolOutcome::NotVirtual);
            }
            handle_invoke_delegated_skill(
                state,
                tool_call,
                tool_arguments,
                cancel,
                emit,
                |state, spec, cancel| Box::pin(spawn_and_join_delegated_child(state, spec, cancel)),
            )
            .await
        }
        "terminate_child_run" => {
            handle_terminate_child_run(
                state,
                tool_call,
                tool_arguments,
                allow_approved_tool_escalation_execution,
                emit,
            )
            .await
        }
        _ => Ok(VirtualToolOutcome::NotVirtual),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MountRequestAccess {
    ReadOnly,
    ReadWrite,
}

impl MountRequestAccess {
    fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }

    fn policy_capability(self) -> alan_agent_protocol::ToolCapability {
        match self {
            Self::ReadOnly => alan_agent_protocol::ToolCapability::Read,
            Self::ReadWrite => alan_agent_protocol::ToolCapability::Write,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct MountRequest {
    pub namespace_path: String,
    pub host_path: PathBuf,
    pub access: MountRequestAccess,
    pub reason: String,
}

impl MountRequest {
    pub(super) fn payload(&self) -> serde_json::Value {
        json!({
            "namespace_path": &self.namespace_path,
            "host_path": self.host_path.display().to_string(),
            "access": self.access.as_str(),
            "reason": &self.reason,
        })
    }
}

async fn handle_request_mount<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    emit(Event::ToolCallStarted {
        title: None,
        id: tool_call.id.clone(),
        name: tool_call.name.clone(),
        audit: None,
    })
    .await;

    let mount_request = match parse_mount_request(tool_arguments) {
        Ok(request) => request,
        Err(error) => {
            let payload = json!({
                "status": "invalid_request",
                "error": error,
            });
            emit(Event::ToolCallCompleted {
                presentation: None,
                id: tool_call.id.clone(),
                name: Some(tool_call.name.clone()),
                success: Some(false),
                result_preview: tool_result_preview(&payload),
                audit: None,
            })
            .await;
            state.session.record_tool_call(
                &tool_call.name,
                tool_arguments.clone(),
                payload.clone(),
                false,
            );
            state
                .session
                .add_tool_message(&tool_call.id, &tool_call.name, payload);
            return Ok(VirtualToolOutcome::Continue {
                refresh_context: true,
            });
        }
    };
    let mount_payload = mount_request.payload();
    let sandbox_confinement = super::tool_policy::SandboxConfinement::detect();

    let mut decision = evaluate_tool_policy(
        &state.runtime_config.policy_engine,
        &state.runtime_config.governance,
        &tool_call.name,
        &mount_payload,
        mount_request.access.policy_capability(),
        state.default_tool_cwd().as_deref(),
        sandbox_confinement,
    );
    if mount_request.access == MountRequestAccess::ReadWrite {
        let read_decision = evaluate_tool_policy(
            &state.runtime_config.policy_engine,
            &state.runtime_config.governance,
            &tool_call.name,
            &mount_payload,
            alan_agent_protocol::ToolCapability::Read,
            state.default_tool_cwd().as_deref(),
            sandbox_confinement,
        );
        decision = merge_mount_policy_decision(decision, read_decision);
    }
    let decision_audit = match &decision {
        ToolPolicyDecision::Allow { audit }
        | ToolPolicyDecision::Escalate { audit, .. }
        | ToolPolicyDecision::Forbidden { audit, .. } => audit.clone(),
    };

    if let ToolPolicyDecision::Forbidden { reason, audit } = decision {
        let payload = json!({
            "status": "blocked_by_policy",
            "error": reason,
            "mount_request": mount_payload,
        });
        state.session.record_event(
            "tool_policy_decision",
            json!({
                "tool_call_id": tool_call.id,
                "tool_name": tool_call.name,
                "policy_source": audit.policy_source,
                "rule_id": audit.rule_id,
                "action": audit.action,
                "reason": audit.reason,
                "capability": audit.capability,
                "sandbox_backend": audit.sandbox_backend,
                "path_mode": audit.path_mode,
            }),
        );
        emit(Event::Error {
            message: payload["error"]
                .as_str()
                .unwrap_or("Mount request blocked by policy")
                .to_string(),
            recoverable: true,
        })
        .await;
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&payload),
            audit: Some(audit.clone()),
        })
        .await;
        state.session.record_tool_call_with_audit(
            &tool_call.name,
            tool_arguments.clone(),
            payload.clone(),
            false,
            Some(audit),
        );
        state
            .session
            .add_tool_message(&tool_call.id, &tool_call.name, payload);
        return Ok(VirtualToolOutcome::Continue {
            refresh_context: false,
        });
    }

    let escalation_audit = alan_agent_protocol::ToolDecisionAudit {
        policy_source: decision_audit.policy_source.clone(),
        rule_id: decision_audit
            .rule_id
            .clone()
            .or_else(|| Some("review-host-mount".to_string())),
        action: "escalate".to_string(),
        reason: Some("host mount grants require approval".to_string()),
        capability: decision_audit.capability.clone(),
        sandbox_backend: decision_audit.sandbox_backend.clone(),
        path_mode: decision_audit.path_mode.clone(),
    };
    state.session.record_event(
        "tool_policy_decision",
        json!({
            "tool_call_id": tool_call.id,
            "tool_name": tool_call.name,
            "policy_source": escalation_audit.policy_source,
            "rule_id": escalation_audit.rule_id,
            "action": escalation_audit.action,
            "reason": escalation_audit.reason,
            "capability": escalation_audit.capability,
            "sandbox_backend": escalation_audit.sandbox_backend,
            "path_mode": escalation_audit.path_mode,
            "original_action": decision_audit.action,
        }),
    );

    let mut details = json!({
        "kind": "mount_escalation",
        "tool_call_id": tool_call.id,
        "tool_name": tool_call.name,
        "mount_request": mount_payload,
        "policy": {
            "policy_source": escalation_audit.policy_source,
            "rule_id": escalation_audit.rule_id,
            "action": escalation_audit.action,
            "reason": escalation_audit.reason,
            "capability": escalation_audit.capability,
            "sandbox_backend": escalation_audit.sandbox_backend,
            "path_mode": escalation_audit.path_mode,
        },
        "live_applied": false,
    });
    details = append_skill_permission_hints(details, state.turn_state.active_skills());

    let pending = PendingConfirmation {
        checkpoint_id: format!("{MOUNT_ESCALATION_CHECKPOINT_PREFIX}{}", tool_call.id),
        checkpoint_type: MOUNT_ESCALATION_CHECKPOINT_TYPE.to_string(),
        summary: format!(
            "Approve host mount {} at {}?",
            mount_request.host_path.display(),
            mount_request.namespace_path
        ),
        details,
        options: vec!["approve".to_string(), "reject".to_string()],
    };

    let request_id = state
        .write_namespace_confirmation_request(&pending)
        .await?
        .unwrap_or_else(|| pending.checkpoint_id.clone());
    let payload = json!({
        "status": "pending_mount_approval",
        "request_id": request_id.clone(),
        "mount_request": mount_payload,
        "live_applied": false,
    });
    emit(Event::ToolCallCompleted {
        presentation: None,
        id: tool_call.id.clone(),
        name: Some(tool_call.name.clone()),
        success: Some(true),
        result_preview: tool_result_preview(&payload),
        audit: Some(escalation_audit.clone()),
    })
    .await;
    state.session.record_tool_call_with_audit(
        &tool_call.name,
        tool_arguments.clone(),
        payload.clone(),
        true,
        Some(escalation_audit),
    );
    state
        .turn_state
        .set_confirmation_for_request(request_id.clone(), pending.clone());
    emit(Event::Yield {
        request_id,
        kind: YieldKind::Confirmation,
        payload: serde_json::to_value(ConfirmationYieldPayload {
            checkpoint_type: pending.checkpoint_type.clone(),
            summary: pending.summary.clone(),
            details: Some(pending.details.clone()),
            options: pending.options.clone(),
            default_option: Some("reject".to_string()),
            presentation_hints: vec![AdaptivePresentationHint::Dangerous],
        })
        .unwrap_or_else(|_| json!({})),
    })
    .await;

    Ok(VirtualToolOutcome::PauseTurn)
}

fn merge_mount_policy_decision(
    primary_decision: ToolPolicyDecision,
    read_decision: ToolPolicyDecision,
) -> ToolPolicyDecision {
    let primary_allows = matches!(&primary_decision, ToolPolicyDecision::Allow { .. });
    match read_decision {
        decision @ ToolPolicyDecision::Forbidden { .. } => decision,
        decision @ ToolPolicyDecision::Escalate { .. } if primary_allows => decision,
        _ => primary_decision,
    }
}

pub(super) fn parse_mount_request(
    args: &serde_json::Value,
) -> std::result::Result<MountRequest, String> {
    let namespace_path = parse_mount_namespace_path(required_string(args, "namespace_path")?)?;
    let host_path = parse_mount_host_path(required_string(args, "host_path")?)?;
    let access = parse_mount_access(required_string(args, "access")?)?;
    let reason = required_string(args, "reason")?;

    if reason.trim().is_empty() {
        return Err("reason must be a non-empty string".to_string());
    }

    Ok(MountRequest {
        namespace_path,
        host_path,
        access,
        reason: reason.trim().to_string(),
    })
}

fn required_string<'a>(
    args: &'a serde_json::Value,
    field: &str,
) -> std::result::Result<&'a str, String> {
    args.get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{field} must be a string"))
}

fn parse_mount_namespace_path(raw: &str) -> std::result::Result<String, String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("/mnt/") {
        return Err("namespace_path must be an absolute path under /mnt/<name>".to_string());
    }

    let components = trimmed
        .trim_start_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    if components.len() < 2 || components.first() != Some(&"mnt") {
        return Err("namespace_path must be an absolute path under /mnt/<name>".to_string());
    }
    if components
        .iter()
        .any(|component| component.is_empty() || *component == "." || *component == "..")
    {
        return Err(
            "namespace_path must not contain empty, '.', or '..' path components".to_string(),
        );
    }
    if RESERVED_MOUNT_NAMESPACE_ROOTS.contains(&components[1]) {
        return Err("namespace_path targets a reserved mount root".to_string());
    }

    Ok(format!("/{}", components.join("/")))
}

fn parse_mount_host_path(raw: &str) -> std::result::Result<PathBuf, String> {
    let trimmed = raw.trim();
    let path = Path::new(trimmed);
    if is_windows_filesystem_root_path(trimmed) || path == Path::new("/") {
        return Err("host_path must not be the host filesystem root".to_string());
    }
    if !path.is_absolute() {
        return Err("host_path must be absolute".to_string());
    }
    if has_invalid_raw_host_path_segment(trimmed)
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err("host_path must not contain '.', '..', or empty components".to_string());
    }
    Ok(path.to_path_buf())
}

fn is_windows_filesystem_root_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    is_windows_drive_root_path(normalized.as_str())
        || normalized
            .strip_prefix("//?/")
            .is_some_and(is_windows_drive_root_path)
        || normalized
            .strip_prefix("//./")
            .is_some_and(is_windows_drive_root_path)
        || normalized
            .strip_prefix("//?/UNC/")
            .is_some_and(is_windows_unc_share_root_path)
        || normalized
            .strip_prefix("//./UNC/")
            .is_some_and(is_windows_unc_share_root_path)
        || normalized
            .strip_prefix("//")
            .is_some_and(is_windows_unc_share_root_path)
}

fn is_windows_drive_root_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2..].iter().all(|byte| *byte == b'/')
}

fn is_windows_unc_share_root_path(path: &str) -> bool {
    let parts = path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    parts.len() == 2
}

fn has_invalid_raw_host_path_segment(path: &str) -> bool {
    let mut segments = path.split('/');
    let Some(first) = segments.next() else {
        return true;
    };
    if !first.is_empty() {
        return false;
    }
    segments.any(|segment| segment.is_empty() || segment == "." || segment == "..")
}

fn parse_mount_access(raw: &str) -> std::result::Result<MountRequestAccess, String> {
    match raw.trim() {
        "read_only" => Ok(MountRequestAccess::ReadOnly),
        "read_write" => Ok(MountRequestAccess::ReadWrite),
        _ => Err("access must be one of: read_only, read_write".to_string()),
    }
}

async fn handle_terminate_child_run<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let Some((child_run_id, reason, mode)) = parse_terminate_child_run_request(tool_arguments)
    else {
        let audit = runtime_virtual_tool_audit("invalid child-run termination payload");
        let payload = json!({
            "status": "invalid_request",
            "error": "Invalid child-run termination payload."
        });
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&payload),
            audit: Some(audit.clone()),
        })
        .await;
        state.session.record_tool_call_with_audit(
            &tool_call.name,
            tool_arguments.clone(),
            payload.clone(),
            false,
            Some(audit),
        );
        state
            .session
            .add_tool_message(&tool_call.id, &tool_call.name, payload);
        return Ok(VirtualToolOutcome::Continue {
            refresh_context: true,
        });
    };

    let audit = match evaluate_terminate_child_run_policy(
        state,
        tool_call,
        tool_arguments,
        allow_approved_tool_escalation_execution,
        emit,
    )
    .await?
    {
        TerminateChildRunPolicyOutcome::Allow(audit) => audit,
        TerminateChildRunPolicyOutcome::PauseTurn => return Ok(VirtualToolOutcome::PauseTurn),
        TerminateChildRunPolicyOutcome::Continue { refresh_context } => {
            return Ok(VirtualToolOutcome::Continue { refresh_context });
        }
    };

    emit(Event::ToolCallStarted {
        title: None,
        id: tool_call.id.clone(),
        name: tool_call.name.clone(),
        audit: Some(audit.clone()),
    })
    .await;

    let result = global_child_run_registry().request_termination(
        &state.session.id,
        &child_run_id,
        "parent_runtime",
        mode,
        reason,
    );
    let (payload, success) = match result {
        Ok(record) => (
            json!({
                "status": "termination_requested",
                "child_run": record
            }),
            true,
        ),
        Err(ChildRunRegistryError::AlreadyTerminal(record)) => (
            json!({
                "status": "already_terminal",
                "child_run": record
            }),
            true,
        ),
        Err(ChildRunRegistryError::NotFound) => (
            json!({
                "status": "not_found",
                "error": "Child run not found for this parent session.",
                "child_run_id": child_run_id
            }),
            false,
        ),
    };

    emit(Event::ToolCallCompleted {
        presentation: None,
        id: tool_call.id.clone(),
        name: Some(tool_call.name.clone()),
        success: Some(success),
        result_preview: tool_result_preview(&payload),
        audit: Some(audit.clone()),
    })
    .await;
    state.session.record_tool_call_with_audit(
        &tool_call.name,
        tool_arguments.clone(),
        payload.clone(),
        success,
        Some(audit),
    );
    state
        .session
        .add_tool_message(&tool_call.id, &tool_call.name, payload);
    Ok(VirtualToolOutcome::Continue {
        refresh_context: true,
    })
}

fn runtime_virtual_tool_audit(reason: &str) -> alan_agent_protocol::ToolDecisionAudit {
    alan_agent_protocol::ToolDecisionAudit {
        policy_source: "runtime_virtual_tool".to_string(),
        rule_id: None,
        action: "allow".to_string(),
        reason: Some(reason.to_string()),
        capability: "write".to_string(),
        sandbox_backend: crate::tools::active_backend_name().to_string(),
        path_mode: Some(crate::tools::active_backend_path_mode().to_string()),
    }
}

enum TerminateChildRunPolicyOutcome {
    Allow(alan_agent_protocol::ToolDecisionAudit),
    PauseTurn,
    Continue { refresh_context: bool },
}

async fn evaluate_terminate_child_run_policy<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<TerminateChildRunPolicyOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let policy_decision = maybe_allow_approved_virtual_tool_escalation_replay(
        evaluate_tool_policy(
            &state.runtime_config.policy_engine,
            &state.runtime_config.governance,
            &tool_call.name,
            tool_arguments,
            alan_agent_protocol::ToolCapability::Write,
            state.default_tool_cwd().as_deref(),
            super::tool_policy::SandboxConfinement::detect(),
        ),
        allow_approved_tool_escalation_execution,
    );
    let policy_audit = match &policy_decision {
        ToolPolicyDecision::Allow { audit }
        | ToolPolicyDecision::Escalate { audit, .. }
        | ToolPolicyDecision::Forbidden { audit, .. } => audit.clone(),
    };
    state.session.record_event(
        "tool_policy_decision",
        json!({
            "tool_call_id": tool_call.id,
            "tool_name": tool_call.name,
            "policy_source": policy_audit.policy_source,
            "rule_id": policy_audit.rule_id,
            "action": policy_audit.action,
            "reason": policy_audit.reason,
            "capability": policy_audit.capability,
            "sandbox_backend": policy_audit.sandbox_backend,
            "path_mode": policy_audit.path_mode,
        }),
    );

    match policy_decision {
        ToolPolicyDecision::Allow { audit } => Ok(TerminateChildRunPolicyOutcome::Allow(audit)),
        ToolPolicyDecision::Escalate {
            summary,
            mut details,
            audit,
            route: _,
        } => {
            details["replay_tool_call"] = json!({
                "call_id": tool_call.id,
                "tool_name": tool_call.name,
                "arguments": tool_arguments,
            });
            details = append_skill_permission_hints(details, state.turn_state.active_skills());
            let pending = PendingConfirmation {
                checkpoint_id: format!("{TOOL_ESCALATION_CHECKPOINT_PREFIX}{}", tool_call.id),
                checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary,
                details,
                options: vec!["approve".to_string(), "reject".to_string()],
            };
            state.session.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                json!({"status":"escalation_required"}),
                true,
                Some(audit),
            );
            let request_id = state
                .write_namespace_confirmation_request(&pending)
                .await?
                .unwrap_or_else(|| pending.checkpoint_id.clone());
            state
                .turn_state
                .set_confirmation_for_request(request_id.clone(), pending.clone());
            emit(Event::Yield {
                request_id,
                kind: YieldKind::Confirmation,
                payload: serde_json::to_value(ConfirmationYieldPayload {
                    checkpoint_type: pending.checkpoint_type.clone(),
                    summary: pending.summary.clone(),
                    details: Some(pending.details.clone()),
                    options: pending.options.clone(),
                    default_option: Some("approve".to_string()),
                    presentation_hints: vec![AdaptivePresentationHint::Dangerous],
                })
                .unwrap_or_else(|_| json!({})),
            })
            .await;
            Ok(TerminateChildRunPolicyOutcome::PauseTurn)
        }
        ToolPolicyDecision::Forbidden { reason, audit } => {
            let blocked_payload = json!({
                "status": "blocked_by_policy",
                "error": reason
            });
            emit(Event::Error {
                message: blocked_payload["error"]
                    .as_str()
                    .unwrap_or("Tool blocked by policy")
                    .to_string(),
                recoverable: true,
            })
            .await;
            emit(Event::ToolCallCompleted {
                presentation: None,
                id: tool_call.id.clone(),
                name: Some(tool_call.name.clone()),
                success: Some(false),
                result_preview: tool_result_preview(&blocked_payload),
                audit: Some(audit.clone()),
            })
            .await;
            state.session.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                blocked_payload.clone(),
                false,
                Some(audit),
            );
            state
                .session
                .add_tool_message(&tool_call.id, &tool_call.name, blocked_payload);
            Ok(TerminateChildRunPolicyOutcome::Continue {
                refresh_context: false,
            })
        }
    }
}

fn maybe_allow_approved_virtual_tool_escalation_replay(
    policy_decision: ToolPolicyDecision,
    allow_approved_tool_escalation_execution: bool,
) -> ToolPolicyDecision {
    match policy_decision {
        ToolPolicyDecision::Escalate { audit, .. } if allow_approved_tool_escalation_execution => {
            ToolPolicyDecision::Allow {
                audit: alan_agent_protocol::ToolDecisionAudit {
                    action: "allow".to_string(),
                    reason: Some("approved tool escalation replay".to_string()),
                    ..audit
                },
            }
        }
        other => other,
    }
}

async fn handle_invoke_delegated_skill<E, F, S>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    emit: &mut E,
    spawn_child: S,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
    S: for<'a> FnOnce(
        &'a RuntimeLoopState,
        SpawnSpec,
        &'a CancellationToken,
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<ChildRuntimeResult>> + Send + 'a>,
    >,
{
    emit(Event::ToolCallStarted {
        title: None,
        id: tool_call.id.clone(),
        name: tool_call.name.clone(),
        audit: None,
    })
    .await;

    let Some(request) = parse_delegated_skill_invocation_request(tool_arguments) else {
        let error_payload = json!({
            "status": "invalid_request",
            "error": "Invalid delegated skill invocation payload."
        });
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&error_payload),
            audit: None,
        })
        .await;
        state.session.record_tool_call(
            &tool_call.name,
            tool_arguments.clone(),
            error_payload.clone(),
            false,
        );
        state
            .session
            .add_tool_message(&tool_call.id, &tool_call.name, error_payload.clone());
        emit(Event::Error {
            message: "Invalid delegated skill invocation payload.".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(VirtualToolOutcome::Continue {
            refresh_context: true,
        });
    };

    if !state.prompt_cache.supports_delegated_skill_invocation() {
        let error_payload = json!({
            "status": "delegated_invocation_unavailable",
            "error": "Delegated skill invocation is not available in this runtime."
        });
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&error_payload),
            audit: None,
        })
        .await;
        state.session.record_tool_call(
            &tool_call.name,
            tool_arguments.clone(),
            error_payload.clone(),
            false,
        );
        state
            .session
            .add_tool_message(&tool_call.id, &tool_call.name, error_payload);
        emit(Event::Error {
            message: "Delegated skill invocation is not available in this runtime.".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(VirtualToolOutcome::Continue {
            refresh_context: true,
        });
    }

    let (persisted_request, result, child_run) =
        match resolve_delegated_skill_invocation(state, &request) {
            Ok(spec) => {
                let persisted_request = request.with_effective_launch_inputs(
                    spec.launch.workspace_root.clone(),
                    spec.launch.cwd.clone(),
                    spec.launch.timeout_secs,
                );
                match spawn_child(state, spec, cancel).await {
                    Ok(mut child_result) => {
                        if cancel.is_cancelled()
                            && matches!(child_result.status, ChildRuntimeStatus::Cancelled)
                            && check_turn_cancelled(state, emit, cancel).await?
                        {
                            return Ok(VirtualToolOutcome::EndTurn);
                        }

                        let output_reference =
                            persist_delegated_child_evidence(state, &request, &child_result).await;
                        if let (Some(child_run_id), Some(reference)) = (
                            child_result.child_run_id.as_deref(),
                            output_reference.as_ref(),
                        ) {
                            global_child_run_registry()
                                .set_state_ref(child_run_id, reference.clone());
                            child_result.child_run = global_child_run_registry().get(child_run_id);
                        }
                        (
                            persisted_request,
                            delegated_result_from_child_result(&child_result, output_reference),
                            Some(delegated_child_run_reference(&child_result)),
                        )
                    }
                    Err(err) => {
                        if cancel.is_cancelled()
                            && check_turn_cancelled(state, emit, cancel).await?
                        {
                            return Ok(VirtualToolOutcome::EndTurn);
                        }

                        let capability_decision = err
                            .downcast_ref::<DelegatedSpawnRejected>()
                            .map(|rejection| rejection.decision.clone());
                        let error_kind = if capability_decision.is_some() {
                            "delegated_capability_mismatch"
                        } else {
                            "child_launch_failed"
                        };
                        let mut result = DelegatedSkillResult::failed(
                            format!(
                                "Failed to launch delegated runtime for skill '{}': {err}",
                                request.skill_id
                            ),
                            Some(json!({
                                "error_kind": error_kind
                            })),
                        );
                        result.error_kind = Some(error_kind.to_string());
                        result.capability_decision = capability_decision;
                        (persisted_request, result, None)
                    }
                }
            }
            Err(result) => (request.clone(), *result, None),
        };

    let (persisted_arguments, tape_record, rollout_record) =
        build_bounded_delegated_invocation_persistence(&persisted_request, result, child_run);
    let preview = tool_result_preview(&json!(tape_record.result.summary.clone()));
    let tape_payload = serde_json::to_value(&tape_record).unwrap_or_else(|_| {
        json!({
            "status": "invalid_result_encoding",
            "error": "Failed to serialize delegated skill result."
        })
    });
    let rollout_payload =
        serde_json::to_value(&rollout_record).unwrap_or_else(|_| tape_payload.clone());
    let invocation_succeeded = matches!(
        tape_record.result.status,
        DelegatedSkillResultStatus::Completed
    );
    emit(Event::ToolCallCompleted {
        presentation: None,
        id: tool_call.id.clone(),
        name: Some(tool_call.name.clone()),
        success: Some(invocation_succeeded),
        result_preview: preview,
        audit: None,
    })
    .await;
    state.session.record_tool_call(
        &tool_call.name,
        persisted_arguments,
        rollout_payload,
        invocation_succeeded,
    );
    state
        .session
        .add_tool_message(&tool_call.id, &tool_call.name, tape_payload);
    Ok(VirtualToolOutcome::Continue {
        refresh_context: true,
    })
}

async fn spawn_and_join_delegated_child(
    state: &RuntimeLoopState,
    spec: SpawnSpec,
    cancel: &CancellationToken,
) -> Result<ChildRuntimeResult> {
    if cancel.is_cancelled() {
        return Ok(ChildRuntimeResult {
            status: ChildRuntimeStatus::Cancelled,
            session_id: String::new(),
            child_run_id: None,
            rollout_path: None,
            output_text: String::new(),
            turn_summary: None,
            structured_output: None,
            warnings: Vec::new(),
            error_message: None,
            pause: None,
            child_run: None,
        });
    }

    let controller = spawn_child_runtime_cancellable(state, spec, cancel).await?;
    controller.join_until_cancelled(cancel).await
}

fn resolve_delegated_skill_invocation(
    state: &mut RuntimeLoopState,
    request: &DelegatedSkillInvocationRequest,
) -> DelegatedSkillSpawnResult<SpawnSpec> {
    let active_skill = state
        .turn_state
        .active_skills()
        .iter()
        .find(|skill| skill.metadata.id == request.skill_id)
        .cloned();

    let skill_metadata = if let Some(skill) = active_skill {
        if !skill.availability.is_available() {
            return Err(Box::new(DelegatedSkillResult::failed(
                format!(
                    "Delegated skill '{}' is {}.",
                    request.skill_id,
                    skill.availability.render_label()
                ),
                Some(json!({
                    "error_kind": "skill_unavailable"
                })),
            )));
        }
        skill.metadata
    } else {
        match state
            .prompt_cache
            .resolve_listed_skill_metadata(request.skill_id.as_str())
        {
            Ok(Some(metadata)) => metadata,
            Ok(None) => {
                return Err(Box::new(DelegatedSkillResult::failed(
                    format!(
                        "Delegated skill '{}' is not active and is not listed for implicit use in the current runtime.",
                        request.skill_id
                    ),
                    Some(json!({
                        "error_kind": "skill_not_visible"
                    })),
                )));
            }
            Err(err) => {
                return Err(Box::new(DelegatedSkillResult::failed(
                    format!(
                        "Failed to resolve delegated skill '{}' from the runtime catalog: {err}",
                        request.skill_id
                    ),
                    Some(json!({
                        "error_kind": "skill_resolution_failed"
                    })),
                )));
            }
        }
    };

    let Some(resolved_target) = skill_metadata.execution.delegate_target() else {
        return Err(Box::new(DelegatedSkillResult::failed(
            format!(
                "Skill '{}' is not resolved for delegated execution.",
                request.skill_id
            ),
            Some(json!({
                "error_kind": "skill_not_delegated"
            })),
        )));
    };

    if resolved_target != request.target {
        return Err(Box::new(DelegatedSkillResult::failed(
            format!(
                "Delegated skill '{}' resolves to delegated target '{}' rather than '{}'.",
                request.skill_id, resolved_target, request.target
            ),
            Some(json!({
                "error_kind": "delegate_target_mismatch",
                "resolved_target": resolved_target
            })),
        )));
    }

    let Some(spawn_target) = skill_metadata.delegated_spawn_target() else {
        return Err(Box::new(DelegatedSkillResult::failed(
            format!(
                "Delegated skill '{}' does not expose a package-local launch target.",
                request.skill_id
            ),
            Some(json!({
                "error_kind": "delegate_target_missing"
            })),
        )));
    };

    build_delegated_spawn_spec(state, request, spawn_target)
}

fn build_delegated_spawn_spec(
    state: &RuntimeLoopState,
    request: &DelegatedSkillInvocationRequest,
    target: alan_agent_protocol::SpawnTarget,
) -> DelegatedSkillSpawnResult<SpawnSpec> {
    let inferred_workspace_root = bound_workspace_root(state);
    let parent_default_cwd = state.default_tool_cwd();
    let workspace_root = normalize_delegated_workspace_root(
        request.workspace_root.as_deref(),
        parent_default_cwd.as_deref(),
        inferred_workspace_root.as_deref(),
    )?;
    let cwd = normalize_delegated_cwd(
        request.cwd.as_deref(),
        workspace_root.as_deref(),
        request.workspace_root.is_some(),
        parent_default_cwd.as_deref(),
    )?;
    let requirements =
        classify_delegated_task_requirements(&request.task, workspace_root.as_deref());
    Ok(SpawnSpec {
        target,
        launch: SpawnLaunchInputs {
            task: request.task.clone(),
            cwd,
            workspace_root,
            timeout_secs: Some(
                request
                    .timeout_secs
                    .unwrap_or(DEFAULT_DELEGATED_TIMEOUT_SECS),
            ),
            output_dir: None,
        },
        handles: vec![SpawnHandle::Workspace, SpawnHandle::ApprovalScope],
        runtime_overrides: delegated_runtime_overrides(request.skill_id.as_str()),
        delegated: Some(DelegatedSpawnContext { requirements }),
    })
}

fn normalize_delegated_workspace_root(
    requested_workspace_root: Option<&Path>,
    parent_default_cwd: Option<&Path>,
    inferred_workspace_root: Option<&Path>,
) -> DelegatedSkillSpawnResult<Option<PathBuf>> {
    match requested_workspace_root {
        None => Ok(inferred_workspace_root.map(lexically_normalize_path)),
        Some(path) => resolve_delegated_launch_path(
            path,
            parent_default_cwd.or(inferred_workspace_root),
            "workspace_root",
        )
        .map(Some),
    }
}

fn normalize_delegated_cwd(
    requested_cwd: Option<&Path>,
    workspace_root: Option<&Path>,
    explicit_workspace_root_provided: bool,
    parent_default_cwd: Option<&Path>,
) -> DelegatedSkillSpawnResult<Option<PathBuf>> {
    match requested_cwd {
        Some(path) => {
            resolve_delegated_launch_path(path, workspace_root.or(parent_default_cwd), "cwd")
                .map(Some)
        }
        None => {
            if explicit_workspace_root_provided {
                Ok(workspace_root.map(Path::to_path_buf))
            } else {
                Ok(parent_default_cwd.map(lexically_normalize_path))
            }
        }
    }
}

fn resolve_delegated_launch_path(
    requested_path: &Path,
    base: Option<&Path>,
    field_name: &str,
) -> DelegatedSkillSpawnResult<PathBuf> {
    if requested_path.is_absolute() {
        return Ok(lexically_normalize_path(requested_path));
    }

    let Some(base) = base else {
        return Err(Box::new(DelegatedSkillResult::failed(
            format!(
                "Delegated skill invocation provided relative {field_name} '{}' but the parent runtime has no base path to resolve it.",
                requested_path.display()
            ),
            Some(json!({
                "error_kind": "relative_launch_path_unresolvable",
                "field": field_name
            })),
        )));
    };

    Ok(lexically_normalize_path(&base.join(requested_path)))
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn delegated_runtime_overrides(skill_id: &str) -> SpawnRuntimeOverrides {
    SpawnRuntimeOverrides {
        tool_profile: delegated_tool_profile(skill_id),
        ..SpawnRuntimeOverrides::default()
    }
}

fn delegated_tool_profile(skill_id: &str) -> Option<SpawnToolProfileOverride> {
    match skill_id {
        "workspace-inspect" => Some(SpawnToolProfileOverride {
            allowed_tools: WORKSPACE_INSPECT_READ_ONLY_TOOLS
                .iter()
                .map(|tool| (*tool).to_string())
                .collect(),
        }),
        _ => None,
    }
}

fn delegated_result_from_child_result(
    result: &ChildRuntimeResult,
    output_reference: Option<DelegatedSkillOutputRef>,
) -> DelegatedSkillResult {
    let mut delegated = match result.status {
        ChildRuntimeStatus::Completed => {
            delegated_result_from_completed_child(result, output_reference.as_ref())
        }
        ChildRuntimeStatus::Failed => child_failure_result(
            format!(
                "Delegated runtime failed: {}",
                result
                    .error_message
                    .clone()
                    .or_else(|| non_empty_trimmed(&result.output_text))
                    .unwrap_or_else(|| "unknown failure".to_string())
            ),
            "child_failed",
            result,
            output_reference.as_ref(),
        ),
        ChildRuntimeStatus::TimedOut => child_failure_result(
            "Delegated runtime timed out.".to_string(),
            "child_timed_out",
            result,
            output_reference.as_ref(),
        ),
        ChildRuntimeStatus::Cancelled => child_failure_result(
            "Delegated runtime was cancelled.".to_string(),
            "child_cancelled",
            result,
            output_reference.as_ref(),
        ),
        ChildRuntimeStatus::Terminated => child_failure_result(
            result
                .error_message
                .clone()
                .unwrap_or_else(|| "Delegated runtime was terminated.".to_string()),
            "child_terminated",
            result,
            output_reference.as_ref(),
        ),
        ChildRuntimeStatus::Paused => {
            let (pause_kind, request_id) = result
                .pause
                .as_ref()
                .map(|pause| {
                    (
                        yield_kind_label(&pause.kind),
                        Some(pause.request_id.clone()),
                    )
                })
                .unwrap_or_else(|| ("unknown".to_string(), None));
            let mut delegated = DelegatedSkillResult::failed(
                format!(
                    "Delegated runtime paused for {} and cannot continue in v1 delegated execution.",
                    pause_kind
                ),
                Some(json!({
                    "error_kind": "child_paused",
                    "pause_kind": pause_kind,
                    "request_id": request_id
                })),
            );
            delegated.error_kind = Some("child_paused".to_string());
            delegated.error_message = result.error_message.clone();
            delegated.child_run = child_run_value(result);
            delegated.warnings = result.warnings.clone();
            delegated
        }
    };
    delegated.capability_decision = result
        .child_run
        .as_ref()
        .and_then(|record| record.delegation_capability_decision.clone());
    delegated
}

fn delegated_result_from_completed_child(
    result: &ChildRuntimeResult,
    output_reference: Option<&DelegatedSkillOutputRef>,
) -> DelegatedSkillResult {
    let output_text =
        non_empty_trimmed(&result.output_text).map(|text| redact_durable_evidence_text(&text).text);
    let mut delegated = DelegatedSkillResult::completed(
        completed_child_summary(result),
        result.structured_output.clone(),
    );
    delegated.child_run = child_run_value(result);
    delegated.warnings = result.warnings.clone();

    if let Some(output_text) = output_text {
        let output_chars = output_text.chars().count();
        if output_chars <= MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS {
            delegated.output_text = Some(output_text);
        } else {
            delegated.summary_preview = Some(truncate_text_with_suffix(
                &output_text,
                MAX_DELEGATED_RESULT_SUMMARY_CHARS,
                "... [truncated; inspect output_ref]",
            ));
            delegated.output_ref = output_reference.cloned();
            delegated.truncation = Some(DelegatedSkillResultTruncation {
                output_text: true,
                original_output_chars: Some(output_chars),
                note: Some(if delegated.output_ref.is_some() {
                    "Full child output is available from the namespace output_ref.".to_string()
                } else {
                    "Child output was truncated, and no parent-resolvable evidence path could be emitted; the inline preview is the declared-complete retained record.".to_string()
                }),
                ..DelegatedSkillResultTruncation::default()
            });
        }
    }

    delegated
}

fn child_failure_result(
    summary: String,
    error_kind: &str,
    result: &ChildRuntimeResult,
    output_reference: Option<&DelegatedSkillOutputRef>,
) -> DelegatedSkillResult {
    let mut delegated = DelegatedSkillResult::failed(
        summary.clone(),
        Some(json!({
            "error_kind": error_kind
        })),
    );
    delegated.error_kind = Some(error_kind.to_string());
    delegated.error_message = result.error_message.clone();
    delegated.child_run = child_run_value(result);
    delegated.warnings = result.warnings.clone();
    if !result.output_text.trim().is_empty() {
        delegated.output_ref = output_reference.cloned();
        delegated.truncation = Some(DelegatedSkillResultTruncation {
            output_text: true,
            original_output_chars: Some(result.output_text.chars().count()),
            note: Some(if delegated.output_ref.is_some() {
                "Child produced output before terminal failure; inspect the namespace output_ref."
                    .to_string()
            } else {
                "Child produced output before terminal failure, but no parent-resolvable evidence path could be emitted; only the marked preview is retained."
                    .to_string()
            }),
            ..DelegatedSkillResultTruncation::default()
        });
    }
    delegated
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DelegatedChildRunReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    child_run_id: Option<String>,
    session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    process_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state_ref: Option<DelegatedSkillOutputRef>,
    terminal_status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DelegatedSkillRolloutRecord {
    #[serde(flatten)]
    invocation: DelegatedSkillInvocationRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    child_run: Option<DelegatedChildRunReference>,
}

fn delegated_child_run_reference(result: &ChildRuntimeResult) -> DelegatedChildRunReference {
    DelegatedChildRunReference {
        child_run_id: result.child_run_id.clone(),
        session_id: result.session_id.clone(),
        process_path: result
            .child_run
            .as_ref()
            .and_then(|record| record.process_path.clone()),
        state_ref: result
            .child_run
            .as_ref()
            .and_then(|record| record.state_ref.clone()),
        terminal_status: child_runtime_status_label(result.status.clone()),
    }
}

fn child_run_value(result: &ChildRuntimeResult) -> Option<serde_json::Value> {
    result
        .child_run
        .as_ref()
        .and_then(|record| serde_json::to_value(record).ok())
        .or_else(|| {
            serde_json::to_value(delegated_child_run_reference(result))
                .ok()
                .filter(|value| !value.is_null())
        })
}

async fn persist_delegated_child_evidence(
    state: &RuntimeLoopState,
    request: &DelegatedSkillInvocationRequest,
    result: &ChildRuntimeResult,
) -> Option<DelegatedSkillOutputRef> {
    if result.output_text.trim().is_empty()
        || (matches!(result.status, ChildRuntimeStatus::Completed)
            && result.output_text.chars().count() <= MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS)
    {
        return None;
    }

    let redacted = redact_durable_evidence_text(&result.output_text);
    let result_doc = json!({
        "child_session_id": result.session_id,
        "child_run_id": result.child_run_id,
        "terminal_status": child_runtime_status_label(result.status.clone()),
        "redactions": redacted.markers,
    });
    let action_id = state
        .namespace_environment()
        .write_action(
            NamespaceActionRecord::new(
                format!("delegate:{}", request.skill_id),
                child_runtime_status_label(result.status.clone()),
            )
            .with_output(redacted.text)
            .with_result(result_doc.to_string())
            .with_approval("not_required"),
        )
        .await
        .ok()?;
    let path = format!(
        "{}/actions/{action_id}/output",
        state.namespace_environment().agent_path()
    );
    let reference = state
        .namespace_environment()
        .evidence_reference(path)
        .await?;
    state
        .namespace_environment()
        .resolve_evidence_reference(&reference, None, child_run_value(result))
        .await
        .ok()?;

    Some(DelegatedSkillOutputRef {
        path: reference.path,
        offset: reference.offset,
        length: reference.length,
        debug: Some(DelegatedSkillOutputDebugMetadata {
            session_id: result.session_id.clone(),
            rollout_path: matches!(result.status, ChildRuntimeStatus::Completed)
                .then(|| {
                    result
                        .rollout_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                })
                .flatten(),
            field: "output_text".to_string(),
        }),
    })
}

fn child_runtime_status_label(status: ChildRuntimeStatus) -> String {
    match status {
        ChildRuntimeStatus::Completed => "completed".to_string(),
        ChildRuntimeStatus::Paused => "paused".to_string(),
        ChildRuntimeStatus::Cancelled => "cancelled".to_string(),
        ChildRuntimeStatus::TimedOut => "timed_out".to_string(),
        ChildRuntimeStatus::Terminated => "terminated".to_string(),
        ChildRuntimeStatus::Failed => "failed".to_string(),
    }
}

fn build_bounded_delegated_invocation_persistence(
    request: &DelegatedSkillInvocationRequest,
    result: DelegatedSkillResult,
    child_run: Option<DelegatedChildRunReference>,
) -> (
    serde_json::Value,
    DelegatedSkillInvocationRecord,
    DelegatedSkillRolloutRecord,
) {
    let (arguments, record) = build_bounded_delegated_tape_record(request, result);
    let rollout_record = DelegatedSkillRolloutRecord {
        invocation: record.clone(),
        child_run,
    };
    (arguments, record, rollout_record)
}

fn build_bounded_delegated_tape_record(
    request: &DelegatedSkillInvocationRequest,
    result: DelegatedSkillResult,
) -> (serde_json::Value, DelegatedSkillInvocationRecord) {
    let skill_id =
        truncate_text_with_suffix(&request.skill_id, MAX_DELEGATED_SKILL_ID_CHARS, "...");
    let target = truncate_text_with_suffix(&request.target, MAX_DELEGATED_TARGET_CHARS, "...");
    let task = truncate_text_with_suffix(&request.task, MAX_DELEGATED_TASK_CHARS, "...");
    let mut result = result;
    let summary_chars = result.summary.chars().count();
    if summary_chars > MAX_DELEGATED_RESULT_SUMMARY_CHARS {
        let preview =
            truncate_text_with_suffix(&result.summary, MAX_DELEGATED_RESULT_SUMMARY_CHARS, "...");
        result.summary = preview.clone();
        result.summary_preview = Some(preview);
        let mut truncation = result.truncation.take().unwrap_or_default();
        truncation.summary = true;
        truncation.original_summary_chars = Some(summary_chars);
        result.truncation = Some(truncation);
    }
    if let Some(value) = result.structured_output.take() {
        let serialized_size = serde_json::to_string(&value)
            .map(|text| text.chars().count())
            .unwrap_or(MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS + 1);
        result.structured_output = Some(truncate_structured_output(
            value,
            MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS,
        ));
        if serialized_size > MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS {
            let mut truncation = result.truncation.take().unwrap_or_default();
            truncation.structured_output = true;
            result.truncation = Some(truncation);
        }
    }
    bound_delegated_result_sidecars(&mut result);

    let record = DelegatedSkillInvocationRecord {
        skill_id,
        target,
        task,
        workspace_root: request.workspace_root.as_ref().map(|path| {
            truncate_text_with_suffix(&path.to_string_lossy(), MAX_DELEGATED_PATH_CHARS, "...")
        }),
        cwd: request.cwd.as_ref().map(|path| {
            truncate_text_with_suffix(&path.to_string_lossy(), MAX_DELEGATED_PATH_CHARS, "...")
        }),
        timeout_secs: request.timeout_secs,
        result,
    };
    let mut arguments = json!({
        "skill_id": record.skill_id.clone(),
        "target": record.target.clone(),
        "task": record.task.clone(),
    });
    if let Some(workspace_root) = record.workspace_root.as_ref() {
        arguments["workspace_root"] = json!(workspace_root);
    }
    if let Some(cwd) = record.cwd.as_ref() {
        arguments["cwd"] = json!(cwd);
    }
    if let Some(timeout_secs) = record.timeout_secs {
        arguments["timeout_secs"] = json!(timeout_secs);
    }

    (arguments, record)
}

fn bound_delegated_result_sidecars(result: &mut DelegatedSkillResult) {
    if let Some(value) = result.child_run.take() {
        let serialized_size = serde_json::to_string(&value)
            .map(|text| text.chars().count())
            .unwrap_or(MAX_DELEGATED_CHILD_RUN_METADATA_CHARS + 1);
        result.child_run = Some(truncate_structured_output(
            value,
            MAX_DELEGATED_CHILD_RUN_METADATA_CHARS,
        ));
        if serialized_size > MAX_DELEGATED_CHILD_RUN_METADATA_CHARS {
            let truncation = result.truncation.get_or_insert_with(Default::default);
            truncation.child_run = true;
            truncation.original_child_run_chars = Some(serialized_size);
            append_truncation_note(truncation, "Child-run metadata was truncated.");
        }
    }

    let original_warning_count = result.warnings.len();
    let (warnings, truncated) = bounded_delegated_warnings(std::mem::take(&mut result.warnings));
    result.warnings = warnings;
    if truncated {
        let truncation = result.truncation.get_or_insert_with(Default::default);
        truncation.warnings = true;
        truncation.original_warning_count = Some(original_warning_count);
        append_truncation_note(truncation, "Warnings were truncated to recent entries.");
    }
}

fn bounded_delegated_warnings(warnings: Vec<String>) -> (Vec<String>, bool) {
    let original_count = warnings.len();
    let skip_count = original_count.saturating_sub(MAX_DELEGATED_RESULT_WARNINGS);
    let mut truncated = skip_count > 0;
    let warnings = warnings
        .into_iter()
        .skip(skip_count)
        .map(|warning| {
            let bounded =
                truncate_text_with_suffix(&warning, MAX_DELEGATED_RESULT_WARNING_CHARS, "...");
            if bounded != warning {
                truncated = true;
            }
            bounded
        })
        .collect();
    (warnings, truncated)
}

fn append_truncation_note(truncation: &mut DelegatedSkillResultTruncation, note: &str) {
    match truncation.note.as_mut() {
        Some(existing) if !existing.contains(note) => {
            existing.push(' ');
            existing.push_str(note);
        }
        Some(_) => {}
        None => truncation.note = Some(note.to_string()),
    }
}

fn completed_child_summary(result: &ChildRuntimeResult) -> String {
    structured_output_summary(result.structured_output.as_ref())
        .or_else(|| {
            non_empty_trimmed(&result.output_text).map(|text| {
                truncate_text_with_suffix(
                    &text,
                    MAX_DELEGATED_RESULT_SUMMARY_CHARS,
                    "... [truncated; inspect output_text or output_ref]",
                )
            })
        })
        .or_else(|| non_empty_trimmed(result.turn_summary.as_deref().unwrap_or_default()))
        .unwrap_or_else(|| "Delegated runtime completed without textual output.".to_string())
}

fn structured_output_summary(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|value| value.get("summary"))
        .and_then(serde_json::Value::as_str)
        .and_then(non_empty_trimmed)
}

fn is_critical_structured_output_key(key: &str) -> bool {
    matches!(
        key,
        "status"
            | "summary"
            | "overall_status"
            | "verification_attempted"
            | "attempted_count"
            | "passed_count"
            | "failed_count"
            | "environment_blocked_count"
            | "blocked_count"
            | "not_run_count"
            | "all_passed"
    )
}

fn truncate_structured_output(value: serde_json::Value, max_size: usize) -> serde_json::Value {
    let rendered = value.to_string();
    if rendered.len() <= max_size {
        return value;
    }

    match value {
        serde_json::Value::Object(map) => {
            let mut truncated = serde_json::Map::new();
            let mut current_size = 0usize;

            for (key, value) in map {
                let is_critical = is_critical_structured_output_key(key.as_str());
                let processed_value = if is_critical {
                    truncate_structured_output(value, (max_size / 4).max(64))
                } else {
                    truncate_structured_output(value, (max_size / 2).max(64))
                };
                let value_size = key.len() + processed_value.to_string().len();
                if current_size + value_size < max_size * 3 / 4 || is_critical {
                    truncated.insert(key, processed_value);
                    current_size += value_size;
                } else {
                    truncated.insert(
                        "_truncated".to_string(),
                        serde_json::Value::String("Additional fields omitted".to_string()),
                    );
                    break;
                }
            }

            serde_json::Value::Object(truncated)
        }
        serde_json::Value::Array(items) => {
            let item_budget = (max_size / items.len().max(1)).max(32);
            let mut truncated = Vec::new();
            let mut current_size = 0usize;

            for item in items {
                let processed = truncate_structured_output(item, item_budget);
                let item_size = processed.to_string().len();
                if current_size + item_size < max_size * 3 / 4 {
                    truncated.push(processed);
                    current_size += item_size;
                } else {
                    truncated.push(json!({
                        "_note": "Additional array items omitted"
                    }));
                    break;
                }
            }

            serde_json::Value::Array(truncated)
        }
        serde_json::Value::String(text) => {
            serde_json::Value::String(truncate_text_with_suffix(&text, max_size, "..."))
        }
        other => other,
    }
}

fn non_empty_trimmed(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn yield_kind_label(kind: &YieldKind) -> String {
    match kind {
        YieldKind::Confirmation => "confirmation".to_string(),
        YieldKind::StructuredInput => "structured_input".to_string(),
        YieldKind::DynamicTool => "dynamic_tool".to_string(),
        YieldKind::Custom(kind) => kind.clone(),
    }
}

pub(super) fn parse_confirmation_request(
    tool_call_id: &str,
    args: &serde_json::Value,
) -> Option<PendingConfirmation> {
    let checkpoint_type = args
        .get("checkpoint_type")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or("confirmation")
        .to_string();
    if checkpoint_type == MOUNT_ESCALATION_CHECKPOINT_TYPE {
        return None;
    }
    let summary = args.get("summary")?.as_str()?.trim().to_string();
    if summary.is_empty() {
        return None;
    }
    let details = args.get("details").cloned().unwrap_or(json!({}));
    let options = args
        .get("options")
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                })
                .collect()
        })
        .filter(|opts: &Vec<String>| !opts.is_empty())
        .unwrap_or_else(|| {
            vec![
                "approve".to_string(),
                "modify".to_string(),
                "reject".to_string(),
            ]
        });

    Some(PendingConfirmation {
        checkpoint_id: tool_call_id.to_string(),
        checkpoint_type,
        summary,
        details,
        options,
    })
}

fn parse_structured_user_input_request(
    tool_call_id: &str,
    arguments: &serde_json::Value,
) -> Option<crate::approval::PendingStructuredInputRequest> {
    let title = arguments.get("title")?.as_str()?.trim().to_string();
    let prompt = arguments.get("prompt")?.as_str()?.trim().to_string();
    if title.is_empty() || prompt.is_empty() {
        return None;
    }
    let request_id = tool_call_id.to_string();

    let questions = arguments
        .get("questions")?
        .as_array()?
        .iter()
        .filter_map(|raw| {
            let id = parse_non_empty_string(raw.get("id"))?;
            let label = parse_non_empty_string(raw.get("label"))?;
            let prompt = parse_non_empty_string(raw.get("prompt"))?;
            let required = raw
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let options = raw
                .get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|opt| {
                            Some(StructuredInputOption {
                                value: parse_non_empty_string(opt.get("value"))?,
                                label: parse_non_empty_string(opt.get("label"))?,
                                description: parse_optional_string(opt.get("description")),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let kind = parse_structured_input_kind(raw.get("kind"), !options.is_empty())?;
            let placeholder = parse_optional_string(raw.get("placeholder"));
            let help_text = parse_optional_string(raw.get("help_text"));
            let default_value = parse_optional_string(raw.get("default"));
            let default_values = parse_string_array(raw.get("defaults"));
            let min_selected = parse_optional_u32(raw.get("min_selected"));
            let max_selected = parse_optional_u32(raw.get("max_selected"));
            let presentation_hints = parse_presentation_hints(raw.get("presentation_hints"));
            let options = normalize_question_options(kind, options);

            if matches!(
                kind,
                StructuredInputKind::Boolean
                    | StructuredInputKind::SingleSelect
                    | StructuredInputKind::MultiSelect
            ) && options.is_empty()
            {
                return None;
            }

            let option_values = options
                .iter()
                .map(|opt| opt.value.as_str())
                .collect::<Vec<_>>();
            let normalized_default_value = match kind {
                StructuredInputKind::Text
                | StructuredInputKind::Number
                | StructuredInputKind::Integer => default_value.clone(),
                StructuredInputKind::Boolean | StructuredInputKind::SingleSelect => {
                    normalize_single_default(default_value.clone(), option_values.as_slice())
                }
                StructuredInputKind::MultiSelect => None,
            };
            let normalized_default_values = if matches!(kind, StructuredInputKind::MultiSelect) {
                normalize_multi_defaults(
                    default_value.as_deref(),
                    default_values,
                    option_values.as_slice(),
                )
            } else {
                Vec::new()
            };
            let (min_selected, max_selected) =
                normalize_selection_constraints(min_selected, max_selected, options.len());

            Some(StructuredInputQuestion {
                id,
                label,
                prompt,
                kind,
                required,
                placeholder,
                help_text,
                default_value: normalized_default_value,
                default_values: normalized_default_values,
                min_selected: if matches!(kind, StructuredInputKind::MultiSelect) {
                    min_selected
                } else {
                    None
                },
                max_selected: if matches!(kind, StructuredInputKind::MultiSelect) {
                    max_selected
                } else {
                    None
                },
                options,
                presentation_hints,
            })
        })
        .collect::<Vec<_>>();

    if questions.is_empty() {
        return None;
    }

    Some(crate::approval::PendingStructuredInputRequest {
        request_id,
        title,
        prompt,
        questions,
    })
}

fn parse_non_empty_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|raw| raw.as_str())
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(ToString::to_string)
}

fn parse_optional_string(value: Option<&serde_json::Value>) -> Option<String> {
    parse_non_empty_string(value)
}

fn parse_string_array(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|raw| raw.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| parse_non_empty_string(Some(item)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_optional_u32(value: Option<&serde_json::Value>) -> Option<u32> {
    value
        .and_then(|raw| raw.as_u64())
        .and_then(|raw| u32::try_from(raw).ok())
}

fn parse_structured_input_kind(
    value: Option<&serde_json::Value>,
    has_options: bool,
) -> Option<StructuredInputKind> {
    match value.and_then(|raw| raw.as_str()) {
        Some("text") => Some(StructuredInputKind::Text),
        Some("boolean") => Some(StructuredInputKind::Boolean),
        Some("number") => Some(StructuredInputKind::Number),
        Some("integer") => Some(StructuredInputKind::Integer),
        Some("single_select") => Some(StructuredInputKind::SingleSelect),
        Some("multi_select") => Some(StructuredInputKind::MultiSelect),
        Some(_) => None,
        None => Some(if has_options {
            StructuredInputKind::SingleSelect
        } else {
            StructuredInputKind::Text
        }),
    }
}

fn parse_presentation_hints(value: Option<&serde_json::Value>) -> Vec<AdaptivePresentationHint> {
    value
        .and_then(|raw| raw.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item.as_str() {
                    Some("radio") => Some(AdaptivePresentationHint::Radio),
                    Some("toggle") => Some(AdaptivePresentationHint::Toggle),
                    Some("searchable") => Some(AdaptivePresentationHint::Searchable),
                    Some("multiline") => Some(AdaptivePresentationHint::Multiline),
                    Some("compact") => Some(AdaptivePresentationHint::Compact),
                    Some("dangerous") => Some(AdaptivePresentationHint::Dangerous),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_question_options(
    kind: StructuredInputKind,
    options: Vec<StructuredInputOption>,
) -> Vec<StructuredInputOption> {
    if matches!(kind, StructuredInputKind::Boolean) && options.is_empty() {
        return boolean_options();
    }
    options
}

fn boolean_options() -> Vec<StructuredInputOption> {
    vec![
        StructuredInputOption {
            value: "true".to_string(),
            label: "Yes".to_string(),
            description: None,
        },
        StructuredInputOption {
            value: "false".to_string(),
            label: "No".to_string(),
            description: None,
        },
    ]
}

fn structured_input_yield_payload(
    capabilities: &alan_agent_protocol::ClientCapabilities,
    title: String,
    prompt: String,
    questions: Vec<StructuredInputQuestion>,
) -> StructuredInputYieldPayload {
    let questions = if capabilities.adaptive_yields.presentation_hints {
        questions
    } else {
        questions
            .into_iter()
            .map(|mut question| {
                question.presentation_hints.clear();
                question
            })
            .collect()
    };

    StructuredInputYieldPayload {
        title,
        prompt: Some(prompt),
        questions,
    }
}

fn normalize_single_default(
    default_value: Option<String>,
    option_values: &[&str],
) -> Option<String> {
    default_value
        .filter(|value| option_values.is_empty() || option_values.contains(&value.as_str()))
}

fn normalize_multi_defaults(
    default_value: Option<&str>,
    default_values: Vec<String>,
    option_values: &[&str],
) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in default_values {
        if option_values.contains(&value.as_str()) && !normalized.contains(&value) {
            normalized.push(value);
        }
    }

    if normalized.is_empty()
        && let Some(value) = default_value
        && option_values.contains(&value)
    {
        normalized.push(value.to_string());
    }

    normalized
}

fn normalize_selection_constraints(
    min_selected: Option<u32>,
    max_selected: Option<u32>,
    option_count: usize,
) -> (Option<u32>, Option<u32>) {
    let option_limit = u32::try_from(option_count).ok();
    let min = min_selected.filter(|value| Some(*value) <= option_limit);
    let max = max_selected.filter(|value| Some(*value) <= option_limit);

    match (min, max) {
        (Some(min), Some(max)) if max < min => (Some(min), None),
        other => other,
    }
}

fn parse_plan_status(raw: &str) -> Option<alan_agent_protocol::PlanItemStatus> {
    match raw {
        "pending" | "blocked" => Some(alan_agent_protocol::PlanItemStatus::Pending),
        "in_progress" => Some(alan_agent_protocol::PlanItemStatus::InProgress),
        "completed" | "skipped" => Some(alan_agent_protocol::PlanItemStatus::Completed),
        _ => None,
    }
}

fn parse_plan_items(value: &serde_json::Value) -> Option<Vec<alan_agent_protocol::PlanItem>> {
    let items = value.as_array()?;
    let parsed = items
        .iter()
        .filter_map(|raw| {
            let id = raw.get("id")?.as_str()?.to_string();
            let content = raw
                .get("content")
                .or_else(|| raw.get("description"))?
                .as_str()?
                .to_string();
            let status_raw = raw.get("status")?.as_str()?;
            let status = parse_plan_status(status_raw)?;
            Some(alan_agent_protocol::PlanItem {
                id,
                content,
                status,
            })
        })
        .collect::<Vec<_>>();
    (!parsed.is_empty()).then_some(parsed)
}

fn parse_plan_update(
    arguments: &serde_json::Value,
) -> Option<(Option<String>, Vec<alan_agent_protocol::PlanItem>)> {
    let explanation = arguments
        .get("explanation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let items = parse_plan_items(arguments.get("items")?)?;
    Some((explanation, items))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DelegatedSkillInvocationRequest {
    skill_id: String,
    target: String,
    task: String,
    workspace_root: Option<PathBuf>,
    cwd: Option<PathBuf>,
    timeout_secs: Option<u64>,
}

impl DelegatedSkillInvocationRequest {
    fn with_effective_launch_inputs(
        &self,
        workspace_root: Option<PathBuf>,
        cwd: Option<PathBuf>,
        timeout_secs: Option<u64>,
    ) -> Self {
        Self {
            skill_id: self.skill_id.clone(),
            target: self.target.clone(),
            task: self.task.clone(),
            workspace_root,
            cwd,
            timeout_secs,
        }
    }
}

fn parse_delegated_skill_invocation_request(
    arguments: &serde_json::Value,
) -> Option<DelegatedSkillInvocationRequest> {
    let skill_id = arguments.get("skill_id")?.as_str()?.trim().to_string();
    let target = arguments.get("target")?.as_str()?.trim().to_string();
    let task = arguments.get("task")?.as_str()?.trim().to_string();
    let workspace_root = parse_optional_path_argument(arguments, "workspace_root")?;
    let cwd = parse_optional_path_argument(arguments, "cwd")?;
    let timeout_secs = parse_optional_timeout_secs_argument(arguments, "timeout_secs")?;
    if skill_id.is_empty() || target.is_empty() || task.is_empty() {
        return None;
    }
    Some(DelegatedSkillInvocationRequest {
        skill_id,
        target,
        task,
        workspace_root,
        cwd,
        timeout_secs,
    })
}

fn parse_terminate_child_run_request(
    arguments: &serde_json::Value,
) -> Option<(String, String, ChildRunTerminationMode)> {
    let child_run_id = arguments.get("child_run_id")?.as_str()?.trim().to_string();
    if child_run_id.is_empty() {
        return None;
    }
    let reason = arguments
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("parent runtime requested child termination")
        .to_string();
    let mode = match arguments
        .get("mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("graceful")
    {
        "graceful" => ChildRunTerminationMode::Graceful,
        "forceful" | "kill" => ChildRunTerminationMode::Forceful,
        _ => return None,
    };
    Some((child_run_id, reason, mode))
}

fn parse_optional_path_argument(
    arguments: &serde_json::Value,
    key: &str,
) -> Option<Option<PathBuf>> {
    match arguments.get(key) {
        None => Some(None),
        Some(value) => {
            let path = value.as_str()?.trim();
            if path.is_empty() {
                return Some(None);
            }
            Some(Some(PathBuf::from(path)))
        }
    }
}

fn parse_optional_timeout_secs_argument(
    arguments: &serde_json::Value,
    key: &str,
) -> Option<Option<u64>> {
    match arguments.get(key) {
        None => Some(None),
        Some(value) => {
            let timeout_secs = value.as_u64()?;
            if timeout_secs == 0 || timeout_secs > MAX_DELEGATED_TIMEOUT_SECS {
                return None;
            }
            Some(Some(timeout_secs))
        }
    }
}

fn truncate_text_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let suffix_len = suffix.chars().count();
    if max_chars <= suffix_len {
        return suffix.chars().take(max_chars).collect();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(suffix_len))
        .collect::<String>();
    truncated.push_str(suffix);
    truncated
}

fn request_confirmation_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_confirmation".to_string(),
        description: "Request user confirmation or approval before proceeding with a significant action. Use this when you need explicit user approval before making changes or proceeding with a recommendation.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "checkpoint_id": {
                    "type": "string",
                    "description": "Optional legacy field. Runtime uses the tool call id as request_id."
                },
                "checkpoint_type": {
                    "type": "string",
                    "description": "The type of checkpoint (e.g., 'business_understanding', 'supplier_recommendation', 'final_confirmation'). Defaults to 'confirmation'."
                },
                "summary": {
                    "type": "string",
                    "description": "A clear summary of what is being proposed or what the user should confirm"
                },
                "details": {
                    "type": "object",
                    "description": "Additional structured details relevant to the confirmation"
                }
            },
            "required": ["summary"]
        }),
    }
}

fn request_mount_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_mount".to_string(),
        description: "Request an approval-gated host directory mount under /mnt. This asks the host to authorize a mount grant; it does not apply the mount directly.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "namespace_path": {
                    "type": "string",
                    "description": "Absolute Alan OS namespace path under /mnt/<name>, without '.', '..', or empty path components."
                },
                "host_path": {
                    "type": "string",
                    "description": "Absolute host directory path to request access to."
                },
                "access": {
                    "type": "string",
                    "enum": ["read_only", "read_write"],
                    "description": "Requested access mode for the host directory mount."
                },
                "reason": {
                    "type": "string",
                    "description": "Non-empty explanation of why this mount is needed."
                }
            },
            "required": ["namespace_path", "host_path", "access", "reason"]
        }),
    }
}

fn request_user_input_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_user_input".to_string(),
        description: "Request structured user input (questions/options) from the client UI and wait for a structured response before continuing.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "request_id": {
                    "type": "string",
                    "description": "Optional legacy field. Runtime uses the tool call id as request_id."
                },
                "title": { "type": "string" },
                "prompt": { "type": "string" },
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "label": { "type": "string" },
                            "prompt": { "type": "string" },
                            "kind": {
                                "type": "string",
                                "enum": ["text", "boolean", "number", "integer", "single_select", "multi_select"]
                            },
                            "required": { "type": "boolean" },
                            "placeholder": { "type": "string" },
                            "help_text": { "type": "string" },
                            "presentation_hints": {
                                "type": "array",
                                "items": {
                                    "type": "string",
                                    "enum": ["radio", "toggle", "searchable", "multiline", "compact", "dangerous"]
                                }
                            },
                            "default": { "type": "string" },
                            "defaults": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "min_selected": { "type": "integer", "minimum": 0 },
                            "max_selected": { "type": "integer", "minimum": 0 },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "value": { "type": "string" },
                                        "label": { "type": "string" },
                                        "description": { "type": "string" }
                                    },
                                    "required": ["value", "label"]
                                }
                            }
                        },
                        "required": ["id", "label", "prompt"]
                    }
                }
            },
            "required": ["title", "prompt", "questions"]
        }),
    }
}

fn update_plan_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "update_plan".to_string(),
        description: "Publish a normalized plan/progress update to the client UI. Use this when the task plan changes or step status changes.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "explanation": { "type": "string" },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "content": { "type": "string" },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
                        },
                        "required": ["id", "content", "status"]
                    }
                }
            },
            "required": ["items"]
        }),
    }
}

fn invoke_delegated_skill_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "invoke_delegated_skill".to_string(),
        description: "Invoke a delegated skill through alan's runtime-owned delegated launch path. Use this for delegated skills listed in the skills catalog or in active-skill runtime context.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "skill_id": {
                    "type": "string",
                    "description": "Resolved delegated skill id exposed in the skills catalog or active-skill runtime context.",
                    "maxLength": MAX_DELEGATED_SKILL_ID_CHARS
                },
                "target": {
                    "type": "string",
                    "description": "Resolved package-local launch target for this delegated skill.",
                    "maxLength": MAX_DELEGATED_TARGET_CHARS
                },
                "task": {
                    "type": "string",
                    "description": "A concise bounded task for the delegated runtime.",
                    "maxLength": MAX_DELEGATED_TASK_CHARS
                },
                "workspace_root": {
                    "type": "string",
                    "description": "Optional explicit workspace root for the delegated runtime. Use this when the delegated task targets a different local workspace than the current runtime.",
                    "maxLength": MAX_DELEGATED_PATH_CHARS
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional nested working directory inside the delegated workspace. When omitted, the delegated runtime starts at `workspace_root` or its default workspace root.",
                    "maxLength": MAX_DELEGATED_PATH_CHARS
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional bounded runtime timeout for the delegated child. When omitted, alan applies a default bounded child timeout.",
                    "minimum": 1,
                    "maximum": MAX_DELEGATED_TIMEOUT_SECS
                }
            },
            "required": ["skill_id", "target", "task"]
        }),
    }
}

fn terminate_child_run_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "terminate_child_run".to_string(),
        description: "Request termination of a delegated child run launched by this parent runtime. Use graceful mode first unless the child is stuck or unsafe to keep running.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "child_run_id": {
                    "type": "string",
                    "description": "Child-run id from a delegated result child_run record."
                },
                "reason": {
                    "type": "string",
                    "description": "Brief reason recorded in the child-run termination audit trail."
                },
                "mode": {
                    "type": "string",
                    "enum": ["graceful", "forceful"],
                    "description": "graceful requests shutdown; forceful aborts when the child is stuck."
                }
            },
            "required": ["child_run_id", "reason", "mode"]
        }),
    }
}

#[cfg(test)]
#[path = "virtual_tools_tests.rs"]
mod tests;
