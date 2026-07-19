use alan_agent_protocol::{Event, ToolCapability, ToolDecisionAudit};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    agent_machine::NormalizedToolCall,
    approval::{
        EFFECT_REPLAY_CHECKPOINT_PREFIX, EFFECT_REPLAY_CHECKPOINT_TYPE, PendingConfirmation,
        append_skill_permission_hints, runtime_confirmation_yield_payload,
    },
    evidence::{
        payload_needs_projection, project_evidence_payload, redact_evidence_payload,
        redaction_markers_in_text,
    },
};

use super::{
    tool_effect_lifecycle::{ToolEffectLifecycle, ToolEffectPlan},
    transition::{NamespaceAgentFiles, NamespaceToolActionOutput, NamespaceToolExecution},
    turn_support::{check_turn_cancelled, tool_result_preview},
};

mod runtime_inputs;

pub(super) use runtime_inputs::ToolExecutionRuntime;

pub(super) struct ToolExecutionRequest<'a> {
    pub(super) tool_call: &'a NormalizedToolCall,
    pub(super) tool_arguments: &'a Value,
    pub(super) tool_timeout_secs: usize,
    pub(super) tool_capability: ToolCapability,
    pub(super) tool_audit: Option<ToolDecisionAudit>,
    pub(super) allow_approved_unknown_effect_execution: bool,
    pub(super) cancel: &'a CancellationToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolExecutionOutcome {
    Completed,
    PauseTurn,
    EndTurn,
}

pub(super) async fn execute_allowed_tool_call<E, F>(
    runtime: ToolExecutionRuntime<'_>,
    request: ToolExecutionRequest<'_>,
    emit: &mut E,
) -> Result<ToolExecutionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let ToolExecutionRequest {
        tool_call,
        tool_arguments,
        tool_timeout_secs,
        tool_capability,
        tool_audit,
        allow_approved_unknown_effect_execution,
        cancel,
    } = request;
    let effect_lifecycle = ToolEffectLifecycle::for_call(
        runtime.machine,
        &runtime.process_path,
        tool_call.id.clone(),
        tool_call.name.clone(),
        tool_arguments,
        tool_capability,
    );
    let effect_plan = effect_lifecycle
        .as_ref()
        .map(|effect| effect.plan(runtime.machine, allow_approved_unknown_effect_execution));

    if matches!(effect_plan.as_ref(), Some(ToolEffectPlan::ConfirmUnknown)) {
        let effect = effect_lifecycle
            .as_ref()
            .expect("effect plan requires a lifecycle");
        let escalation_reason =
            "Previous side effect attempt has unknown status; explicit confirmation required";
        effect.record_unknown_confirmation(runtime.machine, escalation_reason);

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
                runtime.machine.active_skills(),
            ),
            options: vec!["approve".to_string(), "reject".to_string()],
        };
        let request_id = runtime
            .agent_files
            .write_confirmation_request(&pending)
            .await?;
        runtime.machine.record_tool_call_with_audit(
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
        return Ok(ToolExecutionOutcome::PauseTurn);
    }

    emit(Event::ToolCallStarted {
        title: super::tool_presentation::tool_title(&tool_call.name, tool_arguments),
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
        runtime.machine.record_tool_call_with_audit(
            &tool_call.name,
            tool_arguments.clone(),
            replay_payload.clone(),
            true,
            tool_audit,
        );
        runtime
            .machine
            .add_tool_message(&tool_call.id, &tool_call.name, replay_payload.clone());
        effect_lifecycle
            .as_ref()
            .expect("replay plan requires a lifecycle")
            .commit_replay(runtime.machine, &replay_payload, dedupe_reason);
        return Ok(ToolExecutionOutcome::Completed);
    }

    let effect_start = if let Some(effect) = effect_lifecycle.as_ref() {
        effect.record_execute_decision(runtime.machine, "No applied effect record found");
        match effect.begin(runtime.machine).await {
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
                runtime.machine.record_tool_call_with_audit(
                    &tool_call.name,
                    tool_arguments.clone(),
                    failure.payload.clone(),
                    false,
                    tool_audit,
                );
                runtime
                    .machine
                    .add_tool_message(&tool_call.id, &tool_call.name, failure.payload);
                return Ok(ToolExecutionOutcome::Completed);
            }
        }
    } else {
        None
    };

    let tool_start = Instant::now();
    let tool_result = execute_tool_effect(
        runtime.tool_execution,
        &tool_call.name,
        tool_arguments.clone(),
        cancel,
        tool_timeout_secs,
    )
    .await;
    if cancel.is_cancelled()
        && check_turn_cancelled(runtime.machine, &runtime.agent_files, emit, cancel).await?
    {
        return Ok(ToolExecutionOutcome::EndTurn);
    }

    match tool_result {
        Ok(value) => {
            // `Ok` is only the transport result — the Tool may report a logical
            // failure in its payload. Derive effect and completion status from it.
            let payload_success = value.get("success").and_then(Value::as_bool) != Some(false);
            if let (Some(effect), Some(effect_start)) =
                (effect_lifecycle.as_ref(), effect_start.as_ref())
            {
                let reason =
                    (!payload_success).then(|| "tool reported failure in payload".to_string());
                effect.complete(
                    runtime.machine,
                    effect_start,
                    &value,
                    payload_success,
                    reason,
                );
            }
            let tape_value = tool_payload_for_tape(&runtime.agent_files, &value).await;
            emit(Event::ToolCallCompleted {
                presentation: super::tool_presentation::tool_presentation(
                    &tool_call.name,
                    tool_arguments,
                    &value,
                ),
                id: tool_call.id.clone(),
                name: Some(tool_call.name.clone()),
                success: Some(payload_success),
                result_preview: tool_result_preview(&value),
                audit: tool_audit.clone(),
            })
            .await;
            runtime.machine.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                value.clone(),
                payload_success,
                tool_audit.clone(),
            );
            runtime
                .machine
                .add_tool_message(&tool_call.id, &tool_call.name, tape_value);
            info!(
                tool_name = %tool_call.name,
                elapsed_ms = tool_start.elapsed().as_millis(),
                success = payload_success,
                "Tool done"
            );
            Ok(ToolExecutionOutcome::Completed)
        }
        Err(err) => {
            let error_payload = json!({"error": err.to_string()});
            if let (Some(effect), Some(effect_start)) =
                (effect_lifecycle.as_ref(), effect_start.as_ref())
            {
                effect.complete(
                    runtime.machine,
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
            runtime.machine.record_tool_call_with_audit(
                &tool_call.name,
                tool_arguments.clone(),
                error_payload.clone(),
                false,
                tool_audit,
            );
            runtime
                .machine
                .add_tool_message(&tool_call.id, &tool_call.name, error_payload);
            info!(
                tool_name = %tool_call.name,
                elapsed_ms = tool_start.elapsed().as_millis(),
                success = false,
                error = %err,
                "Tool done"
            );
            Ok(ToolExecutionOutcome::Completed)
        }
    }
}

pub(super) fn namespace_tool_payload(tool: NamespaceToolActionOutput) -> Result<Value> {
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

pub(super) async fn tool_payload_for_tape(
    agent_files: &NamespaceAgentFiles,
    payload: &Value,
) -> Value {
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
    let reference = agent_files.action_output_reference(action_id).await;
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

pub(super) async fn execute_tool_effect(
    tools: NamespaceToolExecution,
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
