//! Policy, guardian-review, and human-approval transition for one Tool call.

use std::path::Path;

use alan_agent_protocol::{Event, ToolCapability, ToolDecisionAudit};
use anyhow::Result;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    agent_machine::NormalizedToolCall,
    approval::{
        PendingConfirmation, TOOL_ESCALATION_CHECKPOINT_PREFIX, TOOL_ESCALATION_CHECKPOINT_TYPE,
        append_skill_permission_hints, runtime_confirmation_yield_payload,
    },
};

use super::{
    guardian::{
        DEFAULT_REVIEWER_POLICY, ReviewContext, ReviewOutcome, build_review_request,
        build_transcript, review_generation_result,
    },
    tool_policy::{EscalationRoute, SandboxConfinement, ToolPolicyDecision, evaluate_tool_policy},
    turn_support::tool_result_preview,
};

mod runtime_inputs;

pub(super) use runtime_inputs::ToolAuthorizationRuntime;

pub(super) struct ToolAuthorizationRequest<'a> {
    pub(super) tool_call: &'a NormalizedToolCall,
    pub(super) tool_arguments: &'a Value,
    pub(super) tool_capability: ToolCapability,
    pub(super) current_tool_cwd: Option<&'a Path>,
    pub(super) allow_approved_tool_escalation_execution: bool,
    pub(super) cancel: &'a CancellationToken,
}

#[derive(Debug)]
pub(super) enum ToolAuthorizationOutcome {
    Authorized { audit: ToolDecisionAudit },
    Completed,
    PauseTurn,
}

