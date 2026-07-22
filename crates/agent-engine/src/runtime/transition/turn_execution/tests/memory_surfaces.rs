use super::*;

#[tokio::test]
async fn test_run_turn_refreshes_memory_surfaces_when_tool_batch_ends_turn() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");

    let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "request_confirmation".to_string(),
            arguments: json!({}),
        }],
        "",
    ));
    state.core_config.memory.store_dir = Some(memory_dir.clone());
    state.machine.set_turn_activity(TurnActivityState::Running);

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Test input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Error { message, .. } if message == "Invalid confirmation request."
    )));
    assert!(memory_dir.join("handoffs").join("LATEST.md").exists());
    assert!(memory_dir.join("episodic").exists());
    assert!(
        std::fs::read_dir(memory_dir.join("daily"))
            .unwrap()
            .next()
            .is_some()
    );
}

#[tokio::test]
async fn test_run_turn_promotes_direct_user_fact_when_tool_batch_ends_turn() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");

    let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "request_confirmation".to_string(),
            arguments: json!({}),
        }],
        "",
    ));
    state.core_config.memory.store_dir = Some(memory_dir.clone());
    state.core_config.memory.enabled = true;
    state.machine.set_turn_activity(TurnActivityState::Running);

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("My name is Morris.")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    let user_memory_before =
        std::fs::read_to_string(memory_dir.join("USER.md")).unwrap_or_else(|_| String::new());
    assert!(!user_memory_before.contains("Name: Morris"));
    assert_eq!(run_deferred_runtime_actions(&mut state).await, 1);

    let user_memory = std::fs::read_to_string(memory_dir.join("USER.md")).unwrap();
    assert!(user_memory.contains("Name: Morris"));
}

#[tokio::test]
async fn test_run_turn_defers_memory_promotion_until_after_completion() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");

    let mut state = create_test_state_with_provider(FailOnMemoryPromotionProvider {
        content: "Done.".to_string(),
    });
    state.core_config.memory.store_dir = Some(memory_dir);
    state.core_config.memory.enabled = true;

    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("My name is Morris.")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::TurnCompleted { .. }))
    );
    assert_eq!(state.machine.drain_deferred_runtime_actions().len(), 1);
}

struct SlowTool {
    delay: tokio::time::Duration,
}

impl Tool for SlowTool {
    fn name(&self) -> &str {
        "slow_tool"
    }

    fn description(&self) -> &str {
        "Slow tool used to test cancellation."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Ok(json!({ "ok": true }))
        })
    }
}

#[tokio::test]
async fn test_run_turn_cancelled_tool_batch_does_not_refresh_memory_surfaces() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");

    let mut tools = ToolRegistry::new();
    tools.register(SlowTool {
        delay: tokio::time::Duration::from_millis(50),
    });
    let mut state = create_test_state_with_provider_and_tools(
        ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "slow_tool".to_string(),
                arguments: json!({}),
            }],
            "",
        ),
        tools,
    )
    .await;
    state.core_config.memory.store_dir = Some(memory_dir.clone());
    state.machine.set_turn_activity(TurnActivityState::Running);

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        cancel_for_task.cancel();
    });

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Test input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnCompleted { summary: Some(summary) }
            if summary == "Task cancelled by user"
    )));
    assert!(!memory_dir.join("handoffs").join("LATEST.md").exists());
    assert!(!memory_dir.join("episodic").exists());
    assert!(!memory_dir.join("daily").exists());
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the memory-refresh fields that define this scenario"
)]
async fn test_run_turn_tool_loop_guard_refreshes_memory_surfaces_before_completion_event() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![
            GenerationResponse {
                content: String::new(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![ToolCall {
                    id: Some("call-1".to_string()),
                    name: "update_plan".to_string(),
                    arguments: json!({
                        "explanation": "Loop 1",
                        "items": [{"id": "1", "content": "Step 1", "status": "in_progress"}]
                    }),
                }],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: String::new(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![ToolCall {
                    id: Some("call-2".to_string()),
                    name: "update_plan".to_string(),
                    arguments: json!({
                        "explanation": "Loop 2",
                        "items": [{"id": "2", "content": "Step 2", "status": "in_progress"}]
                    }),
                }],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
        ],
        Arc::clone(&generate_calls),
    );
    let mut state = create_test_state_with_provider(provider);
    state.core_config.memory.store_dir = Some(memory_dir.clone());
    state.runtime_config.max_tool_loops = 2;

    let cancel = CancellationToken::new();
    let mut saw_handoff_before_completion = false;
    let mut emit = |event: Event| {
        if matches!(
            event,
            Event::TurnCompleted {
                summary: Some(ref summary)
            } if summary == "Tool loop stopped by loop guard"
        ) {
            saw_handoff_before_completion = memory_dir.join("handoffs/LATEST.md").exists();
        }
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text(
            "Run until the loop guard stops you.",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(generate_calls.load(Ordering::SeqCst), 2);
    assert_eq!(state.machine.drain_deferred_runtime_actions().len(), 1);
    assert!(saw_handoff_before_completion);
}
