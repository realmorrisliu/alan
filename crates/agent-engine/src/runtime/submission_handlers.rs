use alan_agent_protocol::{Event, InputMode, Op, Submission};
use anyhow::Result;
use serde_json::json;

use crate::ROLLBACK_NON_DURABLE_WARNING;
use crate::approval::{
    RUNTIME_CONFIRMATION_CONTROL_SOURCE, RUNTIME_CONFIRMATION_CONTROL_VERSION,
    is_effect_replay_confirmation, replays_tool_calls, runtime_confirmation_control_kind,
};
use crate::tape::ContentPart;

use super::transition::{HostMountTerminalResult, NamespaceAgentFiles, TurnRunKind};
use super::turn_support::cancel_current_task;
use crate::agent_machine::{
    AgentMachine, HOST_MOUNT_REQUEST_TERMINAL_EVENT_TYPE, NormalizedToolCall,
    PendingHostMountRequest, PendingYield,
};

mod runtime_inputs;

pub(crate) use runtime_inputs::SubmissionRuntime;

#[derive(Debug, Clone)]
pub(super) enum RuntimeOpAction {
    NoTurn,
    RunTurn {
        turn_kind: TurnRunKind,
        user_input: Option<Vec<ContentPart>>,
        activate_task: bool,
    },
    ReplayApprovedToolCall {
        tool_call: NormalizedToolCall,
        approved_unknown_effect_call_id: Option<String>,
        approved_tool_escalation_call_id: Option<String>,
    },
    ReplayApprovedToolBatch {
        tool_calls: Vec<NormalizedToolCall>,
        approved_unknown_effect_call_id: Option<String>,
        approved_tool_escalation_call_id: Option<String>,
    },
}

