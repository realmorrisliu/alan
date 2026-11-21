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

#[tokio::test]
async fn model_selection_persists_only_after_request_control_validation_succeeds() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct PersistTrackingAuthority {
        base: CapturedCallable,
        persisted: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ConnectionAuthority for PersistTrackingAuthority {
        async fn capture(&self, model: Option<&str>) -> Result<CapturedCallable> {
            let mut next = self.base.clone();
            if let Some(model) = model {
                next.identity.model = model.into();
                next.config.openai_responses_model = model.into();
            }
            Ok(next)
        }
        async fn restore(&self, _: &CallableIdentity) -> Result<CapturedCallable> {
            Ok(self.base.clone())
        }
        async fn catalog(&self) -> Result<serde_json::Value> {
            Ok(serde_json::json!({"profile": "main", "models": []}))
        }
        async fn persist_selection(&self, _: &mut CapturedCallable) {
            self.persisted.fetch_add(1, Ordering::SeqCst);
        }
    }

    let persisted = Arc::new(AtomicUsize::new(0));
    let env_base = environment();
    let mut base = callable(&env_base, "gpt-5.4");
    base.config.llm_provider = crate::LlmProvider::OpenAiResponses;
    base.config.openai_responses_model = "gpt-5.4".into();

    let authority = Arc::new(PersistTrackingAuthority {
        base: base.clone(),
        persisted: persisted.clone(),
    });
    let env = environment().with_connection_authority(authority);
    {
        let mut bindings = env.model_bindings.lock().await;
        bindings.confirmed = Some(base);
        // `ultra` is unsupported by OpenAI Responses models -> resolve_runtime_request_controls fails.
        bindings.runtime_intent =
            crate::RequestControlIntent::reasoning_effort(Some(alan_llm::ReasoningEffort::Ultra));
    }

    let mut queues = RuntimeSubmissionQueues::new(AgentMachine::new().input_queue());
    queues.environment = Some(env.clone());

    let rejected = Submission::new(Op::SelectModel {
        model: "gpt-5.2".into(),
    });
    assert!(queues.model_control(&rejected).await);
    assert_eq!(
        persisted.load(Ordering::SeqCst),
        0,
        "rejected model selection must not persist defaults"
    );

    // Once runtime intent is supported, selection succeeds and persists once.
    env.model_bindings.lock().await.runtime_intent =
        crate::RequestControlIntent::reasoning_effort(Some(alan_llm::ReasoningEffort::High));
    let accepted = Submission::new(Op::SelectModel {
        model: "gpt-5.2".into(),
    });
    assert!(queues.model_control(&accepted).await);
    assert_eq!(persisted.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cold_confirmed_binding_reconciles_catalog_on_observation_and_capture() {
    use std::sync::atomic::{AtomicBool, Ordering};

    struct LateCatalogAuthority {
        ready: Arc<AtomicBool>,
        catalog: Arc<crate::ModelCatalog>,
    }

    #[async_trait::async_trait]
    impl ConnectionAuthority for LateCatalogAuthority {
        async fn capture(&self, _: Option<&str>) -> Result<CapturedCallable> {
            anyhow::bail!("unused")
        }
        async fn restore(&self, _: &CallableIdentity) -> Result<CapturedCallable> {
            anyhow::bail!("unused")
        }
        async fn catalog(&self) -> Result<serde_json::Value> {
            Ok(serde_json::json!({"profile": "chatgpt-main", "models": []}))
        }
        async fn refresh_callable_catalog(&self, callable: &mut CapturedCallable) {
            if self.ready.load(Ordering::SeqCst) {
                callable.config.set_model_catalog(self.catalog.clone());
            }
        }
    }

    let ready = Arc::new(AtomicBool::new(false));
    let live_catalog = Arc::new(
        crate::Config::default()
            .resolved_model_catalog()
            .with_chatgpt_models(vec![alan_llm::ProviderModel {
                slug: "gpt-5.3-codex".into(),
                context_window_tokens: 128_000,
                supported_reasoning_efforts: vec![
                    alan_llm::ReasoningEffort::Low,
                    alan_llm::ReasoningEffort::High,
                ],
                default_reasoning_effort: Some(alan_llm::ReasoningEffort::High),
            }])
            .unwrap(),
    );
    let authority = Arc::new(LateCatalogAuthority {
        ready: ready.clone(),
        catalog: live_catalog,
    });
    let env = environment().with_connection_authority(authority);
    let mut initial = callable(&env, "gpt-5.3-codex");
    initial.config.llm_provider = crate::LlmProvider::Chatgpt;
    initial.config.chatgpt_model = "gpt-5.3-codex".into();
    env.model_bindings.lock().await.confirmed = Some(initial);

    let mut queues = RuntimeSubmissionQueues::new(AgentMachine::new().input_queue());
    queues.environment = Some(env.clone());

    // Before live discovery settles, default effort is unknown/None.
    queues.publish_models().await.unwrap();
    assert_eq!(
        queues
            .model_status
            .lock()
            .await
            .selected_next
            .as_ref()
            .unwrap()
            .reasoning
            .effort,
        None
    );

    // Once live discovery settles, publish_models() and capture_input() reconcile the confirmed callable.
    ready.store(true, Ordering::SeqCst);
    queues.publish_models().await.unwrap();
    assert_eq!(
        queues
            .model_status
            .lock()
            .await
            .selected_next
            .as_ref()
            .unwrap()
            .reasoning
            .effort,
        Some(alan_llm::ReasoningEffort::High)
    );

    let input = Submission::new(Op::Input {
        parts: vec![ContentPart::text("hello")],
        mode: InputMode::FollowUp,
    });
    queues.capture_input(&input).await.unwrap();
    let captured_effort = queues
        .outer_queue
        .lock()
        .unwrap()
        .bindings
        .get(&input.id)
        .unwrap()
        .request_controls
        .reasoning_effort();
    assert_eq!(captured_effort, Some(alan_llm::ReasoningEffort::High));
}
