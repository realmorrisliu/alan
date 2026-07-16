use super::*;

#[test]
fn test_agent_machine_new() {
    let machine = AgentMachine::new();
    assert!(machine.tape.messages().is_empty());
    assert!(machine.recorder.is_none());
}

#[test]
fn test_agent_machine_default() {
    let machine = AgentMachine::default();
    assert!(machine.tape.messages().is_empty());
}

#[test]
fn test_add_user_message() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("Hello, agent!");

    let messages = machine.tape.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role(), MessageRole::User);
    assert_eq!(messages[0].text_content(), "Hello, agent!");
    assert_eq!(machine.user_turn_ordinal(), 1);
}

#[test]
fn test_user_turn_ordinal_is_monotonic_across_rollback() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("u1");
    machine.add_user_message("u2");
    assert_eq!(machine.user_turn_ordinal(), 2);

    let removed = machine.rollback_last_turns(1);
    assert!(removed.removed_messages > 0);
    assert_eq!(removed.removed_turns, 1);
    assert_eq!(machine.user_turn_count(), 1);
    assert_eq!(machine.user_turn_ordinal(), 2);

    machine.add_user_message("u3");
    assert_eq!(machine.user_turn_count(), 2);
    assert_eq!(machine.user_turn_ordinal(), 3);
}

#[test]
fn test_user_control_message_does_not_increment_turn_ordinal() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("u1");
    assert_eq!(machine.user_turn_ordinal(), 1);

    machine.add_user_control_message_parts(vec![ContentPart::structured(
        serde_json::json!({"choice":"approve"}),
    )]);

    assert_eq!(machine.user_turn_count(), 2);
    assert_eq!(machine.user_turn_ordinal(), 1);
}

#[test]
fn test_add_assistant_message() {
    let mut machine = AgentMachine::new();
    machine.add_assistant_message("I can help you!", None);

    let messages = machine.tape.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role(), MessageRole::Assistant);
    assert_eq!(messages[0].text_content(), "I can help you!");
}

#[test]
fn test_add_tool_message() {
    let mut machine = AgentMachine::new();
    let payload = serde_json::json!({"result": "success"});
    machine.add_tool_message("call_123", "search_tool", payload);

    let messages = machine.tape.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role(), MessageRole::Tool);
    let responses = messages[0].tool_responses();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].id, "call_123");
}

#[test]
fn test_add_tool_message_accepts_content_parts_payload() {
    let mut machine = AgentMachine::new();
    let payload = serde_json::json!({
        "content_parts": [
            {"type": "text", "text": "hello"},
            {"type": "attachment", "hash": "abc123", "mime_type": "image/png", "metadata": {"w": 10, "h": 10}}
        ]
    });
    machine.add_tool_message("call_123", "capture", payload);

    let messages = machine.tape.messages();
    assert_eq!(messages.len(), 1);
    let responses = messages[0].tool_responses();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].id, "call_123");
    assert!(matches!(
        responses[0].content.first(),
        Some(ContentPart::Text { text }) if text == "hello"
    ));
    assert!(matches!(
        responses[0].content.get(1),
        Some(ContentPart::Attachment { hash, mime_type, .. })
        if hash == "abc123" && mime_type == "image/png"
    ));
}

#[test]
fn test_add_tool_message_accepts_content_parts_array_payload() {
    let mut machine = AgentMachine::new();
    let payload = serde_json::json!([
        {"type": "text", "text": "part-a"},
        {"type": "structured", "data": {"k": "v"}}
    ]);
    machine.add_tool_message("call_124", "custom", payload);

    let messages = machine.tape.messages();
    assert_eq!(messages.len(), 1);
    let responses = messages[0].tool_responses();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].content.len(), 2);
    assert!(matches!(
        responses[0].content.first(),
        Some(ContentPart::Text { text }) if text == "part-a"
    ));
    assert!(matches!(
        responses[0].content.get(1),
        Some(ContentPart::Structured { data }) if data["k"] == "v"
    ));
}

#[test]
fn test_responses_continuation_can_be_marked_and_cleared() {
    let mut machine = AgentMachine::new();
    machine.mark_responses_continuation("openai_responses", "resp_123", 2, 7);

    let continuation = machine.responses_continuation().expect("continuation");
    assert_eq!(continuation.provider, "openai_responses");
    assert_eq!(continuation.last_response_id, "resp_123");
    assert_eq!(continuation.boundary_message_count, 2);
    assert_eq!(continuation.reference_context_revision, 7);

    machine.clear_responses_continuation("test");
    assert!(machine.responses_continuation().is_none());
}

#[test]
fn test_multiple_messages() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("First");
    machine.add_assistant_message("Second", None);
    machine.add_user_message("Third");

    let messages = machine.tape.messages();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].role(), MessageRole::User);
    assert_eq!(messages[1].role(), MessageRole::Assistant);
    assert_eq!(messages[2].role(), MessageRole::User);
}

