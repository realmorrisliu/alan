use super::{GenerationRequest, InstructionRole, Message, MessageRole, ToolCall};
use crate::agent_machine::{Message as MachineMessage, MessageRole as MachineMessageRole};
use crate::tape::{ContentPart, parts_to_text};

const MAX_PROJECTED_TOOL_PAYLOAD_SIZE: usize = 30_000;
const PROJECTION_TRUNCATION_MARKER: &str = "...[truncated]";

pub(crate) fn project_messages(
    messages: &[MachineMessage],
    preserve_thinking: bool,
) -> Vec<Message> {
    messages
        .iter()
        .flat_map(|message| match message {
            MachineMessage::Tool { responses } => responses
                .iter()
                .map(|response| Message {
                    role: MessageRole::Tool,
                    content: project_tool_response(&response.content),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    tool_calls: None,
                    tool_call_id: non_empty_trimmed(&response.id),
                })
                .collect::<Vec<_>>(),
            _ => {
                let role = match message.role() {
                    MachineMessageRole::System => MessageRole::System,
                    MachineMessageRole::Context => MessageRole::Context,
                    MachineMessageRole::User => MessageRole::User,
                    MachineMessageRole::Assistant => MessageRole::Assistant,
                    MachineMessageRole::Tool => MessageRole::Tool,
                };
                let tool_calls = (!message.tool_requests().is_empty()).then(|| {
                    message
                        .tool_requests()
                        .iter()
                        .map(|request| ToolCall {
                            id: non_empty_trimmed(&request.id),
                            name: request.name.clone(),
                            arguments: request.arguments.clone(),
                        })
                        .collect()
                });

                vec![Message {
                    role,
                    content: message.non_thinking_text_content(),
                    thinking: preserve_thinking
                        .then(|| message.thinking_content())
                        .flatten(),
                    thinking_signature: preserve_thinking
                        .then(|| message.thinking_signature())
                        .flatten(),
                    redacted_thinking: preserve_thinking
                        .then(|| {
                            let blocks = message.redacted_thinking_blocks();
                            (!blocks.is_empty()).then_some(blocks)
                        })
                        .flatten(),
                    tool_calls,
                    tool_call_id: None,
                }]
            }
        })
        .collect()
}

pub(crate) fn with_provider_input(
    request: GenerationRequest,
    instruction_role: InstructionRole,
    messages: &[MachineMessage],
) -> GenerationRequest {
    let (key, projected) = match instruction_role {
        InstructionRole::ResponsesInstructions => (
            "responses_input_items",
            build_responses_input_items(messages),
        ),
        InstructionRole::Developer => (
            "chat_completions_messages",
            build_chat_completions_messages(messages),
        ),
        InstructionRole::AnthropicSystem => {
            ("anthropic_messages", build_anthropic_messages(messages))
        }
        InstructionRole::System => return request,
    };

    request.with_extra_param(key, serde_json::Value::Array(projected))
}

fn non_empty_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn project_tool_response(parts: &[ContentPart]) -> String {
    project_tool_response_content(parts, MAX_PROJECTED_TOOL_PAYLOAD_SIZE)
}

fn project_tool_response_content(parts: &[ContentPart], max_size: usize) -> String {
    let mut content = String::new();

    for (index, part) in parts.iter().enumerate() {
        let remaining = max_size.saturating_sub(content.len());
        if remaining == 0 {
            break;
        }

        let remaining_parts = parts.len() - index;
        let part_budget = remaining.div_ceil(remaining_parts);
        content.push_str(&project_tool_content_part(part, part_budget));
    }

    truncate_text_for_projection(&content, max_size)
}

fn project_tool_content_part(part: &ContentPart, max_size: usize) -> String {
    let raw = match part {
        ContentPart::Structured { data } => {
            let truncated = truncate_payload_for_projection(data.clone(), max_size);
            serde_json::to_string(&truncated).unwrap_or_else(|_| "{}".to_string())
        }
        _ => part.to_text_lossy(),
    };

    truncate_text_for_projection(&raw, max_size)
}

