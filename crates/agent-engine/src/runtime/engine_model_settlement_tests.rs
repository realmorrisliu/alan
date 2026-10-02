//! Acknowledged removal must not be undone by failure publication.
use super::*;

async fn assert_excluded(
    machine: &AgentMachine,
    queues: &RuntimeSubmissionQueues,
    path: &std::path::Path,
    dir: &std::path::Path,
    id: &str,
) {
    let local = queues
        .outer_queue
        .lock()
        .unwrap()
        .pending
        .iter()
        .filter_map(|item| match item {
            QueuedRuntimeItem::Submission(s) => Some(s.id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let recovered =
        AgentMachine::load_from_rollout_in_dir(&path.to_path_buf(), "/agent/2", "test", dir)
            .await
            .unwrap();
    let durable = recovered
        .input_queue()
        .lock()
        .unwrap()
        .pending
        .iter()
        .filter_map(|item| match item {
            QueuedRuntimeItem::Submission(s) => Some(s.id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        local, durable,
        "local disposition must match real recovery for {id}"
    );
    assert!(!local.contains(&id.to_owned()));
    assert!(
        !queues.is_paused(),
        "confirmed removal must not pause execution to repair UI"
    );
    assert!(machine.messages().is_empty());
    assert!(queues.active_turn_broker.drain().await.is_empty());
    let history = crate::rollout::RolloutRecorder::load_history(&path.to_path_buf())
        .await
        .unwrap();
    assert!(history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_inputs_removed_v1" && e.payload["submission_ids"] == serde_json::json!([id]))));
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1")));
    assert!(
        !history.iter().any(|item| matches!(
            item,
            crate::rollout::RolloutItem::Message(_)
                | crate::rollout::RolloutItem::ToolCall(_)
                | crate::rollout::RolloutItem::Effect(_)
                | crate::rollout::RolloutItem::TurnContext(_)
        )),
        "excluded input must not generate, mutate Tape, or execute Tools"
    );
    assert!(recovered.messages().is_empty());
}

#[tokio::test]
async fn acknowledged_binding_failure_is_excluded_and_safely_failed() {
    let dir = TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let path = machine.rollout_path().unwrap().clone();
    let env = environment().with_connection_authority(Arc::new(SecretFailure));
    env.model_bindings.lock().await.confirmed = Some(callable(&env, "unavailable"));
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    queues.environment = Some(env.clone());
    queues.recorder = machine.input_recorder();
    let input = Submission {
        id: "binding-excluded".into(),
        intent: Default::default(),
        op: Op::Input {
            parts: vec![ContentPart::text("never run")],
            mode: InputMode::FollowUp,
        },
    };
    queues.admit_input(&input).await.unwrap();
    env.model_bindings.lock().await.captured.clear();
    let error = queues.activate_binding(&input).await.unwrap_err();
    assert!(
        queues
            .fail_accepted_input(
                &input,
                &error,
                "Captured callable unavailable; input excluded without execution."
            )
            .await
    );
    assert_excluded(&machine, &queues, &path, dir.path(), &input.id).await;
    let events = String::from_utf8(
        alan_shell::Shell::new(env.root_transport())
            .cat("/agent/1/machine/ui/events")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(), alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Failed, .. } if submission_ids == vec![input.id.clone()])));
    assert!(!events.contains("SECRET_SENTINEL"));
    assert!(!events.contains("HOST_SENTINEL"));
}

#[tokio::test]
async fn binding_failure_with_unknown_removal_stays_paused() {
    let dir = TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let path = machine.rollout_path().unwrap().clone();
    let env = environment().with_connection_authority(Arc::new(SecretFailure));
    env.model_bindings.lock().await.confirmed = Some(callable(&env, "unavailable"));
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    queues.environment = Some(env.clone());
    queues.recorder = machine.input_recorder();
    let input = Submission {
        id: "binding-uncertain".into(),
        intent: Default::default(),
        op: Op::Input {
            parts: vec![ContentPart::text("never execute")],
            mode: InputMode::FollowUp,
        },
    };
    queues.admit_input(&input).await.unwrap();
    env.model_bindings.lock().await.captured.clear();
    let error = queues.activate_binding(&input).await.unwrap_err();
    let (probe, mut observed) = queues.recorder.as_ref().unwrap().batch_failure_probe(false);
    queues.recorder = Some(probe.clone());
    assert!(
        !queues
            .fail_accepted_input(
                &input,
                &error,
                "Captured callable unavailable; input excluded without execution."
            )
            .await
    );
    assert_eq!(observed.recv().await.unwrap().len(), 1);
    queues
        .handle_control(
            &Submission::new(Op::ContinueQueue),
            &env.agent_files(),
            None,
        )
        .await;
    assert!(queues.is_paused());
    assert!(queues.pop_outer().is_none());
    let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
        .await
        .unwrap();
    for queue in [machine.input_queue(), recovered.input_queue()] {
        let queue = queue.lock().unwrap();
        assert_eq!(queue.pending.len(), 1);
        assert!(matches!(&queue.pending[0], QueuedRuntimeItem::Submission(s) if s.id == input.id));
    }
    assert!(machine.messages().is_empty());
    assert!(queues.active_turn_broker.drain().await.is_empty());
    let events = String::from_utf8(
        alan_shell::Shell::new(env.root_transport())
            .cat("/agent/1/machine/ui/events")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!events.contains("input_completed"));
    assert!(!events.contains("SECRET_SENTINEL"));
    probe.close().await.unwrap();
}

#[tokio::test]
async fn post_removal_ui_failure_never_requeues_steer() {
    let dir = TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let path = machine.rollout_path().unwrap().clone();
    // No AgentFS mount: both settlement and warning publication actually fail.
    let env = namespace_environment_for_test();
    let a = callable(&env, "a");
    {
        let mut bindings = env.model_bindings.lock().await;
        bindings.confirmed = Some(callable(&env, "b"));
        *env.active_binding.write().unwrap() = Some((
            InputBinding {
                callable_binding: a.identity.clone(),
                request_controls: Default::default(),
            },
            a,
        ));
    }
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    queues.environment = Some(env.clone());
    queues.recorder = machine.input_recorder();
    let input = Submission {
        id: "ui-excluded".into(),
        intent: Default::default(),
        op: Op::Input {
            parts: vec![ContentPart::text("never steer")],
            mode: InputMode::Steer,
        },
    };
    // Exercise the actual steering caller with an actual failed AgentFS publication.
    queues
        .admit_during_submission(input.clone(), Default::default(), true)
        .await;
    assert_excluded(&machine, &queues, &path, dir.path(), &input.id).await;
    assert!(
        !queues
            .fail_accepted_input(
                &input,
                &anyhow!("incompatible steering binding"),
                "Steering callable or resolved controls differ from active work"
            )
            .await,
        "failed publication must explicitly report observation uncertainty"
    );
    // Exercise the actual steering caller after acknowledged removal as well.
    queues
        .admit_during_submission(input.clone(), Default::default(), true)
        .await;
    assert_excluded(&machine, &queues, &path, dir.path(), &input.id).await;
}
