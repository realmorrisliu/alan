// Tests for direct submission handling
#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the submission fields that define this scenario"
)]
async fn test_handle_submission_cancel() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message("existing history");
    machine.activate_task();
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        ))
        .await,
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let submission = Submission::new(alan_agent_protocol::Op::Interrupt);

    let cancel = CancellationToken::new();
    let result = handle_submission_with_cancel(&mut state, submission, &mut emit, &cancel).await;

    assert!(result.is_ok(), "interrupt should succeed: {result:?}");
    assert_eq!(events.len(), 1);
    assert_eq!(state.machine.messages().len(), 1);
    assert_eq!(
        state.machine.messages()[0].text_content(),
        "existing history"
    );
    assert!(!state.machine.has_active_task());
    match &events[0] {
        Event::TurnCompleted { summary } => {
            assert_eq!(summary.as_deref(), Some("Task cancelled by user"));
        }
        _ => panic!("Expected TurnCompleted event"),
    }
}

#[tokio::test]
async fn accepted_transition_returns_compact_outcome_and_clears_submission_identity() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        ))
        .await,
    );
    let broker = TurnInputBroker::default();
    let cancel = CancellationToken::new();

    let outcome = advance_accepted_submission(
        &mut state,
        Submission::new(alan_agent_protocol::Op::Interrupt),
        &broker,
        &cancel,
    )
    .await;

    assert_eq!(
        outcome.result.expect("interrupt transition should succeed"),
        TransitionCompletion::Completed
    );
    assert!(!outcome.requeue_inband_submissions);
    assert!(outcome.deferred_actions.is_empty());
    assert_eq!(state.machine.current_submission_id(), None);
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the submission fields that define this scenario"
)]
async fn test_handle_submission_rollback() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message("u1");
    machine.add_assistant_message("a1", None);
    machine.add_user_message("u2");
    machine.add_assistant_message("a2", None);
    machine.activate_task();
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let submission = Submission::new(alan_agent_protocol::Op::Rollback { turns: 1 });

    let cancel = CancellationToken::new();
    let result = handle_submission_with_cancel(&mut state, submission, &mut emit, &cancel).await;

    assert!(result.is_ok());
    assert_eq!(state.machine.messages().len(), 2);
    assert_eq!(events.len(), 3);
    assert!(events.iter().any(|event| matches!(
        event,
        Event::MachineRolledBack {
            turns: 1,
            removed_messages: 2,
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TextDelta { chunk, is_final }
            if *is_final && chunk.contains("Rolled back 1 turn(s), removed 2 message(s).")
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Warning { message }
            if message == crate::ROLLBACK_NON_DURABLE_WARNING
    )));
}

#[tokio::test]
async fn command_intent_cannot_enter_agent_transition_or_inband_steering() {
    use alan_agent_protocol::{ContentPart, InputIntent};
    use crate::runtime::turn_input::is_turn_inband_submission;
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "must not generate",
        ))
        .await,
    );
    let mut emit = |_event| async {};
    let cancel = CancellationToken::new();
    for mode in [InputMode::Steer, InputMode::FollowUp, InputMode::NextTurn] {
        let submission = Submission {
            id: uuid::Uuid::new_v4().to_string(),
            intent: InputIntent::Command,
            op: Op::Input { parts: vec![ContentPart::text("pwd")], mode },
        };
        assert!(!is_turn_inband_submission(&submission));
        let error = handle_submission_with_cancel(&mut state, submission, &mut emit, &cancel)
            .await.unwrap_err();
        assert!(error.to_string().contains("governed command admission"));
        assert!(state.machine.messages().is_empty());
    }
}

#[tokio::test]
async fn identical_inputs_publish_distinct_submission_ids_on_tape() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "answer",
        )).await,
    );
    let broker = TurnInputBroker::default();
    let cancel = CancellationToken::new();
    for id in ["client-one", "client-two"] {
        let mut submission = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("identical question")],
            mode: InputMode::FollowUp,
        });
        submission.id = id.into();
        advance_accepted_submission(&mut state, submission, &broker, &cancel)
            .await.result.unwrap();
    }
    let shell = Shell::new(state.environment.root_transport());
    let tape = shell.cat(&format!("{}/machine/tape", state.environment.agent_path())).await.unwrap();
    let records: Vec<serde_json::Value> = std::str::from_utf8(&tape).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    for id in ["client-one", "client-two"] {
        let matching: Vec<_> = records.iter().filter(|record| record["submission_id"] == id).collect();
        assert_eq!(matching.len(), 2, "one user and one assistant record per input: {records:?}");
        assert_eq!(matching[0]["role"], "user");
        assert_eq!(matching[0]["content"], "identical question");
        assert_eq!(matching[1]["role"], "assistant");
        assert_eq!(matching[1]["content"], "answer");
    }
    let events = shell.cat(&format!("{}/machine/ui/events", state.environment.agent_path())).await.unwrap();
    let ids: Vec<_> = std::str::from_utf8(&events).unwrap().lines()
        .map(|line| serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap())
        .filter_map(|event| match event {
            alan_agent_protocol::UiEvent::InputCompleted { submission_ids, .. } => Some(submission_ids),
            _ => None,
        }).collect();
    assert_eq!(ids, vec![vec!["client-one"], vec!["client-two"]]);
}

#[tokio::test]
async fn input_completion_distinguishes_success_failure_and_cancellation() {
    use alan_agent_protocol::{UiEvent, UiInputStatus};
    for (mode, cancelled, expected) in [
        (InputMode::FollowUp, false, UiInputStatus::Completed),
        (InputMode::FollowUp, true, UiInputStatus::Cancelled),
        (InputMode::Steer, false, UiInputStatus::Failed),
    ] {
        let mut state = runtime_state_with_environment(
            namespace_environment_with_live_process(DelayedMockProvider::new(
                tokio::time::Duration::ZERO, "answer",
            )).await,
        );
        let submission = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("same question")], mode,
        });
        let id = submission.id.clone();
        let cancel = CancellationToken::new();
        if cancelled { cancel.cancel(); }
        let _ = advance_accepted_submission(&mut state, submission, &TurnInputBroker::default(), &cancel).await;
        let shell = Shell::new(state.environment.root_transport());
        let bytes = shell.cat(&format!("{}/machine/ui/events", state.environment.agent_path())).await.unwrap();
        let completions: Vec<_> = std::str::from_utf8(&bytes).unwrap().lines()
            .map(|line| serde_json::from_str::<UiEvent>(line).unwrap())
            .filter_map(|event| match event {
                UiEvent::InputCompleted { submission_ids, status, .. } => Some((submission_ids, status)),
                _ => None,
            }).collect();
        assert_eq!(completions, vec![(vec![id], expected)]);
        assert!(state.machine.current_submission_id().is_none());
    }
}
