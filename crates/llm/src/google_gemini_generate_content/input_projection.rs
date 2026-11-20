//! Projection from provider-neutral [`Message`] sequences to Vertex AI
//! `GenerateContent` `contents` and `systemInstruction`.

use std::collections::HashMap;

use anyhow::{Result, bail};
use serde_json::Value;

use super::signatures::{SKIP_SIGNATURE_VALIDATOR, TurnSignatures, model_requires_call_signatures};
use super::wire::{Content, FunctionCall, FunctionResponse, Part};
use crate::{Message, MessageContentPart, MessageRole};

#[derive(Debug)]
pub(super) struct ProjectedRequestInput {
    pub(super) contents: Vec<Content>,
    pub(super) system_instruction: Option<Content>,
}

pub(super) fn project_request_input(
    model: &str,
    system_prompt: Option<&str>,
    messages: &[Message],
) -> Result<ProjectedRequestInput> {
    let mut contents = Vec::new();
    let mut system_parts = system_prompt
        .map(Part::text)
        .into_iter()
        .collect::<Vec<_>>();
    let mut call_id_to_name: HashMap<String, String> = HashMap::new();
    let mut pending_tool_responses: Vec<Part> = Vec::new();

    for message in messages {
        if message.role != MessageRole::Tool && !pending_tool_responses.is_empty() {
            contents.push(Content::user(std::mem::take(&mut pending_tool_responses)));
        }

        match message.role {
            MessageRole::User | MessageRole::Context => {
                let parts = project_user_parts(&message.content, &message.content_parts)?;
                contents.push(Content::user(parts));
            }
            MessageRole::Assistant => {
                let parts = project_assistant_parts(model, message, &mut call_id_to_name)?;
                contents.push(Content::model(parts));
            }
            MessageRole::Tool => {
                let Some(raw_id) = message
                    .tool_call_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                let content = project_plain_text(
                    &message.content,
                    &message.content_parts,
                    "Google Gemini GenerateContent tool results",
                )?;
                let (id, name) = match call_id_to_name.get(raw_id) {
                    Some(function_name) => (Some(raw_id.to_string()), function_name.clone()),
                    None => (None, raw_id.to_string()),
                };
                let response = match serde_json::from_str::<Value>(&content) {
                    Ok(Value::Object(map)) => Value::Object(map),
                    Ok(other) => serde_json::json!({ "result": other }),
                    Err(_) => serde_json::json!({ "result": content }),
                };
                pending_tool_responses.push(Part {
                    function_response: Some(FunctionResponse { id, name, response }),
                    ..Part::default()
                });
            }
            MessageRole::System => {
                let text = project_plain_text(
                    &message.content,
                    &message.content_parts,
                    "Google Gemini GenerateContent system instructions",
                )?;
                system_parts.push(Part::text(text));
            }
        }
    }

    if !pending_tool_responses.is_empty() {
        contents.push(Content::user(pending_tool_responses));
    }

    let system_instruction = (!system_parts.is_empty()).then_some(Content {
        role: None,
        parts: system_parts,
    });
    Ok(ProjectedRequestInput {
        contents,
        system_instruction,
    })
}

