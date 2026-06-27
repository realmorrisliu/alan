use alan_agent_protocol::{CompactionOutcome, Event};
use anyhow::Result;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::llm::{build_generation_request, project_tool_response_for_prompt};

use super::agent_loop::{DeferredRuntimeAction, RuntimeLoopState, generate_with_retry_with_cancel};
use super::compaction::{CompactionRequest, maybe_compact_context_with_cancel};
use super::response_guardrails::{
    AssistantDraft, GuardrailDecision, ResponseGuardrailContext, ResponseGuardrails,
};
use super::tool_orchestrator::{
    ToolBatchOrchestratorOutcome, ToolOrchestratorInputs, ToolTurnOrchestrator,
};
use super::turn_driver::TurnInputBroker;
use super::turn_support::{
    check_turn_cancelled, detect_provider, emit_streaming_chunks, emit_task_completed_success,
    emit_thinking_chunks, normalize_tool_calls,
};
use super::virtual_tools::virtual_tool_definitions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnRunKind {
    NewTurn,
    ResumeTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnExecutionOutcome {
    Finished,
    Paused,
}

const STREAM_RECOVERY_OUTPUT_SNIPPET_MAX_CHARS: usize = 2000;
const COMPACTION_TIMEOUT_SECS: u64 = 30;

#[derive(Default)]
struct StreamedToolCallBuffer {
    id: Option<String>,
    name: Option<String>,
    arguments_delta: String,
    final_arguments: Option<String>,
}

fn truncate_for_stream_recovery(text: &str) -> String {
    let truncated: String = text
        .chars()
        .take(STREAM_RECOVERY_OUTPUT_SNIPPET_MAX_CHARS)
        .collect();
    if truncated.chars().count() == text.chars().count() {
        truncated
    } else {
        format!("{truncated}...")
    }
}

fn append_system_instruction(request: &mut crate::llm::GenerationRequest, instruction: &str) {
    if let Some(system_prompt) = &mut request.system_prompt {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(instruction);
    } else {
        request.system_prompt = Some(instruction.to_string());
    }
}

fn estimate_runtime_system_instruction_tokens(instruction: &str) -> usize {
    crate::tape::estimate_text_tokens(instruction).saturating_add(1)
}

fn estimate_request_prompt_overhead_tokens(
    turn_recall_bundle: Option<&str>,
    pending_guardrail_instruction: Option<&str>,
) -> usize {
    turn_recall_bundle
        .into_iter()
        .chain(pending_guardrail_instruction)
        .map(estimate_runtime_system_instruction_tokens)
        .sum()
}

fn estimate_pending_turn_prompt_tokens(
    pending_user_input: Option<&[crate::tape::ContentPart]>,
    turn_recall_bundle: Option<&str>,
) -> usize {
    pending_user_input
        .map(crate::tape::estimate_user_message_tokens)
        .unwrap_or(0)
        .saturating_add(estimate_request_prompt_overhead_tokens(
            turn_recall_bundle,
            None,
        ))
}

async fn finalize_turn_memory_best_effort(
    state: &mut RuntimeLoopState,
    surfaces_refreshed: bool,
    surfaces_context: &'static str,
    promotion_context: &'static str,
) {
    if !surfaces_refreshed {
        super::memory_surfaces::refresh_turn_memory_surfaces_best_effort(state, surfaces_context)
            .await;
    }

    if let Some(job) =
        super::memory_promotion::build_turn_memory_promotion_job(state, promotion_context)
    {
        state
            .turn_state
            .push_deferred_runtime_action(DeferredRuntimeAction::TurnMemoryPromotion(job));
    }
}

fn inject_stream_recovery_instruction(
    request: &mut crate::llm::GenerationRequest,
    visible_text_so_far: &str,
) {
    let instruction = if visible_text_so_far.trim().is_empty() {
        "The prior streaming response was interrupted after visible output but before a complete final answer. Continue the response now. Do not restart from the beginning.".to_string()
    } else {
        format!(
            "The prior streaming response was interrupted after partially outputting text to the user.\nContinue from exactly where it stopped.\nDo not repeat already-emitted text.\nReturn only the continuation.\n\nAlready emitted text:\n<already_emitted>\n{}\n</already_emitted>",
            truncate_for_stream_recovery(visible_text_so_far)
        )
    };

    append_system_instruction(request, &instruction);
}

fn turn_tool_definitions(state: &RuntimeLoopState) -> Vec<crate::llm::ToolDefinition> {
    let include_runtime_delegated_tool = state.prompt_cache.supports_delegated_skill_invocation()
        && !state
            .session
            .dynamic_tools
            .contains_key("invoke_delegated_skill");

    let mut tools = state.tools.get_tool_definitions();
    tools.extend(virtual_tool_definitions(include_runtime_delegated_tool));
    tools.extend(
        state
            .session
            .dynamic_tools
            .values()
            .map(|tool| crate::llm::ToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            }),
    );
    tools
}

fn responses_status_supports_continuation(status: Option<&str>) -> bool {
    matches!(status, Some("completed" | "incomplete") | None)
}

fn uses_responses_input_projection(capabilities: crate::llm::ProviderCapabilities) -> bool {
    matches!(
        capabilities.instruction_role,
        crate::llm::InstructionRole::ResponsesInstructions
    )
}

fn responses_server_managed_compact_threshold(state: &RuntimeLoopState) -> Option<u64> {
    let context_window_tokens = state.runtime_config.context_window_tokens;
    let soft_trigger_ratio = state
        .runtime_config
        .compaction_soft_trigger_ratio
        .clamp(0.0, 1.0);
    if context_window_tokens == 0 || soft_trigger_ratio <= 0.0 {
        return None;
    }

    Some(((context_window_tokens as f64) * (soft_trigger_ratio as f64)).ceil() as u64)
}

fn resolve_responses_continuation(
    state: &mut RuntimeLoopState,
    provider: &str,
    reference_context_revision: u64,
    raw_message_count: usize,
) -> Option<crate::session::ResponsesContinuationState> {
    match state.session.responses_continuation().cloned() {
        Some(continuation) if continuation.provider != provider => {
            state
                .session
                .clear_responses_continuation("provider_changed");
            None
        }
        Some(continuation) if continuation.boundary_message_count > raw_message_count => {
            state
                .session
                .clear_responses_continuation("history_changed");
            None
        }
        Some(continuation)
            if continuation.reference_context_revision != reference_context_revision =>
        {
            state
                .session
                .clear_responses_continuation("reference_context_changed");
            None
        }
        Some(continuation) => Some(continuation),
        None => None,
    }
}

fn should_skip_auto_compaction_for_responses_continuation(state: &mut RuntimeLoopState) -> bool {
    if !state
        .llm_client
        .capabilities()
        .supports_server_managed_continuation
    {
        return false;
    }

    let provider = detect_provider(&state.llm_client);
    let context_revision = state.session.tape.context_revision();
    let raw_message_count = state.session.tape.messages().len();
    resolve_responses_continuation(state, provider, context_revision, raw_message_count).is_some()
}

fn responses_attachment_input_part(
    hash: &str,
    mime_type: &str,
    metadata: &serde_json::Value,
) -> serde_json::Value {
    let image_like = mime_type.starts_with("image/");
    if image_like {
        if let Some(image_url) = metadata
            .get("image_url")
            .or_else(|| metadata.get("file_url"))
            .or_else(|| metadata.get("url"))
            .and_then(serde_json::Value::as_str)
        {
            return serde_json::json!({
                "type": "input_image",
                "image_url": image_url,
            });
        }
        if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
            return serde_json::json!({
                "type": "input_image",
                "file_id": file_id,
            });
        }
    }

    if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
        return serde_json::json!({
            "type": "input_file",
            "file_id": file_id,
        });
    }

    if let Some(file_url) = metadata
        .get("file_url")
        .or_else(|| metadata.get("url"))
        .and_then(serde_json::Value::as_str)
    {
        return serde_json::json!({
            "type": "input_file",
            "file_url": file_url,
        });
    }

    serde_json::json!({
        "type": "input_text",
        "text": format!("[attachment: {} ({})]", hash, mime_type),
    })
}

fn chat_completions_attachment_content_part(
    hash: &str,
    mime_type: &str,
    metadata: &serde_json::Value,
) -> serde_json::Value {
    if mime_type.starts_with("image/")
        && let Some(image_url) = metadata
            .get("image_url")
            .or_else(|| metadata.get("file_url"))
            .or_else(|| metadata.get("url"))
            .and_then(serde_json::Value::as_str)
    {
        return serde_json::json!({
            "type": "image_url",
            "image_url": { "url": image_url },
        });
    }

    if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
        return serde_json::json!({
            "type": "file",
            "file": { "file_id": file_id },
        });
    }

    serde_json::json!({
        "type": "text",
        "text": format!("[attachment: {} ({})]", hash, mime_type),
    })
}

fn anthropic_attachment_content_block(
    hash: &str,
    mime_type: &str,
    metadata: &serde_json::Value,
) -> serde_json::Value {
    let block_type = if mime_type.starts_with("image/") {
        "image"
    } else {
        "document"
    };

    if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
        let mut block = serde_json::json!({
            "type": block_type,
            "source": {
                "type": "file",
                "file_id": file_id,
            },
        });
        if block_type == "document"
            && let Some(title) = metadata.get("title").and_then(serde_json::Value::as_str)
        {
            block["title"] = serde_json::Value::String(title.to_string());
        }
        return block;
    }

    if let Some(url) = metadata
        .get("file_url")
        .or_else(|| metadata.get("image_url"))
        .or_else(|| metadata.get("url"))
        .and_then(serde_json::Value::as_str)
    {
        let mut block = serde_json::json!({
            "type": block_type,
            "source": {
                "type": "url",
                "url": url,
            },
        });
        if block_type == "document"
            && let Some(title) = metadata.get("title").and_then(serde_json::Value::as_str)
        {
            block["title"] = serde_json::Value::String(title.to_string());
        }
        return block;
    }

    serde_json::json!({
        "type": "text",
        "text": format!("[attachment: {} ({})]", hash, mime_type),
    })
}

fn responses_message_content(parts: &[crate::tape::ContentPart]) -> Option<serde_json::Value> {
    let needs_array = parts.iter().any(|part| {
        !matches!(
            part,
            crate::tape::ContentPart::Text { .. } | crate::tape::ContentPart::Thinking { .. }
        )
    });

    if !needs_array {
        let text = crate::tape::parts_to_text(parts);
        return (!text.trim().is_empty()).then_some(serde_json::Value::String(text));
    }

    let content_parts: Vec<serde_json::Value> = parts
        .iter()
        .filter_map(|part| match part {
            crate::tape::ContentPart::Text { text } if !text.trim().is_empty() => {
                Some(serde_json::json!({
                    "type": "input_text",
                    "text": text,
                }))
            }
            crate::tape::ContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => Some(responses_attachment_input_part(hash, mime_type, metadata)),
            crate::tape::ContentPart::Structured { data } => Some(serde_json::json!({
                "type": "input_text",
                "text": data.to_string(),
            })),
            _ => None,
        })
        .collect();

    (!content_parts.is_empty()).then_some(serde_json::Value::Array(content_parts))
}

fn chat_completions_message_content(
    parts: &[crate::tape::ContentPart],
) -> Option<serde_json::Value> {
    let needs_array = parts.iter().any(|part| {
        !matches!(
            part,
            crate::tape::ContentPart::Text { .. } | crate::tape::ContentPart::Thinking { .. }
        )
    });

    if !needs_array {
        let text = crate::tape::parts_to_text(parts);
        return (!text.trim().is_empty()).then_some(serde_json::Value::String(text));
    }

    let content_parts: Vec<serde_json::Value> = parts
        .iter()
        .filter_map(|part| match part {
            crate::tape::ContentPart::Text { text } if !text.trim().is_empty() => {
                Some(serde_json::json!({
                    "type": "text",
                    "text": text,
                }))
            }
            crate::tape::ContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => Some(chat_completions_attachment_content_part(
                hash, mime_type, metadata,
            )),
            crate::tape::ContentPart::Structured { data } => Some(serde_json::json!({
                "type": "text",
                "text": data.to_string(),
            })),
            _ => None,
        })
        .collect();

    (!content_parts.is_empty()).then_some(serde_json::Value::Array(content_parts))
}

fn anthropic_message_content(parts: &[crate::tape::ContentPart]) -> Vec<serde_json::Value> {
    parts
        .iter()
        .filter_map(|part| match part {
            crate::tape::ContentPart::Text { text } if !text.trim().is_empty() => {
                Some(serde_json::json!({
                    "type": "text",
                    "text": text,
                }))
            }
            crate::tape::ContentPart::Thinking { text, signature } if !text.trim().is_empty() => {
                let mut block = serde_json::json!({
                    "type": "thinking",
                    "thinking": text,
                });
                if let Some(signature) = signature
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                {
                    block["signature"] = serde_json::Value::String(signature.to_string());
                }
                Some(block)
            }
            crate::tape::ContentPart::RedactedThinking { data } if !data.trim().is_empty() => {
                Some(serde_json::json!({
                    "type": "redacted_thinking",
                    "data": data,
                }))
            }
            crate::tape::ContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => Some(anthropic_attachment_content_block(
                hash, mime_type, metadata,
            )),
            crate::tape::ContentPart::Structured { data } => Some(serde_json::json!({
                "type": "text",
                "text": data.to_string(),
            })),
            _ => None,
        })
        .collect()
}

fn build_responses_input_items_from_tape(
    messages: &[crate::session::Message],
) -> Vec<serde_json::Value> {
    let mut input = Vec::new();

    for message in messages {
        match message {
            crate::session::Message::Tool { responses } => {
                for response in responses {
                    let projected_output = project_tool_response_for_prompt(&response.content);
                    input.push(serde_json::json!({
                        "type": "function_call_output",
                        "call_id": response.id,
                        "output": projected_output,
                    }));
                }
            }
            crate::session::Message::Assistant {
                parts,
                tool_requests,
            } => {
                if let Some(signature) = message.thinking_signature() {
                    input.push(serde_json::json!({
                        "type": "reasoning",
                        "encrypted_content": signature,
                    }));
                }

                if let Some(content) = responses_message_content(parts) {
                    input.push(serde_json::json!({
                        "role": "assistant",
                        "content": content,
                    }));
                }

                for tool_request in tool_requests {
                    input.push(serde_json::json!({
                        "type": "function_call",
                        "call_id": tool_request.id,
                        "name": tool_request.name,
                        "arguments": tool_request.arguments.to_string(),
                    }));
                }
            }
            crate::session::Message::User { parts }
            | crate::session::Message::System { parts }
            | crate::session::Message::Context { parts } => {
                if let Some(content) = responses_message_content(parts) {
                    let role = match message.role() {
                        crate::session::MessageRole::User => "user",
                        _ => "developer",
                    };
                    input.push(serde_json::json!({
                        "role": role,
                        "content": content,
                    }));
                }
            }
        }
    }

    input
}

