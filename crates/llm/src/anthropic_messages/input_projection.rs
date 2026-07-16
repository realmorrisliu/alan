use std::collections::HashSet;

use crate::{Message as LlmMessage, MessageContentPart, MessageRole};

use super::{AnthropicMessagesMessage, ContentBlockInput, is_non_empty};

pub(super) fn convert_messages_for_anthropic_messages(
    messages: Vec<LlmMessage>,
    system_prompt: Option<String>,
) -> anyhow::Result<(Vec<AnthropicMessagesMessage>, Option<String>)> {
    let mut converted = Vec::new();
    let mut known_tool_use_ids = HashSet::new();
    let mut system_parts = system_prompt
        .filter(|content| is_non_empty(content))
        .into_iter()
        .collect::<Vec<_>>();

    for message in messages {
        let LlmMessage {
            role,
            content,
            content_parts,
            thinking,
            thinking_signature,
            redacted_thinking,
            tool_calls,
            tool_call_id,
        } = message;

        let (role, content) = match role {
            MessageRole::User => ("user", content_blocks(content, content_parts, true)?),
            MessageRole::Assistant => {
                let mut blocks = Vec::new();

                if let Some(thinking) = thinking.filter(|content| is_non_empty(content)) {
                    blocks.push(ContentBlockInput::Thinking {
                        thinking,
                        signature: thinking_signature.filter(|signature| is_non_empty(signature)),
                    });
                }

                if let Some(redacted_blocks) = redacted_thinking {
                    for data in redacted_blocks
                        .into_iter()
                        .filter(|data| is_non_empty(data))
                    {
                        blocks.push(ContentBlockInput::RedactedThinking { data });
                    }
                }

                blocks.extend(content_blocks(content, content_parts, false)?);

                if let Some(calls) = tool_calls {
                    for call in calls {
                        if let Some(id) = call.id.filter(|id| is_non_empty(id)) {
                            known_tool_use_ids.insert(id.clone());
                            blocks.push(ContentBlockInput::ToolUse {
                                id,
                                name: call.name,
                                input: call.arguments,
                            });
                        }
                    }
                }

                ("assistant", blocks)
            }
            MessageRole::Tool => {
                let content =
                    plain_text_content(content, content_parts, "Anthropic Messages tool results")?;
                let blocks = if let Some(tool_use_id) = tool_call_id.filter(|id| is_non_empty(id)) {
                    if known_tool_use_ids.contains(&tool_use_id) {
                        vec![ContentBlockInput::ToolResult {
                            tool_use_id,
                            content,
                            is_error: None,
                        }]
                    } else {
                        text_block(content).into_iter().collect()
                    }
                } else {
                    text_block(content).into_iter().collect()
                };
                ("user", blocks)
            }
            MessageRole::System | MessageRole::Context => {
                let content = plain_text_content(
                    content,
                    content_parts,
                    "Anthropic Messages system instructions",
                )?;
                if is_non_empty(&content) {
                    system_parts.push(content);
                }
                continue;
            }
        };

        if !content.is_empty() {
            converted.push(AnthropicMessagesMessage {
                role: role.to_string(),
                content,
            });
        }
    }

    let system_prompt = (!system_parts.is_empty()).then(|| system_parts.join("\n"));
    Ok((converted, system_prompt))
}

pub(super) fn content_blocks(
    fallback: String,
    parts: Vec<MessageContentPart>,
    allow_attachments: bool,
) -> anyhow::Result<Vec<ContentBlockInput>> {
    if parts.is_empty() {
        return Ok((!fallback.is_empty())
            .then_some(ContentBlockInput::Text { text: fallback })
            .into_iter()
            .collect());
    }

    let mut blocks = Vec::new();
    for part in parts {
        if let Some(block) = content_block(part, allow_attachments)? {
            blocks.push(block);
        }
    }
    Ok(if blocks.is_empty() {
        text_block(fallback).into_iter().collect()
    } else {
        blocks
    })
}

fn content_block(
    part: MessageContentPart,
    allow_attachments: bool,
) -> anyhow::Result<Option<ContentBlockInput>> {
    Ok(match part {
        MessageContentPart::Text { text } => {
            (!text.trim().is_empty()).then_some(ContentBlockInput::Text { text })
        }
        MessageContentPart::Structured { data } => Some(ContentBlockInput::Text {
            text: data.to_string(),
        }),
        MessageContentPart::Attachment {
            hash,
            mime_type,
            metadata,
        } => {
            if !allow_attachments {
                anyhow::bail!(
                    "Anthropic Messages accepts attachments only in user messages; cannot represent attachment `{hash}` ({mime_type}) in this role"
                );
            }
            Some(attachment_block(hash, mime_type, metadata)?)
        }
    })
}

