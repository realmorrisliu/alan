use alan_agent_protocol::{AdaptivePresentationHint, ConfirmationYieldPayload, Event};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::approval::{
    EFFECT_REPLAY_CHECKPOINT_PREFIX, EFFECT_REPLAY_CHECKPOINT_TYPE, PendingConfirmation,
    TOOL_ESCALATION_CHECKPOINT_PREFIX, TOOL_ESCALATION_CHECKPOINT_TYPE,
    append_skill_permission_hints, is_runtime_confirmation_checkpoint_type, replays_tool_calls,
};
use crate::evidence::{
    payload_needs_projection, project_evidence_payload, redact_evidence_payload,
    redaction_markers_in_text,
};

use super::loop_guard::ToolLoopGuard;
use super::steering_queue::handle_queued_steering_inputs;
#[cfg(test)]
use super::tool_effect_lifecycle::{EffectCategory, build_effect_identity};
use super::tool_effect_lifecycle::{ToolEffectLifecycle, ToolEffectPlan};
use super::tool_policy::{ToolPolicyDecision, evaluate_tool_policy};
use super::transition::RuntimeLoopState;
use super::turn_driver::TurnInputBroker;
use super::turn_support::{check_turn_cancelled, tool_result_preview};
use super::virtual_tools::{VirtualToolOutcome, try_handle_virtual_tool_call};
use crate::agent_machine::NormalizedToolCall;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolOrchestratorOutcome {
    ContinueToolBatch { refresh_context: bool },
    PauseTurn,
    EndTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolBatchOrchestratorOutcome {
    ContinueTurnLoop { refresh_context: bool },
    PauseTurn,
    EndTurn { surfaces_refreshed: bool },
}

fn confirmation_payload(
    checkpoint_type: String,
    summary: String,
    details: Value,
    options: Vec<String>,
) -> ConfirmationYieldPayload {
    let presentation_hints = if is_runtime_confirmation_checkpoint_type(&checkpoint_type) {
        vec![AdaptivePresentationHint::Dangerous]
    } else {
        vec![]
    };

    let default_option = options
        .iter()
        .find(|option| option.as_str() == "approve")
        .cloned()
        .or_else(|| options.first().cloned());

    ConfirmationYieldPayload {
        checkpoint_type,
        summary,
        details: Some(details),
        options,
        default_option,
        presentation_hints,
    }
}

pub(super) struct ToolTurnOrchestrator {
    loop_guard: ToolLoopGuard,
}

impl ToolTurnOrchestrator {
    pub(super) fn new(max_tool_loops: Option<usize>, tool_repeat_limit: usize) -> Self {
        Self {
            loop_guard: ToolLoopGuard::new(max_tool_loops, tool_repeat_limit),
        }
    }

    pub(super) async fn orchestrate_tool_batch<E, F>(
        &mut self,
        state: &mut RuntimeLoopState,
        tool_calls: &[NormalizedToolCall],
        inputs: ToolOrchestratorInputs<'_>,
        emit: &mut E,
    ) -> Result<ToolBatchOrchestratorOutcome>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        self.orchestrate_tool_batch_internal(state, tool_calls, inputs, None, None, emit)
            .await
    }

    async fn orchestrate_tool_batch_internal<E, F>(
        &mut self,
        state: &mut RuntimeLoopState,
        tool_calls: &[NormalizedToolCall],
        inputs: ToolOrchestratorInputs<'_>,
        approved_unknown_effect_call_index: Option<usize>,
        approved_tool_escalation_call_index: Option<usize>,
        emit: &mut E,
    ) -> Result<ToolBatchOrchestratorOutcome>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        orchestrate_tool_batch_with_guard(
            state,
            &mut self.loop_guard,
            tool_calls,
            inputs,
            approved_unknown_effect_call_index,
            approved_tool_escalation_call_index,
            emit,
        )
        .await
    }
}

pub(super) async fn replay_approved_tool_call_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    approved_unknown_effect_call_id: Option<&str>,
    approved_tool_escalation_call_id: Option<&str>,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    replay_approved_tool_batch_with_cancel(
        state,
        std::slice::from_ref(tool_call),
        approved_unknown_effect_call_id,
        approved_tool_escalation_call_id,
        inputs,
        emit,
    )
    .await
}

