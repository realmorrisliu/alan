use super::*;

#[test]
fn test_load_from_rollout_restores_latest_compaction_attempt_item_when_summary_is_persisted() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-compaction-attempt.jsonl");

        let attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-123".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::AutoPreTurn,
                trigger: CompactionTrigger::Auto,
                reason: CompactionReason::WindowPressure,
                focus: None,
            },
            result: CompactionResult::Retry,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: Some(5),
            input_prompt_tokens: Some(1500),
            output_prompt_tokens: Some(480),
            retry_count: 1,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(4),
            reference_context_revision_after: Some(4),
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };

        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "sess-compaction-attempt".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::CompactionAttempt(attempt.clone()),
            RolloutItem::Compacted(CompactedItem {
                message: "Summary after retry".to_string(),
                attempt_id: Some(attempt.attempt_id.clone()),
                trigger: Some(CompactionTrigger::Auto),
                reason: Some(CompactionReason::WindowPressure),
                focus: None,
                input_messages: Some(18),
                output_messages: Some(5),
                input_tokens: Some(1500),
                output_tokens: Some(480),
                duration_ms: Some(42),
                retry_count: Some(1),
                result: Some(CompactionResult::Retry),
                reference_context_revision: Some(4),
                timestamp: "2026-01-29T14:31:01Z".to_string(),
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

        assert_eq!(machine.latest_compaction_attempt(), Some(&attempt));
    });
}

#[test]
fn test_latest_compaction_attempt_from_rollout_matches_summary_by_attempt_id() {
    let completed_attempt = CompactionAttemptSnapshot {
        attempt_id: "attempt-complete".to_string(),
        submission_id: None,
        request: CompactionRequestMetadata {
            mode: CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: Some("preserve tasks".to_string()),
        },
        result: CompactionResult::Success,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(18),
        output_messages: Some(5),
        input_prompt_tokens: Some(1500),
        output_prompt_tokens: Some(480),
        retry_count: 0,
        tape_mutated: true,
        warning_message: None,
        error_message: None,
        failure_streak: None,
        reference_context_revision_before: Some(4),
        reference_context_revision_after: Some(5),
        timestamp: "2026-01-29T14:31:00Z".to_string(),
    };
    let incomplete_retry = CompactionAttemptSnapshot {
        attempt_id: "attempt-retry".to_string(),
        submission_id: None,
        request: CompactionRequestMetadata {
            mode: CompactionMode::AutoPreTurn,
            trigger: CompactionTrigger::Auto,
            reason: CompactionReason::WindowPressure,
            focus: None,
        },
        result: CompactionResult::Retry,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(24),
        output_messages: Some(8),
        input_prompt_tokens: Some(1800),
        output_prompt_tokens: Some(500),
        retry_count: 1,
        tape_mutated: true,
        warning_message: None,
        error_message: None,
        failure_streak: None,
        reference_context_revision_before: Some(5),
        reference_context_revision_after: Some(5),
        timestamp: "2026-01-29T14:32:00Z".to_string(),
    };
    let items = [
        RolloutItem::CompactionAttempt(completed_attempt.clone()),
        RolloutItem::CompactionAttempt(incomplete_retry),
        RolloutItem::Compacted(CompactedItem {
            message: "Summary after retry".to_string(),
            attempt_id: Some(completed_attempt.attempt_id.clone()),
            trigger: Some(CompactionTrigger::Manual),
            reason: Some(CompactionReason::ExplicitRequest),
            focus: Some("preserve tasks".to_string()),
            input_messages: Some(18),
            output_messages: Some(5),
            input_tokens: Some(1500),
            output_tokens: Some(480),
            duration_ms: Some(42),
            retry_count: Some(0),
            result: Some(CompactionResult::Success),
            reference_context_revision: Some(4),
            timestamp: "2026-01-29T14:31:01Z".to_string(),
        }),
    ];

    assert_eq!(
        latest_compaction_attempt_from_rollout_items(&items),
        Some(completed_attempt)
    );
}

#[test]
fn test_latest_compaction_attempt_from_rollout_ignores_incomplete_tape_mutation() {
    let failure = CompactionAttemptSnapshot {
        attempt_id: "attempt-failure".to_string(),
        submission_id: None,
        request: CompactionRequestMetadata {
            mode: CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: None,
        },
        result: CompactionResult::Failure,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(18),
        output_messages: None,
        input_prompt_tokens: Some(1400),
        output_prompt_tokens: None,
        retry_count: 0,
        tape_mutated: false,
        warning_message: Some("Preserving existing context".to_string()),
        error_message: Some("synthetic failure".to_string()),
        failure_streak: Some(1),
        reference_context_revision_before: Some(4),
        reference_context_revision_after: None,
        timestamp: "2026-01-29T14:31:00Z".to_string(),
    };
    let incomplete_retry = CompactionAttemptSnapshot {
        attempt_id: "attempt-retry".to_string(),
        submission_id: None,
        request: CompactionRequestMetadata {
            mode: CompactionMode::AutoPreTurn,
            trigger: CompactionTrigger::Auto,
            reason: CompactionReason::WindowPressure,
            focus: None,
        },
        result: CompactionResult::Retry,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(24),
        output_messages: Some(8),
        input_prompt_tokens: Some(1800),
        output_prompt_tokens: Some(500),
        retry_count: 1,
        tape_mutated: true,
        warning_message: None,
        error_message: None,
        failure_streak: None,
        reference_context_revision_before: Some(5),
        reference_context_revision_after: Some(5),
        timestamp: "2026-01-29T14:32:00Z".to_string(),
    };
    let items = [
        RolloutItem::CompactionAttempt(failure.clone()),
        RolloutItem::CompactionAttempt(incomplete_retry),
    ];

    assert_eq!(
        latest_compaction_attempt_from_rollout_items(&items),
        Some(failure)
    );
}

