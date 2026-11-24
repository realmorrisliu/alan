use super::*;

#[tokio::test]
async fn test_run_turn_includes_runtime_recall_bundle_for_identity_query() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let memory_dir = definition_root.join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    std::fs::write(
        memory_dir.join("USER.md"),
        "# User Memory\n- Favorite runtime marker: ALAN_IDENTITY_RECALL\n",
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(
        RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_IDENTITY_RECALL",
            seen_system_prompts.clone(),
        )
        .with_seen_requests(seen_requests.clone()),
    );
    state.core_config.memory.store_dir = Some(memory_dir);
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text(
            "What is my favorite runtime marker?",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let requests = seen_requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|req| {
            req.messages.iter().any(|msg| {
                msg.role == alan_llm::MessageRole::Context
                    && msg.content.contains("## Runtime Recall Bundle")
            })
        })
        .expect("expected trailing context message with runtime recall bundle");
    assert!(
        !request
            .system_prompt
            .as_deref()
            .unwrap_or_default()
            .contains("## Runtime Recall Bundle")
    );
    let context = request
        .messages
        .iter()
        .rev()
        .find(|msg| msg.role == alan_llm::MessageRole::Context)
        .unwrap();
    assert!(context.content.contains("## Runtime Recall Bundle"));
    assert!(context.content.contains("/memory/USER.md"));
    assert!(context.content.contains("ALAN_IDENTITY_RECALL"));
}

#[tokio::test]
async fn test_run_turn_includes_runtime_recall_bundle_for_continuity_query() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let memory_dir = definition_root.join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    std::fs::write(
        memory_dir.join("handoffs/LATEST.md"),
        "# Latest Handoff\n- Continuity marker: ALAN_CONTINUITY_RECALL\n",
    )
    .unwrap();
    std::fs::create_dir_all(memory_dir.join("episodic/2026/04/15")).unwrap();
    std::fs::write(
        memory_dir.join("episodic/2026/04/15/process-1.md"),
        "# Agent Process Activity\n- Continuity marker: ALAN_CONTINUITY_RECALL\n",
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(
        RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_CONTINUITY_RECALL",
            seen_system_prompts.clone(),
        )
        .with_seen_requests(seen_requests.clone()),
    );
    state.core_config.memory.store_dir = Some(memory_dir);
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text(
            "What was the previous Agent Process doing?",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let requests = seen_requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|req| {
            req.messages.iter().any(|msg| {
                msg.role == alan_llm::MessageRole::Context
                    && msg.content.contains("## Runtime Recall Bundle")
            })
        })
        .expect("expected trailing context message with runtime recall bundle");
    assert!(
        !request
            .system_prompt
            .as_deref()
            .unwrap_or_default()
            .contains("## Runtime Recall Bundle")
    );
    let context = request
        .messages
        .iter()
        .rev()
        .find(|msg| msg.role == alan_llm::MessageRole::Context)
        .unwrap();
    assert!(context.content.contains("## Runtime Recall Bundle"));
    assert!(context.content.contains("/memory/handoffs/LATEST.md"));
    assert!(
        context
            .content
            .contains("/memory/episodic/2026/04/15/process-1.md")
    );
    assert!(context.content.contains("ALAN_CONTINUITY_RECALL"));
}

#[tokio::test]
async fn test_run_turn_includes_runtime_recall_bundle_for_recent_query_fallback() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let memory_dir = definition_root.join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    std::fs::create_dir_all(memory_dir.join("episodic/2026/04/16")).unwrap();
    for index in 1..=4 {
        std::fs::write(
            memory_dir.join(format!("topics/recent-match-{index}.md")),
            format!("# Topic Note\nwe did document topic match {index}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        memory_dir.join("daily/2026-04-16.md"),
        "## 2026-04-16\nALAN_RECENT_RECALL\n",
    )
    .unwrap();
    for index in 1..=4 {
        std::fs::write(
            memory_dir.join(format!("episodic/2026/04/16/process-{index}.md")),
            format!("# Agent Process Activity\nALAN_RECENT_RECALL_{index}\n"),
        )
        .unwrap();
    }

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(
        RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_RECENT_RECALL_4",
            seen_system_prompts.clone(),
        )
        .with_seen_requests(seen_requests.clone()),
    );
    state.core_config.memory.store_dir = Some(memory_dir);
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("What did we do yesterday?")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let requests = seen_requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|req| {
            req.messages.iter().any(|msg| {
                msg.role == alan_llm::MessageRole::Context
                    && msg.content.contains("## Runtime Recall Bundle")
            })
        })
        .expect("expected trailing context message with runtime recall bundle");
    assert!(
        !request
            .system_prompt
            .as_deref()
            .unwrap_or_default()
            .contains("## Runtime Recall Bundle")
    );
    let context = request
        .messages
        .iter()
        .rev()
        .find(|msg| msg.role == alan_llm::MessageRole::Context)
        .unwrap();
    assert!(context.content.contains("## Runtime Recall Bundle"));
    assert!(context.content.contains("/memory/daily/2026-04-16.md"));
    assert!(
        context
            .content
            .contains("/memory/episodic/2026/04/16/process-4.md")
    );
    assert!(context.content.contains("ALAN_RECENT_RECALL_4"));
    assert!(!context.content.contains("/memory/topics/recent-match-4.md"));
}