pub(super) async fn replay_approved_tool_batch_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    approved_unknown_effect_call_id: Option<&str>,
    approved_tool_escalation_call_id: Option<&str>,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let max_tool_loops = if state.runtime_config.max_tool_loops == 0 {
        None
    } else {
        Some(state.runtime_config.max_tool_loops)
    };
    let approved_unknown_effect_call_index = approved_unknown_effect_call_id.and_then(|call_id| {
        tool_calls
            .first()
            .filter(|call| call.id == call_id)
            .map(|_| 0)
    });
    let approved_tool_escalation_call_index =
        approved_tool_escalation_call_id.and_then(|call_id| {
            tool_calls
                .first()
                .filter(|call| call.id == call_id)
                .map(|_| 0)
        });
    let mut orchestrator =
        ToolTurnOrchestrator::new(max_tool_loops, state.runtime_config.tool_repeat_limit);
    orchestrator
        .orchestrate_tool_batch_internal(
            state,
            tool_calls,
            inputs,
            approved_unknown_effect_call_index,
            approved_tool_escalation_call_index,
            emit,
        )
        .await
}

#[derive(Clone, Copy)]
pub(super) struct ToolOrchestratorInputs<'a> {
    pub cancel: &'a CancellationToken,
    pub steering_broker: Option<&'a TurnInputBroker>,
}

fn maybe_allow_approved_tool_escalation_replay(
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

fn namespace_tool_payload(
    tool: crate::runtime::NamespaceToolActionOutput,
) -> Result<serde_json::Value> {
    let trimmed = tool.output.trim();
    let mut payload = if trimmed.is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(trimmed)
            .unwrap_or_else(|_| json!({ "output": tool.output.clone() }))
    };
    let process = format!("/proc/{}", tool.pid);
    match &mut payload {
        Value::Object(object) => {
            object
                .entry("success")
                .or_insert(Value::Bool(tool.exit_code == 0));
            object.entry("exit_code").or_insert(json!(tool.exit_code));
            object.entry("process").or_insert(json!(process));
            object.insert("action_id".to_string(), json!(tool.action_id));
        }
        other => {
            payload = json!({
                "success": tool.exit_code == 0,
                "exit_code": tool.exit_code,
                "output": other.clone(),
                "process": process,
                "action_id": tool.action_id,
            });
        }
    }
    Ok(payload)
}

async fn tool_payload_for_tape(state: &RuntimeLoopState, payload: &Value) -> Value {
    let redacted_payload = redact_evidence_payload(payload);
    if !payload_needs_projection(&redacted_payload) {
        return redacted_payload;
    }

    let Some(action_id) = payload.get("action_id").and_then(Value::as_str) else {
        return project_evidence_payload(
            payload,
            None,
            Vec::new(),
            Some("reference_unresolvable".to_string()),
        );
    };
    if action_id.is_empty() || action_id.contains('/') {
        return project_evidence_payload(
            payload,
            None,
            Vec::new(),
            Some("reference_unresolvable".to_string()),
        );
    }
    let path = format!("{}/actions/{action_id}/output", state.agent_path());
    let agent_files = state.agent_files();
    let reference = agent_files.evidence_reference(path).await;
    let redactions = if let Some(reference) = reference.as_ref() {
        agent_files
            .resolve_evidence_reference(reference, None, None)
            .await
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|text| redaction_markers_in_text(&text))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let fallback_reason = reference
        .is_none()
        .then(|| "reference_unresolvable".to_string());
    project_evidence_payload(payload, reference, redactions, fallback_reason)
}

async fn execute_tool_effect(
    tools: crate::runtime::transition::NamespaceToolExecution,
    tool_name: &str,
    tool_arguments: Value,
    cancel: &CancellationToken,
    timeout_secs: usize,
) -> Result<Value> {
    let executable = format!("/bin/{tool_name}");
    let arguments_doc =
        serde_json::to_string(&tool_arguments).context("serialize tool arguments")?;
    let tool = tools
        .run_action_with_cancel_and_timeout(
            tool_name,
            &executable,
            [arguments_doc],
            cancel,
            timeout_secs,
        )
        .await?;
    namespace_tool_payload(tool)
}

