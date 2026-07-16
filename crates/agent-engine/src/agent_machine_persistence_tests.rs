use super::*;

#[test]
fn test_empty_message_content() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("");

    let messages = machine.tape.messages();
    assert_eq!(messages[0].text_content(), "");
}

#[test]
fn test_unicode_message_content() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("你好，世界！🌍");

    let messages = machine.tape.messages();
    assert_eq!(messages[0].text_content(), "你好，世界！🌍");
}

#[test]
fn test_record_tool_call() {
    let machine = AgentMachine::new();
    let args = serde_json::json!({"query": "test"});
    let result = serde_json::json!({"status": "ok"});

    // Should not panic without recorder
    machine.record_tool_call("search_tool", args, result, true);
}

#[test]
fn test_record_effect_updates_lookup_index() {
    let mut machine = AgentMachine::new();
    let effect = EffectRecord {
        effect_id: "ef-1".to_string(),
        process_path: "/proc/test".to_string(),
        tool_call_id: "call-1".to_string(),
        idempotency_key: "idem-1".to_string(),
        effect_type: "file".to_string(),
        request_fingerprint: "fp-1".to_string(),
        result_digest: None,
        result_payload: None,
        status: EffectStatus::Unknown,
        applied_at: None,
        reason: Some("pending".to_string()),
        dedupe_hit: false,
        timestamp: "2026-03-03T10:00:00Z".to_string(),
    };

    machine.record_effect(effect);

    let restored = machine.effect_by_idempotency_key("idem-1").unwrap();
    assert_eq!(restored.effect_id, "ef-1");
    assert_eq!(restored.status, EffectStatus::Unknown);
}

#[test]
fn test_record_checkpoint() {
    let machine = AgentMachine::new();

    // Should not panic without recorder
    machine.record_checkpoint(
        "cp-123",
        "supplier_list",
        "Test checkpoint",
        Some("approve"),
    );
}

#[tokio::test]
async fn test_flush() {
    let machine = AgentMachine::new();
    // Should not panic without recorder
    machine.flush().await;
}

#[test]
fn test_add_user_message_with_tool_name() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("Hello");
    let messages = machine.tape.messages();
    assert!(messages[0].is_user());
}

#[test]
fn test_record_event() {
    let machine = AgentMachine::new();
    // Should not panic without recorder
    machine.record_event("test_event", serde_json::json!({"key": "value"}));
}

#[test]
fn test_record_summary() {
    let machine = AgentMachine::new();
    // Should not panic without recorder
    machine.record_summary("Test summary");
}

#[test]
fn test_latest_memory_flush_attempt_from_rollout_items_returns_latest_attempt() {
    let first = MemoryFlushAttemptSnapshot {
        attempt_id: "flush-1".to_string(),
        compaction_mode: CompactionMode::AutoPreTurn,
        pressure_level: alan_agent_protocol::CompactionPressureLevel::Soft,
        result: MemoryFlushResult::Skipped,
        skip_reason: Some(MemoryFlushSkipReason::ReadOnlyMemoryDir),
        source_messages: Some(4),
        output_path: None,
        warning_message: Some("memory dir is read-only".to_string()),
        error_message: None,
        timestamp: "2026-03-03T10:00:00Z".to_string(),
    };
    let second = MemoryFlushAttemptSnapshot {
        attempt_id: "flush-2".to_string(),
        compaction_mode: CompactionMode::AutoPreTurn,
        pressure_level: alan_agent_protocol::CompactionPressureLevel::Soft,
        result: MemoryFlushResult::Success,
        skip_reason: None,
        source_messages: Some(8),
        output_path: Some(".alan/memory/daily/2026-03-03.md".to_string()),
        warning_message: None,
        error_message: None,
        timestamp: "2026-03-03T10:05:00Z".to_string(),
    };
    let items = [
        RolloutItem::MemoryFlushAttempt(first),
        RolloutItem::MemoryFlushAttempt(second.clone()),
    ];

    assert_eq!(
        latest_memory_flush_attempt_from_rollout_items(&items),
        Some(second)
    );
}

