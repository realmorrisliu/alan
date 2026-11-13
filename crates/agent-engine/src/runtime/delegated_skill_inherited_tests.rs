use super::*;

#[tokio::test]
async fn inherited_pause_records_the_reconnect_and_stops_the_parent_turn() {
    use crate::runtime::delegated_child_run::{ChildRuntimePause, InheritedPause};
    use alan_agent_protocol::YieldKind;
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks.",
            "host_mounts": [{"grant": "grant-project", "target": "/mnt/source", "access": "read_only"}]
        }),
    };
    let mut emit = |_: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel, _resume| {
            Box::pin(async {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Paused,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: Some(ChildRuntimePause {
                        request_id: "inherited_budget-1".to_string(),
                        kind: YieldKind::Custom("inherited_budget".to_string()),
                        inherited: Some(InheritedPause {
                            rollout_path: String::new(),
                            ancestor_task_id: "ancestor-1".to_string(),
                            dimension: "model_calls".to_string(),
                            checkpoint_id: "inherited_budget_1".to_string(),
                        }),
                    }),
                    child_run: None,
                })
            })
        },
    )
    .await;

    let error = result.expect_err("an inherited stop must not look like a completed call");
    assert!(
        error.is::<crate::agent_machine::task_budget::InheritedExhaustion>(),
        "the parent must receive the typed ancestor stop: {error}"
    );
    assert_eq!(
        state.machine.pending_delegated_resumes,
        vec![crate::agent_machine::pending_resume::PendingResume {
            rollout_path: String::new(),
            tool_call_id: "call_1".to_string(),
            request_id: "inherited_budget_1".to_string(),
            ancestor_task_id: "ancestor-1".to_string(),
            dimension: "model_calls".to_string(),
        }]
    );
}

#[tokio::test]
async fn a_finished_reconnect_is_consumed_so_the_call_cannot_run_twice() {
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    state.machine.pending_delegated_resumes =
        vec![crate::agent_machine::pending_resume::PendingResume {
            rollout_path: String::new(),
            tool_call_id: "call_1".to_string(),
            request_id: "inherited_budget_1".to_string(),
            ancestor_task_id: "ancestor-1".to_string(),
            dimension: "model_calls".to_string(),
        }];
    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks.",
            "host_mounts": [{"grant": "grant-project", "target": "/mnt/source", "access": "read_only"}]
        }),
    };
    let mut emit = |_: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel, _resume| {
            Box::pin(async {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: "reviewed".to_string(),
                    turn_summary: Some("Delegated review completed.".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;

    assert!(result.is_ok(), "a resumed child must complete normally");
    assert!(
        state.machine.pending_delegated_resumes.is_empty(),
        "a finished reconnect must be consumed so it cannot reconnect again"
    );
}

#[tokio::test]
async fn a_parent_owned_allowance_stops_through_the_local_extension_gate() {
    use crate::agent_machine::task_budget::TaskBudget;
    use crate::runtime::delegated_child_run::{ChildRuntimePause, InheritedPause};
    use alan_agent_protocol::YieldKind;
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    let budget = TaskBudget::start("owner".into(), state.core_config.task_budget.clone(), None)
        .await
        .unwrap();
    let owner_id = budget.snapshot().await.task_id;
    // The parent owns the allowance *and* has spent it; that is the only case
    // where its own extension decision is the right response.
    let limit = budget.snapshot().await.limits.model_calls.get();
    for _ in 0..limit {
        budget
            .reserve(crate::agent_machine::task_budget::Admission::Model)
            .await
            .unwrap();
    }
    state.machine.task_budget = Some(budget);
    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks.",
            "host_mounts": [{"grant": "grant-project", "target": "/mnt/source", "access": "read_only"}]
        }),
    };
    let mut emit = |_: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        move |_state, _spec, _cancel, _resume| {
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Paused,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: Some(ChildRuntimePause {
                        request_id: "inherited_budget-1".to_string(),
                        kind: YieldKind::Custom("inherited_budget".to_string()),
                        inherited: Some(InheritedPause {
                            rollout_path: String::new(),
                            ancestor_task_id: owner_id,
                            dimension: "model_calls".to_string(),
                            checkpoint_id: "inherited_budget_1".to_string(),
                        }),
                    }),
                    child_run: None,
                })
            })
        },
    )
    .await;

    let error = result.expect_err("an owned shortage must stop this turn");
    assert!(
        error.is::<crate::agent_machine::task_budget::Exhausted>(),
        "the owning task must raise its own extendable stop: {error}"
    );
    assert!(
        !error.is::<crate::agent_machine::task_budget::InheritedExhaustion>(),
        "an owned shortage must not be reported as inherited"
    );
    assert_eq!(state.machine.pending_delegated_resumes.len(), 1);
    assert_eq!(
        state.machine.pending_delegated_resumes[0].request_id, "inherited_budget_1",
        "the record keeps the durable checkpoint, not the child's namespace request id"
    );
}

