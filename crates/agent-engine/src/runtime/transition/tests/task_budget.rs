use super::*;
use crate::agent_machine::task_budget::Phase;
use std::num::NonZeroU64;

struct SlowRetryableProvider {
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait::async_trait]
impl LlmProvider for SlowRetryableProvider {
    async fn generate(&mut self, _: GenerationRequest) -> Result<GenerationResponse> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
        anyhow::bail!("503 unavailable")
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.generate(request).await.map(finished_stream)
    }

    async fn chat(&mut self, _: Option<&str>, _: &str) -> Result<String> {
        anyhow::bail!("unused chat")
    }

    fn provider_name(&self) -> &'static str {
        "slow-retryable"
    }
}

#[tokio::test(start_paused = true)]
async fn budget_deadline_stops_nonstream_retry_backoff_before_another_model_attempt() {
    retry_budget_deadline(false).await;
}

#[tokio::test(start_paused = true)]
async fn budget_deadline_stops_stream_retry_backoff_before_another_model_attempt() {
    retry_budget_deadline(true).await;
}

async fn retry_budget_deadline(streaming: bool) {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut state = runtime_state_with_provider(SlowRetryableProvider {
        calls: calls.clone(),
    });
    state.core_config.memory.enabled = false;
    state.core_config.openai_responses_model = "budget-mock".into();
    state.core_config.task_budget.execution_seconds = NonZeroU64::new(1).unwrap();
    state.machine.accept_submission("retry-deadline");
    crate::runtime::task_budget::begin(&mut state)
        .await
        .unwrap();
    let started = tokio::time::Instant::now();
    let cancel = CancellationToken::new();
    let operation = async {
        if streaming {
            handle_submission_with_cancel(
                &mut state,
                Submission {
                    id: "retry-deadline".into(),
                    intent: alan_agent_protocol::InputIntent::ForceAgent,
                    op: Op::Turn {
                        parts: vec![alan_agent_protocol::ContentPart::text("hello")],
                        context: None,
                    },
                },
                &mut |_| async {},
                &cancel,
            )
            .await
            .unwrap();
            assert!(state.machine.task_budget_wait.is_some());
        } else {
            let error = state
                .namespace_generation()
                .generate_response_with_retry(
                    GenerationRequest::new().with_user_message("hello"),
                    0,
                    &cancel,
                )
                .await
                .unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<crate::agent_machine::task_budget::Exhausted>()
                    .unwrap()
                    .dimension,
                crate::agent_machine::task_budget::Dimension::Time,
            );
        }
    };
    let clock = async {
        for _ in 0..5000 {
            // Let namespace workers settle before advancing the test clock.
            for _ in 0..100 {
                tokio::task::yield_now().await;
            }
            tokio::time::advance(std::time::Duration::from_millis(1)).await;
        }
        panic!("retry deadline fixture did not settle");
    };
    tokio::select! {
        _ = operation => {},
        _ = clock => unreachable!(),
    }
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(
        started.elapsed() <= std::time::Duration::from_millis(1001),
        "streaming={streaming}: retry backoff exceeded the deadline: {:?}",
        started.elapsed(),
    );
    let spent = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!((spent.model_calls, spent.tool_calls), (1, 0));
}

#[tokio::test]
async fn budget_retry_exhaustion_pauses_real_submission_and_explicit_extension_resumes_it() {
    let provider = SequencedMockProvider::new(vec![
        SequencedStep::Error("503 unavailable".into()),
        SequencedStep::Success("finished after extension".into()),
    ]);
    let steps = provider.steps.clone();
    let mut state = runtime_state_with_provider(provider);
    state.core_config.memory.enabled = false;
    state.core_config.openai_responses_model = "budget-mock".into();
    state.core_config.task_budget.model_calls = NonZeroU64::new(1).unwrap();
    let submission = Submission {
        id: "budget-task".into(),
        intent: alan_agent_protocol::InputIntent::ForceAgent,
        op: Op::Turn {
            parts: vec![alan_agent_protocol::ContentPart::text("hello")],
            context: None,
        },
    };
    state.machine.accept_submission(submission.id.clone());
    handle_submission_with_cancel(
        &mut state,
        submission,
        &mut |_| async {},
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        steps.lock().unwrap().len(),
        1,
        "exhaustion must prevent the retry from reaching llmfs"
    );
    let id = state
        .machine
        .pending_request_ids()
        .into_iter()
        .next()
        .unwrap();
    let paused = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!(paused.model_calls, 1);
    assert_eq!(paused.phase, Phase::Paused);
    let resume = Submission {
        id: "budget-reply".into(),
        intent: alan_agent_protocol::InputIntent::ForceAgent,
        op: Op::Resume {
            request_id: id,
            content: vec![alan_agent_protocol::ContentPart::structured(
                json!({"choice":"approve"}),
            )],
        },
    };
    handle_submission_with_cancel(
        &mut state,
        resume,
        &mut |_| async {},
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(steps.lock().unwrap().is_empty());
    let finished = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!(
        (finished.model_calls, finished.limits.model_calls.get()),
        (2, 2)
    );
    assert!(finished.foreground_complete);
    assert!(!state.machine.has_pending_interaction());
    assert!(
        state
            .machine
            .messages()
            .iter()
            .any(|message| message.text_content().contains("finished after extension"))
    );
}

#[tokio::test]
async fn budget_limits_virtual_tool_batch_and_preserves_only_unexecuted_calls() {
    let mut state = runtime_state_with_provider(DelayedMockProvider::new(
        std::time::Duration::ZERO,
        "unused",
    ));
    state.core_config.task_budget.tool_calls = NonZeroU64::new(1).unwrap();
    crate::runtime::task_budget::begin(&mut state)
        .await
        .unwrap();
    let calls = ["first", "second"].map(|id| NormalizedToolCall {
        id: id.into(),
        name: "update_plan".into(),
        arguments: json!({"items":[{"id":"step", "content":"Verify budget", "status":"pending"}]}),
    });
    let writer = state.agent_files().begin_tape_generation().await.unwrap();
    let mut guard = ToolLoopGuard::new(None, 3);
    let cancel = CancellationToken::new();
    let outcome = orchestrate_tool_batch_internal(
        &mut guard,
        &mut state,
        &calls,
        ToolOrchestratorInputs {
            explicit_command: false,
            cancel: &cancel,
            steering_broker: None,
        },
        (None, None),
        &writer,
        &mut |_| async {},
    )
    .await
    .unwrap();
    writer.finish().await.unwrap();
    assert_eq!(outcome, ToolBatchOrchestratorOutcome::PauseTurn);
    assert!(state.machine.tool_payload_by_call_id("first").is_some());
    assert!(state.machine.tool_payload_by_call_id("second").is_none());
    let wait = state.machine.task_budget_wait.as_ref().unwrap();
    let remaining = &wait.continuation.as_ref().unwrap().tool_calls;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "second");
    assert_eq!(
        state
            .machine
            .task_budget
            .as_ref()
            .unwrap()
            .snapshot()
            .await
            .tool_calls,
        1
    );
}
