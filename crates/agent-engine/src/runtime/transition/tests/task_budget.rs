use super::*;
use crate::agent_machine::task_budget::Phase;
use std::num::NonZeroU64;

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
