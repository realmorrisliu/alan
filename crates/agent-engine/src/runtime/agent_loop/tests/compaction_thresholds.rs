#[tokio::test]
async fn test_maybe_compact_context_no_compaction_needed() {
    let config = Config::default();
    let machine = AgentMachine::new();
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        )),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    // AgentMachine is empty, no compaction needed
    let result = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_pre_turn(),
    )
    .await;

    assert!(result.is_ok());
    assert!(events.is_empty());
}

#[tokio::test]
async fn test_maybe_compact_context_with_mock_llm() {
    let config = Config::default();
    let mut machine = AgentMachine::new();

    // Add enough messages to trigger compaction
    for i in 0..65 {
        machine.add_user_message(&format!("Message {}", i));
    }

    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "Summary",
        )),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_pre_turn(),
    )
    .await;

    // Should succeed or fail gracefully
    assert!(result.is_ok());
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the compaction fields that define this scenario"
)]
async fn test_maybe_compact_context_triggers_on_estimated_token_budget() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message(&"x".repeat(1200));
    machine.add_assistant_message(&"y".repeat(1200), None);

    let mut runtime_config = super::RuntimeConfig::default();
    runtime_config.compaction_trigger_messages = 100; // avoid message-count trigger
    runtime_config.compaction_keep_last = 1;
    runtime_config.context_window_tokens = 256;
    runtime_config.compaction_hard_trigger_ratio = 0.8;

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "Summary from token-triggered compaction",
        )),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut emit = |_event: Event| async {};
    let result = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_pre_turn(),
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(state.machine.tape_len(), 1);
    let prompt_messages = state.machine.messages_for_prompt();
    assert!(prompt_messages.iter().any(|m| {
        m.is_context()
            && m.text_content()
                .contains("Summary from token-triggered compaction")
    }));
    assert_eq!(
        state.machine.messages()[0].text_content(),
        "y".repeat(1200)
    );
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the compaction fields that define this scenario"
)]
async fn test_maybe_compact_context_triggers_immediately_when_ratio_is_zero() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message(&"x".repeat(1200));
    machine.add_assistant_message(&"y".repeat(1200), None);

    let mut runtime_config = super::RuntimeConfig::default();
    runtime_config.compaction_trigger_messages = 100; // avoid message-count trigger
    runtime_config.compaction_keep_last = 1;
    runtime_config.context_window_tokens = 16_384;
    runtime_config.compaction_hard_trigger_ratio = 0.0;

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "Summary from zero-ratio compaction",
        )),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut emit = |_event: Event| async {};
    let result = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_pre_turn(),
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(state.machine.tape_len(), 1);
    let prompt_messages = state.machine.messages_for_prompt();
    assert!(prompt_messages.iter().any(|m| {
        m.is_context()
            && m.text_content()
                .contains("Summary from zero-ratio compaction")
    }));
    assert_eq!(
        state.machine.messages()[0].text_content(),
        "y".repeat(1200)
    );
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the compaction fields that define this scenario"
)]
async fn test_maybe_compact_context_skips_when_context_window_budget_has_room() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message(&"x".repeat(1200));
    machine.add_assistant_message(&"y".repeat(1200), None);

    let mut runtime_config = super::RuntimeConfig::default();
    runtime_config.compaction_trigger_messages = 100; // avoid message-count trigger
    runtime_config.compaction_keep_last = 1;
    runtime_config.context_window_tokens = 16_384;
    runtime_config.compaction_hard_trigger_ratio = 0.8;

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "Should not compact",
        )),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let original_len = state.machine.tape_len();
    let mut emit = |_event: Event| async {};
    let result = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_pre_turn(),
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(state.machine.tape_len(), original_len);
    assert!(state.machine.tape_summary().is_none());
}

#[tokio::test]
async fn test_auto_pre_turn_soft_compaction_flushes_memory_before_compaction() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let memory_dir = temp_dir.path().join(".alan").join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

    let mut config = Config::default();
    config.memory.store_dir = Some(memory_dir.clone());

    let mut machine = AgentMachine::new();
    for i in 0..6 {
        machine.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
        machine.add_assistant_message(
            &format!("Need to preserve file paths and next steps for blocker {i}."),
            None,
        );
    }

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
        environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
            SequencedStep::Success(memory_flush_json_response()),
            SequencedStep::Success("Summary after soft-threshold compaction".to_string()),
        ])),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
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

    assert!(matches!(outcome, CompactionOutcome::Applied(_)));

    let flush_attempt = events.iter().find_map(|event| match event {
        Event::MemoryFlushObserved { attempt } => Some(attempt.clone()),
        _ => None,
    });
    let compaction_attempt = events.iter().find_map(|event| match event {
        Event::CompactionObserved { attempt } => Some(attempt.clone()),
        _ => None,
    });

    let flush_attempt = flush_attempt.expect("expected memory flush attempt");
    let compaction_attempt = compaction_attempt.expect("expected compaction attempt");
    assert_eq!(flush_attempt.result, MemoryFlushResult::Success);
    assert_eq!(flush_attempt.pressure_level, CompactionPressureLevel::Soft);
    assert_eq!(
        compaction_attempt.pressure_level,
        Some(CompactionPressureLevel::Soft)
    );
    assert_eq!(
        compaction_attempt.memory_flush_attempt_id.as_deref(),
        Some(flush_attempt.attempt_id.as_str())
    );

    let note_path = memory_dir
        .join(crate::prompts::MEMORY_DAILY_DIRNAME)
        .join(format!("{}.md", chrono::Utc::now().format("%F")));
    let note = tokio::fs::read_to_string(note_path).await.unwrap();
    assert!(note.contains("attempt_id"));
    assert!(note.contains("crates/agent-engine/src/runtime/compaction.rs"));
    assert_eq!(
        state.machine.latest_memory_flush_attempt(),
        Some(&flush_attempt)
    );
}

