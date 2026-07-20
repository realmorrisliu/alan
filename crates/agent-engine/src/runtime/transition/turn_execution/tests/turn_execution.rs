use super::*;

#[tokio::test]
async fn test_run_turn_recovers_unavailability_claim_when_network_tool_exists() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![
            GenerationResponse {
                content: "I don't have access to real-time weather data.".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: "I'll check that using available tools.".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
        ],
        Arc::clone(&generate_calls),
    );
    let mut tools = ToolRegistry::new();
    tools.register(NetworkCapabilityTool);
    let mut state = create_test_state_with_provider_and_tools(provider, tools).await;
    let cancel = CancellationToken::new();

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("how's the weather today?")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(
        generate_calls.load(Ordering::SeqCst),
        2,
        "Guardrail should retry once before emitting a contradictory draft"
    );

    let has_guardrail_warning = events.iter().any(|event| {
        matches!(
            event,
            Event::Warning { message }
                if message.contains("Guardrail recovered")
                    && message.contains("capability_contradiction")
        )
    });
    assert!(has_guardrail_warning);

    let emitted_text = events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(emitted_text, "I'll check that using available tools.");
}

#[tokio::test]
async fn test_run_turn_keeps_truthful_network_failure_explanation() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![GenerationResponse {
            content:
                "I can't access the internet right now because that request was blocked by policy."
                    .to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        }],
        Arc::clone(&generate_calls),
    );
    let mut tools = ToolRegistry::new();
    tools.register(NetworkCapabilityTool);
    let mut state = create_test_state_with_provider_and_tools(provider, tools).await;
    state
        .machine
        .push_tape_message(Message::user("how's the weather today?"));
    state.machine.push_tape_message(Message::Assistant {
        parts: Vec::new(),
        tool_requests: vec![ToolRequest {
            id: "call_network".to_string(),
            name: "network_probe".to_string(),
            arguments: json!({}),
        }],
    });
    state.machine.add_tool_message(
        "call_network",
        "network_probe",
        json!({
            "error": "network tool blocked by policy",
            "status": "blocked_by_policy"
        }),
    );
    let cancel = CancellationToken::new();

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::ResumeTurn,
        None,
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(
        generate_calls.load(Ordering::SeqCst),
        1,
        "Truthful failure explanations should not be rewritten by the guardrail"
    );

    let has_guardrail_warning = events.iter().any(|event| {
        matches!(event, Event::Warning { message } if message.contains("Guardrail recovered"))
    });
    assert!(!has_guardrail_warning);

    let emitted_text = events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(
        emitted_text,
        "I can't access the internet right now because that request was blocked by policy."
    );

    let assistant_messages: Vec<_> = state
        .machine
        .messages()
        .iter()
        .filter(|message| matches!(message, Message::Assistant { .. }))
        .collect();
    let last_assistant = assistant_messages
        .last()
        .expect("expected final assistant message to be recorded");
    assert_eq!(
        last_assistant.non_thinking_text_content(),
        "I can't access the internet right now because that request was blocked by policy."
    );
}

#[tokio::test]
async fn test_run_turn_recovers_network_claim_after_non_network_timeout() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![
            GenerationResponse {
                content: "I can't access the internet right now.".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: "I'll check that using available tools.".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
        ],
        Arc::clone(&generate_calls),
    );
    let mut tools = ToolRegistry::new();
    tools.register(NetworkCapabilityTool);
    tools.register(ReadCapabilityTool);
    let mut state = create_test_state_with_provider_and_tools(provider, tools).await;
    state
        .machine
        .push_tape_message(Message::user("how's the weather today?"));
    state.machine.push_tape_message(Message::Assistant {
        parts: Vec::new(),
        tool_requests: vec![ToolRequest {
            id: "call_local".to_string(),
            name: "local_probe".to_string(),
            arguments: json!({}),
        }],
    });
    state.machine.add_tool_message(
        "call_local",
        "local_probe",
        json!({
            "error": "local command timed out",
            "status": "timeout"
        }),
    );
    let cancel = CancellationToken::new();

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::ResumeTurn,
        None,
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(
        generate_calls.load(Ordering::SeqCst),
        2,
        "A non-network timeout should not suppress the network contradiction recovery"
    );

    let has_guardrail_warning = events.iter().any(|event| {
        matches!(
            event,
            Event::Warning { message }
                if message.contains("Guardrail recovered")
                    && message.contains("capability_contradiction")
        )
    });
    assert!(has_guardrail_warning);

    let emitted_text = events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(emitted_text, "I'll check that using available tools.");
}