/// A delegated call paused by our own allowance, then extended and replayed.
#[tokio::test]
async fn extension_replays_the_delegated_call_exactly_once_with_the_resume_request() {
    use crate::agent_machine::task_budget::{Admission, Dimension, Exhausted, TaskBudget};
    use crate::runtime::delegated_child_run::{ChildRuntimePause, InheritedPause};
    use crate::runtime::task_budget::{pause_tool_batch, resolve};
    use alan_agent_protocol::YieldKind;
    use std::sync::{Arc, Mutex};

    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    // One model call of allowance, so reserving it really exhausts the owner.
    let limits = crate::TaskBudgetLimits {
        execution_seconds: std::num::NonZeroU64::new(1800).unwrap(),
        tool_calls: std::num::NonZeroU64::new(16).unwrap(),
        model_calls: std::num::NonZeroU64::new(1).unwrap(),
    };
    let budget = TaskBudget::start("owner".into(), limits, None)
        .await
        .unwrap();
    let owner_id = budget.snapshot().await.task_id;
    budget.reserve(Admission::Model).await.unwrap();
    state.machine.task_budget = Some(budget);

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks.",
            "host_mounts": [{"grant": "grant-project", "target": "/mnt/source", "access": "read_only"}]
        }),
    };
    // (durable checkpoint, execution record) the reconnect launch received.
    type ObservedResume = Option<(String, Option<String>)>;
    let seen: Arc<Mutex<Vec<ObservedResume>>> = Arc::new(Mutex::new(Vec::new()));
    let cancel = CancellationToken::new();
    let mut emit = |_: Event| async {};

    // First run: the child stops on the allowance this task owns.
    let first = {
        let seen = Arc::clone(&seen);
        let owner_id = owner_id.clone();
        handle_invoke_delegated_skill_with_spawn(
            &mut state,
            &tool_call,
            &tool_call.arguments,
            &cancel,
            &mut emit,
            move |_state, _spec, _cancel, resume| {
                seen.lock()
                    .unwrap()
                    .push(resume.map(|target| (target.request_id, target.rollout_path)));
                Box::pin(async move {
                    Ok(ChildRuntimeResult {
                        status: ChildRuntimeStatus::Paused,
                        process_path: "child-machine".to_string(),
                        child_run_id: None,
                        rollout_path: None,
                        output_text: String::new(),
                        turn_summary: None,
                        structured_output: None,
                        warnings: Vec::new(),
                        error_message: None,
                        pause: Some(ChildRuntimePause {
                            request_id: "inherited_budget-1".to_string(),
                            kind: YieldKind::Custom("inherited_budget".to_string()),
                            inherited: Some(InheritedPause {
                                rollout_path: "child-rollout.jsonl".to_string(),
                                ancestor_task_id: owner_id,
                                dimension: "model_calls".to_string(),
                                checkpoint_id: "inherited_budget_1".to_string(),
                            }),
                        }),
                        child_run: None,
                    })
                })
            },
        )
        .await
    };
    let error = first.expect_err("an owned shortage must stop the turn");
    assert!(error.is::<Exhausted>(), "unexpected error: {error}");
    assert_eq!(state.machine.pending_delegated_resumes.len(), 1);

    // The parent pauses with this very call as its continuation.
    let paused = pause_tool_batch(
        &mut state,
        &Exhausted {
            dimension: Dimension::ModelCalls,
        },
        std::slice::from_ref(&tool_call),
        true,
        &mut emit,
    )
    .await
    .unwrap();
    assert!(paused, "the parent must pause instead of dropping the call");
    let wait = state
        .machine
        .task_budget_wait
        .clone()
        .expect("the pause records a budget wait");
    let continuation = wait.continuation.clone().expect("continuation is kept");
    assert_eq!(
        continuation
            .tool_calls
            .iter()
            .map(|call| call.id.as_str())
            .collect::<Vec<_>>(),
        vec!["call_1"],
        "the paused call is the one that must run again"
    );

    // The owner extends: the call is replayed once, resuming its child.
    resolve(&mut state.machine, &wait.checkpoint, true)
        .await
        .unwrap();
    assert!(state.machine.task_budget_wait.is_none());
    let second = {
        let seen = Arc::clone(&seen);
        handle_invoke_delegated_skill_with_spawn(
            &mut state,
            &tool_call,
            &tool_call.arguments,
            &cancel,
            &mut emit,
            move |_state, _spec, _cancel, resume| {
                seen.lock()
                    .unwrap()
                    .push(resume.map(|target| (target.request_id, target.rollout_path)));
                Box::pin(async {
                    Ok(ChildRuntimeResult {
                        status: ChildRuntimeStatus::Completed,
                        process_path: "child-machine".to_string(),
                        child_run_id: None,
                        rollout_path: None,
                        output_text: "reviewed".to_string(),
                        turn_summary: Some("Delegated review completed.".to_string()),
                        structured_output: None,
                        warnings: Vec::new(),
                        error_message: None,
                        pause: None,
                        child_run: None,
                    })
                })
            },
        )
        .await
    };
    assert!(
        second.is_ok(),
        "the resumed child must complete: {second:?}"
    );

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            None,
            Some((
                "inherited_budget_1".to_string(),
                Some("child-rollout.jsonl".to_string())
            ))
        ],
        "the call runs twice, and the reconnect carries the durable checkpoint plus the record the paused child named"
    );
    assert!(
        state.machine.pending_delegated_resumes.is_empty(),
        "a finished reconnect is consumed so it cannot run again"
    );
}

