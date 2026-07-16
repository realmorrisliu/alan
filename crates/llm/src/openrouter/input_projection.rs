use anyhow::{Result, bail};

use super::{OrMessage, Role, convert_tool_calls_for_openrouter};
use crate::{Message, MessageContentPart, MessageRole};

pub(super) fn convert_messages_for_openrouter(messages: Vec<Message>) -> Result<Vec<OrMessage>> {
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
            let content = project_content(content, content_parts)?;

            Ok(match role {
                MessageRole::System => OrMessage::new(Role::System, content),
                MessageRole::User => OrMessage::new(Role::User, content),
                MessageRole::Assistant => {
                    let tool_calls = tool_calls.map(convert_tool_calls_for_openrouter);
                    match tool_calls {
                        Some(tool_calls) if !tool_calls.is_empty() => {
                            OrMessage::assistant_with_tool_calls(content, tool_calls)
                        }
                        _ => OrMessage::new(Role::Assistant, content),
                    }
                }
                MessageRole::Tool => {
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
                MessageRole::Context => OrMessage::new(Role::System, content),
            })
        })
        .collect()
}

fn project_content(fallback: String, parts: Vec<MessageContentPart>) -> Result<String> {
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
                "OpenRouter cannot represent attachment `{hash}` ({mime_type}); select a Connection with attachment support"
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projects_typed_text_and_structured_content() {
        let mut message = Message::user("");
        message.content_parts = vec![
            MessageContentPart::Text {
                text: "selection: ".to_string(),
            },
            MessageContentPart::Structured {
                data: json!({"choice": "approve"}),
            },
        ];

        let projected = convert_messages_for_openrouter(vec![message]).unwrap();
        let value = serde_json::to_value(&projected[0]).unwrap();

        assert_eq!(value["content"], "selection: {\"choice\":\"approve\"}");
    }

    #[test]
    fn rejects_attachments_before_dispatch() {
        let mut message = Message::user("");
        message.content_parts = vec![MessageContentPart::Attachment {
            hash: "doc_hash".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: json!({"file_id": "file_123"}),
        }];

        let error = convert_messages_for_openrouter(vec![message]).unwrap_err();

        assert!(error.to_string().contains("cannot represent attachment"));
    }

    #[test]
    fn falls_back_when_typed_text_projects_to_nothing() {
        let content = project_content(
            "fallback".to_string(),
            vec![MessageContentPart::Text {
                text: "   ".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(content, "fallback");
    }
}
