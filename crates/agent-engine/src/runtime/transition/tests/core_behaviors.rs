#[test]
fn test_sanitize_tool_text_for_compaction_preserves_identifiers_and_trims_noise() {
    let mut tool_output = String::new();
    tool_output.push_str("DEBUG starting noisy stream\n");
    tool_output.push_str("command: cargo test -p alan-agent-engine compact\n");
    tool_output.push_str("path: crates/agent-engine/src/tape.rs\n");
    tool_output.push_str("tool_call_id: call_123\n");
    for idx in 0..200 {
        tool_output.push_str(&format!("DEBUG noisy line {idx}\n"));
    }
    tool_output.push_str("final status: ok\n");

    let sanitized = sanitize_tool_text_for_compaction(&tool_output);
    assert!(sanitized.contains("cargo test -p alan-agent-engine compact"));
    assert!(sanitized.contains("crates/agent-engine/src/tape.rs"));
    assert!(sanitized.contains("call_123"));
    assert!(sanitized.contains("lines omitted"));
    assert!(sanitized.chars().count() < tool_output.chars().count());
}

#[test]
fn test_sanitize_tool_text_for_compaction_enforces_hard_char_cap() {
    let tool_output = "x".repeat(COMPACTION_TOOL_OUTPUT_CHAR_LIMIT * 2);

    let sanitized = sanitize_tool_text_for_compaction(&tool_output);

    assert!(sanitized.chars().count() <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT);
    assert!(sanitized.ends_with("[truncated for compaction]"));
}

#[test]
fn test_sanitize_tool_text_for_compaction_preserves_tail_identifiers_under_hard_cap() {
    let long_noise = "x".repeat(COMPACTION_TOOL_OUTPUT_CHAR_LIMIT);
    let tool_output = format!(
        "{long_noise}\n{long_noise}\n{long_noise}\npath: crates/agent-engine/src/runtime/transition.rs\ntool_call_id: call_tail_123\nfinal status: failed"
    );

    let sanitized = sanitize_tool_text_for_compaction(&tool_output);

    assert!(sanitized.chars().count() <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT);
    assert!(sanitized.contains("crates/agent-engine/src/runtime/transition.rs"));
    assert!(sanitized.contains("call_tail_123"));
    assert!(sanitized.contains("final status: failed"));
}

#[test]
fn test_normalize_tool_calls_with_ids() {
    let tool_calls = vec![
        ToolCall {
            id: Some("call_1".to_string()),
            name: "search".to_string(),
            arguments: json!({"query": "test"}),
        },
        ToolCall {
            id: Some("call_2".to_string()),
            name: "memory_write".to_string(),
            arguments: json!({"content": "data"}),
        },
    ];

    let normalized = normalize_tool_calls(tool_calls);

    assert_eq!(normalized.len(), 2);
    assert_eq!(normalized[0].id, "call_1");
    assert_eq!(normalized[0].name, "search");
    assert_eq!(normalized[1].id, "call_2");
    assert_eq!(normalized[1].name, "memory_write");
}

#[test]
fn test_normalize_tool_calls_missing_ids() {
    let tool_calls = vec![
        ToolCall {
            id: None,
            name: "search".to_string(),
            arguments: json!({}),
        },
        ToolCall {
            id: Some("".to_string()),
            name: "write".to_string(),
            arguments: json!({}),
        },
        ToolCall {
            id: Some("  ".to_string()),
            name: "read".to_string(),
            arguments: json!({}),
        },
    ];

    let normalized = normalize_tool_calls(tool_calls);

    assert_eq!(normalized.len(), 3);
    // All should have generated IDs
    assert!(!normalized[0].id.is_empty());
    assert!(!normalized[1].id.is_empty());
    assert!(!normalized[2].id.is_empty());
    // IDs should be different
    assert_ne!(normalized[0].id, normalized[1].id);
}

#[test]
fn test_normalize_tool_calls_empty() {
    let tool_calls: Vec<ToolCall> = vec![];
    let normalized = normalize_tool_calls(tool_calls);
    assert!(normalized.is_empty());
}

#[test]
fn test_split_text_for_typing() {
    let text = "Hello";
    let chunks = split_text_for_typing(text);

    assert_eq!(chunks, vec!["Hello".to_string()]);
}

#[test]
fn test_split_text_for_typing_empty() {
    let chunks = split_text_for_typing("");
    assert!(chunks.is_empty());
}

#[test]
fn test_split_text_for_typing_unicode() {
    let text = "你好";
    let chunks = split_text_for_typing(text);

    assert_eq!(chunks, vec!["你好".to_string()]);
}

#[test]
fn test_split_text_for_typing_long_text_chunks_preserve_content() {
    let text = "This is a longer sentence that should be chunked near whitespace boundaries for streaming.";
    let chunks = split_text_for_typing(text);

    assert!(chunks.len() >= 2);
    assert!(chunks.iter().all(|c| !c.is_empty()));
    assert_eq!(chunks.concat(), text);
}

