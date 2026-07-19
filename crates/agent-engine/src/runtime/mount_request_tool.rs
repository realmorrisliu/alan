use alan_agent_protocol::{AdaptivePresentationHint, ConfirmationYieldPayload, Event, YieldKind};
use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use std::path::{Component, Path, PathBuf};

use crate::approval::{
    MOUNT_ESCALATION_CHECKPOINT_PREFIX, MOUNT_ESCALATION_CHECKPOINT_TYPE, PendingConfirmation,
    append_skill_permission_hints,
};
use crate::llm::ToolDefinition;

use super::tool_policy::{ToolPolicyDecision, evaluate_tool_policy};
use super::transition::RuntimeLoopState;
use super::turn_support::tool_result_preview;
use super::virtual_tool::VirtualToolOutcome;
use crate::agent_machine::NormalizedToolCall;

const RESERVED_MOUNT_NAMESPACE_ROOTS: &[&str] = &["llm", "mem", "route"];

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

pub(super) async fn handle_request_mount<E, F>(
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
            state.machine.record_tool_call(
                &tool_call.name,
                tool_arguments.clone(),
                payload.clone(),
                false,
            );
            state
                .machine
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
        state.tool_execution().default_cwd().as_deref(),
        sandbox_confinement,
    );
    if mount_request.access == MountRequestAccess::ReadWrite {
        let read_decision = evaluate_tool_policy(
            &state.runtime_config.policy_engine,
            &state.runtime_config.governance,
            &tool_call.name,
            &mount_payload,
            alan_agent_protocol::ToolCapability::Read,
            state.tool_execution().default_cwd().as_deref(),
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
        state.machine.record_event(
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
        state.machine.record_tool_call_with_audit(
            &tool_call.name,
            tool_arguments.clone(),
            payload.clone(),
            false,
            Some(audit),
        );
        state
            .machine
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
    state.machine.record_event(
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
    details = append_skill_permission_hints(details, state.machine.active_skills());

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
        .agent_files()
        .write_confirmation_request(&pending)
        .await?;
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
    state.machine.record_tool_call_with_audit(
        &tool_call.name,
        tool_arguments.clone(),
        payload.clone(),
        true,
        Some(escalation_audit),
    );
    state
        .machine
        .set_confirmation_for_request(request_id.clone(), pending.clone());
    super::ui_surfaces::paused(&state.agent_files()).await?;
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
    let normalized =
        dunce::canonicalize(path).unwrap_or_else(|_| dunce::simplified(path).to_path_buf());
    if let Some(component) = crate::tools::protected_path_component(&normalized) {
        return Err(format!(
            "host_path must not directly target protected `{component}` state"
        ));
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

pub(super) fn request_mount_tool_definition() -> ToolDefinition {
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

#[cfg(all(test, unix))]
mod tests {
    use super::parse_mount_host_path;

    #[test]
    fn rejects_protected_host_state_root() {
        let error = parse_mount_host_path("/tmp/project/.git")
            .expect_err("a protected Host state root must not become a direct mount");

        assert!(error.contains("protected `.git` state"));
    }
}