#[tokio::test]
async fn test_persist_compaction_attempt_updates_latest_and_rollout() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    let attempt = CompactionAttemptSnapshot {
        attempt_id: "attempt-123".to_string(),
        submission_id: Some("sub-456".to_string()),
        request: CompactionRequestMetadata {
            mode: CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: Some("preserve todos".to_string()),
        },
        result: CompactionResult::Success,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(10),
        output_messages: Some(3),
        input_prompt_tokens: Some(800),
        output_prompt_tokens: Some(250),
        retry_count: 0,
        tape_mutated: true,
        warning_message: None,
        error_message: None,
        failure_streak: None,
        reference_context_revision_before: Some(2),
        reference_context_revision_after: Some(2),
        timestamp: "2026-03-03T10:00:00Z".to_string(),
    };

    machine
        .persist_compaction_observation(attempt.clone(), None)
        .await
        .unwrap();
    assert_eq!(machine.latest_compaction_attempt(), Some(&attempt));

    let rollout_path = machine.rollout_path().unwrap().clone();
    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    let persisted = items.into_iter().find_map(|item| match item {
        RolloutItem::CompactionAttempt(snapshot) => Some(snapshot),
        _ => None,
    });

    assert_eq!(persisted, Some(attempt));
}

#[tokio::test]
async fn test_persist_memory_flush_attempt_updates_latest_and_rollout() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    let attempt = MemoryFlushAttemptSnapshot {
        attempt_id: "flush-123".to_string(),
        compaction_mode: CompactionMode::AutoPreTurn,
        pressure_level: alan_agent_protocol::CompactionPressureLevel::Soft,
        result: MemoryFlushResult::Success,
        skip_reason: None,
        source_messages: Some(7),
        output_path: Some(".alan/memory/daily/2026-03-03.md".to_string()),
        warning_message: None,
        error_message: None,
        timestamp: "2026-03-03T10:00:00Z".to_string(),
    };

    machine
        .persist_memory_flush_attempt(attempt.clone())
        .await
        .unwrap();
    assert_eq!(machine.latest_memory_flush_attempt(), Some(&attempt));

    let rollout_path = machine.rollout_path().unwrap().clone();
    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    let persisted = items.into_iter().find_map(|item| match item {
        RolloutItem::MemoryFlushAttempt(snapshot) => Some(snapshot),
        _ => None,
    });

    assert_eq!(persisted, Some(attempt));
}

#[tokio::test]
async fn test_persist_compaction_observation_batches_attempt_and_summary() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    let attempt = CompactionAttemptSnapshot {
        attempt_id: "attempt-batched".to_string(),
        submission_id: Some("sub-batched".to_string()),
        request: CompactionRequestMetadata {
            mode: CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: Some("preserve blockers".to_string()),
        },
        result: CompactionResult::Retry,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(10),
        output_messages: Some(3),
        input_prompt_tokens: Some(800),
        output_prompt_tokens: Some(250),
        retry_count: 1,
        tape_mutated: true,
        warning_message: None,
        error_message: None,
        failure_streak: None,
        reference_context_revision_before: Some(2),
        reference_context_revision_after: Some(2),
        timestamp: "2026-03-03T10:00:00Z".to_string(),
    };
    let compacted = CompactedItem {
        message: "Summary after retry".to_string(),
        attempt_id: Some(attempt.attempt_id.clone()),
        trigger: Some(CompactionTrigger::Manual),
        reason: Some(CompactionReason::ExplicitRequest),
        focus: Some("preserve blockers".to_string()),
        input_messages: Some(10),
        output_messages: Some(3),
        input_tokens: Some(800),
        output_tokens: Some(250),
        duration_ms: Some(35),
        retry_count: Some(1),
        result: Some(CompactionResult::Retry),
        reference_context_revision: Some(2),
        timestamp: "2026-03-03T10:00:01Z".to_string(),
    };

    machine
        .persist_compaction_observation(attempt.clone(), Some(compacted.clone()))
        .await
        .unwrap();

    let rollout_path = machine.rollout_path().unwrap().clone();
    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    let persisted_attempt = items.iter().find_map(|item| match item {
        RolloutItem::CompactionAttempt(snapshot) => Some(snapshot),
        _ => None,
    });
    let persisted_compacted = items.iter().find_map(|item| match item {
        RolloutItem::Compacted(compacted) => Some(compacted),
        _ => None,
    });

    assert_eq!(machine.latest_compaction_attempt(), Some(&attempt));
    assert_eq!(persisted_attempt, Some(&attempt));
    assert_eq!(
        persisted_compacted.map(|item| item.attempt_id.as_deref()),
        Some(Some("attempt-batched"))
    );
    assert_eq!(
        persisted_compacted.map(|item| item.message.as_str()),
        Some(compacted.message.as_str())
    );
}

