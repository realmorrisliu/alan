use super::*;
use tempfile::TempDir;

#[test]
fn test_rollout_item_serialization() {
    let meta = RolloutItem::AgentMachineMeta(AgentMachineMeta {
        rollout_id: "rollout-123".to_string(),
        process_path: "/proc/7".to_string(),
        started_at: "2026-01-29T14:30:52Z".to_string(),
        cwd: "/test".to_string(),
        model: "gemini-test".to_string(),
        reasoning_effort: None,
    });

    let json = serde_json::to_string(&meta).unwrap();
    assert!(json.contains("agent_machine_meta"));
    assert!(json.contains("rollout-123"));
    assert!(json.contains("/proc/7"));
    assert!(json.contains("gemini-test"));

    let deserialized: RolloutItem = serde_json::from_str(&json).unwrap();
    match deserialized {
        RolloutItem::AgentMachineMeta(m) => assert_eq!(m.rollout_id, "rollout-123"),
        _ => panic!("Expected AgentMachineMeta"),
    }
}

#[test]
fn test_message_record_serialization() {
    let msg = MessageRecord {
        role: "user".to_string(),
        content: Some("Hello".to_string()),
        tool_name: None,
        message: None,
        timestamp: "2026-01-29T14:30:55Z".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("user"));
    assert!(json.contains("Hello"));

    let deserialized: MessageRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.role, "user");
    assert_eq!(deserialized.content, Some("Hello".to_string()));
    assert!(deserialized.message.is_none());
}

#[test]
fn test_turn_context_item_serialization() {
    let ctx = TurnContextItem {
        model: "gemini-2.0-flash".to_string(),
        reasoning_effort: None,
        system_prompt: "System".to_string(),
        context_items: vec![ContextItemRecord {
            id: "onboarding".to_string(),
            kind: "static".to_string(),
            title: "Onboarding".to_string(),
            content: "Steps".to_string(),
            fingerprint: "abcd1234".to_string(),
        }],
        tools: vec!["web_search".to_string()],
        memory_enabled: true,
        active_skills: vec!["skill-1".to_string()],
        reference_context: Some(ReferenceContextSnapshotRecord {
            revision: 3,
            changed: true,
            reordered: false,
            added: 1,
            updated: 0,
            removed: 0,
        }),
        timestamp: "2026-01-29T14:30:56Z".to_string(),
    };

    let json = serde_json::to_string(&ctx).unwrap();
    assert!(json.contains("gemini-2.0-flash"));
    assert!(json.contains("onboarding"));

    let deserialized: TurnContextItem = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.model, "gemini-2.0-flash");
    assert_eq!(deserialized.context_items[0].id, "onboarding");
    assert_eq!(
        deserialized.reference_context.as_ref().map(|r| r.revision),
        Some(3)
    );
}

#[tokio::test]
async fn test_record_turn_context_persists_reasoning_effort() {
    let temp_dir = TempDir::new().unwrap();
    let recorder = RolloutRecorder::new_in_dir("machine-123", "gemini-2.0-flash", temp_dir.path())
        .await
        .unwrap();

    recorder
        .record_turn_context(
            "gemini-2.0-flash",
            Some(alan_agent_protocol::ReasoningEffort::High),
            "System",
            vec![],
            vec!["web_search".to_string()],
            true,
            vec![],
            None,
        )
        .await
        .unwrap();

    let items = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    let persisted_effort = items.into_iter().find_map(|item| match item {
        RolloutItem::TurnContext(ctx) => ctx.reasoning_effort,
        _ => None,
    });
    assert_eq!(
        persisted_effort,
        Some(alan_agent_protocol::ReasoningEffort::High)
    );
}

#[test]
fn test_turn_context_item_deserializes_without_reference_context_metadata() {
    let json = r#"{
        "model":"gemini-2.0-flash",
        "system_prompt":"System",
        "context_items":[],
        "tools":["web_search"],
        "memory_enabled":true,
        "active_skills":[],
        "timestamp":"2026-01-29T14:30:56Z"
    }"#;

    let deserialized: TurnContextItem = serde_json::from_str(json).unwrap();
    assert!(deserialized.reference_context.is_none());
}

