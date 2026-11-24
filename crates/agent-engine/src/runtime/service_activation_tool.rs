use std::sync::Arc;

use alan_agent_protocol::{AdaptivePresentationHint, ConfirmationYieldPayload, Event, YieldKind};
use anyhow::Result;
use serde::Deserialize;
use serde_json::json;

use crate::{
    agent_machine::{AgentMachine, NormalizedToolCall},
    approval::{
        PendingConfirmation, SERVICE_ACTIVATION_CHECKPOINT_TYPE, append_skill_permission_hints,
    },
    llm::ToolDefinition,
};

use super::{
    service_activation::{
        ServiceActivationAuthority, ServiceActivationRequest, ServiceMountAccess,
    },
    transition::NamespaceAgentFiles,
    turn_support::tool_result_preview,
    virtual_tool::VirtualToolOutcome,
};

pub(super) struct ServiceActivationRuntime<'a> {
    pub(super) machine: &'a mut AgentMachine,
    pub(super) agent_files: NamespaceAgentFiles,
    pub(super) authority: Arc<dyn ServiceActivationAuthority>,
}

impl<'a> ServiceActivationRuntime<'a> {
    pub(super) fn new(
        machine: &'a mut AgentMachine,
        agent_files: NamespaceAgentFiles,
        authority: Arc<dyn ServiceActivationAuthority>,
    ) -> Self {
        Self {
            machine,
            agent_files,
            authority,
        }
    }
}

pub(super) fn request_service_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_service".to_string(),
        description: "Request user approval to activate and hot-mount an installed fileserver package under /mnt/<service> (Root Agent only).".to_string(),
        parameters: json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "package_id": {
                    "type": "string",
                    "description": "Installed fileserver package identifier (e.g. 'github-fileserver')."
                },
                "access": {
                    "type": "string",
                    "enum": ["read_only", "read_write"],
                    "default": "read_write",
                    "description": "Requested mount access mode for /mnt/<service> (defaults to 'read_write')."
                },
                "allow_network": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether outbound network access is requested for the confined fileserver (defaults to false)."
                },
                "reason": {
                    "type": "string",
                    "description": "Non-empty explanation of why activating this service is needed."
                }
            },
            "required": ["package_id", "reason"]
        }),
    }
}

pub(super) fn parse_service_activation_request(
    args: &serde_json::Value,
) -> std::result::Result<ServiceActivationRequest, String> {
    const ALLOWED_FIELDS: &[&str] = &["package_id", "access", "allow_network", "reason"];

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RawRequest {
        package_id: String,
        #[serde(default)]
        access: Option<String>,
        #[serde(default)]
        allow_network: Option<bool>,
        reason: String,
    }

    let object = args
        .as_object()
        .ok_or_else(|| "request_service arguments must be an object".to_string())?;
    if object
        .keys()
        .any(|field| !ALLOWED_FIELDS.contains(&field.as_str()))
    {
        return Err("request_service arguments contain unsupported fields".to_string());
    }
    let raw: RawRequest = serde_json::from_value(args.clone()).map_err(|err| err.to_string())?;
    let package_id = raw.package_id.trim();
    if package_id.is_empty() {
        return Err("package_id must be a non-empty string".to_string());
    }
    let reason = raw.reason.trim();
    if reason.is_empty() {
        return Err("reason must be a non-empty string".to_string());
    }
    let access = match raw.access.as_deref().map(str::trim) {
        None => ServiceMountAccess::ReadWrite,
        Some("read_only") => ServiceMountAccess::ReadOnly,
        Some("read_write") => ServiceMountAccess::ReadWrite,
        Some(_) => return Err("access must be one of: read_only, read_write".to_string()),
    };

    Ok(ServiceActivationRequest {
        package_id: package_id.to_string(),
        access,
        allow_network: raw.allow_network.unwrap_or(false),
        reason: reason.to_string(),
    })
}

async fn finish_immediate_tool_result<E, F>(
    machine: &mut AgentMachine,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    payload: serde_json::Value,
    success: bool,
    emit: &mut E,
) -> VirtualToolOutcome
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    emit(Event::ToolCallCompleted {
        presentation: None,
        id: tool_call.id.clone(),
        name: Some(tool_call.name.clone()),
        success: Some(success),
        result_preview: tool_result_preview(&payload),
        audit: None,
    })
    .await;
    machine.record_tool_call(
        &tool_call.name,
        tool_arguments.clone(),
        payload.clone(),
        success,
    );
    machine.add_tool_message(&tool_call.id, &tool_call.name, payload);
    VirtualToolOutcome::Continue {
        refresh_context: false,
    }
}

