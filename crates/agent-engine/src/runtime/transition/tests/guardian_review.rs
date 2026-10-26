use super::*;
use crate::runtime::guardian::{ReviewContext, build_review_request, generate_review};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct SlowReviewProvider {
    calls: Arc<AtomicUsize>,
    delay: Duration,
}

#[async_trait::async_trait]
impl LlmProvider for SlowReviewProvider {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        assert!(request.tools.is_empty());
        assert!(request.system_prompt.unwrap().contains("UNTRUSTED DATA"));
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
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
        "slow-review"
    }
}

fn request() -> GenerationRequest {
    build_review_request(&ReviewContext {
        policy: crate::runtime::guardian::DEFAULT_REVIEWER_POLICY,
        transcript: "user: inspect this project",
        approval_request: &json!({"tool_name": "bash", "command": "cargo test"}),
    })
}

async fn drive<T>(operation: impl std::future::Future<Output = T>) -> T {
    let clock = async {
        for _ in 0..10_000 {
            for _ in 0..100 {
                tokio::task::yield_now().await;
            }
            tokio::time::advance(Duration::from_millis(5)).await;
        }
        panic!("review did not settle within the fixture bound");
    };
    tokio::select! {
        result = operation => result,
        _ = clock => unreachable!(),
    }
}

#[tokio::test(start_paused = true)]
async fn reviewer_deadline_includes_retry_backoff_without_another_admission() {
    let calls = Arc::new(AtomicUsize::new(0));
    let state = runtime_state_with_provider(SlowReviewProvider {
        calls: calls.clone(),
        delay: Duration::from_millis(900),
    });
    let started = tokio::time::Instant::now();
    let error = drive(generate_review(
        &state.namespace_generation(),
        request(),
        1,
        &CancellationToken::new(),
    ))
    .await
    .unwrap_err();
    assert_eq!(error.to_string(), "reviewer aggregate deadline exceeded");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(started.elapsed() <= Duration::from_millis(1010));
}

#[tokio::test(start_paused = true)]
async fn reviewer_caps_zero_and_long_request_timeouts_and_aborts_before_returning() {
    for timeout in [0, 300] {
        let calls = Arc::new(AtomicUsize::new(0));
        let (root, _) = namespace_root_with_provider(SlowReviewProvider {
            calls: calls.clone(),
            delay: Duration::from_secs(3600),
        });
        let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
        let started = tokio::time::Instant::now();
        let error = drive(generate_review(
            &environment.generation(),
            request(),
            timeout,
            &CancellationToken::new(),
        ))
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "reviewer aggregate deadline exceeded");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(started.elapsed() <= Duration::from_millis(30_010));
        let status = Shell::new(root)
            .cat("/mnt/llm/connections/default/g0/status")
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&status).unwrap()["status"],
            "aborted"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn reviewer_retries_charge_the_original_task_and_keep_the_call_limit() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut state = runtime_state_with_provider(SlowReviewProvider {
        calls: calls.clone(),
        delay: Duration::ZERO,
    });
    state.core_config.memory.enabled = false;
    state.machine.accept_submission("review-retry-limit");
    crate::runtime::task_budget::begin(&mut state)
        .await
        .unwrap();
    let error = drive(generate_review(
        &state.namespace_generation(),
        request(),
        0,
        &CancellationToken::new(),
    ))
    .await
    .unwrap_err();
    assert!(!error.to_string().contains("aggregate deadline"));
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    let spent = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!(spent.model_calls, 4);
}

#[tokio::test(start_paused = true)]
async fn reviewer_preserves_the_shorter_task_deadline() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut state = runtime_state_with_provider(SlowReviewProvider {
        calls: calls.clone(),
        delay: Duration::from_secs(3600),
    });
    state.core_config.memory.enabled = false;
    state.core_config.task_budget.execution_seconds = std::num::NonZeroU64::new(1).unwrap();
    state.machine.accept_submission("review-task-deadline");
    crate::runtime::task_budget::begin(&mut state)
        .await
        .unwrap();
    let error = drive(generate_review(
        &state.namespace_generation(),
        request(),
        0,
        &CancellationToken::new(),
    ))
    .await
    .unwrap_err();
    assert!(error.is::<crate::agent_machine::task_budget::Exhausted>());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn reviewer_user_cancellation_retires_without_retry_or_cancelling_its_parent() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (root, _) = namespace_root_with_provider(SlowReviewProvider {
        calls: calls.clone(),
        delay: Duration::from_secs(3600),
    });
    let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
    let parent = CancellationToken::new();
    let cancel = parent.child_token();
    let generation = environment.generation();
    let (result, ()) = drive(async {
        tokio::join!(generate_review(&generation, request(), 0, &cancel), async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
            cancel.cancel();
        },)
    })
    .await;
    assert_eq!(result.unwrap_err().to_string(), "reviewer cancelled");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!parent.is_cancelled());
    let status = Shell::new(root)
        .cat("/mnt/llm/connections/default/g0/status")
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&status).unwrap()["status"],
        "aborted"
    );
}

#[tokio::test(start_paused = true)]
async fn reviewer_deadline_preserves_one_correlated_wait_but_user_cancellation_does_not() {
    use crate::runtime::tool_authorization::{
        ToolAuthorizationOutcome, ToolAuthorizationRequest, authorize_tool_call,
    };

    for cancelled in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut state = runtime_state_with_provider(SlowReviewProvider {
            calls: calls.clone(),
            delay: Duration::from_secs(3600),
        });
        state.runtime_config.llm_request_timeout_secs = 1;
        let call = NormalizedToolCall {
            id: "reviewed-call".into(),
            name: "do_thing".into(),
            arguments: json!({}),
        };
        let cancel = CancellationToken::new();
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            async {}
        };
        let runtime = tool_authorization_runtime(&mut state);
        let (result, ()) = drive(async {
            tokio::join!(
                authorize_tool_call(
                    runtime,
                    ToolAuthorizationRequest {
                        explicit_command: false,
                        tool_call: &call,
                        tool_arguments: &call.arguments,
                        tool_capability: alan_agent_protocol::ToolCapability::Unknown,
                        current_tool_cwd: None,
                        allow_approved_tool_escalation_execution: false,
                        cancel: &cancel,
                    },
                    &mut emit,
                ),
                async {
                    if cancelled {
                        while calls.load(Ordering::SeqCst) == 0 {
                            tokio::task::yield_now().await;
                        }
                        cancel.cancel();
                    }
                },
            )
        })
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let requests = state.agent_files().request_ids().await.unwrap();
        let yields: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Yield { request_id, .. } => Some(request_id),
                _ => None,
            })
            .collect();
        if cancelled {
            assert_eq!(result.unwrap_err().to_string(), "Tool review cancelled");
            assert!(state.machine.pending_confirmation().is_none());
            assert!(requests.is_empty());
            assert!(yields.is_empty());
        } else {
            assert!(matches!(
                result.unwrap(),
                ToolAuthorizationOutcome::PauseTurn
            ));
            let pending = state.machine.pending_confirmation().unwrap();
            assert_eq!(pending.checkpoint_id, "tool_escalation_reviewed-call");
            assert_eq!(pending.details["replay_tool_call"]["call_id"], call.id);
            assert_eq!(requests.len(), 1);
            assert_eq!(yields, vec![&requests[0]]);
        }
    }
}