#[tokio::test]
async fn test_run_turn_resume_turn_with_steer_keeps_truthful_network_failure_explanation() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![GenerationResponse {
            content:
                "I can't access the internet right now because that request was blocked by policy."
                    .to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        }],
        Arc::clone(&generate_calls),
    );
    let mut tools = ToolRegistry::new();
    tools.register(NetworkCapabilityTool);
    let mut state = create_test_state_with_provider_and_tools(provider, tools).await;
    state
        .machine
        .push_tape_message(Message::user("earlier turn"));
    state
        .machine
        .push_tape_message(Message::assistant("earlier turn completed"));
    state
        .machine
        .push_tape_message(Message::user("how's the weather today?"));
    state.machine.push_tape_message(Message::Assistant {
        parts: Vec::new(),
        tool_requests: vec![ToolRequest {
            id: "call_network".to_string(),
            name: "network_probe".to_string(),
            arguments: json!({}),
        }],
    });
    state.machine.add_tool_message(
        "call_network",
        "network_probe",
        json!({
            "error": "network tool blocked by policy",
            "status": "blocked_by_policy"
        }),
    );
    let cancel = CancellationToken::new();

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::ResumeTurn,
        Some(vec![ContentPart::text(
            "steer: explain the network failure clearly",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(
        generate_calls.load(Ordering::SeqCst),
        1,
        "Steer input should not hide earlier failures from the same active turn"
    );

    let has_guardrail_warning = events.iter().any(|event| {
        matches!(event, Event::Warning { message } if message.contains("Guardrail recovered"))
    });
    assert!(!has_guardrail_warning);

    let emitted_text = events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(
        emitted_text,
        "I can't access the internet right now because that request was blocked by policy."
    );
}

#[tokio::test]
async fn test_run_turn_new_turn_ignores_prior_failures_without_completed_assistant_boundary() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![
            GenerationResponse {
                content: "I can't access the internet right now.".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: "I'll check that using available tools.".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
        ],
        Arc::clone(&generate_calls),
    );
    let mut tools = ToolRegistry::new();
    tools.register(NetworkCapabilityTool);
    let mut state = create_test_state_with_provider_and_tools(provider, tools).await;
    state
        .machine
        .push_tape_message(Message::user("earlier turn"));
    state.machine.push_tape_message(Message::Assistant {
        parts: Vec::new(),
        tool_requests: vec![ToolRequest {
            id: "call_network".to_string(),
            name: "network_probe".to_string(),
            arguments: json!({}),
        }],
    });
    state.machine.add_tool_message(
        "call_network",
        "network_probe",
        json!({
            "error": "network tool blocked by policy",
            "status": "blocked_by_policy"
        }),
    );
    let cancel = CancellationToken::new();

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("how's the weather today?")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(
        generate_calls.load(Ordering::SeqCst),
        2,
        "Prior-turn failures must not suppress recovery in a new turn"
    );

    let has_guardrail_warning = events.iter().any(|event| {
        matches!(
            event,
            Event::Warning { message }
                if message.contains("Guardrail recovered")
                    && message.contains("capability_contradiction")
        )
    });
    assert!(has_guardrail_warning);

    let emitted_text = events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(emitted_text, "I'll check that using available tools.");
}

#[tokio::test]
async fn test_run_turn_empty_response_fallback() {
    // Provider returns empty content
    let mut state = create_test_state_with_provider(ContentMockProvider::new(""));
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

    // Check for empty response fallback
    let has_fallback = events.iter().any(|e| {
        matches!(e, Event::TurnCompleted { summary } if summary.as_deref() == Some("Turn completed with empty response fallback"))
    });
    assert!(has_fallback, "Expected empty response fallback");

    let assistant_messages: Vec<_> = state
        .machine
        .messages()
        .iter()
        .filter(|m| matches!(m, crate::agent_machine::Message::Assistant { .. }))
        .collect();
    assert_eq!(
        assistant_messages.len(),
        1,
        "Expected fallback assistant message"
    );
    assert_eq!(
        assistant_messages[0].non_thinking_text_content(),
        "I apologize, but I couldn't generate a response."
    );
}

#[tokio::test]
async fn test_run_turn_empty_content_with_thinking_persists_reasoning() {
    let mut state = create_test_state_with_provider(
        ContentMockProvider::new("").with_thinking("internal reasoning"),
    );
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

    let assistant_messages: Vec<_> = state
        .machine
        .messages()
        .iter()
        .filter(|m| matches!(m, crate::agent_machine::Message::Assistant { .. }))
        .collect();
    assert_eq!(
        assistant_messages.len(),
        1,
        "Expected a single assistant message"
    );
    assert_eq!(
        assistant_messages[0].thinking_content().as_deref(),
        Some("internal reasoning")
    );
    assert_eq!(
        assistant_messages[0].non_thinking_text_content(),
        "I apologize, but I couldn't generate a response."
    );
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the compaction fields that define this scenario"
)]
async fn test_run_turn_performs_mid_turn_compaction_before_follow_up_generation() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![
            GenerationResponse {
                content: String::new(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![ToolCall {
                    id: Some("call-mid-turn".to_string()),
                    name: "emit_large_output".to_string(),
                    arguments: json!({}),
                }],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: "Mid-turn compaction summary".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: "Finished after compaction".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
        ],
        Arc::clone(&generate_calls),
    );
    let mut tools = ToolRegistry::new();
    tools.register(LargeOutputTool::new("very long tool output\n".repeat(600)));
    let mut state = create_test_state_with_provider_and_tools(provider, tools).await;
    state.runtime_config.compaction_trigger_messages = 1_000;
    state.runtime_config.compaction_keep_last = 1;
    state.runtime_config.context_window_tokens = 512;
    state.runtime_config.compaction_hard_trigger_ratio = 0.5;

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Use the tool and continue")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(generate_calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        state.machine.tape_summary(),
        Some("Mid-turn compaction summary")
    );
    assert_eq!(state.machine.compactions_this_turn(), 1);
    assert!(
        state
            .machine
            .messages()
            .iter()
            .any(|message| message.text_content().contains("Finished after compaction"))
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::TurnCompleted {
                summary: Some(summary)
            } if summary.contains("Task completed")
        )
    }));
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the compaction fields that define this scenario"
)]
async fn test_run_turn_resets_mid_turn_compaction_budget_for_new_turns() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![
            GenerationResponse {
                content: String::new(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![ToolCall {
                    id: Some("call-mid-turn".to_string()),
                    name: "emit_large_output".to_string(),
                    arguments: json!({}),
                }],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: "Mid-turn compaction summary".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: "Finished after compaction".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
        ],
        Arc::clone(&generate_calls),
    );
    let mut tools = ToolRegistry::new();
    tools.register(LargeOutputTool::new("very long tool output\n".repeat(600)));
    let mut state = create_test_state_with_provider_and_tools(provider, tools).await;
    state.runtime_config.compaction_trigger_messages = 1_000;
    state.runtime_config.compaction_keep_last = 1;
    state.runtime_config.context_window_tokens = 512;
    state.runtime_config.compaction_hard_trigger_ratio = 0.5;
    state.machine.record_auto_mid_turn_compaction(256);
    state.machine.record_auto_mid_turn_compaction(512);

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Use the tool and continue")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(generate_calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        state.machine.tape_summary(),
        Some("Mid-turn compaction summary")
    );
    assert_eq!(state.machine.compactions_this_turn(), 1);
}

