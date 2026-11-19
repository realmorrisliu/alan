//! Concurrent execution of contiguous pure read-only Tool call runs within a batch.
//!
//! Only non-virtual, policy-allowed [`ToolCapability::Read`] calls without a task budget or
//! approved-replay override run in parallel. Execution overlaps across Tool Processes, while
//! AgentFS `actions/` publication, `Event` emission, and Machine tape recording happen strictly
//! in the original batch index order.

use std::time::{Duration, Instant};

use alan_agent_protocol::{Event, ToolCapability, ToolDecisionAudit};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use tokio::task::JoinSet;
use tracing::info;

use super::{
    NamespaceToolActionEvidence, NamespaceToolProcessError, NormalizedToolCall, RuntimeLoopState,
    namespace_environment::PreparedToolAction,
};
use crate::runtime::{
    loop_guard::ToolLoopGuard,
    tool_batch::ToolOrchestratorInputs,
    tool_execution::{namespace_tool_payload, tool_payload_for_tape},
    tool_policy::{SandboxConfinement, ToolPolicyDecision, evaluate_tool_policy_with_posture},
    tool_presentation::{tool_presentation, tool_title},
    turn_support::{check_turn_cancelled, tool_result_preview},
};

pub(super) enum ParallelReadBatchOutcome {
    Completed { count: usize },
    EndTurn,
}

struct AdmittedReadCall {
    tool_call: NormalizedToolCall,
    timeout_secs: usize,
    audit: ToolDecisionAudit,
}

fn is_virtual_tool(name: &str) -> bool {
    matches!(
        name,
        "request_confirmation"
            | "request_mount"
            | "request_user_input"
            | "update_plan"
            | "invoke_delegated_skill"
            | "spawn_subagent"
            | "terminate_child_run"
    )
}

