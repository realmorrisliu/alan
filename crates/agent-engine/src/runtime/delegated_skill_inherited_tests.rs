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
        |_state, _spec, _cancel| {
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
                            ancestor_task_id: "ancestor-1".to_string(),
                            dimension: "model_calls".to_string(),
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
            tool_call_id: "call_1".to_string(),
            request_id: "inherited_budget-1".to_string(),
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
            tool_call_id: "call_1".to_string(),
            request_id: "inherited_budget-1".to_string(),
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
        |_state, _spec, _cancel| {
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