#[test]
fn test_latest_compaction_attempt_from_rollout_does_not_let_linked_summary_override_newer_attempt()
{
    let completed_attempt = CompactionAttemptSnapshot {
        attempt_id: "attempt-complete".to_string(),
        submission_id: None,
        request: CompactionRequestMetadata {
            mode: CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: Some("preserve tasks".to_string()),
        },
        result: CompactionResult::Success,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(18),
        output_messages: Some(5),
        input_prompt_tokens: Some(1500),
        output_prompt_tokens: Some(480),
        retry_count: 0,
        tape_mutated: true,
        warning_message: None,
        error_message: None,
        failure_streak: None,
        reference_context_revision_before: Some(4),
        reference_context_revision_after: Some(5),
        timestamp: "2026-01-29T14:31:00Z".to_string(),
    };
    let failure = CompactionAttemptSnapshot {
        attempt_id: "attempt-failure".to_string(),
        submission_id: None,
        request: CompactionRequestMetadata {
            mode: CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: None,
        },
        result: CompactionResult::Failure,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(18),
        output_messages: None,
        input_prompt_tokens: Some(1400),
        output_prompt_tokens: None,
        retry_count: 1,
        tape_mutated: false,
        warning_message: Some("Preserving existing context".to_string()),
        error_message: Some("synthetic failure".to_string()),
        failure_streak: Some(1),
        reference_context_revision_before: Some(5),
        reference_context_revision_after: None,
        timestamp: "2026-01-29T14:32:00Z".to_string(),
    };
    let items = [
        RolloutItem::CompactionAttempt(completed_attempt),
        RolloutItem::CompactionAttempt(failure.clone()),
        RolloutItem::Compacted(CompactedItem {
            message: "Summary after retry".to_string(),
            attempt_id: Some("attempt-complete".to_string()),
            trigger: Some(CompactionTrigger::Manual),
            reason: Some(CompactionReason::ExplicitRequest),
            focus: Some("preserve tasks".to_string()),
            input_messages: Some(18),
            output_messages: Some(5),
            input_tokens: Some(1500),
            output_tokens: Some(480),
            duration_ms: Some(42),
            retry_count: Some(0),
            result: Some(CompactionResult::Success),
            reference_context_revision: Some(4),
            timestamp: "2026-01-29T14:31:01Z".to_string(),
        }),
    ];

    assert_eq!(
        latest_compaction_attempt_from_rollout_items(&items),
        Some(failure)
    );
}

#[test]
fn test_load_from_rollout_does_not_let_repersisted_linked_summary_override_newer_failure() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let rollout_path = temp_dir.path().join("rollout-linked-summary.jsonl");

        let completed_attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-complete".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: Some("preserve tasks".to_string()),
            },
            result: CompactionResult::Success,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: Some(5),
            input_prompt_tokens: Some(1500),
            output_prompt_tokens: Some(480),
            retry_count: 0,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(4),
            reference_context_revision_after: Some(5),
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };
        let failure = CompactionAttemptSnapshot {
            attempt_id: "attempt-failure".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: None,
            },
            result: CompactionResult::Failure,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: None,
            input_prompt_tokens: Some(1400),
            output_prompt_tokens: None,
            retry_count: 1,
            tape_mutated: false,
            warning_message: Some("Preserving existing context".to_string()),
            error_message: Some("synthetic failure".to_string()),
            failure_streak: Some(1),
            reference_context_revision_before: Some(5),
            reference_context_revision_after: None,
            timestamp: "2026-01-29T14:32:00Z".to_string(),
        };
        let items = [
            RolloutItem::AgentMachineMeta(AgentMachineMeta {
                rollout_id: "sess-linked-summary".to_string(),
                process_path: "/proc/test".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::CompactionAttempt(completed_attempt.clone()),
            RolloutItem::Compacted(CompactedItem {
                message: "Summary after retry".to_string(),
                attempt_id: Some(completed_attempt.attempt_id.clone()),
                trigger: Some(CompactionTrigger::Manual),
                reason: Some(CompactionReason::ExplicitRequest),
                focus: Some("preserve tasks".to_string()),
                input_messages: Some(18),
                output_messages: Some(5),
                input_tokens: Some(1500),
                output_tokens: Some(480),
                duration_ms: Some(42),
                retry_count: Some(0),
                result: Some(CompactionResult::Success),
                reference_context_revision: Some(4),
                timestamp: "2026-01-29T14:31:01Z".to_string(),
            }),
            RolloutItem::CompactionAttempt(failure.clone()),
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

        assert_eq!(machine.latest_compaction_attempt(), Some(&failure));

        machine.flush().await;
        let recovered_path = machine
            .rollout_path()
            .expect("recovered machine should have rollout path")
            .clone();
        let reloaded = AgentMachine::load_from_rollout_in_dir(
            &recovered_path,
            "/proc/test",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        assert_eq!(reloaded.latest_compaction_attempt(), Some(&failure));
    });
}
