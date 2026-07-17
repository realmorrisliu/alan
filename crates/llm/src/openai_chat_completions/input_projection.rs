use tracing::warn;

use crate::{Message as LlmMessage, MessageContentPart, MessageRole};

use super::{
    OpenAiChatCompletionsFunctionCall, OpenAiChatCompletionsToolCall,
    OpenAiResponsesFunctionCallItem, OpenAiResponsesFunctionCallOutputItem,
    OpenAiResponsesInputItem, OpenAiResponsesInputMessage, OpenAiResponsesReasoningInputItem,
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
    is_user_message: bool,
) -> anyhow::Result<Option<serde_json::Value>> {
    if parts.is_empty() {
        return Ok((is_user_message || !fallback.is_empty())
            .then_some(serde_json::Value::String(fallback)));
    }

    let mut projected = Vec::new();
    for part in parts {
        if let Some(part) = chat_completions_part(part, is_user_message)? {
            projected.push(part);
        }
    }
    Ok(if projected.is_empty() {
        (is_user_message || !fallback.is_empty()).then_some(serde_json::Value::String(fallback))
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

pub(super) fn openai_chat_completions_message_value(
    role: impl Into<String>,
    content: Option<serde_json::Value>,
    reasoning_content: Option<String>,
    reasoning: Option<serde_json::Value>,
    tool_calls: Option<Vec<OpenAiChatCompletionsToolCall>>,
    tool_call_id: Option<String>,
) -> serde_json::Value {
    let mut message = serde_json::Map::new();
    message.insert("role".to_string(), serde_json::Value::String(role.into()));
    if let Some(content) = content {
        message.insert("content".to_string(), content);
    }
    if let Some(reasoning_content) = reasoning_content {
        message.insert(
            "reasoning_content".to_string(),
            serde_json::Value::String(reasoning_content),
        );
    }
    if let Some(reasoning) = reasoning {
        message.insert("reasoning".to_string(), reasoning);
    }
    if let Some(tool_calls) = tool_calls {
        message.insert(
            "tool_calls".to_string(),
            serde_json::to_value(tool_calls).unwrap_or_else(|_| serde_json::Value::Array(vec![])),
        );
    }
    if let Some(tool_call_id) = tool_call_id {
        message.insert(
            "tool_call_id".to_string(),
            serde_json::Value::String(tool_call_id),
        );
    }
    serde_json::Value::Object(message)
}

pub(crate) fn convert_messages_for_openai_responses(
    messages: Vec<LlmMessage>,
) -> anyhow::Result<Vec<OpenAiResponsesInputItem>> {
    let mut input = Vec::new();

    for message in messages {
        match message.role {
            MessageRole::System | MessageRole::Context | MessageRole::User => {
                if let Some(content) = responses_content(message.content, message.content_parts)? {
                    let role = match message.role {
                        MessageRole::User => "user",
                        _ => "developer",
                    };
                    input.push(OpenAiResponsesInputItem::Message(
                        OpenAiResponsesInputMessage {
                            role: role.to_string(),
                            content,
                        },
                    ));
                }
            }
            MessageRole::Assistant => {
                if let Some(signature) = message
                    .thinking_signature
                    .filter(|value| is_non_empty(value))
                {
                    input.push(OpenAiResponsesInputItem::Reasoning(
                        OpenAiResponsesReasoningInputItem {
                            kind: "reasoning".to_string(),
                            encrypted_content: signature,
                        },
                    ));
                }

                if let Some(content) = responses_output(message.content, message.content_parts)? {
                    input.push(OpenAiResponsesInputItem::Message(
                        OpenAiResponsesInputMessage {
                            role: "assistant".to_string(),
                            content,
                        },
                    ));
                }

                if let Some(tool_calls) = message.tool_calls {
                    for tool_call in tool_calls {
                        let call_id = tool_call.id.unwrap_or_default();
                        if call_id.is_empty() {
                            warn!(
                                tool_name = %tool_call.name,
                                "Skipping assistant tool call without id in Responses API projection"
                            );
                            continue;
                        }

                        input.push(OpenAiResponsesInputItem::FunctionCall(
                            OpenAiResponsesFunctionCallItem {
                                kind: "function_call".to_string(),
                                call_id,
                                name: tool_call.name,
                                arguments: tool_call.arguments.to_string(),
                            },
                        ));
                    }
                }
            }
            MessageRole::Tool => {
                let output = plain_text_content(
                    message.content,
                    message.content_parts,
                    "OpenAI Responses tool output",
                )?;
                let Some(call_id) = message.tool_call_id.filter(|value| is_non_empty(value)) else {
                    warn!("Skipping tool message without tool_call_id in Responses API projection");
                    continue;
                };

                input.push(OpenAiResponsesInputItem::FunctionCallOutput(
                    OpenAiResponsesFunctionCallOutputItem {
                        kind: "function_call_output".to_string(),
                        call_id,
                        output,
                    },
                ));
            }
        }
    }

    Ok(input)
}

pub(super) fn is_non_empty(value: &str) -> bool {
    !value.trim().is_empty()
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
    fn empty_chat_user_message_preserves_required_content_field() {
        let projected = convert_messages_for_openai_chat_completions_with_instruction_role(
            vec![crate::Message::user("")],
            "developer",
        )
        .unwrap();

        assert_eq!(projected, vec![json!({"role": "user", "content": ""})]);
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
