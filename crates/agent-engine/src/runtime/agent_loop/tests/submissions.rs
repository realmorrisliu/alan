// Tests for handle_submission
#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the submission fields that define this scenario"
)]
async fn test_handle_submission_cancel() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message("existing history");
    machine.has_active_task = true;
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        current_submission_id: None,
        environment: namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        ))
        .await,
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        turn_state: TurnState::default(),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let submission = Submission::new(alan_agent_protocol::Op::Interrupt);

    let result = handle_submission(&mut state, submission, &mut emit).await;

    assert!(result.is_ok(), "interrupt should succeed: {result:?}");
    assert_eq!(events.len(), 1);
    assert_eq!(state.machine.tape.messages().len(), 1);
    assert_eq!(
        state.machine.tape.messages()[0].text_content(),
        "existing history"
    );
    assert!(!state.machine.has_active_task);
    match &events[0] {
        Event::TurnCompleted { summary } => {
            assert_eq!(summary.as_deref(), Some("Task cancelled by user"));
        }
        _ => panic!("Expected TurnCompleted event"),
    }
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
    machine.has_active_task = true;
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        current_submission_id: None,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        )),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        turn_state: TurnState::default(),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let submission = Submission::new(alan_agent_protocol::Op::Rollback { turns: 1 });

    let result = handle_submission(&mut state, submission, &mut emit).await;

    assert!(result.is_ok());
    assert_eq!(state.machine.tape.messages().len(), 2);
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
