use super::*;

#[tokio::test]
async fn test_run_turn_llm_error() {
    // Use error provider
    struct ErrorMockProvider;

    #[async_trait]
    impl LlmProvider for ErrorMockProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Err(anyhow::anyhow!("LLM error"))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Err(anyhow::anyhow!("LLM error"))
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            Err(anyhow::anyhow!("LLM error"))
        }

        fn provider_name(&self) -> &'static str {
            "error_mock"
        }
    }

    let mut state = create_test_state_with_provider(ErrorMockProvider);
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

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("LLM request failed")
    );

    // Should have error event
    let has_error = events.iter().any(
        |e| matches!(e, Event::Error { message, .. } if message.contains("LLM request failed")),
    );
    assert!(has_error, "Expected Error event for LLM failure");

    let mut state = create_test_state_with_provider(ErrorMockProvider);
    let input = alan_agent_protocol::Submission::new(alan_agent_protocol::Op::Turn {
        parts: vec![ContentPart::text("failed input")],
        context: None,
    });
    let id = input.id.clone();
    let outcome = crate::runtime::transition::advance_accepted_submission(
        &mut state,
        input,
        &crate::runtime::turn_input::TurnInputBroker::default(),
        &cancel,
    )
    .await;
    assert!(outcome.result.is_err());
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    let ui = shell
        .cat(&format!(
            "{}/machine/ui/events",
            state.environment.agent_path()
        ))
        .await
        .unwrap();
    assert!(std::str::from_utf8(&ui).unwrap().lines().any(|line| {
        matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
            alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Failed, error: Some(error) }
            if submission_ids == [id.clone()] && error.contains("LLM request failed"))
    }));
    // The Process loop owns terminal UI publication; one failed generation must
    // not produce both a transition error cell and another Process error cell.
    let state = create_test_state_with_provider(ErrorMockProvider);
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    let capabilities = crate::provider_capabilities_for_config(&state.core_config);
    let config = crate::runtime::AgentProcessConfig {
        agent_config: crate::AgentConfig::from(state.core_config),
        ..Default::default()
    };
    let mut controller = crate::runtime::spawn_with_namespace_environment(
        config,
        state.environment,
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    controller.wait_until_ready().await.unwrap();
    let input = alan_agent_protocol::Submission::new(alan_agent_protocol::Op::Turn {
        parts: vec![ContentPart::text("provider failure")],
        context: None,
    });
    let id = input.id.clone();
    controller.handle.submission_tx.send(input).await.unwrap();
    let mut stream = shell.tail("/agent/1/machine/ui/events").await.unwrap();
    let events = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut pending = Vec::new();
        let mut events = Vec::new();
        loop {
            pending.extend(stream.read(4096).await.unwrap());
            while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<_> = pending.drain(..=end).collect();
                events.push(serde_json::from_slice::<alan_agent_protocol::UiEvent>(&line).unwrap());
            }
            if events
                .iter()
                .any(|event| matches!(event, alan_agent_protocol::UiEvent::Error { .. }))
                && matches!(events.last(), Some(alan_agent_protocol::UiEvent::Activity { snapshot })
                    if snapshot.state == alan_agent_protocol::UiActivityState::Idle)
            {
                break events;
            }
        }
    })
    .await
    .unwrap();
    stream.close().await.unwrap();
    controller.shutdown().await.unwrap();
    assert_eq!(events.iter().filter(|event| matches!(event,
        alan_agent_protocol::UiEvent::Error { message, .. } if message.contains("LLM request failed")
    )).count(), 1);
    assert_eq!(events.iter().filter(|event| matches!(event,
        alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Failed, .. }
        if submission_ids == std::slice::from_ref(&id)
    )).count(), 1);
}

#[tokio::test]
async fn test_streaming_mode_uses_request_response_generation() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let mut state = create_test_state_with_provider(PanicOnStreamProvider {
        content: "final response through request response".to_string(),
        generate_calls: Arc::clone(&generate_calls),
    });
    state.runtime_config.streaming_mode = crate::config::StreamingMode::On;

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Test streaming config")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(generate_calls.load(Ordering::SeqCst), 1);
    let emitted_text = events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(emitted_text, "final response through request response");
}

