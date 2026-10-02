//! Selection persistence and catalog availability are independent boundaries.
use super::*;

#[tokio::test]
async fn selection_recorder_failure_retains_a_and_exact_pending_active_bindings() {
    let temp = TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", temp.path())
        .await
        .unwrap();
    let env = environment();
    let a = callable(&env, "A");
    let b = callable(&env, "B");
    let authority = Arc::new(SelectionPair { a: a.clone(), b });
    let env = env.with_connection_authority(authority);
    let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
    queues.environment = Some(env.clone());
    queues.recorder = machine.input_recorder();
    queues
        .initialize_bindings(&crate::Config::default(), Default::default())
        .await
        .unwrap();
    let pending = Submission::new(Op::Input {
        parts: vec![ContentPart::text("pending A")],
        mode: InputMode::FollowUp,
    });
    queues.admit_input(&pending).await.unwrap();
    queues.activate_binding(&pending).await.unwrap();
    let captured = env.model_bindings.lock().await.captured[&pending.id]
        .identity
        .clone();
    let active = env
        .active_binding
        .read()
        .unwrap()
        .as_ref()
        .unwrap()
        .0
        .clone();
    let binding = queues.outer_queue.lock().unwrap().bindings[&pending.id].clone();
    let (probe, mut observed) = machine.input_recorder().unwrap().batch_failure_probe(false);
    queues.recorder = Some(probe.clone());
    let selection = Submission::new(Op::SelectModel { model: "B".into() });
    assert!(queues.model_control(&selection).await);
    let batch = tokio::time::timeout(Duration::from_secs(5), observed.recv())
        .await
        .expect("selection persistence attempt timed out")
        .unwrap();
    assert!(
        matches!(&batch[0], crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_model_selected_v1" && e.payload["submission_id"] == selection.id)
    );
    assert_eq!(
        env.model_bindings
            .lock()
            .await
            .confirmed
            .as_ref()
            .unwrap()
            .identity,
        a.identity
    );
    assert_eq!(
        env.model_bindings.lock().await.captured[&pending.id].identity,
        captured
    );
    assert_eq!(
        env.active_binding.read().unwrap().as_ref().unwrap().0,
        active
    );
    assert_eq!(
        queues.outer_queue.lock().unwrap().bindings[&pending.id],
        binding
    );
    assert!(!queues.is_paused());
    let shell = alan_shell::Shell::new(env.root_transport());
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(), alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Failed, .. } if submission_ids == vec![selection.id.clone()])));
    let snapshot: alan_agent_protocol::UiModelSnapshot =
        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/models").await.unwrap()).unwrap();
    assert_eq!(snapshot.selected_next.unwrap().model, "A");
    assert_eq!(snapshot.admitted[0].submission_id, pending.id);
    let history = crate::rollout::RolloutRecorder::load_history(machine.rollout_path().unwrap())
        .await
        .unwrap();
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_model_selected_v1" || e.event_type == "machine_input_dispatched_v1")));
    probe.close().await.unwrap();
    machine.input_recorder().unwrap().close().await.unwrap();
}

