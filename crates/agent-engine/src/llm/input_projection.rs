use super::{Message, MessageRole, ToolCall};
use crate::agent_machine::{Message as MachineMessage, MessageRole as MachineMessageRole};
use crate::tape::ContentPart;

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

#[cfg(test)]
#[path = "input_projection_tests.rs"]
mod tests;
