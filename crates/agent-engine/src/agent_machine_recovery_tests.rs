use super::*;

#[test]
fn test_load_from_rollout_recovers_only_unsettled_logical_host_mount_waits() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-host-mount-wait.jsonl");
        let pending = PendingHostMountRequest {
            request_id: "request-42".to_string(),
            tool_call_id: "call-mount".to_string(),
            namespace_path: "/mnt/project".to_string(),
            access: "read_only".to_string(),
            reason: "Read project files".to_string(),
            label: Some("Project".to_string()),
            request_events_offset: 137,
        };
        let mut items = vec![
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "test-host-mount-wait".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-07-19T00:00:00Z".to_string(),
                cwd: "/mnt/project".to_string(),
                model: "test-model".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Event(EventRecord {
                event_type: HOST_MOUNT_REQUEST_WAITING_EVENT_TYPE.to_string(),
                payload: serde_json::to_value(&pending).unwrap(),
                timestamp: "2026-07-19T00:00:01Z".to_string(),
            }),
        ];
        let waiting_payload = serde_json::to_string(&items[1]).unwrap();
        assert!(!waiting_payload.contains("host_path"));
        tokio::fs::write(
            &rollout_path,
            items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n",
        )
        .await
        .unwrap();

        let recovered = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/restarted",
            "test-model",
            temp_dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            recovered.pending_host_mount("request-42"),
            Some(pending.clone())
        );

        items.push(RolloutItem::Event(EventRecord {
            event_type: HOST_MOUNT_REQUEST_TERMINAL_EVENT_TYPE.to_string(),
            payload: serde_json::json!({
                "request_id": "request-42",
                "status": "rejected",
                "error": "User declined"
            }),
            timestamp: "2026-07-19T00:00:02Z".to_string(),
        }));
        tokio::fs::write(
            &rollout_path,
            items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n",
        )
        .await
        .unwrap();
        let settled = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/restarted-again",
            "test-model",
            temp_dir.path(),
        )
        .await
        .unwrap();
        assert!(settled.pending_host_mount("request-42").is_none());
        assert!(!settled.has_pending_interaction());
    });
}

#[test]
fn test_load_from_rollout_prefers_rich_message_payload_when_available() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-rich-message.jsonl");

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "test-rich-rollout".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Message(MessageRecord {
                role: "assistant".to_string(),
                content: Some("final answer".to_string()),
                tool_name: None,
                message: Some(Message::Assistant {
                    parts: vec![
                        ContentPart::thinking("internal reasoning"),
                        ContentPart::text("final answer"),
                    ],
                    tool_requests: vec![crate::tape::ToolRequest {
                        id: "call_123".to_string(),
                        name: "web_search".to_string(),
                        arguments: serde_json::json!({"query":"alan"}),
                    }],
                }),
                timestamp: "2026-01-29T14:30:56Z".to_string(),
            }),
        ];

        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        tokio::fs::write(&rollout_path, content).await.unwrap();
        let machine = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(machine.messages().len(), 1);
        let message = &machine.messages()[0];
        assert_eq!(
            message.thinking_content().as_deref(),
            Some("internal reasoning")
        );
        assert_eq!(message.non_thinking_text_content(), "final answer");
        assert_eq!(message.tool_requests().len(), 1);
        assert_eq!(message.tool_requests()[0].name, "web_search");
    });
}

#[test]
fn test_load_from_rollout_recovers_complete_records_before_torn_tail() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-torn-tail.jsonl");

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "test-torn-tail".to_string(),
                process_path: "/proc/1".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some("survives crash".to_string()),
                tool_name: None,
                message: Some(Message::user("survives crash")),
                timestamp: "2026-01-29T14:30:56Z".to_string(),
            }),
        ];
        let mut content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        content.push_str("\n{\"type\":\"message\",\"role\":");
        tokio::fs::write(&rollout_path, content).await.unwrap();

        let machine = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/2",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(machine.messages(), &[Message::user("survives crash")]);
    });
}