fn truncate_payload_for_projection(
    payload: serde_json::Value,
    max_size: usize,
) -> serde_json::Value {
    let payload_str = payload.to_string();
    if payload_str.len() <= max_size {
        return payload;
    }

    match payload {
        serde_json::Value::Object(map) => {
            let mut truncated = serde_json::Map::new();
            let mut current_size = 0;

            for (key, value) in map {
                let is_critical = matches!(key.as_str(), "success" | "error" | "url" | "title");
                if is_critical {
                    truncated.insert(key, value);
                    continue;
                }

                let processed_value = if key == "content" || key == "aggregated_content" {
                    if let serde_json::Value::String(value) = &value {
                        serde_json::Value::String(truncate_text_for_projection(value, max_size / 4))
                    } else {
                        value
                    }
                } else {
                    truncate_payload_for_projection(value, max_size / 2)
                };

                let value_size = processed_value.to_string().len();
                if current_size + value_size < max_size * 3 / 4 {
                    truncated.insert(key, processed_value);
                    current_size += value_size;
                } else {
                    truncated.insert(
                        "_truncated".to_string(),
                        serde_json::Value::String("Additional fields omitted".to_string()),
                    );
                    break;
                }
            }

            serde_json::Value::Object(truncated)
        }
        serde_json::Value::Array(items) => {
            let item_count = items.len();
            let mut truncated = Vec::new();
            let mut current_size = 0;

            for item in items {
                let processed = truncate_payload_for_projection(item, max_size / item_count.max(1));
                let item_size = processed.to_string().len();

                if current_size + item_size < max_size * 3 / 4 {
                    truncated.push(processed);
                    current_size += item_size;
                } else {
                    truncated.push(serde_json::json!({
                        "_note": "Additional array items omitted"
                    }));
                    break;
                }
            }

            serde_json::Value::Array(truncated)
        }
        serde_json::Value::String(value) => {
            if value.len() > max_size / 10 {
                serde_json::Value::String(truncate_text_for_projection(&value, max_size / 10))
            } else {
                serde_json::Value::String(value)
            }
        }
        other => other,
    }
}

fn truncate_text_for_projection(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    if max_len == 0 {
        return String::new();
    }
    if max_len <= PROJECTION_TRUNCATION_MARKER.len() {
        return utf8_prefix(PROJECTION_TRUNCATION_MARKER, max_len);
    }

    let prefix_len = max_len - PROJECTION_TRUNCATION_MARKER.len();
    format!(
        "{}{}",
        utf8_prefix(text, prefix_len),
        PROJECTION_TRUNCATION_MARKER
    )
}

fn utf8_prefix(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    let mut end = 0;
    for (index, character) in text.char_indices() {
        let next = index + character.len_utf8();
        if next > max_len {
            break;
        }
        end = next;
    }

    text[..end].to_string()
}

fn responses_attachment_input_part(
    hash: &str,
    mime_type: &str,
    metadata: &serde_json::Value,
) -> serde_json::Value {
    if mime_type.starts_with("image/") {
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
        "text": format!("[attachment: {hash} ({mime_type})]"),
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
        "text": format!("[attachment: {hash} ({mime_type})]"),
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
        add_anthropic_document_title(&mut block, block_type, metadata);
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
        add_anthropic_document_title(&mut block, block_type, metadata);
        return block;
    }

    serde_json::json!({
        "type": "text",
        "text": format!("[attachment: {hash} ({mime_type})]"),
    })
}

fn add_anthropic_document_title(
    block: &mut serde_json::Value,
    block_type: &str,
    metadata: &serde_json::Value,
) {
    if block_type == "document"
        && let Some(title) = metadata.get("title").and_then(serde_json::Value::as_str)
    {
        block["title"] = serde_json::Value::String(title.to_string());
    }
}

fn responses_message_content(parts: &[ContentPart]) -> Option<serde_json::Value> {
    let needs_array = parts.iter().any(|part| {
        !matches!(
            part,
            ContentPart::Text { .. } | ContentPart::Thinking { .. }
        )
    });
    if !needs_array {
        let text = parts_to_text(parts);
        return (!text.trim().is_empty()).then_some(serde_json::Value::String(text));
    }

    let content_parts = parts
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } if !text.trim().is_empty() => Some(serde_json::json!({
                "type": "input_text",
                "text": text,
            })),
            ContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => Some(responses_attachment_input_part(hash, mime_type, metadata)),
            ContentPart::Structured { data } => Some(serde_json::json!({
                "type": "input_text",
                "text": data.to_string(),
            })),
            _ => None,
        })
        .collect::<Vec<_>>();

    (!content_parts.is_empty()).then_some(serde_json::Value::Array(content_parts))
}