#[tokio::test]
async fn test_auto_pre_turn_soft_compaction_continues_after_memory_flush_failure() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let memory_dir = temp_dir.path().join(".alan").join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

    let mut config = Config::default();
    config.memory.store_dir = Some(memory_dir.clone());

    let mut machine = AgentMachine::new();
    for i in 0..6 {
        machine.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
        machine.add_assistant_message(
            &format!("Need to preserve file paths and next steps for blocker {i}."),
            None,
        );
    }

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
        environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
            SequencedStep::Error("synthetic memory flush failure".to_string()),
            SequencedStep::Success("Summary after failed memory flush".to_string()),
        ])),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
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

    assert!(matches!(outcome, CompactionOutcome::Applied(_)));

    let flush_attempt = events.iter().find_map(|event| match event {
        Event::MemoryFlushObserved { attempt } => Some(attempt.clone()),
        _ => None,
    });
    let compaction_attempt = events.iter().find_map(|event| match event {
        Event::CompactionObserved { attempt } => Some(attempt.clone()),
        _ => None,
    });
    let warnings: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            Event::Warning { message } => Some(message.clone()),
            _ => None,
        })
        .collect();

    let flush_attempt = flush_attempt.expect("expected memory flush attempt");
    let compaction_attempt = compaction_attempt.expect("expected compaction attempt");
    assert_eq!(flush_attempt.result, MemoryFlushResult::Failure);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("Silent memory flush failed"))
    );
    assert_eq!(
        compaction_attempt.memory_flush_attempt_id.as_deref(),
        Some(flush_attempt.attempt_id.as_str())
    );
    assert!(
        !memory_dir
            .join(crate::prompts::MEMORY_DAILY_DIRNAME)
            .join(format!("{}.md", chrono::Utc::now().format("%F")))
            .exists(),
        "failed memory flush should not write a daily note"
    );
}

#[tokio::test]
async fn test_auto_pre_turn_soft_compaction_skips_memory_flush_when_nothing_is_durable() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let memory_dir = temp_dir.path().join(".alan").join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

    let mut config = Config::default();
    config.memory.store_dir = Some(memory_dir.clone());

    let mut machine = AgentMachine::new();
    for i in 0..6 {
        machine.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
        machine.add_assistant_message(
            &format!("Need to preserve file paths and next steps for blocker {i}."),
            None,
        );
    }

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
        environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
            SequencedStep::Success(
                "{\"why\":\"\",\"key_decisions\":[],\"constraints\":[],\"next_steps\":[],\"important_refs\":[]}"
                    .to_string(),
            ),
            SequencedStep::Success("Summary after noop memory flush".to_string()),
        ])),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
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

    assert!(matches!(outcome, CompactionOutcome::Applied(_)));

    let flush_attempt = events.iter().find_map(|event| match event {
        Event::MemoryFlushObserved { attempt } => Some(attempt.clone()),
        _ => None,
    });
    let compaction_attempt = events.iter().find_map(|event| match event {
        Event::CompactionObserved { attempt } => Some(attempt.clone()),
        _ => None,
    });

    let flush_attempt = flush_attempt.expect("expected memory flush attempt");
    let compaction_attempt = compaction_attempt.expect("expected compaction attempt");
    assert_eq!(flush_attempt.result, MemoryFlushResult::Skipped);
    assert_eq!(
        flush_attempt.skip_reason,
        Some(alan_agent_protocol::MemoryFlushSkipReason::NoDurableContent)
    );
    assert!(flush_attempt.warning_message.is_none());
    assert!(flush_attempt.error_message.is_none());
    assert_eq!(
        compaction_attempt.memory_flush_attempt_id.as_deref(),
        Some(flush_attempt.attempt_id.as_str())
    );
    assert!(
        !memory_dir
            .join(crate::prompts::MEMORY_DAILY_DIRNAME)
            .join(format!("{}.md", chrono::Utc::now().format("%F")))
            .exists(),
        "noop memory flush should not write a daily note"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Warning { .. })),
        "noop memory flush should not emit warnings"
    );
}

