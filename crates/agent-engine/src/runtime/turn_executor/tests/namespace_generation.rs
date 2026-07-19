use super::*;

#[tokio::test]
async fn test_run_turn_with_content_response() {
    let mut state = create_test_state_with_provider(ContentMockProvider::new("Hello, world!"));
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

    // Check events
    let has_turn_started = events.iter().any(|e| matches!(e, Event::TurnStarted {}));
    let has_turn_completed = events.iter().any(|e| {
        matches!(
            e,
            Event::TurnCompleted {
                summary: Some(_),
                ..
            }
        )
    });

    assert!(has_turn_started, "Expected TurnStarted event");
    assert!(has_turn_completed, "Expected TurnCompleted event");
}

#[tokio::test]
async fn test_namespace_turn_reads_agent_input_generates_via_llmfs_and_writes_agent_output() {
    let procfs = Arc::new(alan_kernel::ProcFs::new());
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let recorded_requests = Arc::new(Mutex::new(Vec::new()));
    llmfs.register_connection(
        "default",
        Box::new(NamedRecordingStreamProvider {
            provider_name: "openai_responses",
            chunks: vec![
                "hello ".to_string(),
                "from ".to_string(),
                "namespace turn loop".to_string(),
            ],
            requests: Arc::clone(&recorded_requests),
        }),
    );

    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/proc",
        alan_ap::InProcessTransport::new(procfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());

    let pid = shell
        .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    assert_eq!(pid, "1");
    shell
        .write("/agent/1/io/input", b"hello agent")
        .await
        .unwrap();
    let mut output_tail = shell.tail("/agent/1/io/output").await.unwrap();

    let mut state = create_test_state_with_provider(PanicIfGeneratedProvider);
    state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        None,
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    assert!(matches!(result, TurnExecutionOutcome::Finished));
    let streamed = output_tail.read(64 * 1024).await.unwrap();
    output_tail.close().await.unwrap();
    assert_eq!(
        String::from_utf8(streamed).unwrap(),
        "hello from namespace turn loop"
    );

    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    assert!(tape.contains(r#""role":"user""#), "{tape}");
    assert!(tape.contains(r#""content":"hello agent""#), "{tape}");
    assert!(tape.contains(r#""role":"assistant""#), "{tape}");
    assert!(
        tape.contains(r#""content":"hello from namespace turn loop""#),
        "{tape}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::TurnCompleted {
                summary: Some(_),
                ..
            }
        )),
        "namespace turn should still publish legacy completion during migration"
    );
    let text_events: Vec<(String, bool)> = events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { chunk, is_final } => Some((chunk.clone(), *is_final)),
            _ => None,
        })
        .collect();
    assert_eq!(
        text_events,
        vec![
            ("hello ".to_string(), false),
            ("from ".to_string(), false),
            ("namespace turn loop".to_string(), false),
            (String::new(), true),
        ],
        "namespace turn should forward llmfs token events without re-chunking"
    );
    let first_text_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::TextDelta {
                    chunk,
                    is_final: false
                } if chunk == "hello "
            )
        })
        .unwrap();
    let completed_index = events
        .iter()
        .position(|event| matches!(event, Event::TurnCompleted { .. }))
        .unwrap();
    assert!(
        first_text_index < completed_index,
        "namespace text deltas should be emitted before turn completion"
    );

    let requests = recorded_requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].extra_params.is_empty(),
        "namespace generation must write a neutral llmfs request, not provider-local projection params: {:?}",
        requests[0].extra_params.keys().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_namespace_tape_writes_user_once_across_tool_loop_assistant_outputs() {
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let generate_calls = Arc::new(AtomicUsize::new(0));
    llmfs.register_connection(
        "default",
        Box::new(SequenceMockProvider::new(
            vec![
                GenerationResponse {
                    content: "I will update the plan.".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![ToolCall {
                        id: Some("call_plan".to_string()),
                        name: "update_plan".to_string(),
                        arguments: json!({
                            "explanation": "Testing",
                            "items": [{"id": "1", "content": "Step 1", "status": "completed"}]
                        }),
                    }],
                    usage: None,
                    finish_reason: Some("tool_calls".to_string()),
                    provider_response_id: None,
                    provider_response_status: None,
                    warnings: Vec::new(),
                },
                GenerationResponse {
                    content: "Done.".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: Vec::new(),
                    usage: None,
                    finish_reason: Some("stop".to_string()),
                    provider_response_id: None,
                    provider_response_status: None,
                    warnings: Vec::new(),
                },
            ],
            Arc::clone(&generate_calls),
        )),
    );

    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());

    let mut state = create_test_state_with_provider(PanicIfGeneratedProvider);
    state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Original user request")]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    assert!(matches!(result, TurnExecutionOutcome::Finished));
    assert_eq!(generate_calls.load(Ordering::SeqCst), 2);
    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    let records = tape
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let user_records = records
        .iter()
        .filter(|record| record["role"] == "user")
        .collect::<Vec<_>>();
    let assistant_contents = records
        .iter()
        .filter(|record| record["role"] == "assistant")
        .map(|record| record["content"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    assert_eq!(user_records.len(), 1, "tape was {tape}");
    assert_eq!(user_records[0]["content"], "Original user request");
    assert_eq!(
        assistant_contents,
        vec!["I will update the plan.".to_string(), "Done.".to_string()]
    );
}

#[tokio::test]
async fn test_namespace_turn_without_mounted_model_does_not_fallback_to_provider() {
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    shell
        .write("/agent/1/io/input", b"hello with no mounted model")
        .await
        .unwrap();

    let mut state = create_test_state_with_provider(PanicIfGeneratedProvider);
    state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "missing");

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        None,
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    assert!(matches!(result, TurnExecutionOutcome::Finished));
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Error {
                message,
                recoverable: true,
            } if message.contains("Namespace LLM request failed")
        )),
        "missing llm mount should surface a namespace error: {events:?}"
    );
    let output = String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap();
    assert!(output.is_empty(), "missing model must not produce output");
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY, ALAN_M2_LIVE_MODEL, and network access"]
async fn test_namespace_turn_live_openai_responses_ignored() {
    let api_key = std::env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY is required for the ignored live M2 test");
    let model = std::env::var("ALAN_M2_LIVE_MODEL")
        .expect("ALAN_M2_LIVE_MODEL is required for the ignored live M2 test");

    let procfs = Arc::new(alan_kernel::ProcFs::new());
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let provider = alan_llm::factory::create_provider(
        alan_llm::factory::ProviderConfig::openai_responses(api_key, model),
    )
    .expect("create live OpenAI Responses provider");
    llmfs.register_connection("live", provider);

    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/proc",
        alan_ap::InProcessTransport::new(procfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());

    let pid = shell
        .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    assert_eq!(pid, "1");
    shell
        .write(
            "/agent/1/io/input",
            b"Reply with exactly this text and nothing else: alan-m2-live-ok",
        )
        .await
        .unwrap();
    let mut output_tail = shell.tail("/agent/1/io/output").await.unwrap();

    let mut state = create_test_state_with_provider(PanicIfGeneratedProvider);
    state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "live");

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        None,
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    assert!(matches!(result, TurnExecutionOutcome::Finished));
    let streamed = String::from_utf8(output_tail.read(64 * 1024).await.unwrap()).unwrap();
    output_tail.close().await.unwrap();
    assert!(
        streamed.contains("alan-m2-live-ok"),
        "unexpected live response: {streamed}"
    );
}