fn build_chat_completions_messages_from_tape(
    messages: &[crate::session::Message],
) -> Vec<serde_json::Value> {
    let mut projected = Vec::new();

    for message in messages {
        match message {
            crate::session::Message::Tool { responses } => {
                for response in responses {
                    let projected_content = project_tool_response_for_prompt(&response.content);
                    projected.push(serde_json::json!({
                        "role": "tool",
                        "content": projected_content,
                        "tool_call_id": response.id,
                    }));
                }
            }
            crate::session::Message::Assistant {
                parts,
                tool_requests,
            } => {
                let mut message_value = serde_json::json!({
                    "role": "assistant",
                });

                if let Some(content) = chat_completions_message_content(parts) {
                    message_value["content"] = content;
                }
                if let Some(thinking) = message.thinking_content() {
                    message_value["reasoning_content"] = serde_json::Value::String(thinking);
                }
                if let Some(signature) = message.thinking_signature() {
                    message_value["reasoning"] = serde_json::json!({
                        "encrypted_content": signature,
                    });
                }
                if !tool_requests.is_empty() {
                    message_value["tool_calls"] = serde_json::Value::Array(
                        tool_requests
                            .iter()
                            .map(|tool_request| {
                                serde_json::json!({
                                    "id": tool_request.id,
                                    "type": "function",
                                    "function": {
                                        "name": tool_request.name,
                                        "arguments": tool_request.arguments.to_string(),
                                    },
                                })
                            })
                            .collect(),
                    );
                }

                projected.push(message_value);
            }
            crate::session::Message::User { parts } => {
                if let Some(content) = chat_completions_message_content(parts) {
                    projected.push(serde_json::json!({
                        "role": "user",
                        "content": content,
                    }));
                }
            }
            crate::session::Message::System { parts }
            | crate::session::Message::Context { parts } => {
                if let Some(content) = chat_completions_message_content(parts) {
                    projected.push(serde_json::json!({
                        "role": "developer",
                        "content": content,
                    }));
                }
            }
        }
    }

    projected
}

fn build_anthropic_messages_from_tape(
    messages: &[crate::session::Message],
) -> Vec<serde_json::Value> {
    let mut projected = Vec::new();
    let mut known_tool_use_ids = std::collections::HashSet::new();

    for message in messages {
        match message {
            crate::session::Message::Tool { responses } => {
                for response in responses {
                    let projected_content = project_tool_response_for_prompt(&response.content);
                    let mut blocks = Vec::new();
                    if known_tool_use_ids.contains(&response.id) {
                        blocks.push(serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": response.id,
                            "content": projected_content,
                        }));
                    } else if !projected_content.trim().is_empty() {
                        blocks.push(serde_json::json!({
                            "type": "text",
                            "text": projected_content,
                        }));
                    }
                    if !blocks.is_empty() {
                        projected.push(serde_json::json!({
                            "role": "user",
                            "content": blocks,
                        }));
                    }
                }
            }
            crate::session::Message::Assistant {
                parts,
                tool_requests,
            } => {
                let mut blocks = anthropic_message_content(parts);
                for tool_request in tool_requests {
                    known_tool_use_ids.insert(tool_request.id.clone());
                    blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": tool_request.id,
                        "name": tool_request.name,
                        "input": tool_request.arguments,
                    }));
                }
                if !blocks.is_empty() {
                    projected.push(serde_json::json!({
                        "role": "assistant",
                        "content": blocks,
                    }));
                }
            }
            crate::session::Message::User { parts } => {
                let blocks = anthropic_message_content(parts);
                if !blocks.is_empty() {
                    projected.push(serde_json::json!({
                        "role": "user",
                        "content": blocks,
                    }));
                }
            }
            crate::session::Message::System { .. } | crate::session::Message::Context { .. } => {}
        }
    }

    projected
}

fn strip_repeated_recovery_prefix(existing_text: &str, recovered_text: &str) -> String {
    if existing_text.is_empty() || recovered_text.is_empty() {
        return recovered_text.to_string();
    }

    if let Some(stripped) = recovered_text.strip_prefix(existing_text) {
        return stripped.to_string();
    }

    let mut overlap_bytes = 0usize;
    let mut overlap_chars = 0usize;
    for (byte_idx, _) in existing_text.char_indices() {
        let suffix = &existing_text[byte_idx..];
        if recovered_text.starts_with(suffix) {
            let suffix_chars = suffix.chars().count();
            if suffix_chars > overlap_chars {
                overlap_chars = suffix_chars;
                overlap_bytes = suffix.len();
            }
        }
    }

    let overlap_threshold_chars = {
        let shortest_len = existing_text
            .chars()
            .count()
            .min(recovered_text.chars().count());
        // Keep threshold low enough for short sentences / CJK text, but high enough
        // to avoid accidental one-character trimming.
        (shortest_len / 2).clamp(3, 16)
    };

    if overlap_chars >= overlap_threshold_chars {
        return recovered_text[overlap_bytes.min(recovered_text.len())..].to_string();
    }

    recovered_text.to_string()
}

fn resolve_workspace_persona_dirs(state: &RuntimeLoopState) -> Vec<std::path::PathBuf> {
    state.workspace_persona_dirs.clone()
}

fn build_domain_prompt_with_skills(
    state: &mut RuntimeLoopState,
    user_input: Option<&[crate::tape::ContentPart]>,
    active_skills: Option<&[crate::skills::ActiveSkillEnvelope]>,
) -> super::prompt_cache::PromptAssemblyResult {
    state
        .prompt_cache
        .rebind_paths(resolve_workspace_persona_dirs(state));
    state.prompt_cache.set_workspace_memory_dir(
        state
            .core_config
            .memory
            .enabled
            .then(|| state.core_config.memory.workspace_dir.clone())
            .flatten(),
    );
    match active_skills {
        Some(active_skills) => state
            .prompt_cache
            .build_with_active_skills(active_skills, user_input),
        None => state.prompt_cache.build(user_input),
    }
}

