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
    runtime::HookDecision,
};

use super::{
    guardian::{
        ReviewContext, ReviewOutcome, build_review_request, build_transcript,
        resolve_guardian_policy, review_generation_result,
    },
    tool_policy::{
        EscalationRoute, SandboxConfinement, ToolPolicyDecision, evaluate_tool_policy_with_posture,
    },
    turn_support::tool_result_preview,
};

mod runtime_inputs;

pub(super) use runtime_inputs::ToolAuthorizationRuntime;

pub(super) struct ToolAuthorizationRequest<'a> {
    pub(super) explicit_command: bool,
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
        explicit_command,
        tool_call,
        tool_arguments,
        tool_capability,
        current_tool_cwd,
        allow_approved_tool_escalation_execution,
        cancel,
    } = request;
    let confinement = SandboxConfinement::detect();
    let git_push_delegations = runtime
        .authority
        .as_ref()
        .map(|authority| authority.git_push_delegations())
        .unwrap_or_default();
    let mut policy_decision = maybe_allow_approved_tool_escalation_replay(
        evaluate_tool_policy_with_posture(
            runtime.policy_engine,
            runtime.governance,
            runtime.trust_posture,
            &git_push_delegations,
            &tool_call.name,
            tool_arguments,
            tool_capability,
            current_tool_cwd,
            confinement,
        ),
        allow_approved_tool_escalation_execution,
    );
    if explicit_command
        && let ToolPolicyDecision::Escalate { route, audit, .. } = &mut policy_decision
    {
        if confinement.os_enforced
            && audit.rule_id.as_deref() == Some("review-unknown")
            && tool_capability != ToolCapability::Network
        {
            let mut allowed_audit = audit.clone();
            allowed_audit.action = "allow".to_string();
            allowed_audit.policy_source = "explicit_command".to_string();
            policy_decision = ToolPolicyDecision::Allow {
                audit: allowed_audit,
            };
        } else {
            *route = EscalationRoute::AlwaysHuman;
        }
    }
    if let ToolPolicyDecision::Escalate { audit, .. } = &policy_decision {
        let cap_label = super::tool_policy::capability_label(tool_capability);
        if runtime
            .machine
            .is_tool_allowed_for_session(&tool_call.name, cap_label, tool_arguments)
        {
            let mut allowed_audit = audit.clone();
            allowed_audit.action = "allow".to_string();
            allowed_audit.policy_source = "session_allowlist".to_string();
            policy_decision = ToolPolicyDecision::Allow {
                audit: allowed_audit,
            };
        }
    }
    if let Some(hook) = runtime.tool_policy_hook.as_ref() {
        let decision = hook.evaluate(&tool_call.name, tool_arguments, tool_capability);
        let base_audit = match &policy_decision {
            ToolPolicyDecision::Allow { audit }
            | ToolPolicyDecision::Escalate { audit, .. }
            | ToolPolicyDecision::Forbidden { audit, .. } => audit.clone(),
        };
        match decision {
            HookDecision::Continue => {}
            HookDecision::Deny(reason) => {
                let mut audit = base_audit;
                audit.action = "deny".to_string();
                audit.policy_source = "tool_policy_hook".to_string();
                audit.rule_id = Some("tool-policy-hook-deny".to_string());
                audit.reason = Some(reason.clone());
                policy_decision = ToolPolicyDecision::Forbidden { reason, audit };
            }
            HookDecision::Allow
                if !matches!(policy_decision, ToolPolicyDecision::Forbidden { .. }) =>
            {
                let mut audit = base_audit;
                audit.action = "allow".to_string();
                audit.policy_source = "tool_policy_hook".to_string();
                audit.rule_id = Some("tool-policy-hook-allow".to_string());
                audit.reason = None;
                policy_decision = ToolPolicyDecision::Allow { audit };
            }
            HookDecision::Allow => {}
            HookDecision::Escalate(reason)
                if !allow_approved_tool_escalation_execution
                    && !matches!(policy_decision, ToolPolicyDecision::Forbidden { .. }) =>
            {
                let mut audit = base_audit;
                audit.action = "escalate".to_string();
                audit.policy_source = "tool_policy_hook".to_string();
                audit.rule_id = Some("tool-policy-hook-escalate".to_string());
                audit.reason = Some(reason.clone());
                policy_decision = ToolPolicyDecision::Escalate {
                    summary: reason.clone(),
                    details: json!({
                        "reason": reason,
                        "tool_name": tool_call.name,
                    }),
                    audit,
                    route: EscalationRoute::AlwaysHuman,
                };
            }
            HookDecision::Escalate(_) => {}
        }
    }
    record_policy_decision(runtime.machine, tool_call, &policy_decision);

    match policy_decision {
        ToolPolicyDecision::Allow { audit } => Ok(ToolAuthorizationOutcome::Authorized { audit }),
        ToolPolicyDecision::Escalate {
            summary,
            mut details,
            audit,
            route,
        } => {
            let checkpoint_id = format!("{TOOL_ESCALATION_CHECKPOINT_PREFIX}{}", tool_call.id);
            let pre_review_authority = AuthoritySnapshot::capture(&runtime);
            if !details
                .as_object()
                .is_some_and(|object| object.contains_key("capability"))
            {
                details["capability"] = json!(audit.capability);
            }
            details["escalation_route"] = json!(match route {
                EscalationRoute::Reviewer => "reviewer",
                EscalationRoute::AlwaysHuman => "always_human",
            });
            details["replay_tool_call"] = json!({
                "call_id": tool_call.id,
                "tool_name": tool_call.name,
                "arguments": tool_arguments,
            });
            details["escalation_correlation"] = json!({
                "checkpoint_id": checkpoint_id,
                "submission_id": runtime.machine.current_submission_id(),
                "tool_call_id": tool_call.id,
                "host_mount_revision": pre_review_authority.host_mount_revision,
            });
            details = append_skill_permission_hints(details, runtime.machine.active_skills());

            if matches!(route, EscalationRoute::Reviewer) {
                match Box::pin(review_escalation(
                    &mut runtime,
                    tool_call,
                    tool_arguments,
                    &details,
                    &audit,
                    &pre_review_authority,
                    cancel,
                    emit,
                ))
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

            if let Some(existing) = runtime.machine.pending_confirmation() {
                tracing::warn!(
                    existing_checkpoint = %existing.checkpoint_id,
                    requested_checkpoint = %checkpoint_id,
                    "single pending escalation rule prevented stacking duplicate confirmation"
                );
                return Ok(ToolAuthorizationOutcome::PauseTurn);
            }

            let pending = PendingConfirmation {
                checkpoint_id,
                checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary,
                details,
                options: vec![
                    "approve".to_string(),
                    "allow_session".to_string(),
                    "reject".to_string(),
                ],
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
            super::ui_surfaces::paused(&runtime.agent_files, Some(runtime.machine)).await?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthoritySnapshot {
    trust_posture: crate::policy::TrustPosture,
    guardian_profile: Option<String>,
    governance_json: String,
    host_mount_revision: Option<u64>,
}

impl AuthoritySnapshot {
    fn capture(runtime: &ToolAuthorizationRuntime<'_>) -> Self {
        Self {
            trust_posture: runtime.trust_posture,
            guardian_profile: runtime.guardian_profile.map(str::to_string),
            governance_json: serde_json::to_string(runtime.governance).unwrap_or_default(),
            host_mount_revision: runtime
                .authority
                .as_ref()
                .and_then(|authority| authority.authority_revision()),
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Guardian review correlates runtime inputs, tool call metadata, and authority snapshot"
)]
async fn review_escalation<E, F>(
    runtime: &mut ToolAuthorizationRuntime<'_>,
    tool_call: &NormalizedToolCall,
    tool_arguments: &Value,
    details: &Value,
    audit: &ToolDecisionAudit,
    pre_review_authority: &AuthoritySnapshot,
    cancel: &CancellationToken,
    emit: &mut E,
) -> Result<EscalationReviewOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let policy = resolve_guardian_policy(runtime.guardian_config.policy_path.as_deref());
    let transcript = build_transcript(runtime.machine.messages());
    let review_context = ReviewContext {
        policy: &policy,
        transcript: &transcript,
        approval_request: details,
    };
    let request = build_review_request(&review_context);

    let aggregate_secs = runtime
        .guardian_config
        .timeout_secs
        .min(runtime.llm_request_timeout_secs)
        .max(1);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(aggregate_secs);

    let outcome = if let Some(profile) = runtime.guardian_profile {
        let dedicated_outcome = match runtime.authority.as_ref() {
            Some(authority) => {
                match Box::pin(run_captured_guardian_review(
                    authority,
                    profile,
                    request.clone(),
                    deadline,
                    cancel,
                ))
                .await
                {
                    Ok(outcome) => outcome,
                    Err(fatal) => return Err(fatal),
                }
            }
            None => ReviewOutcome::Unavailable {
                reason: "guardian Connection environment unavailable".to_string(),
            },
        };
        match dedicated_outcome {
            ReviewOutcome::Unavailable { reason } => {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    tracing::warn!(%reason, "dedicated guardian exhausted aggregate deadline; pausing for human");
                    ReviewOutcome::Unavailable { reason }
                } else if runtime
                    .machine
                    .try_consume_guardian_fallback(runtime.guardian_config.max_fallbacks_per_turn)
                {
                    let message = format!(
                        "dedicated guardian `{profile}` unavailable ({reason}); falling back to primary connection"
                    );
                    super::ui_surfaces::warning(&runtime.agent_files, message.clone()).await?;
                    emit(Event::Warning { message }).await;
                    Box::pin(run_generation_review_with_deadline(
                        &runtime.generation,
                        request,
                        deadline,
                        cancel,
                    ))
                    .await?
                } else {
                    let message = format!(
                        "dedicated guardian `{profile}` fallback budget exhausted; pausing for human"
                    );
                    super::ui_surfaces::warning(&runtime.agent_files, message.clone()).await?;
                    emit(Event::Warning { message }).await;
                    ReviewOutcome::Unavailable { reason }
                }
            }
            other => other,
        }
    } else {
        Box::pin(run_generation_review_with_deadline(
            &runtime.generation,
            request,
            deadline,
            cancel,
        ))
        .await?
    };

    if matches!(outcome, ReviewOutcome::Allow) {
        let post_review_authority = AuthoritySnapshot::capture(runtime);
        if &post_review_authority != pre_review_authority {
            let message = "auto-review approval invalidated because authority or grants changed during review; pausing for you".to_string();
            super::ui_surfaces::warning(&runtime.agent_files, message.clone()).await?;
            emit(Event::Warning { message }).await;
            return Ok(EscalationReviewOutcome::Human);
        }
    }

    match outcome {
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

async fn run_captured_guardian_review(
    authority: &super::transition::NamespaceToolAuthorization,
    profile: &str,
    request: alan_llm::GenerationRequest,
    deadline: tokio::time::Instant,
    cancel: &CancellationToken,
) -> Result<ReviewOutcome> {
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
        return Ok(ReviewOutcome::Unavailable {
            reason: "guardian aggregate review deadline expired".to_string(),
        });
    }
    let child_cancel = cancel.child_token();
    let captured =
        match tokio::time::timeout(remaining, authority.capture_guardian_generation(profile)).await
        {
            Ok(Ok(generation)) => generation,
            Ok(Err(error)) => {
                return Ok(ReviewOutcome::Unavailable {
                    reason: format!("guardian profile capture failed: {error:#}"),
                });
            }
            Err(_) => {
                child_cancel.cancel();
                return Ok(ReviewOutcome::Unavailable {
                    reason: "guardian profile capture timed out".to_string(),
                });
            }
        };
    Box::pin(run_generation_review_with_deadline(
        &captured, request, deadline, cancel,
    ))
    .await
}

async fn run_generation_review_with_deadline(
    generation: &super::transition::NamespaceGeneration,
    request: alan_llm::GenerationRequest,
    deadline: tokio::time::Instant,
    cancel: &CancellationToken,
) -> Result<ReviewOutcome> {
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
        return Ok(ReviewOutcome::Unavailable {
            reason: "guardian aggregate review deadline expired".to_string(),
        });
    }
    let child_cancel = cancel.child_token();
    let timeout_secs = remaining.as_secs().max(1);
    let result = match tokio::time::timeout(
        remaining,
        generation.generate_response_with_retry(request, timeout_secs, &child_cancel),
    )
    .await
    {
        Ok(res) => res,
        Err(_) => {
            child_cancel.cancel();
            return Ok(ReviewOutcome::Unavailable {
                reason: "guardian review timed out within aggregate deadline".to_string(),
            });
        }
    };

    if result.as_ref().is_err_and(|error| {
        error.is::<crate::agent_machine::task_budget::Exhausted>()
            || error.is::<super::transition::NamespaceOperationUncertainty>()
    }) {
        return Err(result.unwrap_err());
    }

    Ok(review_generation_result(result))
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

#[cfg(test)]
#[path = "tool_authorization/tests.rs"]
mod tests;
