//! System 1 calibrated Rubric + bounded System 2 escalation engine for `TrustPosture::ConfinedSmart`.
//!
//! Runs ONLY for reviewer-routed escalations (`EscalationRoute::Reviewer`) under
//! `TrustPosture::ConfinedSmart`. Red-line (`EscalationRoute::AlwaysHuman`) rules remain
//! strictly human-gated.

use alan_agent_protocol::{
    Event, ToolCapability, ToolDecisionAudit, UiSystem1Calibration, UiSystem1Snapshot,
};
use alan_llm::{CalibrationKind, DecisionRequest, DecisionResponse};
use anyhow::Result;
use serde_json::{Value, json};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::agent_machine::NormalizedToolCall;

use super::{
    guardian::{
        ReviewContext, ReviewOutcome, build_review_request, build_transcript,
        resolve_guardian_policy, review_generation_result,
    },
    tool_authorization::{
        AuthoritySnapshot, EscalationReviewOutcome, ToolAuthorizationRuntime,
        record_reviewer_denial,
    },
    tool_policy::capability_label,
    transition::{NamespaceAgentFiles, NamespaceGeneration},
};

mod rubric;
pub(crate) mod tier2_fallback;

#[cfg(test)]
pub(crate) use rubric::{
    ActionRiskProfile, RUBRIC_Q_INTENT_ALIGNED, RUBRIC_Q_PROMPT_INJECTION_SIGNAL,
    RUBRIC_Q_REVERSIBLE, RUBRIC_Q_SECRET_OR_EXFIL_RISK, RUBRIC_Q_VERDICT, VERDICT_ALLOW,
    VERDICT_DENY, VERDICT_ESCALATE,
};
pub(crate) use rubric::{
    RubricAssessment, RubricContext, SmartRubricDecision, aggregate_rubric, build_rubric_request,
    classify_action_risk,
};
use tier2_fallback::{
    DecisionAttempt, Tier2Attempt, evaluate_tier2_structured_rubric, remaining_decision_ms,
    remaining_duration, try_decision_with_tier2, try_native_decision,
};

pub(crate) const SMART_APPROVAL_ROLLOUT_EVENT: &str = "smart_approval_decision";

pub(crate) fn map_calibration(calibration: CalibrationKind) -> UiSystem1Calibration {
    match calibration {
        CalibrationKind::Calibrated => UiSystem1Calibration::Calibrated,
        CalibrationKind::Uncalibrated => UiSystem1Calibration::Uncalibrated,
    }
}

/// Extract only the safe Connection/profile name from an evaluator source tag.
/// Never carries credentials, secrets, or request IDs.
pub(crate) fn profile_name_from_source(source: &str) -> Option<String> {
    let name = source
        .strip_prefix("smart_evaluator_profile:")
        .or_else(|| source.strip_prefix("guardian_profile:"))
        .or_else(|| source.strip_prefix("main_connection:"))
        .unwrap_or(source)
        .trim();
    if name.is_empty() || matches!(name, "cancelled" | "dedicated_profile") {
        None
    } else {
        Some(name.to_string())
    }
}

/// Advisory publication of System 1 observability counters to `machine/ui/system1`.
pub(crate) async fn publish_system1_snapshot(
    files: &NamespaceAgentFiles,
    snapshot: &UiSystem1Snapshot,
) {
    if let Err(error) = files.write_ui_system1_snapshot(snapshot).await {
        tracing::warn!(error = %error, "Failed to publish System 1 observability snapshot");
    }
}

/// Seed configured evaluator metadata (when in `ConfinedSmart`) and publish `machine/ui/system1`.
pub(crate) async fn publish_state_system1(state: &mut super::transition::RuntimeLoopState) {
    let files = state.agent_files();
    let snapshot =
        if state.runtime_config.trust_posture == crate::policy::TrustPosture::ConfinedSmart {
            let configured_profile = state
                .runtime_config
                .smart_evaluator_profile
                .clone()
                .or_else(|| state.runtime_config.guardian_profile.clone());
            state
                .machine
                .set_system1_evaluator_metadata(configured_profile, None, None)
        } else {
            state.machine.system1_snapshot()
        };
    publish_system1_snapshot(&files, &snapshot).await;
}

