use super::*;
use crate::agent_machine::Message as MachineMessage;
use crate::tape::{ToolRequest, ToolResponse};
use serde_json::json;

fn provider_input(
    instruction_role: InstructionRole,
    messages: &[MachineMessage],
) -> std::collections::HashMap<String, serde_json::Value> {
    let request = super::super::build_generation_request(None, Vec::new(), Vec::new(), None, None);
    with_provider_input(request, instruction_role, messages).extra_params
}

#[test]
fn project_messages_maps_roles_and_content() {
    let messages = vec![
        MachineMessage::user("Hello"),
        MachineMessage::assistant("Hi there"),
    ];

    let projected = project_messages(&messages, true);

    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].role, MessageRole::User);
    assert_eq!(projected[0].content, "Hello");
    assert_eq!(projected[1].role, MessageRole::Assistant);
}

#[test]
fn project_messages_ignores_blank_tool_ids() {
    let messages = vec![
        MachineMessage::assistant_with_tools(
            "",
            vec![ToolRequest {
                id: "   ".to_string(),
                name: "web_search".to_string(),
                arguments: json!({"query": "test"}),
            }],
        ),
        MachineMessage::tool_text("   ", "{}"),
    ];

    let projected = project_messages(&messages, true);

    assert_eq!(projected[0].tool_calls.as_ref().unwrap()[0].id, None);
    assert_eq!(projected[1].tool_call_id, None);
}

#[test]
fn project_messages_preserves_structured_tool_payload() {
    let payload = json!({
        "success": true,
        "company": "y-warm.com"
    });
    let messages = vec![MachineMessage::tool_structured(
        "tool_call_123",
        payload.clone(),
    )];

    let projected = project_messages(&messages, true);

    assert_eq!(projected[0].role, MessageRole::Tool);
    assert_eq!(projected[0].tool_call_id.as_deref(), Some("tool_call_123"));
    assert_eq!(projected[0].content, payload.to_string());
}

#[test]
fn project_messages_caps_structured_and_text_tool_payloads() {
    let structured = MachineMessage::tool_structured(
        "structured",
        json!({"success": true, "content": "x".repeat(50_000)}),
    );
    let text = MachineMessage::tool_text("text", "x".repeat(50_000));

    let projected = project_messages(&[structured, text], true);

    for message in projected {
        assert!(message.content.len() <= MAX_PROJECTED_TOOL_PAYLOAD_SIZE);
        assert!(message.content.contains(PROJECTION_TRUNCATION_MARKER));
    }
}

#[test]
fn project_messages_preserves_payload_within_budget() {
    let content = "x".repeat(20_000);
    let projected = project_messages(
        &[MachineMessage::tool_text("tool_call_123", content.clone())],
        true,
    );

    assert_eq!(projected[0].content, content);
}

#[test]
fn project_messages_caps_tool_payload_by_bytes_without_breaking_utf8() {
    let content = "你".repeat(20_000);
    let projected = project_messages(&[MachineMessage::tool_text("tool_call_123", content)], true);

    assert!(projected[0].content.len() <= MAX_PROJECTED_TOOL_PAYLOAD_SIZE);
    assert!(projected[0].content.contains(PROJECTION_TRUNCATION_MARKER));

    let text = "你好世界好";
    assert_eq!(truncate_text_for_projection(text, text.len()), text);
    let truncated = truncate_text_for_projection(text, 14);
    assert!(truncated.len() <= 14);
    assert_eq!(truncated, PROJECTION_TRUNCATION_MARKER);
}

#[test]
fn thinking_projection_preserves_or_drops_all_reasoning_metadata() {
    let mut machine = crate::agent_machine::AgentMachine::new();
    machine.add_assistant_message_with_reasoning(
        "hello",
        Some("my reasoning"),
        Some("sig_123"),
        &["ciphertext".to_string()],
    );

    let preserved = project_messages(machine.tape.messages(), true);
    assert_eq!(preserved[0].thinking.as_deref(), Some("my reasoning"));
    assert_eq!(preserved[0].thinking_signature.as_deref(), Some("sig_123"));
    assert_eq!(
        preserved[0].redacted_thinking,
        Some(vec!["ciphertext".to_string()])
    );

    let dropped = project_messages(machine.tape.messages(), false);
    assert_eq!(dropped[0].content, "hello");
    assert_eq!(dropped[0].thinking, None);
    assert_eq!(dropped[0].thinking_signature, None);
    assert_eq!(dropped[0].redacted_thinking, None);
}