#[test]
fn test_clear_machine() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("Test");

    machine.clear();

    assert!(machine.tape.messages().is_empty());
}

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
fn test_message_serialization() {
    let message = Message::user("Hello");

    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("Hello"));
    assert!(json.contains("user"));

    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.text_content(), "Hello");
}

#[test]
fn test_message_serialization_with_tool() {
    let message = Message::Tool {
        responses: vec![ToolResponse {
            id: "web_search".to_string(),
            content: vec![ContentPart::structured(
                serde_json::json!({"result": "found"}),
            )],
        }],
    };

    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("web_search"));
    assert!(json.contains("found"));

    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.tool_responses()[0].id, "web_search");
}

#[test]
fn test_machine_rollout_path_without_recorder() {
    let machine = AgentMachine::new();
    assert!(machine.rollout_path().is_none());
}

#[test]
fn test_machine_has_active_task_defaults_false() {
    let machine = AgentMachine::new();
    assert!(!machine.has_active_task);
}

#[test]
fn test_machine_clear_resets_active_task() {
    let mut machine = AgentMachine::new();
    machine.has_active_task = true;
    machine.clear();
    assert!(!machine.has_active_task);
}

#[test]
fn test_truncate_payload_small_payload_unchanged() {
    let payload = serde_json::json!({
        "success": true,
        "url": "https://example.com",
        "title": "Example"
    });
    let result = truncate_payload(payload.clone(), 1000);
    assert_eq!(result, payload);
}

#[test]
fn test_truncate_text() {
    let text = "This is a long text that needs to be truncated";
    let truncated = truncate_text(text, 20);
    assert!(truncated.contains("...[truncated]"));
    assert!(truncated.len() < text.len() + 15); // +15 for "...[truncated]"
}

#[test]
fn test_truncate_text_short() {
    let text = "Short";
    let truncated = truncate_text(text, 100);
    assert_eq!(truncated, text);
}

#[test]
fn test_truncate_payload_large_content() {
    let large_content = "a".repeat(10000);
    let payload = serde_json::json!({
        "success": true,
        "url": "https://example.com",
        "content": large_content
    });
    let result = truncate_payload(payload, 5000);
    let result_str = result.to_string();
    assert!(result_str.len() < 6000); // Should be significantly reduced
    assert!(result_str.contains("...[truncated]"));
}

#[test]
fn test_truncate_payload_preserves_critical_fields() {
    let large_content = "x".repeat(5000);
    let payload = serde_json::json!({
        "success": false,
        "error": "Some error",
        "url": "https://example.com",
        "title": "Test Title",
        "content": large_content
    });
    let result = truncate_payload(payload, 2000);
    // Critical fields should be preserved
    assert_eq!(result["success"], false);
    assert_eq!(result["error"], "Some error");
    assert_eq!(result["url"], "https://example.com");
    assert_eq!(result["title"], "Test Title");
}

#[test]
fn test_add_tool_message_preserves_large_payload_on_tape() {
    let mut machine = AgentMachine::new();
    let large_content = "x".repeat(50000);
    let payload = serde_json::json!({
        "success": true,
        "content": large_content
    });

    machine.add_tool_message("call_456", "test_tool", payload);

    let messages = machine.tape.messages();
    assert_eq!(messages.len(), 1);
    let responses = messages[0].tool_responses();
    assert_eq!(responses.len(), 1);

    // Tape should keep the full payload; projection handles truncation.
    let response_str = serde_json::to_string(&responses[0].content).unwrap();
    assert!(
        response_str.len() > 50000,
        "Payload should stay full on tape, got {} chars",
        response_str.len()
    );
}

// Additional truncation tests for better coverage

#[test]
fn test_truncate_payload_array() {
    let payload = serde_json::json!([
        {"id": 1, "content": "First item"},
        {"id": 2, "content": "Second item"},
        {"id": 3, "content": "Third item"}
    ]);
    // Small max_size to trigger truncation
    let result = truncate_payload(payload, 100);
    // Result should be an array
    assert!(result.is_array());
    let arr = result.as_array().unwrap();
    // Should contain items but may have truncation note
    assert!(!arr.is_empty());
}

#[test]
fn test_truncate_payload_nested_object() {
    let payload = serde_json::json!({
        "level1": {
            "level2": {
                "data": "x".repeat(5000)
            }
        }
    });
    let result = truncate_payload(payload, 1000);
    // Should preserve structure but truncate content
    assert!(result.get("level1").is_some());
}

#[test]
fn test_truncate_payload_string_only() {
    let payload = serde_json::Value::String("a".repeat(5000));
    let result = truncate_payload(payload, 1000);
    // String should be truncated
    let result_str = result.as_str().unwrap();
    assert!(result_str.len() < 5000);
    assert!(result_str.contains("...[truncated]"));
}