fn chat_completions_message_content(parts: &[ContentPart]) -> Option<serde_json::Value> {
    let needs_array = parts.iter().any(|part| {
        !matches!(
            part,
            ContentPart::Text { .. } | ContentPart::Thinking { .. }
        )
    });
    if !needs_array {
        let text = parts_to_text(parts);
        return (!text.trim().is_empty()).then_some(serde_json::Value::String(text));
    }

    let content_parts = parts
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } if !text.trim().is_empty() => Some(serde_json::json!({
                "type": "text",
                "text": text,
            })),
            ContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => Some(chat_completions_attachment_content_part(
                hash, mime_type, metadata,
            )),
            ContentPart::Structured { data } => Some(serde_json::json!({
                "type": "text",
                "text": data.to_string(),
            })),
            _ => None,
        })
        .collect::<Vec<_>>();

    (!content_parts.is_empty()).then_some(serde_json::Value::Array(content_parts))
}

fn anthropic_message_content(parts: &[ContentPart]) -> Vec<serde_json::Value> {
    parts
        .iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } if !text.trim().is_empty() => Some(serde_json::json!({
                "type": "text",
                "text": text,
            })),
            ContentPart::Thinking { text, signature } if !text.trim().is_empty() => {
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
            ContentPart::RedactedThinking { data } if !data.trim().is_empty() => {
                Some(serde_json::json!({
                    "type": "redacted_thinking",
                    "data": data,
                }))
            }
            ContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => Some(anthropic_attachment_content_block(
                hash, mime_type, metadata,
            )),
            ContentPart::Structured { data } => Some(serde_json::json!({
                "type": "text",
                "text": data.to_string(),
            })),
            _ => None,
        })
        .collect()
}

fn build_responses_input_items(messages: &[MachineMessage]) -> Vec<serde_json::Value> {
    let mut input = Vec::new();

    for message in messages {
        match message {
            MachineMessage::Tool { responses } => {
                for response in responses {
                    input.push(serde_json::json!({
                        "type": "function_call_output",
                        "call_id": response.id,
                        "output": project_tool_response(&response.content),
                    }));
                }
            }
            MachineMessage::Assistant {
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
            MachineMessage::User { parts }
            | MachineMessage::System { parts }
            | MachineMessage::Context { parts } => {
                if let Some(content) = responses_message_content(parts) {
                    let role = if message.role() == MachineMessageRole::User {
                        "user"
                    } else {
                        "developer"
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

fn build_chat_completions_messages(messages: &[MachineMessage]) -> Vec<serde_json::Value> {
    let mut projected = Vec::new();

    for message in messages {
        match message {
            MachineMessage::Tool { responses } => {
                for response in responses {
                    projected.push(serde_json::json!({
                        "role": "tool",
                        "content": project_tool_response(&response.content),
                        "tool_call_id": response.id,
                    }));
                }
            }
            MachineMessage::Assistant {
                parts,
                tool_requests,
            } => {
                let mut message_value = serde_json::json!({ "role": "assistant" });
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
            MachineMessage::User { parts } => {
                if let Some(content) = chat_completions_message_content(parts) {
                    projected.push(serde_json::json!({
                        "role": "user",
                        "content": content,
                    }));
                }
            }
            MachineMessage::System { parts } | MachineMessage::Context { parts } => {
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

fn build_anthropic_messages(messages: &[MachineMessage]) -> Vec<serde_json::Value> {
    let mut projected = Vec::new();
    let mut known_tool_use_ids = std::collections::HashSet::new();

    for message in messages {
        match message {
            MachineMessage::Tool { responses } => {
                for response in responses {
                    let content = project_tool_response(&response.content);
                    let block = if known_tool_use_ids.contains(&response.id) {
                        Some(serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": response.id,
                            "content": content,
                        }))
                    } else if !content.trim().is_empty() {
                        Some(serde_json::json!({
                            "type": "text",
                            "text": content,
                        }))
                    } else {
                        None
                    };
                    if let Some(block) = block {
                        projected.push(serde_json::json!({
                            "role": "user",
                            "content": [block],
                        }));
                    }
                }
            }
            MachineMessage::Assistant {
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
            MachineMessage::User { parts } => {
                let blocks = anthropic_message_content(parts);
                if !blocks.is_empty() {
                    projected.push(serde_json::json!({
                        "role": "user",
                        "content": blocks,
                    }));
                }
            }
            MachineMessage::System { .. } | MachineMessage::Context { .. } => {}
        }
    }

    projected
}

#[cfg(test)]
#[path = "input_projection_tests.rs"]
mod tests;