/// Run a single agent turn
pub(super) async fn run_turn_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    turn_kind: TurnRunKind,
    user_input: Option<Vec<crate::tape::ContentPart>>,
    emit: &mut E,
    cancel: &CancellationToken,
    steering_broker: Option<&TurnInputBroker>,
) -> Result<TurnExecutionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if matches!(turn_kind, TurnRunKind::NewTurn) {
        state.turn_state.reset_auto_mid_turn_compaction_state();
        emit(Event::TurnStarted {}).await;
    }

    let user_input_for_skills = user_input.clone();
    let turn_recall_bundle = if state.core_config.memory.enabled {
        super::memory_recall::build_turn_recall_bundle(
            state.core_config.memory.workspace_dir.as_deref(),
            user_input_for_skills.as_deref(),
        )
    } else {
        None
    };

    if !should_skip_auto_compaction_for_responses_continuation(state) {
        let compaction_request = CompactionRequest::automatic_pre_turn()
            .with_additional_prompt_tokens(estimate_pending_turn_prompt_tokens(
                user_input_for_skills.as_deref(),
                turn_recall_bundle.as_deref(),
            ));
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(COMPACTION_TIMEOUT_SECS),
            maybe_compact_context_with_cancel(state, emit, &compaction_request, cancel),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                warn!(error = %e, "Context compaction failed");
            }
            Err(_) => {
                warn!("Context compaction timeout - continuing without compaction");
            }
        }
    }
    if check_turn_cancelled(state, emit, cancel).await? {
        return Ok(TurnExecutionOutcome::Finished);
    }

    if matches!(turn_kind, TurnRunKind::NewTurn) {
        state
            .turn_state
            .begin_turn(state.session.tape.messages().len());
    }
    if let Some(user_input) = user_input {
        state.session.add_user_message_parts(user_input);
    }

    // Resume turns keep the same active skill envelopes for the logical turn.
    // Current user input can still add new skills via prompt assembly merge logic.
    let resumed_active_skills = matches!(turn_kind, TurnRunKind::ResumeTurn)
        .then(|| state.turn_state.active_skills().to_vec())
        .filter(|active_skills| !active_skills.is_empty());
    let prompt_build = build_domain_prompt_with_skills(
        state,
        user_input_for_skills.as_deref(),
        resumed_active_skills.as_deref(),
    );
    debug!(
        elapsed_ms = prompt_build.elapsed_ms,
        skills_cache_hit = prompt_build.skills_cache_hit,
        persona_cache_hit = prompt_build.persona_cache_hit,
        active_skills = prompt_build.active_skills.len(),
        cache_builds = prompt_build.metrics.builds,
        cache_hits = prompt_build.metrics.hits,
        "Prepared prompt assembly inputs"
    );
    state
        .turn_state
        .set_active_skills(prompt_build.active_skills.clone());
    let active_skill_ids = prompt_build
        .active_skills
        .iter()
        .map(|skill| skill.metadata.id.clone())
        .collect::<Vec<_>>();
    let _domain_prompt = prompt_build.domain_prompt;
    let system_prompt = prompt_build.system_prompt;

    let tools = turn_tool_definitions(state);
    let tool_names = tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect::<Vec<_>>();
    let turn_request_controls = crate::resolve_turn_request_controls(
        &state.core_config,
        state.llm_client.capabilities(),
        state.runtime_config.request_control_intent,
        state.turn_state.active_turn_request_control_intent(),
    )?;
    let model = state.core_config.effective_model().to_string();
    let memory_enabled = state.core_config.memory.enabled;
    let context_items = state.session.tape.context_items().to_vec();
    let context_delta = state.session.tape.last_context_delta().clone();
    state.session.record_turn_context_if_changed(
        &model,
        turn_request_controls.reasoning_effort(),
        &system_prompt,
        &context_items,
        &tool_names,
        memory_enabled,
        &active_skill_ids,
        &context_delta,
    );

    let max_tool_loops = if state.runtime_config.max_tool_loops == 0 {
        None
    } else {
        Some(state.runtime_config.max_tool_loops)
    };
    let mut tool_orchestrator =
        ToolTurnOrchestrator::new(max_tool_loops, state.runtime_config.tool_repeat_limit);
    let mut response_guardrails = ResponseGuardrails::default();
    let mut pending_guardrail_instruction: Option<String> = None;
    loop {
        if check_turn_cancelled(state, emit, cancel).await? {
            return Ok(TurnExecutionOutcome::Finished);
        }
        let provider = detect_provider(&state.llm_client);
        let provider_capabilities = state.llm_client.capabilities();
        let responses_input_projection = uses_responses_input_projection(provider_capabilities);
        let supports_server_managed_continuation =
            provider_capabilities.supports_server_managed_continuation;
        let supports_provider_compaction = provider_capabilities.supports_provider_compaction;
        if !supports_server_managed_continuation
            && state
                .session
                .responses_continuation()
                .is_some_and(|continuation| continuation.provider == provider)
        {
            state
                .session
                .clear_responses_continuation("provider_capability_unavailable");
        }

        let prompt_view = state.session.tape.prompt_view();
        let estimated_prompt_tokens =
            prompt_view
                .estimated_tokens
                .saturating_add(estimate_request_prompt_overhead_tokens(
                    turn_recall_bundle.as_deref(),
                    pending_guardrail_instruction.as_deref(),
                ));
        let context_revision = prompt_view.reference_context.revision;
        let messages = prompt_view.messages;
        let raw_tape_messages = state.session.tape.messages().to_vec();
        let mut previous_response_id: Option<String> = None;
        let mut responses_input_items: Option<Vec<serde_json::Value>> = None;
        let llm_messages = if responses_input_projection {
            match supports_server_managed_continuation.then(|| {
                resolve_responses_continuation(
                    state,
                    provider,
                    context_revision,
                    raw_tape_messages.len(),
                )
            }) {
                Some(Some(continuation)) => {
                    previous_response_id = Some(continuation.last_response_id);
                    responses_input_items = Some(build_responses_input_items_from_tape(
                        &raw_tape_messages[continuation.boundary_message_count..],
                    ));
                    state
                        .llm_client
                        .project_messages(&raw_tape_messages[continuation.boundary_message_count..])
                }
                None => {
                    responses_input_items = Some(build_responses_input_items_from_tape(&messages));
                    state.llm_client.project_messages(&messages)
                }
                Some(None) => {
                    responses_input_items = Some(build_responses_input_items_from_tape(&messages));
                    state.llm_client.project_messages(&messages)
                }
            }
        } else {
            state.llm_client.project_messages(&messages)
        };
        let llm_tools: Vec<crate::llm::ToolDefinition> = tools
            .iter()
            .map(|t| {
                crate::llm::ToolDefinition::new(&t.name, &t.description)
                    .with_parameters(t.parameters.clone())
            })
            .collect();

        let mut request = build_generation_request(
            Some(system_prompt.clone()),
            llm_messages,
            llm_tools,
            Some(state.runtime_config.temperature),
            Some(state.runtime_config.max_tokens as i32),
        );
        if let Some(recall_bundle) = turn_recall_bundle.as_deref() {
            append_system_instruction(&mut request, recall_bundle);
        }
        if let Some(instruction) = pending_guardrail_instruction.as_deref() {
            append_system_instruction(&mut request, instruction);
        }
        if matches!(
            provider_capabilities.instruction_role,
            crate::llm::InstructionRole::Developer
        ) {
            request = request.with_extra_param(
                "chat_completions_messages",
                serde_json::Value::Array(build_chat_completions_messages_from_tape(&messages)),
            );
        } else if matches!(
            provider_capabilities.instruction_role,
            crate::llm::InstructionRole::AnthropicSystem
        ) {
            request = request.with_extra_param(
                "anthropic_messages",
                serde_json::Value::Array(build_anthropic_messages_from_tape(&messages)),
            );
        }
        if let Some(responses_input_items) = responses_input_items {
            request = request.with_extra_param(
                "responses_input_items",
                serde_json::Value::Array(responses_input_items),
            );
        }
        if supports_provider_compaction
            && let Some(compact_threshold) = responses_server_managed_compact_threshold(state)
        {
            request = request.with_context_management_compact_threshold(compact_threshold);
        }
        if let Some(previous_response_id) = previous_response_id {
            request = request
                .with_previous_response_id(previous_response_id)
                .with_store(true);
        }
        let request_controls = turn_request_controls.clone();
        request = request.with_reasoning_controls(request_controls.reasoning);

        let request_start = Instant::now();
        let reasoning_effort_log = request_controls
            .reasoning_effort()
            .map(|effort| effort.to_string());
        info!(
            messages = messages.len(),
            estimated_prompt_tokens,
            context_revision,
            tools = tools.len(),
            provider,
            reasoning_effort = reasoning_effort_log.as_deref(),
            request_control_source = ?request_controls.source,
            "LLM request"
        );

        let streaming_requested = match state.runtime_config.streaming_mode {
            crate::config::StreamingMode::Off => false,
            crate::config::StreamingMode::On | crate::config::StreamingMode::Auto => true,
        };
        let mut response_may_be_incomplete = false;

        let response = if streaming_requested {
            // Streaming path: buffer visible output until the final draft is accepted.
            match state.llm_client.generate_stream(request.clone()).await {
                Ok(mut rx) => {
                    let mut accumulated_thinking = String::new();
                    let mut accumulated_thinking_signature: Option<String> = None;
                    let mut accumulated_redacted_thinking: Vec<String> = Vec::new();
                    let mut accumulated_content = String::new();
                    let mut accumulated_tool_calls: Vec<crate::llm::ToolCall> = Vec::new();
                    let mut accumulated_usage: Option<crate::llm::TokenUsage> = None;
                    let mut accumulated_provider_response_id: Option<String> = None;
                    let mut accumulated_provider_response_status: Option<String> = None;
                    // Track tool call assembly from deltas
                    let mut tool_call_buffers: std::collections::HashMap<
                        usize,
                        StreamedToolCallBuffer,
                    > = std::collections::HashMap::new();
                    let mut stream_finished = false;
                    let mut stream_finish_reason: Option<String> = None;
                    let mut emitted_stream_output = false;
                    let mut emitted_visible_stream_output = false;
                    let mut stream_interrupted_after_partial = false;

                    while let Some(chunk) = rx.recv().await {
                        if cancel.is_cancelled() {
                            break;
                        }

                        // Handle thinking delta
                        if let Some(ref thinking) = chunk.thinking
                            && !thinking.is_empty()
                        {
                            accumulated_thinking.push_str(thinking);
                            emitted_stream_output = true;
                        }
                        if let Some(signature) = chunk.thinking_signature
                            && !signature.is_empty()
                        {
                            match &mut accumulated_thinking_signature {
                                Some(existing) => existing.push_str(&signature),
                                None => accumulated_thinking_signature = Some(signature),
                            }
                        }
                        if let Some(redacted) = chunk.redacted_thinking
                            && !redacted.is_empty()
                        {
                            accumulated_redacted_thinking.push(redacted);
                        }

                        // Handle text delta — finalize thinking first
                        if let Some(ref text) = chunk.text
                            && !text.is_empty()
                        {
                            accumulated_content.push_str(text);
                            emitted_stream_output = true;
                            emitted_visible_stream_output = true;
                        }

                        // Handle tool call deltas
                        if let Some(ref delta) = chunk.tool_call_delta {
                            emitted_stream_output = true;
                            let entry = tool_call_buffers.entry(delta.index).or_default();
                            if let Some(ref id) = delta.id {
                                entry.id = Some(id.clone());
                            }
                            if let Some(ref name) = delta.name {
                                entry.name = Some(name.clone());
                            }
                            if let Some(ref args) = delta.arguments_delta {
                                entry.arguments_delta.push_str(args);
                            }
                            if let Some(ref arguments) = delta.arguments {
                                entry.final_arguments = Some(arguments.clone());
                            }
                        }

                        if let Some(usage) = chunk.usage {
                            accumulated_usage = Some(usage);
                        }
                        if let Some(response_id) = chunk.provider_response_id
                            && !response_id.is_empty()
                        {
                            accumulated_provider_response_id = Some(response_id);
                        }
                        if let Some(status) = chunk.provider_response_status
                            && !status.is_empty()
                        {
                            accumulated_provider_response_status = Some(status);
                        }

                        if chunk.is_finished {
                            stream_finished = true;
                            stream_finish_reason = chunk.finish_reason.clone();
                            break;
                        }
                    }

                    if cancel.is_cancelled() && check_turn_cancelled(state, emit, cancel).await? {
                        return Ok(TurnExecutionOutcome::Finished);
                    }

                    let terminal_stream_error = stream_finish_reason
                        .as_deref()
                        .map(|reason| {
                            let normalized = reason.to_ascii_lowercase();
                            normalized == "stream_closed"
                                || normalized == "stream_error"
                                || normalized == "error"
                                || normalized.contains("error")
                        })
                        .unwrap_or(false);

                    let mut fallback_response: Option<crate::llm::GenerationResponse> = None;
                    if !stream_finished || terminal_stream_error {
                        let has_any_stream_payload = emitted_stream_output
                            || !accumulated_content.is_empty()
                            || !accumulated_thinking.is_empty()
                            || !tool_call_buffers.is_empty();

                        if !has_any_stream_payload {
                            warn!(
                                elapsed_ms = request_start.elapsed().as_millis(),
                                "LLM stream ended before producing output; falling back to non-streaming generation"
                            );
                            fallback_response = Some(
                                match generate_with_retry_with_cancel(
                                    &mut state.llm_client,
                                    request,
                                    state.runtime_config.llm_request_timeout_secs,
                                    cancel,
                                )
                                .await
                                {
                                    Ok(response) => response,
                                    Err(error) => {
                                        if cancel.is_cancelled()
                                            && check_turn_cancelled(state, emit, cancel).await?
                                        {
                                            return Ok(TurnExecutionOutcome::Finished);
                                        }
                                        error!(elapsed_ms = request_start.elapsed().as_millis(), error = %error, "LLM failed");
                                        emit(Event::Error {
                                            message: format!("LLM request failed: {}", error),
                                            recoverable: true,
                                        })
                                        .await;
                                        return Ok(TurnExecutionOutcome::Finished);
                                    }
                                },
                            );
                        } else if !emitted_visible_stream_output {
                            // Only tool deltas/metadata were observed; prefer safe fallback generation.
                            warn!(
                                elapsed_ms = request_start.elapsed().as_millis(),
                                "LLM stream interrupted before visible output; falling back to non-streaming generation"
                            );
                            fallback_response = Some(
                                match generate_with_retry_with_cancel(
                                    &mut state.llm_client,
                                    request,
                                    state.runtime_config.llm_request_timeout_secs,
                                    cancel,
                                )
                                .await
                                {
                                    Ok(response) => response,
                                    Err(error) => {
                                        if cancel.is_cancelled()
                                            && check_turn_cancelled(state, emit, cancel).await?
                                        {
                                            return Ok(TurnExecutionOutcome::Finished);
                                        }
                                        error!(elapsed_ms = request_start.elapsed().as_millis(), error = %error, "LLM failed");
                                        emit(Event::Error {
                                            message: format!("LLM request failed: {}", error),
                                            recoverable: true,
                                        })
                                        .await;
                                        return Ok(TurnExecutionOutcome::Finished);
                                    }
                                },
                            );
                        } else {
                            warn!(
                                elapsed_ms = request_start.elapsed().as_millis(),
                                finish_reason = ?stream_finish_reason,
                                "LLM stream ended unexpectedly after partial output; preserving partial response"
                            );
                            stream_interrupted_after_partial = true;
                            let detail = stream_finish_reason
                                .as_deref()
                                .unwrap_or("stream interrupted");
                            emit(Event::Warning {
                                message: format!(
                                    "Stream interrupted after partial output ({detail}); response may be incomplete."
                                ),
                            })
                            .await;
                            response_may_be_incomplete = true;

                            if matches!(
                                state.runtime_config.partial_stream_recovery_mode,
                                crate::config::PartialStreamRecoveryMode::ContinueOnce
                            ) {
                                let mut recovery_request = request.clone();
                                inject_stream_recovery_instruction(
                                    &mut recovery_request,
                                    &accumulated_content,
                                );
                                match generate_with_retry_with_cancel(
                                    &mut state.llm_client,
                                    recovery_request,
                                    state.runtime_config.llm_request_timeout_secs,
                                    cancel,
                                )
                                .await
                                {
                                    Ok(recovered) => {
                                        if cancel.is_cancelled()
                                            && check_turn_cancelled(state, emit, cancel).await?
                                        {
                                            return Ok(TurnExecutionOutcome::Finished);
                                        }

                                        let crate::llm::GenerationResponse {
                                            content: recovered_content,
                                            thinking: recovered_thinking,
                                            thinking_signature: recovered_thinking_signature,
                                            redacted_thinking: recovered_redacted_thinking,
                                            tool_calls: recovered_tool_calls,
                                            usage: recovered_usage,
                                            finish_reason: _recovered_finish_reason,
                                            provider_response_id: recovered_provider_response_id,
                                            provider_response_status:
                                                recovered_provider_response_status,
                                            warnings: recovered_warnings,
                                        } = recovered;

                                        if let Some(recovered_thinking) = recovered_thinking
                                            && !recovered_thinking.is_empty()
                                        {
                                            accumulated_thinking.push_str(&recovered_thinking);
                                        }

                                        if let Some(signature) = recovered_thinking_signature
                                            && !signature.is_empty()
                                        {
                                            match &mut accumulated_thinking_signature {
                                                Some(existing) => existing.push_str(&signature),
                                                None => {
                                                    accumulated_thinking_signature = Some(signature)
                                                }
                                            }
                                        }

                                        if !recovered_redacted_thinking.is_empty() {
                                            accumulated_redacted_thinking
                                                .extend(recovered_redacted_thinking);
                                        }

                                        let continuation = strip_repeated_recovery_prefix(
                                            &accumulated_content,
                                            &recovered_content,
                                        );
                                        if !continuation.is_empty() {
                                            accumulated_content.push_str(&continuation);
                                        }

                                        if !recovered_tool_calls.is_empty() {
                                            accumulated_tool_calls.extend(recovered_tool_calls);
                                        }
                                        if let Some(usage) = recovered_usage {
                                            accumulated_usage = Some(usage);
                                        }
                                        if let Some(response_id) = recovered_provider_response_id
                                            && !response_id.is_empty()
                                        {
                                            accumulated_provider_response_id = Some(response_id);
                                        }
                                        if let Some(status) = recovered_provider_response_status
                                            && !status.is_empty()
                                        {
                                            accumulated_provider_response_status = Some(status);
                                        }
                                        for warning in recovered_warnings {
                                            emit(Event::Warning { message: warning }).await;
                                        }
                                    }
                                    Err(error) => {
                                        if cancel.is_cancelled()
                                            && check_turn_cancelled(state, emit, cancel).await?
                                        {
                                            return Ok(TurnExecutionOutcome::Finished);
                                        }
                                        warn!(
                                            elapsed_ms = request_start.elapsed().as_millis(),
                                            error = %error,
                                            "Partial stream recovery failed; preserving partial output"
                                        );
                                        emit(Event::Warning {
                                            message: format!(
                                                "Failed to recover interrupted stream: {error}"
                                            ),
                                        })
                                        .await;
                                    }
                                }
                            }
                        }
                    }

                    if let Some(response) = fallback_response {
                        response
                    } else {
                        // Assemble tool calls from buffers
                        if stream_interrupted_after_partial && !tool_call_buffers.is_empty() {
                            let skipped = tool_call_buffers.len();
                            tool_call_buffers.clear();
                            warn!(
                                skipped,
                                "Skipping streamed tool calls after partial stream interruption"
                            );
                            emit(Event::Warning {
                                message: format!(
                                    "Skipped {skipped} streamed tool call(s) because the stream ended early."
                                ),
                            })
                            .await;
                        } else {
                            let mut indices: Vec<usize> =
                                tool_call_buffers.keys().copied().collect();
                            indices.sort();
                            for idx in indices {
                                if let Some(StreamedToolCallBuffer {
                                    id,
                                    name: Some(name),
                                    arguments_delta,
                                    final_arguments,
                                }) = tool_call_buffers.remove(&idx)
                                {
                                    let arguments_json = final_arguments.unwrap_or(arguments_delta);
                                    match serde_json::from_str(&arguments_json) {
                                        Ok(arguments) => {
                                            accumulated_tool_calls.push(crate::llm::ToolCall {
                                                id,
                                                name,
                                                arguments,
                                            });
                                        }
                                        Err(err) => {
                                            warn!(
                                                tool_name = %name,
                                                error = %err,
                                                "Dropping malformed streamed tool call arguments"
                                            );
                                            emit(Event::Warning {
                                                message: format!(
                                                    "Dropped malformed streamed tool call `{name}` arguments."
                                                ),
                                            })
                                            .await;
                                        }
                                    }
                                }
                            }
                        }

                        crate::llm::GenerationResponse {
                            content: accumulated_content,
                            thinking: if accumulated_thinking.is_empty() {
                                None
                            } else {
                                Some(accumulated_thinking)
                            },
                            thinking_signature: accumulated_thinking_signature,
                            redacted_thinking: accumulated_redacted_thinking,
                            tool_calls: accumulated_tool_calls,
                            usage: accumulated_usage,
                            finish_reason: stream_finish_reason.clone(),
                            provider_response_id: accumulated_provider_response_id,
                            provider_response_status: accumulated_provider_response_status,
                            warnings: Vec::new(),
                        }
                    }
                }
                Err(error) => {
                    if cancel.is_cancelled() && check_turn_cancelled(state, emit, cancel).await? {
                        return Ok(TurnExecutionOutcome::Finished);
                    }
                    warn!(
                        elapsed_ms = request_start.elapsed().as_millis(),
                        error = %error,
                        "LLM stream initialization failed; falling back to non-streaming generation"
                    );
                    match generate_with_retry_with_cancel(
                        &mut state.llm_client,
                        request,
                        state.runtime_config.llm_request_timeout_secs,
                        cancel,
                    )
                    .await
                    {
                        Ok(response) => response,
                        Err(error) => {
                            if cancel.is_cancelled()
                                && check_turn_cancelled(state, emit, cancel).await?
                            {
                                return Ok(TurnExecutionOutcome::Finished);
                            }
                            error!(elapsed_ms = request_start.elapsed().as_millis(), error = %error, "LLM failed");
                            emit(Event::Error {
                                message: format!("LLM request failed: {}", error),
                                recoverable: true,
                            })
                            .await;
                            return Ok(TurnExecutionOutcome::Finished);
                        }
                    }
                }
            }
        } else {
            // Non-streaming path (existing behavior)
            match generate_with_retry_with_cancel(
                &mut state.llm_client,
                request,
                state.runtime_config.llm_request_timeout_secs,
                cancel,
            )
            .await
            {
                Ok(response) => response,
                Err(error) => {
                    if cancel.is_cancelled() && check_turn_cancelled(state, emit, cancel).await? {
                        return Ok(TurnExecutionOutcome::Finished);
                    }
                    error!(elapsed_ms = request_start.elapsed().as_millis(), error = %error, "LLM failed");
                    emit(Event::Error {
                        message: format!("LLM request failed: {}", error),
                        recoverable: true,
                    })
                    .await;
                    return Ok(TurnExecutionOutcome::Finished);
                }
            }
        };

        if let Some(usage) = response.usage {
            info!(
                prompt_tokens = usage.prompt_tokens,
                completion_tokens = usage.completion_tokens,
                total_tokens = usage.total_tokens,
                reasoning_tokens = ?usage.reasoning_tokens,
                "LLM usage"
            );
        }

        for warning in &response.warnings {
            emit(Event::Warning {
                message: warning.clone(),
            })
            .await;
        }

        let tool_calls = normalize_tool_calls(response.tool_calls);

        let guardrail_context = ResponseGuardrailContext::from_state(state);
        let guardrail_draft = AssistantDraft::new(&response.content, !tool_calls.is_empty());
        match response_guardrails.evaluate(&guardrail_context, &guardrail_draft) {
            GuardrailDecision::Accept => {
                pending_guardrail_instruction = None;
            }
            GuardrailDecision::Recover {
                rule_id,
                reason,
                instruction,
            } => {
                warn!(
                    rule_id,
                    reason = %reason,
                    "Response guardrail triggered for assistant output"
                );
                emit(Event::Warning {
                    message: format!(
                        "Guardrail recovered ({rule_id}): {reason}. Retrying before output."
                    ),
                })
                .await;
                pending_guardrail_instruction = Some(instruction);
                maybe_compact_mid_turn_if_needed(
                    state,
                    emit,
                    cancel,
                    estimate_request_prompt_overhead_tokens(
                        turn_recall_bundle.as_deref(),
                        pending_guardrail_instruction.as_deref(),
                    ),
                )
                .await?;
                if check_turn_cancelled(state, emit, cancel).await? {
                    return Ok(TurnExecutionOutcome::Finished);
                }
                continue;
            }
        }

        if let Some(ref thinking) = response.thinking
            && !thinking.is_empty()
        {
            emit_thinking_chunks(emit, thinking).await;
        }

        if !response.content.is_empty() {
            emit_streaming_chunks(emit, &response.content).await;
        }

        let assistant_message_persisted = if !tool_calls.is_empty() {
            let session_tool_calls: Vec<crate::tape::ToolRequest> = tool_calls
                .iter()
                .map(|tc| crate::tape::ToolRequest {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                })
                .collect();
            state
                .session
                .add_assistant_message_with_tool_calls_and_reasoning(
                    &response.content,
                    session_tool_calls,
                    response.thinking.as_deref(),
                    response.thinking_signature.as_deref(),
                    &response.redacted_thinking,
                );
            true
        } else if !response.content.is_empty() {
            state.session.add_assistant_message_with_reasoning(
                &response.content,
                response.thinking.as_deref(),
                response.thinking_signature.as_deref(),
                &response.redacted_thinking,
            );
            true
        } else {
            false
        };

        if supports_server_managed_continuation && assistant_message_persisted {
            if let Some(response_id) = response.provider_response_id.as_deref()
                && responses_status_supports_continuation(
                    response.provider_response_status.as_deref(),
                )
            {
                state.session.mark_responses_continuation(
                    provider,
                    response_id,
                    state.session.tape.messages().len(),
                    context_revision,
                );
            } else {
                state
                    .session
                    .clear_responses_continuation("continuation_unavailable");
            }
        }

        if !tool_calls.is_empty() {
            match tool_orchestrator
                .orchestrate_tool_batch(
                    state,
                    &tool_calls,
                    ToolOrchestratorInputs {
                        cancel,
                        steering_broker,
                    },
                    emit,
                )
                .await?
            {
                ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. } => {
                    maybe_compact_mid_turn_if_needed(
                        state,
                        emit,
                        cancel,
                        estimate_request_prompt_overhead_tokens(
                            turn_recall_bundle.as_deref(),
                            pending_guardrail_instruction.as_deref(),
                        ),
                    )
                    .await?;
                    if check_turn_cancelled(state, emit, cancel).await? {
                        return Ok(TurnExecutionOutcome::Finished);
                    }
                }
                ToolBatchOrchestratorOutcome::PauseTurn => return Ok(TurnExecutionOutcome::Paused),
                ToolBatchOrchestratorOutcome::EndTurn { surfaces_refreshed } => {
                    if !cancel.is_cancelled() {
                        finalize_turn_memory_best_effort(
                            state,
                            surfaces_refreshed,
                            "turn-ended-after-tool-batch",
                            "after tool-driven end turn",
                        )
                        .await;
                    }
                    return Ok(TurnExecutionOutcome::Finished);
                }
            }
            continue;
        }

        if response.content.is_empty() {
            let fallback_text = "I apologize, but I couldn't generate a response.";
            // Persist fallback output (and any reasoning metadata) to tape so
            // subsequent turns can reference what the assistant actually emitted.
            state.session.add_assistant_message_with_reasoning(
                fallback_text,
                response.thinking.as_deref(),
                response.thinking_signature.as_deref(),
                &response.redacted_thinking,
            );
            if supports_server_managed_continuation {
                if let Some(response_id) = response.provider_response_id.as_deref()
                    && responses_status_supports_continuation(
                        response.provider_response_status.as_deref(),
                    )
                {
                    state.session.mark_responses_continuation(
                        provider,
                        response_id,
                        state.session.tape.messages().len(),
                        context_revision,
                    );
                } else {
                    state
                        .session
                        .clear_responses_continuation("continuation_unavailable");
                }
            }
            finalize_turn_memory_best_effort(
                state,
                false,
                "fallback-turn-completed",
                "after fallback turn",
            )
            .await;
            emit(Event::TextDelta {
                chunk: fallback_text.to_string(),
                is_final: true,
            })
            .await;
            emit(Event::TurnCompleted {
                summary: Some("Turn completed with empty response fallback".to_string()),
            })
            .await;
            return Ok(TurnExecutionOutcome::Finished);
        }

        if response_may_be_incomplete {
            finalize_turn_memory_best_effort(
                state,
                false,
                "interrupted-stream-completed",
                "after interrupted stream",
            )
            .await;
            emit_task_completed_success(
                emit,
                "Task completed with interrupted stream; response may be incomplete.",
            )
            .await;
        } else {
            finalize_turn_memory_best_effort(
                state,
                false,
                "turn-completed",
                "after completed turn",
            )
            .await;
            emit_task_completed_success(emit, "Task completed").await;
        }
        return Ok(TurnExecutionOutcome::Finished);
    }
}