#[test]
fn test_compacted_item_serialization() {
    let item = CompactedItem {
        message: "Summary".to_string(),
        attempt_id: Some("attempt-123".to_string()),
        trigger: Some(CompactionTrigger::Manual),
        reason: Some(CompactionReason::ExplicitRequest),
        focus: Some("preserve todos".to_string()),
        input_messages: Some(24),
        output_messages: Some(8),
        input_tokens: Some(1200),
        output_tokens: Some(400),
        duration_ms: Some(35),
        retry_count: Some(1),
        result: Some(CompactionResult::Retry),
        reference_context_revision: Some(3),
        timestamp: "2026-01-29T14:31:00Z".to_string(),
    };

    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("Summary"));
    assert!(json.contains("\"manual\""));
    assert!(json.contains("\"explicit_request\""));
    assert!(json.contains("\"preserve todos\""));

    let deserialized: CompactedItem = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.message, "Summary");
    assert_eq!(deserialized.attempt_id.as_deref(), Some("attempt-123"));
    assert_eq!(deserialized.trigger, Some(CompactionTrigger::Manual));
    assert_eq!(deserialized.reason, Some(CompactionReason::ExplicitRequest));
    assert_eq!(deserialized.focus.as_deref(), Some("preserve todos"));
    assert_eq!(deserialized.reference_context_revision, Some(3));
}

#[test]
fn test_compaction_attempt_item_serialization() {
    let attempt = CompactionAttemptSnapshot {
        attempt_id: "attempt-123".to_string(),
        submission_id: Some("sub-456".to_string()),
        request: alan_agent_protocol::CompactionRequestMetadata {
            mode: alan_agent_protocol::CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: Some("preserve todos".to_string()),
        },
        result: CompactionResult::Retry,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(12),
        output_messages: Some(4),
        input_prompt_tokens: Some(900),
        output_prompt_tokens: Some(300),
        retry_count: 1,
        tape_mutated: true,
        warning_message: None,
        error_message: None,
        failure_streak: None,
        reference_context_revision_before: Some(3),
        reference_context_revision_after: Some(3),
        timestamp: "2026-01-29T14:31:00Z".to_string(),
    };

    let json = serde_json::to_string(&RolloutItem::CompactionAttempt(attempt)).unwrap();
    assert!(json.contains("\"compaction_attempt\""));
    assert!(json.contains("\"attempt-123\""));
    assert!(json.contains("\"retry\""));

    let deserialized: RolloutItem = serde_json::from_str(&json).unwrap();
    match deserialized {
        RolloutItem::CompactionAttempt(snapshot) => {
            assert_eq!(snapshot.attempt_id, "attempt-123");
            assert_eq!(snapshot.submission_id.as_deref(), Some("sub-456"));
            assert_eq!(snapshot.result, CompactionResult::Retry);
            assert_eq!(snapshot.retry_count, 1);
        }
        other => panic!("expected compaction attempt item, got {other:?}"),
    }
}

#[test]
fn test_tool_call_record_serialization() {
    let tool = ToolCallRecord {
        name: "web_search".to_string(),
        arguments: serde_json::json!({"query": "test"}),
        result: serde_json::json!({"found": 5}),
        result_digest: Some("digest-1".to_string()),
        result_preview: Some("found".to_string()),
        redaction: Some(ToolPayloadRedactionSummary {
            redacted_fields: 1,
            truncated_values: 0,
        }),
        success: true,
        audit: None,
        timestamp: "2026-01-29T14:31:02Z".to_string(),
    };

    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("web_search"));
    assert!(json.contains("true"));

    let deserialized: ToolCallRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "web_search");
    assert!(deserialized.success);
    assert_eq!(deserialized.result_digest.as_deref(), Some("digest-1"));
    assert_eq!(deserialized.result_preview.as_deref(), Some("found"));
    assert_eq!(
        deserialized.redaction,
        Some(ToolPayloadRedactionSummary {
            redacted_fields: 1,
            truncated_values: 0,
        })
    );
}

#[test]
fn test_build_durable_tool_payload_redacts_sensitive_headers() {
    let durable = build_durable_tool_payload(&serde_json::json!({
        "github_token": "github-secret",
        "session_token": "machine-secret",
        "status": 200,
        "headers": {
            "set-cookie": "machine=secret",
            "authorization": "Bearer top-secret",
            "content-type": "application/json"
        }
    }));

    assert_eq!(
        durable.payload["github_token"],
        serde_json::json!("[REDACTED reason=secret_key]")
    );
    assert_eq!(
        durable.payload["session_token"],
        serde_json::json!("[REDACTED reason=secret_key]")
    );
    assert_eq!(
        durable.payload["headers"]["set-cookie"],
        serde_json::json!("[REDACTED reason=secret_key]")
    );
    assert_eq!(
        durable.payload["headers"]["authorization"],
        serde_json::json!("[REDACTED reason=secret_key]")
    );
    assert_eq!(
        durable.payload["headers"]["content-type"],
        serde_json::json!("application/json")
    );
    assert_eq!(
        durable.redaction,
        Some(ToolPayloadRedactionSummary {
            redacted_fields: 4,
            truncated_values: 0,
        })
    );
}