pub(super) async fn handle_non_compaction_runtime_op<E, F>(
    mut runtime: SubmissionRuntime<'_>,
    op: Op,
    emit: &mut E,
) -> Result<RuntimeOpAction>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    match op {
        Op::CompactWithOptions { .. } => {
            anyhow::bail!("manual compaction must enter through the accepted-submission transition")
        }
        Op::Rollback { turns } => {
            if turns == 0 {
                emit(Event::Error {
                    message: "turns must be >= 1".to_string(),
                    recoverable: true,
                })
                .await;
                return Ok(RuntimeOpAction::NoTurn);
            }
            let rollback = runtime.machine.rollback_last_turns(turns);
            runtime.machine.clear_plan_snapshot();
            super::ui_surfaces::plan_updated(&runtime.agent_files, None, Vec::new()).await?;
            super::ui_surfaces::rollback(&runtime.agent_files, rollback.removed_turns).await?;
            emit(Event::MachineRolledBack {
                turns: rollback.removed_turns,
                removed_messages: rollback.removed_messages,
            })
            .await;
            let confirmation = if rollback.removed_turns == turns {
                format!(
                    "Rolled back {} turn(s), removed {} message(s).",
                    rollback.removed_turns, rollback.removed_messages
                )
            } else {
                format!(
                    "Rolled back {} turn(s) out of requested {} turn(s), removed {} message(s).",
                    rollback.removed_turns, turns, rollback.removed_messages
                )
            };
            emit(Event::TextDelta {
                chunk: confirmation,
                is_final: true,
            })
            .await;
            super::ui_surfaces::warning(&runtime.agent_files, ROLLBACK_NON_DURABLE_WARNING).await?;
            emit(Event::Warning {
                message: ROLLBACK_NON_DURABLE_WARNING.to_string(),
            })
            .await;
        }
        Op::Interrupt => {
            cancel_current_task(runtime.machine, &runtime.agent_files, emit).await?;
        }

        // ====================================================================
        // New unified operations (Phase 2)
        // ====================================================================
        Op::Turn { parts, context } => {
            let reasoning_effort = context.as_ref().and_then(|c| c.reasoning_effort);

            let queued_next_turn_inputs = runtime.machine.drain_next_turn_inputs();
            let queued_next_turn_count = queued_next_turn_inputs.len();
            let mut merged_parts = Vec::new();
            for queued_parts in queued_next_turn_inputs {
                merged_parts.extend(queued_parts);
            }
            merged_parts.extend(parts);

            runtime.machine.reset_turn();
            runtime.machine.set_active_turn_request_control_intent(
                crate::RequestControlIntent::reasoning_effort(reasoning_effort),
            );

            if queued_next_turn_count > 0 {
                let message = format!(
                    "Applied {queued_next_turn_count} queued next_turn input(s) to this turn."
                );
                super::ui_surfaces::warning(&runtime.agent_files, message.clone()).await?;
                emit(Event::Warning { message }).await;
            }

            return Ok(RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::NewTurn,
                user_input: Some(merged_parts),
                activate_task: true,
            });
        }

        Op::Input { parts, mode } => {
            match mode {
                InputMode::Steer => {
                    if !(runtime.machine.is_turn_active()
                        || runtime.machine.has_pending_interaction())
                    {
                        emit(Event::Error {
                            message: "Input(mode=steer) requires an active or pending turn. Use Op::Turn to start a new turn.".to_string(),
                            recoverable: true,
                        })
                        .await;
                        return Ok(RuntimeOpAction::NoTurn);
                    }

                    return Ok(RuntimeOpAction::RunTurn {
                        turn_kind: TurnRunKind::ResumeTurn,
                        user_input: Some(parts),
                        activate_task: false,
                    });
                }
                InputMode::FollowUp => {
                    if runtime.machine.is_turn_active() || runtime.machine.has_pending_interaction()
                    {
                        // In normal runtime flow this path should be handled by in-band queueing in
                        // accepted-submission transition. Keep this as a safe fallback.
                        runtime
                            .machine
                            .push_buffered_inband_submission(Submission::new(Op::Input {
                                parts,
                                mode: InputMode::FollowUp,
                            }));
                        let message =
                            "Queued follow_up input for execution after current turn.".to_string();
                        super::ui_surfaces::warning(&runtime.agent_files, message.clone()).await?;
                        emit(Event::Warning { message }).await;
                        return Ok(RuntimeOpAction::NoTurn);
                    }

                    runtime.machine.reset_turn();
                    return Ok(RuntimeOpAction::RunTurn {
                        turn_kind: TurnRunKind::NewTurn,
                        user_input: Some(parts),
                        activate_task: true,
                    });
                }
                InputMode::NextTurn => {
                    let queued_size = runtime.machine.queue_next_turn_input(parts);
                    match queued_size {
                        Some(size) => {
                            let message = format!(
                                "Queued next_turn input (queue_size={size}); it will apply to the next explicit turn."
                            );
                            super::ui_surfaces::warning(&runtime.agent_files, message.clone())
                                .await?;
                            emit(Event::Warning { message }).await;
                        }
                        None => {
                            emit(Event::Error {
                                message: "Too many queued next_turn inputs (limit=16); dropping newest input."
                                    .to_string(),
                                recoverable: true,
                            })
                            .await;
                        }
                    }
                    return Ok(RuntimeOpAction::NoTurn);
                }
            }
        }

        Op::Resume {
            request_id,
            content,
        } => {
            if let Some(pending) = runtime.machine.pending_host_mount(&request_id) {
                let Some(terminal) = runtime
                    .host_mount_requests
                    .terminal_result(&request_id)
                    .await?
                else {
                    emit(Event::Error {
                        message: format!(
                            "Host Mount request '{request_id}' is still pending; only Host Mount Service can settle it."
                        ),
                        recoverable: true,
                    })
                    .await;
                    return Ok(RuntimeOpAction::NoTurn);
                };
                let taken = runtime.machine.take_pending(&request_id);
                debug_assert!(matches!(taken, Some(PendingYield::HostMount(_))));
                return Ok(handle_host_mount_terminal(&mut runtime, pending, terminal));
            }
            let result = resume_content_to_value(&content);
            match runtime.machine.take_pending(&request_id) {
                Some(PendingYield::Confirmation(pending)) => {
                    let choice = result
                        .get("choice")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string)
                        .or_else(|| first_resume_text(&content));
                    let choice_str = choice
                        .as_deref()
                        .unwrap_or_else(|| default_confirmation_choice(&pending));
                    let modifications = result
                        .get("modifications")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    return handle_confirmation_resolution(
                        &mut runtime,
                        pending,
                        choice_str,
                        modifications,
                    )
                    .await;
                }
                Some(PendingYield::StructuredInput(pending)) => {
                    runtime.machine.add_tool_message(
                        &pending.request_id,
                        "request_user_input",
                        result,
                    );
                    return Ok(RuntimeOpAction::RunTurn {
                        turn_kind: TurnRunKind::ResumeTurn,
                        user_input: None,
                        activate_task: false,
                    });
                }
                Some(PendingYield::HostMount(_)) => unreachable!(
                    "Host Mount pending requests are resolved from service status above"
                ),
                None => {
                    emit(Event::Error {
                        message: format!(
                            "Resume request_id '{}' does not match any pending yield.",
                            request_id
                        ),
                        recoverable: true,
                    })
                    .await;
                    return Ok(RuntimeOpAction::NoTurn);
                }
            }
        }
    }
    Ok(RuntimeOpAction::NoTurn)
}