#[test]
fn responses_input_projects_developer_role_and_attachments() {
    let messages = vec![
        MachineMessage::Context {
            parts: vec![ContentPart::text("Workspace context")],
        },
        MachineMessage::User {
            parts: vec![
                ContentPart::text("What is in this image?"),
                ContentPart::Attachment {
                    hash: "img_hash".to_string(),
                    mime_type: "image/png".to_string(),
                    metadata: json!({"image_url": "https://example.com/cat.png"}),
                },
            ],
        },
    ];

    let params = provider_input(InstructionRole::ResponsesInstructions, &messages);

    assert_eq!(
        params["responses_input_items"],
        json!([
            {"role": "developer", "content": "Workspace context"},
            {
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "What is in this image?"},
                    {"type": "input_image", "image_url": "https://example.com/cat.png"}
                ]
            }
        ])
    );
}

#[test]
fn chat_completions_input_projects_developer_role_and_attachments() {
    let messages = vec![
        MachineMessage::Context {
            parts: vec![ContentPart::text("Workspace context")],
        },
        MachineMessage::User {
            parts: vec![
                ContentPart::text("What is in this image?"),
                ContentPart::Attachment {
                    hash: "img_hash".to_string(),
                    mime_type: "image/png".to_string(),
                    metadata: json!({"file_url": "https://example.com/cat.png"}),
                },
            ],
        },
    ];

    let params = provider_input(InstructionRole::Developer, &messages);

    assert_eq!(
        params["chat_completions_messages"],
        json!([
            {"role": "developer", "content": "Workspace context"},
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "What is in this image?"},
                    {
                        "type": "image_url",
                        "image_url": {"url": "https://example.com/cat.png"}
                    }
                ]
            }
        ])
    );
}

#[test]
fn anthropic_input_projects_file_attachments() {
    let messages = vec![MachineMessage::User {
        parts: vec![
            ContentPart::text("Read this document"),
            ContentPart::Attachment {
                hash: "doc_hash".to_string(),
                mime_type: "application/pdf".to_string(),
                metadata: json!({"file_id": "file_123", "title": "Spec"}),
            },
        ],
    }];

    let params = provider_input(InstructionRole::AnthropicSystem, &messages);

    assert_eq!(
        params["anthropic_messages"],
        json!([{
            "role": "user",
            "content": [
                {"type": "text", "text": "Read this document"},
                {
                    "type": "document",
                    "source": {"type": "file", "file_id": "file_123"},
                    "title": "Spec"
                }
            ]
        }])
    );
}

#[test]
fn provider_inputs_share_bounded_tool_projection() {
    let large_output = "x".repeat(40_000);
    let response = ToolResponse {
        id: "call-1".to_string(),
        content: vec![ContentPart::text(large_output)],
    };
    let tool_message = MachineMessage::Tool {
        responses: vec![response],
    };
    let assistant = MachineMessage::Assistant {
        parts: Vec::new(),
        tool_requests: vec![ToolRequest {
            id: "call-1".to_string(),
            name: "tool".to_string(),
            arguments: json!({}),
        }],
    };

    let responses = provider_input(
        InstructionRole::ResponsesInstructions,
        std::slice::from_ref(&tool_message),
    );
    let response_output = responses["responses_input_items"][0]["output"]
        .as_str()
        .unwrap();
    assert!(response_output.len() <= MAX_PROJECTED_TOOL_PAYLOAD_SIZE);
    assert!(response_output.contains(PROJECTION_TRUNCATION_MARKER));

    let chat = provider_input(
        InstructionRole::Developer,
        std::slice::from_ref(&tool_message),
    );
    let chat_output = chat["chat_completions_messages"][0]["content"]
        .as_str()
        .unwrap();
    assert_eq!(chat_output, response_output);

    let anthropic = provider_input(InstructionRole::AnthropicSystem, &[assistant, tool_message]);
    let anthropic_output = anthropic["anthropic_messages"][1]["content"][0]["content"]
        .as_str()
        .unwrap();
    assert_eq!(anthropic_output, response_output);
}

#[test]
fn system_role_does_not_add_private_provider_input() {
    let params = provider_input(InstructionRole::System, &[MachineMessage::user("Hello")]);
    assert!(params.is_empty());
}
