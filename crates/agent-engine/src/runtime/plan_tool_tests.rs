use super::*;

#[test]
fn test_update_plan_tool_definition() {
    let def = update_plan_tool_definition();
    assert_eq!(def.name, "update_plan");
    assert!(def.description.contains("plan"));
    assert_eq!(def.parameters["type"], "object");
    assert!(def.parameters["properties"].get("explanation").is_some());
    assert!(def.parameters["properties"].get("items").is_some());
}

// Tests for parse_plan_update
#[test]
fn test_parse_plan_update_valid() {
    let args = json!({
        "explanation": "Test explanation",
        "items": [
            {"id": "1", "content": "Step 1", "status": "pending"},
            {"id": "2", "content": "Step 2", "status": "in_progress"}
        ]
    });

    let result = parse_plan_update(&args);
    assert!(result.is_some());

    let (explanation, items) = result.unwrap();
    assert_eq!(explanation, Some("Test explanation".to_string()));
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, "1");
    assert_eq!(items[1].content, "Step 2");
}

#[test]
fn test_parse_plan_update_without_explanation() {
    let args = json!({
        "items": [{"id": "1", "content": "Step 1", "status": "completed"}]
    });

    let result = parse_plan_update(&args);
    assert!(result.is_some());

    let (explanation, items) = result.unwrap();
    assert_eq!(explanation, None);
    assert_eq!(items.len(), 1);
}

#[test]
fn test_parse_plan_update_missing_items() {
    let args = json!({
        "explanation": "Test"
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_empty_items() {
    let args = json!({
        "items": []
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_missing_id() {
    let args = json!({
        "items": [{"content": "Step 1", "status": "pending"}]
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_missing_content() {
    let args = json!({
        "items": [{"id": "1", "status": "pending"}]
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_missing_status() {
    let args = json!({
        "items": [{"id": "1", "content": "Step 1"}]
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_invalid_status() {
    let args = json!({
        "items": [{"id": "1", "content": "Step 1", "status": "invalid_status"}]
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_using_description() {
    // Tests that "description" field can be used as fallback for "content"
    let args = json!({
        "items": [{"id": "1", "description": "Step description", "status": "pending"}]
    });

    let result = parse_plan_update(&args);
    assert!(result.is_some());
    assert_eq!(result.unwrap().1[0].content, "Step description");
}

// Tests for parse_plan_status
#[test]
fn test_parse_plan_status_valid_values() {
    assert!(parse_plan_status("pending").is_some());
    assert!(parse_plan_status("blocked").is_some());
    assert!(parse_plan_status("in_progress").is_some());
    assert!(parse_plan_status("completed").is_some());
    assert!(parse_plan_status("skipped").is_some());
}

#[test]
fn test_parse_plan_status_invalid_values() {
    assert!(parse_plan_status("unknown").is_none());
    assert!(parse_plan_status("").is_none());
    assert!(parse_plan_status("PENDING").is_none()); // Case sensitive
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_update_plan() {
    let mut state = create_test_agent_loop_state();
    let expected_items = vec![alan_agent_protocol::PlanItem {
        id: "1".to_string(),
        content: "Step 1".to_string(),
        status: alan_agent_protocol::PlanItemStatus::InProgress,
    }];

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "update_plan".to_string(),
        arguments: json!({
            "explanation": "Test plan",
            "items": [{"id": "1", "content": "Step 1", "status": "in_progress"}]
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::PlanUpdated { explanation, items }
            if explanation.as_deref() == Some("Test plan") && items == &expected_items
    )));

    let prompt_view = state.machine.tape.prompt_view();
    let tool_result = prompt_view
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == "call_1")
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected update_plan tool payload");
    assert!(tool_result.contains("\"status\":\"plan_updated\""));
    assert!(tool_result.contains("\"items\":["));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invalid_update_plan() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "update_plan".to_string(),
        arguments: json!({
            // Missing items
            "explanation": "Test"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: false
        }
    ));
}
