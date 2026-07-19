use alan_agent_protocol::{Event, InputMode, Op, Submission};
use anyhow::Result;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::ROLLBACK_NON_DURABLE_WARNING;
use crate::approval::{
    MOUNT_ESCALATION_CHECKPOINT_TYPE, RUNTIME_CONFIRMATION_CONTROL_SOURCE,
    RUNTIME_CONFIRMATION_CONTROL_VERSION, is_effect_replay_confirmation, replays_tool_calls,
    runtime_confirmation_control_kind,
};
use crate::tape::ContentPart;

use super::compaction::{CompactionRequest, maybe_compact_context_for_request};
use super::transition::{
    ApprovedMountGrant, ApprovedMountGrantAccess, NamespaceMountApplication, RuntimeLoopState,
};
use super::turn_executor::TurnRunKind;
use super::turn_support::cancel_current_task;
use crate::agent_machine::{NormalizedToolCall, PendingYield};

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

pub(super) async fn handle_runtime_op_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    op: Op,
    emit: &mut E,
    _cancel: &CancellationToken,
) -> Result<RuntimeOpAction>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    match op {
        Op::CompactWithOptions { focus } => {
            maybe_compact_context_for_request(state, emit, CompactionRequest::manual(focus))
                .await?;
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
            let rollback = state.machine.rollback_last_turns(turns);
            state.machine.clear_plan_snapshot();
            super::ui_surfaces::plan_updated(state.namespace_environment(), None, Vec::new())
                .await?;
            super::ui_surfaces::rollback(state.namespace_environment(), rollback.removed_turns)
                .await?;
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
            super::ui_surfaces::warning(
                state.namespace_environment(),
                ROLLBACK_NON_DURABLE_WARNING,
            )
            .await?;
            emit(Event::Warning {
                message: ROLLBACK_NON_DURABLE_WARNING.to_string(),
            })
            .await;
        }
        Op::Interrupt => {
            cancel_current_task(state, emit).await?;
        }

        // ====================================================================
        // New unified operations (Phase 2)
        // ====================================================================
        Op::Turn { parts, context } => {
            let reasoning_effort = context.as_ref().and_then(|c| c.reasoning_effort);

            let queued_next_turn_inputs = state.machine.drain_next_turn_inputs();
            let queued_next_turn_count = queued_next_turn_inputs.len();
            let mut merged_parts = Vec::new();
            for queued_parts in queued_next_turn_inputs {
                merged_parts.extend(queued_parts);
            }
            merged_parts.extend(parts);

            state.machine.reset_turn();
            state.machine.set_active_turn_request_control_intent(
                crate::RequestControlIntent::reasoning_effort(reasoning_effort),
            );

            if queued_next_turn_count > 0 {
                let message = format!(
                    "Applied {queued_next_turn_count} queued next_turn input(s) to this turn."
                );
                super::ui_surfaces::warning(state.namespace_environment(), message.clone()).await?;
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
                    if !(state.machine.is_turn_active() || state.machine.has_pending_interaction())
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
                    if state.machine.is_turn_active() || state.machine.has_pending_interaction() {
                        // In normal runtime flow this path should be handled by in-band queueing in
                        // turn_driver. Keep this as a safe fallback.
                        state
                            .machine
                            .push_buffered_inband_submission(Submission::new(Op::Input {
                                parts,
                                mode: InputMode::FollowUp,
                            }));
                        let message =
                            "Queued follow_up input for execution after current turn.".to_string();
                        super::ui_surfaces::warning(state.namespace_environment(), message.clone())
                            .await?;
                        emit(Event::Warning { message }).await;
                        return Ok(RuntimeOpAction::NoTurn);
                    }

                    state.machine.reset_turn();
                    return Ok(RuntimeOpAction::RunTurn {
                        turn_kind: TurnRunKind::NewTurn,
                        user_input: Some(parts),
                        activate_task: true,
                    });
                }
                InputMode::NextTurn => {
                    let queued_size = state.machine.queue_next_turn_input(parts);
                    match queued_size {
                        Some(size) => {
                            let message = format!(
                                "Queued next_turn input (queue_size={size}); it will apply to the next explicit turn."
                            );
                            super::ui_surfaces::warning(
                                state.namespace_environment(),
                                message.clone(),
                            )
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
            let result = resume_content_to_value(&content);
            match state.machine.take_pending(&request_id) {
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
                        state,
                        pending,
                        choice_str,
                        modifications,
                    )
                    .await;
                }
                Some(PendingYield::StructuredInput(pending)) => {
                    state.machine.add_tool_message(
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

fn default_confirmation_choice(pending: &crate::approval::PendingConfirmation) -> &'static str {
    if pending.checkpoint_type == MOUNT_ESCALATION_CHECKPOINT_TYPE {
        "reject"
    } else {
        "approve"
    }
}

fn checkpoint_choice_for_rollout(choice_str: &str) -> &str {
    match choice_str {
        "approve" => "approved",
        "reject" => "rejected",
        _ => choice_str,
    }
}

async fn persist_runtime_confirmation_checkpoint(
    state: &RuntimeLoopState,
    pending: &crate::approval::PendingConfirmation,
    choice_str: &str,
) {
    let knowledge_root = match state
        .namespace_environment()
        .current_tape_checkpoint()
        .await
    {
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
    state
        .machine
        .record_checkpoint_with_optional_knowledge_root(
            &pending.checkpoint_id,
            &pending.checkpoint_type,
            &pending.summary,
            Some(checkpoint_choice_for_rollout(choice_str)),
            knowledge_root.as_deref(),
        );
}

async fn handle_confirmation_resolution(
    state: &mut RuntimeLoopState,
    pending: crate::approval::PendingConfirmation,
    choice_str: &str,
    modifications: Option<String>,
) -> Result<RuntimeOpAction> {
    let replay_tool_batch = if replays_tool_calls(&pending.checkpoint_type) {
        state.machine.take_tool_replay_batch(&pending.checkpoint_id)
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

    if pending.checkpoint_type == MOUNT_ESCALATION_CHECKPOINT_TYPE {
        return Ok(handle_mount_escalation_resolution(
            state, pending, choice_str,
        ));
    }

    if let Some(control_kind) = runtime_confirmation_control_kind(&pending.checkpoint_type) {
        payload["__alan_internal_control"] = json!({
            "kind": control_kind,
            "version": RUNTIME_CONFIRMATION_CONTROL_VERSION,
            "source": RUNTIME_CONFIRMATION_CONTROL_SOURCE
        });
        state
            .machine
            .add_user_control_message_parts(vec![ContentPart::structured(payload)]);
        persist_runtime_confirmation_checkpoint(state, &pending, choice_str).await;
    } else {
        state
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

fn handle_mount_escalation_resolution(
    state: &mut RuntimeLoopState,
    pending: crate::approval::PendingConfirmation,
    choice_str: &str,
) -> RuntimeOpAction {
    let Some((tool_call_id, mount_request)) = validated_mount_escalation_request(&pending) else {
        let result = json!({
            "status": "invalid_mount_escalation_checkpoint",
            "approved": false,
            "live_applied": false,
            "checkpoint_id": pending.checkpoint_id.clone(),
            "checkpoint_type": pending.checkpoint_type.clone(),
            "choice": choice_str,
            "error": "Invalid mount escalation checkpoint.",
        });
        state
            .machine
            .add_tool_message(&pending.checkpoint_id, "request_mount", result);
        return RuntimeOpAction::RunTurn {
            turn_kind: TurnRunKind::ResumeTurn,
            user_input: None,
            activate_task: false,
        };
    };
    let approved = choice_str == "approve";
    let grant = parse_mount_grant_request(&mount_request);
    let namespace_application = if approved {
        grant
            .as_ref()
            .map(|grant| {
                state
                    .environment
                    .apply_approved_mount_grant(&grant.approved_mount_grant())
            })
            .unwrap_or_else(|| {
                NamespaceMountApplication::unavailable("missing approved mount grant details")
            })
    } else {
        NamespaceMountApplication {
            namespace_applied: false,
            namespace_error: None,
        }
    };
    let native_grant = grant.as_ref().and_then(|grant| {
        crate::HostMountGrant::new(
            grant.namespace_path.clone(),
            grant.host_path.clone(),
            match grant.access {
                ApprovedMountGrantAccess::ReadOnly => alan_kernel::Access::ReadOnly,
                ApprovedMountGrantAccess::ReadWrite => alan_kernel::Access::ReadWrite,
            },
        )
        .ok()
    });
    let namespace_applied = approved && namespace_application.namespace_applied;
    let scratch_dir = state
        .namespace_environment()
        .tool_execution_binding()
        .map(|binding| binding.scratch_dir)
        .or_else(|| {
            state
                .runtime_config
                .store_bindings
                .as_ref()
                .map(|stores| stores.tmp.clone())
        });
    let tool_sandbox_projection_changed = namespace_applied
        && native_grant.as_ref().is_some_and(|grant| {
            state.environment.persist_approved_host_mount(grant.clone());
            scratch_dir
                .clone()
                .is_some_and(|scratch| state.environment.sync_tool_execution_binding(scratch))
        });
    let tool_sandbox_applied = namespace_applied
        && grant.as_ref().is_some_and(|grant| {
            let binding = state.namespace_environment().tool_execution_binding();
            binding.is_some_and(|binding| {
                binding.host_mounts.iter().any(|mounted| {
                    mounted.namespace_path == grant.namespace_path
                        && normalize_mount_grant_host_path(&mounted.host_path)
                            == normalize_mount_grant_host_path(&grant.host_path)
                })
            })
        });
    let status = if approved { "approved" } else { "rejected" };
    let result = json!({
        "status": status,
        "approved": approved,
        "live_applied": false,
        "namespace_applied": namespace_application.namespace_applied,
        "namespace_error": namespace_application.namespace_error,
        "tool_sandbox_applied": tool_sandbox_applied,
        "tool_sandbox_projection_changed": tool_sandbox_projection_changed,
        "checkpoint_id": pending.checkpoint_id.clone(),
        "checkpoint_type": pending.checkpoint_type.clone(),
        "choice": choice_str,
        "mount_request": mount_request,
    });

    if approved {
        state.machine.record_event(
            "host_mount_grant",
            host_mount_grant_event_payload(&pending.details, &result),
        );
    }
    state
        .machine
        .add_tool_message(&tool_call_id, "request_mount", result);

    RuntimeOpAction::RunTurn {
        turn_kind: TurnRunKind::ResumeTurn,
        user_input: None,
        activate_task: false,
    }
}

fn validated_mount_escalation_request(
    pending: &crate::approval::PendingConfirmation,
) -> Option<(String, serde_json::Value)> {
    if pending
        .details
        .get("kind")
        .and_then(serde_json::Value::as_str)
        != Some("mount_escalation")
    {
        return None;
    }
    if pending
        .details
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        != Some("request_mount")
    {
        return None;
    }
    let tool_call_id = pending
        .details
        .get("tool_call_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|raw| !raw.is_empty())?
        .to_string();
    let mount_request = pending.details.get("mount_request")?;
    let mount_request = super::virtual_tools::parse_mount_request(mount_request)
        .ok()?
        .payload();
    Some((tool_call_id, mount_request))
}

fn host_mount_grant_event_payload(
    details: &serde_json::Value,
    result: &serde_json::Value,
) -> serde_json::Value {
    let request = result
        .get("mount_request")
        .unwrap_or(&serde_json::Value::Null);
    json!({
        "namespace_path": request.get("namespace_path").and_then(serde_json::Value::as_str),
        "host_path": request.get("host_path").and_then(serde_json::Value::as_str),
        "access": request.get("access").and_then(serde_json::Value::as_str),
        "reason": request.get("reason").and_then(serde_json::Value::as_str),
        "checkpoint_id": result.get("checkpoint_id").and_then(serde_json::Value::as_str),
        "approved": true,
        "live_applied": false,
        "namespace_applied": result.get("namespace_applied").and_then(serde_json::Value::as_bool),
        "namespace_error": result.get("namespace_error").and_then(serde_json::Value::as_str),
        "tool_sandbox_applied": result.get("tool_sandbox_applied").and_then(serde_json::Value::as_bool),
        "tool_sandbox_projection_changed": result.get("tool_sandbox_projection_changed").and_then(serde_json::Value::as_bool),
        "tool_call_id": details.get("tool_call_id").and_then(serde_json::Value::as_str),
    })
}

struct MountGrantDetails {
    namespace_path: String,
    host_path: std::path::PathBuf,
    access: ApprovedMountGrantAccess,
    reason: String,
}

impl MountGrantDetails {
    fn approved_mount_grant(&self) -> ApprovedMountGrant {
        ApprovedMountGrant::new(
            self.namespace_path.clone(),
            self.host_path.clone(),
            self.access,
            self.reason.clone(),
        )
    }
}

fn parse_mount_grant_request(request: &serde_json::Value) -> Option<MountGrantDetails> {
    let namespace_path = request.get("namespace_path")?.as_str()?.trim();
    let host_path = request.get("host_path")?.as_str()?.trim();
    let access = request.get("access")?.as_str()?.trim();
    let reason = request.get("reason")?.as_str()?.trim();
    if namespace_path.is_empty() || host_path.is_empty() || reason.is_empty() {
        return None;
    }
    let access = match access {
        "read_only" => ApprovedMountGrantAccess::ReadOnly,
        "read_write" => ApprovedMountGrantAccess::ReadWrite,
        _ => return None,
    };
    Some(MountGrantDetails {
        namespace_path: namespace_path.to_string(),
        host_path: std::path::PathBuf::from(host_path),
        access,
        reason: reason.to_string(),
    })
}

fn normalize_mount_grant_host_path(path: &std::path::Path) -> std::path::PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| dunce::simplified(path).to_path_buf())
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