fn resume_content_to_value(content: &[ContentPart]) -> serde_json::Value {
    match content {
        [] => serde_json::Value::Null,
        [single] => match single {
            ContentPart::Structured { data } => data.clone(),
            ContentPart::Text { text } | ContentPart::Thinking { text, .. } => {
                serde_json::Value::String(text.clone())
            }
            other => serde_json::to_value(other).unwrap_or(serde_json::Value::Null),
        },
        _ => serde_json::Value::Array(
            content
                .iter()
                .map(|part| match part {
                    ContentPart::Structured { data } => data.clone(),
                    ContentPart::Text { text } | ContentPart::Thinking { text, .. } => {
                        serde_json::Value::String(text.clone())
                    }
                    other => serde_json::to_value(other).unwrap_or(serde_json::Value::Null),
                })
                .collect(),
        ),
    }
}

fn first_resume_text(content: &[ContentPart]) -> Option<String> {
    content.iter().find_map(|part| match part {
        ContentPart::Text { text } | ContentPart::Thinking { text, .. } => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    })
}

fn default_confirmation_choice(_pending: &crate::approval::PendingConfirmation) -> &'static str {
    "approve"
}

fn checkpoint_choice_for_rollout(choice_str: &str) -> &str {
    match choice_str {
        "approve" => "approved",
        "reject" => "rejected",
        _ => choice_str,
    }
}

async fn persist_runtime_confirmation_checkpoint(
    machine: &mut AgentMachine,
    agent_files: &NamespaceAgentFiles,
    pending: &crate::approval::PendingConfirmation,
    choice_str: &str,
) {
    let knowledge_root = match agent_files.current_tape_checkpoint().await {
        Ok(root) => {
            let trimmed = root.trim();
            if trimmed.is_empty() { None } else { Some(root) }
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                checkpoint_id = %pending.checkpoint_id,
                checkpoint_type = %pending.checkpoint_type,
                "Failed to read namespace tape checkpoint for rollout persistence"
            );
            None
        }
    };
    machine.record_checkpoint_with_optional_knowledge_root(
        &pending.checkpoint_id,
        &pending.checkpoint_type,
        &pending.summary,
        Some(checkpoint_choice_for_rollout(choice_str)),
        knowledge_root.as_deref(),
    );
}

