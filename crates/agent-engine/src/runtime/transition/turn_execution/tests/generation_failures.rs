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

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));

    // Should have error event
    let has_error = events.iter().any(
        |e| matches!(e, Event::Error { message, .. } if message.contains("LLM request failed")),
    );
    assert!(has_error, "Expected Error event for LLM failure");
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
