use super::*;

#[test]
fn test_terminate_child_run_tool_definition() {
    let def = terminate_child_run_tool_definition();
    assert_eq!(def.name, "terminate_child_run");
    assert!(def.description.contains("child run"));
    assert_eq!(def.parameters["type"], "object");
    assert_eq!(
        def.parameters["properties"]["child_run_id"]["type"],
        "string"
    );
    assert_eq!(def.parameters["properties"]["reason"]["type"], "string");
    assert_eq!(
        def.parameters["properties"]["mode"]["enum"],
        json!(["graceful", "forceful"])
    );
    assert_eq!(
        def.parameters["required"],
        json!(["child_run_id", "reason", "mode"])
    );
}
#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_success() {
    let mut state = create_test_agent_loop_state();
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    state
        .child_run_registry()
        .register(test_child_run_record(&child_run_id, &state.process_path()));

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "no longer needed",
            "mode": "forceful"
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

    let record = state
        .child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Terminating);
    let termination = record.termination.as_ref().unwrap();
    assert_eq!(termination.actor, "parent_runtime");
    assert_eq!(termination.reason, "no longer needed");

    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallStarted { audit: Some(audit), .. }
            if audit.action == "allow"
                && audit.capability == "write"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(true), audit: Some(audit), .. }
            if audit.action == "allow" && audit.capability == "write"
    )));

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"termination_requested\""));
    assert!(tool_result.contains("\"status\":\"terminating\""));
    assert!(tool_result.contains("\"actor\":\"parent_runtime\""));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_unknown_child() {
    let mut state = create_test_agent_loop_state();
    let child_run_id = format!("missing-child-run-{}", uuid::Uuid::new_v4());

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "stop missing child",
            "mode": "graceful"
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
        Event::ToolCallCompleted { success: Some(false), audit: Some(audit), .. }
            if audit.action == "allow" && audit.capability == "write"
    )));

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"not_found\""));
    assert!(tool_result.contains(tool_call.arguments["child_run_id"].as_str().unwrap()));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_already_terminal() {
    let mut state = create_test_agent_loop_state();
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    state
        .child_run_registry()
        .register(test_child_run_record(&child_run_id, &state.process_path()));
    state
        .child_run_registry()
        .mark_terminal(&child_run_id, ChildRunStatus::Completed, None);

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "already done",
            "mode": "graceful"
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
        Event::ToolCallCompleted { success: Some(true), audit: Some(audit), .. }
            if audit.action == "allow" && audit.capability == "write"
    )));

    let record = state
        .child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Completed);
    assert!(record.termination.is_none());

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"already_terminal\""));
    assert!(tool_result.contains("\"status\":\"completed\""));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_escalates_under_escalating_policy() {
    let mut state = create_test_agent_loop_state();
    state.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    state.runtime_config.policy_engine = crate::policy::PolicyEngine::escalate_all();
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    state
        .child_run_registry()
        .register(test_child_run_record(&child_run_id, &state.process_path()));

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "needs review",
            "mode": "graceful"
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
    assert!(state.machine.pending_confirmation().is_some());
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield { kind: alan_agent_protocol::YieldKind::Confirmation, payload, .. }
            if payload["details"]["replay_tool_call"]["tool_name"] == json!("terminate_child_run")
    )));

    let record = state
        .child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Starting);
    assert!(record.termination.is_none());
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_denied_by_policy() {
    let mut state = create_test_agent_loop_state();
    let temp = TempDir::new().unwrap();
    std::fs::write(
        temp.path().join("policy.yaml"),
        r#"
rules:
  - id: deny-child-termination
    tool: terminate_child_run
    capability: write
    action: deny
    reason: child termination disabled
default_action: allow
"#,
    )
    .unwrap();
    state.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    state.runtime_config.policy_engine =
        crate::policy::PolicyEngine::load_or_default(Some(&temp.path().join("policy.yaml")));
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    state
        .child_run_registry()
        .register(test_child_run_record(&child_run_id, &state.process_path()));

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "policy should deny",
            "mode": "graceful"
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
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(false), audit: Some(audit), .. }
            if audit.action == "deny" && audit.rule_id.as_deref() == Some("deny-child-termination")
    )));

    let record = state
        .child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Starting);
    assert!(record.termination.is_none());

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"blocked_by_policy\""));
    assert!(tool_result.contains("child termination disabled"));
}