#[tokio::test]
async fn test_cancel_current_task() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.set_confirmation(PendingConfirmation {
        checkpoint_id: "cp_123".to_string(),
        checkpoint_type: "test_checkpoint".to_string(),
        summary: "Test".to_string(),
        details: json!({}),
        options: vec!["approve".to_string()],
    });
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };
    state.machine.add_user_message("existing history");
    state.machine.activate_task();

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let agent_files = state.agent_files();
    let host_mount_requests = state.environment.host_mount_requests();
    let result = cancel_current_task(
        &mut state.machine,
        &agent_files,
        &host_mount_requests,
        &mut emit,
    )
    .await;

    assert!(result.is_ok());
    assert!(state.machine.pending_confirmation().is_none());
    assert!(!state.machine.has_active_task());
    assert_eq!(state.machine.messages().len(), 1);
    assert_eq!(
        state.machine.messages()[0].text_content(),
        "existing history"
    );

    // Check events
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::TurnCompleted { summary } => {
            assert_eq!(summary.as_deref(), Some("Task cancelled by user"));
        }
        _ => panic!("Expected TurnCompleted event"),
    }
}

#[tokio::test]
async fn test_handle_submission_promotes_direct_user_fact_when_replayed_tool_call_ends_turn() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join(".alan/memory");

    let checkpoint_id = "tool_escalation_call-1";
    let mut machine = AgentMachine::new();
    machine.add_user_message("My name is Morris.");

    machine.begin_turn(0);
    machine.set_confirmation(PendingConfirmation {
        checkpoint_id: checkpoint_id.to_string(),
        checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
        summary: "Replay tool call".to_string(),
        details: json!({
            "replay_tool_call": {
                "call_id": "call-1",
                "tool_name": "request_confirmation",
                "arguments": {}
            }
        }),
        options: vec!["approve".to_string(), "reject".to_string()],
    });

    let mut state = create_replay_memory_test_state(memory_dir.clone(), machine);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    let result = handle_submission_with_cancel(
        &mut state,
        Submission::new(alan_agent_protocol::Op::Resume {
            request_id: checkpoint_id.to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                json!({"choice": "approve"}),
            )],
        }),
        &mut emit,
        &cancel,
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(state.machine.turn_activity(), TurnActivityState::Idle);
    assert_eq!(run_deferred_runtime_actions(&mut state).await, 1);

    let user_memory = std::fs::read_to_string(memory_dir.join("USER.md")).unwrap();
    assert!(user_memory.contains("Name: Morris"));
}

#[tokio::test]
async fn test_handle_submission_promotes_direct_user_fact_when_replayed_tool_batch_ends_turn() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join(".alan/memory");

    let checkpoint_id = "tool_escalation_batch-1";
    let mut machine = AgentMachine::new();
    machine.add_user_message("My name is Morris.");

    machine.begin_turn(0);
    machine.set_confirmation(PendingConfirmation {
        checkpoint_id: checkpoint_id.to_string(),
        checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
        summary: "Replay tool batch".to_string(),
        details: json!({}),
        options: vec!["approve".to_string(), "reject".to_string()],
    });
    machine.set_tool_replay_batch(
        checkpoint_id,
        vec![NormalizedToolCall {
            id: "call-1".to_string(),
            name: "request_confirmation".to_string(),
            arguments: json!({}),
        }],
    );

    let mut state = create_replay_memory_test_state(memory_dir.clone(), machine);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    let result = handle_submission_with_cancel(
        &mut state,
        Submission::new(alan_agent_protocol::Op::Resume {
            request_id: checkpoint_id.to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                json!({"choice": "approve"}),
            )],
        }),
        &mut emit,
        &cancel,
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(state.machine.turn_activity(), TurnActivityState::Idle);
    assert_eq!(run_deferred_runtime_actions(&mut state).await, 1);

    let user_memory = std::fs::read_to_string(memory_dir.join("USER.md")).unwrap();
    assert!(user_memory.contains("Name: Morris"));
}

#[tokio::test]
async fn test_emit_streaming_chunks() {
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    emit_streaming_chunks(&mut emit, "Hi").await;

    // Should have: TextDelta content chunk, TextDelta final
    assert_eq!(events.len(), 2);

    match &events[0] {
        Event::TextDelta { chunk, is_final } => {
            assert_eq!(chunk, "Hi");
            assert!(!is_final);
        }
        _ => panic!("Expected TextDelta"),
    }

    match &events[1] {
        Event::TextDelta { chunk, is_final } => {
            assert!(chunk.is_empty());
            assert!(*is_final);
        }
        _ => panic!("Expected final TextDelta"),
    }
}

#[test]
fn test_transition_state_creation() {
    let config = Config::default();
    let machine = AgentMachine::new();
    let runtime_config = super::RuntimeConfig::default();

    let state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    assert!(state.machine.pending_confirmation().is_none());
}

#[test]
fn test_pending_confirmation_clone() {
    let pending = PendingConfirmation {
        checkpoint_id: "cp_123".to_string(),
        checkpoint_type: "test_checkpoint".to_string(),
        summary: "Test summary".to_string(),
        details: json!({"key": "value"}),
        options: vec!["approve".to_string(), "reject".to_string()],
    };

    let cloned = pending.clone();
    assert_eq!(pending.checkpoint_id, cloned.checkpoint_id);
    assert_eq!(pending.checkpoint_type, cloned.checkpoint_type);
    assert_eq!(pending.summary, cloned.summary);
}

#[test]
fn test_normalized_tool_call_creation() {
    let call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "search".to_string(),
        arguments: json!({"query": "test"}),
    };

    assert_eq!(call.id, "call_1");
    assert_eq!(call.name, "search");
}

// Tests for maybe_compact_context
