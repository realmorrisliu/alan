use alan_agent_protocol::{Event, InputMode, Op, Submission};
use anyhow::Result;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::ROLLBACK_NON_DURABLE_WARNING;
use crate::approval::{
    RUNTIME_CONFIRMATION_CONTROL_SOURCE, RUNTIME_CONFIRMATION_CONTROL_VERSION,
    is_effect_replay_confirmation, replays_tool_calls, runtime_confirmation_control_kind,
};
use crate::tape::ContentPart;

use super::agent_loop::{NormalizedToolCall, RuntimeLoopState};
use super::compaction::{CompactionRequest, maybe_compact_context_for_request};
use super::turn_executor::TurnRunKind;
use super::turn_state::PendingYield;
use super::turn_support::{cancel_current_task, tool_result_preview};

fn refresh_prompt_cache_host_capabilities(state: &mut RuntimeLoopState) {
    let path_dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    refresh_prompt_cache_host_capabilities_with_path_dirs(state, path_dirs);
}

fn refresh_prompt_cache_host_capabilities_with_path_dirs<I, P>(
    state: &mut RuntimeLoopState,
    path_dirs: I,
) where
    I: IntoIterator<Item = P>,
    P: AsRef<std::path::Path>,
{
    let delegated_supported = state.prompt_cache.supports_delegated_skill_invocation();
    let host_capabilities = crate::skills::build_skill_host_capabilities_with_path_dirs(
        state
            .static_tool_names()
            .into_iter()
            .chain(state.session.dynamic_tools.keys().cloned()),
        path_dirs,
        delegated_supported,
    );
    state.prompt_cache.set_host_capabilities(host_capabilities);
}

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
        Op::RegisterDynamicTools { tools } => {
            state.session.dynamic_tools = tools
                .iter()
                .cloned()
                .map(|tool| (tool.name.clone(), tool))
                .collect();
            refresh_prompt_cache_host_capabilities(state);
            emit(Event::TextDelta {
                chunk: format!(
                    "Registered {} dynamic tool(s).",
                    state.session.dynamic_tools.len()
                ),
                is_final: true,
            })
            .await;
        }
        Op::SetClientCapabilities { capabilities } => {
            state.session.client_capabilities = capabilities;
        }
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
            let rollback = state.session.rollback_last_turns(turns);
            state.turn_state.clear_plan_snapshot();
            emit(Event::SessionRolledBack {
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
            let workspace_id = context.as_ref().and_then(|c| c.workspace_id.clone());
            let reasoning_effort = context.as_ref().and_then(|c| c.reasoning_effort);

            if let Some(requested_workspace_id) = workspace_id.as_deref()
                && requested_workspace_id != state.workspace_id
            {
                emit(Event::Error {
                    message: format!(
                        "Turn requested workspace '{}' but this runtime is '{}'. Route the request to the matching workspace runtime.",
                        requested_workspace_id, state.workspace_id
                    ),
                    recoverable: true,
                })
                .await;
                return Ok(RuntimeOpAction::NoTurn);
            }

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
                emit(Event::Warning {
                    message: format!(
                        "Applied {queued_next_turn_count} queued next_turn input(s) to this turn."
                    ),
                })
                .await;
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
                        emit(Event::Warning {
                            message: "Queued follow_up input for execution after current turn."
                                .to_string(),
                        })
                        .await;
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
                            emit(Event::Warning {
                                message: format!(
                                    "Queued next_turn input (queue_size={size}); it will apply to the next explicit turn."
                                ),
                            })
                            .await;
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
                    let choice_str = choice.as_deref().unwrap_or("approve");
                    let modifications = result
                        .get("modifications")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    return handle_confirmation_resolution(
                        state,
                        pending,
                        choice_str,
                        modifications,
                    );
                }
                Some(PendingYield::StructuredInput(pending)) => {
                    state.session.add_tool_message(
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
                Some(PendingYield::DynamicToolCall(pending)) => {
                    let success = result
                        .get("success")
                        .and_then(|value| value.as_bool())
                        .unwrap_or_else(|| result.get("error").is_none());
                    emit(Event::ToolCallCompleted {
                        presentation: None,
                        id: pending.call_id.clone(),
                        name: Some(pending.tool_name.clone()),
                        success: Some(success),
                        result_preview: if success {
                            tool_result_preview(&result)
                        } else {
                            tool_result_preview(&serde_json::json!({
                                "error": result
                                    .get("error")
                                    .and_then(|value| value.as_str())
                                    .unwrap_or("dynamic tool failed")
                            }))
                        },
                        audit: None,
                    })
                    .await;
                    state
                        .session
                        .add_tool_message(&pending.call_id, &pending.tool_name, result);
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

fn handle_confirmation_resolution(
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

    if let Some(control_kind) = runtime_confirmation_control_kind(&pending.checkpoint_type) {
        payload["__alan_internal_control"] = json!({
            "kind": control_kind,
            "version": RUNTIME_CONFIRMATION_CONTROL_VERSION,
            "source": RUNTIME_CONFIRMATION_CONTROL_SOURCE
        });
        state
            .session
            .add_user_control_message_parts(vec![ContentPart::structured(payload)]);
    } else {
        state
            .session
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
    use std::sync::Arc;

    use super::*;
    use crate::{
        config::Config,
        runtime::{NamespaceRuntimeEnvironment, RuntimeConfig, RuntimeEnvironment, TurnState},
        session::Session,
        tape::ContentPart,
        tools::ToolRegistry,
    };
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace, ProcFs};
    use alan_shell::Shell;

    fn namespace_environment_for_test() -> RuntimeEnvironment {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        RuntimeEnvironment::namespace(NamespaceRuntimeEnvironment::new(
            root, "/agent/1", "default",
        ))
    }

    async fn namespace_environment_with_live_process_for_test() -> (RuntimeEnvironment, Shell) {
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
            RuntimeEnvironment::namespace(NamespaceRuntimeEnvironment::new(
                root, "/agent/1", "default",
            )),
            shell,
        )
    }

    fn create_test_state() -> RuntimeLoopState {
        let config = Config::default();
        let session = Session::new();
        let runtime_config = RuntimeConfig::default();

        RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            environment: namespace_environment_for_test(),
            tool_catalog: ToolRegistry::new(),
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        }
    }

    #[tokio::test]
    async fn test_handle_start_task_wrong_agent() {
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
                workspace_id: Some("wrong-workspace".to_string()),
                ..alan_agent_protocol::TurnContext::default()
            }),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                // Check that error event was emitted
                let has_error = events.iter().any(|e| matches!(e, Event::Error { .. }));
                assert!(has_error, "Expected Error event for wrong workspace");
            }
            _ => panic!("Expected NoTurn for wrong workspace"),
        }
    }

    #[tokio::test]
    async fn test_handle_start_task_correct_agent() {
        let mut state = create_test_state();
        state.session.add_user_message("existing message");
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![ContentPart::text("test input")],
            context: Some(alan_agent_protocol::TurnContext {
                workspace_id: Some("test-workspace".to_string()),
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
                assert_eq!(state.session.tape.messages().len(), 1);
                assert_eq!(
                    state.session.tape.messages()[0].text_content(),
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
    async fn test_handle_start_task_no_workspace_id() {
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
        let messages = state.session.tape.messages();
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
        let messages = state.session.tape.messages();
        assert!(!messages.is_empty());
        assert!(messages[0].text_content().contains("modify"));
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
        assert!(!state.session.tape.messages().is_empty());
    }

    #[tokio::test]
    async fn test_handle_register_dynamic_tools() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let tools = vec![
            alan_agent_protocol::DynamicToolSpec {
                name: "custom_tool1".to_string(),
                description: "Tool 1".to_string(),
                parameters: json!({}),
                capability: Some(alan_agent_protocol::ToolCapability::Read),
            },
            alan_agent_protocol::DynamicToolSpec {
                name: "custom_tool2".to_string(),
                description: "Tool 2".to_string(),
                parameters: json!({}),
                capability: None,
            },
        ];

        let op = Op::RegisterDynamicTools { tools };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                // Verify event was emitted
                let has_event = events.iter().any(|e| {
                    matches!(
                        e,
                        Event::TextDelta { chunk, is_final }
                            if *is_final && chunk.contains("Registered 2 dynamic tool(s).")
                    )
                });
                assert!(has_event);

                // Verify tools were registered
                assert!(state.session.dynamic_tools.contains_key("custom_tool1"));
                assert!(state.session.dynamic_tools.contains_key("custom_tool2"));
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_register_dynamic_tools_refreshes_prompt_cache_capabilities() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let skill_dir = workspace_root.join(".alan/agents/default/skills/dynamic-helper");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Dynamic Helper
description: Needs a dynamic tool
capabilities:
  required_tools: ["custom_tool1"]
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();

        let mut state = create_test_state();
        state.prompt_cache =
            crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
                crate::skills::ResolvedCapabilityView::from_package_dirs(vec![
                    crate::skills::ScopedPackageDir {
                        path: workspace_root.join(".alan/agents/default/skills"),
                        scope: crate::skills::SkillScope::Repo,
                    },
                ]),
                Vec::new(),
                crate::skills::SkillHostCapabilities::default().with_runtime_defaults(),
            );

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        let before = state
            .prompt_cache
            .build(Some(&[ContentPart::text("please use $dynamic-helper")]));
        assert!(
            before
                .system_prompt
                .contains("Skill '$dynamic-helper' is unavailable")
        );

        let op = Op::RegisterDynamicTools {
            tools: vec![alan_agent_protocol::DynamicToolSpec {
                name: "custom_tool1".to_string(),
                description: "Tool 1".to_string(),
                parameters: json!({}),
                capability: Some(alan_agent_protocol::ToolCapability::Read),
            }],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        let after = state
            .prompt_cache
            .build(Some(&[ContentPart::text("please use $dynamic-helper")]));
        assert!(after.system_prompt.contains("## Skill: Dynamic Helper"));
        assert!(
            !after
                .system_prompt
                .contains("Skill '$dynamic-helper' is unavailable")
        );
    }

    #[tokio::test]
    async fn test_handle_register_dynamic_tools_preserves_runtime_delegated_support() {
        let mut state = create_test_state();
        state.prompt_cache.set_host_capabilities(
            crate::skills::SkillHostCapabilities::default()
                .with_runtime_defaults()
                .with_delegated_skill_invocation(),
        );
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        let op = Op::RegisterDynamicTools {
            tools: vec![alan_agent_protocol::DynamicToolSpec {
                name: "custom_tool1".to_string(),
                description: "Tool 1".to_string(),
                parameters: json!({}),
                capability: Some(alan_agent_protocol::ToolCapability::Read),
            }],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(state.prompt_cache.supports_delegated_skill_invocation());
    }

    #[test]
    fn test_refresh_prompt_cache_host_capabilities_with_path_dirs_refreshes_host_path_executables()
    {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let skill_dir = workspace_root.join(".alan/agents/default/skills/jq-helper");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: JQ Helper
description: Needs jq
capabilities:
  required_tools: ["jq"]
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();

        let executable_path = {
            #[cfg(windows)]
            {
                temp.path().join("jq.cmd")
            }

            #[cfg(not(windows))]
            {
                temp.path().join("jq")
            }
        };
        std::fs::write(&executable_path, "echo jq\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = std::fs::metadata(&executable_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&executable_path, permissions).unwrap();
        }

        let mut state = create_test_state();
        state.prompt_cache =
            crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
                crate::skills::ResolvedCapabilityView::from_package_dirs(vec![
                    crate::skills::ScopedPackageDir {
                        path: workspace_root.join(".alan/agents/default/skills"),
                        scope: crate::skills::SkillScope::Repo,
                    },
                ]),
                Vec::new(),
                crate::skills::SkillHostCapabilities::default().with_runtime_defaults(),
            );

        let before = state
            .prompt_cache
            .build(Some(&[ContentPart::text("please use $jq-helper")]));
        assert!(
            before
                .system_prompt
                .contains("Skill '$jq-helper' is unavailable")
        );

        refresh_prompt_cache_host_capabilities_with_path_dirs(&mut state, [temp.path()]);

        let prompt = state
            .prompt_cache
            .build(Some(&[ContentPart::text("please use $jq-helper")]));
        assert!(prompt.system_prompt.contains("## Skill: JQ Helper"));
        assert!(
            !prompt
                .system_prompt
                .contains("Skill '$jq-helper' is unavailable")
        );
    }

    #[tokio::test]
    async fn test_handle_dynamic_tool_result_no_pending() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "call_123".to_string(),
            content: vec![ContentPart::structured(json!({
                "success": true,
                "result": {"data": "value"}
            }))],
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
    async fn test_handle_dynamic_tool_result_success() {
        let mut state = create_test_state();
        state
            .turn_state
            .set_dynamic_tool_call(crate::approval::PendingDynamicToolCall {
                call_id: "call_123".to_string(),
                tool_name: "custom_tool".to_string(),
                arguments: json!({"arg": "value"}),
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "call_123".to_string(),
            content: vec![ContentPart::structured(json!({
                "success": true,
                "result": {"data": "result"}
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

        let has_completed = events.iter().any(|event| {
            matches!(
                event,
                Event::ToolCallCompleted {
                    id,
                    result_preview: Some(_),
                    ..
                } if id == "call_123"
            )
        });
        assert!(
            has_completed,
            "Expected ToolCallCompleted after dynamic tool resume"
        );

        // Verify tool message was recorded
        assert!(!state.session.tape.messages().is_empty());
    }

    #[tokio::test]
    async fn test_handle_compact_without_focus() {
        let mut state = create_test_state();
        // Add some messages to make compaction meaningful
        for i in 0..10 {
            state.session.add_user_message(&format!("Message {}", i));
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
            state.session.add_user_message(&format!("Message {}", i));
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
        state.session.add_user_message("u1");
        state.session.add_assistant_message("a1", None);
        state.session.add_user_message("u2");
        state.session.add_assistant_message("a2", None);

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
                let has_session_rolled_back = events.iter().any(|e| {
                    matches!(
                        e,
                        Event::SessionRolledBack {
                            turns: 1,
                            removed_messages: 2,
                        }
                    )
                });
                assert!(has_session_rolled_back);
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
        state.session.add_user_message("u1");
        state.session.add_assistant_message("a1", None);

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
                        Event::SessionRolledBack {
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
        state.session.add_user_message("u1");
        state.session.add_assistant_message("a1", None);
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
        state.session.has_active_task = true;
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
                assert!(!state.session.has_active_task);
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
        state.session.has_active_task = true;
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
        state.session.has_active_task = true;
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
        assert!(!state.session.has_active_task);
    }

    #[tokio::test]
    async fn test_handle_interrupt_op_keeps_agent_process_running() {
        let (environment, shell) = namespace_environment_with_live_process_for_test().await;
        let mut state = create_test_state();
        state.environment = environment;
        state.session.has_active_task = true;
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result =
            handle_runtime_op_with_cancel(&mut state, Op::Interrupt, &mut emit, &cancel).await;

        assert!(result.is_ok());
        assert!(!state.session.has_active_task);
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

        let messages = state.session.tape.messages();
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

        let messages = state.session.tape.messages();
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

        let messages = state.session.tape.messages();
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