#[test]
fn test_record_turn_context() {
    let machine = AgentMachine::new();
    let context_items = vec![ContextItem {
        id: "ctx-1".to_string(),
        kind: "customer".to_string(),
        title: "Customer Profile".to_string(),
        content: "Test content".to_string(),
        fingerprint: "abc123".to_string(),
    }];
    // Should not panic without recorder
    machine.record_turn_context(
        "gemini-2.0-flash",
        Some(alan_agent_protocol::ReasoningEffort::Low),
        "System prompt",
        &context_items,
        &["tool1".to_string(), "tool2".to_string()],
        true,
        &["skill1".to_string()],
    );
}

#[test]
fn test_record_turn_context_if_changed_dedupes_identical_snapshots() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let mut machine = AgentMachine::new_with_recorder_in_dir(
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        let context_items = vec![ContextItem {
            id: "ctx-1".to_string(),
            kind: "customer".to_string(),
            title: "Customer Profile".to_string(),
            content: "Test content".to_string(),
            fingerprint: "abc123".to_string(),
        }];

        let unchanged = ContextItemsDelta::default();
        assert!(machine.record_turn_context_if_changed(
            "gemini-2.0-flash",
            None,
            "System prompt",
            &context_items,
            &["tool1".to_string()],
            true,
            &["skill1".to_string()],
            &unchanged,
        ));

        assert!(!machine.record_turn_context_if_changed(
            "gemini-2.0-flash",
            None,
            "System prompt",
            &context_items,
            &["tool1".to_string()],
            true,
            &["skill1".to_string()],
            &unchanged,
        ));

        // A tool list change should still record even when reference context is unchanged.
        assert!(machine.record_turn_context_if_changed(
            "gemini-2.0-flash",
            None,
            "System prompt",
            &context_items,
            &["tool1".to_string(), "tool2".to_string()],
            true,
            &["skill1".to_string()],
            &unchanged,
        ));

        machine.flush().await;

        let rollout_path = machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let turn_context_count = items
            .into_iter()
            .filter(|item| matches!(item, RolloutItem::TurnContext(_)))
            .count();
        assert_eq!(turn_context_count, 2);
    });
}