pub(super) async fn authorize_tool_call<E, F>(
    mut runtime: ToolAuthorizationRuntime<'_>,
    request: ToolAuthorizationRequest<'_>,
    emit: &mut E,
) -> Result<ToolAuthorizationOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let ToolAuthorizationRequest {
        tool_call,
        tool_arguments,
        tool_capability,
        current_tool_cwd,
        allow_approved_tool_escalation_execution,
        cancel,
    } = request;
    let policy_decision = maybe_allow_approved_tool_escalation_replay(
        evaluate_tool_policy(
            runtime.policy_engine,
            runtime.governance,
            &tool_call.name,
            tool_arguments,
            tool_capability,
            current_tool_cwd,
            SandboxConfinement::detect(),
        ),
        allow_approved_tool_escalation_execution,
    );
    record_policy_decision(runtime.machine, tool_call, &policy_decision);

    match policy_decision {
        ToolPolicyDecision::Allow { audit } => Ok(ToolAuthorizationOutcome::Authorized { audit }),
        ToolPolicyDecision::Escalate {
            summary,
            mut details,
            audit,
            route,
        } => {
            details["escalation_route"] = json!(match route {
                EscalationRoute::Reviewer => "reviewer",
                EscalationRoute::AlwaysHuman => "always_human",
            });
            details["replay_tool_call"] = json!({
                "call_id": tool_call.id,
                "tool_name": tool_call.name,
                "arguments": tool_arguments,
            });
            details = append_skill_permission_hints(details, runtime.machine.active_skills());

            if matches!(route, EscalationRoute::Reviewer) {
                match review_escalation(
                    &mut runtime,
                    tool_call,
                    tool_arguments,
                    &details,
                    &audit,
                    cancel,
                    emit,
                )
                .await?
                {
                    EscalationReviewOutcome::Authorized => {
                        return Ok(ToolAuthorizationOutcome::Authorized { audit });
                    }
                    EscalationReviewOutcome::Completed => {
                        return Ok(ToolAuthorizationOutcome::Completed);
                    }
                    EscalationReviewOutcome::Human => {}
                }
            }

            let pending = PendingConfirmation {
                checkpoint_id: format!("{TOOL_ESCALATION_CHECKPOINT_PREFIX}{}", tool_call.id),
                checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary,
                details,
                options: vec!["approve".to_string(), "reject".to_string()],
            };
            let request_id = runtime
                .agent_files
                .write_confirmation_request(&pending)
                .await?;
            runtime.machine.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                json!({"status":"escalation_required","request_id": request_id.clone()}),
                true,
                Some(audit),
            );
            runtime
                .machine
                .set_confirmation_for_request(request_id.clone(), pending.clone());
            super::ui_surfaces::paused(&runtime.agent_files).await?;
            emit(Event::Yield {
                request_id,
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload: serde_json::to_value(runtime_confirmation_yield_payload(&pending))
                    .unwrap_or_else(|_| json!({})),
            })
            .await;
            Ok(ToolAuthorizationOutcome::PauseTurn)
        }
        ToolPolicyDecision::Forbidden { reason, audit } => {
            let blocked_payload = json!({
                "error": reason,
                "status": "blocked_by_policy"
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
            runtime.machine.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                blocked_payload.clone(),
                false,
                Some(audit),
            );
            runtime
                .machine
                .add_tool_message(&tool_call.id, &tool_call.name, blocked_payload);
            Ok(ToolAuthorizationOutcome::Completed)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscalationReviewOutcome {
    Authorized,
    Completed,
    Human,
}

async fn review_escalation<E, F>(
    runtime: &mut ToolAuthorizationRuntime<'_>,
    tool_call: &NormalizedToolCall,
    tool_arguments: &Value,
    details: &Value,
    audit: &ToolDecisionAudit,
    cancel: &CancellationToken,
    emit: &mut E,
) -> Result<EscalationReviewOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let transcript = build_transcript(runtime.machine.messages());
    let review_context = ReviewContext {
        policy: DEFAULT_REVIEWER_POLICY,
        transcript: &transcript,
        approval_request: details,
    };
    let request = build_review_request(&review_context);
    let result = runtime
        .generation
        .generate_response_with_retry(request, runtime.llm_request_timeout_secs, cancel)
        .await;

    match review_generation_result(result) {
        ReviewOutcome::Allow => {
            runtime.machine.record_guardian_review(false);
            Ok(EscalationReviewOutcome::Authorized)
        }
        ReviewOutcome::Deny { rationale } => {
            let tripped = runtime.machine.record_guardian_review(true);
            let message = format!("auto-review denied: {rationale}");
            super::ui_surfaces::warning(&runtime.agent_files, message.clone()).await?;
            emit(Event::Warning { message }).await;
            if tripped {
                let message = "auto-review circuit breaker tripped; pausing for you".to_string();
                super::ui_surfaces::warning(&runtime.agent_files, message.clone()).await?;
                emit(Event::Warning { message }).await;
                Ok(EscalationReviewOutcome::Human)
            } else {
                let denied_payload = json!({
                    "status": "denied_by_reviewer",
                    "reason": rationale,
                    "instruction": "Do not work around this denial. Pursue a materially safer \
                        alternative, or stop and ask the user."
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&denied_payload),
                    audit: Some(audit.clone()),
                })
                .await;
                runtime.machine.record_tool_call_with_audit(
                    &tool_call.name,
                    tool_arguments.clone(),
                    denied_payload.clone(),
                    false,
                    Some(audit.clone()),
                );
                runtime
                    .machine
                    .add_tool_message(&tool_call.id, &tool_call.name, denied_payload);
                Ok(EscalationReviewOutcome::Completed)
            }
        }
        ReviewOutcome::Unavailable { reason } => {
            tracing::warn!(%reason, "auto-review unavailable; pausing for human");
            Ok(EscalationReviewOutcome::Human)
        }
    }
}

fn record_policy_decision(
    machine: &mut crate::agent_machine::AgentMachine,
    tool_call: &NormalizedToolCall,
    decision: &ToolPolicyDecision,
) {
    let audit = match decision {
        ToolPolicyDecision::Allow { audit }
        | ToolPolicyDecision::Escalate { audit, .. }
        | ToolPolicyDecision::Forbidden { audit, .. } => audit,
    };
    machine.record_event(
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
}

fn maybe_allow_approved_tool_escalation_replay(
    policy_decision: ToolPolicyDecision,
    allow_approved_tool_escalation_execution: bool,
) -> ToolPolicyDecision {
    match policy_decision {
        ToolPolicyDecision::Escalate { audit, .. } if allow_approved_tool_escalation_execution => {
            ToolPolicyDecision::Allow {
                audit: ToolDecisionAudit {
                    action: "allow".to_string(),
                    reason: Some("approved tool escalation replay".to_string()),
                    ..audit
                },
            }
        }
        other => other,
    }
}