#[test]
fn test_load_from_rollout_does_not_count_runtime_confirmation_control_messages_as_turns() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-control-turn-ordinal.jsonl");

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "test-control-turn-ordinal".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
            reasoning_effort: None,
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some("run task".to_string()),
                tool_name: None,
                message: Some(Message::User {
                    parts: vec![ContentPart::text("run task")],
                }),
                timestamp: "2026-01-29T14:30:53Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some(
                    "{\"checkpoint_id\":\"tool_escalation_call-1\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"approve\"}".to_string(),
                ),
                tool_name: None,
                message: Some(Message::User {
                    parts: vec![ContentPart::structured(serde_json::json!({
                        "checkpoint_id": "tool_escalation_call-1",
                        "checkpoint_type": "tool_escalation",
                        "choice": "approve",
                        "__alan_internal_control": {
                            "kind": "tool_escalation_confirmation",
                            "version": 1,
                            "source": "runtime/submission_handlers"
                        }
                    }))],
                }),
                timestamp: "2026-01-29T14:30:54Z".to_string(),
            }),
            RolloutItem::Checkpoint(CheckpointRecord {
                checkpoint_id: "tool_escalation_call-1".to_string(),
                checkpoint_type: "tool_escalation".to_string(),
                summary: "approve side effect".to_string(),
                choice: Some("approved".to_string()),
                knowledge_root: None,
                timestamp: "2026-01-29T14:30:54Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some("next task".to_string()),
                tool_name: None,
                message: Some(Message::user("next task")),
                timestamp: "2026-01-29T14:30:55Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some(
                    "{\"checkpoint_id\":\"effect_replay_call-2\",\"checkpoint_type\":\"effect_replay_confirmation\",\"choice\":\"reject\",\"__alan_internal_control\":{\"kind\":\"effect_replay_confirmation\",\"version\":1,\"source\":\"runtime/submission_handlers\"}}"
                        .to_string(),
                ),
                tool_name: None,
                message: Some(Message::user_parts(vec![ContentPart::structured(
                    serde_json::json!({
                        "checkpoint_id": "effect_replay_call-2",
                        "checkpoint_type": "effect_replay_confirmation",
                        "choice": "reject",
                        "__alan_internal_control": {
                            "kind": "effect_replay_confirmation",
                            "version": 1,
                            "source": "runtime/submission_handlers"
                        }
                    }),
                )])),
                timestamp: "2026-01-29T14:30:56Z".to_string(),
            }),
            RolloutItem::Checkpoint(CheckpointRecord {
                checkpoint_id: "effect_replay_call-2".to_string(),
                checkpoint_type: "effect_replay_confirmation".to_string(),
                summary: "reject side effect".to_string(),
                choice: Some("rejected".to_string()),
                knowledge_root: None,
                timestamp: "2026-01-29T14:30:56Z".to_string(),
            }),
        ];

        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";

        tokio::fs::write(&rollout_path, content).await.unwrap();
        let mut machine = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(
            machine.user_turn_ordinal(),
            2,
            "only non-control user messages should increment turn ordinal during recovery"
        );
        assert_eq!(machine.user_turn_count(), 4);
        let removed = machine.rollback_last_turns(2);
        assert_eq!(
            removed.removed_messages, 4,
            "runtime control messages should remain outside logical user-turn rollback"
        );
        assert_eq!(removed.removed_turns, 2);
    });
}

#[test]
fn test_load_from_rollout_excludes_current_runtime_control_from_turn_ordinal() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir
            .path()
            .join("rollout-strict-control-without-checkpoint.jsonl");

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "test-strict-control-without-checkpoint".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some(
                    "{\"checkpoint_id\":\"tool_escalation_call-11\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"approve\",\"__alan_internal_control\":{\"kind\":\"tool_escalation_confirmation\",\"version\":1,\"source\":\"runtime/submission_handlers\"}}"
                        .to_string(),
                ),
                tool_name: None,
                message: Some(Message::User {
                    parts: vec![ContentPart::structured(serde_json::json!({
                        "checkpoint_id": "tool_escalation_call-11",
                        "checkpoint_type": "tool_escalation",
                        "choice": "approve",
                        "__alan_internal_control": {
                            "kind": "tool_escalation_confirmation",
                            "version": 1,
                            "source": "runtime/submission_handlers"
                        }
                    }))],
                }),
                timestamp: "2026-01-29T14:30:53Z".to_string(),
            }),
        ];

        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";

        tokio::fs::write(&rollout_path, content).await.unwrap();
        let machine = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(machine.user_turn_ordinal(), 0);
        assert_eq!(machine.user_turn_count(), 1);
    });
}

#[test]
fn test_load_from_rollout_counts_user_payloads_without_internal_control_marker() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-user-payload-turn-ordinal.jsonl");

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "test-user-payload-turn-ordinal".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some(
                    "{\"checkpoint_id\":\"custom-id\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"approve\"}"
                        .to_string(),
                ),
                tool_name: None,
                message: Some(Message::User {
                    parts: vec![ContentPart::structured(serde_json::json!({
                        "checkpoint_id": "custom-id",
                        "checkpoint_type": "tool_escalation",
                        "choice": "approve",
                    }))],
                }),
                timestamp: "2026-01-29T14:30:53Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some(
                    "{\"checkpoint_id\":\"manual-id\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"reject\"}"
                        .to_string(),
                ),
                tool_name: None,
                message: Some(Message::user_parts(vec![ContentPart::structured(
                    serde_json::json!({
                        "checkpoint_id": "manual-id",
                        "checkpoint_type": "tool_escalation",
                        "choice": "reject"
                    }),
                )])),
                timestamp: "2026-01-29T14:30:54Z".to_string(),
            }),
        ];

        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";

        tokio::fs::write(&rollout_path, content).await.unwrap();
        let machine = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(
            machine.user_turn_ordinal(),
            2,
            "user payloads without internal control markers should count as turns"
        );
        assert_eq!(machine.user_turn_count(), 2);
    });
}

