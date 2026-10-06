// Uncertain dispatch is observable error, never terminal settlement.
use alan_agent_protocol::UiEvent;
async fn dispatch_failure_fixture(
    mode: Option<InputMode>,
) -> (
    RuntimeLoopState,
    TempDir,
    Submission,
    crate::runtime::model_binding::InputBinding,
    alan_llm::MockLlmProvider,
) {
    let provider = alan_llm::MockLlmProvider::new();
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(provider.clone()).await,
    );
    let dir = TempDir::new().unwrap();
    state.machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "test", dir.path())
        .await
        .unwrap();
    let input = Submission::new(match mode {
        Some(mode) => Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("exact task")],
            mode,
        },
        None => Op::Turn {
            parts: vec![alan_agent_protocol::ContentPart::text("exact task")],
            context: None,
        },
    });
    let callable = crate::runtime::model_binding::CapturedCallable {
        identity: crate::runtime::model_binding::CallableIdentity {
            profile: "test".into(),
            provider: "openai_responses".into(),
            model: "test".into(),
            credential_ref: None,
            revision: "1".into(),
        },
        root: state.environment.root_transport(),
        connection: "default".into(),
        config: Config::default(),
    };
    let binding = {
        let mut bindings = state.environment.model_bindings.lock().await;
        let binding = bindings.resolve(&callable, &input).unwrap();
        bindings.captured.insert(input.id.clone(), callable);
        binding
    };
    state
        .machine
        .input_queue()
        .lock()
        .unwrap()
        .bindings
        .insert(input.id.clone(), binding.clone());
    state.machine.admit_input(&input).await.unwrap();
    let history = RolloutRecorder::load_history(state.machine.rollout_path().unwrap())
        .await
        .unwrap();
    assert!(history.iter().any(|i| matches!(i, RolloutItem::Event(e) if e.event_type == "machine_input_admitted_v1" && e.payload["id"] == input.id && e.payload["op"] == serde_json::to_value(&input.op).unwrap() && e.payload["callable_binding"] == serde_json::to_value(&binding.callable_binding).unwrap() && e.payload["request_controls"] == serde_json::to_value(&binding.request_controls).unwrap())));
    (state, dir, input, binding, provider)
}

async fn assert_dispatch_uncertain(
    state: &RuntimeLoopState,
    input: &Submission,
    binding: &crate::runtime::model_binding::InputBinding,
) {
    assert_eq!(state.machine.current_submission_id(), None);
    assert!(
        state.machine.messages().is_empty(),
        "no generation or Tool execution"
    );
    let q = state.machine.input_queue();
    {
    let q = q.lock().unwrap();
    assert!(q.queue_uncertain_ids.contains(&input.id));
    assert!(!q.settled_ids.contains(&input.id));
    assert_eq!(&q.bindings[&input.id], binding);
    }
    assert_eq!(
        state.environment.model_bindings.lock().await.captured[&input.id].identity,
        binding.callable_binding
    );
    let events = String::from_utf8(
        Shell::new(state.environment.root_transport())
            .cat("/agent/1/machine/ui/events")
            .await
            .unwrap(),
    )
    .unwrap();
    let events: Vec<UiEvent> = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(!events.iter().any(|e| matches!(e, UiEvent::InputCompleted {submission_ids,..} if submission_ids.contains(&input.id))));
    assert!(events.iter().any(|e| matches!(e, UiEvent::Error {message,recoverable:true} if message.contains(&input.id) && message.contains("uncertain"))),"existing nonterminal error must expose dispatch uncertainty for target ID");
}

#[tokio::test]
async fn dispatch_ack_flush_failure_no_terminal_receipt_recovery_follows_evidence() {
    for mode in [None, Some(InputMode::FollowUp), Some(InputMode::Steer)] {
        let (mut state, dir, input, binding, provider) = dispatch_failure_fixture(mode).await;
        let recorder = state.machine.input_recorder().unwrap();
        let probe = recorder.flush_failure_probe().await;
        state.machine.set_input_recorder_for_test(probe.clone());
        assert!(
            advance_accepted_submission(
                &mut state,
                input.clone(),
                &TurnInputBroker::default(),
                &CancellationToken::new()
            )
            .await
            .result
            .is_err()
        );
        assert_dispatch_uncertain(&state, &input, &binding).await;
        assert_eq!(
            provider.recorded_requests().len(),
            0,
            "uncertain dispatch must not invoke provider"
        );
        assert!(probe.close().await.is_err());
        // Failed flush is not evidence of absence: complete written records govern recovery.
        let history = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        let dispatched = history.iter().any(|i| matches!(i, RolloutItem::Event(e) if (e.event_type == "machine_input_dispatched_v1" && e.payload["submission_id"] == input.id) || (e.event_type == "machine_inputs_dispatched_v1" && e.payload["submission_ids"].as_array().is_some_and(|ids| ids.contains(&json!(input.id))))));
        let recovered =
            AgentMachine::load_from_rollout_in_dir(recorder.path(), "/proc/2", "test", dir.path())
                .await
                .unwrap();
        let q = recovered.input_queue();
        let q = q.lock().unwrap();
        let retained = q.pending.iter().any(|i| matches!(i, crate::agent_machine::input_queue::QueuedRuntimeItem::Submission(s) if serde_json::to_value(s).unwrap() == serde_json::to_value(&input).unwrap()));
        assert_eq!(
            retained, !dispatched,
            "recovery must follow complete durable evidence"
        );
        if !dispatched {
            assert_eq!(&q.bindings[&input.id], &binding);
        }
    }
}

#[tokio::test]
async fn dispatch_ack_no_write_exact_batch_retains_recoverable_input() {
    for mode in [None, Some(InputMode::FollowUp), Some(InputMode::Steer)] {
        let (mut state, dir, input, binding, provider) = dispatch_failure_fixture(mode).await;
        let recorder = state.machine.input_recorder().unwrap();
        let (probe, mut observed) = recorder.batch_failure_probe(false);
        state.machine.set_input_recorder_for_test(probe.clone());
        assert!(
            advance_accepted_submission(
                &mut state,
                input.clone(),
                &TurnInputBroker::default(),
                &CancellationToken::new()
            )
            .await
            .result
            .is_err()
        );
        let batch = observed.recv().await.unwrap();
        assert_eq!(batch.len(), 1);
        assert!(
            matches!(&batch[0],RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1" && e.payload == json!({"submission_id":input.id})),
            "no-write fault must target exact dispatch ID/type"
        );
        assert_dispatch_uncertain(&state, &input, &binding).await;
        assert_eq!(
            provider.recorded_requests().len(),
            0,
            "uncertain dispatch must not invoke provider"
        );
        let history = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        assert!(
            !history
                .iter()
                .any(|i| matches!(i,RolloutItem::Event(e) if e.event_type.contains("dispatched")))
        );
        let recovered =
            AgentMachine::load_from_rollout_in_dir(recorder.path(), "/proc/2", "test", dir.path())
                .await
                .unwrap();
        let q = recovered.input_queue();
        {
        let q = q.lock().unwrap();
        assert!(q.pending.iter().any(|i| matches!(i,crate::agent_machine::input_queue::QueuedRuntimeItem::Submission(s) if serde_json::to_value(s).unwrap() == serde_json::to_value(&input).unwrap())));
        assert_eq!(&q.bindings[&input.id], &binding);
        }
        probe.close().await.unwrap();
    }
}
