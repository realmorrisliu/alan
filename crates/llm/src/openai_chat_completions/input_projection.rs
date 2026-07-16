use crate::MessageContentPart;

use super::{
    LlmMessage, MessageRole, OpenAiChatCompletionsFunctionCall, OpenAiChatCompletionsToolCall,
    is_non_empty, openai_chat_completions_message_value,
};

pub(super) fn convert_messages_for_openai_chat_completions_with_instruction_role(
    messages: Vec<LlmMessage>,
    instruction_role: &'static str,
) -> anyhow::Result<Vec<serde_json::Value>> {
    messages
        .into_iter()
        .map(|message| {
            let LlmMessage {
                role,
                content,
                content_parts,
                thinking,
                thinking_signature,
                redacted_thinking: _,
                tool_calls,
                tool_call_id,
            } = message;
            let allow_attachments = role == MessageRole::User;
            let role = match role {
                MessageRole::System => instruction_role,
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
                MessageRole::Context => instruction_role,
            };

            let tool_calls = tool_calls.map(|calls| {
                calls
                    .into_iter()
                    .map(|call| OpenAiChatCompletionsToolCall {
                        id: call.id.unwrap_or_default(),
                        r#type: "function".to_string(),
                        function: OpenAiChatCompletionsFunctionCall {
                            name: call.name,
                            arguments: call.arguments.to_string(),
                        },
                    })
                    .collect()
            });

            let reasoning_content = if role == "assistant" {
                thinking.filter(|value| is_non_empty(value))
            } else {
                None
            };
            let reasoning = if role == "assistant" {
                thinking_signature
                    .filter(|value| is_non_empty(value))
                    .map(|signature| serde_json::json!({ "encrypted_content": signature }))
            } else {
                None
            };

            Ok(openai_chat_completions_message_value(
                role,
                chat_completions_content(content, content_parts, allow_attachments)?,
                reasoning_content,
                reasoning,
                tool_calls,
                tool_call_id,
            ))
        })
        .collect()
}

pub(super) fn chat_completions_content(
    fallback: String,
    parts: Vec<MessageContentPart>,
    allow_attachments: bool,
) -> anyhow::Result<Option<serde_json::Value>> {
    if parts.is_empty() {
        return Ok((!fallback.is_empty()).then_some(serde_json::Value::String(fallback)));
    }

    let mut projected = Vec::new();
    for part in parts {
        if let Some(part) = chat_completions_part(part, allow_attachments)? {
            projected.push(part);
        }
    }
    Ok(if projected.is_empty() {
        (!fallback.is_empty()).then_some(serde_json::Value::String(fallback))
    } else {
        Some(serde_json::Value::Array(projected))
    })
}

pub(super) fn responses_content(
    fallback: String,
    parts: Vec<MessageContentPart>,
) -> anyhow::Result<Option<serde_json::Value>> {
    if parts.is_empty() {
        return Ok((!fallback.is_empty()).then_some(serde_json::Value::String(fallback)));
    }

    let mut projected = Vec::new();
    for part in parts {
        if let Some(part) = responses_part(part)? {
            projected.push(part);
        }
    }
    Ok(if projected.is_empty() {
        (!fallback.is_empty()).then_some(serde_json::Value::String(fallback))
    } else {
        Some(serde_json::Value::Array(projected))
    })
}

pub(super) fn responses_output(
    fallback: String,
    parts: Vec<MessageContentPart>,
) -> anyhow::Result<Option<serde_json::Value>> {
    let content = plain_text_content(fallback, parts, "OpenAI Responses assistant messages")?;
    Ok(is_non_empty(&content).then_some(serde_json::Value::String(content)))
}