#[test]
fn test_truncate_payload_aggregated_content() {
    let large_content = "b".repeat(5000);
    let payload = serde_json::json!({
        "success": true,
        "aggregated_content": large_content
    });
    let result = truncate_payload(payload, 2000);
    // aggregated_content should be truncated
    let content = result["aggregated_content"].as_str().unwrap();
    assert!(content.len() < 5000);
    assert!(content.contains("...[truncated]"));
}

#[test]
fn test_truncate_payload_array_truncation_note() {
    // Create a large array that will trigger truncation
    let items: Vec<serde_json::Value> = (0..100)
        .map(|i| serde_json::json!({"id": i, "data": "x".repeat(100)}))
        .collect();
    let payload = serde_json::Value::Array(items);
    let result = truncate_payload(payload, 500);
    // Should contain truncation note in one of the items
    let arr = result.as_array().unwrap();
    let has_note = arr.iter().any(|item| {
        item.get("_note")
            .and_then(|n| n.as_str())
            .map(|s| s.contains("omitted"))
            .unwrap_or(false)
    });
    assert!(has_note, "Should have truncation note in array items");
}

#[test]
fn test_truncate_payload_object_truncated_field() {
    // Create an object with many large fields
    let mut map = serde_json::Map::new();
    for i in 0..50 {
        map.insert(
            format!("field{}", i),
            serde_json::Value::String("y".repeat(200)),
        );
    }
    let payload = serde_json::Value::Object(map);
    let result = truncate_payload(payload, 1000);
    // Should have _truncated field
    assert!(
        result.get("_truncated").is_some(),
        "Should have _truncated field for omitted fields"
    );
}

#[test]
fn test_truncate_payload_mixed_types() {
    let payload = serde_json::json!({
        "string": "test",
        "number": 42,
        "bool": true,
        "null": null,
        "array": [1, 2, 3],
        "nested": {"key": "value"}
    });
    let result = truncate_payload(payload, 1000);
    // All types should be preserved
    assert_eq!(result["string"], "test");
    assert_eq!(result["number"], 42);
    assert_eq!(result["bool"], true);
    assert!(result["null"].is_null());
    assert!(result["array"].is_array());
    assert!(result["nested"].is_object());
}

#[test]
fn test_rollback_last_turns_removes_latest_turn_messages() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("u1");
    machine.add_assistant_message("a1", None);
    machine.add_tool_message("call1", "web_search", serde_json::json!({"ok": true}));
    machine.add_user_message("u2");
    machine.add_assistant_message("a2", None);

    let removed = machine.rollback_last_turns(1);

    assert_eq!(removed.removed_turns, 1);
    assert_eq!(removed.removed_messages, 2);
    let messages = machine.tape.messages();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].text_content(), "u1");
    assert_eq!(messages[1].text_content(), "a1");
    assert!(messages[2].is_tool());
}

#[test]
fn test_rollback_last_turns_clears_all_when_request_exceeds_history() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("u1");
    machine.add_assistant_message("a1", None);
    machine.has_active_task = true;

    let removed = machine.rollback_last_turns(10);

    assert_eq!(removed.removed_turns, 1);
    assert_eq!(removed.removed_messages, 2);
    assert!(machine.tape.messages().is_empty());
    assert!(!machine.has_active_task);
}

#[test]
fn test_rollback_last_turns_ignores_control_user_messages_for_turn_boundaries() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("u1");
    machine.add_assistant_message("a1", None);
    machine.add_user_control_message_parts(vec![ContentPart::structured(serde_json::json!({
        "checkpoint_id": "tool_escalation_call-1",
        "checkpoint_type": "tool_escalation",
        "choice": "approve",
        "__alan_internal_control": {
            "kind": "tool_escalation_confirmation",
            "version": 1,
            "source": "runtime/submission_handlers"
        }
    }))]);
    machine.add_assistant_message("a2", None);

    let removed = machine.rollback_last_turns(1);

    assert_eq!(
        removed.removed_messages, 4,
        "rollback should anchor on the real user turn, not synthetic control messages"
    );
    assert_eq!(removed.removed_turns, 1);
    assert!(machine.tape.messages().is_empty());
}

#[test]
fn test_rollback_last_turns_ignores_effect_replay_control_messages_for_turn_boundaries() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("u1");
    machine.add_assistant_message("a1", None);
    machine.add_user_control_message_parts(vec![ContentPart::structured(serde_json::json!({
        "checkpoint_id": "effect_replay_call-1",
        "checkpoint_type": "effect_replay_confirmation",
        "choice": "approve",
        "__alan_internal_control": {
            "kind": "effect_replay_confirmation",
            "version": 1,
            "source": "runtime/submission_handlers"
        }
    }))]);
    machine.add_assistant_message("a2", None);

    let removed = machine.rollback_last_turns(1);

    assert_eq!(
        removed.removed_messages, 4,
        "rollback should ignore effect replay control messages the same way as policy controls"
    );
    assert_eq!(removed.removed_turns, 1);
    assert!(machine.tape.messages().is_empty());
}