pub(super) async fn handle_request_service<E, F>(
    runtime: ServiceActivationRuntime<'_>,
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

    let req = match parse_service_activation_request(tool_arguments) {
        Ok(req) => req,
        Err(error) => {
            let payload = json!({
                "status": "invalid_request",
                "error": error,
            });
            return Ok(finish_immediate_tool_result(
                runtime.machine,
                tool_call,
                tool_arguments,
                payload,
                false,
                emit,
            )
            .await);
        }
    };

    let available = match runtime.authority.inspect_request(&req) {
        Ok(available) => available,
        Err(error) => {
            let payload = json!({
                "status": "invalid_request",
                "package_id": req.package_id,
                "error": error,
            });
            return Ok(finish_immediate_tool_result(
                runtime.machine,
                tool_call,
                tool_arguments,
                payload,
                false,
                emit,
            )
            .await);
        }
    };

    if available.active
        && let Some((active_access, active_network)) = runtime
            .authority
            .active_service_grant(&available.service_id)
        && active_access.satisfies(req.access)
        && (!req.allow_network || active_network)
    {
        let payload = json!({
            "status": "already_active",
            "package_id": available.package_id,
            "service_id": available.service_id,
            "mount_path": available.mount_path,
            "access": active_access.as_str(),
            "allow_network": active_network,
        });
        return Ok(finish_immediate_tool_result(
            runtime.machine,
            tool_call,
            tool_arguments,
            payload,
            true,
            emit,
        )
        .await);
    }

    if let Some(existing) = runtime.machine.pending_confirmation() {
        tracing::warn!(
            existing_checkpoint = %existing.checkpoint_id,
            requested_checkpoint = %tool_call.id,
            "single pending confirmation rule prevented stacking duplicate service activation"
        );
        return Ok(VirtualToolOutcome::PauseTurn);
    }

    let network_desc = if req.allow_network {
        if available.network_domains.is_empty() {
            "enabled".to_string()
        } else {
            format!("enabled ({})", available.network_domains.join(", "))
        }
    } else {
        "disabled".to_string()
    };
    let github_note = if available.package_id == "github-fileserver" {
        " Host `gh` auth stays confined inside `github-fileserver` while general `bash` stays isolated."
    } else {
        ""
    };
    let prompt = format!(
        "Activating `{}` at `{}` (access: {}, network: {}). Reason: {}. Boundary: only the `{}` fileserver process receives this grant; general `bash` and other tools remain isolated.{}",
        available.package_id,
        available.mount_path,
        req.access.as_str(),
        network_desc,
        req.reason,
        available.service_id,
        github_note,
    );

    let details = append_skill_permission_hints(
        json!({
            "request": req,
            "service_id": available.service_id,
            "mount_path": available.mount_path,
            "network_domains": available.network_domains,
            "prompt": prompt,
        }),
        runtime.machine.active_skills(),
    );
    let pending = PendingConfirmation {
        checkpoint_id: tool_call.id.clone(),
        checkpoint_type: SERVICE_ACTIVATION_CHECKPOINT_TYPE.to_string(),
        summary: format!(
            "Activate '{}' at {}?",
            available.package_id, available.mount_path
        ),
        details,
        options: vec![
            "approve".to_string(),
            "allow_persistent".to_string(),
            "reject".to_string(),
        ],
    };

    let request_id = runtime
        .agent_files
        .write_confirmation_request(&pending)
        .await?;
    let pending_payload = json!({
        "status": "pending_confirmation",
        "request_id": request_id.clone(),
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
    runtime.machine.record_tool_call(
        &tool_call.name,
        tool_arguments.clone(),
        pending_payload,
        true,
    );
    runtime
        .machine
        .set_confirmation_for_request(request_id.clone(), pending.clone());
    super::ui_surfaces::paused(&runtime.agent_files, Some(runtime.machine)).await?;
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

    Ok(VirtualToolOutcome::PauseTurn)
}

/// Record deterministic skipped tool responses for trailing calls when a multi-tool batch
/// pauses on `request_service`, ensuring no dangling unmatched tool calls remain in tape.
pub(super) async fn record_skipped_trailing_tool_calls<E, F>(
    machine: &mut AgentMachine,
    remaining: &[NormalizedToolCall],
    emit: &mut E,
) where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    for skipped in remaining {
        let skipped_payload = json!({
            "status": "skipped_due_to_service_activation",
            "error": "Skipped because preceding `request_service` paused for user approval; retry after service activation settles.",
        });
        emit(Event::ToolCallStarted {
            title: None,
            id: skipped.id.clone(),
            name: skipped.name.clone(),
            audit: None,
        })
        .await;
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: skipped.id.clone(),
            name: Some(skipped.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&skipped_payload),
            audit: None,
        })
        .await;
        machine.record_tool_call(
            &skipped.name,
            skipped.arguments.clone(),
            skipped_payload.clone(),
            false,
        );
        machine.add_tool_message(&skipped.id, &skipped.name, skipped_payload);
    }
}

#[cfg(test)]
#[path = "service_activation_tool_tests.rs"]
mod tests;
