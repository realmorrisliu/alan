mod runtime_inputs;

use alan_agent_protocol::{CustomYieldPayload, Event, YieldKind};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::agent_machine::{HOST_MOUNT_REQUEST_WAITING_EVENT_TYPE, PendingHostMountRequest};
use crate::llm::ToolDefinition;

use super::tool_policy::{ToolPolicyDecision, evaluate_tool_policy};
use super::turn_support::tool_result_preview;
use super::virtual_tool::VirtualToolOutcome;
use crate::agent_machine::NormalizedToolCall;

pub(crate) use runtime_inputs::MountRequestRuntime;

const RESERVED_MOUNT_NAMESPACE_ROOTS: &[&str] = &[
    "connections",
    "host-mount",
    "llm",
    "mem",
    "package",
    "route",
    "service-manager",
];

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
    pub access: MountRequestAccess,
    pub reason: String,
    pub label: Option<String>,
}

impl MountRequest {
    pub(super) fn payload(&self) -> serde_json::Value {
        json!({
            "namespace_path": &self.namespace_path,
            "access": self.access.as_str(),
            "reason": &self.reason,
            "label": &self.label,
        })
    }
}

/// Return the only `request_mount` arguments allowed to enter Machine or rollout evidence.
/// Invalid documents may contain Host-owned locations or other unknown data, so their values are
/// intentionally discarded instead of being persisted as part of the rejection record.
pub(super) fn durable_mount_request_arguments(args: &serde_json::Value) -> serde_json::Value {
    parse_mount_request(args)
        .map(|request| request.payload())
        .unwrap_or_else(|_| json!({ "invalid_request": true }))
}

pub(super) async fn handle_request_mount<E, F>(
    runtime: MountRequestRuntime<'_>,
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
            runtime.machine.record_tool_call(
                &tool_call.name,
                durable_mount_request_arguments(tool_arguments),
                payload.clone(),
                false,
            );
            runtime
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
        runtime.policy_engine,
        runtime.governance,
        &tool_call.name,
        &mount_payload,
        mount_request.access.policy_capability(),
        runtime.tool_execution.default_cwd().as_deref(),
        sandbox_confinement,
    );
    if mount_request.access == MountRequestAccess::ReadWrite {
        let read_decision = evaluate_tool_policy(
            runtime.policy_engine,
            runtime.governance,
            &tool_call.name,
            &mount_payload,
            alan_agent_protocol::ToolCapability::Read,
            runtime.tool_execution.default_cwd().as_deref(),
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
        runtime.machine.record_event(
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
        runtime.machine.record_tool_call_with_audit(
            &tool_call.name,
            mount_payload.clone(),
            payload.clone(),
            false,
            Some(audit),
        );
        runtime
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
    runtime.machine.record_event(
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

    let request_document = serde_json::to_vec(&mount_payload)?;
    let request_id = match runtime.host_mount_requests.create(&request_document).await {
        Ok(request_id) => request_id,
        Err(error) => {
            let payload = json!({
                "status": "request_service_unavailable",
                "error": "Host Mount Service did not accept the logical request",
                "mount_request": mount_payload,
            });
            emit(Event::Error {
                message: format!("Host Mount Service request failed: {error}"),
                recoverable: true,
            })
            .await;
            emit(Event::ToolCallCompleted {
                presentation: None,
                id: tool_call.id.clone(),
                name: Some(tool_call.name.clone()),
                success: Some(false),
                result_preview: tool_result_preview(&payload),
                audit: Some(escalation_audit.clone()),
            })
            .await;
            runtime.machine.record_tool_call_with_audit(
                &tool_call.name,
                mount_payload.clone(),
                payload.clone(),
                false,
                Some(escalation_audit),
            );
            runtime
                .machine
                .add_tool_message(&tool_call.id, &tool_call.name, payload);
            return Ok(VirtualToolOutcome::Continue {
                refresh_context: false,
            });
        }
    };
    let pending = PendingHostMountRequest {
        request_id: request_id.clone(),
        tool_call_id: tool_call.id.clone(),
        namespace_path: mount_request.namespace_path.clone(),
        access: mount_request.access.as_str().to_string(),
        reason: mount_request.reason.clone(),
        label: mount_request.label.clone(),
        request_events_offset: 0,
    };
    let payload = json!({
        "status": "pending_authorization",
        "request_reference": request_id.clone(),
        "mount_request": mount_payload,
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
    runtime.machine.record_tool_call_with_audit(
        &tool_call.name,
        mount_payload,
        payload.clone(),
        true,
        Some(escalation_audit),
    );
    runtime.machine.record_event(
        HOST_MOUNT_REQUEST_WAITING_EVENT_TYPE,
        serde_json::to_value(&pending).unwrap_or_else(|_| json!({})),
    );
    runtime.machine.set_host_mount_request(pending.clone());
    super::ui_surfaces::paused(&runtime.agent_files).await?;
    emit(Event::Yield {
        request_id,
        kind: YieldKind::Custom("authorization_wait".to_string()),
        payload: serde_json::to_value(CustomYieldPayload {
            title: Some("Waiting for Host Mount authorization".to_string()),
            prompt: None,
            details: Some(json!({
                "request_reference": pending.request_id,
                "namespace_path": pending.namespace_path,
                "access": pending.access,
                "reason": pending.reason,
                "label": pending.label,
                "status": "pending",
            })),
            form: None,
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
    const ALLOWED_FIELDS: &[&str] = &["namespace_path", "access", "reason", "label"];

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Document {
        namespace_path: String,
        access: String,
        reason: String,
        #[serde(default)]
        label: Option<String>,
    }

    let object = args
        .as_object()
        .ok_or_else(|| "request_mount arguments must be an object".to_string())?;
    if object
        .keys()
        .any(|field| !ALLOWED_FIELDS.contains(&field.as_str()))
    {
        return Err("request_mount arguments contain unsupported fields".to_string());
    }
    let document: Document =
        serde_json::from_value(args.clone()).map_err(|error| error.to_string())?;
    let namespace_path = parse_mount_namespace_path(&document.namespace_path)?;
    let access = parse_mount_access(&document.access)?;
    let reason = document.reason.trim();

    if reason.trim().is_empty() {
        return Err("reason must be a non-empty string".to_string());
    }
    let label = document.label.map(|label| label.trim().to_string());
    if label.as_deref() == Some("") {
        return Err("label must be a non-empty string when provided".to_string());
    }

    Ok(MountRequest {
        namespace_path,
        access,
        reason: reason.to_string(),
        label,
    })
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
            "additionalProperties": false,
            "properties": {
                "namespace_path": {
                    "type": "string",
                    "description": "Absolute Alan OS namespace path under /mnt/<name>, without '.', '..', or empty path components."
                },
                "access": {
                    "type": "string",
                    "enum": ["read_only", "read_write"],
                    "description": "Requested access mode for the host directory mount."
                },
                "reason": {
                    "type": "string",
                    "description": "Non-empty explanation of why this mount is needed."
                },
                "label": {
                    "type": "string",
                    "description": "Optional human-readable label for the requested mount."
                }
            },
            "required": ["namespace_path", "access", "reason"]
        }),
    }
}