#[test]
fn test_build_durable_tool_payload_redacts_secrets_embedded_in_strings() {
    let durable = build_durable_tool_payload(&serde_json::json!({
        "output": "api_key=embedded-secret\nAuthorization: Bearer embedded-token"
    }));

    let output = durable.payload["output"]
        .as_str()
        .expect("output should remain a string");
    assert!(!output.contains("embedded-secret"));
    assert!(!output.contains("embedded-token"));
    assert!(output.contains("[REDACTED reason=secret_key]"));
    assert!(!durable.digest.is_empty());
    assert!(
        durable
            .preview
            .as_deref()
            .is_none_or(|preview| !preview.contains("embedded-secret"))
    );
}

#[test]
fn test_build_durable_tool_payload_truncates_large_strings() {
    let durable = build_durable_tool_payload(&serde_json::json!({
        "body": "x".repeat(2000)
    }));

    let body = durable.payload["body"]
        .as_str()
        .expect("body should stay a string");
    assert!(body.contains("...[truncated]"));
    assert_eq!(
        durable.redaction,
        Some(ToolPayloadRedactionSummary {
            redacted_fields: 0,
            truncated_values: 1,
        })
    );
}

#[test]
fn test_build_durable_tool_payload_preserves_bounded_evidence_projection_preview() {
    let projection = crate::evidence::project_evidence_payload(
        &serde_json::json!({
            "output": "x".repeat(crate::evidence::MAX_INLINE_EVIDENCE_BYTES + 1)
        }),
        None,
        Vec::new(),
        Some("reference_unresolvable".to_string()),
    );
    let original_preview = projection["preview"].as_str().unwrap();
    assert!(original_preview.len() > DURABLE_PAYLOAD_MAX_STRING_CHARS);

    let durable = build_durable_tool_payload(&projection);

    assert_eq!(durable.payload["preview"], projection["preview"]);
    assert_eq!(
        durable.payload["truncation"]["preview_bytes"],
        serde_json::json!(original_preview.len())
    );
    assert!(durable.redaction.is_none());
}

#[test]
fn test_effect_record_serialization() {
    let effect = EffectRecord {
        effect_id: "ef-1".to_string(),
        process_path: "/proc/7".to_string(),
        tool_call_id: "call-1".to_string(),
        idempotency_key: "idem-1".to_string(),
        effect_type: "file".to_string(),
        request_fingerprint: "fp-1".to_string(),
        result_digest: Some("digest-1".to_string()),
        result_payload: Some(serde_json::json!({"ok": true})),
        status: EffectStatus::Applied,
        applied_at: Some("2026-03-03T10:00:00Z".to_string()),
        reason: None,
        dedupe_hit: false,
        timestamp: "2026-03-03T10:00:01Z".to_string(),
    };

    let json = serde_json::to_string(&effect).unwrap();
    assert!(json.contains("ef-1"));
    assert!(json.contains("applied"));

    let deserialized: EffectRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.effect_id, "ef-1");
    assert_eq!(deserialized.status, EffectStatus::Applied);
}