#[tokio::test]
async fn test_namespace_turn_omits_provider_local_responses_continuation_fields() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = CapturingResponsesProvider {
        requests: Arc::clone(&requests),
        response: GenerationResponse {
            content: "Follow-up answer".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: Some("resp_next".to_string()),
            provider_response_status: Some("completed".to_string()),
        },
        provider_name: "openai_responses",
    };
    let mut state = create_test_state_with_provider(provider);
    state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
    state.runtime_config.context_window_tokens = 1000;
    state.runtime_config.compaction_soft_trigger_ratio = 0.5;
    state.machine.add_user_message("Earlier input");
    state.machine.add_assistant_message("Earlier output", None);
    let boundary_message_count = state.machine.messages().len();
    let reference_context_revision = state.machine.context_revision();
    state.machine.mark_responses_continuation(
        "openai_responses",
        "resp_prev",
        boundary_message_count,
        reference_context_revision,
    );
    let cancel = CancellationToken::new();

    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("New input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    let requests = requests.lock().unwrap();
    let request = requests.last().expect("captured request");
    assert!(!request.extra_params.contains_key("previous_response_id"));
    assert!(!request.extra_params.contains_key("store"));
    assert!(!request.extra_params.contains_key("context_management"));
    assert!(!request.extra_params.contains_key("responses_input_items"));
    let message_texts: Vec<_> = request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(
        message_texts,
        vec!["Earlier input", "Earlier output", "New input"]
    );
    drop(requests);

    assert!(
        state.machine.responses_continuation().is_none(),
        "namespace generation must not maintain provider-managed continuation state"
    );
}

#[tokio::test]
async fn test_run_turn_populates_generation_request_reasoning_effort() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = CapturingResponsesProvider {
        requests: Arc::clone(&requests),
        response: GenerationResponse {
            content: "answer".to_string(),
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
        provider_name: "openai_responses",
    };
    let mut state = create_test_state_with_provider(provider);
    state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
    state.runtime_config.request_control_intent = crate::RequestControlIntent::reasoning_effort(
        Some(alan_agent_protocol::ReasoningEffort::High),
    );
    let cancel = CancellationToken::new();

    let mut emit = |_event: Event| async {};
    run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    let requests = requests.lock().unwrap();
    let request = requests.last().expect("captured request");
    assert_eq!(
        request.reasoning.effort,
        Some(alan_agent_protocol::ReasoningEffort::High)
    );
}