fn chat_completions_part(
    part: MessageContentPart,
    allow_attachments: bool,
) -> anyhow::Result<Option<serde_json::Value>> {
    Ok(match part {
        MessageContentPart::Text { text } => {
            (!text.trim().is_empty()).then(|| serde_json::json!({"type": "text", "text": text}))
        }
        MessageContentPart::Structured { data } => Some(serde_json::json!({
            "type": "text",
            "text": data.to_string()
        })),
        MessageContentPart::Attachment {
            hash,
            mime_type,
            metadata,
        } => {
            if !allow_attachments {
                anyhow::bail!(
                    "OpenAI Chat Completions accepts attachments only in user messages; cannot represent attachment `{hash}` ({mime_type}) in this role"
                );
            }
            if mime_type.starts_with("image/")
                && let Some(url) = attachment_url(&metadata)
            {
                return Ok(Some(serde_json::json!({
                    "type": "image_url",
                    "image_url": {"url": url}
                })));
            }
            if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
                return Ok(Some(serde_json::json!({
                    "type": "file",
                    "file": {"file_id": file_id}
                })));
            }
            if let Some((file_data, filename)) = metadata
                .get("file_data")
                .and_then(serde_json::Value::as_str)
                .zip(metadata.get("filename").and_then(serde_json::Value::as_str))
            {
                return Ok(Some(serde_json::json!({
                    "type": "file",
                    "file": {"file_data": file_data, "filename": filename}
                })));
            }
            anyhow::bail!(
                "OpenAI Chat Completions cannot represent attachment `{hash}` ({mime_type}); use an image URL, uploaded `file_id`, or `file_data` with `filename`"
            );
        }
    })
}

pub(super) fn plain_text_content(
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

fn responses_part(part: MessageContentPart) -> anyhow::Result<Option<serde_json::Value>> {
    Ok(match part {
        MessageContentPart::Text { text } => (!text.trim().is_empty())
            .then(|| serde_json::json!({"type": "input_text", "text": text})),
        MessageContentPart::Structured { data } => Some(serde_json::json!({
            "type": "input_text",
            "text": data.to_string()
        })),
        MessageContentPart::Attachment {
            hash,
            mime_type,
            metadata,
        } => {
            if mime_type.starts_with("image/") {
                if let Some(url) = attachment_url(&metadata) {
                    return Ok(Some(
                        serde_json::json!({"type": "input_image", "image_url": url}),
                    ));
                }
                if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
                    return Ok(Some(
                        serde_json::json!({"type": "input_image", "file_id": file_id}),
                    ));
                }
                anyhow::bail!(
                    "OpenAI Responses cannot represent image attachment `{hash}` ({mime_type}); use an image URL or uploaded `file_id`"
                );
            }
            if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
                return Ok(Some(serde_json::json!({
                    "type": "input_file",
                    "file_id": file_id
                })));
            }
            if let Some(url) = attachment_url(&metadata) {
                return Ok(Some(serde_json::json!({
                    "type": "input_file",
                    "file_url": url
                })));
            }
            if let Some((file_data, filename)) = metadata
                .get("file_data")
                .and_then(serde_json::Value::as_str)
                .zip(metadata.get("filename").and_then(serde_json::Value::as_str))
            {
                return Ok(Some(serde_json::json!({
                    "type": "input_file",
                    "file_data": file_data,
                    "filename": filename
                })));
            }
            anyhow::bail!(
                "OpenAI Responses cannot represent document attachment `{hash}` ({mime_type}); use a URL, uploaded `file_id`, or `file_data` with `filename`"
            );
        }
    })
}