#[test]
fn test_load_from_rollout_preserves_turn_ordinal_across_repeated_recovery() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-turn-ordinal-recovery.jsonl");

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "test-turn-ordinal-recovery".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some("task one".to_string()),
                tool_name: None,
                message: Some(Message::user("task one")),
                timestamp: "2026-01-29T14:30:53Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "assistant".to_string(),
                content: Some("ack".to_string()),
                tool_name: None,
                message: Some(Message::assistant("ack")),
                timestamp: "2026-01-29T14:30:54Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some("task two".to_string()),
                tool_name: None,
                message: Some(Message::user("task two")),
                timestamp: "2026-01-29T14:30:55Z".to_string(),
            }),
        ];

        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";

        tokio::fs::write(&rollout_path, content).await.unwrap();

        let first = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(first.user_turn_ordinal(), 2);
        drop(first);

        let second = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();
        assert_eq!(
            second.user_turn_ordinal(),
            2,
            "recovered history should preserve monotonic turn ordinal across repeated recovery"
        );
    });
}

#[test]
fn test_load_from_rollout_preserves_turn_ordinal_floor_from_effect_keys_after_compaction() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-compaction-turn-floor.jsonl");

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "sess-compaction-floor".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Compacted(CompactedItem {
                message: "Older turns compacted".to_string(),
                attempt_id: None,
                trigger: None,
                reason: None,
                focus: None,
                input_messages: None,
                output_messages: None,
                input_tokens: None,
                output_tokens: None,
                duration_ms: None,
                retry_count: None,
                result: None,
                reference_context_revision: None,
                timestamp: "2026-01-29T14:31:00Z".to_string(),
            }),
            RolloutItem::Message(MessageRecord {
                role: "user".to_string(),
                content: Some("latest visible turn".to_string()),
                tool_name: None,
                message: Some(Message::user("latest visible turn")),
                timestamp: "2026-01-29T14:31:01Z".to_string(),
            }),
            RolloutItem::Effect(EffectRecord {
                effect_id: "ef-compaction".to_string(),
                process_path: "sess-compaction-floor".to_string(),
                tool_call_id: "call-1".to_string(),
                idempotency_key: "machine:turn:7:fp-1".to_string(),
                effect_type: "file".to_string(),
                request_fingerprint: "fp-1".to_string(),
                result_digest: Some("digest-1".to_string()),
                result_payload: Some(serde_json::json!({"ok": true})),
                status: EffectStatus::Applied,
                applied_at: Some("2026-01-29T14:31:02Z".to_string()),
                reason: None,
                dedupe_hit: false,
                timestamp: "2026-01-29T14:31:02Z".to_string(),
            }),
        ];

        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        tokio::fs::write(&rollout_path, content).await.unwrap();

        let machine = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(
            machine.user_turn_ordinal(),
            7,
            "effect idempotency keys should preserve turn ordinal floor after compaction"
        );
        assert_eq!(machine.user_turn_count(), 1);
    });
}

#[test]
fn test_load_from_rollout_preserves_generic_event_records_across_recovery() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-events.jsonl");

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "sess-events".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Event(EventRecord {
                event_type: "custom_event".to_string(),
                payload: serde_json::json!({
                    "phase": "testing",
                    "value": 5
                }),
                timestamp: "2026-01-29T14:31:00Z".to_string(),
            }),
        ];

        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        tokio::fs::write(&rollout_path, content).await.unwrap();

        let machine = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();
        machine.flush().await;

        let recovered_path = machine
            .rollout_path()
            .expect("recovered machine should have rollout path")
            .clone();
        let recovered_items = RolloutRecorder::load_history(&recovered_path)
            .await
            .unwrap();

        let event = recovered_items.into_iter().find_map(|item| match item {
            RolloutItem::Event(event) if event.event_type == "custom_event" => Some(event),
            _ => None,
        });

        let event = event.expect("expected recovered custom event");
        assert_eq!(event.payload["phase"], "testing");
        assert_eq!(event.payload["value"], 5);
        assert_eq!(event.timestamp, "2026-01-29T14:31:00Z");
    });
}

#[test]
fn test_load_from_rollout_repersists_memory_flush_attempt_records() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-memory-flush-attempt.jsonl");

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

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "sess-memory-flush-attempt".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-03-03T09:59:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::MemoryFlushAttempt(attempt.clone()),
        ];

        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        tokio::fs::write(&rollout_path, content).await.unwrap();

        let machine = AgentMachine::load_from_rollout_in_dir(
            &rollout_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(machine.latest_memory_flush_attempt(), Some(&attempt));

        machine.flush().await;
        let recovered_path = machine
            .rollout_path()
            .expect("recovered machine should have rollout path")
            .clone();
        let recovered_items = RolloutRecorder::load_history(&recovered_path)
            .await
            .unwrap();

        let persisted = recovered_items.into_iter().find_map(|item| match item {
            RolloutItem::MemoryFlushAttempt(snapshot) => Some(snapshot),
            _ => None,
        });

        assert_eq!(persisted, Some(attempt));
    });
}