#[tokio::test]
async fn test_run_turn_uses_turn_reasoning_effort_before_runtime_effort() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = CapturingResponsesProvider {
        requests: Arc::clone(&requests),
        response: GenerationResponse {
            content: "answer".to_string(),
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
        provider_name: "openai_responses",
    };
    let mut state = create_test_state_with_provider(provider);
    let temp_dir = TempDir::new().unwrap();
    state.machine =
        AgentMachine::new_with_recorder_in_dir("/proc/test", "gpt-5.4", temp_dir.path())
            .await
            .unwrap();
    state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
    state.runtime_config.request_control_intent = crate::RequestControlIntent::reasoning_effort(
        Some(alan_agent_protocol::ReasoningEffort::High),
    );
    state.turn_state.set_active_turn_request_control_intent(
        crate::RequestControlIntent::reasoning_effort(Some(
            alan_agent_protocol::ReasoningEffort::Low,
        )),
    );
    let cancel = CancellationToken::new();

    let mut emit = |_event: Event| async {};
    run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    {
        let requests = requests.lock().unwrap();
        let request = requests.last().expect("captured request");
        assert_eq!(
            request.reasoning.effort,
            Some(alan_agent_protocol::ReasoningEffort::Low)
        );
    }

    state.machine.flush().await;
    let rollout_path = state.machine.rollout_path().expect("rollout path");
    let persisted_effort = RolloutRecorder::load_history(rollout_path)
        .await
        .unwrap()
        .into_iter()
        .find_map(|item| match item {
            RolloutItem::TurnContext(ctx) => ctx.reasoning_effort,
            _ => None,
        });
    assert_eq!(
        persisted_effort,
        Some(alan_agent_protocol::ReasoningEffort::Low)
    );
}

#[tokio::test]
async fn test_run_turn_uses_runtime_reasoning_effort_without_budget_fallback() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = CapturingResponsesProvider {
        requests: Arc::clone(&requests),
        response: GenerationResponse {
            content: "answer".to_string(),
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
        provider_name: "openai_responses",
    };
    let mut state = create_test_state_with_provider(provider);
    state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
    state.runtime_config.request_control_intent = crate::RequestControlIntent::reasoning_effort(
        Some(alan_agent_protocol::ReasoningEffort::High),
    );
    let cancel = CancellationToken::new();

    let mut emit = |_event: Event| async {};
    run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    let requests = requests.lock().unwrap();
    let request = requests.last().expect("captured request");
    assert_eq!(
        request.reasoning.effort,
        Some(alan_agent_protocol::ReasoningEffort::High)
    );
}

#[tokio::test]
async fn test_run_turn_omits_reasoning_controls_when_unset() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = CapturingResponsesProvider {
        requests: Arc::clone(&requests),
        response: GenerationResponse {
            content: "answer".to_string(),
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
        provider_name: "openai_responses",
    };
    let mut state = create_test_state_with_provider(provider);
    state.core_config.llm_provider = crate::config::LlmProvider::OpenRouter;
    state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
    let cancel = CancellationToken::new();

    let mut emit = |_event: Event| async {};
    run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    let requests = requests.lock().unwrap();
    let request = requests.last().expect("captured request");
    assert_eq!(request.reasoning.effort, None);
}

#[tokio::test]
async fn test_namespace_turn_sends_reference_context_as_neutral_messages() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = CapturingResponsesProvider {
        requests: Arc::clone(&requests),
        response: GenerationResponse {
            content: "Fresh answer".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: Some("resp_fresh".to_string()),
            provider_response_status: Some("completed".to_string()),
        },
        provider_name: "openai_responses",
    };
    let mut state = create_test_state_with_provider(provider);
    state.machine.add_user_message("Earlier input");
    state.machine.add_assistant_message("Earlier output", None);
    let boundary_message_count = state.machine.messages().len();
    let reference_context_revision = state.machine.context_revision();
    state.machine.mark_responses_continuation(
        "openai_responses",
        "resp_prev",
        boundary_message_count,
        reference_context_revision,
    );
    state
        .machine
        .apply_context_items_for_test(vec![crate::tape::ContextItem::new(
            "ctx_1",
            "domain_note",
            "Domain note",
            "Reference context changed",
        )]);
    let cancel = CancellationToken::new();

    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("New input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    let requests = requests.lock().unwrap();
    let request = requests.last().expect("captured request");
    assert!(!request.extra_params.contains_key("previous_response_id"));
    assert!(!request.extra_params.contains_key("responses_input_items"));
    assert!(
        request
            .messages
            .iter()
            .any(|message| message.content == "New input")
    );
    assert!(
        request
            .messages
            .iter()
            .any(|message| message.content.contains("Reference context changed")),
        "reference context should stay in the neutral llmfs message list: {:?}",
        request.messages
    );
}

