#[tokio::test]
async fn test_manual_compaction_records_audit_fields() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let config = Config::default();
    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    for i in 0..65 {
        machine.add_user_message(&format!("Message {}", i));
    }

    let rollout_path = machine.rollout_path().unwrap().clone();
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "Manual compaction summary",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut submission = Submission::new(Op::CompactWithOptions {
        focus: Some("preserve todos and constraints".into()),
    });
    submission.id = "sub-compact".into();
    advance_accepted_submission(
        &mut state,
        submission,
        &TurnInputBroker::default(),
        &CancellationToken::new(),
    ).await.result.unwrap();
    assert_eq!(state.machine.current_submission_id(), None);
    state.machine.flush().await;

    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    let attempt = items.iter().find_map(|item| match item {
        RolloutItem::CompactionAttempt(attempt) => Some(attempt),
        _ => None,
    });
    let compacted = items.iter().find_map(|item| match item {
        RolloutItem::Compacted(compacted) => Some(compacted),
        _ => None,
    });

    let attempt = attempt.expect("expected compaction attempt rollout item");
    let compacted = compacted.expect("expected compacted rollout item");
    assert_eq!(attempt.result, CompactionResult::Success);
    assert_eq!(attempt.submission_id.as_deref(), Some("sub-compact"));
    assert_eq!(attempt.request.trigger, CompactionTrigger::Manual);
    assert_eq!(attempt.request.reason, CompactionReason::ExplicitRequest);
    assert_eq!(
        attempt.request.focus.as_deref(),
        Some("preserve todos and constraints")
    );
    assert!(attempt.tape_mutated);
    assert_eq!(
        compacted.attempt_id.as_deref(),
        Some(attempt.attempt_id.as_str())
    );
    assert_eq!(compacted.message, "Manual compaction summary");
    assert_eq!(compacted.trigger, Some(CompactionTrigger::Manual));
    assert_eq!(compacted.reason, Some(CompactionReason::ExplicitRequest));
    assert_eq!(
        compacted.focus.as_deref(),
        Some("preserve todos and constraints")
    );
    assert_eq!(compacted.result, Some(CompactionResult::Success));
    assert!(compacted.input_messages.is_some());
    assert!(compacted.output_messages.is_some());
    assert!(compacted.input_tokens.is_some());
    assert!(compacted.output_tokens.is_some());
    assert!(compacted.duration_ms.is_some());
    assert_eq!(compacted.reference_context_revision, Some(0));

}

