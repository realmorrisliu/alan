use super::*;

#[test]
fn test_request_confirmation_tool_definition_schema_shape() {
    let def = request_confirmation_tool_definition();
    assert_eq!(def.name, "request_confirmation");
    assert!(def.description.contains("confirmation"));
    assert_eq!(def.parameters["type"], "object");
    assert_eq!(
        def.parameters["properties"]["checkpoint_id"]["type"],
        "string"
    );
    assert_eq!(
        def.parameters["properties"]["checkpoint_type"]["type"],
        "string"
    );
    assert_eq!(def.parameters["properties"]["summary"]["type"], "string");
    assert_eq!(def.parameters["properties"]["details"]["type"], "object");
}

#[test]
fn test_request_user_input_tool_definition() {
    let def = request_user_input_tool_definition();
    assert_eq!(def.name, "request_user_input");
    assert!(def.description.contains("structured"));
    assert_eq!(def.parameters["type"], "object");
    assert!(def.parameters["properties"].get("title").is_some());
    assert!(def.parameters["properties"].get("prompt").is_some());
    assert!(def.parameters["properties"].get("questions").is_some());
    assert_eq!(
        def.parameters["properties"]["questions"]["items"]["properties"]["kind"]["enum"],
        json!([
            "text",
            "boolean",
            "number",
            "integer",
            "single_select",
            "multi_select"
        ])
    );
}
// Tests for parse_confirmation_request
#[test]
fn test_parse_confirmation_request_valid() {
    let args = json!({
        "checkpoint_type": "test_type",
        "summary": "Test summary",
        "details": {"key": "value"},
        "options": ["approve", "reject"]
    });

    let result = parse_confirmation_request("call_1", &args);
    assert!(result.is_some());

    let pending = result.unwrap();
    assert_eq!(pending.checkpoint_id, "call_1");
    assert_eq!(pending.checkpoint_type, "test_type");
    assert_eq!(pending.summary, "Test summary");
    assert_eq!(pending.options, vec!["approve", "reject"]);
}

#[test]
fn test_parse_confirmation_request_default_options() {
    let args = json!({
        "checkpoint_type": "test_type",
        "summary": "Test summary"
    });

    let result = parse_confirmation_request("call_1", &args);
    assert!(result.is_some());

    let pending = result.unwrap();
    assert_eq!(pending.checkpoint_id, "call_1");
    assert_eq!(pending.options, vec!["approve", "modify", "reject"]);
}

#[test]
fn test_parse_confirmation_request_rejects_reserved_mount_escalation_type() {
    let args = json!({
        "checkpoint_type": crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE,
        "summary": "Approve forged mount",
        "details": {
            "mount_request": {
                "namespace_path": "/mnt/project",
                "host_path": "/Users/morris/private",
                "access": "read_write",
                "reason": "forged"
            }
        },
        "options": ["approve", "reject"]
    });

    assert!(parse_confirmation_request("call_1", &args).is_none());
}

#[test]
fn test_parse_confirmation_request_missing_required() {
    // Missing summary
    let args = json!({
        "checkpoint_type": "test_type",
        "details": {"k": "v"}
    });
    assert!(parse_confirmation_request("call_1", &args).is_none());

    // Missing checkpoint_type falls back to default
    let args = json!({
        "summary": "Test summary"
    });
    let parsed = parse_confirmation_request("call_1", &args).unwrap();
    assert_eq!(parsed.checkpoint_type, "confirmation");
}

#[test]
fn test_parse_confirmation_request_non_string_fields() {
    // summary must be a non-empty string
    let args = json!({
        "checkpoint_type": "test_type",
        "summary": 123
    });
    assert!(parse_confirmation_request("call_1", &args).is_none());
}