enum System1Outcome {
    Assessed {
        assessment: RubricAssessment,
        source: String,
    },
    FailedClosed {
        reason: String,
        source: String,
    },
}

/// Entry point for `TrustPosture::ConfinedSmart` when `EscalationRoute::Reviewer`.
#[allow(
    clippy::too_many_arguments,
    reason = "Smart escalation review correlates runtime handles, tool call metadata, capability, mutable confirmation details, and pre-review authority snapshot"
)]
pub(super) async fn review_smart_escalation<E, F>(
    runtime: &mut ToolAuthorizationRuntime<'_>,
    tool_call: &NormalizedToolCall,
    tool_arguments: &Value,
    tool_capability: ToolCapability,
    details: &mut Value,
    audit: &ToolDecisionAudit,
    pre_review_authority: &AuthoritySnapshot,
    cancel: &CancellationToken,
    emit: &mut E,
) -> Result<EscalationReviewOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let transcript = build_transcript(runtime.machine.messages());
    let rubric_request = build_rubric_request(&RubricContext {
        transcript: &transcript,
        tool_name: &tool_call.name,
        tool_arguments,
        tool_capability,
        approval_request: details,
    });

    let aggregate_secs = runtime
        .guardian_config
        .timeout_secs
        .min(runtime.llm_request_timeout_secs)
        .max(1);
    let deadline = Instant::now() + std::time::Duration::from_secs(aggregate_secs);

    let (assessment, evaluator_source) =
        match evaluate_system1(runtime, rubric_request, deadline, cancel, emit).await? {
            System1Outcome::Assessed { assessment, source } => (assessment, source),
            System1Outcome::FailedClosed { reason, source } => {
                details["smart_approval"] = json!({
                    "outcome": "fail_closed_human",
                    "evaluator_source": source,
                    "reason": reason,
                });
                let snap = runtime.machine.record_system1_escalation(
                    profile_name_from_source(&source),
                    None,
                    UiSystem1Calibration::Unavailable,
                );
                publish_system1_snapshot(&runtime.agent_files, &snap).await;
                record_smart_rollout(
                    runtime,
                    tool_call,
                    tool_capability,
                    "fail_closed_human",
                    &source,
                    None,
                    Some(&reason),
                    None,
                );
                let notice = format!("smart review failed closed ({reason}); pausing for you");
                publish_inline_notice(runtime, notice, emit).await?;
                return Ok(EscalationReviewOutcome::Human);
            }
        };

    let risk = classify_action_risk(&tool_call.name, tool_arguments, tool_capability, audit);
    match aggregate_rubric(&assessment, risk) {
        SmartRubricDecision::SilentAllow => {
            if authority_changed(runtime, pre_review_authority) {
                return fail_stale_authority(
                    runtime,
                    tool_call,
                    tool_capability,
                    details,
                    &assessment,
                    &evaluator_source,
                    "authority, posture, or grants changed during smart review",
                    None,
                    true,
                    emit,
                )
                .await;
            }
            runtime.machine.record_guardian_review(false);
            let snap = runtime.machine.record_system1_allow(
                profile_name_from_source(&evaluator_source),
                Some(assessment.model.clone()),
                map_calibration(assessment.calibration),
            );
            publish_system1_snapshot(&runtime.agent_files, &snap).await;
            record_smart_rollout(
                runtime,
                tool_call,
                tool_capability,
                "silent_allow",
                &evaluator_source,
                Some(&assessment),
                None,
                None,
            );
            Ok(EscalationReviewOutcome::Authorized)
        }
        SmartRubricDecision::Deny {
            rationale,
            triggered_dimensions,
        } => {
            let tripped = runtime.machine.record_guardian_review(true);
            let snap = runtime.machine.record_system1_deny(
                profile_name_from_source(&evaluator_source),
                Some(assessment.model.clone()),
                map_calibration(assessment.calibration),
            );
            publish_system1_snapshot(&runtime.agent_files, &snap).await;
            let meta = json!({
                "triggered_dimensions": triggered_dimensions,
                "circuit_breaker_tripped": tripped,
            });
            record_smart_rollout(
                runtime,
                tool_call,
                tool_capability,
                "deny",
                &evaluator_source,
                Some(&assessment),
                Some(&rationale),
                Some(meta),
            );
            publish_inline_notice(runtime, format!("smart review denied: {rationale}"), emit)
                .await?;
            if tripped {
                details["smart_approval"] =
                    assessment.to_breakdown_json("deny", Some(&rationale), &evaluator_source, None);
                let breaker_msg =
                    "auto-review circuit breaker tripped; pausing for you".to_string();
                publish_inline_notice(runtime, breaker_msg, emit).await?;
                Ok(EscalationReviewOutcome::Human)
            } else {
                record_reviewer_denial(runtime, tool_call, tool_arguments, audit, &rationale, emit)
                    .await;
                Ok(EscalationReviewOutcome::Completed)
            }
        }
        SmartRubricDecision::EscalateToSystem2 { reason } => {
            let snap = runtime.machine.record_system1_escalation(
                profile_name_from_source(&evaluator_source),
                Some(assessment.model.clone()),
                map_calibration(assessment.calibration),
            );
            publish_system1_snapshot(&runtime.agent_files, &snap).await;
            handle_system2_escalation(
                runtime,
                tool_call,
                tool_arguments,
                tool_capability,
                details,
                audit,
                pre_review_authority,
                &transcript,
                &assessment,
                &evaluator_source,
                &reason,
                deadline,
                cancel,
                emit,
            )
            .await
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "System 2 escalation carries System 1 rubric evidence, authority snapshot, and confirmation details"
)]
async fn handle_system2_escalation<E, F>(
    runtime: &mut ToolAuthorizationRuntime<'_>,
    tool_call: &NormalizedToolCall,
    tool_arguments: &Value,
    tool_capability: ToolCapability,
    details: &mut Value,
    audit: &ToolDecisionAudit,
    pre_review_authority: &AuthoritySnapshot,
    transcript: &str,
    assessment: &RubricAssessment,
    evaluator_source: &str,
    reason: &str,
    deadline: Instant,
    cancel: &CancellationToken,
    emit: &mut E,
) -> Result<EscalationReviewOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let mut s2_details = details.clone();
    s2_details["smart_approval_rubric"] = assessment.summary_for_system2(reason);

    let s2_outcome =
        run_bounded_system2(runtime, transcript, &s2_details, deadline, cancel, emit).await?;

    if matches!(s2_outcome, ReviewOutcome::Allow)
        && authority_changed(runtime, pre_review_authority)
    {
        return fail_stale_authority(
            runtime,
            tool_call,
            tool_capability,
            details,
            assessment,
            evaluator_source,
            "authority, posture, or grants changed during smart/guardian review",
            Some(json!({"outcome": "allow_invalidated"})),
            false,
            emit,
        )
        .await;
    }

    match s2_outcome {
        ReviewOutcome::Allow => {
            runtime.machine.record_guardian_review(false);
            record_smart_rollout(
                runtime,
                tool_call,
                tool_capability,
                "system2_allow",
                evaluator_source,
                Some(assessment),
                Some(reason),
                Some(json!({"outcome": "allow"})),
            );
            Ok(EscalationReviewOutcome::Authorized)
        }
        ReviewOutcome::Deny {
            rationale: s2_rationale,
        } => {
            let tripped = runtime.machine.record_guardian_review(true);
            let s2_meta = json!({
                "outcome": "deny",
                "rationale": s2_rationale,
                "circuit_breaker_tripped": tripped,
            });
            record_smart_rollout(
                runtime,
                tool_call,
                tool_capability,
                "system2_deny",
                evaluator_source,
                Some(assessment),
                Some(reason),
                Some(s2_meta.clone()),
            );
            publish_inline_notice(runtime, format!("auto-review denied: {s2_rationale}"), emit)
                .await?;
            if tripped {
                details["smart_approval"] = assessment.to_breakdown_json(
                    "system2_deny_circuit_breaker",
                    Some(&s2_rationale),
                    evaluator_source,
                    Some(s2_meta),
                );
                let breaker_msg =
                    "auto-review circuit breaker tripped; pausing for you".to_string();
                publish_inline_notice(runtime, breaker_msg, emit).await?;
                Ok(EscalationReviewOutcome::Human)
            } else {
                record_reviewer_denial(
                    runtime,
                    tool_call,
                    tool_arguments,
                    audit,
                    &s2_rationale,
                    emit,
                )
                .await;
                Ok(EscalationReviewOutcome::Completed)
            }
        }
        ReviewOutcome::Unavailable { reason: s2_reason } => {
            let s2_meta = json!({"outcome": "unavailable", "reason": s2_reason});
            details["smart_approval"] = assessment.to_breakdown_json(
                "escalate_human",
                Some(reason),
                evaluator_source,
                Some(s2_meta.clone()),
            );
            record_smart_rollout(
                runtime,
                tool_call,
                tool_capability,
                "escalate_human",
                evaluator_source,
                Some(assessment),
                Some(reason),
                Some(s2_meta),
            );
            let notice = format!(
                "smart review escalating to you ({}; {}; guardian unavailable: {})",
                assessment.concise_notice_summary(),
                reason,
                s2_reason,
            );
            publish_inline_notice(runtime, notice, emit).await?;
            Ok(EscalationReviewOutcome::Human)
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Stale-authority failure handler records System 1 rollout, updates confirmation details, and emits inline user notice"
)]
async fn fail_stale_authority<E, F>(
    runtime: &mut ToolAuthorizationRuntime<'_>,
    tool_call: &NormalizedToolCall,
    tool_capability: ToolCapability,
    details: &mut Value,
    assessment: &RubricAssessment,
    evaluator_source: &str,
    reason: &str,
    system2: Option<Value>,
    record_escalation: bool,
    emit: &mut E,
) -> Result<EscalationReviewOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if record_escalation {
        let snap = runtime.machine.record_system1_escalation(
            profile_name_from_source(evaluator_source),
            Some(assessment.model.clone()),
            map_calibration(assessment.calibration),
        );
        publish_system1_snapshot(&runtime.agent_files, &snap).await;
    }
    details["smart_approval"] = assessment.to_breakdown_json(
        "invalidated_stale_authority",
        Some(reason),
        evaluator_source,
        system2.clone(),
    );
    record_smart_rollout(
        runtime,
        tool_call,
        tool_capability,
        "invalidated_stale_authority",
        evaluator_source,
        Some(assessment),
        Some(reason),
        system2,
    );
    let notice = format!(
        "smart approval invalidated because {reason} ({}); pausing for you",
        assessment.concise_notice_summary()
    );
    publish_inline_notice(runtime, notice, emit).await?;
    Ok(EscalationReviewOutcome::Human)
}