#[tokio::test]
async fn test_namespace_live_turn_generation_retries_transient_stream_failure() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let mut state = create_test_state_with_provider(TransientStreamFailureProvider {
        generate_calls: Arc::clone(&generate_calls),
    });
    state.runtime_config.llm_request_timeout_secs = 5;

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Test transient stream retry")]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
    assert_eq!(generate_calls.load(Ordering::SeqCst), 2);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Error { .. })),
        "transient stream failure should retry without surfacing an error: {events:?}"
    );
    let emitted_text = events
        .iter()
        .filter_map(|event| match event {
            Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(emitted_text, "Recovered after retry.");
}

#[tokio::test]
async fn cancellation_during_output_prevents_successful_settlement() {
    for content in ["answer before cancellation", ""] {
        let mut state = create_test_state_with_provider(ContentMockProvider::new(content));
        state.core_config.memory.enabled = false;
        state.machine.accept_submission("cancel-during-output");
        state.machine.set_turn_activity(TurnActivityState::Running);
        let cancel = CancellationToken::new();
        let mut completions = Vec::new();
        let mut emit = |event: Event| {
            match event {
                Event::TextDelta { is_final: true, .. } => cancel.cancel(),
                Event::TurnCompleted { summary } => completions.push(summary),
                _ => {}
            }
            async {}
        };
        run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("answer this")]),
            &mut emit,
            &cancel,
            None,
        )
        .await
        .unwrap();
        assert!(state.machine.submission_was_cancelled());
        assert_eq!(completions, [Some("Task cancelled by user".to_string())]);
    }
}

#[tokio::test]
async fn oversized_generation_turn_surfaces_payload_too_large_with_compact_hint() {
    let generate_calls = Arc::new(AtomicUsize::new(0));
    struct CountingProvider {
        calls: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl LlmProvider for CountingProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(GenerationResponse {
                content: "unexpected".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(String::new())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            tx.send(StreamChunk {
                text: Some("ok".to_string()),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: None,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
            })
            .await
            .unwrap();
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "counting_mock"
        }
    }

    let mut state = create_test_state_with_provider(CountingProvider {
        calls: Arc::clone(&generate_calls),
    });
    state.core_config.memory.enabled = false;
    state.runtime_config.compaction_soft_trigger_ratio = 1000.0;
    state.runtime_config.compaction_hard_trigger_ratio = 1000.0;
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    // Exceed MAX_GENERATION_DOC_BYTES (16 MiB).
    let oversized_input = "a".repeat((16 << 20) + 1024);
    let err = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text(oversized_input)]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("stream_error:payload_too_large"), "{msg}");
    assert!(msg.contains("/compact"), "{msg}");
    assert!(!msg.contains("stream_error:unknown"), "{msg}");
    assert_eq!(
        generate_calls.load(Ordering::SeqCst),
        0,
        "oversized payload must fail pre-flight without retrying or dispatching to provider"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Error { message, .. }
            if message.contains("stream_error:payload_too_large") && message.contains("/compact")
    )));
    assert!(
        !state
            .machine
            .messages()
            .iter()
            .any(|m| matches!(m, crate::tape::Message::Assistant { .. })),
        "failed generation start must not persist any assistant or tool-call record"
    );
}

