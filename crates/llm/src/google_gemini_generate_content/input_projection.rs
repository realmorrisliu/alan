use anyhow::{Result, bail};

use super::{Content, Part};
use crate::{Message, MessageContentPart, MessageRole};

#[derive(Debug)]
pub(super) struct ProjectedRequestInput {
    pub(super) contents: Vec<Content>,
    pub(super) system_instruction: Option<Content>,
}

pub(super) fn project_request_input(
    system_prompt: Option<&str>,
    messages: &[Message],
) -> Result<ProjectedRequestInput> {
    let mut contents = Vec::new();
    let mut system_parts = system_prompt
        .map(Part::text)
        .into_iter()
        .collect::<Vec<_>>();

    for message in messages {
        let content = project_content(&message.content, &message.content_parts)?;
        match message.role {
            MessageRole::User | MessageRole::Context => {
                contents.push(Content::user(vec![Part::text(content)]));
            }
            MessageRole::Assistant => {
                contents.push(Content::model(vec![Part::text(content)]));
            }
            MessageRole::Tool => {
                let Some(name) = message.tool_call_id.clone() else {
                    continue;
                };
                let payload = serde_json::from_str(&content)
                    .unwrap_or_else(|_| serde_json::json!({"result": content}));
                contents.push(Content::function(vec![Part::function_response(
                    name, payload,
                )]));
            }
            MessageRole::System => system_parts.push(Part::text(content)),
        }
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

fn project_content(fallback: &str, parts: &[MessageContentPart]) -> Result<String> {
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
                "Google Gemini GenerateContent cannot represent attachment `{hash}` ({mime_type}); select a Connection with attachment support"
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

        let projected = project_request_input(Some("base"), &[system, user]).unwrap();

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
    fn rejects_attachments_before_dispatch() {
        let mut message = Message::user("");
        message.content_parts = vec![MessageContentPart::Attachment {
            hash: "doc_hash".to_string(),
            mime_type: "application/pdf".to_string(),
            metadata: json!({"file_id": "file_123"}),
        }];

        let error = project_request_input(None, &[message]).unwrap_err();

        assert!(error.to_string().contains("cannot represent attachment"));
    }

    #[test]
    fn falls_back_when_typed_text_projects_to_nothing() {
        let content = project_content(
            "fallback",
            &[MessageContentPart::Text {
                text: "   ".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(content, "fallback");
    }
}
