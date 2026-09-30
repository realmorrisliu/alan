// Dispatch persistence failures must settle the exact visible identity.
#[tokio::test]
async fn dispatch_ack_failure_publishes_correlated_failed_before_identity_clear() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "must not execute",
        ))
        .await,
    );
    let dir = tempfile::tempdir().unwrap();
    let recorder = crate::rollout::RolloutRecorder::new_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let input = Submission::new(Op::Turn {
        parts: vec![alan_agent_protocol::ContentPart::text("task")],
        context: None,
    });
    state.machine.set_input_recorder_for_test(recorder.clone());
    state.machine.admit_input(&input).await.unwrap();
    let probe = recorder.flush_failure_probe().await;
    state.machine.set_input_recorder_for_test(probe.clone());
    let shell = Shell::new(state.environment.root_transport());
    let broker = TurnInputBroker::default();
    let cancel = CancellationToken::new();
    let id = input.id.clone();
    assert!(
        advance_accepted_submission(&mut state, input, &broker, &cancel)
            .await
            .result
            .is_err()
    );
    assert_eq!(state.machine.current_submission_id(), None);
    assert!(
        state.machine.messages().is_empty(),
        "no generation or Tool execution"
    );
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    let failed: Vec<_> = events
        .lines()
        .filter_map(|line| {
            match serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap() {
                alan_agent_protocol::UiEvent::InputCompleted {
                    submission_ids,
                    status,
                    error,
                } if submission_ids == [id.clone()] => Some((status, error)),
                _ => None,
            }
        })
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, alan_agent_protocol::UiInputStatus::Failed);
    assert!(failed[0].1.as_ref().unwrap().contains("uncertain"));
    assert!(probe.close().await.is_err());
}