#[tokio::test]
async fn stream_error_with_partial_tool_call_leaves_no_dangling_records_for_next_turn() {
    struct MalformedThenCleanProvider {
        calls: Arc<AtomicUsize>,
        seen_requests: Arc<std::sync::Mutex<Vec<GenerationRequest>>>,
    }

    #[async_trait]
    impl LlmProvider for MalformedThenCleanProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            unreachable!("namespace generation uses generate_stream")
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(String::new())
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let attempt = self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen_requests.lock().unwrap().push(request);
            let (tx, rx) = tokio::sync::mpsc::channel(8);
            if attempt == 0 {
                // Emit enough text to pass any guardrail hold plus a partial tool-call delta,
                // then fail with Vertex's MALFORMED_FUNCTION_CALL finish reason.
                tx.send(StreamChunk {
                    text: Some("partial output before malformed tool call ".repeat(3)),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: false,
                    finish_reason: None,
                    provider_response_id: None,
                    provider_response_status: None,
                })
                .await
                .unwrap();
                tx.send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    sequence_number: None,
                    tool_call_delta: Some(alan_llm::ToolCallDelta {
                        index: 0,
                        id: Some("call_corrupted".to_string()),
                        name: Some("read_file".to_string()),
                        arguments_delta: Some("{\"path\":\"trun".to_string()),
                        arguments: None,
                    }),
                    is_finished: false,
                    finish_reason: None,
                    provider_response_id: None,
                    provider_response_status: None,
                })
                .await
                .unwrap();
                tx.send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("MALFORMED_FUNCTION_CALL".to_string()),
                    provider_response_id: None,
                    provider_response_status: None,
                })
                .await
                .unwrap();
            } else {
                tx.send(StreamChunk {
                    text: Some("Clean recovery on next turn.".to_string()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("STOP".to_string()),
                    provider_response_id: None,
                    provider_response_status: None,
                })
                .await
                .unwrap();
            }
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "vertex_mock"
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let seen_requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(MalformedThenCleanProvider {
        calls: Arc::clone(&calls),
        seen_requests: Arc::clone(&seen_requests),
    });
    state.core_config.memory.enabled = false;
    let cancel = CancellationToken::new();

    // Turn 1 fails mid-stream with MALFORMED_FUNCTION_CALL after partial text & tool delta.
    let mut turn1_events = Vec::new();
    let mut emit1 = |event: Event| {
        turn1_events.push(event);
        async {}
    };
    let err = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("first turn")]),
        &mut emit1,
        &cancel,
        None,
    )
    .await
    .unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("stream_error:malformed_function_call"),
        "expected explicit Vertex finish reason, got: {err_msg}"
    );
    assert!(!err_msg.contains("stream_error:unknown"), "{err_msg}");

    // Machine tape must contain no assistant or tool-call message after stream failure.
    assert!(
        !state.machine.messages().iter().any(|m| matches!(
            m,
            crate::tape::Message::Assistant { .. } | crate::tape::Message::Tool { .. }
        )),
        "failed stream must not leave dangling assistant/tool records: {:?}",
        state.machine.messages()
    );
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    let tape_after_fail = shell
        .cat(&format!("{}/machine/tape", state.environment.agent_path()))
        .await
        .unwrap();
    let tape_str = std::str::from_utf8(&tape_after_fail).unwrap();
    assert!(
        !tape_str.contains("\"role\":\"assistant\""),
        "machine/tape must not record a failed assistant turn: {tape_str}"
    );

    // Turn 2 recovers cleanly and sends no corrupted assistant/tool-call history.
    let mut turn2_events = Vec::new();
    let mut emit2 = |event: Event| {
        turn2_events.push(event);
        async {}
    };
    let outcome = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("second turn")]),
        &mut emit2,
        &cancel,
        None,
    )
    .await
    .unwrap();
    assert!(matches!(outcome, TurnExecutionOutcome::Finished));

    {
        let requests = seen_requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1]
                .messages
                .iter()
                .all(|m| m.role != alan_llm::MessageRole::Assistant && m.tool_calls.is_none()),
            "next-turn request must not include dangling assistant or tool_calls: {:?}",
            requests[1].messages
        );
    }
    let tape_after_turn2 = shell
        .cat(&format!("{}/machine/tape", state.environment.agent_path()))
        .await
        .unwrap();
    let tape_str2 = std::str::from_utf8(&tape_after_turn2).unwrap();
    assert!(
        tape_str2.contains("Clean recovery on next turn."),
        "next-turn assistant output must be recorded cleanly: {tape_str2}"
    );
}