#[tokio::test]
async fn test_run_turn_pre_turn_compaction_accounts_for_runtime_recall_budget() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let memory_dir = definition_root.join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    std::fs::write(
        memory_dir.join("USER.md"),
        format!(
            "# User Memory\n- Favorite runtime marker: {}\n",
            "ALAN_PRETURN_RECALL ".repeat(80)
        ),
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(
        RecordingToolCallProvider::new(
            Vec::new(),
            "COMPACTED_FOR_RECALL",
            seen_system_prompts.clone(),
        )
        .with_seen_requests(seen_requests.clone()),
    );
    state.core_config.memory.store_dir = Some(memory_dir.clone());
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
    state.runtime_config.compaction_keep_last = 2;
    state.runtime_config.compaction_trigger_messages = usize::MAX;
    state.runtime_config.compaction_soft_trigger_ratio = 1.0;
    state.runtime_config.compaction_hard_trigger_ratio = 1.0;
    for idx in 0..3 {
        state
            .machine
            .add_user_message(&format!("Earlier user context {idx} {}", "u".repeat(220)));
        state.machine.add_assistant_message(
            &format!("Earlier assistant context {idx} {}", "a".repeat(220)),
            None,
        );
    }

    let user_input = vec![ContentPart::text("What is my favorite runtime marker?")];
    let turn_recall_bundle = crate::runtime::memory_recall::build_turn_recall_bundle(
        Some(memory_dir.as_path()),
        Some(&user_input),
    );
    let pending_prompt_tokens = estimate_pending_turn_prompt_tokens(
        Some(&user_input),
        turn_recall_bundle.as_deref(),
        None,
        None,
    );
    assert!(pending_prompt_tokens > 0);

    let base_prompt_tokens = state.machine.estimated_prompt_tokens();
    state.runtime_config.context_window_tokens =
        (base_prompt_tokens + pending_prompt_tokens - 1) as u32;

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(user_input),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(state.machine.tape_summary(), Some("COMPACTED_FOR_RECALL"));

    let requests = seen_requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let request = requests
        .iter()
        .find(|req| {
            req.messages.iter().any(|msg| {
                msg.role == alan_llm::MessageRole::Context
                    && msg.content.contains("## Runtime Recall Bundle")
            })
        })
        .expect("expected trailing context message with runtime recall bundle");
    assert!(
        !request
            .system_prompt
            .as_deref()
            .unwrap_or_default()
            .contains("## Runtime Recall Bundle")
    );
    let context = request
        .messages
        .iter()
        .rev()
        .find(|msg| msg.role == alan_llm::MessageRole::Context)
        .unwrap();
    assert!(context.content.contains("ALAN_PRETURN_RECALL"));
    assert_eq!(state.machine.drain_deferred_runtime_actions().len(), 1);
}

#[tokio::test]
async fn test_run_turn_omits_runtime_recall_bundle_when_memory_disabled() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let memory_dir = definition_root.join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    std::fs::write(
        memory_dir.join("USER.md"),
        "# User Memory\n- Favorite runtime marker: ALAN_DISABLED_RECALL\n",
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
        Vec::new(),
        "ok",
        seen_system_prompts.clone(),
    ));
    state.core_config.memory.store_dir = Some(memory_dir);
    state.core_config.memory.enabled = false;
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text(
            "What is my favorite runtime marker?",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let system_prompts = seen_system_prompts.lock().unwrap();
    let request_prompt = system_prompts.last().expect("expected system prompt");
    assert!(!request_prompt.contains("## Runtime Recall Bundle"));
    assert!(!request_prompt.contains("ALAN_DISABLED_RECALL"));
}

