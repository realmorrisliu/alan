use super::*;

#[test]
fn test_message_role_serialization() {
    let roles = vec![
        (MessageRole::System, "\"system\""),
        (MessageRole::Context, "\"context\""),
        (MessageRole::User, "\"user\""),
        (MessageRole::Assistant, "\"assistant\""),
        (MessageRole::Tool, "\"tool\""),
    ];

    for (role, expected) in roles {
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, expected);

        let deserialized: MessageRole = serde_json::from_str(expected).unwrap();
        assert!(std::mem::discriminant(&deserialized) == std::mem::discriminant(&role));
    }
}

#[test]
fn test_message_serialization_user() {
    let message = Message::user("Hello");

    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("Hello"));
    assert!(json.contains("user"));

    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.text_content(), "Hello");
    assert_eq!(deserialized.role(), MessageRole::User);
}

#[test]
fn test_message_serialization_tool() {
    let message = Message::Tool {
        responses: vec![ToolResponse {
            id: "call_1".to_string(),
            content: vec![ContentPart::structured(
                serde_json::json!({"result": "found"}),
            )],
        }],
    };

    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("found"));
    assert!(json.contains("tool"));

    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.role(), MessageRole::Tool);
    assert_eq!(deserialized.tool_responses().len(), 1);
    assert_eq!(deserialized.tool_responses()[0].id, "call_1");
}

#[test]
fn test_message_assistant_with_tool_requests() {
    let message = Message::assistant_with_tools(
        "Let me search for that.",
        vec![ToolRequest {
            id: "call_1".to_string(),
            name: "web_search".to_string(),
            arguments: serde_json::json!({"query": "rust"}),
        }],
    );

    assert_eq!(message.role(), MessageRole::Assistant);
    assert_eq!(message.text_content(), "Let me search for that.");
    assert_eq!(message.tool_requests().len(), 1);
    assert_eq!(message.tool_requests()[0].name, "web_search");

    // Round-trip serialization
    let json = serde_json::to_string(&message).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.tool_requests().len(), 1);
}

#[test]
fn test_content_part_constructors() {
    let text = ContentPart::text("hello");
    assert_eq!(text.as_text(), Some("hello"));

    let thinking = ContentPart::thinking("reasoning...");
    assert_eq!(thinking.as_text(), Some("reasoning..."));

    let structured = ContentPart::structured(serde_json::json!({"key": "value"}));
    assert!(structured.as_text().is_none());
}

#[test]
fn test_tool_response_text_content() {
    let resp = ToolResponse {
        id: "call_1".to_string(),
        content: vec![ContentPart::text("part1"), ContentPart::text("part2")],
    };
    assert_eq!(resp.text_content(), "part1part2");
}

#[test]
fn test_message_is_predicates() {
    assert!(Message::user("hi").is_user());
    assert!(Message::assistant("hi").is_assistant());
    assert!(Message::system("hi").is_system());
    assert!(Message::context("hi").is_context());
    assert!(Message::Tool { responses: vec![] }.is_tool());
}
