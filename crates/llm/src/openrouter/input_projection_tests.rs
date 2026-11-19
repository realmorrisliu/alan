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
fn projects_user_image_attachments_from_urls_data_urls_and_base64() {
    let mut message = Message::user("");
    message.content_parts = vec![
        MessageContentPart::Text {
            text: "Compare these".to_string(),
        },
        MessageContentPart::Attachment {
            hash: "img_url".to_string(),
            mime_type: "image/png".to_string(),
            metadata: json!({"image_url": "https://example.com/a.png", "detail": "high"}),
        },
        MessageContentPart::Attachment {
            hash: "img_data_url".to_string(),
            mime_type: "image/jpeg".to_string(),
            metadata: json!({"file_data": "data:image/jpeg;base64,AA=="}),
        },
        MessageContentPart::Attachment {
            hash: "img_raw".to_string(),
            mime_type: "image/webp".to_string(),
            metadata: json!({"data": "QkI="}),
        },
    ];

    let projected = convert_messages_for_openrouter(vec![message]).unwrap();
    let value = serde_json::to_value(&projected[0]).unwrap();

    assert_eq!(
        value["content"],
        json!([
            {"type": "text", "text": "Compare these"},
            {
                "type": "image_url",
                "image_url": {"url": "https://example.com/a.png", "detail": "high"}
            },
            {
                "type": "image_url",
                "image_url": {"url": "data:image/jpeg;base64,AA=="}
            },
            {
                "type": "image_url",
                "image_url": {"url": "data:image/webp;base64,QkI="}
            }
        ])
    );
}

#[test]
fn rejects_non_image_or_incomplete_attachments_before_dispatch() {
    let mut pdf = Message::user("");
    pdf.content_parts = vec![MessageContentPart::Attachment {
        hash: "doc_hash".to_string(),
        mime_type: "application/pdf".to_string(),
        metadata: json!({"file_id": "file_123"}),
    }];
    let error = convert_messages_for_openrouter(vec![pdf]).unwrap_err();
    assert!(error.to_string().contains("cannot represent attachment"));

    let mut invalid = Message::user("");
    invalid.content_parts = vec![MessageContentPart::Attachment {
        hash: "img_empty".to_string(),
        mime_type: "image/png".to_string(),
        metadata: json!({}),
    }];
    let error = convert_messages_for_openrouter(vec![invalid]).unwrap_err();
    assert!(error.to_string().contains("cannot represent attachment"));
}

#[test]
fn rejects_attachments_in_non_user_roles() {
    let mut assistant = Message::assistant("");
    assistant.content_parts = vec![MessageContentPart::Attachment {
        hash: "img_hash".to_string(),
        mime_type: "image/png".to_string(),
        metadata: json!({"data": "AA=="}),
    }];

    let error = convert_messages_for_openrouter(vec![assistant]).unwrap_err();
    assert!(error.to_string().contains("only in user messages"));
}

#[test]
fn falls_back_when_typed_text_projects_to_nothing() {
    let content = project_text(
        "fallback".to_string(),
        vec![MessageContentPart::Text {
            text: "   ".to_string(),
        }],
    )
    .unwrap();

    assert_eq!(content, "fallback");
}