async fn orchestrate_tool_call_with_guard<E, F>(
    state: &mut RuntimeLoopState,
    loop_guard: &mut ToolLoopGuard,
    tool_call: &NormalizedToolCall,
    inputs: ToolOrchestratorInputs<'_>,
    allow_approved_unknown_effect_execution: bool,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<ToolOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let tool_arguments = tool_call.arguments.clone();

    if let Some(msg) = loop_guard.before_tool_call(&tool_call.name, &tool_arguments) {
        emit(Event::Error {
            message: msg.clone(),
            recoverable: true,
        })
        .await;
        emit(Event::TextDelta {
            chunk: msg,
            is_final: true,
        })
        .await;
        return Ok(ToolOrchestratorOutcome::EndTurn);
    }

    match try_handle_virtual_tool_call(
        state,
        tool_call,
        &tool_arguments,
        inputs.cancel,
        allow_approved_tool_escalation_execution,
        emit,
    )
    .await?
    {
        VirtualToolOutcome::NotVirtual => {}
        VirtualToolOutcome::Continue { refresh_context } => {
            return Ok(ToolOrchestratorOutcome::ContinueToolBatch { refresh_context });
        }
        VirtualToolOutcome::PauseTurn => return Ok(ToolOrchestratorOutcome::PauseTurn),
        VirtualToolOutcome::EndTurn => return Ok(ToolOrchestratorOutcome::EndTurn),
    }

    let tool_package = state
        .tool_execution()
        .discover_packages()
        .await?
        .into_iter()
        .find(|package| package.name == tool_call.name);
    let Some(tool_package) = tool_package else {
        let payload = json!({
            "success": false,
            "error": format!(
                "Tool '{}' is unavailable because its executable and valid manifest are not both mounted",
                tool_call.name
            )
        });
        emit(Event::ToolCallStarted {
            title: None,
            id: tool_call.id.clone(),
            name: tool_call.name.clone(),
            audit: None,
        })
        .await;
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
        return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
            refresh_context: false,
        });
    };
    let tool_capability = state
        .tool_execution()
        .resolve_capability(&tool_package, &tool_arguments);
    let current_tool_cwd = state.tool_execution().default_cwd();
    let policy_decision = maybe_allow_approved_tool_escalation_replay(
        evaluate_tool_policy(
            &state.runtime_config.policy_engine,
            &state.runtime_config.governance,
            &tool_call.name,
            &tool_arguments,
            tool_capability,
            current_tool_cwd.as_deref(),
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

    let tool_audit = match policy_decision {
        ToolPolicyDecision::Allow { audit } => Some(audit),
        ToolPolicyDecision::Escalate {
            summary,
            mut details,
            audit,
            route,
        } => {
            details["escalation_route"] = json!(match route {
                super::tool_policy::EscalationRoute::Reviewer => "reviewer",
                super::tool_policy::EscalationRoute::AlwaysHuman => "always_human",
            });
            details["replay_tool_call"] = json!({
                "call_id": tool_call.id,
                "tool_name": tool_call.name,
                "arguments": tool_arguments,
            });
            details = append_skill_permission_hints(details, state.machine.active_skills());

            // Reviewer-routed escalations consult the guardian before pausing for
            // a human. The sandbox + the deterministic red line remain the
            // boundary; the reviewer only decides whether to bother the human.
            let go_human = if matches!(route, super::tool_policy::EscalationRoute::Reviewer) {
                let transcript = super::guardian::build_transcript(state.machine.messages());
                let outcome = {
                    let review_ctx = super::guardian::ReviewContext {
                        policy: super::guardian::DEFAULT_REVIEWER_POLICY,
                        transcript: &transcript,
                        approval_request: &details,
                    };
                    let llm_request_timeout_secs = state.runtime_config.llm_request_timeout_secs;
                    let request = super::guardian::build_review_request(&review_ctx);
                    let result = state
                        .namespace_generation()
                        .generate_response_with_retry(
                            request,
                            llm_request_timeout_secs,
                            inputs.cancel,
                        )
                        .await;
                    super::guardian::review_generation_result(result)
                };
                match outcome {
                    super::guardian::ReviewOutcome::Allow => {
                        state.machine.record_guardian_review(false);
                        false
                    }
                    super::guardian::ReviewOutcome::Deny { rationale } => {
                        let tripped = state.machine.record_guardian_review(true);
                        let message = format!("auto-review denied: {rationale}");
                        super::ui_surfaces::warning(&state.agent_files(), message.clone()).await?;
                        emit(Event::Warning { message }).await;
                        if tripped {
                            let message =
                                "auto-review circuit breaker tripped; pausing for you".to_string();
                            super::ui_surfaces::warning(&state.agent_files(), message.clone())
                                .await?;
                            emit(Event::Warning { message }).await;
                            true
                        } else {
                            // Self-correction: feed the denial back to the agent.
                            let denied_payload = json!({
                                "status": "denied_by_reviewer",
                                "reason": rationale,
                                "instruction": "Do not work around this denial. Pursue a \
                                 materially safer alternative, or stop and ask the user."
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
                            state.machine.record_tool_call_with_audit(
                                &tool_call.name,
                                tool_arguments.clone(),
                                denied_payload.clone(),
                                false,
                                Some(audit),
                            );
                            state.machine.add_tool_message(
                                &tool_call.id,
                                &tool_call.name,
                                denied_payload,
                            );
                            return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                                refresh_context: false,
                            });
                        }
                    }
                    super::guardian::ReviewOutcome::Unavailable { reason } => {
                        tracing::warn!(%reason, "auto-review unavailable; pausing for human");
                        true
                    }
                }
            } else {
                // Always-human red line.
                true
            };

            if go_human {
                let pending = PendingConfirmation {
                    checkpoint_id: format!("{TOOL_ESCALATION_CHECKPOINT_PREFIX}{}", tool_call.id),
                    checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
                    summary,
                    details,
                    options: vec!["approve".to_string(), "reject".to_string()],
                };
                let request_id = state
                    .agent_files()
                    .write_confirmation_request(&pending)
                    .await?;
                state.machine.record_tool_call_with_audit(
                    &tool_call.name,
                    tool_arguments.clone(),
                    json!({"status":"escalation_required","request_id": request_id.clone()}),
                    true,
                    Some(audit),
                );
                state
                    .machine
                    .set_confirmation_for_request(request_id.clone(), pending.clone());
                super::ui_surfaces::paused(&state.agent_files()).await?;
                emit(Event::Yield {
                    request_id,
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    payload: serde_json::to_value(confirmation_payload(
                        pending.checkpoint_type.clone(),
                        pending.summary.clone(),
                        pending.details.clone(),
                        pending.options.clone(),
                    ))
                    .unwrap_or_else(|_| json!({})),
                })
                .await;
                return Ok(ToolOrchestratorOutcome::PauseTurn);
            }

            // Reviewer approved — proceed to execute (still sandboxed).
            Some(audit)
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
            return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                refresh_context: false,
            });
        }
    };

    let effect_lifecycle = ToolEffectLifecycle::for_call(
        &state.machine,
        state.process_path(),
        tool_call.id.clone(),
        tool_call.name.clone(),
        &tool_arguments,
        tool_capability,
    );
    let effect_plan = effect_lifecycle
        .as_ref()
        .map(|effect| effect.plan(&state.machine, allow_approved_unknown_effect_execution));

    if matches!(effect_plan.as_ref(), Some(ToolEffectPlan::ConfirmUnknown)) {
        let effect = effect_lifecycle
            .as_ref()
            .expect("effect plan requires a lifecycle");
        let escalation_reason =
            "Previous side effect attempt has unknown status; explicit confirmation required";
        effect.record_unknown_confirmation(&mut state.machine, escalation_reason);

        let pending = PendingConfirmation {
            checkpoint_id: format!("{EFFECT_REPLAY_CHECKPOINT_PREFIX}{}", tool_call.id),
            checkpoint_type: EFFECT_REPLAY_CHECKPOINT_TYPE.to_string(),
            summary: "Potential duplicate side effect requires confirmation".to_string(),
            details: append_skill_permission_hints(
                json!({
                    "reason": escalation_reason,
                    "effect_status": "unknown",
                    "effect_type": effect.effect_type(),
                    "idempotency_key": effect.idempotency_key(),
                    "request_fingerprint": effect.request_fingerprint(),
                    "replay_tool_call": {
                        "call_id": tool_call.id,
                        "tool_name": tool_call.name,
                        "arguments": tool_arguments,
                    }
                }),
                state.machine.active_skills(),
            ),
            options: vec!["approve".to_string(), "reject".to_string()],
        };
        let request_id = state
            .agent_files()
            .write_confirmation_request(&pending)
            .await?;
        state.machine.record_tool_call_with_audit(
            &tool_call.name,
            tool_arguments.clone(),
            json!({
                "status": "escalation_required",
                "reason": escalation_reason,
                "idempotency_key": effect.idempotency_key(),
                "effect_status": "unknown",
                "request_id": request_id.clone()
            }),
            true,
            tool_audit.clone(),
        );
        state
            .machine
            .set_confirmation_for_request(request_id.clone(), pending.clone());
        super::ui_surfaces::paused(&state.agent_files()).await?;
        emit(Event::Yield {
            request_id,
            kind: alan_agent_protocol::YieldKind::Confirmation,
            payload: serde_json::to_value(confirmation_payload(
                pending.checkpoint_type.clone(),
                pending.summary.clone(),
                pending.details.clone(),
                pending.options.clone(),
            ))
            .unwrap_or_else(|_| json!({})),
        })
        .await;
        return Ok(ToolOrchestratorOutcome::PauseTurn);
    }

    emit(Event::ToolCallStarted {
        title: super::tool_presentation::tool_title(&tool_call.name, &tool_arguments),
        id: tool_call.id.clone(),
        name: tool_call.name.clone(),
        audit: tool_audit.clone(),
    })
    .await;

    if let Some(ToolEffectPlan::ReplayApplied {
        payload: replay_payload,
    }) = effect_plan
    {
        let dedupe_reason = "Matching applied side effect found; skipped physical execution";
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            success: Some(true),
            result_preview: tool_result_preview(&replay_payload),
            audit: tool_audit.clone(),
        })
        .await;
        state.machine.record_tool_call_with_audit(
            &tool_call.name,
            tool_arguments.clone(),
            replay_payload.clone(),
            true,
            tool_audit,
        );
        state
            .machine
            .add_tool_message(&tool_call.id, &tool_call.name, replay_payload.clone());
        effect_lifecycle
            .as_ref()
            .expect("replay plan requires a lifecycle")
            .commit_replay(&mut state.machine, &replay_payload, dedupe_reason);
        return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
            refresh_context: false,
        });
    }

    let effect_start = if let Some(effect) = effect_lifecycle.as_ref() {
        effect.record_execute_decision(&mut state.machine, "No applied effect record found");
        match effect.begin(&mut state.machine).await {
            Ok(record) => Some(record),
            Err(failure) => {
                emit(Event::Error {
                    message: failure.message.clone(),
                    recoverable: true,
                })
                .await;
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&failure.payload),
                    audit: tool_audit.clone(),
                })
                .await;
                state.machine.record_tool_call_with_audit(
                    &tool_call.name,
                    tool_arguments.clone(),
                    failure.payload.clone(),
                    false,
                    tool_audit,
                );
                state
                    .machine
                    .add_tool_message(&tool_call.id, &tool_call.name, failure.payload);
                return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                    refresh_context: false,
                });
            }
        }
    } else {
        None
    };

    let execution_target = state.tool_execution();
    let tool_start = Instant::now();
    let tool_timeout_secs = tool_package.timeout_secs;
    let tool_result = execute_tool_effect(
        execution_target,
        &tool_call.name,
        tool_arguments.clone(),
        inputs.cancel,
        tool_timeout_secs,
    )
    .await;
    if inputs.cancel.is_cancelled() && check_turn_cancelled(state, emit, inputs.cancel).await? {
        return Ok(ToolOrchestratorOutcome::EndTurn);
    }

    match tool_result {
        Ok(value) => {
            // `Ok` is only the transport result — the tool may report a logical
            // failure in its payload (e.g. bash `{ "success": false, "exit_code": 1
            // }`). Derive completion + effect status from that, so a failed
            // side-effecting command is not cached as `Applied` (which would make a
            // retry skip physical execution) and is not rendered as a success.
            let payload_success =
                value.get("success").and_then(serde_json::Value::as_bool) != Some(false);
            if let (Some(effect), Some(effect_start)) =
                (effect_lifecycle.as_ref(), effect_start.as_ref())
            {
                let reason =
                    (!payload_success).then(|| "tool reported failure in payload".to_string());
                effect.complete(
                    &mut state.machine,
                    effect_start,
                    &value,
                    payload_success,
                    reason,
                );
            }
            let tape_value = tool_payload_for_tape(state, &value).await;
            emit(Event::ToolCallCompleted {
                presentation: super::tool_presentation::tool_presentation(
                    &tool_call.name,
                    &tool_arguments,
                    &value,
                ),
                id: tool_call.id.clone(),
                name: Some(tool_call.name.clone()),
                success: Some(payload_success),
                result_preview: tool_result_preview(&value),
                audit: tool_audit.clone(),
            })
            .await;
            state.machine.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                value.clone(),
                payload_success,
                tool_audit.clone(),
            );
            state
                .machine
                .add_tool_message(&tool_call.id, &tool_call.name, tape_value);
            info!(
                tool_name = %tool_call.name,
                elapsed_ms = tool_start.elapsed().as_millis(),
                success = payload_success,
                "Tool done"
            );
            Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                refresh_context: false,
            })
        }
        Err(err) => {
            let error_payload = json!({"error": err.to_string()});
            if let (Some(effect), Some(effect_start)) =
                (effect_lifecycle.as_ref(), effect_start.as_ref())
            {
                effect.complete(
                    &mut state.machine,
                    effect_start,
                    &error_payload,
                    false,
                    Some(err.to_string()),
                );
            }
            emit(Event::ToolCallCompleted {
                presentation: None,
                id: tool_call.id.clone(),
                name: Some(tool_call.name.clone()),
                success: Some(false),
                result_preview: tool_result_preview(&error_payload),
                audit: tool_audit.clone(),
            })
            .await;
            state.machine.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                error_payload.clone(),
                false,
                tool_audit,
            );
            state
                .machine
                .add_tool_message(&tool_call.id, &tool_call.name, error_payload);
            info!(
                tool_name = %tool_call.name,
                elapsed_ms = tool_start.elapsed().as_millis(),
                success = false,
                error = %err,
                "Tool done"
            );
            Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                refresh_context: false,
            })
        }
    }
}

