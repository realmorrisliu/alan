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

use super::agent_loop::{
    ApprovedMountGrant, ApprovedMountGrantAccess, NamespaceMountApplication, NormalizedToolCall,
    RuntimeLoopState,
};
use super::compaction::{CompactionRequest, maybe_compact_context_for_request};
use super::turn_executor::TurnRunKind;
use super::turn_state::PendingYield;
use super::turn_support::cancel_current_task;

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
            state.turn_state.clear_plan_snapshot();
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

            let queued_next_turn_inputs = state.turn_state.drain_next_turn_inputs();
            let queued_next_turn_count = queued_next_turn_inputs.len();
            let mut merged_parts = Vec::new();
            for queued_parts in queued_next_turn_inputs {
                merged_parts.extend(queued_parts);
            }
            merged_parts.extend(parts);

            state.turn_state.clear();
            state.turn_state.set_active_turn_request_control_intent(
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
                    if !(state.turn_state.is_turn_active()
                        || state.turn_state.has_pending_interaction())
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
                    if state.turn_state.is_turn_active()
                        || state.turn_state.has_pending_interaction()
                    {
                        // In normal runtime flow this path should be handled by in-band queueing in
                        // turn_driver. Keep this as a safe fallback.
                        state
                            .turn_state
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

                    state.turn_state.clear();
                    return Ok(RuntimeOpAction::RunTurn {
                        turn_kind: TurnRunKind::NewTurn,
                        user_input: Some(parts),
                        activate_task: true,
                    });
                }
                InputMode::NextTurn => {
                    let queued_size = state.turn_state.queue_next_turn_input(parts);
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
            match state.turn_state.take_pending(&request_id) {
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
        state
            .turn_state
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
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::{
        agent_machine::AgentMachine,
        config::Config,
        rollout::{RolloutItem, RolloutRecorder},
        runtime::{MountGrantApplicator, NamespaceRuntimeEnvironment, RuntimeConfig, TurnState},
        tape::ContentPart,
        tools::ToolRegistry,
    };
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace, ProcFs};
    use alan_shell::Shell;
    use tempfile::TempDir;

    #[derive(Debug, Default)]
    struct RecordingMountGrantApplicator {
        grants: Mutex<Vec<ApprovedMountGrant>>,
        fail_with: Option<&'static str>,
    }

    impl RecordingMountGrantApplicator {
        fn failing(message: &'static str) -> Self {
            Self {
                grants: Mutex::new(Vec::new()),
                fail_with: Some(message),
            }
        }

        fn grants(&self) -> Vec<ApprovedMountGrant> {
            self.grants.lock().unwrap().clone()
        }
    }

    impl MountGrantApplicator for RecordingMountGrantApplicator {
        fn apply_mount_grant(&self, grant: &ApprovedMountGrant) -> Result<Namespace> {
            self.grants.lock().unwrap().push(grant.clone());
            if let Some(message) = self.fail_with {
                anyhow::bail!(message);
            }
            let access = match grant.access {
                ApprovedMountGrantAccess::ReadOnly => Access::ReadOnly,
                ApprovedMountGrantAccess::ReadWrite => Access::ReadWrite,
            };
            let mut namespace = Namespace::new();
            namespace.mount(
                &grant.namespace_path,
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
                access,
            );
            Ok(namespace)
        }
    }

    fn namespace_environment_for_test() -> NamespaceRuntimeEnvironment {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        attach_test_process_context(NamespaceRuntimeEnvironment::new(
            root, "/agent/1", "default",
        ))
    }

    fn namespace_environment_with_mount_applicator_for_test(
        applicator: Arc<dyn MountGrantApplicator>,
    ) -> NamespaceRuntimeEnvironment {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        attach_test_process_context(
            NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
                .with_mount_grant_applicator(applicator),
        )
    }

    fn attach_test_process_context(
        environment: NamespaceRuntimeEnvironment,
    ) -> NamespaceRuntimeEnvironment {
        let runner = crate::tools::ToolProcessRunner::from_registry(&ToolRegistry::new());
        environment.with_tool_process_context(alan_kernel::Pid(1), runner)
    }

    async fn namespace_environment_with_live_process_for_test()
    -> (NamespaceRuntimeEnvironment, Shell) {
        let procfs = Arc::new(ProcFs::new());
        let agentfs = Arc::new(alan_agentfs::AgentFs::new());
        let mut namespace = Namespace::new();
        namespace.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let shell = Shell::new(root.clone());
        let pid = shell
            .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
            .await
            .unwrap();
        assert_eq!(pid, "1");
        (
            NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            shell,
        )
    }

    fn create_test_state() -> RuntimeLoopState {
        let config = Config::default();
        let machine = AgentMachine::new();
        let runtime_config = RuntimeConfig::default();

        RuntimeLoopState {
            machine,
            current_submission_id: None,
            environment: namespace_environment_for_test(),
            core_config: config,
            runtime_config,
            definition_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        }
    }

    fn bind_test_source_mount(state: &mut RuntimeLoopState, source: &std::path::Path) {
        let launch_context = crate::ProcessLaunchContext::new(
            Namespace::new(),
            alan_kernel::Credentials::user("test-agent"),
            "/mnt/source",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new("/mnt/source", source, alan_kernel::Access::ReadWrite)
                .unwrap(),
        );
        state.environment = state
            .environment
            .clone()
            .with_launch_context(launch_context.clone());
        assert!(
            state.namespace_environment().set_tool_execution_binding(
                crate::tools::ToolExecutionBinding::from_launch_context(
                    &launch_context,
                    source.join("scratch"),
                )
                .unwrap(),
            )
        );
    }

    fn mount_escalation_pending_confirmation_with(
        host_path: &str,
        access: &str,
        reason: &str,
    ) -> crate::approval::PendingConfirmation {
        crate::approval::PendingConfirmation {
            checkpoint_id: "mount_escalation_call_mount".to_string(),
            checkpoint_type: crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE.to_string(),
            summary: "Approve host mount?".to_string(),
            details: json!({
                "kind": "mount_escalation",
                "tool_call_id": "call_mount",
                "tool_name": "request_mount",
                "mount_request": {
                    "namespace_path": "/mnt/project",
                    "host_path": host_path,
                    "access": access,
                    "reason": reason
                },
                "live_applied": false
            }),
            options: vec!["approve".to_string(), "reject".to_string()],
        }
    }

    fn tool_result_text_for_call(state: &RuntimeLoopState, call_id: &str) -> String {
        state
            .machine
            .tape
            .messages()
            .iter()
            .rev()
            .find_map(|message| match message {
                crate::tape::Message::Tool { responses } => responses
                    .iter()
                    .rev()
                    .find(|response| response.id == call_id)
                    .map(crate::tape::ToolResponse::text_content),
                _ => None,
            })
            .expect("expected tool result")
    }

    fn tool_result_json_for_call(state: &RuntimeLoopState, call_id: &str) -> serde_json::Value {
        serde_json::from_str(&tool_result_text_for_call(state, call_id))
            .expect("tool result should be json")
    }

    #[tokio::test]
    async fn test_turn_context_has_no_process_identity_gate() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![ContentPart::text("test input")],
            context: Some(alan_agent_protocol::TurnContext {
                ..alan_agent_protocol::TurnContext::default()
            }),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        assert!(matches!(result.unwrap(), RuntimeOpAction::RunTurn { .. }));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Error { .. }))
        );
    }

    #[tokio::test]
    async fn test_handle_start_task_correct_agent() {
        let mut state = create_test_state();
        state.machine.add_user_message("existing message");
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![ContentPart::text("test input")],
            context: Some(alan_agent_protocol::TurnContext {
                reasoning_effort: Some(alan_agent_protocol::ReasoningEffort::High),
            }),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                user_input,
                activate_task,
                ..
            } => {
                assert!(activate_task);
                assert!(user_input.is_some());
                let text = alan_agent_protocol::parts_to_text(&user_input.unwrap());
                assert!(text.contains("test input"));
                // Turn should preserve existing conversation history.
                assert_eq!(state.machine.tape.messages().len(), 1);
                assert_eq!(
                    state.machine.tape.messages()[0].text_content(),
                    "existing message"
                );
                assert_eq!(
                    state
                        .turn_state
                        .active_turn_request_control_intent()
                        .reasoning_effort,
                    Some(alan_agent_protocol::ReasoningEffort::High)
                );
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_start_task_preserves_attachments_without_identity_field() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![
                ContentPart::text("test input"),
                ContentPart::Attachment {
                    hash: "doc1.pdf".to_string(),
                    mime_type: "application/pdf".to_string(),
                    metadata: serde_json::Value::Null,
                },
                ContentPart::Attachment {
                    hash: "doc2.pdf".to_string(),
                    mime_type: "application/pdf".to_string(),
                    metadata: serde_json::Value::Null,
                },
            ],
            context: Some(alan_agent_protocol::TurnContext::default()),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn { user_input, .. } => {
                let parts = user_input.unwrap();
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].as_text(), Some("test input"));
                assert!(matches!(parts[1], ContentPart::Attachment { .. }));
                assert!(matches!(parts[2], ContentPart::Attachment { .. }));
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_confirm_no_pending() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "chk_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(
                    |e| matches!(e, Event::Error { message, .. } if message.contains("does not match")),
                );
                assert!(has_error);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_confirm_wrong_checkpoint() {
        let mut state = create_test_state();
        state
            .turn_state
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "other_checkpoint".to_string(),
                checkpoint_type: "test".to_string(),
                summary: "Test".to_string(),
                details: json!({}),
                options: vec!["approve".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "chk_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(|e| {
                    matches!(e, Event::Error { message, .. } if message.contains("does not match"))
                });
                assert!(has_error);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_confirm_approve() {
        let mut state = create_test_state();
        state
            .turn_state
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "chk_123".to_string(),
                checkpoint_type: "test".to_string(),
                summary: "Test".to_string(),
                details: json!({
                    "replay_tool_call": {
                        "call_id": "call_1",
                        "tool_name": "read_file",
                        "arguments": {"path": "test.txt"}
                    }
                }),
                options: vec!["approve".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "chk_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        // Tool message should be recorded
        let messages = state.machine.tape.messages();
        assert!(!messages.is_empty());
        assert!(messages[0].text_content().contains("approve"));
    }

    #[tokio::test]
    async fn test_handle_confirm_with_modifications() {
        let mut state = create_test_state();
        state
            .turn_state
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "chk_123".to_string(),
                checkpoint_type: "test".to_string(),
                summary: "Test".to_string(),
                details: json!({}),
                options: vec!["approve".to_string(), "modify".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "chk_123".to_string(),
            content: vec![ContentPart::structured(json!({
                "choice": "modify",
                "modifications": "Changed something"
            }))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        // Tool message should contain modifications
        let messages = state.machine.tape.messages();
        assert!(!messages.is_empty());
        assert!(messages[0].text_content().contains("modify"));
    }

    #[tokio::test]
    async fn test_runtime_confirmation_resume_persists_checkpoint_with_knowledge_root() {
        let temp = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine = AgentMachine::new_with_recorder_in_dir(
            "runtime-confirmation-checkpoint-with-root",
            "test-model",
            temp.path(),
        )
        .await
        .unwrap();
        let (environment, _shell) = namespace_environment_with_live_process_for_test().await;
        state.environment = environment;
        state
            .namespace_environment()
            .write_user_state("seed confirmation context")
            .await
            .unwrap();
        let expected_root = state
            .namespace_environment()
            .current_tape_checkpoint()
            .await
            .unwrap();
        state
            .turn_state
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "tool_escalation_call_123".to_string(),
                checkpoint_type: crate::approval::TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary: "Approve tool escalation?".to_string(),
                details: json!({}),
                options: vec!["approve".to_string(), "reject".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "tool_escalation_call_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "reject"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let checkpoint = items
            .iter()
            .find_map(|item| match item {
                RolloutItem::Checkpoint(checkpoint)
                    if checkpoint.checkpoint_id == "tool_escalation_call_123" =>
                {
                    Some(checkpoint)
                }
                _ => None,
            })
            .expect("expected persisted runtime confirmation checkpoint");
        assert_eq!(
            checkpoint.checkpoint_type,
            crate::approval::TOOL_ESCALATION_CHECKPOINT_TYPE
        );
        assert_eq!(checkpoint.choice.as_deref(), Some("rejected"));
        assert_eq!(
            checkpoint.knowledge_root.as_deref(),
            Some(expected_root.as_str())
        );
    }

    #[tokio::test]
    async fn test_runtime_confirmation_resume_persists_checkpoint_without_knowledge_root_on_read_failure()
     {
        let temp = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine = AgentMachine::new_with_recorder_in_dir(
            "runtime-confirmation-checkpoint-no-root",
            "test-model",
            temp.path(),
        )
        .await
        .unwrap();
        let root = InProcessTransport::new(Arc::new(MountFs::new(Namespace::new())));
        state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        state
            .turn_state
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "tool_escalation_call_456".to_string(),
                checkpoint_type: crate::approval::TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary: "Approve tool escalation?".to_string(),
                details: json!({}),
                options: vec!["approve".to_string(), "reject".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "tool_escalation_call_456".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "reject"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let checkpoint = items
            .iter()
            .find_map(|item| match item {
                RolloutItem::Checkpoint(checkpoint)
                    if checkpoint.checkpoint_id == "tool_escalation_call_456" =>
                {
                    Some(checkpoint)
                }
                _ => None,
            })
            .expect("expected persisted runtime confirmation checkpoint");
        assert_eq!(checkpoint.choice.as_deref(), Some("rejected"));
        assert!(checkpoint.knowledge_root.is_none());
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_approve_records_grant_and_tool_result() {
        let temp = TempDir::new().unwrap();
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine =
            AgentMachine::new_with_recorder_in_dir("mount-approve", "test-model", temp.path())
                .await
                .unwrap();
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .turn_state
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_write",
                "Need to edit project files",
            ));
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "mount_escalation_call_mount".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["status"], "approved");
        assert_eq!(tool_result["tool_sandbox_applied"], false);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], false);
        assert_eq!(tool_result["namespace_applied"], false);
        assert_eq!(
            tool_result["namespace_error"],
            "live namespace mount applicator unavailable"
        );
        assert_eq!(tool_result["live_applied"], false);
        assert_eq!(
            tool_result["mount_request"]["namespace_path"],
            "/mnt/project"
        );
        let roots = state.namespace_environment().tool_sandbox_writable_roots();
        assert_eq!(
            roots,
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let grant = items
            .iter()
            .find_map(|item| match item {
                RolloutItem::Event(event) if event.event_type == "host_mount_grant" => Some(event),
                _ => None,
            })
            .expect("expected approved mount grant event");
        assert_eq!(grant.payload["namespace_path"], "/mnt/project");
        assert_eq!(
            grant.payload["host_path"],
            approved_host.path().to_str().unwrap()
        );
        assert_eq!(grant.payload["access"], "read_write");
        assert_eq!(grant.payload["reason"], "Need to edit project files");
        assert_eq!(
            grant.payload["checkpoint_id"],
            "mount_escalation_call_mount"
        );
        assert_eq!(grant.payload["approved"], true);
        assert_eq!(grant.payload["live_applied"], false);
        assert_eq!(grant.payload["namespace_applied"], false);
        assert_eq!(
            grant.payload["namespace_error"],
            "live namespace mount applicator unavailable"
        );
        assert_eq!(grant.payload["tool_sandbox_applied"], false);
        assert_eq!(grant.payload["tool_sandbox_projection_changed"], false);
        assert_eq!(grant.payload["tool_call_id"], "call_mount");
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_applies_namespace_with_applicator() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment =
            namespace_environment_with_mount_applicator_for_test(applicator.clone());
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .turn_state
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_write",
                "Need to edit project files",
            ));
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "mount_escalation_call_mount".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["namespace_applied"], true);
        assert_eq!(tool_result["namespace_error"], serde_json::Value::Null);
        assert_eq!(tool_result["tool_sandbox_applied"], true);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], true);
        assert_eq!(
            state.namespace_environment().tool_sandbox_writable_roots(),
            vec![
                dunce::canonicalize(host_mount_root.path()).unwrap(),
                dunce::canonicalize(approved_host.path()).unwrap()
            ]
        );
        let grants = applicator.grants();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].namespace_path, "/mnt/project");
        assert_eq!(grants[0].host_path, approved_host.path());
        assert_eq!(grants[0].access, ApprovedMountGrantAccess::ReadWrite);
        assert_eq!(grants[0].reason, "Need to edit project files");

        let child_context = state
            .namespace_environment()
            .launch_context()
            .expect("approved grant should persist in the Process Launch Context")
            .child();
        assert_eq!(
            child_context.host_path("/mnt/project/file.txt"),
            Some(approved_host.path().join("file.txt"))
        );
        assert!(
            child_context
                .namespace
                .describe()
                .iter()
                .any(|(path, access)| path == "/mnt/project" && *access == Access::ReadWrite)
        );
    }

    #[tokio::test]
    async fn test_first_approved_mount_creates_process_tool_binding() {
        let system_store = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment = namespace_environment_with_mount_applicator_for_test(applicator)
            .with_launch_context(crate::ProcessLaunchContext::root());
        state.runtime_config.store_bindings = Some(crate::AgentRuntimeStoreBindings {
            rollouts: system_store.path().join("rollouts"),
            checkpoints: system_store.path().join("checkpoints"),
            cache: system_store.path().join("cache"),
            tmp: system_store.path().join("tmp"),
            metadata: system_store.path().join("metadata"),
        });
        state
            .turn_state
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_write",
                "Need project files",
            ));
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "mount_escalation_call_mount".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        let binding = state
            .namespace_environment()
            .tool_execution_binding()
            .expect("first approved Host Mount should create a Tool binding");
        assert_eq!(
            binding.cwd,
            dunce::canonicalize(approved_host.path()).unwrap()
        );
        assert_eq!(binding.namespace_cwd, std::path::Path::new("/mnt/project"));
        assert_eq!(
            state
                .namespace_environment()
                .launch_context()
                .expect("the Process Launch Context should remain available")
                .cwd,
            "/"
        );
        assert_eq!(binding.host_mounts.len(), 1);
        assert_eq!(binding.host_mounts[0].namespace_path, "/mnt/project");
        assert_eq!(
            binding.host_mounts[0].resolve_host_path("/mnt/project/file.txt"),
            Some(approved_host.path().join("file.txt"))
        );
        let sandbox = binding.sandbox_spec.as_ref().unwrap();
        assert!(
            !sandbox
                .readable_roots
                .iter()
                .any(|root| root == &system_store.path().join("tmp"))
        );
        let execution = crate::tools::Sandbox::from_spec_with_backend(
            sandbox.clone(),
            crate::tools::SandboxBackendKind::HostMountPathGuard,
        )
        .exec("pwd", &binding.cwd)
        .await
        .unwrap();
        assert_eq!(execution.exit_code, 0, "{execution:?}");
        assert_eq!(
            execution.stdout.trim(),
            binding.cwd.to_string_lossy().as_ref()
        );
        let result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(result["tool_sandbox_applied"], true);
        assert_eq!(result["tool_sandbox_projection_changed"], true);
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_read_only_applies_namespace_only() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment =
            namespace_environment_with_mount_applicator_for_test(applicator.clone());
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .turn_state
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_only",
                "Need to inspect project files",
            ));
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "mount_escalation_call_mount".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["namespace_applied"], true);
        assert_eq!(tool_result["tool_sandbox_applied"], true);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], true);
        assert_eq!(
            state.namespace_environment().tool_sandbox_writable_roots(),
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );
        let grants = applicator.grants();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].access, ApprovedMountGrantAccess::ReadOnly);
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_reports_namespace_apply_failure() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::failing("mount failed"));
        let mut state = create_test_state();
        state.environment =
            namespace_environment_with_mount_applicator_for_test(applicator.clone());
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .turn_state
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_write",
                "Need to edit project files",
            ));
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "mount_escalation_call_mount".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["namespace_applied"], false);
        assert_eq!(tool_result["namespace_error"], "mount failed");
        assert_eq!(tool_result["tool_sandbox_applied"], false);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], false);
        assert_eq!(
            state.namespace_environment().tool_sandbox_writable_roots(),
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );
        let grants = applicator.grants();
        assert_eq!(grants.len(), 1);
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_duplicate_read_write_grant_is_idempotent() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment = namespace_environment_with_mount_applicator_for_test(applicator);
        bind_test_source_mount(&mut state, host_mount_root.path());
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        for _ in 0..2 {
            state
                .turn_state
                .set_confirmation(mount_escalation_pending_confirmation_with(
                    approved_host.path().to_str().unwrap(),
                    "read_write",
                    "Need to edit project files",
                ));
            handle_runtime_op_with_cancel(
                &mut state,
                Op::Resume {
                    request_id: "mount_escalation_call_mount".to_string(),
                    content: vec![ContentPart::structured(json!({"choice": "approve"}))],
                },
                &mut emit,
                &cancel,
            )
            .await
            .unwrap();
        }

        let roots = state.namespace_environment().tool_sandbox_writable_roots();
        assert_eq!(
            roots,
            vec![
                dunce::canonicalize(host_mount_root.path()).unwrap(),
                dunce::canonicalize(approved_host.path()).unwrap()
            ]
        );
        let latest = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(latest["tool_sandbox_applied"], true);
        assert_eq!(latest["tool_sandbox_projection_changed"], false);
    }

    #[tokio::test]
    async fn test_reapproved_namespace_path_replaces_persisted_host_grant() {
        let host_mount_root = TempDir::new().unwrap();
        let first_host = TempDir::new().unwrap();
        let replacement_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment = namespace_environment_with_mount_applicator_for_test(applicator);
        bind_test_source_mount(&mut state, host_mount_root.path());
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        for host in [first_host.path(), replacement_host.path()] {
            state
                .turn_state
                .set_confirmation(mount_escalation_pending_confirmation_with(
                    host.to_str().unwrap(),
                    "read_write",
                    "Replace project mount",
                ));
            handle_runtime_op_with_cancel(
                &mut state,
                Op::Resume {
                    request_id: "mount_escalation_call_mount".to_string(),
                    content: vec![ContentPart::structured(json!({"choice": "approve"}))],
                },
                &mut emit,
                &cancel,
            )
            .await
            .unwrap();
        }

        let launch_context = state
            .namespace_environment()
            .launch_context()
            .expect("approved grant should remain in the Process Launch Context");
        assert_eq!(
            launch_context.host_path("/mnt/project/file.txt"),
            Some(replacement_host.path().join("file.txt"))
        );
        assert_eq!(
            state.namespace_environment().tool_sandbox_writable_roots(),
            vec![
                dunce::canonicalize(host_mount_root.path()).unwrap(),
                dunce::canonicalize(replacement_host.path()).unwrap()
            ]
        );
        let latest = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(latest["tool_sandbox_applied"], true);
        assert_eq!(latest["tool_sandbox_projection_changed"], true);
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_read_only_grant_does_not_expand_tool_sandbox() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment = namespace_environment_with_mount_applicator_for_test(applicator);
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .turn_state
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_only",
                "Need to inspect project files",
            ));
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        let result = handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "mount_escalation_call_mount".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await;
        assert!(result.is_ok());

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["status"], "approved");
        assert_eq!(tool_result["tool_sandbox_applied"], true);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], true);
        assert_eq!(
            state.namespace_environment().tool_sandbox_writable_roots(),
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_reject_returns_tool_result_without_grant() {
        let temp = TempDir::new().unwrap();
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine =
            AgentMachine::new_with_recorder_in_dir("mount-reject", "test-model", temp.path())
                .await
                .unwrap();
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .turn_state
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_write",
                "Need to edit project files",
            ));
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "mount_escalation_call_mount".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "reject"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["status"], "rejected");
        assert_eq!(tool_result["approved"], false);
        assert_eq!(tool_result["tool_sandbox_applied"], false);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], false);
        assert_eq!(tool_result["live_applied"], false);
        assert_eq!(
            state.namespace_environment().tool_sandbox_writable_roots(),
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        assert!(!items.iter().any(|item| matches!(
            item,
            RolloutItem::Event(event) if event.event_type == "host_mount_grant"
        )));
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_missing_choice_defaults_to_reject() {
        let temp = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine = AgentMachine::new_with_recorder_in_dir(
            "mount-default-reject",
            "test-model",
            temp.path(),
        )
        .await
        .unwrap();
        let host_path = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        let host_path = host_path.display().to_string();
        state
            .turn_state
            .set_confirmation(mount_escalation_pending_confirmation_with(
                &host_path,
                "read_write",
                "Need to edit project files",
            ));
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "mount_escalation_call_mount".to_string(),
            content: vec![ContentPart::structured(json!({}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        let tool_result = tool_result_text_for_call(&state, "call_mount");
        assert!(tool_result.contains("\"status\":\"rejected\""));
        assert!(tool_result.contains("\"choice\":\"reject\""));
        assert!(tool_result.contains("\"approved\":false"));

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        assert!(!items.iter().any(|item| matches!(
            item,
            RolloutItem::Event(event) if event.event_type == "host_mount_grant"
        )));
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_rejects_forged_checkpoint() {
        let temp = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine =
            AgentMachine::new_with_recorder_in_dir("mount-forged", "test-model", temp.path())
                .await
                .unwrap();
        state
            .turn_state
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "forged_mount".to_string(),
                checkpoint_type: crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary: "Approve forged mount?".to_string(),
                details: json!({
                    "kind": "mount_escalation",
                    "tool_call_id": "call_mount",
                    "tool_name": "request_confirmation",
                    "mount_request": {
                        "namespace_path": "/mnt/project",
                        "host_path": "relative/path",
                        "access": "read_write",
                        "reason": "forged"
                    },
                    "live_applied": false
                }),
                options: vec!["approve".to_string(), "reject".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "forged_mount".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        let tool_result = tool_result_text_for_call(&state, "forged_mount");
        assert!(tool_result.contains("\"status\":\"invalid_mount_escalation_checkpoint\""));
        assert!(tool_result.contains("\"approved\":false"));

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        assert!(!items.iter().any(|item| matches!(
            item,
            RolloutItem::Event(event) if event.event_type == "host_mount_grant"
        )));
    }

    #[tokio::test]
    async fn test_handle_user_input() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Input {
            parts: vec![ContentPart::text("Hello world")],
            mode: InputMode::Steer,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(|e| {
                    matches!(e, Event::Error { message, .. } if message.contains("Use Op::Turn"))
                });
                assert!(
                    has_error,
                    "Expected guidance error for Input without active turn"
                );
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_structured_user_input_no_pending() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "req_123".to_string(),
            content: vec![ContentPart::structured(json!({"answers": []}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(
                    |e| matches!(e, Event::Error { message, .. } if message.contains("does not match")),
                );
                assert!(has_error);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_structured_user_input_wrong_id() {
        let mut state = create_test_state();
        state
            .turn_state
            .set_structured_input(crate::approval::PendingStructuredInputRequest {
                request_id: "other_id".to_string(),
                title: "Test".to_string(),
                prompt: "Test".to_string(),
                questions: vec![],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "req_123".to_string(),
            content: vec![ContentPart::structured(json!({"answers": []}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(|e| {
                    matches!(e, Event::Error { message, .. } if message.contains("does not match"))
                });
                assert!(has_error);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_structured_user_input_success() {
        let mut state = create_test_state();
        state
            .turn_state
            .set_structured_input(crate::approval::PendingStructuredInputRequest {
                request_id: "req_123".to_string(),
                title: "Test".to_string(),
                prompt: "Test".to_string(),
                questions: vec![],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "req_123".to_string(),
            content: vec![ContentPart::structured(json!({
                "answers": [{"question_id": "q1", "value": "answer1"}]
            }))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                user_input,
                activate_task,
                turn_kind,
            } => {
                assert!(!activate_task);
                assert!(user_input.is_none());
                assert!(matches!(turn_kind, TurnRunKind::ResumeTurn));
            }
            _ => panic!("Expected RunTurn"),
        }

        // Verify tool message was recorded
        assert!(!state.machine.tape.messages().is_empty());
    }

    #[tokio::test]
    async fn test_handle_compact_without_focus() {
        let mut state = create_test_state();
        // Add some messages to make compaction meaningful
        for i in 0..10 {
            state.machine.add_user_message(&format!("Message {}", i));
        }
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::CompactWithOptions { focus: None };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                // Compaction completed
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_compact_with_options() {
        let mut state = create_test_state();
        for i in 0..10 {
            state.machine.add_user_message(&format!("Message {}", i));
        }
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::CompactWithOptions {
            focus: Some("preserve todos".to_string()),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), RuntimeOpAction::NoTurn));
    }

    #[tokio::test]
    async fn test_handle_rollback_invalid_zero() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Rollback { turns: 0 };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(|e| {
                    matches!(e, Event::Error { message, .. } if message.contains("turns must be >= 1"))
                });
                assert!(has_error);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_rollback_success() {
        let mut state = create_test_state();
        state.machine.add_user_message("u1");
        state.machine.add_assistant_message("a1", None);
        state.machine.add_user_message("u2");
        state.machine.add_assistant_message("a2", None);

        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Rollback { turns: 1 };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_machine_rolled_back = events.iter().any(|e| {
                    matches!(
                        e,
                        Event::MachineRolledBack {
                            turns: 1,
                            removed_messages: 2,
                        }
                    )
                });
                assert!(has_machine_rolled_back);
                let has_confirmation = events.iter().any(
                    |e| matches!(
                        e,
                        Event::TextDelta { chunk, is_final }
                            if *is_final && chunk.contains("Rolled back 1 turn(s), removed 2 message(s).")
                    ),
                );
                assert!(has_confirmation);
                let has_warning = events.iter().any(|e| {
                    matches!(
                        e,
                        Event::Warning { message }
                            if message == ROLLBACK_NON_DURABLE_WARNING
                    )
                });
                assert!(has_warning);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_rollback_reports_actual_removed_turns_when_history_is_shorter() {
        let mut state = create_test_state();
        state.machine.add_user_message("u1");
        state.machine.add_assistant_message("a1", None);

        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Rollback { turns: 10 };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                assert!(events.iter().any(|e| {
                    matches!(
                        e,
                        Event::MachineRolledBack {
                            turns: 1,
                            removed_messages: 2,
                        }
                    )
                }));
                assert!(events.iter().any(|e| matches!(
                    e,
                    Event::TextDelta { chunk, is_final }
                        if *is_final
                            && chunk.contains("Rolled back 1 turn(s) out of requested 10 turn(s), removed 2 message(s).")
                )));
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_rollback_clears_plan_snapshot() {
        let mut state = create_test_state();
        state.machine.add_user_message("u1");
        state.machine.add_assistant_message("a1", None);
        state.turn_state.set_plan_snapshot(
            Some("Stale plan".to_string()),
            vec![alan_agent_protocol::PlanItem {
                id: "plan-1".to_string(),
                content: "This should be cleared on rollback".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        );

        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};

        let op = Op::Rollback { turns: 1 };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), RuntimeOpAction::NoTurn));
        assert!(state.turn_state.plan_snapshot().is_none());
    }

    #[tokio::test]
    async fn test_handle_cancel() {
        let mut state = create_test_state();
        let (environment, _shell) = namespace_environment_with_live_process_for_test().await;
        state.environment = environment;
        state.machine.has_active_task = true;
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Interrupt;

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                // Task should be cancelled
                assert!(!state.machine.has_active_task);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    // Tests for parse_replay_tool_call_from_confirmation_details
    #[test]
    fn test_parse_replay_tool_call_valid() {
        let details = json!({
            "replay_tool_call": {
                "call_id": "call_123",
                "tool_name": "read_file",
                "arguments": {"path": "test.txt"}
            }
        });

        let result = parse_replay_tool_call_from_confirmation_details(&details);
        assert!(result.is_some());

        let call = result.unwrap();
        assert_eq!(call.id, "call_123");
        assert_eq!(call.name, "read_file");
        assert_eq!(call.arguments, json!({"path": "test.txt"}));
    }

    #[test]
    fn test_parse_replay_tool_call_missing_replay() {
        let details = json!({
            "other_field": "value"
        });

        assert!(parse_replay_tool_call_from_confirmation_details(&details).is_none());
    }

    #[test]
    fn test_parse_replay_tool_call_empty_call_id() {
        let details = json!({
            "replay_tool_call": {
                "call_id": "  ",
                "tool_name": "read_file",
                "arguments": {}
            }
        });

        assert!(parse_replay_tool_call_from_confirmation_details(&details).is_none());
    }

    #[test]
    fn test_parse_replay_tool_call_empty_tool_name() {
        let details = json!({
            "replay_tool_call": {
                "call_id": "call_123",
                "tool_name": "",
                "arguments": {}
            }
        });

        assert!(parse_replay_tool_call_from_confirmation_details(&details).is_none());
    }

    #[test]
    fn test_parse_replay_tool_call_missing_arguments() {
        let details = json!({
            "replay_tool_call": {
                "call_id": "call_123",
                "tool_name": "read_file"
            }
        });

        assert!(parse_replay_tool_call_from_confirmation_details(&details).is_none());
    }

    // ========================================================================
    // Tests for new Phase 2 Op variants
    // ========================================================================

    #[tokio::test]
    async fn test_handle_turn_op() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![ContentPart::text("Hello from Turn")],
            context: None,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                assert!(matches!(turn_kind, TurnRunKind::NewTurn));
                let text = alan_agent_protocol::parts_to_text(&user_input.unwrap());
                assert!(text.contains("Hello from Turn"));
                assert!(activate_task);
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_input_op() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Input {
            parts: vec![ContentPart::text("follow up")],
            mode: InputMode::Steer,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(|e| {
                    matches!(e, Event::Error { message, .. } if message.contains("Use Op::Turn"))
                });
                assert!(
                    has_error,
                    "Expected guidance error for Input without active turn"
                );
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_follow_up_without_active_turn_starts_new_turn() {
        let mut state = create_test_state();
        state.turn_state.set_active_turn_request_control_intent(
            crate::RequestControlIntent::reasoning_effort(Some(
                alan_agent_protocol::ReasoningEffort::High,
            )),
        );
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Input {
            parts: vec![ContentPart::text("run after current")],
            mode: InputMode::FollowUp,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                assert!(matches!(turn_kind, TurnRunKind::NewTurn));
                assert_eq!(
                    user_input,
                    Some(vec![ContentPart::text("run after current")])
                );
                assert!(activate_task);
                assert!(
                    state
                        .turn_state
                        .active_turn_request_control_intent()
                        .is_empty()
                );
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_next_turn_is_queue_only_and_applies_on_next_turn() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let queue_op = Op::Input {
            parts: vec![ContentPart::text("context for next turn")],
            mode: InputMode::NextTurn,
        };
        let queue_result =
            handle_runtime_op_with_cancel(&mut state, queue_op, &mut emit, &cancel).await;
        assert!(queue_result.is_ok());
        assert!(matches!(queue_result.unwrap(), RuntimeOpAction::NoTurn));
        assert_eq!(state.turn_state.queued_next_turn_input_count(), 1);

        let turn_op = Op::Turn {
            parts: vec![ContentPart::text("explicit turn")],
            context: None,
        };
        let turn_result = handle_runtime_op_with_cancel(&mut state, turn_op, &mut emit, &cancel)
            .await
            .unwrap();

        match turn_result {
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                assert!(matches!(turn_kind, TurnRunKind::NewTurn));
                assert!(activate_task);
                let merged_text = alan_agent_protocol::parts_to_text(&user_input.unwrap());
                assert!(merged_text.contains("context for next turn"));
                assert!(merged_text.contains("explicit turn"));
            }
            _ => panic!("Expected RunTurn"),
        }
        assert_eq!(state.turn_state.queued_next_turn_input_count(), 0);
    }

    #[tokio::test]
    async fn test_handle_next_turn_overflow_emits_recoverable_error() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        for _ in 0..16 {
            let result = handle_runtime_op_with_cancel(
                &mut state,
                Op::Input {
                    parts: vec![ContentPart::text("queued")],
                    mode: InputMode::NextTurn,
                },
                &mut emit,
                &cancel,
            )
            .await
            .unwrap();
            assert!(matches!(result, RuntimeOpAction::NoTurn));
        }

        let overflow_result = handle_runtime_op_with_cancel(
            &mut state,
            Op::Input {
                parts: vec![ContentPart::text("overflow")],
                mode: InputMode::NextTurn,
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();
        assert!(matches!(overflow_result, RuntimeOpAction::NoTurn));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Error { message, recoverable }
                if *recoverable && message.contains("Too many queued next_turn inputs")
        )));
    }

    #[tokio::test]
    async fn test_handle_input_op_during_active_turn_uses_resume_turn() {
        let mut state = create_test_state();
        state
            .turn_state
            .set_turn_activity(crate::runtime::turn_state::TurnActivityState::Running);
        state.machine.has_active_task = true;
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Input {
            parts: vec![ContentPart::text("steer current turn")],
            mode: InputMode::Steer,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                assert!(matches!(turn_kind, TurnRunKind::ResumeTurn));
                assert_eq!(
                    user_input,
                    Some(vec![ContentPart::text("steer current turn")])
                );
                assert!(!activate_task);
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_interrupt_op() {
        let mut state = create_test_state();
        let (environment, _shell) = namespace_environment_with_live_process_for_test().await;
        state.environment = environment;
        state.machine.has_active_task = true;
        state
            .turn_state
            .set_turn_activity(crate::runtime::turn_state::TurnActivityState::Running);
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Interrupt;

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(!state.machine.has_active_task);
    }

    #[tokio::test]
    async fn test_handle_interrupt_op_keeps_agent_process_running() {
        let (environment, shell) = namespace_environment_with_live_process_for_test().await;
        let mut state = create_test_state();
        state.environment = environment;
        state.machine.has_active_task = true;
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result =
            handle_runtime_op_with_cancel(&mut state, Op::Interrupt, &mut emit, &cancel).await;

        assert!(result.is_ok());
        assert!(!state.machine.has_active_task);
        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/status").await.unwrap()).unwrap(),
            "running\n"
        );
        let agent_events = String::from_utf8(shell.cat("/agent/1/events").await.unwrap()).unwrap();
        assert!(
            !agent_events.contains("ctl:"),
            "generic interrupt must not be routed through machine/ctl: {agent_events:?}"
        );
    }

    #[tokio::test]
    async fn test_handle_resume_no_pending_yields_error() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "nonexistent".to_string(),
            content: vec![ContentPart::structured(
                serde_json::json!({"choice": "approve"}),
            )],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), RuntimeOpAction::NoTurn));

        // Should have emitted an error event
        let has_error = events.iter().any(
            |e| matches!(e, Event::Error { message, .. } if message.contains("does not match")),
        );
        assert!(has_error);
    }

    #[tokio::test]
    async fn test_handle_resume_with_pending_confirmation() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.turn_state.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp-1".to_string(),
            checkpoint_type: "review".to_string(),
            summary: "Review this".to_string(),
            details: json!({}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "cp-1".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn { turn_kind, .. } => {
                assert!(matches!(turn_kind, TurnRunKind::ResumeTurn));
            }
            _ => panic!("Expected RunTurn with ResumeTurn"),
        }
    }

    #[tokio::test]
    async fn test_tool_escalation_resume_records_structured_trace_message() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.turn_state.set_confirmation(PendingConfirmation {
            checkpoint_id: "tool_escalation_tool_123".to_string(),
            checkpoint_type: "tool_escalation".to_string(),
            summary: "Approve?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "tool_escalation_tool_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "reject"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        let messages = state.machine.tape.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_user());
        match messages[0].parts().first() {
            Some(ContentPart::Structured { data }) => {
                assert_eq!(
                    data.get("__alan_internal_control")
                        .and_then(|marker| marker.get("kind"))
                        .and_then(serde_json::Value::as_str),
                    Some("tool_escalation_confirmation")
                );
            }
            _ => panic!("expected structured control message"),
        }
    }

    #[tokio::test]
    async fn test_effect_replay_resume_records_structured_trace_message() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.turn_state.set_confirmation(PendingConfirmation {
            checkpoint_id: "effect_replay_call-123".to_string(),
            checkpoint_type: "effect_replay_confirmation".to_string(),
            summary: "Replay side effect?".to_string(),
            details: json!({"effect_status":"unknown"}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "effect_replay_call-123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "reject"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        let messages = state.machine.tape.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_user());
        match messages[0].parts().first() {
            Some(ContentPart::Structured { data }) => {
                assert_eq!(
                    data.get("__alan_internal_control")
                        .and_then(|marker| marker.get("kind"))
                        .and_then(serde_json::Value::as_str),
                    Some("effect_replay_confirmation")
                );
            }
            _ => panic!("expected structured control message"),
        }
    }

    #[tokio::test]
    async fn test_non_tool_escalation_resume_still_records_tool_message() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.turn_state.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp-1".to_string(),
            checkpoint_type: "review".to_string(),
            summary: "Review?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "cp-1".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        let messages = state.machine.tape.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_tool());
        assert_eq!(messages[0].tool_responses()[0].id, "cp-1");
    }

    #[tokio::test]
    async fn test_tool_escalation_replay_batch_does_not_bypass_unknown_without_unknown_marker() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.turn_state.set_confirmation(PendingConfirmation {
            checkpoint_id: "tool_escalation_call-1".to_string(),
            checkpoint_type: "tool_escalation".to_string(),
            summary: "Approve policy escalation".to_string(),
            details: json!({
                "reason": "policy requires approval",
                "replay_tool_call": {
                    "call_id": "call-1",
                    "tool_name": "write_file",
                    "arguments": {"path":"notes.txt","payload":"hello"}
                }
            }),
            options: vec!["approve".to_string(), "reject".to_string()],
        });
        state.turn_state.set_tool_replay_batch(
            "tool_escalation_call-1",
            vec![NormalizedToolCall {
                id: "call-1".to_string(),
                name: "write_file".to_string(),
                arguments: json!({"path":"notes.txt","payload":"hello"}),
            }],
        );

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "tool_escalation_call-1".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        match result {
            RuntimeOpAction::ReplayApprovedToolBatch {
                approved_unknown_effect_call_id,
                approved_tool_escalation_call_id,
                ..
            } => {
                assert_eq!(approved_unknown_effect_call_id, None);
                assert_eq!(approved_tool_escalation_call_id.as_deref(), Some("call-1"));
            }
            _ => panic!("expected replay batch action"),
        }
    }

    #[tokio::test]
    async fn test_effect_replay_confirmation_marks_unknown_bypass_for_unknown_effect() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.turn_state.set_confirmation(PendingConfirmation {
            checkpoint_id: "effect_replay_call-1".to_string(),
            checkpoint_type: "effect_replay_confirmation".to_string(),
            summary: "Approve unknown-effect replay".to_string(),
            details: json!({
                "effect_status": "unknown",
                "replay_tool_call": {
                    "call_id": "call-1",
                    "tool_name": "write_file",
                    "arguments": {"path":"notes.txt","payload":"hello"}
                }
            }),
            options: vec!["approve".to_string(), "reject".to_string()],
        });
        state.turn_state.set_tool_replay_batch(
            "effect_replay_call-1",
            vec![NormalizedToolCall {
                id: "call-1".to_string(),
                name: "write_file".to_string(),
                arguments: json!({"path":"notes.txt","payload":"hello"}),
            }],
        );

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "effect_replay_call-1".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        match result {
            RuntimeOpAction::ReplayApprovedToolBatch {
                approved_unknown_effect_call_id,
                approved_tool_escalation_call_id,
                ..
            } => {
                assert_eq!(approved_unknown_effect_call_id.as_deref(), Some("call-1"));
                assert_eq!(approved_tool_escalation_call_id, None);
            }
            _ => panic!("expected replay batch action"),
        }
    }
}