fn record_allow_policy_decision(
    machine: &mut crate::agent_machine::AgentMachine,
    tool_call: &NormalizedToolCall,
    audit: &ToolDecisionAudit,
) {
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

fn tool_error_payload(error: &anyhow::Error) -> Value {
    let mut payload = json!({"success": false, "error": error.to_string()});
    if let Some(process_error) = error.downcast_ref::<NamespaceToolProcessError>() {
        payload["process"] = json!(format!("/proc/{}", process_error.pid));
        payload["effect_status"] = json!("unknown");
        payload["reason"] = json!(process_error.category.reason());
    }
    payload
}

/// If `tool_calls[start_idx..]` begins with at least two pure, policy-allowed read-only Tool
/// calls, execute that contiguous prefix concurrently and commit results in exact batch order.
/// Returns `Ok(None)` when fewer than two calls qualify so the caller uses the sequential path.
pub(super) async fn try_run_parallel_read_only_prefix<E, F>(
    loop_guard: &mut ToolLoopGuard,
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    start_idx: usize,
    approved_call_indices: (Option<usize>, Option<usize>),
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<Option<ParallelReadBatchOutcome>>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let slice = &tool_calls[start_idx..];
    if slice.len() < 2
        || inputs.explicit_command
        || inputs.cancel.is_cancelled()
        || state.machine.task_budget.is_some()
    {
        return Ok(None);
    }

    let (approved_unknown_idx, approved_escalation_idx) = approved_call_indices;
    let tool_execution = state.tool_execution();
    let packages = tool_execution.discover_packages().await?;
    let current_cwd = tool_execution.default_cwd();
    let confinement = SandboxConfinement::detect();

    let mut eligible = Vec::new();
    for (offset, call) in slice.iter().enumerate() {
        let abs_idx = start_idx + offset;
        if approved_unknown_idx == Some(abs_idx) || approved_escalation_idx == Some(abs_idx) {
            break;
        }
        if is_virtual_tool(&call.name) {
            break;
        }
        let Some(package) = packages.iter().find(|pkg| pkg.name == call.name) else {
            break;
        };
        let capability = tool_execution.resolve_capability(package, &call.arguments);
        if capability != ToolCapability::Read {
            break;
        }
        let decision = evaluate_tool_policy_with_posture(
            &state.runtime_config.policy_engine,
            &state.runtime_config.governance,
            state.runtime_config.trust_posture,
            &[],
            &call.name,
            &call.arguments,
            capability,
            current_cwd.as_deref(),
            confinement,
        );
        match decision {
            ToolPolicyDecision::Allow { audit } => eligible.push(AdmittedReadCall {
                tool_call: call.clone(),
                timeout_secs: package.timeout_secs,
                audit,
            }),
            ToolPolicyDecision::Escalate { .. } | ToolPolicyDecision::Forbidden { .. } => break,
        }
    }

    if eligible.len() < 2 {
        return Ok(None);
    }

    // Advance loop guard in deterministic order; stop before any call that trips repeat guard.
    let mut admitted = Vec::with_capacity(eligible.len());
    let mut guard_trip_message = None;
    for candidate in eligible {
        if let Some(msg) =
            loop_guard.before_tool_call(&candidate.tool_call.name, &candidate.tool_call.arguments)
        {
            guard_trip_message = Some(msg);
            break;
        }
        admitted.push(candidate);
    }

    if admitted.is_empty() {
        let msg = guard_trip_message.expect("empty admitted list implies guard tripped");
        crate::runtime::ui_surfaces::error_notice(&state.agent_files(), &msg).await?;
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
        return Ok(Some(ParallelReadBatchOutcome::EndTurn));
    }

    for call in &admitted {
        record_allow_policy_decision(&mut state.machine, &call.tool_call, &call.audit);
        emit(Event::ToolCallStarted {
            title: tool_title(&call.tool_call.name, &call.tool_call.arguments),
            id: call.tool_call.id.clone(),
            name: call.tool_call.name.clone(),
            audit: Some(call.audit.clone()),
        })
        .await;
    }

    // Execute all admitted read-only tool processes concurrently.
    let submission_id = state.machine.current_submission_id().map(str::to_string);
    let mut join_set = JoinSet::new();
    for (slot, call) in admitted.iter().enumerate() {
        let tools = tool_execution.clone();
        let tool_name = call.tool_call.name.clone();
        let call_id = call.tool_call.id.clone();
        let tool_arguments = call.tool_call.arguments.clone();
        let submission_id = submission_id.clone();
        let cancel = inputs.cancel.clone();
        let timeout_secs = call.timeout_secs;
        join_set.spawn(async move {
            let started = Instant::now();
            let executable = format!("/bin/{tool_name}");
            let result =
                match serde_json::to_string(&tool_arguments).context("serialize tool arguments") {
                    Ok(arguments_doc) => {
                        let evidence = NamespaceToolActionEvidence {
                            call_id: &call_id,
                            approval: "not_required",
                            arguments: &tool_arguments,
                            submission_id: submission_id.as_deref(),
                        };
                        tools
                            .execute_action_process(
                                &tool_name,
                                Some(evidence),
                                &executable,
                                [arguments_doc],
                                &cancel,
                                timeout_secs,
                            )
                            .await
                    }
                    Err(error) => Err(error),
                };
            (slot, started.elapsed(), result)
        });
    }

    let mut ordered_results: Vec<Option<(Duration, Result<PreparedToolAction>)>> =
        (0..admitted.len()).map(|_| None).collect();
    while let Some(joined) = join_set.join_next().await {
        let (slot, elapsed, result) = joined.context("parallel read-only tool task panicked")?;
        ordered_results[slot] = Some((elapsed, result));
    }

    let agent_files = state.agent_files();
    let host_mount_requests = state.environment.host_mount_requests();

    // Publish actions and commit results strictly in original batch index order.
    let mut published: Vec<(AdmittedReadCall, Duration, Result<Value>)> =
        Vec::with_capacity(admitted.len());
    for (call, slot_result) in admitted.into_iter().zip(ordered_results) {
        let (elapsed, raw_result) = slot_result.expect("every spawned slot produced a result");
        let tool_result = match raw_result {
            Ok(prepared) => tool_execution
                .publish_prepared_action(prepared)
                .await
                .and_then(namespace_tool_payload),
            Err(error) => Err(error),
        };
        published.push((call, elapsed, tool_result));
    }

    if inputs.cancel.is_cancelled() {
        for (call, _, tool_result) in &published {
            let payload = match tool_result {
                Ok(value) => Some(tool_payload_for_tape(&agent_files, value).await),
                Err(error) if error.downcast_ref::<NamespaceToolProcessError>().is_some() => {
                    Some(tool_error_payload(error))
                }
                Err(_) => None,
            };
            if let Some(payload) = payload {
                state
                    .machine
                    .add_tool_message(&call.tool_call.id, &call.tool_call.name, payload);
            }
        }
        if check_turn_cancelled(
            &mut state.machine,
            &agent_files,
            &host_mount_requests,
            emit,
            inputs.cancel,
        )
        .await?
        {
            return Ok(Some(ParallelReadBatchOutcome::EndTurn));
        }
    }

    let count = published.len();
    for (call, elapsed, tool_result) in published {
        let tool_call = &call.tool_call;
        match tool_result {
            Ok(value) => {
                let payload_success = value.get("success").and_then(Value::as_bool) != Some(false);
                let tape_value = tool_payload_for_tape(&agent_files, &value).await;
                emit(Event::ToolCallCompleted {
                    presentation: tool_presentation(&tool_call.name, &tool_call.arguments, &value),
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(payload_success),
                    result_preview: tool_result_preview(&value),
                    audit: Some(call.audit.clone()),
                })
                .await;
                state.machine.record_tool_call_with_audit(
                    &tool_call.name,
                    tool_call.arguments.clone(),
                    value,
                    payload_success,
                    Some(call.audit),
                );
                state
                    .machine
                    .add_tool_message(&tool_call.id, &tool_call.name, tape_value);
                info!(
                    tool_name = %tool_call.name,
                    elapsed_ms = elapsed.as_millis(),
                    success = payload_success,
                    parallel_read_batch = true,
                    "Tool done"
                );
            }
            Err(err) => {
                let error_payload = tool_error_payload(&err);
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&error_payload),
                    audit: Some(call.audit.clone()),
                })
                .await;
                state.machine.record_tool_call_with_audit(
                    &tool_call.name,
                    tool_call.arguments.clone(),
                    error_payload.clone(),
                    false,
                    Some(call.audit),
                );
                state
                    .machine
                    .add_tool_message(&tool_call.id, &tool_call.name, error_payload);
                info!(
                    tool_name = %tool_call.name,
                    elapsed_ms = elapsed.as_millis(),
                    success = false,
                    error = %err,
                    parallel_read_batch = true,
                    "Tool done"
                );
            }
        }
    }

    if let Some(msg) = guard_trip_message {
        crate::runtime::ui_surfaces::error_notice(&agent_files, &msg).await?;
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
        return Ok(Some(ParallelReadBatchOutcome::EndTurn));
    }

    Ok(Some(ParallelReadBatchOutcome::Completed { count }))
}

#[cfg(test)]
#[path = "parallel_read_tools_tests.rs"]
mod tests;