fn project_assistant_parts(
    model: &str,
    message: &Message,
    call_id_to_name: &mut HashMap<String, String>,
) -> Result<Vec<Part>> {
    let signatures = TurnSignatures::decode(message.thinking_signature.as_deref());
    let mut parts = Vec::new();

    if let Some(thinking) = message
        .thinking
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        parts.push(Part {
            text: Some(thinking.to_string()),
            thought: Some(true),
            ..Part::default()
        });
    }

    let text = project_plain_text(
        &message.content,
        &message.content_parts,
        "Google Gemini GenerateContent assistant messages",
    )?;
    if !text.is_empty() {
        parts.push(Part::text(text));
    }

    if let Some(text_sig) = signatures.text() {
        if let Some(last_text) = parts
            .iter_mut()
            .rev()
            .find(|part| part.text.is_some() && !part.is_thought())
        {
            last_text.thought_signature = Some(text_sig.to_string());
        } else if let Some(last_thought) = parts.iter_mut().rev().find(|part| part.is_thought()) {
            last_thought.thought_signature = Some(text_sig.to_string());
        }
    }

    if let Some(calls) = message
        .tool_calls
        .as_ref()
        .filter(|calls| !calls.is_empty())
    {
        let needs_fallback =
            !signatures.has_any_call_signature() && model_requires_call_signatures(model);
        for (ordinal, call) in calls.iter().enumerate() {
            if let Some(id) = call
                .id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                call_id_to_name.insert(id.to_string(), call.name.clone());
            }
            let thought_signature =
                signatures
                    .call(ordinal)
                    .map(ToString::to_string)
                    .or_else(|| {
                        (ordinal == 0 && needs_fallback)
                            .then(|| SKIP_SIGNATURE_VALIDATOR.to_string())
                    });
            parts.push(Part {
                function_call: Some(FunctionCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    args: call.arguments.clone(),
                    partial_args: None,
                    will_continue: None,
                }),
                thought_signature,
                ..Part::default()
            });
        }
    }

    if parts.is_empty() {
        parts.push(Part::text(message.content.clone()));
    }

    Ok(parts)
}

fn project_user_parts(fallback: &str, parts: &[MessageContentPart]) -> Result<Vec<Part>> {
    if parts.is_empty() {
        return Ok(vec![Part::text(fallback)]);
    }

    let mut projected = Vec::new();
    let mut text_buffer = String::new();

    for part in parts {
        match part {
            MessageContentPart::Text { text } => text_buffer.push_str(text),
            MessageContentPart::Structured { data } => text_buffer.push_str(&data.to_string()),
            MessageContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => {
                if !text_buffer.trim().is_empty() {
                    projected.push(Part::text(std::mem::take(&mut text_buffer)));
                } else {
                    text_buffer.clear();
                }
                projected.push(project_attachment(hash, mime_type, metadata)?);
            }
        }
    }

    if !text_buffer.trim().is_empty() {
        projected.push(Part::text(text_buffer));
    }

    if projected.is_empty() {
        projected.push(Part::text(fallback));
    }

    Ok(projected)
}

fn project_attachment(hash: &str, mime_type: &str, metadata: &Value) -> Result<Part> {
    let field = |key: &str| {
        metadata
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };

    if let Some(raw) = field("inline_data")
        .or_else(|| field("file_data"))
        .or_else(|| field("data"))
        .or_else(|| field("base64"))
    {
        return if raw.starts_with("data:") {
            parse_data_uri(hash, mime_type, raw)
        } else {
            Ok(Part::inline_data(mime_type, raw))
        };
    }

    if let Some(uri) = field("file_uri")
        .or_else(|| field("file_url"))
        .or_else(|| field("image_url"))
        .or_else(|| field("url"))
    {
        if uri.starts_with("data:") {
            return parse_data_uri(hash, mime_type, uri);
        }
        if uri.starts_with("gs://") || uri.starts_with("https://") || uri.starts_with("http://") {
            return Ok(Part::file_data(mime_type, uri));
        }
    }

    bail!(
        "Google Gemini GenerateContent cannot represent attachment `{hash}` ({mime_type}); provide base64 `data`/`file_data`, a `data:` URI, or a `file_uri`/`url`"
    )
}

fn parse_data_uri(hash: &str, fallback_mime: &str, uri: &str) -> Result<Part> {
    let Some(rest) = uri.strip_prefix("data:") else {
        bail!(
            "Google Gemini GenerateContent cannot represent attachment `{hash}` ({fallback_mime}): invalid data URI"
        );
    };
    let Some((header, payload)) = rest.split_once(',') else {
        bail!(
            "Google Gemini GenerateContent cannot represent attachment `{hash}` ({fallback_mime}): malformed data URI"
        );
    };
    let Some(mime_prefix) = header.strip_suffix(";base64") else {
        bail!(
            "Google Gemini GenerateContent cannot represent attachment `{hash}` ({fallback_mime}): data URI must be base64-encoded"
        );
    };
    let data = payload.trim();
    if data.is_empty() {
        bail!(
            "Google Gemini GenerateContent cannot represent attachment `{hash}` ({fallback_mime}): empty inline data"
        );
    }
    let resolved_mime = if mime_prefix.is_empty() {
        fallback_mime
    } else {
        mime_prefix
    };
    Ok(Part::inline_data(resolved_mime, data))
}