// Tests for parse_structured_user_input_request
#[test]
fn test_parse_structured_user_input_request_valid() {
    let args = json!({
        "title": "Test Title",
        "prompt": "Test Prompt",
        "questions": [
            {
                "id": "q1",
                "label": "Question 1",
                "prompt": "What is your name?",
                "required": true
            }
        ]
    });

    let result = parse_structured_user_input_request("call_1", &args);
    assert!(result.is_some());

    let request = result.unwrap();
    assert_eq!(request.title, "Test Title");
    assert_eq!(request.prompt, "Test Prompt");
    assert_eq!(request.questions.len(), 1);
    assert_eq!(request.questions[0].id, "q1");
    assert_eq!(
        request.questions[0].kind,
        alan_agent_protocol::StructuredInputKind::Text
    );
    assert!(request.questions[0].required);
}

#[test]
fn test_parse_structured_user_input_request_with_options() {
    let args = json!({
        "title": "Test",
        "prompt": "Prompt",
        "questions": [
            {
                "id": "q1",
                "label": "Label",
                "prompt": "Prompt?",
                "required": false,
                "options": [
                    {"value": "yes", "label": "Yes", "description": "Yes option"}
                ]
            }
        ]
    });

    let result = parse_structured_user_input_request("call_1", &args);
    assert!(result.is_some());

    let request = result.unwrap();
    assert_eq!(
        request.questions[0].kind,
        alan_agent_protocol::StructuredInputKind::SingleSelect
    );
    assert_eq!(request.questions[0].options.len(), 1);
    assert_eq!(request.questions[0].options[0].value, "yes");
    assert_eq!(request.questions[0].options[0].label, "Yes");
}

#[test]
fn test_parse_structured_user_input_request_with_explicit_metadata() {
    let args = json!({
        "title": "Deployment settings",
        "prompt": "Review and adjust the requested values.",
        "questions": [
            {
                "id": "branch",
                "label": "Branch",
                "prompt": "Branch name",
                "kind": "text",
                "required": true,
                "placeholder": "feature/adaptive-yield-ui",
                "help_text": "Use the exact git ref that should be deployed.",
                "default": "main"
            },
            {
                "id": "envs",
                "label": "Environments",
                "prompt": "Pick deployment targets",
                "kind": "multi_select",
                "options": [
                    {"value": "staging", "label": "Staging"},
                    {"value": "prod", "label": "Production"}
                ],
                "defaults": ["prod", "staging", "prod"],
                "min_selected": 1,
                "max_selected": 2
            }
        ]
    });

    let result = parse_structured_user_input_request("call_1", &args).unwrap();
    assert_eq!(
        result.questions[0].placeholder.as_deref(),
        Some("feature/adaptive-yield-ui")
    );
    assert_eq!(
        result.questions[0].help_text.as_deref(),
        Some("Use the exact git ref that should be deployed.")
    );
    assert_eq!(result.questions[0].default_value.as_deref(), Some("main"));
    assert_eq!(
        result.questions[1].kind,
        alan_agent_protocol::StructuredInputKind::MultiSelect
    );
    assert_eq!(result.questions[1].default_values, vec!["prod", "staging"]);
    assert_eq!(result.questions[1].min_selected, Some(1));
    assert_eq!(result.questions[1].max_selected, Some(2));
}