#[tokio::test]
async fn test_auto_pre_turn_soft_compaction_records_already_flushed_cycle_skip() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let memory_dir = temp_dir.path().join(".alan").join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

    let mut config = Config::default();
    config.memory.store_dir = Some(memory_dir.clone());

    let mut machine = AgentMachine::new();
    for i in 0..6 {
        machine.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
        machine.add_assistant_message(
            &format!("Need to preserve file paths and next steps for blocker {i}."),
            None,
        );
    }
    machine.note_auto_memory_flush_attempt();

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
        environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
            SequencedStep::Success("Summary after already-flushed-cycle skip".to_string()),
        ])),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
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

    assert!(matches!(outcome, CompactionOutcome::Applied(_)));

    let flush_attempt = events.iter().find_map(|event| match event {
        Event::MemoryFlushObserved { attempt } => Some(attempt.clone()),
        _ => None,
    });
    let compaction_attempt = events.iter().find_map(|event| match event {
        Event::CompactionObserved { attempt } => Some(attempt.clone()),
        _ => None,
    });

    let flush_attempt = flush_attempt.expect("expected memory flush attempt");
    let compaction_attempt = compaction_attempt.expect("expected compaction attempt");
    assert_eq!(flush_attempt.result, MemoryFlushResult::Skipped);
    assert_eq!(
        flush_attempt.skip_reason,
        Some(alan_agent_protocol::MemoryFlushSkipReason::AlreadyFlushedThisCycle)
    );
    assert_eq!(
        compaction_attempt.memory_flush_attempt_id.as_deref(),
        Some(flush_attempt.attempt_id.as_str())
    );
    assert!(
        !memory_dir
            .join(crate::prompts::MEMORY_DAILY_DIRNAME)
            .join(format!("{}.md", chrono::Utc::now().format("%F")))
            .exists(),
        "already-flushed-cycle skip should not write a daily note"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Warning { .. })),
        "already-flushed-cycle skip should not emit warnings"
    );
}

#[tokio::test]
async fn test_auto_pre_turn_hard_compaction_skips_memory_flush() {
    let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
    let memory_dir = temp_dir.path().join(".alan").join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n").unwrap();

    let mut config = Config::default();
    config.memory.store_dir = Some(memory_dir);

    let mut machine = AgentMachine::new();
    for i in 0..6 {
        machine.add_user_message(&format!("Investigate blocker {i} in runtime compaction."));
        machine.add_assistant_message(
            &format!("Need to preserve file paths and next steps for blocker {i}."),
            None,
        );
    }

    let estimated_prompt_tokens = machine.estimated_prompt_tokens();
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
        environment: namespace_environment_with_provider(SequencedMockProvider::new(vec![
            SequencedStep::Success("Summary at hard threshold".to_string()),
        ])),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
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

    assert!(matches!(outcome, CompactionOutcome::Applied(_)));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::MemoryFlushObserved { .. }))
    );
    let compaction_attempt = events.iter().find_map(|event| match event {
        Event::CompactionObserved { attempt } => Some(attempt),
        _ => None,
    });
    assert_eq!(
        compaction_attempt.and_then(|attempt| attempt.pressure_level),
        Some(CompactionPressureLevel::Hard)
    );
    assert_eq!(
        compaction_attempt.and_then(|attempt| attempt.memory_flush_attempt_id.as_deref()),
        None
    );
}

#[tokio::test]
async fn test_manual_compaction_bypasses_automatic_thresholds_without_memory_flush() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message("Investigate the compaction contract.");
    machine.add_assistant_message("Need to preserve the current next step.", None);

    let runtime_config = super::RuntimeConfig {
        compaction_trigger_messages: 100,
        compaction_keep_last: 1,
        context_window_tokens: 128_000,
        compaction_soft_trigger_ratio: 0.90,
        compaction_hard_trigger_ratio: 0.95,
        ..super::RuntimeConfig::default()
    };

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "Manual compaction below threshold",
        )),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let outcome =
        maybe_compact_context_for_request(&mut state, &mut emit, CompactionRequest::manual(None))
            .await
            .unwrap();

    assert!(matches!(outcome, CompactionOutcome::Applied(_)));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::MemoryFlushObserved { .. }))
    );
    assert_eq!(
        state.machine.tape_summary(),
        Some("Manual compaction below threshold")
    );
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the compaction fields that define this scenario"
)]
async fn test_maybe_compact_context_allows_mid_turn_emergency_near_hard_limit() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message(&"x".repeat(1200));
    machine.add_assistant_message(&"y".repeat(1200), None);
    let estimated_prompt_tokens = machine.estimated_prompt_tokens();

    let mut runtime_config = super::RuntimeConfig::default();
    runtime_config.compaction_trigger_messages = 100;
    runtime_config.compaction_keep_last = 1;
    runtime_config.context_window_tokens = (estimated_prompt_tokens + 10) as u32;
    runtime_config.compaction_hard_trigger_ratio = 1.0;

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "Summary from emergency mid-turn compaction",
        )),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut emit = |_event: Event| async {};
    let result = maybe_compact_context_for_request(
        &mut state,
        &mut emit,
        CompactionRequest::automatic_mid_turn(),
    )
    .await;

    assert!(matches!(result, Ok(CompactionOutcome::Applied(_))));
    assert_eq!(
        state.machine.tape_summary(),
        Some("Summary from emergency mid-turn compaction")
    );
}
