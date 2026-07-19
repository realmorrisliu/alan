use alan_agent_protocol::Event;
use anyhow::Result;
#[cfg(test)]
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::approval::{
    PendingConfirmation, TOOL_ESCALATION_CHECKPOINT_PREFIX, TOOL_ESCALATION_CHECKPOINT_TYPE,
    append_skill_permission_hints, replays_tool_calls, runtime_confirmation_yield_payload,
};

use super::loop_guard::ToolLoopGuard;
use super::steering_queue::handle_queued_steering_inputs;
#[cfg(test)]
use super::tool_effect_lifecycle::{EffectCategory, build_effect_identity};
use super::tool_execution::{
    ToolExecutionOutcome, ToolExecutionRequest, execute_allowed_tool_call,
};
#[cfg(test)]
use super::tool_execution::{execute_tool_effect, tool_payload_for_tape};
use super::tool_policy::{ToolPolicyDecision, evaluate_tool_policy};
use super::transition::RuntimeLoopState;
use super::turn_driver::TurnInputBroker;
use super::turn_support::tool_result_preview;
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
                    payload: serde_json::to_value(runtime_confirmation_yield_payload(&pending))
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

    let runtime = super::transition::tool_execution_runtime(state);
    let request = ToolExecutionRequest {
        tool_call,
        tool_arguments: &tool_arguments,
        tool_timeout_secs: tool_package.timeout_secs,
        tool_capability,
        tool_audit,
        allow_approved_unknown_effect_execution,
        cancel: inputs.cancel,
    };
    match execute_allowed_tool_call(runtime, request, emit).await? {
        ToolExecutionOutcome::Completed => Ok(ToolOrchestratorOutcome::ContinueToolBatch {
            refresh_context: false,
        }),
        ToolExecutionOutcome::PauseTurn => Ok(ToolOrchestratorOutcome::PauseTurn),
        ToolExecutionOutcome::EndTurn => Ok(ToolOrchestratorOutcome::EndTurn),
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
        let memory_dir = state
            .core_config
            .memory
            .enabled
            .then_some(state.core_config.memory.store_dir.as_deref())
            .flatten();
        let process_path = state.process_path();
        super::memory_surfaces::refresh_active_turn_memory_surfaces_best_effort(
            &state.machine,
            memory_dir,
            &process_path,
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