async fn maybe_compact_mid_turn_if_needed<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    cancel: &CancellationToken,
    additional_prompt_tokens: usize,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if should_skip_auto_compaction_for_responses_continuation(state) {
        return Ok(());
    }

    let estimated_prompt_tokens = state
        .session
        .tape
        .estimated_prompt_tokens()
        .saturating_add(additional_prompt_tokens);
    let context_window_tokens = state.runtime_config.context_window_tokens as usize;
    if !state
        .turn_state
        .can_auto_mid_turn_compact(estimated_prompt_tokens, context_window_tokens)
    {
        return Ok(());
    }

    let compaction_request = CompactionRequest::automatic_mid_turn()
        .with_additional_prompt_tokens(additional_prompt_tokens);
    match tokio::time::timeout(
        tokio::time::Duration::from_secs(COMPACTION_TIMEOUT_SECS),
        maybe_compact_context_with_cancel(state, emit, &compaction_request, cancel),
    )
    .await
    {
        Ok(Ok(CompactionOutcome::Applied(outcome))) => {
            state
                .turn_state
                .record_auto_mid_turn_compaction(outcome.output_prompt_tokens);
        }
        Ok(Ok(CompactionOutcome::Skipped(_))) => {}
        Ok(Ok(CompactionOutcome::Failed(_))) => {}
        Ok(Err(e)) => {
            warn!(error = %e, "Mid-turn context compaction failed");
        }
        Err(_) => {
            warn!("Mid-turn context compaction timeout - continuing without compaction");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::turn_state::TurnActivityState;
    use crate::{
        config::Config,
        llm::LlmClient,
        rollout::{RolloutItem, RolloutRecorder},
        runtime::{RuntimeConfig, TurnState},
        session::Session,
        skills::{ResolvedCapabilityView, ScopedPackageDir, SkillScope},
        tape::{ContentPart, Message, ToolRequest, ToolResponse},
        tools::{Tool, ToolContext, ToolRegistry, ToolResult},
    };
    use alan_llm::{
        GenerationRequest, GenerationResponse, LlmProvider, StreamChunk, ToolCall, ToolCallDelta,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    fn maybe_memory_promotion_response(request: &GenerationRequest) -> Option<GenerationResponse> {
        let system_prompt = request.system_prompt.as_deref()?;
        if system_prompt != crate::prompts::MEMORY_PROMOTION_PROMPT {
            return None;
        }

        let joined_user_text = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let content = if joined_user_text.contains("My name is Morris.") {
            serde_json::json!({
                "writes": [{
                    "kind": "user_identity",
                    "target": "USER.md",
                    "confidence": "high",
                    "disposition": "promote_now",
                    "observation": "Name: Morris",
                    "evidence": ["My name is Morris."],
                    "promotion_rationale": "Direct user-stated stable identity detail."
                }]
            })
            .to_string()
        } else {
            serde_json::json!({ "writes": [] }).to_string()
        };

        Some(GenerationResponse {
            content,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        })
    }

    // Mock provider that returns content without tool calls
    struct ContentMockProvider {
        content: String,
        thinking: Option<String>,
    }

    impl ContentMockProvider {
        fn new(content: impl Into<String>) -> Self {
            Self {
                content: content.into(),
                thinking: None,
            }
        }

        fn with_thinking(mut self, thinking: impl Into<String>) -> Self {
            self.thinking = Some(thinking.into());
            self
        }
    }

    #[async_trait]
    impl LlmProvider for ContentMockProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: self.thinking.clone(),
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.content.clone())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx
                .send(StreamChunk {
                    text: Some(self.content.clone()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stop".to_string()),
                })
                .await;
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "content_mock"
        }
    }

    struct FailOnMemoryPromotionProvider {
        content: String,
    }

    #[async_trait]
    impl LlmProvider for FailOnMemoryPromotionProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            if maybe_memory_promotion_response(&request).is_some() {
                panic!("turn execution should not synchronously call memory promotion");
            }

            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.content.clone())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "fail_on_memory_promotion"
        }
    }

    // Mock provider that returns tool calls
    struct ToolCallMockProvider {
        tool_calls: Vec<ToolCall>,
        content: String,
    }

    impl ToolCallMockProvider {
        fn new(tool_calls: Vec<ToolCall>, content: impl Into<String>) -> Self {
            Self {
                tool_calls,
                content: content.into(),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for ToolCallMockProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: self.tool_calls.clone(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(format!("mock: {}", self.content))
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx
                .send(StreamChunk {
                    text: Some(self.content.clone()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stop".to_string()),
                })
                .await;
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "tool_mock"
        }
    }

    struct StreamedFinalToolArgumentsProvider {
        stream_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for StreamedFinalToolArgumentsProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Ok(GenerationResponse {
                content: "stream fallback should not be used".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("streamed-final-tool-arguments".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let call = self.stream_calls.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = tokio::sync::mpsc::channel(4);

            if call == 0 {
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: Some(ToolCallDelta {
                            index: 0,
                            id: Some("call_1".to_string()),
                            name: Some("update_plan".to_string()),
                            arguments_delta: Some("{\"explanation\":".to_string()),
                            arguments: None,
                        }),
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await;
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: Some(ToolCallDelta {
                            index: 0,
                            id: Some("call_1".to_string()),
                            name: Some("update_plan".to_string()),
                            arguments_delta: None,
                            arguments: Some(
                                json!({
                                    "explanation": "Streamed final args",
                                    "items": [
                                        {
                                            "id": "1",
                                            "content": "Step 1",
                                            "status": "completed"
                                        }
                                    ]
                                })
                                .to_string(),
                            ),
                        }),
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await;
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some("tool_calls".to_string()),
                    })
                    .await;
            } else {
                let _ = tx
                    .send(StreamChunk {
                        text: Some("Plan updated".to_string()),
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some("stop".to_string()),
                    })
                    .await;
            }

            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "streamed_final_tool_arguments_mock"
        }
    }

    struct CapturingResponsesProvider {
        requests: Arc<Mutex<Vec<GenerationRequest>>>,
        response: GenerationResponse,
        provider_name: &'static str,
    }

    #[async_trait]
    impl LlmProvider for CapturingResponsesProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.requests.lock().unwrap().push(request);
            Ok(self.response.clone())
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.response.content.clone())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            self.provider_name
        }
    }

    struct SequenceMockProvider {
        responses: VecDeque<GenerationResponse>,
        generate_calls: Arc<AtomicUsize>,
    }

    impl SequenceMockProvider {
        fn new(responses: Vec<GenerationResponse>, generate_calls: Arc<AtomicUsize>) -> Self {
            Self {
                responses: responses.into(),
                generate_calls,
            }
        }
    }

    #[async_trait]
    impl LlmProvider for SequenceMockProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            self.responses
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("No more scripted responses"))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("sequence mock".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            Err(anyhow::anyhow!(
                "streaming not supported in SequenceMockProvider"
            ))
        }

        fn provider_name(&self) -> &'static str {
            "sequence_mock"
        }
    }

    struct NetworkCapabilityTool;

    impl Tool for NetworkCapabilityTool {
        fn name(&self) -> &str {
            "network_probe"
        }

        fn description(&self) -> &str {
            "Test tool classified as network capability."
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move { Ok(json!({"ok": true})) })
        }

        fn capability(
            &self,
            _arguments: &serde_json::Value,
        ) -> alan_agent_protocol::ToolCapability {
            alan_agent_protocol::ToolCapability::Network
        }
    }

    struct ReadCapabilityTool;

    impl Tool for ReadCapabilityTool {
        fn name(&self) -> &str {
            "local_probe"
        }

        fn description(&self) -> &str {
            "Test tool classified as read capability."
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move { Ok(json!({"ok": true})) })
        }

        fn capability(
            &self,
            _arguments: &serde_json::Value,
        ) -> alan_agent_protocol::ToolCapability {
            alan_agent_protocol::ToolCapability::Read
        }
    }

    struct LargeOutputTool {
        output: String,
    }

    impl LargeOutputTool {
        fn new(output: impl Into<String>) -> Self {
            Self {
                output: output.into(),
            }
        }
    }

    impl Tool for LargeOutputTool {
        fn name(&self) -> &str {
            "emit_large_output"
        }

        fn description(&self) -> &str {
            "Emit a large text payload for compaction tests."
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
            let payload = serde_json::to_value(ContentPart::text(self.output.clone())).unwrap();
            Box::pin(async move { Ok(payload) })
        }
    }

    fn create_test_state_with_provider<P: LlmProvider + 'static>(provider: P) -> RuntimeLoopState {
        let config = Config::default();
        let session = Session::new();
        let tools = ToolRegistry::new();
        // Keep turn-executor tests deterministic by defaulting to non-streaming unless a test
        // explicitly opts into streaming semantics.
        let runtime_config = RuntimeConfig {
            streaming_mode: crate::config::StreamingMode::Off,
            ..RuntimeConfig::default()
        };

        RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            llm_client: LlmClient::new(provider),
            tools,
            core_config: config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        }
    }

    async fn run_deferred_runtime_actions(state: &mut RuntimeLoopState) -> usize {
        let cancel = CancellationToken::new();
        let actions = state.turn_state.drain_deferred_runtime_actions();
        let count = actions.len();
        for action in actions {
            assert_eq!(
                super::super::agent_loop::run_deferred_runtime_action_with_cancel(
                    state, action, &cancel,
                )
                .await,
                super::super::agent_loop::DeferredRuntimeActionExit::Completed,
                "run deferred runtime action"
            );
        }
        count
    }

    fn prompt_cache_for_workspace_root(
        workspace_root: &std::path::Path,
        workspace_persona_dirs: Vec<std::path::PathBuf>,
    ) -> crate::runtime::prompt_cache::PromptAssemblyCache {
        let capability_view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: workspace_root.join(".alan/agents/default/skills"),
            scope: SkillScope::Repo,
        }]);
        crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            capability_view,
            workspace_persona_dirs,
            crate::skills::SkillHostCapabilities::default(),
        )
    }

    fn create_repo_skill(
        workspace_root: &std::path::Path,
        dir_name: &str,
        skill_name: &str,
        description: &str,
        body: &str,
    ) {
        let skill_dir = workspace_root
            .join(".alan/agents/default/skills")
            .join(dir_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {skill_name}
description: {description}
---

{body}
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn test_strip_repeated_recovery_prefix_does_not_trim_tiny_overlap() {
        let result = strip_repeated_recovery_prefix("abc", "apple pie");
        assert_eq!(result, "apple pie");
    }

    #[test]
    fn test_strip_repeated_recovery_prefix_trims_full_existing_prefix() {
        let result = strip_repeated_recovery_prefix("partial ", "partial and recovered");
        assert_eq!(result, "and recovered");
    }

    #[test]
    fn test_strip_repeated_recovery_prefix_trims_long_suffix_overlap() {
        let existing = "The quick brown fox jumps over ";
        let recovered = "brown fox jumps over the lazy dog";
        let result = strip_repeated_recovery_prefix(existing, recovered);
        assert_eq!(result, "the lazy dog");
    }

    #[test]
    fn test_strip_repeated_recovery_prefix_handles_short_overlap() {
        let existing = "今天北京天气";
        let recovered = "北京天气很好，适合出门";
        let result = strip_repeated_recovery_prefix(existing, recovered);
        assert_eq!(result, "很好，适合出门");
    }

    #[test]
    fn test_turn_tool_definitions_include_runtime_delegated_schema_when_supported() {
        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.prompt_cache.set_host_capabilities(
            crate::skills::SkillHostCapabilities::default()
                .with_runtime_defaults()
                .with_delegated_skill_invocation(),
        );

        let tools = turn_tool_definitions(&state);
        assert!(
            tools
                .iter()
                .any(|tool| tool.name == "invoke_delegated_skill")
        );
    }

    #[test]
    fn test_turn_tool_definitions_prefer_dynamic_delegated_bridge_schema() {
        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.prompt_cache.set_host_capabilities(
            crate::skills::SkillHostCapabilities::default()
                .with_runtime_defaults()
                .with_delegated_skill_invocation(),
        );
        state.session.dynamic_tools.insert(
            "invoke_delegated_skill".to_string(),
            alan_agent_protocol::DynamicToolSpec {
                name: "invoke_delegated_skill".to_string(),
                description: "Delegated bridge".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task": {"type": "string"}
                    }
                }),
                capability: Some(alan_agent_protocol::ToolCapability::Read),
            },
        );

        let tools = turn_tool_definitions(&state);
        let delegated_tools: Vec<_> = tools
            .iter()
            .filter(|tool| tool.name == "invoke_delegated_skill")
            .collect();
        assert_eq!(delegated_tools.len(), 1);
        assert_eq!(delegated_tools[0].description, "Delegated bridge");
    }

    #[test]
    fn test_build_domain_prompt_with_skills_includes_mentioned_repo_skill_instructions() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let user_input = vec![ContentPart::text("please use $my-skill for this task")];
        let prompt = build_domain_prompt_with_skills(&mut state, Some(&user_input), None);

        assert!(prompt.system_prompt.contains("## Available Skills"));
        assert!(
            prompt
                .system_prompt
                .contains("## Active Skill Instructions")
        );
        assert!(prompt.system_prompt.contains("## Skill: My Skill"));
        assert!(prompt.system_prompt.contains("Use this skill when asked."));
    }

    #[test]
    fn test_build_domain_prompt_with_skills_uses_persona_fallback_from_memory_dir() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let alan_dir = workspace_root.join(".alan");
        let persona_dir = alan_dir.join("agents/default/persona");
        let memory_dir = alan_dir.join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        crate::prompts::ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        std::fs::write(persona_dir.join("SOUL.md"), "custom fallback persona").unwrap();

        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.core_config.memory.workspace_dir = Some(memory_dir);
        state.workspace_persona_dirs = vec![persona_dir];
        state.prompt_cache =
            prompt_cache_for_workspace_root(&workspace_root, state.workspace_persona_dirs.clone());

        let prompt = build_domain_prompt_with_skills(&mut state, None, None);

        assert!(prompt.system_prompt.contains("Workspace Persona Context"));
        assert!(prompt.system_prompt.contains("custom fallback persona"));
    }

    #[test]
    fn test_build_domain_prompt_with_skills_omits_memory_bootstrap_when_memory_disabled() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let alan_dir = workspace_root.join(".alan");
        let memory_dir = alan_dir.join("memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("USER.md"), "# User Memory\n- Morris\n").unwrap();

        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.core_config.memory.workspace_dir = Some(memory_dir);
        state.core_config.memory.enabled = false;
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let prompt = build_domain_prompt_with_skills(&mut state, None, None);

        assert!(!prompt.system_prompt.contains("Workspace Memory Bootstrap"));
        assert!(!prompt.system_prompt.contains("# User Memory"));
    }

    struct StreamEndsImmediatelyProvider {
        fallback_content: String,
        generate_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for StreamEndsImmediatelyProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            Ok(GenerationResponse {
                content: self.fallback_content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.fallback_content.clone())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            drop(tx);
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "stream_ends_immediately_mock"
        }
    }

    struct PartialStreamThenCloseProvider {
        generate_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for PartialStreamThenCloseProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            Ok(GenerationResponse {
                content: "partial and recovered response".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("partial-stream".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (tx, rx) = tokio::sync::mpsc::channel(2);
            let _ = tx
                .send(StreamChunk {
                    text: Some("partial ".to_string()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
            drop(tx);
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "partial_stream_then_close_mock"
        }
    }

    struct TerminalErrorNoPayloadProvider {
        fallback_content: String,
        generate_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for TerminalErrorNoPayloadProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            Ok(GenerationResponse {
                content: self.fallback_content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.fallback_content.clone())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (tx, rx) = tokio::sync::mpsc::channel(2);
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stream_error".to_string()),
                })
                .await;
            drop(tx);
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "terminal_error_no_payload_mock"
        }
    }

    struct TerminalErrorAfterPartialProvider {
        generate_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for TerminalErrorAfterPartialProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            Ok(GenerationResponse {
                content: "partial resumed".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("partial-stream".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (tx, rx) = tokio::sync::mpsc::channel(3);
            let _ = tx
                .send(StreamChunk {
                    text: Some("partial ".to_string()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stream_error".to_string()),
                })
                .await;
            drop(tx);
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "terminal_error_after_partial_mock"
        }
    }

    struct ThinkingThenCloseProvider {
        generate_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for ThinkingThenCloseProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            Ok(GenerationResponse {
                content: "final recovered answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("thinking-then-close".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (tx, rx) = tokio::sync::mpsc::channel(2);
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: Some("reasoning...".to_string()),
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
            drop(tx);
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "thinking_then_close_mock"
        }
    }

    struct StreamingGuardrailRetryProvider {
        stream_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for StreamingGuardrailRetryProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Ok(GenerationResponse {
                content: "should_not_use_generate".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("unused-chat".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let call = self.stream_calls.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = tokio::sync::mpsc::channel(3);
            let text = if call == 0 {
                "I can't access the internet right now."
            } else {
                "I'll check that using available tools."
            };
            let _ = tx
                .send(StreamChunk {
                    text: Some(text.to_string()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stop".to_string()),
                })
                .await;
            drop(tx);
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "streaming_guardrail_retry_mock"
        }
    }

    #[tokio::test]
    async fn test_run_turn_with_content_response() {
        let mut state = create_test_state_with_provider(ContentMockProvider::new("Hello, world!"));
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));

        // Check events
        let has_turn_started = events.iter().any(|e| matches!(e, Event::TurnStarted {}));
        let has_turn_completed = events.iter().any(|e| {
            matches!(
                e,
                Event::TurnCompleted {
                    summary: Some(_),
                    ..
                }
            )
        });

        assert!(has_turn_started, "Expected TurnStarted event");
        assert!(has_turn_completed, "Expected TurnCompleted event");
    }

    #[tokio::test]
    async fn test_run_turn_uses_previous_response_id_for_responses_continuation() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = CapturingResponsesProvider {
            requests: Arc::clone(&requests),
            response: GenerationResponse {
                content: "Follow-up answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: Some("resp_next".to_string()),
                provider_response_status: Some("completed".to_string()),
            },
            provider_name: "openai_responses",
        };
        let mut state = create_test_state_with_provider(provider);
        state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
        state.runtime_config.context_window_tokens = 1000;
        state.runtime_config.compaction_soft_trigger_ratio = 0.5;
        state.session.add_user_message("Earlier input");
        state.session.add_assistant_message("Earlier output", None);
        let boundary_message_count = state.session.tape.messages().len();
        let reference_context_revision = state.session.tape.context_revision();
        state.session.mark_responses_continuation(
            "openai_responses",
            "resp_prev",
            boundary_message_count,
            reference_context_revision,
        );
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("New input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        let requests = requests.lock().unwrap();
        let request = requests.last().expect("captured request");
        assert_eq!(
            request.extra_params.get("previous_response_id"),
            Some(&json!("resp_prev"))
        );
        assert_eq!(request.extra_params.get("store"), Some(&json!(true)));
        assert_eq!(
            request.extra_params.get("context_management"),
            Some(&json!({"compact_threshold": 500}))
        );
        assert_eq!(
            request.extra_params.get("responses_input_items"),
            Some(&json!([
                {
                    "role": "user",
                    "content": "New input"
                }
            ]))
        );
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].role, alan_llm::MessageRole::User);
        assert_eq!(request.messages[0].content, "New input");
        drop(requests);

        let continuation = state
            .session
            .responses_continuation()
            .expect("continuation");
        assert_eq!(continuation.last_response_id, "resp_next");
    }

    #[tokio::test]
    async fn test_run_turn_populates_generation_request_reasoning_effort() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = CapturingResponsesProvider {
            requests: Arc::clone(&requests),
            response: GenerationResponse {
                content: "answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            provider_name: "openai_responses",
        };
        let mut state = create_test_state_with_provider(provider);
        state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
        state.runtime_config.request_control_intent = crate::RequestControlIntent::reasoning_effort(
            Some(alan_agent_protocol::ReasoningEffort::High),
        );
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await
        .unwrap();

        let requests = requests.lock().unwrap();
        let request = requests.last().expect("captured request");
        assert_eq!(
            request.reasoning.effort,
            Some(alan_agent_protocol::ReasoningEffort::High)
        );
    }

    #[tokio::test]
    async fn test_run_turn_uses_turn_reasoning_effort_before_session_effort() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = CapturingResponsesProvider {
            requests: Arc::clone(&requests),
            response: GenerationResponse {
                content: "answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            provider_name: "openai_responses",
        };
        let mut state = create_test_state_with_provider(provider);
        let temp_dir = TempDir::new().unwrap();
        state.session = Session::new_with_recorder_in_dir("gpt-5.4", temp_dir.path())
            .await
            .unwrap();
        state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
        state.runtime_config.request_control_intent = crate::RequestControlIntent::reasoning_effort(
            Some(alan_agent_protocol::ReasoningEffort::High),
        );
        state.turn_state.set_active_turn_request_control_intent(
            crate::RequestControlIntent::reasoning_effort(Some(
                alan_agent_protocol::ReasoningEffort::Low,
            )),
        );
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await
        .unwrap();

        {
            let requests = requests.lock().unwrap();
            let request = requests.last().expect("captured request");
            assert_eq!(
                request.reasoning.effort,
                Some(alan_agent_protocol::ReasoningEffort::Low)
            );
        }

        state.session.flush().await;
        let rollout_path = state.session.rollout_path().expect("rollout path");
        let persisted_effort = RolloutRecorder::load_history(rollout_path)
            .await
            .unwrap()
            .into_iter()
            .find_map(|item| match item {
                RolloutItem::TurnContext(ctx) => ctx.reasoning_effort,
                _ => None,
            });
        assert_eq!(
            persisted_effort,
            Some(alan_agent_protocol::ReasoningEffort::Low)
        );
    }

    #[tokio::test]
    async fn test_run_turn_uses_session_reasoning_effort_without_budget_fallback() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = CapturingResponsesProvider {
            requests: Arc::clone(&requests),
            response: GenerationResponse {
                content: "answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            provider_name: "openai_responses",
        };
        let mut state = create_test_state_with_provider(provider);
        state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
        state.runtime_config.request_control_intent = crate::RequestControlIntent::reasoning_effort(
            Some(alan_agent_protocol::ReasoningEffort::High),
        );
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await
        .unwrap();

        let requests = requests.lock().unwrap();
        let request = requests.last().expect("captured request");
        assert_eq!(
            request.reasoning.effort,
            Some(alan_agent_protocol::ReasoningEffort::High)
        );
    }

    #[tokio::test]
    async fn test_run_turn_omits_reasoning_controls_when_unset() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = CapturingResponsesProvider {
            requests: Arc::clone(&requests),
            response: GenerationResponse {
                content: "answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            provider_name: "openai_responses",
        };
        let mut state = create_test_state_with_provider(provider);
        state.core_config.llm_provider = crate::config::LlmProvider::OpenRouter;
        state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await
        .unwrap();

        let requests = requests.lock().unwrap();
        let request = requests.last().expect("captured request");
        assert_eq!(request.reasoning.effort, None);
    }

    #[tokio::test]
    async fn test_run_turn_invalidates_responses_continuation_when_reference_context_changes() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = CapturingResponsesProvider {
            requests: Arc::clone(&requests),
            response: GenerationResponse {
                content: "Fresh answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: Some("resp_fresh".to_string()),
                provider_response_status: Some("completed".to_string()),
            },
            provider_name: "openai_responses",
        };
        let mut state = create_test_state_with_provider(provider);
        state.session.add_user_message("Earlier input");
        state.session.add_assistant_message("Earlier output", None);
        let boundary_message_count = state.session.tape.messages().len();
        let reference_context_revision = state.session.tape.context_revision();
        state.session.mark_responses_continuation(
            "openai_responses",
            "resp_prev",
            boundary_message_count,
            reference_context_revision,
        );
        state
            .session
            .tape
            .apply_context_items(vec![crate::tape::ContextItem::new(
                "ctx_1",
                "workspace_note",
                "Workspace note",
                "Reference context changed",
            )]);
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("New input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        let requests = requests.lock().unwrap();
        let request = requests.last().expect("captured request");
        assert!(!request.extra_params.contains_key("previous_response_id"));
        assert!(
            request
                .extra_params
                .get("responses_input_items")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|items| items.iter().any(|item| {
                    item.get("role").and_then(serde_json::Value::as_str) == Some("developer")
                }))
        );
        assert!(
            request
                .messages
                .iter()
                .any(|message| message.content == "New input")
        );
    }

    #[test]
    fn test_build_responses_input_items_from_tape_projects_developer_role_and_attachments() {
        let messages = vec![
            crate::session::Message::Context {
                parts: vec![ContentPart::text("Workspace context")],
            },
            crate::session::Message::User {
                parts: vec![
                    ContentPart::text("What is in this image?"),
                    ContentPart::Attachment {
                        hash: "img_hash".to_string(),
                        mime_type: "image/png".to_string(),
                        metadata: json!({
                            "image_url": "https://example.com/cat.png"
                        }),
                    },
                ],
            },
        ];

        let items = build_responses_input_items_from_tape(&messages);
        assert_eq!(
            items,
            vec![
                json!({
                    "role": "developer",
                    "content": "Workspace context"
                }),
                json!({
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": "What is in this image?"
                        },
                        {
                            "type": "input_image",
                            "image_url": "https://example.com/cat.png"
                        }
                    ]
                })
            ]
        );
    }

    #[test]
    fn test_build_chat_completions_messages_from_tape_projects_developer_role_and_attachments() {
        let messages = vec![
            crate::session::Message::Context {
                parts: vec![ContentPart::text("Workspace context")],
            },
            crate::session::Message::User {
                parts: vec![
                    ContentPart::text("What is in this image?"),
                    ContentPart::Attachment {
                        hash: "img_hash".to_string(),
                        mime_type: "image/png".to_string(),
                        metadata: json!({
                            "image_url": "https://example.com/cat.png"
                        }),
                    },
                ],
            },
        ];

        let projected = build_chat_completions_messages_from_tape(&messages);
        assert_eq!(
            projected,
            vec![
                json!({
                    "role": "developer",
                    "content": "Workspace context"
                }),
                json!({
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": "What is in this image?"
                        },
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": "https://example.com/cat.png"
                            }
                        }
                    ]
                })
            ]
        );
    }

    #[test]
    fn test_build_chat_completions_messages_from_tape_projects_file_url_image_attachments() {
        let messages = vec![crate::session::Message::User {
            parts: vec![
                ContentPart::text("What is in this image?"),
                ContentPart::Attachment {
                    hash: "img_hash".to_string(),
                    mime_type: "image/png".to_string(),
                    metadata: json!({
                        "file_url": "https://example.com/cat.png"
                    }),
                },
            ],
        }];

        let projected = build_chat_completions_messages_from_tape(&messages);
        assert_eq!(
            projected,
            vec![json!({
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "What is in this image?"
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "https://example.com/cat.png"
                        }
                    }
                ]
            })]
        );
    }

    #[test]
    fn test_build_anthropic_messages_from_tape_projects_file_attachments() {
        let messages = vec![crate::session::Message::User {
            parts: vec![
                ContentPart::text("Read this document"),
                ContentPart::Attachment {
                    hash: "doc_hash".to_string(),
                    mime_type: "application/pdf".to_string(),
                    metadata: json!({
                        "file_id": "file_123",
                        "title": "Spec"
                    }),
                },
            ],
        }];

        let projected = build_anthropic_messages_from_tape(&messages);
        assert_eq!(
            projected,
            vec![json!({
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "Read this document"
                    },
                    {
                        "type": "document",
                        "source": {
                            "type": "file",
                            "file_id": "file_123"
                        },
                        "title": "Spec"
                    }
                ]
            })]
        );
    }

    #[test]
    fn test_build_responses_input_items_from_tape_caps_tool_payloads() {
        let large_output = "x".repeat(40_000);
        let messages = vec![crate::session::Message::Tool {
            responses: vec![ToolResponse {
                id: "call-1".to_string(),
                content: vec![ContentPart::text(large_output.clone())],
            }],
        }];

        let items = build_responses_input_items_from_tape(&messages);
        let output = items[0]
            .get("output")
            .and_then(serde_json::Value::as_str)
            .expect("responses item should contain string output");

        assert_eq!(
            output,
            project_tool_response_for_prompt(&[ContentPart::text(large_output)])
        );
        assert!(output.len() <= 30_003);
    }

    #[test]
    fn test_build_chat_completions_messages_from_tape_caps_tool_payloads() {
        let large_output = "x".repeat(40_000);
        let messages = vec![crate::session::Message::Tool {
            responses: vec![ToolResponse {
                id: "call-1".to_string(),
                content: vec![ContentPart::text(large_output.clone())],
            }],
        }];

        let projected = build_chat_completions_messages_from_tape(&messages);
        let output = projected[0]
            .get("content")
            .and_then(serde_json::Value::as_str)
            .expect("chat completions tool message should contain string content");

        assert_eq!(
            output,
            project_tool_response_for_prompt(&[ContentPart::text(large_output)])
        );
        assert!(output.len() <= 30_003);
    }

    #[test]
    fn test_build_anthropic_messages_from_tape_caps_tool_payloads() {
        let large_output = "x".repeat(40_000);
        let messages = vec![
            crate::session::Message::Assistant {
                parts: Vec::new(),
                tool_requests: vec![ToolRequest {
                    id: "call-1".to_string(),
                    name: "tool".to_string(),
                    arguments: json!({}),
                }],
            },
            crate::session::Message::Tool {
                responses: vec![ToolResponse {
                    id: "call-1".to_string(),
                    content: vec![ContentPart::text(large_output.clone())],
                }],
            },
        ];

        let projected = build_anthropic_messages_from_tape(&messages);
        let output = projected[1]["content"][0]
            .get("content")
            .and_then(serde_json::Value::as_str)
            .expect("anthropic tool_result should contain string content");

        assert_eq!(
            output,
            project_tool_response_for_prompt(&[ContentPart::text(large_output)])
        );
        assert!(output.len() <= 30_003);
    }

    #[tokio::test]
    async fn test_run_turn_skips_auto_compaction_for_responses_continuation() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = CapturingResponsesProvider {
            requests: Arc::clone(&requests),
            response: GenerationResponse {
                content: "Follow-up answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: Some("resp_next".to_string()),
                provider_response_status: Some("completed".to_string()),
            },
            provider_name: "openai_responses",
        };
        let mut state = create_test_state_with_provider(provider);
        state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
        state.runtime_config.compaction_trigger_messages = 0;
        state.runtime_config.context_window_tokens = 1;
        state.runtime_config.compaction_soft_trigger_ratio = 0.0;
        state.runtime_config.compaction_hard_trigger_ratio = 0.0;
        state.runtime_config.compaction_trigger_ratio = 0.0;
        state.session.add_user_message("Earlier input");
        state.session.add_assistant_message("Earlier output", None);
        let boundary_message_count = state.session.tape.messages().len();
        let reference_context_revision = state.session.tape.context_revision();
        state.session.mark_responses_continuation(
            "openai_responses",
            "resp_prev",
            boundary_message_count,
            reference_context_revision,
        );
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("New input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        let requests = requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            1,
            "responses continuation should skip local auto-compaction requests"
        );
        assert_eq!(
            requests[0].extra_params.get("previous_response_id"),
            Some(&json!("resp_prev"))
        );
    }

    #[tokio::test]
    async fn test_run_turn_chatgpt_ignores_responses_continuation_state() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = CapturingResponsesProvider {
            requests: Arc::clone(&requests),
            response: GenerationResponse {
                content: "Follow-up answer".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: Some("resp_next".to_string()),
                provider_response_status: Some("completed".to_string()),
            },
            provider_name: "chatgpt",
        };
        let mut state = create_test_state_with_provider(provider);
        state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
        state.runtime_config.context_window_tokens = 1000;
        state.runtime_config.compaction_soft_trigger_ratio = 0.5;
        state.session.add_user_message("Earlier input");
        state.session.add_assistant_message("Earlier output", None);
        let boundary_message_count = state.session.tape.messages().len();
        let reference_context_revision = state.session.tape.context_revision();
        state.session.mark_responses_continuation(
            "chatgpt",
            "resp_prev",
            boundary_message_count,
            reference_context_revision,
        );
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("New input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let requests = requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            1,
            "chatgpt should issue a single fresh request"
        );
        let request = requests.last().expect("captured request");
        assert!(!request.extra_params.contains_key("previous_response_id"));
        assert!(!request.extra_params.contains_key("store"));
        assert!(
            !request.extra_params.contains_key("context_management"),
            "chatgpt should not inherit openai_responses provider compaction payloads"
        );
        assert_eq!(
            request.extra_params.get("responses_input_items"),
            Some(&json!([
                {
                    "role": "user",
                    "content": "Earlier input"
                },
                {
                    "role": "assistant",
                    "content": "Earlier output"
                },
                {
                    "role": "user",
                    "content": "New input"
                }
            ]))
        );
        assert!(state.session.responses_continuation().is_none());
    }

    #[tokio::test]
    async fn test_run_turn_recovers_unavailability_claim_when_network_tool_exists() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![
                GenerationResponse {
                    content: "I don't have access to real-time weather data.".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: "I'll check that using available tools.".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
            ],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state.tools.register(NetworkCapabilityTool);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("how's the weather today?")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(
            generate_calls.load(Ordering::SeqCst),
            2,
            "Guardrail should retry once before emitting a contradictory draft"
        );

        let has_guardrail_warning = events.iter().any(|event| {
            matches!(
                event,
                Event::Warning { message }
                    if message.contains("Guardrail recovered")
                        && message.contains("capability_contradiction")
            )
        });
        assert!(has_guardrail_warning);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(emitted_text, "I'll check that using available tools.");
    }

    #[tokio::test]
    async fn test_run_turn_keeps_truthful_network_failure_explanation() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![GenerationResponse {
                content:
                    "I can't access the internet right now because that request was blocked by policy."
                        .to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            }],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state.tools.register(NetworkCapabilityTool);
        state
            .session
            .tape
            .push(Message::user("how's the weather today?"));
        state.session.tape.push(Message::Assistant {
            parts: Vec::new(),
            tool_requests: vec![ToolRequest {
                id: "call_network".to_string(),
                name: "network_probe".to_string(),
                arguments: json!({}),
            }],
        });
        state.session.add_tool_message(
            "call_network",
            "network_probe",
            json!({
                "error": "network tool blocked by policy",
                "status": "blocked_by_policy"
            }),
        );
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            None,
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(
            generate_calls.load(Ordering::SeqCst),
            1,
            "Truthful failure explanations should not be rewritten by the guardrail"
        );

        let has_guardrail_warning = events.iter().any(|event| {
            matches!(event, Event::Warning { message } if message.contains("Guardrail recovered"))
        });
        assert!(!has_guardrail_warning);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(
            emitted_text,
            "I can't access the internet right now because that request was blocked by policy."
        );

        let assistant_messages: Vec<_> = state
            .session
            .tape
            .messages()
            .iter()
            .filter(|message| matches!(message, Message::Assistant { .. }))
            .collect();
        let last_assistant = assistant_messages
            .last()
            .expect("expected final assistant message to be recorded");
        assert_eq!(
            last_assistant.non_thinking_text_content(),
            "I can't access the internet right now because that request was blocked by policy."
        );
    }

    #[tokio::test]
    async fn test_run_turn_recovers_network_claim_after_non_network_timeout() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![
                GenerationResponse {
                    content: "I can't access the internet right now.".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: "I'll check that using available tools.".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
            ],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state.tools.register(NetworkCapabilityTool);
        state.tools.register(ReadCapabilityTool);
        state
            .session
            .tape
            .push(Message::user("how's the weather today?"));
        state.session.tape.push(Message::Assistant {
            parts: Vec::new(),
            tool_requests: vec![ToolRequest {
                id: "call_local".to_string(),
                name: "local_probe".to_string(),
                arguments: json!({}),
            }],
        });
        state.session.add_tool_message(
            "call_local",
            "local_probe",
            json!({
                "error": "local command timed out",
                "status": "timeout"
            }),
        );
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            None,
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(
            generate_calls.load(Ordering::SeqCst),
            2,
            "A non-network timeout should not suppress the network contradiction recovery"
        );

        let has_guardrail_warning = events.iter().any(|event| {
            matches!(
                event,
                Event::Warning { message }
                    if message.contains("Guardrail recovered")
                        && message.contains("capability_contradiction")
            )
        });
        assert!(has_guardrail_warning);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(emitted_text, "I'll check that using available tools.");
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_with_steer_keeps_truthful_network_failure_explanation() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![GenerationResponse {
                content:
                    "I can't access the internet right now because that request was blocked by policy."
                        .to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            }],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state.tools.register(NetworkCapabilityTool);
        state.session.tape.push(Message::user("earlier turn"));
        state
            .session
            .tape
            .push(Message::assistant("earlier turn completed"));
        state
            .session
            .tape
            .push(Message::user("how's the weather today?"));
        state.session.tape.push(Message::Assistant {
            parts: Vec::new(),
            tool_requests: vec![ToolRequest {
                id: "call_network".to_string(),
                name: "network_probe".to_string(),
                arguments: json!({}),
            }],
        });
        state.session.add_tool_message(
            "call_network",
            "network_probe",
            json!({
                "error": "network tool blocked by policy",
                "status": "blocked_by_policy"
            }),
        );
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            Some(vec![ContentPart::text(
                "steer: explain the network failure clearly",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(
            generate_calls.load(Ordering::SeqCst),
            1,
            "Steer input should not hide earlier failures from the same active turn"
        );

        let has_guardrail_warning = events.iter().any(|event| {
            matches!(event, Event::Warning { message } if message.contains("Guardrail recovered"))
        });
        assert!(!has_guardrail_warning);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(
            emitted_text,
            "I can't access the internet right now because that request was blocked by policy."
        );
    }

    #[tokio::test]
    async fn test_run_turn_new_turn_ignores_prior_failures_without_completed_assistant_boundary() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![
                GenerationResponse {
                    content: "I can't access the internet right now.".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: "I'll check that using available tools.".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
            ],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state.tools.register(NetworkCapabilityTool);
        state.session.tape.push(Message::user("earlier turn"));
        state.session.tape.push(Message::Assistant {
            parts: Vec::new(),
            tool_requests: vec![ToolRequest {
                id: "call_network".to_string(),
                name: "network_probe".to_string(),
                arguments: json!({}),
            }],
        });
        state.session.add_tool_message(
            "call_network",
            "network_probe",
            json!({
                "error": "network tool blocked by policy",
                "status": "blocked_by_policy"
            }),
        );
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("how's the weather today?")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(
            generate_calls.load(Ordering::SeqCst),
            2,
            "Prior-turn failures must not suppress recovery in a new turn"
        );

        let has_guardrail_warning = events.iter().any(|event| {
            matches!(
                event,
                Event::Warning { message }
                    if message.contains("Guardrail recovered")
                        && message.contains("capability_contradiction")
            )
        });
        assert!(has_guardrail_warning);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(emitted_text, "I'll check that using available tools.");
    }

    #[tokio::test]
    async fn test_run_turn_empty_response_fallback() {
        // Provider returns empty content
        let mut state = create_test_state_with_provider(ContentMockProvider::new(""));
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        // Check for empty response fallback
        let has_fallback = events.iter().any(|e| {
            matches!(e, Event::TurnCompleted { summary } if summary.as_deref() == Some("Turn completed with empty response fallback"))
        });
        assert!(has_fallback, "Expected empty response fallback");

        let assistant_messages: Vec<_> = state
            .session
            .tape
            .messages()
            .iter()
            .filter(|m| matches!(m, crate::session::Message::Assistant { .. }))
            .collect();
        assert_eq!(
            assistant_messages.len(),
            1,
            "Expected fallback assistant message"
        );
        assert_eq!(
            assistant_messages[0].non_thinking_text_content(),
            "I apologize, but I couldn't generate a response."
        );
    }

    #[tokio::test]
    async fn test_run_turn_empty_content_with_thinking_persists_reasoning() {
        let mut state = create_test_state_with_provider(
            ContentMockProvider::new("").with_thinking("internal reasoning"),
        );
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));

        let assistant_messages: Vec<_> = state
            .session
            .tape
            .messages()
            .iter()
            .filter(|m| matches!(m, crate::session::Message::Assistant { .. }))
            .collect();
        assert_eq!(
            assistant_messages.len(),
            1,
            "Expected a single assistant message"
        );
        assert_eq!(
            assistant_messages[0].thinking_content().as_deref(),
            Some("internal reasoning")
        );
        assert_eq!(
            assistant_messages[0].non_thinking_text_content(),
            "I apologize, but I couldn't generate a response."
        );
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_run_turn_performs_mid_turn_compaction_before_follow_up_generation() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![
                GenerationResponse {
                    content: String::new(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![ToolCall {
                        id: Some("call-mid-turn".to_string()),
                        name: "emit_large_output".to_string(),
                        arguments: json!({}),
                    }],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: "Mid-turn compaction summary".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: "Finished after compaction".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
            ],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state
            .tools
            .register(LargeOutputTool::new("very long tool output\n".repeat(600)));
        state.runtime_config.compaction_trigger_messages = 1_000;
        state.runtime_config.compaction_keep_last = 1;
        state.runtime_config.context_window_tokens = 512;
        state.runtime_config.compaction_trigger_ratio = 0.5;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Use the tool and continue")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(generate_calls.load(Ordering::SeqCst), 3);
        assert_eq!(
            state.session.tape.summary(),
            Some("Mid-turn compaction summary")
        );
        assert_eq!(state.turn_state.compactions_this_turn(), 1);
        assert!(
            state
                .session
                .tape
                .messages()
                .iter()
                .any(|message| message.text_content().contains("Finished after compaction"))
        );
        assert!(events.iter().any(|event| {
            matches!(
                event,
                Event::TurnCompleted {
                    summary: Some(summary)
                } if summary.contains("Task completed")
            )
        }));
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_run_turn_resets_mid_turn_compaction_budget_for_new_turns() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![
                GenerationResponse {
                    content: String::new(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![ToolCall {
                        id: Some("call-mid-turn".to_string()),
                        name: "emit_large_output".to_string(),
                        arguments: json!({}),
                    }],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: "Mid-turn compaction summary".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: "Finished after compaction".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
            ],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state
            .tools
            .register(LargeOutputTool::new("very long tool output\n".repeat(600)));
        state.runtime_config.compaction_trigger_messages = 1_000;
        state.runtime_config.compaction_keep_last = 1;
        state.runtime_config.context_window_tokens = 512;
        state.runtime_config.compaction_trigger_ratio = 0.5;
        state.turn_state.record_auto_mid_turn_compaction(256);
        state.turn_state.record_auto_mid_turn_compaction(512);

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Use the tool and continue")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(generate_calls.load(Ordering::SeqCst), 3);
        assert_eq!(
            state.session.tape.summary(),
            Some("Mid-turn compaction summary")
        );
        assert_eq!(state.turn_state.compactions_this_turn(), 1);
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn() {
        let mut state = create_test_state_with_provider(ContentMockProvider::new("Response"));
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn, // Resume, not new turn
            None,                    // No new user input
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        // Resume turn should not emit TurnStarted
        let turn_started_count = events
            .iter()
            .filter(|e| matches!(e, Event::TurnStarted {}))
            .count();
        assert_eq!(
            turn_started_count, 0,
            "Resume turn should not emit TurnStarted"
        );
    }

    #[tokio::test]
    async fn test_run_turn_with_cancel() {
        let mut state = create_test_state_with_provider(ContentMockProvider::new("Response"));
        let cancel = CancellationToken::new();
        cancel.cancel(); // Cancel immediately

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        // Should finish early due to cancellation
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    }

    #[tokio::test]
    async fn test_run_turn_with_update_plan_tool() {
        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "update_plan".to_string(),
                arguments: json!({
                    "explanation": "Test plan",
                    "items": [{"id": "1", "content": "Step 1", "status": "in_progress"}]
                }),
            }],
            "", // No content, just tool call
        ));
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        // Should report update_plan completion via tool lifecycle event.
        let has_update_plan_completion = events.iter().any(|e| {
            matches!(
                e,
                Event::ToolCallCompleted {
                    id,
                    result_preview: Some(preview),
                    ..
                } if id == "call_1" && preview.contains("plan_updated")
            )
        });
        assert!(
            has_update_plan_completion,
            "Expected ToolCallCompleted preview for update_plan"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::PlanUpdated { explanation, items }
                if explanation.as_deref() == Some("Test plan")
                    && items.len() == 1
                    && items[0].content == "Step 1"
        )));
    }

    #[tokio::test]
    async fn test_streamed_tool_execution_prefers_final_arguments_over_deltas() {
        let mut state = create_test_state_with_provider(StreamedFinalToolArgumentsProvider {
            stream_calls: Arc::new(AtomicUsize::new(0)),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test streamed tool args")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));

        let has_update_plan_completion = events.iter().any(|event| {
            matches!(
                event,
                Event::ToolCallCompleted {
                    id,
                    result_preview: Some(preview),
                    ..
                } if id == "call_1" && preview.contains("plan_updated")
            )
        });
        assert!(
            has_update_plan_completion,
            "Expected streamed tool execution to use final arguments"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::PlanUpdated { explanation, items }
                if explanation.as_deref() == Some("Streamed final args")
                    && items.len() == 1
                    && items[0].content == "Step 1"
                    && matches!(items[0].status, alan_agent_protocol::PlanItemStatus::Completed)
        )));

        let dropped_malformed_warning = events.iter().any(|event| {
            matches!(
                event,
                Event::Warning { message }
                    if message.contains("Dropped malformed streamed tool call")
            )
        });
        assert!(
            !dropped_malformed_warning,
            "Expected final tool arguments to override malformed deltas"
        );
    }

    #[tokio::test]
    async fn test_run_turn_with_confirmation_tool() {
        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_id": "chk_123",
                    "checkpoint_type": "test",
                    "summary": "Test confirmation"
                }),
            }],
            "",
        ));
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Paused));

        // Should have Yield Confirmation event
        let has_confirmation = events.iter().any(|e| {
            matches!(
                e,
                Event::Yield {
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    ..
                }
            )
        });
        assert!(has_confirmation, "Expected Yield Confirmation event");
    }

    #[tokio::test]
    async fn test_run_turn_refreshes_memory_surfaces_when_tool_batch_ends_turn() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");

        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({}),
            }],
            "",
        ));
        state.core_config.memory.workspace_dir = Some(memory_dir.clone());
        state
            .turn_state
            .set_turn_activity(TurnActivityState::Running);

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Error { message, .. } if message == "Invalid confirmation request."
        )));
        assert!(memory_dir.join("handoffs").join("LATEST.md").exists());
        assert!(memory_dir.join("sessions").exists());
        assert!(
            std::fs::read_dir(memory_dir.join("daily"))
                .unwrap()
                .next()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_run_turn_promotes_direct_user_fact_when_tool_batch_ends_turn() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");

        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({}),
            }],
            "",
        ));
        state.core_config.memory.workspace_dir = Some(memory_dir.clone());
        state.core_config.memory.enabled = true;
        state
            .turn_state
            .set_turn_activity(TurnActivityState::Running);

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("My name is Morris.")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        let user_memory_before =
            std::fs::read_to_string(memory_dir.join("USER.md")).unwrap_or_else(|_| String::new());
        assert!(!user_memory_before.contains("Name: Morris"));
        assert_eq!(run_deferred_runtime_actions(&mut state).await, 1);

        let user_memory = std::fs::read_to_string(memory_dir.join("USER.md")).unwrap();
        assert!(user_memory.contains("Name: Morris"));
    }

    #[tokio::test]
    async fn test_run_turn_defers_memory_promotion_until_after_completion() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");

        let mut state = create_test_state_with_provider(FailOnMemoryPromotionProvider {
            content: "Done.".to_string(),
        });
        state.core_config.memory.workspace_dir = Some(memory_dir);
        state.core_config.memory.enabled = true;

        let cancel = CancellationToken::new();
        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("My name is Morris.")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::TurnCompleted { .. }))
        );
        assert_eq!(state.turn_state.drain_deferred_runtime_actions().len(), 1);
    }

    struct SlowTool {
        delay: tokio::time::Duration,
    }

    impl Tool for SlowTool {
        fn name(&self) -> &str {
            "slow_tool"
        }

        fn description(&self) -> &str {
            "Slow tool used to test cancellation."
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
            let delay = self.delay;
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                Ok(json!({ "ok": true }))
            })
        }
    }

    #[tokio::test]
    async fn test_run_turn_cancelled_tool_batch_does_not_refresh_memory_surfaces() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");

        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "slow_tool".to_string(),
                arguments: json!({}),
            }],
            "",
        ));
        state.core_config.memory.workspace_dir = Some(memory_dir.clone());
        state
            .turn_state
            .set_turn_activity(TurnActivityState::Running);
        state.tools.register(SlowTool {
            delay: tokio::time::Duration::from_millis(50),
        });

        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            cancel_for_task.cancel();
        });

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::TurnCompleted { summary: Some(summary) }
                if summary == "Task cancelled by user"
        )));
        assert!(!memory_dir.join("handoffs").join("LATEST.md").exists());
        assert!(!memory_dir.join("sessions").exists());
        assert!(!memory_dir.join("daily").exists());
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn test_run_turn_tool_loop_guard_refreshes_memory_surfaces_before_completion_event() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![
                GenerationResponse {
                    content: String::new(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![ToolCall {
                        id: Some("call-1".to_string()),
                        name: "update_plan".to_string(),
                        arguments: json!({
                            "explanation": "Loop 1",
                            "items": [{"id": "1", "content": "Step 1", "status": "in_progress"}]
                        }),
                    }],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: String::new(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![ToolCall {
                        id: Some("call-2".to_string()),
                        name: "update_plan".to_string(),
                        arguments: json!({
                            "explanation": "Loop 2",
                            "items": [{"id": "2", "content": "Step 2", "status": "in_progress"}]
                        }),
                    }],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
            ],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state.core_config.memory.workspace_dir = Some(memory_dir.clone());
        state.runtime_config.max_tool_loops = 2;

        let cancel = CancellationToken::new();
        let mut saw_handoff_before_completion = false;
        let mut emit = |event: Event| {
            if matches!(
                event,
                Event::TurnCompleted {
                    summary: Some(ref summary)
                } if summary == "Tool loop stopped by loop guard"
            ) {
                saw_handoff_before_completion = memory_dir.join("handoffs/LATEST.md").exists();
            }
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "Run until the loop guard stops you.",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(generate_calls.load(Ordering::SeqCst), 2);
        assert_eq!(state.turn_state.drain_deferred_runtime_actions().len(), 1);
        assert!(saw_handoff_before_completion);
    }

    #[tokio::test]
    async fn test_run_turn_confirmation_includes_active_skill_permission_hints() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let skill_dir = workspace_root.join(".alan/agents/default/skills/release-check");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
        ));
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "please use $release-check for this task",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "release-check");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "May require write approval."
        );
    }

    struct RecordingToolCallProvider {
        tool_calls: Vec<ToolCall>,
        content: String,
        seen_system_prompts: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl RecordingToolCallProvider {
        fn new(
            tool_calls: Vec<ToolCall>,
            content: impl Into<String>,
            seen_system_prompts: Arc<std::sync::Mutex<Vec<String>>>,
        ) -> Self {
            Self {
                tool_calls,
                content: content.into(),
                seen_system_prompts,
            }
        }

        fn record_system_prompt(&self, request: &GenerationRequest) {
            if let Some(system_prompt) = request.system_prompt.as_ref() {
                self.seen_system_prompts
                    .lock()
                    .unwrap()
                    .push(system_prompt.clone());
            }
        }
    }

    #[async_trait]
    impl LlmProvider for RecordingToolCallProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.record_system_prompt(&request);
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: self.tool_calls.clone(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(format!("mock: {}", self.content))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.record_system_prompt(&request);
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx
                .send(StreamChunk {
                    text: Some(self.content.clone()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stop".to_string()),
                })
                .await;
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "recording_tool_call_mock"
        }
    }

    #[tokio::test]
    async fn test_run_turn_includes_runtime_recall_bundle_for_identity_query() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let memory_dir = workspace_root.join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("USER.md"),
            "# User Memory\n- Favorite runtime marker: ALAN_IDENTITY_RECALL\n",
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_IDENTITY_RECALL",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.workspace_dir = Some(memory_dir);
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "What is my favorite runtime marker?",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let request_prompt = system_prompts
            .iter()
            .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
            .expect("expected runtime recall bundle prompt");
        assert!(request_prompt.contains("## Runtime Recall Bundle"));
        assert!(request_prompt.contains(".alan/memory/USER.md"));
        assert!(request_prompt.contains("ALAN_IDENTITY_RECALL"));
    }

    #[tokio::test]
    async fn test_run_turn_includes_runtime_recall_bundle_for_continuity_query() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let memory_dir = workspace_root.join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("handoffs/LATEST.md"),
            "# Latest Handoff\n- Continuity marker: ALAN_CONTINUITY_RECALL\n",
        )
        .unwrap();
        std::fs::create_dir_all(memory_dir.join("sessions/2026/04/15")).unwrap();
        std::fs::write(
            memory_dir.join("sessions/2026/04/15/session-1.md"),
            "# Session Summary\n- Continuity marker: ALAN_CONTINUITY_RECALL\n",
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_CONTINUITY_RECALL",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.workspace_dir = Some(memory_dir);
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "What were we doing in the previous session?",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let request_prompt = system_prompts
            .iter()
            .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
            .expect("expected runtime recall bundle prompt");
        assert!(request_prompt.contains("## Runtime Recall Bundle"));
        assert!(request_prompt.contains(".alan/memory/handoffs/LATEST.md"));
        assert!(request_prompt.contains(".alan/memory/sessions/2026/04/15/session-1.md"));
        assert!(request_prompt.contains("ALAN_CONTINUITY_RECALL"));
    }

    #[tokio::test]
    async fn test_run_turn_includes_runtime_recall_bundle_for_recent_query_fallback() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let memory_dir = workspace_root.join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        std::fs::create_dir_all(memory_dir.join("sessions/2026/04/16")).unwrap();
        for index in 1..=4 {
            std::fs::write(
                memory_dir.join(format!("topics/recent-match-{index}.md")),
                format!("# Topic Note\nwe did document topic match {index}\n"),
            )
            .unwrap();
        }
        std::fs::write(
            memory_dir.join("daily/2026-04-16.md"),
            "## 2026-04-16\nALAN_RECENT_RECALL\n",
        )
        .unwrap();
        for index in 1..=4 {
            std::fs::write(
                memory_dir.join(format!("sessions/2026/04/16/session-{index}.md")),
                format!("# Session Summary\nALAN_RECENT_RECALL_{index}\n"),
            )
            .unwrap();
        }

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_RECENT_RECALL_4",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.workspace_dir = Some(memory_dir);
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("What did we do yesterday?")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let request_prompt = system_prompts
            .iter()
            .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
            .expect("expected runtime recall bundle prompt");
        assert!(request_prompt.contains("## Runtime Recall Bundle"));
        assert!(request_prompt.contains(".alan/memory/daily/2026-04-16.md"));
        assert!(request_prompt.contains(".alan/memory/sessions/2026/04/16/session-4.md"));
        assert!(request_prompt.contains("ALAN_RECENT_RECALL_4"));
        assert!(!request_prompt.contains(".alan/memory/topics/recent-match-4.md"));
    }

    #[tokio::test]
    async fn test_run_turn_pre_turn_compaction_accounts_for_runtime_recall_budget() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let memory_dir = workspace_root.join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("USER.md"),
            format!(
                "# User Memory\n- Favorite runtime marker: {}\n",
                "ALAN_PRETURN_RECALL ".repeat(80)
            ),
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "COMPACTED_FOR_RECALL",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.workspace_dir = Some(memory_dir.clone());
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        state.runtime_config.compaction_keep_last = 2;
        state.runtime_config.compaction_trigger_messages = usize::MAX;
        state.runtime_config.compaction_soft_trigger_ratio = 1.0;
        state.runtime_config.compaction_hard_trigger_ratio = 1.0;
        state.runtime_config.compaction_trigger_ratio = 1.0;
        for idx in 0..3 {
            state
                .session
                .add_user_message(&format!("Earlier user context {idx} {}", "u".repeat(220)));
            state.session.add_assistant_message(
                &format!("Earlier assistant context {idx} {}", "a".repeat(220)),
                None,
            );
        }

        let user_input = vec![ContentPart::text("What is my favorite runtime marker?")];
        let turn_recall_bundle = crate::runtime::memory_recall::build_turn_recall_bundle(
            Some(memory_dir.as_path()),
            Some(&user_input),
        );
        let pending_prompt_tokens =
            estimate_pending_turn_prompt_tokens(Some(&user_input), turn_recall_bundle.as_deref());
        assert!(pending_prompt_tokens > 0);

        let base_prompt_tokens = state.session.tape.estimated_prompt_tokens();
        state.runtime_config.context_window_tokens =
            (base_prompt_tokens + pending_prompt_tokens - 1) as u32;

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(user_input),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(state.session.tape.summary(), Some("COMPACTED_FOR_RECALL"));

        let system_prompts = seen_system_prompts.lock().unwrap();
        assert_eq!(system_prompts.len(), 2);
        let request_prompt = system_prompts
            .iter()
            .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
            .expect("expected runtime recall bundle prompt");
        assert!(request_prompt.contains("## Runtime Recall Bundle"));
        assert!(request_prompt.contains("ALAN_PRETURN_RECALL"));
        assert_eq!(state.turn_state.drain_deferred_runtime_actions().len(), 1);
    }

    #[tokio::test]
    async fn test_run_turn_omits_runtime_recall_bundle_when_memory_disabled() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let memory_dir = workspace_root.join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("USER.md"),
            "# User Memory\n- Favorite runtime marker: ALAN_DISABLED_RECALL\n",
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "ok",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.workspace_dir = Some(memory_dir);
        state.core_config.memory.enabled = false;
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "What is my favorite runtime marker?",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let request_prompt = system_prompts.last().expect("expected system prompt");
        assert!(!request_prompt.contains("## Runtime Recall Bundle"));
        assert!(!request_prompt.contains("ALAN_DISABLED_RECALL"));
    }

    #[tokio::test]
    async fn test_maybe_compact_mid_turn_accounts_for_runtime_prompt_overhead() {
        let mut state = create_test_state_with_provider(ContentMockProvider::new(
            "MID_TURN_COMPACTION_SUMMARY",
        ));
        state.runtime_config.compaction_keep_last = 2;
        state.runtime_config.compaction_trigger_messages = usize::MAX;
        state.runtime_config.compaction_soft_trigger_ratio = 1.0;
        state.runtime_config.compaction_hard_trigger_ratio = 1.0;
        state.runtime_config.compaction_trigger_ratio = 1.0;
        for idx in 0..3 {
            state
                .session
                .add_user_message(&format!("Mid-turn user context {idx} {}", "u".repeat(220)));
            state.session.add_assistant_message(
                &format!("Mid-turn assistant context {idx} {}", "a".repeat(220)),
                None,
            );
        }

        let pending_guardrail_instruction = format!(
            "Retry with a corrected answer and preserve tool intent.\n{}",
            "guardrail-overhead ".repeat(80)
        );
        let additional_prompt_tokens = estimate_request_prompt_overhead_tokens(
            None,
            Some(pending_guardrail_instruction.as_str()),
        );
        assert!(additional_prompt_tokens > 0);

        let base_prompt_tokens = state.session.tape.estimated_prompt_tokens();
        state.runtime_config.context_window_tokens =
            (base_prompt_tokens + additional_prompt_tokens - 1) as u32;

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = maybe_compact_mid_turn_if_needed(
            &mut state,
            &mut emit,
            &cancel,
            additional_prompt_tokens,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(
            state.session.tape.summary(),
            Some("MID_TURN_COMPACTION_SUMMARY")
        );
        assert_eq!(state.turn_state.compactions_this_turn(), 1);
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_preserves_active_skill_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let skill_dir = workspace_root.join(".alan/agents/default/skills/release-check");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
            seen_system_prompts.clone(),
        ));
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
            "please use $release-check for this task",
        )]));
        state
            .turn_state
            .set_active_skills(prior_prompt.active_skills);

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            None,
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let resumed_prompt = system_prompts.last().expect("expected system prompt");
        assert!(resumed_prompt.contains("## Skill: Release Check"));
        assert!(resumed_prompt.contains("Use this skill when asked."));

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "release-check");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "May require write approval."
        );
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_with_steer_preserves_active_skill_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let skill_dir = workspace_root.join(".alan/agents/default/skills/release-check");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
            seen_system_prompts.clone(),
        ));
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
            "please use $release-check for this task",
        )]));
        state
            .turn_state
            .set_active_skills(prior_prompt.active_skills);

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            Some(vec![ContentPart::text(
                "steer: tighten the approval explanation",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let resumed_prompt = system_prompts.last().expect("expected system prompt");
        assert!(resumed_prompt.contains("## Skill: Release Check"));
        assert!(resumed_prompt.contains("Use this skill when asked."));

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "release-check");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "May require write approval."
        );
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_without_prior_active_skills_can_activate_skill_from_steer() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let skill_dir = workspace_root.join(".alan/agents/default/skills/release-check");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
            seen_system_prompts.clone(),
        ));
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            Some(vec![ContentPart::text(
                "steer: please use $release-check for this task",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let resumed_prompt = system_prompts.last().expect("expected system prompt");
        assert!(resumed_prompt.contains("## Skill: Release Check"));
        assert!(resumed_prompt.contains("Use this skill when asked."));

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "release-check");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "May require write approval."
        );
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_with_steer_can_add_new_skill_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");

        let release_skill_dir = workspace_root.join(".alan/agents/default/skills/release-check");
        std::fs::create_dir_all(&release_skill_dir).unwrap();
        std::fs::write(
            release_skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this release skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            release_skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let audit_skill_dir = workspace_root.join(".alan/agents/default/skills/safety-audit");
        std::fs::create_dir_all(&audit_skill_dir).unwrap();
        std::fs::write(
            audit_skill_dir.join("SKILL.md"),
            r#"---
name: Safety Audit
description: Review risky operations for safety concerns
---

# Instructions
Use this safety skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            audit_skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require network approval."
"#,
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
            seen_system_prompts.clone(),
        ));
        state.prompt_cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());

        let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
            "please use $release-check for this task",
        )]));
        state
            .turn_state
            .set_active_skills(prior_prompt.active_skills);

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            Some(vec![ContentPart::text(
                "steer: also use $safety-audit before approving this",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let resumed_prompt = system_prompts.last().expect("expected system prompt");
        assert!(resumed_prompt.contains("## Skill: Release Check"));
        assert!(resumed_prompt.contains("Use this release skill when asked."));
        assert!(resumed_prompt.contains("## Skill: Safety Audit"));
        assert!(resumed_prompt.contains("Use this safety skill when asked."));

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 2);
        let skill_ids: std::collections::BTreeSet<String> = hints
            .iter()
            .filter_map(|hint| {
                hint.get("skill_id")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            })
            .collect();
        assert_eq!(
            skill_ids,
            std::collections::BTreeSet::from([
                "release-check".to_string(),
                "safety-audit".to_string(),
            ])
        );
    }

    #[tokio::test]
    async fn test_run_turn_llm_error() {
        // Use error provider
        struct ErrorMockProvider;

        #[async_trait]
        impl LlmProvider for ErrorMockProvider {
            async fn generate(
                &mut self,
                _request: GenerationRequest,
            ) -> anyhow::Result<GenerationResponse> {
                Err(anyhow::anyhow!("LLM error"))
            }

            async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
                Err(anyhow::anyhow!("LLM error"))
            }

            async fn generate_stream(
                &mut self,
                _request: GenerationRequest,
            ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
                Err(anyhow::anyhow!("LLM error"))
            }

            fn provider_name(&self) -> &'static str {
                "error_mock"
            }
        }

        let mut state = create_test_state_with_provider(ErrorMockProvider);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));

        // Should have error event
        let has_error = events.iter().any(
            |e| matches!(e, Event::Error { message, .. } if message.contains("LLM request failed")),
        );
        assert!(has_error, "Expected Error event for LLM failure");
    }

    #[tokio::test]
    async fn test_stream_end_without_output_falls_back_to_non_streaming() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(StreamEndsImmediatelyProvider {
            fallback_content: "fallback non-stream response".to_string(),
            generate_calls: Arc::clone(&generate_calls),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test fallback")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(generate_calls.load(Ordering::SeqCst), 1);
        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert!(emitted_text.contains("fallback non-stream response"));
    }

    #[tokio::test]
    async fn test_partial_stream_attempts_recovery_and_emits_warning() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(PartialStreamThenCloseProvider {
            generate_calls: Arc::clone(&generate_calls),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test partial stream")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(generate_calls.load(Ordering::SeqCst), 1);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(emitted_text, "partial and recovered response");

        let has_partial_warning = events.iter().any(|event| {
            matches!(
                event,
                Event::Warning { message }
                    if message.contains("Stream interrupted after partial output")
            )
        });
        assert!(has_partial_warning);
        let has_incomplete_summary = events.iter().any(|event| {
            matches!(
                event,
                Event::TurnCompleted { summary: Some(summary) }
                    if summary.contains("response may be incomplete")
            )
        });
        assert!(has_incomplete_summary);
    }

    #[tokio::test]
    async fn test_partial_stream_recovery_can_be_disabled() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(PartialStreamThenCloseProvider {
            generate_calls: Arc::clone(&generate_calls),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;
        state.runtime_config.partial_stream_recovery_mode =
            crate::config::PartialStreamRecoveryMode::Off;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "Test partial stream with recovery off",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(generate_calls.load(Ordering::SeqCst), 0);
        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(emitted_text, "partial ");
    }

    #[tokio::test]
    async fn test_terminal_stream_error_without_payload_falls_back_to_non_streaming() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(TerminalErrorNoPayloadProvider {
            fallback_content: "fallback from terminal stream error".to_string(),
            generate_calls: Arc::clone(&generate_calls),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "Test terminal stream error fallback",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(generate_calls.load(Ordering::SeqCst), 1);
        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert!(emitted_text.contains("fallback from terminal stream error"));
    }

    #[tokio::test]
    async fn test_terminal_stream_error_after_partial_output_preserves_partial_and_warns() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(TerminalErrorAfterPartialProvider {
            generate_calls: Arc::clone(&generate_calls),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "Test terminal stream error with partial output",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(generate_calls.load(Ordering::SeqCst), 1);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(emitted_text, "partial resumed");

        let has_warning = events.iter().any(|event| {
            matches!(
                event,
                Event::Warning { message }
                    if message.contains("Stream interrupted after partial output")
                        && message.contains("stream_error")
            )
        });
        assert!(has_warning);
    }

    #[tokio::test]
    async fn test_run_turn_streaming_recovers_unavailability_claim_when_network_tool_exists() {
        let stream_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(StreamingGuardrailRetryProvider {
            stream_calls: Arc::clone(&stream_calls),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;
        state.tools.register(NetworkCapabilityTool);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("how's the weather today?")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(
            stream_calls.load(Ordering::SeqCst),
            2,
            "Guardrail should retry once before emitting a contradictory streamed draft"
        );

        let has_guardrail_warning = events.iter().any(|event| {
            matches!(
                event,
                Event::Warning { message } if message.contains("Guardrail recovered")
            )
        });
        assert!(has_guardrail_warning);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(emitted_text, "I'll check that using available tools.");

        let assistant_messages: Vec<_> = state
            .session
            .tape
            .messages()
            .iter()
            .filter(|m| matches!(m, crate::session::Message::Assistant { .. }))
            .collect();
        assert_eq!(assistant_messages.len(), 1);
        assert_eq!(
            assistant_messages[0].non_thinking_text_content(),
            "I'll check that using available tools."
        );
    }

    #[tokio::test]
    async fn test_thinking_only_interruption_falls_back_to_non_streaming() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(ThinkingThenCloseProvider {
            generate_calls: Arc::clone(&generate_calls),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("test thinking-only interruption")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(generate_calls.load(Ordering::SeqCst), 1);

        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(emitted_text, "final recovered answer");

        let has_fallback_warning = events.iter().any(|event| {
            matches!(
                event,
                Event::Warning { message }
                    if message.contains("LLM stream interrupted before visible output")
            )
        });
        assert!(!has_fallback_warning);

        let has_thinking_output = events.iter().any(|event| {
            matches!(
                event,
                Event::ThinkingDelta { chunk, is_final: false } if chunk.contains("reasoning")
            )
        });
        assert!(!has_thinking_output);
    }
}
