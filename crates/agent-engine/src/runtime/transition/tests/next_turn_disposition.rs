//! Bounded real transition and persistence-fault coverage for NextTurn disposition.
use super::*;
use alan_agent_protocol::ContentPart;

async fn fixture() -> (RuntimeLoopState, TempDir) {
    let temp = TempDir::new().unwrap();
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "answer",
        ))
        .await,
    );
    state.core_config.memory.enabled = false;
    state.machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "test", temp.path())
        .await
        .unwrap();
    (state, temp)
}
fn input() -> Submission {
    Submission::new(Op::Input {
        parts: vec![ContentPart::text("queued")],
        mode: InputMode::NextTurn,
    })
}
fn turn() -> Submission {
    Submission::new(Op::Turn {
        parts: vec![ContentPart::text("explicit")],
        context: None,
    })
}
async fn events(state: &RuntimeLoopState) -> Vec<alan_agent_protocol::UiEvent> {
    String::from_utf8(
        Shell::new(state.environment.root_transport())
            .cat("/agent/1/machine/ui/events")
            .await
            .unwrap(),
    )
    .unwrap()
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect()
}
async fn run(
    state: &mut RuntimeLoopState,
    s: Submission,
    cancel: &CancellationToken,
) -> AcceptedSubmissionOutcome {
    advance_accepted_submission(state, s, &TurnInputBroker::default(), cancel).await
}

#[tokio::test]
async fn followup_cancel_before_start_is_durable_and_exact() {
    let (mut state, temp) = fixture().await;
    let s = input();
    state.machine.admit_input(&s).await.unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    run(&mut state, s.clone(), &cancel).await.result.unwrap();
    assert!(events(&state).await.iter().any(|e| matches!(e, alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Cancelled, .. } if submission_ids == &vec![s.id.clone()])));
    let recovered = AgentMachine::load_from_rollout_in_dir(
        state.machine.rollout_path().unwrap(),
        "/proc/2",
        "test",
        temp.path(),
    )
    .await
    .unwrap();
    assert!(
        recovered.input_queue().lock().unwrap().pending.is_empty(),
        "cancelled NextTurn replayed"
    );
}

#[tokio::test]
async fn followup_overflow_is_durable_and_exact() {
    let (mut state, temp) = fixture().await;
    for _ in 0..16 {
        run(&mut state, input(), &CancellationToken::new())
            .await
            .result
            .unwrap();
    }
    let s = input();
    run(&mut state, s.clone(), &CancellationToken::new()).await;
    assert!(events(&state).await.iter().any(|e| matches!(e, alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Failed, .. } if submission_ids == &vec![s.id.clone()])));
    let recovered = AgentMachine::load_from_rollout_in_dir(
        state.machine.rollout_path().unwrap(),
        "/proc/2",
        "test",
        temp.path(),
    )
    .await
    .unwrap();
    assert!(!recovered.input_queue().lock().unwrap().pending.iter().any(|i| matches!(i, crate::agent_machine::input_queue::QueuedRuntimeItem::Submission(i) if i.id == s.id)));
}

#[tokio::test]
async fn followup_shared_missing_bindings_reject_broker_and_direct() {
    for brokered in [false, true] {
        let (mut state, _) = fixture().await;
        run(&mut state, input(), &CancellationToken::new())
            .await
            .result
            .unwrap();
        let s = turn();
        let mut emit = |_| async {};
        let result = if brokered {
            super::super::accepted_submission::drive_turn_submission_with_cancel(
                &mut state,
                s,
                &TurnInputBroker::default(),
                &mut emit,
                &CancellationToken::new(),
            )
            .await
        } else {
            run(&mut state, s, &CancellationToken::new())
                .await
                .result
                .map(|_| ())
        };
        assert!(
            result
                .unwrap_err()
                .to_string()
                .to_lowercase()
                .contains("incompatible"),
            "must reject at shared compatibility boundary"
        );
        assert_eq!(state.machine.queued_next_turn_input_count(), 1);
    }
}

#[tokio::test]
async fn followup_incompatible_removal_fault_has_no_terminal_receipt() {
    let (mut state, _) = fixture().await;
    run(&mut state, input(), &CancellationToken::new())
        .await
        .result
        .unwrap();
    let s = turn();
    state.machine.admit_input(&s).await.unwrap();
    let recorder = state.machine.input_recorder().unwrap();
    let (probe, _observed) = recorder.batch_failure_probe(false);
    state.machine.set_input_recorder_for_test(probe);
    assert!(
        run(&mut state, s.clone(), &CancellationToken::new())
            .await
            .result
            .is_err()
    );
    assert!(!events(&state).await.iter().any(|e| matches!(e, alan_agent_protocol::UiEvent::InputCompleted { submission_ids, .. } if submission_ids.contains(&s.id))), "uncertain removal issued certain receipt");
    assert!(
        state
            .machine
            .input_queue()
            .lock()
            .unwrap()
            .queue_uncertain_ids
            .contains(&s.id)
    );
}
