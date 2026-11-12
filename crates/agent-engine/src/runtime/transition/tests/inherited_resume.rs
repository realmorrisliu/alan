//! An ancestor-authorized reconnect must fail closed when the paused work is gone.

use super::*;
use crate::agent_machine::task_budget::InheritedResumeUnrecoverable;

#[tokio::test]
async fn a_reconnect_without_its_paused_work_fails_closed() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join(".alan/memory");
    let mut machine = AgentMachine::new();
    machine.add_user_message("Summarize the repository.");
    machine.begin_turn(0);

    let mut state = create_replay_memory_test_state(memory_dir, machine);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    // The ancestor authorized a reconnect, but this Process never restored the
    // stop: there is no pending yield and no inherited wait to continue.
    let result = handle_submission_with_cancel(
        &mut state,
        Submission::new(alan_agent_protocol::Op::Resume {
            request_id: "inherited_budget_1".to_string(),
            content: vec![],
        }),
        &mut emit,
        &cancel,
    )
    .await;

    let error = result.expect_err("an unresolvable reconnect must not look like an idle turn");
    let unrecoverable = error
        .downcast_ref::<InheritedResumeUnrecoverable>()
        .expect("the reconnect must fail with its own typed error");
    assert_eq!(unrecoverable.checkpoint_id, "inherited_budget_1");
    assert!(
        state.machine.pending_yield("inherited_budget_1").is_none(),
        "a failed reconnect must not fabricate a pending yield"
    );
    assert!(
        state.machine.inherited_budget_wait.is_none(),
        "a failed reconnect must not fabricate an inherited wait"
    );
    assert!(
        state
            .machine
            .pending_tool_replay_batch("inherited_budget_1")
            .is_none(),
        "a failed reconnect must not queue work to replay"
    );
}

#[tokio::test]
async fn a_stale_resume_of_another_kind_stays_idle() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join(".alan/memory");
    let mut machine = AgentMachine::new();
    machine.add_user_message("Summarize the repository.");
    machine.begin_turn(0);

    let mut state = create_replay_memory_test_state(memory_dir, machine);
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(format!("{event:?}"));
        async {}
    };

    let result = handle_submission_with_cancel(
        &mut state,
        Submission::new(alan_agent_protocol::Op::Resume {
            request_id: "tool_escalation_batch-1".to_string(),
            content: vec![],
        }),
        &mut emit,
        &cancel,
    )
    .await;

    assert!(
        result.is_ok(),
        "an unrelated stale resume keeps its idle behaviour"
    );
    assert_eq!(events.len(), 1);
    assert!(events[0].contains("does not match any pending yield"));
}
