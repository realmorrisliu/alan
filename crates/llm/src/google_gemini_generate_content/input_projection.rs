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

const INTERRUPTED_TURN_BRIDGE_TEXT: &str =
    "[Previous turn did not complete before next user input.]";

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
    let mut expected_calls: Vec<(Option<String>, String)> = Vec::new();
    let mut deferred_user_parts: Vec<Part> = Vec::new();
    let mut pending_tool_responses: Vec<ProjectedToolResponse> = Vec::new();
    let mut last_user_turn_is_function_response = false;

    for message in messages {
        match message.role {
            MessageRole::Tool => {
                if let Some(projected) = project_tool_response(message, &call_id_to_name)? {
                    pending_tool_responses.push(projected);
                }
            }
            MessageRole::User => {
                let parts = project_user_parts(&message.content, &message.content_parts)?;
                if !expected_calls.is_empty() {
                    // Defer user messages that arrive while a tool-call batch
                    // is still open (for example mid-batch steering) so all
                    // functionResponses stay immediately paired with their model turn.
                    deferred_user_parts.extend(parts);
                    continue;
                }
                if !pending_tool_responses.is_empty() {
                    flush_tool_responses(
                        &mut contents,
                        &mut pending_tool_responses,
                        &mut expected_calls,
                        &mut last_user_turn_is_function_response,
                    );
                }
                append_user_turn(
                    &mut contents,
                    parts,
                    &mut last_user_turn_is_function_response,
                );
            }
            MessageRole::Assistant => {
                if !pending_tool_responses.is_empty() || !expected_calls.is_empty() {
                    flush_tool_responses(
                        &mut contents,
                        &mut pending_tool_responses,
                        &mut expected_calls,
                        &mut last_user_turn_is_function_response,
                    );
                }
                if !deferred_user_parts.is_empty() {
                    append_user_turn(
                        &mut contents,
                        std::mem::take(&mut deferred_user_parts),
                        &mut last_user_turn_is_function_response,
                    );
                }
                let (parts, new_expected_calls) =
                    project_assistant_parts(model, message, &mut call_id_to_name)?;
                expected_calls = new_expected_calls;
                push_or_merge_model_turn(
                    &mut contents,
                    parts,
                    &mut last_user_turn_is_function_response,
                );
            }
            MessageRole::System | MessageRole::Context => {
                let text = project_plain_text(
                    &message.content,
                    &message.content_parts,
                    "Google Gemini GenerateContent system instructions",
                )?;
                if !text.is_empty() {
                    system_parts.push(Part::text(text));
                }
            }
        }
    }

    if !pending_tool_responses.is_empty()
        || (!expected_calls.is_empty() && !deferred_user_parts.is_empty())
    {
        flush_tool_responses(
            &mut contents,
            &mut pending_tool_responses,
            &mut expected_calls,
            &mut last_user_turn_is_function_response,
        );
    }
    if !deferred_user_parts.is_empty() {
        append_user_turn(
            &mut contents,
            deferred_user_parts,
            &mut last_user_turn_is_function_response,
        );
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

struct ProjectedToolResponse {
    id: Option<String>,
    name: String,
    response: Value,
    raw_content: String,
}

fn project_tool_response(
    message: &Message,
    call_id_to_name: &HashMap<String, String>,
) -> Result<Option<ProjectedToolResponse>> {
    let Some(raw_id) = message
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
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
    Ok(Some(ProjectedToolResponse {
        id,
        name,
        response,
        raw_content: content,
    }))
}

fn flush_tool_responses(
    contents: &mut Vec<Content>,
    pending_tool_responses: &mut Vec<ProjectedToolResponse>,
    expected_calls: &mut Vec<(Option<String>, String)>,
    last_user_turn_is_function_response: &mut bool,
) {
    let drained = std::mem::take(pending_tool_responses);
    let expected = std::mem::take(expected_calls);
    if expected.is_empty() {
        // Orphan tool results without a preceding functionCall turn must be
        // sent as plain text rather than standalone `functionResponse` parts.
        let orphan_parts = drained
            .into_iter()
            .map(|tr| Part::text(format!("[Tool result ({}): {}]", tr.name, tr.raw_content)))
            .collect();
        append_user_turn(contents, orphan_parts, last_user_turn_is_function_response);
        return;
    }

    let mut remaining: Vec<Option<ProjectedToolResponse>> = drained.into_iter().map(Some).collect();
    let mut response_parts = Vec::with_capacity(expected.len());

    for (expected_id, expected_name) in expected {
        let matched_idx = remaining.iter().position(|candidate| {
            let Some(tr) = candidate.as_ref() else {
                return false;
            };
            match (&expected_id, &tr.id) {
                (Some(eid), Some(tid)) => eid == tid,
                _ => tr.name == expected_name,
            }
        });
        if let Some(idx) = matched_idx {
            let tr = remaining[idx].take().expect("checked Some");
            response_parts.push(Part {
                function_response: Some(FunctionResponse {
                    id: tr.id,
                    name: tr.name,
                    response: tr.response,
                }),
                ..Part::default()
            });
        } else {
            response_parts.push(Part {
                function_response: Some(FunctionResponse {
                    id: expected_id,
                    name: expected_name,
                    response: serde_json::json!({
                        "error": "tool call did not complete before next turn"
                    }),
                }),
                ..Part::default()
            });
        }
    }

    push_pure_function_response_turn(
        contents,
        response_parts,
        last_user_turn_is_function_response,
    );

    let extra_parts: Vec<Part> = remaining
        .into_iter()
        .flatten()
        .map(|tr| Part::text(format!("[Tool result ({}): {}]", tr.name, tr.raw_content)))
        .collect();
    if !extra_parts.is_empty() {
        append_user_turn(contents, extra_parts, last_user_turn_is_function_response);
    }
}

fn push_pure_function_response_turn(
    contents: &mut Vec<Content>,
    parts: Vec<Part>,
    last_user_turn_is_function_response: &mut bool,
) {
    if parts.is_empty() {
        return;
    }
    if contents
        .last()
        .is_some_and(|last| last.role.as_deref() == Some("user"))
    {
        // Keep a pure model bridge if the preceding turn was also `user`.
        contents.push(Content::model(vec![Part::text(
            INTERRUPTED_TURN_BRIDGE_TEXT,
        )]));
    }
    contents.push(Content::user(parts));
    *last_user_turn_is_function_response = true;
}

fn append_user_turn(
    contents: &mut Vec<Content>,
    parts: Vec<Part>,
    last_user_turn_is_function_response: &mut bool,
) {
    let mut parts = strip_redundant_empty_text(parts);
    if parts.is_empty() {
        parts.push(Part::text(" "));
    }
    if *last_user_turn_is_function_response
        && contents
            .last()
            .is_some_and(|last| last.role.as_deref() == Some("user"))
    {
        // Vertex AI rejects mixing conversational text into a functionResponse
        // turn or sending two consecutive `user` turns. Bridge with a model turn
        // so the functionResponse turn remains pure and role alternation holds.
        contents.push(Content::model(vec![Part::text(
            INTERRUPTED_TURN_BRIDGE_TEXT,
        )]));
        *last_user_turn_is_function_response = false;
    }
    if let Some(last) = contents
        .last_mut()
        .filter(|last| last.role.as_deref() == Some("user"))
    {
        last.parts.extend(parts);
    } else {
        contents.push(Content::user(parts));
    }
    *last_user_turn_is_function_response = false;
}

fn push_or_merge_model_turn(
    contents: &mut Vec<Content>,
    parts: Vec<Part>,
    last_user_turn_is_function_response: &mut bool,
) {
    let parts: Vec<Part> = parts
        .into_iter()
        .filter(|part| !is_empty_text_part(part))
        .collect();
    if parts.is_empty() {
        return;
    }
    if let Some(last) = contents
        .last_mut()
        .filter(|last| last.role.as_deref() == Some("model"))
    {
        last.parts.extend(parts);
    } else {
        contents.push(Content::model(parts));
    }
    *last_user_turn_is_function_response = false;
}

fn strip_redundant_empty_text(parts: Vec<Part>) -> Vec<Part> {
    let has_non_empty = parts.iter().any(|part| !is_empty_text_part(part));
    if has_non_empty {
        parts
            .into_iter()
            .filter(|part| !is_empty_text_part(part))
            .collect()
    } else {
        parts
    }
}

fn is_empty_text_part(part: &Part) -> bool {
    part.text.as_deref().is_some_and(str::is_empty)
        && part.thought.is_none()
        && part.thought_signature.is_none()
        && part.inline_data.is_none()
        && part.file_data.is_none()
        && part.function_call.is_none()
        && part.function_response.is_none()
}

type ExpectedToolCall = (Option<String>, String);

fn project_assistant_parts(
    model: &str,
    message: &Message,
    call_id_to_name: &mut HashMap<String, String>,
) -> Result<(Vec<Part>, Vec<ExpectedToolCall>)> {
    let signatures = TurnSignatures::decode(message.thinking_signature.as_deref());
    let mut parts = Vec::new();
    let mut expected_calls = Vec::new();

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
            expected_calls.push((call.id.clone(), call.name.clone()));
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

    Ok((parts, expected_calls))
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

    #[test]
    fn merges_consecutive_user_turns_and_skips_empty_model_turns() {
        let projected = project_request_input(
            "gemini-3-pro-preview",
            None,
            &[
                Message::user("first"),
                Message::assistant(""),
                Message::user("second"),
            ],
        )
        .unwrap();
        assert_eq!(projected.contents.len(), 1);
        assert_eq!(projected.contents[0].role.as_deref(), Some("user"));
        assert_eq!(projected.contents[0].parts.len(), 2);
        assert_eq!(
            projected.contents[0].parts[0].text.as_deref(),
            Some("first")
        );
        assert_eq!(
            projected.contents[0].parts[1].text.as_deref(),
            Some("second")
        );
    }

    #[test]
    fn keeps_function_response_turn_pure_and_bridges_before_following_user_input() {
        let assistant = Message::assistant_with_tools(
            "",
            vec![ToolCall::new("bash", json!({"command": "ls"})).with_id("call_1")],
        );
        let tool = Message::tool("call_1", r#"{"exit_code":0}"#);
        let follow_up = Message::user("1");

        let projected = project_request_input(
            "gemini-3-pro-preview",
            None,
            &[Message::user("start"), assistant, tool, follow_up],
        )
        .unwrap();

        let roles: Vec<&str> = projected
            .contents
            .iter()
            .map(|c| c.role.as_deref().unwrap())
            .collect();
        assert_eq!(roles, vec!["user", "model", "user", "model", "user"]);
        assert!(projected.contents[2].parts.len() == 1);
        assert!(projected.contents[2].parts[0].function_response.is_some());
        assert!(projected.contents[2].parts[0].text.is_none());
        assert_eq!(projected.contents[4].parts[0].text.as_deref(), Some("1"));
    }

    #[test]
    fn regroups_mid_batch_steering_after_all_tool_responses_and_repairs_missing_calls() {
        let assistant = Message::assistant_with_tools(
            "",
            vec![
                ToolCall::new("read_file", json!({"path": "a.rs"})).with_id("c1"),
                ToolCall::new("read_file", json!({"path": "b.rs"})).with_id("c2"),
            ],
        );
        // Simulate steering_queue inserting User before the skipped Tool response.
        let projected = project_request_input(
            "gemini-3-pro-preview",
            None,
            &[
                Message::user("go"),
                assistant,
                Message::tool("c1", "alpha"),
                Message::user("steer now"),
                Message::tool("c2", "skipped"),
            ],
        )
        .unwrap();
        let roles: Vec<&str> = projected
            .contents
            .iter()
            .map(|c| c.role.as_deref().unwrap())
            .collect();
        assert_eq!(roles, vec!["user", "model", "user", "model", "user"]);
        assert_eq!(projected.contents[2].parts.len(), 2);
        assert_eq!(
            projected.contents[2].parts[0]
                .function_response
                .as_ref()
                .unwrap()
                .id
                .as_deref(),
            Some("c1")
        );
        assert_eq!(
            projected.contents[2].parts[1]
                .function_response
                .as_ref()
                .unwrap()
                .id
                .as_deref(),
            Some("c2")
        );
        assert_eq!(
            projected.contents[4].parts[0].text.as_deref(),
            Some("steer now")
        );

        // Also test an interrupted batch where c2 never recorded a Tool response.
        let interrupted = project_request_input(
            "gemini-3-pro-preview",
            None,
            &[
                Message::user("go"),
                Message::assistant_with_tools(
                    "",
                    vec![
                        ToolCall::new("read_file", json!({"path": "a.rs"})).with_id("c1"),
                        ToolCall::new("read_file", json!({"path": "b.rs"})).with_id("c2"),
                    ],
                ),
                Message::tool("c1", "alpha"),
                Message::user("retry"),
            ],
        )
        .unwrap();
        assert_eq!(interrupted.contents[2].parts.len(), 2);
        assert_eq!(
            interrupted.contents[2].parts[1]
                .function_response
                .as_ref()
                .unwrap()
                .response["error"],
            "tool call did not complete before next turn"
        );
    }

    #[test]
    fn projects_trailing_context_into_system_instruction_without_interrupted_bridge_after_tools() {
        let assistant = Message::assistant_with_tools(
            "",
            vec![ToolCall::new("bash", json!({"command": "pwd"})).with_id("call_1")],
        );
        let tool = Message::tool("call_1", r#"{"exit_code":0,"stdout":"/mnt/grants/repo\n"}"#);
        let runtime_ctx = Message::context(
            "Current selected Process directory: \"/mnt/grants/repo\"\nResolve relative Tool paths here.",
        );

        let projected = project_request_input(
            "gemini-2.5-pro",
            Some("Static base prompt"),
            &[Message::user("check cwd"), assistant, tool, runtime_ctx],
        )
        .unwrap();

        let roles: Vec<&str> = projected
            .contents
            .iter()
            .map(|c| c.role.as_deref().unwrap())
            .collect();
        assert_eq!(roles, vec!["user", "model", "user"]);
        let serialized = serde_json::to_string(&projected.contents).unwrap();
        assert!(
            !serialized.contains(INTERRUPTED_TURN_BRIDGE_TEXT),
            "runtime context must not inject interrupted-turn bridge text: {serialized}"
        );
        let sys_parts = &projected.system_instruction.unwrap().parts;
        assert_eq!(sys_parts.len(), 2);
        assert_eq!(sys_parts[0].text.as_deref(), Some("Static base prompt"));
        assert!(
            sys_parts[1]
                .text
                .as_deref()
                .unwrap()
                .contains("/mnt/grants/repo")
        );
    }

    #[test]
    fn degrades_orphan_tool_response_without_preceding_function_call_to_text() {
        let projected = project_request_input(
            "gemini-3-pro-preview",
            None,
            &[Message::tool("bash", "ok"), Message::user("next")],
        )
        .unwrap();
        assert_eq!(projected.contents.len(), 1);
        assert_eq!(projected.contents[0].role.as_deref(), Some("user"));
        assert!(projected.contents[0].parts[0].function_response.is_none());
        assert!(
            projected.contents[0].parts[0]
                .text
                .as_deref()
                .unwrap()
                .contains("Tool result (bash)")
        );
    }
}
