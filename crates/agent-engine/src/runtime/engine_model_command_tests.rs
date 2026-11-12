//! Commands must not capture or activate generation authority.
use super::*;

#[tokio::test]
async fn continuation_controls_preserve_the_active_callable_and_ignore_next_selection() {
    let env = environment();
    let active = callable(&env, "admitted-model");
    let next = callable(&env, "next-model");
    let binding = InputBinding {
        callable_binding: active.identity.clone(),
        request_controls: Default::default(),
    };
    *env.active_binding.write().unwrap() = Some((binding.clone(), active.clone()));
    env.model_bindings.lock().await.confirmed = Some(next);
    let mut queues = RuntimeSubmissionQueues::new(AgentMachine::new().input_queue());
    queues.environment = Some(env.clone());
    for op in [
        Op::Resume {
            request_id: "pending-budget-or-permission".into(),
            content: vec![],
        },
        Op::CompactWithOptions { focus: None },
        Op::Rollback { turns: 1 },
    ] {
        queues.activate_binding(&Submission::new(op)).await.unwrap();
        let retained = env.active_binding.read().unwrap();
        let (retained_binding, retained_callable) =
            retained.as_ref().expect("control lost active binding");
        assert_eq!(retained_binding, &binding);
        assert_eq!(retained_callable.identity, active.identity);
    }
}

#[tokio::test]
async fn command_skips_capture_clears_active_and_generation_stays_closed() {
    let env = environment().with_connection_authority(Arc::new(SecretFailure));
    let a = callable(&env, "other-model");
    let binding = InputBinding {
        callable_binding: a.identity.clone(),
        request_controls: Default::default(),
    };
    let queues = RuntimeSubmissionQueues::new(AgentMachine::new().input_queue());
    let mut queues = queues;
    queues.environment = Some(env.clone());
    let command = Submission {
        id: "direct-command".into(),
        intent: alan_agent_protocol::InputIntent::Command,
        op: Op::Input {
            parts: vec![ContentPart::text("cd /")],
            mode: InputMode::FollowUp,
        },
    };
    {
        let mut bindings = env.model_bindings.lock().await;
        bindings.confirmed = Some(a.clone());
        *env.active_binding.write().unwrap() = Some((binding.clone(), a));
    }
    queues.capture_input(&command).await.unwrap();
    assert!(
        !queues
            .outer_queue
            .lock()
            .unwrap()
            .bindings
            .contains_key(&command.id)
    );
    assert!(
        !env.model_bindings
            .lock()
            .await
            .captured
            .contains_key(&command.id)
    );
    // A legacy command binding must not trigger restore or inherit a previous active model.
    queues
        .outer_queue
        .lock()
        .unwrap()
        .bindings
        .insert(command.id.clone(), binding);
    env.model_bindings.lock().await.confirmed = None;
    queues.activate_binding(&command).await.unwrap();
    assert!(env.active_binding.read().unwrap().is_none());
    for op in [
        Op::Input {
            parts: vec![ContentPart::text("generate")],
            mode: InputMode::FollowUp,
        },
        Op::Turn {
            parts: vec![ContentPart::text("generate")],
            context: None,
        },
    ] {
        let input = Submission::new(op);
        assert!(queues.capture_input(&input).await.is_err());
        assert!(queues.activate_binding(&input).await.is_err());
        assert!(env.active_binding.read().unwrap().is_none());
    }
    // Command intent must not exempt an invalid Command Turn from generation validation.
    let mut invalid = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("generate")],
        context: None,
    });
    invalid.intent = alan_agent_protocol::InputIntent::Command;
    assert!(queues.activate_binding(&invalid).await.is_err());
}