#[test]
fn test_record_turn_context_if_changed_records_reasoning_effort_changes() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let mut machine = AgentMachine::new_with_recorder_in_dir(
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        let context_items = vec![ContextItem {
            id: "ctx-1".to_string(),
            kind: "customer".to_string(),
            title: "Customer Profile".to_string(),
            content: "Test content".to_string(),
            fingerprint: "abc123".to_string(),
        }];
        let unchanged = ContextItemsDelta::default();

        assert!(machine.record_turn_context_if_changed(
            "gemini-2.0-flash",
            Some(alan_agent_protocol::ReasoningEffort::Low),
            "System prompt",
            &context_items,
            &["tool1".to_string()],
            true,
            &["skill1".to_string()],
            &unchanged,
        ));
        assert!(machine.record_turn_context_if_changed(
            "gemini-2.0-flash",
            Some(alan_agent_protocol::ReasoningEffort::High),
            "System prompt",
            &context_items,
            &["tool1".to_string()],
            true,
            &["skill1".to_string()],
            &unchanged,
        ));

        machine.flush().await;

        let rollout_path = machine.rollout_path().unwrap().clone();
        let efforts = RolloutRecorder::load_history(&rollout_path)
            .await
            .unwrap()
            .into_iter()
            .filter_map(|item| match item {
                RolloutItem::TurnContext(ctx) => Some(ctx.reasoning_effort),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            efforts,
            vec![
                Some(alan_agent_protocol::ReasoningEffort::Low),
                Some(alan_agent_protocol::ReasoningEffort::High)
            ]
        );
    });
}

#[test]
fn test_add_tool_message_persists_redacted_tool_payload_with_tool_call_id() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use tokio::time::{Duration, Instant, sleep};

        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let mut machine = AgentMachine::new_with_recorder_in_dir(
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        machine.add_tool_message(
            "call_789",
            "web_search",
            serde_json::json!({
                "ok": true,
                "headers": {
                    "set-cookie": "machine=secret-cookie",
                    "content-type": "application/json"
                }
            }),
        );

        let rollout_path = machine.rollout_path().unwrap().clone();
        let start = Instant::now();
        let mut found = false;
        while start.elapsed() < Duration::from_secs(1) {
            if let Ok(content) = tokio::fs::read_to_string(&rollout_path).await
                && content.contains("\"role\":\"tool\"")
                && content.contains("\"tool_name\":\"call_789\"")
                && content.contains("\\\"set-cookie\\\":\\\"[REDACTED reason=secret_key]\\\"")
                && !content.contains("secret-cookie")
            {
                found = true;
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert!(
            found,
            "expected tool message with payload and tool_call_id to be persisted"
        );
    });
}

#[tokio::test]
async fn test_flush_waits_for_queued_rollout_writes() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();

    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    machine.add_user_message("u1");
    machine.add_assistant_message("a1", None);
    machine.record_event("evt", serde_json::json!({"ok": true}));
    machine.flush().await;

    let rollout_path = machine.rollout_path().unwrap().clone();
    let content = tokio::fs::read_to_string(&rollout_path).await.unwrap();
    let user_pos = content.find("\"content\":\"u1\"").unwrap();
    let assistant_pos = content.find("\"content\":\"a1\"").unwrap();
    let event_pos = content.find("\"event_type\":\"evt\"").unwrap();
    assert!(user_pos < assistant_pos);
    assert!(assistant_pos < event_pos);
}

#[tokio::test]
async fn test_rollback_records_non_durable_audit_marker() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();

    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    machine.add_user_message("u1");
    machine.add_assistant_message("a1", None);
    machine.add_user_message("u2");
    machine.add_assistant_message("a2", None);

    let removed = machine.rollback_last_turns(1);
    assert_eq!(removed.removed_turns, 1);
    assert_eq!(removed.removed_messages, 2);
    machine.flush().await;

    let rollout_path = machine.rollout_path().unwrap().clone();
    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    let rollback_event = items.into_iter().find_map(|item| match item {
        RolloutItem::Event(event) if event.event_type == "machine_rollback" => Some(event),
        _ => None,
    });

    let event = rollback_event.expect("expected machine_rollback event");
    assert_eq!(event.payload["requested_turns"], serde_json::json!(1));
    assert_eq!(event.payload["removed_turns"], serde_json::json!(1));
    assert_eq!(event.payload["removed_messages"], serde_json::json!(2));
    assert_eq!(event.payload["durable"], serde_json::json!(false));
    assert_eq!(event.payload["scope"], serde_json::json!("in_memory"));
    assert_eq!(
        event.payload["warning"],
        serde_json::json!(ROLLBACK_NON_DURABLE_WARNING)
    );
}

// Tests for payload truncation