#[tokio::test]
async fn test_run_turn_keeps_system_prompt_static_and_deduplicates_bootstrapped_memory_across_turns()
 {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let memory_dir = definition_root.join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    std::fs::write(
        memory_dir.join("USER.md"),
        "# User Memory\n- Bootstrapped identity key: BOOTSTRAP_IDENTITY_99\n",
    )
    .unwrap();
    std::fs::write(
        memory_dir.join("topics/topic-alpha.md"),
        "# Topic Alpha\n- Alpha architecture detail: ALPHA_ONLY_44\n",
    )
    .unwrap();
    std::fs::write(
        memory_dir.join("topics/topic-beta.md"),
        "# Topic Beta\n- Beta protocol detail: BETA_ONLY_55\n",
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(
        RecordingToolCallProvider::new(Vec::new(), "acknowledged", seen_system_prompts.clone())
            .with_seen_requests(seen_requests.clone()),
    );
    state.core_config.memory.store_dir = Some(memory_dir.clone());
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
    state.prompt_cache.set_memory_store_dir(Some(memory_dir));

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text(
            "What is my identity key and ALPHA_ONLY_44?",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();
    run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Tell me about BETA_ONLY_55")]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    let requests = seen_requests.lock().unwrap();
    // Each turn executes 1 generation + optional deferred calls; filter main turns that have user messages.
    let turn_requests: Vec<_> = requests
        .iter()
        .filter(|req| !req.messages.is_empty())
        .collect();
    assert_eq!(turn_requests.len(), 2);
    assert_eq!(
        turn_requests[0].system_prompt, turn_requests[1].system_prompt,
        "system_prompt prefix must stay byte-identical across turns when recall changes"
    );
    let sys = turn_requests[0].system_prompt.as_deref().unwrap();
    assert!(sys.contains("BOOTSTRAP_IDENTITY_99"));
    assert!(!sys.contains("## Runtime Recall Bundle"));

    let ctx0 = turn_requests[0]
        .messages
        .iter()
        .rev()
        .find(|msg| msg.role == alan_llm::MessageRole::Context)
        .expect("turn 1 trailing recall context");
    assert!(ctx0.content.contains("ALPHA_ONLY_44"));
    assert!(
        !ctx0.content.contains("/memory/USER.md"),
        "already-bootstrapped USER.md must not be duplicated into Runtime Recall Bundle"
    );

    let ctx1 = turn_requests[1]
        .messages
        .iter()
        .rev()
        .find(|msg| msg.role == alan_llm::MessageRole::Context)
        .expect("turn 2 trailing recall context");
    assert!(ctx1.content.contains("BETA_ONLY_55"));
}

#[tokio::test]
async fn test_maybe_compact_mid_turn_accounts_for_runtime_prompt_overhead() {
    let mut state =
        create_test_state_with_provider(ContentMockProvider::new("MID_TURN_COMPACTION_SUMMARY"));
    state.runtime_config.compaction_keep_last = 2;
    state.runtime_config.compaction_trigger_messages = usize::MAX;
    state.runtime_config.compaction_soft_trigger_ratio = 1.0;
    state.runtime_config.compaction_hard_trigger_ratio = 1.0;
    for idx in 0..3 {
        state
            .machine
            .add_user_message(&format!("Mid-turn user context {idx} {}", "u".repeat(220)));
        state.machine.add_assistant_message(
            &format!("Mid-turn assistant context {idx} {}", "a".repeat(220)),
            None,
        );
    }

    let pending_guardrail_instruction = format!(
        "Retry with a corrected answer and preserve tool intent.\n{}",
        "guardrail-overhead ".repeat(80)
    );
    let additional_prompt_tokens = estimate_request_prompt_overhead_tokens(
        None,
        Some(pending_guardrail_instruction.as_str()),
        None,
        None,
    );
    assert!(additional_prompt_tokens > 0);

    let base_prompt_tokens = state.machine.estimated_prompt_tokens();
    state.runtime_config.context_window_tokens =
        (base_prompt_tokens + additional_prompt_tokens - 1) as u32;

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result =
        maybe_compact_mid_turn_if_needed(&mut state, &mut emit, &cancel, additional_prompt_tokens)
            .await;

    assert!(result.is_ok());
    assert_eq!(
        state.machine.tape_summary(),
        Some("MID_TURN_COMPACTION_SUMMARY")
    );
    assert_eq!(state.machine.compactions_this_turn(), 1);
}