#[tokio::test]
async fn test_run_turn_resume_turn() {
    let mut state = create_test_state_with_provider(ContentMockProvider::new("Response"));
    let cancel = CancellationToken::new();

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::ResumeTurn, // Resume, not new turn
        None,                    // No new user input
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    // Resume turn should not emit TurnStarted
    let turn_started_count = events
        .iter()
        .filter(|e| matches!(e, Event::TurnStarted {}))
        .count();
    assert_eq!(
        turn_started_count, 0,
        "Resume turn should not emit TurnStarted"
    );
}

#[tokio::test]
async fn test_run_turn_with_cancel() {
    let mut state = create_test_state_with_provider(ContentMockProvider::new("Response"));
    let cancel = CancellationToken::new();
    cancel.cancel(); // Cancel immediately

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
    // Should finish early due to cancellation
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
}

#[tokio::test]
async fn test_run_turn_with_update_plan_tool() {
    let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "update_plan".to_string(),
            arguments: json!({
                "explanation": "Test plan",
                "items": [{"id": "1", "content": "Step 1", "status": "in_progress"}]
            }),
        }],
        "", // No content, just tool call
    ));
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

    // Should report update_plan completion via tool lifecycle event.
    let has_update_plan_completion = events.iter().any(|e| {
        matches!(
            e,
            Event::ToolCallCompleted {
                id,
                result_preview: Some(preview),
                ..
            } if id == "call_1" && preview.contains("plan_updated")
        )
    });
    assert!(
        has_update_plan_completion,
        "Expected ToolCallCompleted preview for update_plan"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::PlanUpdated { explanation, items }
            if explanation.as_deref() == Some("Test plan")
                && items.len() == 1
                && items[0].content == "Step 1"
    )));
}

#[tokio::test]
async fn test_run_turn_with_confirmation_tool() {
    let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "request_confirmation".to_string(),
            arguments: json!({
                "checkpoint_id": "chk_123",
                "checkpoint_type": "test",
                "summary": "Test confirmation"
            }),
        }],
        "",
    ));
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
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Paused));

    // Should have Yield Confirmation event
    let has_confirmation = events.iter().any(|e| {
        matches!(
            e,
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                ..
            }
        )
    });
    assert!(has_confirmation, "Expected Yield Confirmation event");
}