#[tokio::test]
async fn test_record_tool_call_redacts_sensitive_values_before_persisting() {
    let temp_dir = TempDir::new().unwrap();
    let recorder =
        RolloutRecorder::new_in_dir("test-tool-redaction", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();

    recorder
        .record_tool_call_with_audit(
            "web_fetch",
            serde_json::json!({
                "headers": {
                    "authorization": "Bearer top-secret"
                }
            }),
            serde_json::json!({
                "headers": {
                    "set-cookie": "machine=super-secret"
                }
            }),
            true,
            None,
        )
        .await
        .unwrap();

    let items = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    let tool_call = items.into_iter().find_map(|item| match item {
        RolloutItem::ToolCall(record) => Some(record),
        _ => None,
    });

    let record = tool_call.expect("tool call should be persisted");
    assert_eq!(
        record.arguments["headers"]["authorization"],
        serde_json::json!("[REDACTED reason=secret_key]")
    );
    assert_eq!(
        record.result["headers"]["set-cookie"],
        serde_json::json!("[REDACTED reason=secret_key]")
    );
    assert!(
        record.result_digest.is_some(),
        "durable tool call should persist a result digest"
    );
}

#[test]
fn test_checkpoint_record_serialization() {
    let cp = CheckpointRecord {
        checkpoint_id: "cp-123".to_string(),
        checkpoint_type: "supplier_list".to_string(),
        summary: "Found 5 suppliers".to_string(),
        choice: Some("approved".to_string()),
        knowledge_root: Some("sha256:abc123".to_string()),
        timestamp: "2026-01-29T14:35:00Z".to_string(),
    };

    let json = serde_json::to_string(&cp).unwrap();
    assert!(json.contains("cp-123"));
    assert!(json.contains("approved"));
    assert!(json.contains("sha256:abc123"));

    let deserialized: CheckpointRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.checkpoint_id, "cp-123");
    assert_eq!(deserialized.choice, Some("approved".to_string()));
    assert_eq!(
        deserialized.knowledge_root.as_deref(),
        Some("sha256:abc123")
    );
}

#[test]
fn test_checkpoint_record_without_choice() {
    let cp = CheckpointRecord {
        checkpoint_id: "cp-456".to_string(),
        checkpoint_type: "requirements".to_string(),
        summary: "Requirements gathered".to_string(),
        choice: None,
        knowledge_root: None,
        timestamp: "2026-01-29T14:36:00Z".to_string(),
    };

    let json = serde_json::to_string(&cp).unwrap();
    assert!(json.contains("cp-456"));
    assert!(json.contains("null"));
    assert!(
        !json.contains("knowledge_root"),
        "missing root stays omitted for legacy-compatible checkpoints: {json}"
    );

    let deserialized: CheckpointRecord = serde_json::from_str(&json).unwrap();
    assert!(deserialized.choice.is_none());
    assert!(deserialized.knowledge_root.is_none());
}

#[test]
fn test_event_record_serialization() {
    let event = EventRecord {
        event_type: "thinking".to_string(),
        payload: serde_json::json!({"message": "Analyzing..."}),
        timestamp: "2026-01-29T14:37:00Z".to_string(),
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("thinking"));
    assert!(json.contains("Analyzing..."));

    let deserialized: EventRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.event_type, "thinking");
}

#[test]
fn test_rollout_recorder_clone() {
    // Just test that Clone is implemented correctly
    let _temp_dir = TempDir::new().unwrap();
    let _path = _temp_dir.path().join("test.jsonl");

    // Create a minimal recorder for testing clone
    // Note: We can't easily create a full recorder without async, but we can verify the types
    fn check_clone<T: Clone>(_: T) {}

    // Check that RolloutItem implements Clone
    let item = RolloutItem::Message(MessageRecord {
        role: "user".to_string(),
        content: Some("test".to_string()),
        tool_name: None,
        message: None,
        timestamp: "2026-01-29T14:30:55Z".to_string(),
    });
    check_clone(item);
}

#[tokio::test]
async fn test_load_history_checkpoint_item() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("checkpoint.jsonl");

    let content = r#"{"type":"checkpoint","checkpoint_id":"cp-test","checkpoint_type":"supplier_list","summary":"Test summary","choice":"approved","timestamp":"2026-01-29T14:35:00Z"}"#;

    fs::write(&file_path, content).await.unwrap();

    let items = RolloutRecorder::load_history(&file_path).await.unwrap();
    assert_eq!(items.len(), 1);

    match &items[0] {
        RolloutItem::Checkpoint(cp) => {
            assert_eq!(cp.checkpoint_id, "cp-test");
            assert_eq!(cp.checkpoint_type, "supplier_list");
            assert_eq!(cp.summary, "Test summary");
            assert_eq!(cp.choice, Some("approved".to_string()));
        }
        _ => panic!("Expected Checkpoint"),
    }
}

#[tokio::test]
async fn test_load_history_event_item() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("event.jsonl");

    let content = r#"{"type":"event","event_type":"thinking","payload":{"message":"Thinking..."},"timestamp":"2026-01-29T14:37:00Z"}"#;

    fs::write(&file_path, content).await.unwrap();

    let items = RolloutRecorder::load_history(&file_path).await.unwrap();
    assert_eq!(items.len(), 1);

    match &items[0] {
        RolloutItem::Event(evt) => {
            assert_eq!(evt.event_type, "thinking");
        }
        _ => panic!("Expected Event"),
    }
}