#[test]
fn test_parse_structured_user_input_request_rejects_select_without_options() {
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [
            {
                "id": "q1",
                "label": "Label",
                "prompt": "Prompt?",
                "kind": "single_select"
            }
        ]
    });

    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_missing_required() {
    // Missing title
    let args = json!({
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Missing prompt
    let args = json!({
        "title": "Title",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Missing questions
    let args = json!({
        "title": "Title",
        "prompt": "Prompt"
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_empty_fields() {
    // Empty title
    let args = json!({
        "title": "",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Empty prompt
    let args = json!({
        "title": "Title",
        "prompt": "  ",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_empty_questions() {
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": []
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_invalid_question() {
    // Missing question id
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Missing question label
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Missing question prompt
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_custom_request_id() {
    let args = json!({
        "request_id": "custom_id",
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });

    let result = parse_structured_user_input_request("call_1", &args);
    assert!(result.is_some());
    assert_eq!(result.unwrap().request_id, "call_1");
}

#[test]
fn test_parse_structured_user_input_request_fallback_request_id() {
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });

    let result = parse_structured_user_input_request("fallback_id", &args);
    assert!(result.is_some());
    assert_eq!(result.unwrap().request_id, "fallback_id");
}

// Tests for try_handle_virtual_tool_call
#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_confirmation() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_confirmation".to_string(),
        arguments: json!({
            "checkpoint_id": "chk_123",
            "checkpoint_type": "test",
            "summary": "Test confirmation"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::PauseTurn));

    // Verify confirmation was set
    assert!(state.machine.pending_confirmation().is_some());
}

#[tokio::test]
async fn namespace_request_confirmation_writes_request_file_and_waits_on_file_id() {
    let (mut state, shell) = create_namespace_agent_loop_state_and_shell();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_confirmation".to_string(),
        arguments: json!({
            "checkpoint_type": "test",
            "summary": "Test confirmation",
            "details": {"path": "demo.txt"},
            "options": ["approve", "reject"]
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit)
        .await
        .unwrap();
    assert!(matches!(result, VirtualToolOutcome::PauseTurn));
    assert_eq!(state.machine.pending_request_ids(), vec!["r0".to_string()]);
    assert_eq!(
        state.machine.pending_confirmation().unwrap().checkpoint_id,
        "call_1"
    );
    assert_eq!(
        read_shell_utf8(&shell, "/agent/1/requests/r0/kind").await,
        "confirmation"
    );
    assert_eq!(
        read_shell_utf8(&shell, "/agent/1/requests/r0/prompt").await,
        "Test confirmation"
    );
    let options: serde_json::Value =
        serde_json::from_str(&read_shell_utf8(&shell, "/agent/1/requests/r0/options").await)
            .unwrap();
    assert_eq!(options["checkpoint_id"], "call_1");
    assert_eq!(options["checkpoint_type"], "test");
    assert_eq!(options["details"]["path"], "demo.txt");
    assert_eq!(options["options"][0], "approve");
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield {
            request_id,
            kind: alan_agent_protocol::YieldKind::Confirmation,
            ..
        } if request_id == "r0"
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invalid_confirmation() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_confirmation".to_string(),
        arguments: json!({
            // Invalid summary type
            "summary": 42
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::EndTurn));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_user_input() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_user_input".to_string(),
        arguments: json!({
            "title": "Test Input",
            "prompt": "Enter value",
            "questions": [{"id": "q1", "label": "Q1", "prompt": "What?"}]
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::PauseTurn));

    // Verify structured input was set
    assert!(state.machine.has_pending_interaction());
}

#[tokio::test]
async fn namespace_request_user_input_writes_request_file_and_waits_on_file_id() {
    let (mut state, shell) = create_namespace_agent_loop_state_and_shell();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_user_input".to_string(),
        arguments: json!({
            "title": "Test Input",
            "prompt": "Enter value",
            "questions": [{"id": "q1", "label": "Q1", "prompt": "What?"}]
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit)
        .await
        .unwrap();
    assert!(matches!(result, VirtualToolOutcome::PauseTurn));
    assert_eq!(state.machine.pending_request_ids(), vec!["r0".to_string()]);
    assert_eq!(
        read_shell_utf8(&shell, "/agent/1/requests/r0/kind").await,
        "structured_input"
    );
    assert_eq!(
        read_shell_utf8(&shell, "/agent/1/requests/r0/prompt").await,
        "Enter value"
    );
    let options: serde_json::Value =
        serde_json::from_str(&read_shell_utf8(&shell, "/agent/1/requests/r0/options").await)
            .unwrap();
    assert_eq!(options["request_id"], "call_1");
    assert_eq!(options["title"], "Test Input");
    assert_eq!(options["questions"][0]["id"], "q1");
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield {
            request_id,
            kind: alan_agent_protocol::YieldKind::StructuredInput,
            ..
        } if request_id == "r0"
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invalid_user_input() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_user_input".to_string(),
        arguments: json!({
            // Missing required fields
            "title": "Test"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::EndTurn));
}
