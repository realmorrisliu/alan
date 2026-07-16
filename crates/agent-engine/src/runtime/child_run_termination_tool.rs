use alan_agent_protocol::{AdaptivePresentationHint, ConfirmationYieldPayload, Event, YieldKind};
use anyhow::Result;
use serde_json::json;

use crate::approval::{
    PendingConfirmation, TOOL_ESCALATION_CHECKPOINT_PREFIX, TOOL_ESCALATION_CHECKPOINT_TYPE,
    append_skill_permission_hints,
};
use crate::llm::ToolDefinition;

use super::agent_loop::{NormalizedToolCall, RuntimeLoopState};
use super::child_runs::{ChildRunRegistryError, ChildRunTerminationMode};
use super::tool_policy::{ToolPolicyDecision, evaluate_tool_policy};
use super::turn_support::tool_result_preview;
use super::virtual_tool::VirtualToolOutcome;

pub(super) async fn handle_terminate_child_run<E, F>(
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

    let result = state.child_run_registry().request_termination(
        &state.process_path(),
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
                "error": "Child run not found for this parent machine.",
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
    state.machine.record_tool_call_with_audit(
        &tool_call.name,
        tool_arguments.clone(),
        payload.clone(),
        success,
        Some(audit),
    );
    state
        .machine
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
    state.machine.record_event(
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
            state.machine.record_tool_call_with_audit(
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
            super::ui_surfaces::paused(state.namespace_environment()).await?;
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
            state.machine.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                blocked_payload.clone(),
                false,
                Some(audit),
            );
            state
                .machine
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

pub(super) fn terminate_child_run_tool_definition() -> ToolDefinition {
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
