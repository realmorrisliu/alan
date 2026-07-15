use crate::MessageContentPart;

use super::ContentBlockInput;

pub(super) fn content_blocks(
    fallback: String,
    parts: Vec<MessageContentPart>,
) -> anyhow::Result<Vec<ContentBlockInput>> {
    if parts.is_empty() {
        return Ok((!fallback.is_empty())
            .then_some(ContentBlockInput::Text { text: fallback })
            .into_iter()
            .collect());
    }

    let mut blocks = Vec::new();
    for part in parts {
        if let Some(block) = content_block(part)? {
            blocks.push(block);
        }
    }
    Ok(blocks)
}

fn content_block(part: MessageContentPart) -> anyhow::Result<Option<ContentBlockInput>> {
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
        } => Some(attachment_block(hash, mime_type, metadata)?),
    })
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

        let error = content_blocks(String::new(), parts).unwrap_err();
        assert!(error.to_string().contains("cannot represent attachment"));
    }
}