fn authority_changed(
    runtime: &ToolAuthorizationRuntime<'_>,
    pre_review_authority: &AuthoritySnapshot,
) -> bool {
    AuthoritySnapshot::capture(runtime) != *pre_review_authority
}

async fn publish_inline_notice<E, F>(
    runtime: &ToolAuthorizationRuntime<'_>,
    message: String,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    super::ui_surfaces::warning(&runtime.agent_files, message.clone()).await?;
    emit(Event::Warning { message }).await;
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "Smart rollout helper combines runtime handle, tool call metadata, outcome, rubric assessment, and System 2 evidence"
)]
fn record_smart_rollout(
    runtime: &mut ToolAuthorizationRuntime<'_>,
    tool_call: &NormalizedToolCall,
    tool_capability: ToolCapability,
    outcome: &str,
    evaluator_source: &str,
    assessment: Option<&RubricAssessment>,
    reason: Option<&str>,
    system2: Option<Value>,
) {
    let payload = match assessment {
        Some(assessment) => json!({
            "tool_call_id": tool_call.id,
            "tool_name": tool_call.name,
            "capability": capability_label(tool_capability),
            "outcome": outcome,
            "evaluator_source": evaluator_source,
            "calibration": assessment.calibration,
            "provider": assessment.provider,
            "model": assessment.model,
            "reason": reason,
            "rubric": assessment.rubric_scores_json(),
            "system2": system2,
        }),
        None => json!({
            "tool_call_id": tool_call.id,
            "tool_name": tool_call.name,
            "capability": capability_label(tool_capability),
            "outcome": outcome,
            "evaluator_source": evaluator_source,
            "reason": reason,
            "system2": system2,
        }),
    };
    runtime
        .machine
        .record_event(SMART_APPROVAL_ROLLOUT_EVENT, payload);
}