fn project_plain_text(
    fallback: &str,
    parts: &[MessageContentPart],
    context: &str,
) -> Result<String> {
    if parts.is_empty() {
        return Ok(fallback.to_string());
    }

    let mut content = String::new();
    for part in parts {
        match part {
            MessageContentPart::Text { text } => content.push_str(text),
            MessageContentPart::Structured { data } => content.push_str(&data.to_string()),
            MessageContentPart::Attachment {
                hash, mime_type, ..
            } => bail!(
                "{context} accepts attachments only in user messages; cannot represent attachment `{hash}` ({mime_type})"
            ),
        }
    }

    Ok(if content.trim().is_empty() {
        fallback.to_string()
    } else {
        content
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolCall;
    use serde_json::json;

    #[test]
    fn projects_typed_content_and_system_messages() {
        let mut user = Message::user("");
        user.content_parts = vec![
            MessageContentPart::Text {
                text: "selection: ".to_string(),
            },
            MessageContentPart::Structured {
                data: json!({"choice": "approve"}),
            },
        ];
        let mut system = Message::system("");
        system.content_parts = vec![MessageContentPart::Structured {
            data: json!({"policy": "strict"}),
        }];

        let projected =
            project_request_input("gemini-2.5-pro", Some("base"), &[system, user]).unwrap();

        assert_eq!(
            projected.contents[0].parts[0].text.as_deref(),
            Some("selection: {\"choice\":\"approve\"}")
        );
        let system_parts = &projected.system_instruction.unwrap().parts;
        assert_eq!(system_parts[0].text.as_deref(), Some("base"));
        assert_eq!(
            system_parts[1].text.as_deref(),
            Some("{\"policy\":\"strict\"}")
        );
    }

    #[test]
    fn projects_inline_base64_and_data_uri_attachments() {
        let mut user = Message::user("");
        user.content_parts = vec![
            MessageContentPart::Text {
                text: "inspect ".to_string(),
            },
            MessageContentPart::Attachment {
                hash: "img1".to_string(),
                mime_type: "image/png".to_string(),
                metadata: json!({"data": "iVBORw0KGgo="}),
            },
            MessageContentPart::Attachment {
                hash: "doc1".to_string(),
                mime_type: "application/pdf".to_string(),
                metadata: json!({"file_data": "data:application/pdf;base64,JVBERi0="}),
            },
            MessageContentPart::Attachment {
                hash: "gcs1".to_string(),
                mime_type: "image/jpeg".to_string(),
                metadata: json!({"file_uri": "gs://bucket/photo.jpg"}),
            },
        ];

        let projected = project_request_input("gemini-2.5-pro", None, &[user]).unwrap();
        let parts = &projected.contents[0].parts;
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0].text.as_deref(), Some("inspect "));
        assert_eq!(
            parts[1]
                .inline_data
                .as_ref()
                .map(|d| (&d.mime_type[..], &d.data[..])),
            Some(("image/png", "iVBORw0KGgo="))
        );
        assert_eq!(
            parts[2]
                .inline_data
                .as_ref()
                .map(|d| (&d.mime_type[..], &d.data[..])),
            Some(("application/pdf", "JVBERi0="))
        );
        assert_eq!(
            parts[3]
                .file_data
                .as_ref()
                .map(|d| (&d.mime_type[..], &d.file_uri[..])),
            Some(("image/jpeg", "gs://bucket/photo.jpg"))
        );
    }

    #[test]
    fn rejects_unreferenced_or_file_id_attachments_before_dispatch() {
        let mut message = Message::user("");
        message.content_parts = vec![MessageContentPart::Attachment {
            hash: "doc_hash".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: json!({"file_id": "file_123"}),
        }];

        let error = project_request_input("gemini-2.5-pro", None, &[message]).unwrap_err();
        assert!(error.to_string().contains("cannot represent attachment"));
    }

    #[test]
    fn rejects_attachments_in_assistant_messages() {
        let mut message = Message::assistant("");
        message.content_parts = vec![MessageContentPart::Attachment {
            hash: "img".to_string(),
            mime_type: "image/png".to_string(),
            metadata: json!({"data": "AA=="}),
        }];

        let error = project_request_input("gemini-2.5-pro", None, &[message]).unwrap_err();
        assert!(error.to_string().contains("only in user messages"));
    }

    #[test]
    fn replays_function_calls_thought_signatures_and_groups_tool_responses() {
        let mut sigs = TurnSignatures::default();
        sigs.record_call(0, Some("sig-call-0"));

        let mut assistant = Message::assistant_with_tools(
            "",
            vec![
                ToolCall::new("read_file", json!({"path": "a.rs"})).with_id("call_1"),
                ToolCall::new("read_file", json!({"path": "b.rs"})).with_id("call_2"),
            ],
        );
        assistant.thinking = Some("checking files".to_string());
        assistant.thinking_signature = sigs.encode();

        let tool_1 = Message::tool("call_1", r#"{"content":"alpha"}"#);
        let tool_2 = Message::tool("call_2", "plain text result");

        let projected = project_request_input(
            "gemini-3-pro-preview",
            None,
            &[Message::user("compare"), assistant, tool_1, tool_2],
        )
        .unwrap();

        assert_eq!(projected.contents.len(), 3);
        let model_parts = &projected.contents[1].parts;
        assert_eq!(model_parts.len(), 3);
        assert_eq!(model_parts[0].thought, Some(true));
        assert_eq!(model_parts[0].text.as_deref(), Some("checking files"));
        assert_eq!(
            model_parts[1].thought_signature.as_deref(),
            Some("sig-call-0")
        );
        assert_eq!(
            model_parts[1]
                .function_call
                .as_ref()
                .map(|c| c.name.as_str()),
            Some("read_file")
        );
        assert_eq!(model_parts[2].thought_signature, None);

        // Both tool responses are grouped into one user turn with resolved function names.
        assert_eq!(projected.contents[2].role.as_deref(), Some("user"));
        let resp_parts = &projected.contents[2].parts;
        assert_eq!(resp_parts.len(), 2);
        let r0 = resp_parts[0].function_response.as_ref().unwrap();
        assert_eq!(r0.id.as_deref(), Some("call_1"));
        assert_eq!(r0.name, "read_file");
        assert_eq!(r0.response, json!({"content": "alpha"}));
        let r1 = resp_parts[1].function_response.as_ref().unwrap();
        assert_eq!(r1.id.as_deref(), Some("call_2"));
        assert_eq!(r1.name, "read_file");
        assert_eq!(r1.response, json!({"result": "plain text result"}));
    }

    #[test]
    fn injects_validator_bypass_for_gemini_3_unsigned_tool_call_history() {
        let assistant = Message::assistant_with_tools(
            "",
            vec![ToolCall::new("search", json!({"q": "rust"})).with_id("c1")],
        );
        let projected = project_request_input("gemini-3-pro-preview", None, &[assistant]).unwrap();
        assert_eq!(
            projected.contents[0].parts[0].thought_signature.as_deref(),
            Some(SKIP_SIGNATURE_VALIDATOR)
        );
    }

    #[test]
    fn falls_back_when_typed_text_projects_to_nothing() {
        let content = project_plain_text(
            "fallback",
            &[MessageContentPart::Text {
                text: "   ".to_string(),
            }],
            "ctx",
        )
        .unwrap();

        assert_eq!(content, "fallback");
    }
}