#[tokio::test]
async fn an_owned_allowance_with_room_does_not_pause_the_owner() {
    use crate::agent_machine::task_budget::TaskBudget;
    use crate::runtime::delegated_child_run::{ChildRuntimePause, InheritedPause};
    use alan_agent_protocol::YieldKind;
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    let budget = TaskBudget::start("owner".into(), state.core_config.task_budget.clone(), None)
        .await
        .unwrap();
    let owner_id = budget.snapshot().await.task_id;
    // The owner has spent nothing, so a child that stops on its own share is not
    // evidence that the owner is out of money.
    state.machine.task_budget = Some(budget);
    let tool_call = NormalizedToolCall {
        id: "call_room".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks.",
            "host_mounts": [{"grant": "grant-project", "target": "/mnt/source", "access": "read_only"}]
        }),
    };
    let mut emit = |_: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        move |_state, _spec, _cancel, _resume| {
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Paused,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: Some(ChildRuntimePause {
                        request_id: "r7".to_string(),
                        kind: YieldKind::Custom("inherited_budget".to_string()),
                        inherited: Some(InheritedPause {
                            rollout_path: String::new(),
                            ancestor_task_id: owner_id,
                            dimension: "model_calls".to_string(),
                            checkpoint_id: "inherited_budget_room".to_string(),
                        }),
                    }),
                    child_run: None,
                })
            })
        },
    )
    .await;

    assert!(
        matches!(result, Ok(VirtualToolOutcome::Continue { .. })),
        "an owner with room keeps the call: {result:?}"
    );
    assert!(
        state.machine.pending_delegated_resumes.is_empty(),
        "no reconnect is recorded when the owner is not short"
    );
}