async fn orchestrate_tool_batch_with_guard<E, F>(
    state: &mut RuntimeLoopState,
    loop_guard: &mut ToolLoopGuard,
    tool_calls: &[NormalizedToolCall],
    inputs: ToolOrchestratorInputs<'_>,
    approved_unknown_effect_call_index: Option<usize>,
    approved_tool_escalation_call_index: Option<usize>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let mut refresh_context = false;

    for (idx, tool_call) in tool_calls.iter().enumerate() {
        let allow_approved_unknown_effect_execution =
            approved_unknown_effect_call_index.is_some_and(|approved_index| approved_index == idx);
        let allow_approved_tool_escalation_execution =
            approved_tool_escalation_call_index.is_some_and(|approved_index| approved_index == idx);
        match orchestrate_tool_call_with_guard(
            state,
            loop_guard,
            tool_call,
            inputs,
            allow_approved_unknown_effect_execution,
            allow_approved_tool_escalation_execution,
            emit,
        )
        .await?
        {
            ToolOrchestratorOutcome::ContinueToolBatch {
                refresh_context: call_refresh,
            } => {
                refresh_context |= call_refresh;
                if handle_queued_steering_inputs(
                    &mut state.machine,
                    tool_calls,
                    idx + 1,
                    inputs.steering_broker,
                    emit,
                )
                .await?
                {
                    return Ok(ToolBatchOrchestratorOutcome::ContinueTurnLoop {
                        refresh_context: true,
                    });
                }
            }
            ToolOrchestratorOutcome::PauseTurn => {
                if let Some(pending) = state.machine.pending_confirmation()
                    && replays_tool_calls(&pending.checkpoint_type)
                {
                    state
                        .machine
                        .set_tool_replay_batch(pending.checkpoint_id, tool_calls[idx..].to_vec());
                }
                return Ok(ToolBatchOrchestratorOutcome::PauseTurn);
            }
            ToolOrchestratorOutcome::EndTurn => {
                return Ok(ToolBatchOrchestratorOutcome::EndTurn {
                    surfaces_refreshed: false,
                });
            }
        }
    }

    if let Some(msg) = loop_guard.after_tool_batch() {
        emit(Event::Error {
            message: msg.clone(),
            recoverable: true,
        })
        .await;
        emit(Event::TextDelta {
            chunk: msg,
            is_final: true,
        })
        .await;
        super::memory_surfaces::refresh_active_turn_memory_surfaces_best_effort(
            state,
            "tool-loop-guard-ended-turn",
        )
        .await;
        emit(Event::TurnCompleted {
            summary: Some("Tool loop stopped by loop guard".to_string()),
        })
        .await;
        return Ok(ToolBatchOrchestratorOutcome::EndTurn {
            surfaces_refreshed: true,
        });
    }

    Ok(ToolBatchOrchestratorOutcome::ContinueTurnLoop { refresh_context })
}

#[cfg(test)]
mod tests;