fn attachment_url(metadata: &serde_json::Value) -> Option<&str> {
    metadata
        .get("image_url")
        .or_else(|| metadata.get("file_url"))
        .or_else(|| metadata.get("url"))
        .and_then(serde_json::Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn image_parts() -> Vec<MessageContentPart> {
        vec![
            MessageContentPart::Text {
                text: "   ".to_string(),
            },
            MessageContentPart::Text {
                text: "Describe this image".to_string(),
            },
            MessageContentPart::Attachment {
                hash: "img_hash".to_string(),
                mime_type: "image/png".to_string(),
                metadata: json!({"image_url": "https://example.com/cat.png"}),
            },
        ]
    }

    #[test]
    fn projects_responses_image_input() {
        assert_eq!(
            responses_content(String::new(), image_parts()).unwrap(),
            Some(json!([
                {"type": "input_text", "text": "Describe this image"},
                {"type": "input_image", "image_url": "https://example.com/cat.png"}
            ]))
        );
    }

    #[test]
    fn projects_chat_completions_image_input() {
        assert_eq!(
            chat_completions_content(String::new(), image_parts(), true).unwrap(),
            Some(json!([
                {"type": "text", "text": "Describe this image"},
                {"type": "image_url", "image_url": {"url": "https://example.com/cat.png"}}
            ]))
        );
    }

    #[test]
    fn projects_inline_document_input() {
        let parts = || {
            vec![MessageContentPart::Attachment {
                hash: "doc_hash".to_string(),
                mime_type: "application/pdf".to_string(),
                metadata: json!({"file_data": "data:application/pdf;base64,AA==", "filename": "spec.pdf"}),
            }]
        };

        assert_eq!(
            chat_completions_content(String::new(), parts(), true).unwrap(),
            Some(json!([{
                "type": "file",
                "file": {"file_data": "data:application/pdf;base64,AA==", "filename": "spec.pdf"}
            }]))
        );
        assert_eq!(
            responses_content(String::new(), parts()).unwrap(),
            Some(json!([{
                "type": "input_file",
                "file_data": "data:application/pdf;base64,AA==",
                "filename": "spec.pdf"
            }]))
        );
    }

    #[test]
    fn falls_back_when_typed_parts_project_to_nothing() {
        let parts = || {
            vec![MessageContentPart::Text {
                text: "   ".to_string(),
            }]
        };

        assert_eq!(
            responses_content("fallback".to_string(), parts()).unwrap(),
            Some(json!("fallback"))
        );
        assert_eq!(
            chat_completions_content("fallback".to_string(), parts(), true).unwrap(),
            Some(json!("fallback"))
        );

        let mut message = crate::Message::user("");
        message.content_parts = parts();
        assert!(
            super::super::convert_messages_for_openai_responses(vec![message])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn projects_typed_assistant_parts_as_plain_output_content() {
        let mut message = crate::Message::assistant("");
        message.content_parts = vec![
            MessageContentPart::Text {
                text: "answer: ".to_string(),
            },
            MessageContentPart::Structured {
                data: json!({"ok": true}),
            },
        ];

        let projected = super::super::convert_messages_for_openai_responses(vec![message]).unwrap();

        match &projected[0] {
            super::super::OpenAiResponsesInputItem::Message(message) => {
                assert_eq!(message.role, "assistant");
                assert_eq!(message.content, json!("answer: {\"ok\":true}"));
            }
            _ => panic!("expected assistant message"),
        }
    }

    #[test]
    fn rejects_url_backed_chat_completions_document() {
        let parts = vec![MessageContentPart::Attachment {
            hash: "doc_hash".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: json!({"file_url": "https://example.com/spec.pdf"}),
        }];

        let error = chat_completions_content(String::new(), parts, true).unwrap_err();
        assert!(error.to_string().contains("file_id"));
        assert!(error.to_string().contains("file_data"));
    }

    #[test]
    fn rejects_unreferenced_responses_document() {
        let parts = vec![MessageContentPart::Attachment {
            hash: "doc_hash".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: json!({}),
        }];

        let error = responses_content(String::new(), parts).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot represent document attachment")
        );
    }

    #[test]
    fn rejects_attachment_outside_chat_user_message() {
        let error = chat_completions_content(String::new(), image_parts(), false).unwrap_err();
        assert!(error.to_string().contains("only in user messages"));
    }

    #[test]
    fn plain_text_projection_falls_back_for_blank_typed_parts() {
        let content = plain_text_content(
            "fallback".to_string(),
            vec![MessageContentPart::Text {
                text: "   ".to_string(),
            }],
            "test",
        )
        .unwrap();

        assert_eq!(content, "fallback");
    }
}