#[tokio::test]
async fn test_compaction_retry_result_is_audited_when_trimming_succeeds() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let config = Config::default();
    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    for i in 0..65 {
        machine.add_user_message(&format!("Message {}", i));
    }

    let rollout_path = machine.rollout_path().unwrap().clone();
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(FailThenSucceedMockProvider::new(
            1,
            "Compaction summary after retry",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut emit = |_event: Event| async {};
    let outcome =
        maybe_compact_context_for_request(&mut state, &mut emit, CompactionRequest::manual(None))
            .await
            .unwrap();
    state.machine.flush().await;

    match outcome {
        CompactionOutcome::Applied(outcome) => {
            assert_eq!(outcome.result, CompactionResult::Retry);
        }
        other => panic!("expected compaction to apply after retry, got {other:?}"),
    }

    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    let attempt = items.iter().find_map(|item| match item {
        RolloutItem::CompactionAttempt(attempt) => Some(attempt),
        _ => None,
    });
    let compacted = items.iter().find_map(|item| match item {
        RolloutItem::Compacted(compacted) => Some(compacted),
        _ => None,
    });

    let attempt = attempt.expect("expected compaction attempt rollout item");
    let compacted = compacted.expect("expected compacted rollout item");
    assert_eq!(attempt.result, CompactionResult::Retry);
    assert_eq!(attempt.retry_count, 1);
    assert!(attempt.tape_mutated);
    assert_eq!(
        compacted.attempt_id.as_deref(),
        Some(attempt.attempt_id.as_str())
    );
    assert_eq!(compacted.message, "Compaction summary after retry");
    assert_eq!(compacted.retry_count, Some(1));
    assert_eq!(compacted.result, Some(CompactionResult::Retry));
}

#[tokio::test]
async fn test_compaction_generation_failure_uses_degraded_fallback_and_audits_it() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let config = Config::default();
    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    for i in 0..65 {
        machine.add_user_message(&format!("Message {}", i));
    }

    let estimated_prompt_tokens = machine.estimated_prompt_tokens();
    let rollout_path = machine.rollout_path().unwrap().clone();
    let runtime_config = super::RuntimeConfig {
        compaction_trigger_messages: 100,
        compaction_keep_last: 1,
        context_window_tokens: ((estimated_prompt_tokens as f64) / 0.95).ceil() as u32,
        compaction_soft_trigger_ratio: 0.70,
        compaction_hard_trigger_ratio: 0.80,
        ..super::RuntimeConfig::default()
    };

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(ErrorMockProvider::new(
            "synthetic compaction failure",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let outcome = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_pre_turn(),
    )
    .await
    .unwrap();

    match outcome {
        CompactionOutcome::Applied(outcome) => {
            assert_eq!(outcome.result, CompactionResult::Degraded);
        }
        _ => panic!("expected degraded compaction to apply"),
    }
    assert!(
        state
            .machine
            .tape_summary()
            .is_some_and(|summary| summary.contains("Deterministic fallback summary"))
    );
    assert!(events.iter().any(|event| {
        matches!(event, Event::Warning { message } if message.contains("deterministic fallback summary"))
    }));

    state.machine.flush().await;
    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    let compacted = items.iter().find_map(|item| match item {
        RolloutItem::Compacted(compacted) => Some(compacted),
        _ => None,
    });
    let compacted = compacted.expect("expected compacted rollout item");
    assert_eq!(compacted.result, Some(CompactionResult::Degraded));

    let attempt = items.iter().find_map(|item| match item {
        RolloutItem::CompactionAttempt(attempt) => Some(attempt),
        _ => None,
    });
    let attempt = attempt.expect("expected compaction attempt item");
    assert_eq!(attempt.result, CompactionResult::Degraded);
    assert_eq!(attempt.pressure_level, Some(CompactionPressureLevel::Hard));
    assert!(attempt.tape_mutated);
    assert_eq!(
        compacted.attempt_id.as_deref(),
        Some(attempt.attempt_id.as_str())
    );
}

#[tokio::test]
async fn test_degraded_compaction_rebases_active_turn_start() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message("older turn 1");
    machine.add_user_message("older turn 2");
    machine.add_user_message("current turn");

    let runtime_config = super::RuntimeConfig {
        compaction_keep_last: 1,
        context_window_tokens: 1,
        compaction_hard_trigger_ratio: 0.80,
        ..super::RuntimeConfig::default()
    };

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(ErrorMockProvider::new(
            "synthetic compaction failure",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let retention_start = state
        .machine
        .compaction_retention_start(state.runtime_config.compaction_keep_last);
    assert!(retention_start > 0);
    state.machine.begin_turn(retention_start);

    let mut emit = |_event: Event| async {};
    let outcome = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_pre_turn(),
    )
    .await
    .unwrap();

    match outcome {
        CompactionOutcome::Applied(outcome) => {
            assert_eq!(outcome.result, CompactionResult::Degraded);
        }
        _ => panic!("expected degraded compaction to apply"),
    }

    assert_eq!(state.machine.active_turn_message_start(), Some(0));
}

#[tokio::test]
async fn test_manual_compaction_failure_below_hard_threshold_preserves_tape() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let config = Config::default();
    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    for i in 0..15 {
        machine.add_user_message(&format!("User message {i}"));
        machine.add_assistant_message(&format!("Assistant reply {i}"), None);
    }

    let original_messages = stateful_messages_snapshot(&machine);
    let rollout_path = machine.rollout_path().unwrap().clone();
    let runtime_config = super::RuntimeConfig {
        compaction_keep_last: 4,
        ..super::RuntimeConfig::default()
    };

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(ErrorMockProvider::new(
            "synthetic manual compaction failure",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let outcome = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::manual(Some("preserve open todos".to_string())),
    )
    .await
    .unwrap();

    assert!(matches!(outcome, CompactionOutcome::Failed(_)));
    assert_eq!(
        stateful_messages_snapshot(&state.machine),
        original_messages
    );
    assert!(state.machine.tape_summary().is_none());
    assert!(events.iter().any(|event| {
        matches!(event, Event::Warning { message } if message.contains("Preserving existing context."))
    }));

    state.machine.flush().await;
    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    assert!(
        !items
            .iter()
            .any(|item| matches!(item, RolloutItem::Compacted(_))),
        "failed manual compaction below hard threshold must not write Compacted"
    );
    let attempt = items
        .iter()
        .find_map(|item| match item {
            RolloutItem::CompactionAttempt(attempt) => Some(attempt),
            _ => None,
        })
        .expect("expected compaction attempt item");
    assert_eq!(attempt.result, CompactionResult::Failure);
    assert!(!attempt.tape_mutated);
}

#[tokio::test]
async fn test_auto_pre_turn_soft_compaction_failure_preserves_tape() {
    let mut config = Config::default();
    config.memory.enabled = false;
    let mut machine = AgentMachine::new();
    for i in 0..6 {
        machine.add_user_message(&format!("Investigate soft failure {i}"));
        machine.add_assistant_message(&format!("Assistant context {i}"), None);
    }

    let original_messages = stateful_messages_snapshot(&machine);
    let estimated_prompt_tokens = machine.estimated_prompt_tokens();
    let runtime_config = super::RuntimeConfig {
        compaction_trigger_messages: 100,
        compaction_keep_last: 1,
        context_window_tokens: ((estimated_prompt_tokens as f64) / 0.75).ceil() as u32,
        compaction_soft_trigger_ratio: 0.70,
        compaction_hard_trigger_ratio: 0.85,
        ..super::RuntimeConfig::default()
    };

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(ErrorMockProvider::new(
            "synthetic soft compaction failure",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let outcome = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_pre_turn(),
    )
    .await
    .unwrap();

    assert!(matches!(outcome, CompactionOutcome::Failed(_)));
    assert_eq!(
        stateful_messages_snapshot(&state.machine),
        original_messages
    );
    assert!(state.machine.tape_summary().is_none());
    let attempt = events
        .iter()
        .find_map(|event| match event {
            Event::CompactionObserved { attempt } => Some(attempt),
            _ => None,
        })
        .expect("expected compaction attempt event");
    assert_eq!(attempt.pressure_level, Some(CompactionPressureLevel::Soft));
    assert_eq!(attempt.result, CompactionResult::Failure);
    assert!(!attempt.tape_mutated);
}

#[test]
fn test_build_degraded_compaction_summary_bounds_prior_summary_growth() {
    let huge_summary = "legacy summary ".repeat(1_000);
    let messages = vec![
        crate::tape::Message::user("user context ".repeat(40)),
        crate::tape::Message::assistant("assistant context ".repeat(40)),
    ];

    let summary_one = build_degraded_compaction_summary(&messages, Some(&huge_summary)).unwrap();
    let summary_two = build_degraded_compaction_summary(&messages, Some(&summary_one)).unwrap();

    assert!(summary_one.contains("Prior summary excerpt:"));
    assert!(summary_one.chars().count() <= DEGRADED_COMPACTION_SUMMARY_MAX_CHARS);
    assert!(summary_two.contains("Prior summary excerpt:"));
    assert!(summary_two.chars().count() <= DEGRADED_COMPACTION_SUMMARY_MAX_CHARS);
}

#[test]
fn test_build_degraded_compaction_summary_bounds_existing_summary_without_snippets() {
    let huge_summary = "legacy summary ".repeat(1_000);
    let summary = build_degraded_compaction_summary(
        &[crate::tape::Message::context("reference-only")],
        Some(&huge_summary),
    )
    .unwrap();

    assert!(summary.chars().count() <= DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS);
}

#[tokio::test]
async fn test_compaction_failure_without_fallback_escalates_warning_and_preserves_tape() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let config = Config::default();
    let mut machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    for _ in 0..65 {
        machine.push_tape_message(crate::tape::Message::assistant(""));
    }

    let original_messages = stateful_messages_snapshot(&machine);
    let rollout_path = machine.rollout_path().unwrap().clone();
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(ErrorMockProvider::new(
            "synthetic compaction failure",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let first =
        maybe_compact_context_for_request(&mut state, &mut emit, CompactionRequest::manual(None))
            .await
            .unwrap();
    let second =
        maybe_compact_context_for_request(&mut state, &mut emit, CompactionRequest::manual(None))
            .await
            .unwrap();

    assert!(matches!(first, CompactionOutcome::Failed(_)));
    assert!(matches!(second, CompactionOutcome::Failed(_)));
    assert_eq!(
        stateful_messages_snapshot(&state.machine),
        original_messages
    );
    assert!(state.machine.tape_summary().is_none());

    let warning_messages: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            Event::Warning { message } => Some(message.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(warning_messages.len(), 2);
    assert!(warning_messages[1].contains("consider starting a new machine"));

    state.machine.flush().await;
    let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
    let failure_attempts: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            RolloutItem::CompactionAttempt(attempt) => Some(attempt),
            _ => None,
        })
        .collect();
    assert_eq!(failure_attempts.len(), 2);
    assert!(
        failure_attempts
            .iter()
            .all(|attempt| attempt.result == CompactionResult::Failure && !attempt.tape_mutated)
    );
}