/// Dispatch System 1 Rubric over:
/// `smart_evaluator_profile` -> `guardian_profile` -> bounded main Connection fallback
/// without mutating main Connection bindings.
///
/// Tier 1 native `decide()` runs first; when native `decide()` returns
/// `NamespaceDecisionFailure::Unavailable`, seamlessly falls back to Tier 2
/// `Uncalibrated` no-tools structured generation on the captured guardian/active
/// Connection, failing strictly closed to Human on double failure, timeout,
/// cancellation, or malformed output.
async fn evaluate_system1<E, F>(
    runtime: &mut ToolAuthorizationRuntime<'_>,
    request: DecisionRequest,
    deadline: Instant,
    cancel: &CancellationToken,
    emit: &mut E,
) -> Result<System1Outcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    const LABEL: &str = "smart evaluation";
    if cancel.is_cancelled() {
        return Ok(System1Outcome::FailedClosed {
            reason: "smart evaluation cancelled".to_string(),
            source: "cancelled".to_string(),
        });
    }

    let mut last_unavailable: Option<(String, String)> = None;
    let mut smart_fallback: Option<(String, NamespaceGeneration)> = None;
    let smart_profile = runtime.smart_evaluator_profile;
    let guardian_profile = runtime.guardian_profile;
    let used_dedicated = smart_profile.is_some() || guardian_profile.is_some();

    if let Some(profile) = smart_profile {
        let source = format!("smart_evaluator_profile:{profile}");
        match try_dedicated_decision(runtime, profile, true, request.clone(), deadline, cancel)
            .await
        {
            DecisionAttempt::Success(resp) => return Ok(finalize_system1_response(resp, source)),
            DecisionAttempt::FailClosed(reason) => {
                return Ok(System1Outcome::FailedClosed { reason, source });
            }
            DecisionAttempt::RetryNext {
                reason,
                fallback_generation,
            } => {
                if let Some(generation) = fallback_generation {
                    smart_fallback = Some((source.clone(), generation));
                }
                last_unavailable = Some((source, reason));
            }
        }
    }

    if let Some(profile) = guardian_profile
        && smart_profile != Some(profile)
    {
        if let Some((prev_src, prev_reason)) = &last_unavailable {
            let msg = format!(
                "{prev_src} unavailable ({prev_reason}); falling back to guardian profile `{profile}` for smart evaluation"
            );
            publish_inline_notice(runtime, msg, emit).await?;
        }
        let source = format!("guardian_profile:{profile}");
        match try_dedicated_decision(runtime, profile, false, request.clone(), deadline, cancel)
            .await
        {
            DecisionAttempt::Success(resp) => return Ok(finalize_system1_response(resp, source)),
            DecisionAttempt::FailClosed(reason) => {
                return Ok(System1Outcome::FailedClosed { reason, source });
            }
            DecisionAttempt::RetryNext {
                reason,
                fallback_generation,
            } => {
                if let Some(generation) = fallback_generation {
                    tracing::debug!(
                        profile,
                        %reason,
                        "Guardian native decide() unavailable; falling back to Tier 2 uncalibrated structured rubric"
                    );
                    match evaluate_tier2_structured_rubric(
                        &generation,
                        &request,
                        deadline,
                        cancel,
                        LABEL,
                    )
                    .await
                    {
                        Tier2Attempt::Success(resp) => {
                            return Ok(finalize_system1_response(resp, source));
                        }
                        Tier2Attempt::FailClosed(t2_reason) => {
                            return Ok(System1Outcome::FailedClosed {
                                reason: t2_reason,
                                source,
                            });
                        }
                        Tier2Attempt::Unavailable(t2_reason) => {
                            last_unavailable = Some((source, format!("{reason}; {t2_reason}")));
                        }
                    }
                } else {
                    last_unavailable = Some((source, reason));
                }
            }
        }
    }

    if let Some((smart_source, smart_generation)) = smart_fallback {
        tracing::debug!(
            source = %smart_source,
            "Smart evaluator native decide() unavailable; trying Tier 2 uncalibrated structured rubric"
        );
        match evaluate_tier2_structured_rubric(&smart_generation, &request, deadline, cancel, LABEL)
            .await
        {
            Tier2Attempt::Success(resp) => {
                return Ok(finalize_system1_response(resp, smart_source));
            }
            Tier2Attempt::FailClosed(t2_reason) => {
                return Ok(System1Outcome::FailedClosed {
                    reason: t2_reason,
                    source: smart_source,
                });
            }
            Tier2Attempt::Unavailable(t2_reason) => {
                let reason = match last_unavailable {
                    Some((_, prev)) => format!("{prev}; {t2_reason}"),
                    None => t2_reason,
                };
                last_unavailable = Some((smart_source, reason));
            }
        }
    }

    if used_dedicated {
        let (prev_src, prev_reason) = last_unavailable.unwrap_or_else(|| {
            (
                "dedicated_profile".to_string(),
                "dedicated smart evaluator unavailable".to_string(),
            )
        });
        if remaining_decision_ms(deadline).is_none() {
            return Ok(System1Outcome::FailedClosed {
                reason: format!("{prev_reason}; aggregate deadline exhausted"),
                source: prev_src,
            });
        }
        if !runtime
            .machine
            .try_consume_guardian_fallback(runtime.guardian_config.max_fallbacks_per_turn)
        {
            let msg = format!(
                "dedicated smart evaluator fallback budget exhausted ({prev_src}: {prev_reason}); pausing for human"
            );
            publish_inline_notice(runtime, msg, emit).await?;
            return Ok(System1Outcome::FailedClosed {
                reason: format!("fallback budget exhausted after {prev_src}: {prev_reason}"),
                source: prev_src,
            });
        }
        let msg = format!(
            "{prev_src} unavailable ({prev_reason}); falling back to primary connection for smart evaluation"
        );
        publish_inline_notice(runtime, msg, emit).await?;
    }

    let main_conn = runtime.generation.as_decision_connection();
    let source = format!("main_connection:{}", main_conn.connection_name());
    match try_decision_with_tier2(&main_conn, request, deadline, cancel, LABEL).await {
        DecisionAttempt::Success(resp) => Ok(finalize_system1_response(resp, source)),
        DecisionAttempt::RetryNext { reason, .. } | DecisionAttempt::FailClosed(reason) => {
            Ok(System1Outcome::FailedClosed { reason, source })
        }
    }
}

