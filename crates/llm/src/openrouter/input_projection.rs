use anyhow::{Result, bail};
use openrouter_rs::api::chat::ContentPart;
use serde_json::Value;

use super::{OrMessage, Role, convert_tool_calls_for_openrouter};
use crate::{Message, MessageContentPart, MessageRole};

pub(super) fn is_openrouter_openai_reasoning_model(model: &str) -> bool {
    let lower = model.trim().to_ascii_lowercase();
    let Some(slug) = lower.strip_prefix("openai/") else {
        return false;
    };
    slug.starts_with("gpt-5")
        || slug == "o1"
        || slug.starts_with("o1-")
        || slug == "o3"
        || slug.starts_with("o3-")
        || slug == "o4"
        || slug.starts_with("o4-")
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "model-agnostic wrapper preserved during OpenRouter model-role projection rollout"
    )
)]
pub(super) fn convert_messages_for_openrouter(messages: Vec<Message>) -> Result<Vec<OrMessage>> {
    convert_messages_for_openrouter_model("", messages)
}

pub(super) fn convert_messages_for_openrouter_model(
    model: &str,
    messages: Vec<Message>,
) -> Result<Vec<OrMessage>> {
    let openai_reasoning = is_openrouter_openai_reasoning_model(model);
    let instruction_role = if openai_reasoning {
        Role::Developer
    } else {
        Role::System
    };
    let context_role = if openai_reasoning {
        Role::Developer
    } else {
        // Non-OpenAI providers (including Anthropic and strict chat templates)
        // treat mid/trailing `system` messages as invalid or cache-busting system hoists.
        Role::User
    };

    messages
        .into_iter()
        .map(|message| {
            let Message {
                role,
                content,
                content_parts,
                tool_calls,
                tool_call_id,
                ..
            } = message;

            Ok(match role {
                MessageRole::System => OrMessage::new(
                    instruction_role.clone(),
                    project_text(content, content_parts)?,
                ),
                MessageRole::User => project_user_message(content, content_parts)?,
                MessageRole::Assistant => {
                    let content = project_text(content, content_parts)?;
                    let tool_calls = tool_calls.map(convert_tool_calls_for_openrouter);
                    match tool_calls {
                        Some(tool_calls) if !tool_calls.is_empty() => {
                            OrMessage::assistant_with_tool_calls(content, tool_calls)
                        }
                        _ => OrMessage::new(Role::Assistant, content),
                    }
                }
                MessageRole::Tool => {
                    let content = project_text(content, content_parts)?;
                    let tool_call_id = tool_call_id
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "OpenRouter tool response messages require a non-empty tool_call_id"
                            )
                        })?;
                    OrMessage::tool_response(&tool_call_id, content)
                }
                MessageRole::Context => {
                    OrMessage::new(context_role.clone(), project_text(content, content_parts)?)
                }
            })
        })
        .collect()
}

/// User turns keep the plain-string wire shape unless they carry attachments, which
/// project to OpenAI-style multimodal parts.
fn project_user_message(fallback: String, parts: Vec<MessageContentPart>) -> Result<OrMessage> {
    if !parts
        .iter()
        .any(|part| matches!(part, MessageContentPart::Attachment { .. }))
    {
        return Ok(OrMessage::new(Role::User, project_text(fallback, parts)?));
    }

    let mut projected = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            MessageContentPart::Text { text } if text.trim().is_empty() => {}
            MessageContentPart::Text { text } => projected.push(ContentPart::text(text)),
            MessageContentPart::Structured { data } => {
                projected.push(ContentPart::text(data.to_string()))
            }
            MessageContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => projected.push(attachment_part(&hash, &mime_type, &metadata)?),
        }
    }
    Ok(OrMessage::with_parts(Role::User, projected))
}

fn attachment_part(hash: &str, mime_type: &str, metadata: &Value) -> Result<ContentPart> {
    let field = |key: &str| {
        metadata
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
    };
    if mime_type.starts_with("image/") {
        let url = if let Some(url) = field("image_url")
            .or_else(|| field("file_url"))
            .or_else(|| field("url"))
        {
            url.to_string()
        } else if let Some(raw) = field("file_data")
            .or_else(|| field("data"))
            .or_else(|| field("base64"))
        {
            to_data_url(mime_type, raw)
        } else {
            bail!(
                "OpenRouter cannot represent attachment `{hash}` ({mime_type}); provide an image URL, data URL, or base64 `data`"
            );
        };
        return Ok(match field("detail") {
            Some(detail) => ContentPart::image_url_with_detail(url, detail),
            None => ContentPart::image_url(url),
        });
    }

    bail!(
        "OpenRouter cannot represent attachment `{hash}` ({mime_type}); only image attachments are supported"
    );
}

fn to_data_url(mime_type: &str, raw: &str) -> String {
    if raw.starts_with("data:") {
        raw.to_string()
    } else {
        format!("data:{mime_type};base64,{raw}")
    }
}

fn project_text(fallback: String, parts: Vec<MessageContentPart>) -> Result<String> {
    if parts.is_empty() {
        return Ok(fallback);
    }

    let mut content = String::new();
    for part in parts {
        match part {
            MessageContentPart::Text { text } => content.push_str(&text),
            MessageContentPart::Structured { data } => content.push_str(&data.to_string()),
            MessageContentPart::Attachment {
                hash, mime_type, ..
            } => bail!(
                "OpenRouter accepts attachments only in user messages; cannot represent attachment `{hash}` ({mime_type}) in this role"
            ),
        }
    }

    Ok(if content.trim().is_empty() {
        fallback
    } else {
        content
    })
}

#[cfg(test)]
#[path = "input_projection_tests.rs"]
mod tests;
