use crate::MessageContentPart;

pub(super) fn chat_completions_content(
    fallback: String,
    parts: Vec<MessageContentPart>,
) -> Option<serde_json::Value> {
    if parts.is_empty() {
        return (!fallback.is_empty()).then_some(serde_json::Value::String(fallback));
    }

    let parts = parts
        .into_iter()
        .filter_map(chat_completions_part)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        (!fallback.is_empty()).then_some(serde_json::Value::String(fallback))
    } else {
        Some(serde_json::Value::Array(parts))
    }
}

pub(super) fn responses_content(
    fallback: String,
    parts: Vec<MessageContentPart>,
) -> Option<serde_json::Value> {
    if parts.is_empty() {
        return (!fallback.is_empty()).then_some(serde_json::Value::String(fallback));
    }

    let parts = parts
        .into_iter()
        .filter_map(responses_part)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        (!fallback.is_empty()).then_some(serde_json::Value::String(fallback))
    } else {
        Some(serde_json::Value::Array(parts))
    }
}

fn chat_completions_part(part: MessageContentPart) -> Option<serde_json::Value> {
    match part {
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
            if mime_type.starts_with("image/")
                && let Some(url) = attachment_url(&metadata)
            {
                return Some(serde_json::json!({
                    "type": "image_url",
                    "image_url": {"url": url}
                }));
            }
            if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
                return Some(serde_json::json!({
                    "type": "file",
                    "file": {"file_id": file_id}
                }));
            }
            Some(fallback_text("text", hash, mime_type))
        }
    }
}

fn responses_part(part: MessageContentPart) -> Option<serde_json::Value> {
    match part {
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
                    return Some(serde_json::json!({"type": "input_image", "image_url": url}));
                }
                if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
                    return Some(serde_json::json!({"type": "input_image", "file_id": file_id}));
                }
            }
            if let Some(file_id) = metadata.get("file_id").and_then(serde_json::Value::as_str) {
                return Some(serde_json::json!({
                    "type": "input_file",
                    "file_id": file_id
                }));
            }
            if let Some(url) = attachment_url(&metadata) {
                return Some(serde_json::json!({
                    "type": "input_file",
                    "file_url": url
                }));
            }
            Some(fallback_text("input_text", hash, mime_type))
        }
    }
}

fn attachment_url(metadata: &serde_json::Value) -> Option<&str> {
    metadata
        .get("image_url")
        .or_else(|| metadata.get("file_url"))
        .or_else(|| metadata.get("url"))
        .and_then(serde_json::Value::as_str)
}

fn fallback_text(kind: &str, hash: String, mime_type: String) -> serde_json::Value {
    serde_json::json!({
        "type": kind,
        "text": format!("[attachment: {hash} ({mime_type})]"),
    })
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
            responses_content(String::new(), image_parts()),
            Some(json!([
                {"type": "input_text", "text": "Describe this image"},
                {"type": "input_image", "image_url": "https://example.com/cat.png"}
            ]))
        );
    }

    #[test]
    fn projects_chat_completions_image_input() {
        assert_eq!(
            chat_completions_content(String::new(), image_parts()),
            Some(json!([
                {"type": "text", "text": "Describe this image"},
                {"type": "image_url", "image_url": {"url": "https://example.com/cat.png"}}
            ]))
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
            responses_content("fallback".to_string(), parts()),
            Some(json!("fallback"))
        );
        assert_eq!(
            chat_completions_content("fallback".to_string(), parts()),
            Some(json!("fallback"))
        );

        let mut message = crate::Message::user("");
        message.content_parts = parts();
        assert!(super::super::convert_messages_for_openai_responses(vec![message]).is_empty());
    }
}
