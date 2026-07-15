use super::*;
use crate::agent_machine::Message as MachineMessage;
use crate::tape::ToolRequest;
use serde_json::json;

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