fn plain_text_content(
    fallback: String,
    parts: Vec<MessageContentPart>,
    context: &str,
) -> anyhow::Result<String> {
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
            } => anyhow::bail!("{context} cannot represent attachment `{hash}` ({mime_type})"),
        }
    }
    Ok(if !is_non_empty(&content) {
        fallback
    } else {
        content
    })
}

fn text_block(content: String) -> Option<ContentBlockInput> {
    is_non_empty(&content).then_some(ContentBlockInput::Text { text: content })
}

fn attachment_block(
    hash: String,
    mime_type: String,
    metadata: serde_json::Value,
) -> anyhow::Result<ContentBlockInput> {
    let source = metadata
        .get("file_id")
        .and_then(serde_json::Value::as_str)
        .map(|file_id| serde_json::json!({"type": "file", "file_id": file_id}))
        .or_else(|| {
            attachment_url(&metadata).map(|url| serde_json::json!({"type": "url", "url": url}))
        });

    let Some(source) = source else {
        anyhow::bail!(
            "Anthropic Messages cannot represent attachment `{hash}` ({mime_type}); use an uploaded `file_id` or URL"
        );
    };

    Ok(if mime_type.starts_with("image/") {
        ContentBlockInput::Image { source }
    } else {
        ContentBlockInput::Document {
            source,
            title: metadata
                .get("title")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            citations: None,
        }
    })
}

fn attachment_url(metadata: &serde_json::Value) -> Option<&str> {
    metadata
        .get("file_url")
        .or_else(|| metadata.get("image_url"))
        .or_else(|| metadata.get("url"))
        .and_then(serde_json::Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projects_document_file_input() {
        let blocks = content_blocks(
            String::new(),
            vec![
                MessageContentPart::Text {
                    text: " ".to_string(),
                },
                MessageContentPart::Attachment {
                    hash: "doc_hash".to_string(),
                    mime_type: "application/pdf".to_string(),
                    metadata: json!({"file_id": "file_123", "title": "Spec"}),
                },
            ],
            true,
        )
        .unwrap();

        assert_eq!(
            serde_json::to_value(blocks).unwrap(),
            json!([{
                "type": "document",
                "source": {"type": "file", "file_id": "file_123"},
                "title": "Spec"
            }])
        );
    }

    #[test]
    fn rejects_unreferenced_attachment() {
        let parts = vec![MessageContentPart::Attachment {
            hash: "doc_hash".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: json!({}),
        }];

        let error = content_blocks(String::new(), parts, true).unwrap_err();
        assert!(error.to_string().contains("cannot represent attachment"));
    }

    #[test]
    fn projects_typed_system_and_tool_content() {
        let mut system = LlmMessage::system("");
        system.content_parts = vec![MessageContentPart::Structured {
            data: json!({"policy": "strict"}),
        }];
        let mut tool = LlmMessage::tool("missing", "");
        tool.content_parts = vec![
            MessageContentPart::Text {
                text: "result: ".to_string(),
            },
            MessageContentPart::Structured {
                data: json!({"ok": true}),
            },
        ];

        let (messages, system_prompt) =
            convert_messages_for_anthropic_messages(vec![system, tool], Some("base".to_string()))
                .unwrap();

        assert_eq!(
            system_prompt.as_deref(),
            Some("base\n{\"policy\":\"strict\"}")
        );
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            json!([{
                "role": "user",
                "content": [{"type": "text", "text": "result: {\"ok\":true}"}]
            }])
        );
    }

    #[test]
    fn rejects_attachment_outside_user_messages() {
        let mut assistant = LlmMessage::assistant("");
        assistant.content_parts = vec![MessageContentPart::Attachment {
            hash: "doc_hash".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: json!({"file_id": "file_123"}),
        }];

        let error = convert_messages_for_anthropic_messages(vec![assistant], None).unwrap_err();

        assert!(error.to_string().contains("only in user messages"));
    }

    #[test]
    fn falls_back_when_typed_text_projects_to_nothing() {
        let blocks = content_blocks(
            "fallback".to_string(),
            vec![MessageContentPart::Text {
                text: "   ".to_string(),
            }],
            true,
        )
        .unwrap();

        assert_eq!(
            serde_json::to_value(blocks).unwrap(),
            json!([{"type": "text", "text": "fallback"}])
        );
    }
}