struct SelectionPair {
    a: CapturedCallable,
    b: CapturedCallable,
}
#[async_trait]
impl ConnectionAuthority for SelectionPair {
    async fn capture(&self, model: Option<&str>) -> Result<CapturedCallable> {
        Ok(if model == Some("B") {
            self.b.clone()
        } else {
            self.a.clone()
        })
    }
    async fn restore(&self, identity: &CallableIdentity) -> Result<CapturedCallable> {
        let value = if identity == &self.a.identity {
            &self.a
        } else {
            &self.b
        };
        anyhow::ensure!(identity == &value.identity, "exact restore unavailable");
        Ok(value.clone())
    }
    async fn catalog(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
}

async fn failed_selection(
    shell: &alan_shell::Shell,
    id: &str,
) -> Result<alan_agent_protocol::UiInputStatus> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await?)?;
            for line in events.lines() {
                if let alan_agent_protocol::UiEvent::InputCompleted {
                    submission_ids,
                    status,
                    ..
                } = serde_json::from_str(line)?
                    && submission_ids.iter().any(|entry| entry == id)
                {
                    return Ok(status);
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

struct UnavailableCatalog(CapturedCallable);
#[async_trait]
impl ConnectionAuthority for UnavailableCatalog {
    async fn capture(&self, model: Option<&str>) -> Result<CapturedCallable> {
        anyhow::ensure!(model.is_none(), "PRIVATE_CATALOG_ERROR /private/catalog");
        Ok(self.0.clone())
    }
    async fn restore(&self, identity: &CallableIdentity) -> Result<CapturedCallable> {
        anyhow::ensure!(identity == &self.0.identity, "exact restore unavailable");
        Ok(self.0.clone())
    }
    async fn catalog(&self) -> Result<serde_json::Value> {
        anyhow::bail!("PRIVATE_CATALOG_ERROR /private/catalog")
    }
}

#[tokio::test]
async fn unavailable_catalog_real_process_retains_a_and_rejects_exact_selection() {
    let temp = TempDir::new().unwrap();
    let env = environment();
    let mut a = callable(&env, "A");
    a.identity.revision = "PRIVATE_REV".into();
    a.identity.credential_ref = Some("PRIVATE_CREDENTIAL".into());
    a.config.memory.enabled = false;
    a.config.model_reasoning_effort = Some(alan_agent_protocol::ReasoningEffort::Low);
    let core = a.config.clone();
    let binding = InputBinding {
        callable_binding: a.identity.clone(),
        request_controls: crate::resolve_runtime_request_controls(
            &core,
            crate::provider_capabilities_for_config(&core),
            Default::default(),
        )
        .unwrap(),
    };
    let source = AgentMachine::new_with_recorder_in_dir("/agent/old", "A", temp.path())
        .await
        .unwrap();
    let pending = Submission::new(Op::Input {
        parts: vec![ContentPart::text("pending A")],
        mode: InputMode::FollowUp,
    });
    source
        .input_queue()
        .lock()
        .unwrap()
        .bindings
        .insert(pending.id.clone(), binding.clone());
    source.admit_input(&pending).await.unwrap();
    let path = source.rollout_path().unwrap().clone();
    source.input_recorder().unwrap().close().await.unwrap();
    let env = env.with_connection_authority(Arc::new(UnavailableCatalog(a)));
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            recovery_rollout_path: Some(path),
            store_bindings: Some(crate::AgentRuntimeStoreBindings {
                rollouts: temp.path().into(),
                checkpoints: temp.path().join("checkpoints"),
                cache: temp.path().join("cache"),
                tmp: temp.path().join("tmp"),
                metadata: temp.path().join("metadata"),
            }),
            ..Default::default()
        },
        env.clone(),
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let ready = runtime.wait_until_ready().await.unwrap();
    let before: alan_agent_protocol::UiModelSnapshot =
        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/models").await.unwrap()).unwrap();
    let selection = Submission::new(Op::SelectModel { model: "B".into() });
    runtime
        .handle
        .submission_tx
        .send(selection.clone())
        .await
        .unwrap();
    let outcome = failed_selection(&shell, &selection.id).await;
    let after: alan_agent_protocol::UiModelSnapshot =
        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/models").await.unwrap()).unwrap();
    let activity: alan_agent_protocol::UiActivitySnapshot =
        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/activity").await.unwrap()).unwrap();
    runtime.shutdown().await.unwrap();
    assert_eq!(outcome.unwrap(), alan_agent_protocol::UiInputStatus::Failed);
    assert_eq!(before, after);
    assert!(after.catalog.is_none() && after.active.is_none());
    assert_eq!(after.selected_next.as_ref().unwrap().model, "A");
    assert_eq!(
        after.selected_next.as_ref().unwrap().reasoning,
        binding.request_controls.reasoning
    );
    assert_eq!(after.admitted.len(), 1);
    assert_eq!(after.admitted[0].submission_id, pending.id);
    let admitted = after.admitted[0].binding.as_ref().unwrap();
    assert_eq!(admitted.model, "A");
    assert_eq!(admitted.reasoning, binding.request_controls.reasoning);
    assert_eq!(
        admitted.control_source,
        alan_agent_protocol::UiModelControlSource::AgentConfig
    );
    assert_eq!(activity.state, alan_agent_protocol::UiActivityState::Paused);
    let raw = serde_json::to_string(&after).unwrap();
    for private in [
        "PRIVATE_REV",
        "PRIVATE_CREDENTIAL",
        "PRIVATE_CATALOG_ERROR",
        "revision",
        "credential_ref",
        "/private/catalog",
    ] {
        assert!(!raw.contains(private));
    }
    let history = crate::rollout::RolloutRecorder::load_history(&ready.rollout_path.unwrap())
        .await
        .unwrap();
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1" || e.event_type == "machine_model_selected_v1")));
}