fn finalize_system1_response(response: DecisionResponse, source: String) -> System1Outcome {
    match RubricAssessment::from_response(&response) {
        Ok(assessment) => System1Outcome::Assessed { assessment, source },
        Err(err) => System1Outcome::FailedClosed {
            reason: format!("malformed rubric assessment: {err}"),
            source,
        },
    }
}

async fn try_dedicated_decision(
    runtime: &ToolAuthorizationRuntime<'_>,
    profile: &str,
    is_smart_profile: bool,
    request: DecisionRequest,
    deadline: Instant,
    cancel: &CancellationToken,
) -> DecisionAttempt {
    if cancel.is_cancelled() {
        return DecisionAttempt::FailClosed("smart evaluation cancelled".to_string());
    }
    let Some(remaining) = remaining_duration(deadline) else {
        return DecisionAttempt::FailClosed(
            "smart evaluation aggregate deadline expired".to_string(),
        );
    };
    let Some(authority) = runtime.authority.as_ref() else {
        return DecisionAttempt::RetryNext {
            reason: "smart evaluator Connection authority unavailable".to_string(),
            fallback_generation: None,
        };
    };

    let capture_fut = async {
        if is_smart_profile {
            authority.capture_smart_decision_connection(profile).await
        } else {
            authority
                .capture_guardian_decision_connection(profile)
                .await
        }
    };
    let connection = match tokio::time::timeout(remaining, capture_fut).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(error)) => {
            return DecisionAttempt::RetryNext {
                reason: format!("profile capture failed: {error:#}"),
                fallback_generation: None,
            };
        }
        Err(_) => {
            return DecisionAttempt::FailClosed("profile capture timed out".to_string());
        }
    };

    try_native_decision(&connection, request, deadline, cancel, "smart evaluation").await
}