async fn handle_confirmation_resolution(
    runtime: &mut SubmissionRuntime<'_>,
    pending: crate::approval::PendingConfirmation,
    choice_str: &str,
    modifications: Option<String>,
) -> Result<RuntimeOpAction> {
    let replay_tool_batch = if replays_tool_calls(&pending.checkpoint_type) {
        runtime
            .machine
            .take_tool_replay_batch(&pending.checkpoint_id)
    } else {
        None
    };

    let mut payload = json!({
        "checkpoint_id": pending.checkpoint_id,
        "checkpoint_type": pending.checkpoint_type.clone(),
        "choice": choice_str,
    });

    if let Some(modifications) = modifications {
        payload["modifications"] = serde_json::Value::String(modifications);
    }

    if let Some(control_kind) = runtime_confirmation_control_kind(&pending.checkpoint_type) {
        payload["__alan_internal_control"] = json!({
            "kind": control_kind,
            "version": RUNTIME_CONFIRMATION_CONTROL_VERSION,
            "source": RUNTIME_CONFIRMATION_CONTROL_SOURCE
        });
        runtime
            .machine
            .add_user_control_message_parts(vec![ContentPart::structured(payload)]);
        persist_runtime_confirmation_checkpoint(
            runtime.machine,
            &runtime.agent_files,
            &pending,
            choice_str,
        )
        .await;
    } else {
        runtime
            .machine
            .add_tool_message(&pending.checkpoint_id, "request_confirmation", payload);
    }

    let allow_unknown_effect_replay = is_effect_replay_confirmation(&pending.checkpoint_type)
        && is_unknown_effect_confirmation(&pending);
    let allow_tool_escalation_replay =
        pending.checkpoint_type == crate::approval::TOOL_ESCALATION_CHECKPOINT_TYPE;

    if replays_tool_calls(&pending.checkpoint_type)
        && choice_str == "approve"
        && let Some(tool_calls) = replay_tool_batch
    {
        return Ok(RuntimeOpAction::ReplayApprovedToolBatch {
            approved_unknown_effect_call_id: if allow_unknown_effect_replay {
                tool_calls.first().map(|call| call.id.clone())
            } else {
                None
            },
            approved_tool_escalation_call_id: if allow_tool_escalation_replay {
                tool_calls.first().map(|call| call.id.clone())
            } else {
                None
            },
            tool_calls,
        });
    }
    if replays_tool_calls(&pending.checkpoint_type)
        && choice_str == "approve"
        && let Some(tool_call) = parse_replay_tool_call_from_confirmation_details(&pending.details)
    {
        return Ok(RuntimeOpAction::ReplayApprovedToolCall {
            approved_unknown_effect_call_id: if allow_unknown_effect_replay {
                Some(tool_call.id.clone())
            } else {
                None
            },
            approved_tool_escalation_call_id: if allow_tool_escalation_replay {
                Some(tool_call.id.clone())
            } else {
                None
            },
            tool_call,
        });
    }
    Ok(RuntimeOpAction::RunTurn {
        turn_kind: TurnRunKind::ResumeTurn,
        user_input: None,
        activate_task: false,
    })
}

fn handle_host_mount_terminal(
    runtime: &mut SubmissionRuntime<'_>,
    pending: PendingHostMountRequest,
    terminal: HostMountTerminalResult,
) -> RuntimeOpAction {
    let status = terminal.status.as_str();
    let approved = status == "approved";
    let result = json!({
        "status": status,
        "approved": approved,
        "request_reference": pending.request_id,
        "namespace_path": pending.namespace_path,
        "access": pending.access,
        "reason": pending.reason,
        "label": pending.label,
        "grant_reference": terminal.grant_reference,
        "error": terminal.error,
    });
    runtime.machine.record_event(
        HOST_MOUNT_REQUEST_TERMINAL_EVENT_TYPE,
        json!({
            "request_id": pending.request_id,
            "status": status,
            "grant_reference": terminal.grant_reference,
            "error": terminal.error,
        }),
    );
    runtime
        .machine
        .add_tool_message(&pending.tool_call_id, "request_mount", result);

    RuntimeOpAction::RunTurn {
        turn_kind: TurnRunKind::ResumeTurn,
        user_input: None,
        activate_task: false,
    }
}

fn is_unknown_effect_confirmation(pending: &crate::approval::PendingConfirmation) -> bool {
    pending
        .details
        .get("effect_status")
        .and_then(serde_json::Value::as_str)
        == Some("unknown")
}

fn parse_replay_tool_call_from_confirmation_details(
    details: &serde_json::Value,
) -> Option<NormalizedToolCall> {
    let replay = details.get("replay_tool_call")?;
    let call_id = replay.get("call_id")?.as_str()?.trim();
    let tool_name = replay.get("tool_name")?.as_str()?.trim();
    let arguments = replay.get("arguments")?.clone();

    if call_id.is_empty() || tool_name.is_empty() {
        return None;
    }

    Some(NormalizedToolCall {
        id: call_id.to_string(),
        name: tool_name.to_string(),
        arguments,
    })
}

#[cfg(test)]
#[path = "submission_handlers/tests.rs"]
mod tests;