#[tokio::test]
async fn test_namespace_turn_does_not_use_provider_managed_compaction() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = CapturingResponsesProvider {
        requests: Arc::clone(&requests),
        response: GenerationResponse {
            content: "Follow-up answer".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: Some("resp_next".to_string()),
            provider_response_status: Some("completed".to_string()),
        },
        provider_name: "openai_responses",
    };
    let mut state = create_test_state_with_provider(provider);
    state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
    state.runtime_config.compaction_trigger_messages = 0;
    state.runtime_config.context_window_tokens = 1;
    state.runtime_config.compaction_soft_trigger_ratio = 0.0;
    state.runtime_config.compaction_hard_trigger_ratio = 0.0;
    state.machine.add_user_message("Earlier input");
    state.machine.add_assistant_message("Earlier output", None);
    let boundary_message_count = state.machine.messages().len();
    let reference_context_revision = state.machine.context_revision();
    state.machine.mark_responses_continuation(
        "openai_responses",
        "resp_prev",
        boundary_message_count,
        reference_context_revision,
    );
    let cancel = CancellationToken::new();

    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("New input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "provider-managed continuation must not add an extra namespace request"
    );
    assert!(
        !requests[0]
            .extra_params
            .contains_key("previous_response_id")
    );
    assert!(!requests[0].extra_params.contains_key("context_management"));
}

#[tokio::test]
async fn compaction_timeout_aborts_namespace_generation_before_continuing() {
    let started = Arc::new(tokio::sync::Notify::new());
    let (mut state, shell) = create_test_state_with_provider_and_tools_and_shell(
        BlockingStreamProvider {
            started: Arc::clone(&started),
        },
        ToolRegistry::new(),
    );
    state.runtime_config.compaction_trigger_messages = 0;
    state.runtime_config.context_window_tokens = 1;
    state.runtime_config.compaction_soft_trigger_ratio = 0.0;
    state.runtime_config.compaction_hard_trigger_ratio = 0.0;
    state.runtime_config.compaction_keep_last = 1;
    state.machine.add_user_message("Earlier input");
    let earlier_output = "Earlier output".repeat(20);
    state.machine.add_assistant_message(&earlier_output, None);

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let request = CompactionRequest::automatic_pre_turn();
    let result = maybe_compact_context_with_turn_timeout(
        &mut state,
        &mut emit,
        &request,
        &cancel,
        std::time::Duration::from_millis(250),
    )
    .await;

    assert!(result.timed_out);
    assert!(matches!(result.result, Ok(CompactionOutcome::Skipped(_))));
    let status = String::from_utf8(
        shell
            .cat("/mnt/llm/connections/default/g0/status")
            .await
            .unwrap(),
    )
    .unwrap();
    let status: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert_eq!(status["status"], "aborted");
}

#[tokio::test]
async fn test_namespace_chatgpt_request_omits_provider_local_projection_fields() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = CapturingResponsesProvider {
        requests: Arc::clone(&requests),
        response: GenerationResponse {
            content: "Follow-up answer".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: Some("resp_next".to_string()),
            provider_response_status: Some("completed".to_string()),
        },
        provider_name: "chatgpt",
    };
    let mut state = create_test_state_with_provider(provider);
    state.runtime_config.streaming_mode = crate::config::StreamingMode::Off;
    state.runtime_config.context_window_tokens = 1000;
    state.runtime_config.compaction_soft_trigger_ratio = 0.5;
    state.machine.add_user_message("Earlier input");
    state.machine.add_assistant_message("Earlier output", None);
    let boundary_message_count = state.machine.messages().len();
    let reference_context_revision = state.machine.context_revision();
    state.machine.mark_responses_continuation(
        "chatgpt",
        "resp_prev",
        boundary_message_count,
        reference_context_revision,
    );
    let cancel = CancellationToken::new();

    let mut emit = |_event: Event| async {};
    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("New input")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let requests = requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "chatgpt should issue a single fresh request"
    );
    let request = requests.last().expect("captured request");
    assert!(!request.extra_params.contains_key("previous_response_id"));
    assert!(!request.extra_params.contains_key("store"));
    assert!(
        !request.extra_params.contains_key("context_management"),
        "chatgpt should not inherit openai_responses provider compaction payloads"
    );
    assert!(!request.extra_params.contains_key("responses_input_items"));
    let message_texts: Vec<_> = request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(
        message_texts,
        vec!["Earlier input", "Earlier output", "New input"]
    );
    assert!(state.machine.responses_continuation().is_none());
}