/// Bounded System 2 (Guardian): at most ONE no-tools generation call with the System 1 Rubric summary.
async fn run_bounded_system2<E, F>(
    runtime: &mut ToolAuthorizationRuntime<'_>,
    transcript: &str,
    enriched_details: &Value,
    deadline: Instant,
    cancel: &CancellationToken,
    emit: &mut E,
) -> Result<ReviewOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if cancel.is_cancelled() {
        return Ok(ReviewOutcome::Unavailable {
            reason: "system 2 guardian review cancelled".to_string(),
        });
    }
    if remaining_duration(deadline).is_none() {
        return Ok(ReviewOutcome::Unavailable {
            reason: "system 2 guardian aggregate deadline expired".to_string(),
        });
    }

    let policy = resolve_guardian_policy(runtime.guardian_config.policy_path.as_deref());
    let request = build_review_request(&ReviewContext {
        policy: &policy,
        transcript,
        approval_request: enriched_details,
    });
    debug_assert!(
        request.tools.is_empty(),
        "System 2 Guardian review request must carry zero tools"
    );

    if let Some(profile) = runtime.guardian_profile {
        let capture_result = match runtime.authority.as_ref() {
            Some(authority) => match remaining_duration(deadline) {
                Some(remaining) => {
                    match tokio::time::timeout(
                        remaining,
                        authority.capture_guardian_generation(profile),
                    )
                    .await
                    {
                        Ok(Ok(generation)) => Ok(generation),
                        Ok(Err(error)) => {
                            Err(format!("guardian profile capture failed: {error:#}"))
                        }
                        Err(_) => {
                            return Ok(ReviewOutcome::Unavailable {
                                reason: "guardian profile capture timed out".to_string(),
                            });
                        }
                    }
                }
                None => {
                    return Ok(ReviewOutcome::Unavailable {
                        reason: "system 2 guardian aggregate deadline expired".to_string(),
                    });
                }
            },
            None => Err("guardian Connection authority unavailable".to_string()),
        };

        match capture_result {
            Ok(captured) => {
                return Ok(run_single_guardian_call(&captured, &request, deadline, cancel).await);
            }
            Err(reason) => {
                if remaining_duration(deadline).is_none() {
                    return Ok(ReviewOutcome::Unavailable { reason });
                }
                if !runtime
                    .machine
                    .try_consume_guardian_fallback(runtime.guardian_config.max_fallbacks_per_turn)
                {
                    let msg = format!(
                        "dedicated guardian `{profile}` fallback budget exhausted; pausing for human"
                    );
                    publish_inline_notice(runtime, msg, emit).await?;
                    return Ok(ReviewOutcome::Unavailable { reason });
                }
                let msg = format!(
                    "dedicated guardian `{profile}` unavailable ({reason}); falling back to primary connection for bounded System 2 review"
                );
                publish_inline_notice(runtime, msg, emit).await?;
                return Ok(run_single_guardian_call(
                    &runtime.generation,
                    &request,
                    deadline,
                    cancel,
                )
                .await);
            }
        }
    }

    Ok(run_single_guardian_call(&runtime.generation, &request, deadline, cancel).await)
}

async fn run_single_guardian_call(
    generation: &NamespaceGeneration,
    request: &alan_llm::GenerationRequest,
    deadline: Instant,
    cancel: &CancellationToken,
) -> ReviewOutcome {
    if cancel.is_cancelled() {
        return ReviewOutcome::Unavailable {
            reason: "system 2 guardian review cancelled".to_string(),
        };
    }
    let Some(remaining) = remaining_duration(deadline) else {
        return ReviewOutcome::Unavailable {
            reason: "system 2 guardian aggregate deadline expired".to_string(),
        };
    };
    let child_cancel = cancel.child_token();
    let timeout_secs = remaining.as_secs().max(1);
    let result = match tokio::time::timeout(
        remaining,
        generation.generate_controlled(request, timeout_secs, &child_cancel),
    )
    .await
    {
        Ok(res) => res,
        Err(_) => {
            child_cancel.cancel();
            return ReviewOutcome::Unavailable {
                reason: "system 2 guardian review timed out within aggregate deadline".to_string(),
            };
        }
    };
    review_generation_result(result)
}

#[cfg(test)]
mod tests;
