//! Executable Process-loop admission and durable recovery regressions.
use super::*;
#[path = "engine_live_steer_tests.rs"]
mod live_steer;
use crate::runtime::model_binding::{CallableIdentity, CapturedCallable, ConnectionAuthority};

struct NoCallable;
#[async_trait]
impl ConnectionAuthority for NoCallable {
    async fn capture_initial(&self) -> anyhow::Result<Option<CapturedCallable>> {
        Ok(None)
    }
    async fn capture(&self, _: Option<&str>) -> anyhow::Result<CapturedCallable> {
        anyhow::bail!("unavailable")
    }
    async fn restore(&self, _: &CallableIdentity) -> anyhow::Result<CapturedCallable> {
        anyhow::bail!("unavailable")
    }
    async fn catalog(&self) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("unavailable")
    }
}

fn stores(temp: &TempDir) -> crate::AgentRuntimeStoreBindings {
    crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    }
}

async fn settlement(
    shell: &alan_shell::Shell,
    id: &str,
) -> anyhow::Result<(alan_agent_protocol::UiInputStatus, Option<String>)> {
    settlement_at(shell, "/agent/1", id).await
}

async fn settlement_at(
    shell: &alan_shell::Shell,
    agent: &str,
    id: &str,
) -> anyhow::Result<(alan_agent_protocol::UiInputStatus, Option<String>)> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let events =
                String::from_utf8(shell.cat(&format!("{agent}/machine/ui/events")).await?)?;
            for line in events.lines() {
                if let alan_agent_protocol::UiEvent::InputCompleted {
                    submission_ids,
                    status,
                    error,
                } = serde_json::from_str(line)?
                    && submission_ids.iter().any(|entry| entry == id)
                {
                    return Ok((status, error));
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?
}

#[tokio::test]
async fn admission_recovery_confirmed_none_rejects_input_and_turn_before_admission() {
    let temp = TempDir::new().unwrap();
    let mock = MockLlmProvider::new();
    let llmfs = alan_llmfs::LlmFs::new();
    llmfs.register_connection("default", Box::new(mock.clone()));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    let env = NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
        "/agent/1",
        "default",
    )
    .with_connection_authority(Arc::new(NoCallable));
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(stores(&temp)),
            ..Default::default()
        },
        env.clone(),
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let path = runtime
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    let mut outcomes = Vec::new();
    let mut ids = Vec::new();
    for op in [
        Op::Input {
            parts: vec![ContentPart::text("no callable")],
            mode: InputMode::FollowUp,
        },
        Op::Turn {
            parts: vec![ContentPart::text("no callable turn")],
            context: None,
        },
    ] {
        let input = Submission::new(op);
        runtime
            .handle
            .submission_tx
            .send(input.clone())
            .await
            .unwrap();
        outcomes.push(settlement(&shell, &input.id).await);
        ids.push(input.id);
    }
    runtime.shutdown().await.unwrap();
    for outcome in outcomes {
        let (status, error) = outcome.unwrap();
        assert_eq!(status, alan_agent_protocol::UiInputStatus::Failed);
        assert_eq!(
            error.as_deref(),
            Some("No confirmed callable binding; generation input not admitted.")
        );
    }
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_admitted_v1" && ids.iter().any(|id| e.payload["id"] == *id))));
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1")));
    assert!(mock.recorded_requests().is_empty());
    assert!(env.model_bindings.lock().await.captured.is_empty());
}

#[tokio::test]
async fn admission_recovery_live_compatible_steer_keeps_existing_late_boundary() {
    let temp = TempDir::new().unwrap();
    let mock = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection(
        "A",
        Box::new(GatedFirstGeneration {
            mock: mock.clone(),
            started: started.clone(),
            release: release.clone(),
        }),
    );
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(registry)),
        alan_kernel::Access::ReadWrite,
    );
    let env = NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
        "/agent/1",
        "A",
    );
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(stores(&temp)),
            ..Default::default()
        },
        env,
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let path = runtime
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    let active = Submission::new(Op::Input {
        parts: vec![ContentPart::text("active")],
        mode: InputMode::FollowUp,
    });
    let steer = Submission::new(Op::Input {
        parts: vec![ContentPart::text("compatible steering")],
        mode: InputMode::Steer,
    });
    let result: anyhow::Result<_> = async {
        runtime.handle.submission_tx.send(active.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), started.notified()).await?;
        runtime.handle.submission_tx.send(steer.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let history = crate::rollout::RolloutRecorder::load_history(&path).await?;
                if history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_admitted_v1" && e.payload["id"] == steer.id)) { return Ok::<_, anyhow::Error>(()); }
                tokio::task::yield_now().await;
            }
        }).await??;
        // Admission may precede broker.push. A later serialized model control
        // (rejected here because this fixture has no selection authority) proves
        // the observer finished accepting Steer before ending the gated response.
        let barrier = Submission::new(Op::SelectModel { model: "A".into() });
        runtime.handle.submission_tx.send(barrier.clone()).await?;
        anyhow::ensure!(settlement(&shell, &barrier.id).await?.0 == alan_agent_protocol::UiInputStatus::Failed, "late-boundary observer barrier");
        release.notify_one();
        settlement(&shell, &steer.id).await
    }.await;
    release.notify_one();
    runtime.shutdown().await.unwrap();
    let outcome = result.unwrap();
    assert_eq!(outcome.0, alan_agent_protocol::UiInputStatus::Failed);
    assert_eq!(
        outcome.1.as_deref(),
        Some("Steering input arrived after the turn completed; submit a new turn")
    );
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1" && e.payload["submission_id"] == steer.id)), "steer must join active work, not become a new dispatch");
    assert!(!mock.recorded_requests().is_empty());
}

struct RecoveryAuthority {
    a: CapturedCallable,
    b: CapturedCallable,
    available: bool,
    restored: Arc<Mutex<Vec<CallableIdentity>>>,
}
#[async_trait]
impl ConnectionAuthority for RecoveryAuthority {
    async fn capture(&self, model: Option<&str>) -> anyhow::Result<CapturedCallable> {
        anyhow::ensure!(model.is_none(), "PRIVATE_CATALOG_ERROR /private/catalog");
        Ok(self.b.clone())
    }
    async fn restore(&self, identity: &CallableIdentity) -> anyhow::Result<CapturedCallable> {
        if identity == &self.b.identity {
            return Ok(self.b.clone());
        }
        self.restored.lock().unwrap().push(identity.clone());
        anyhow::ensure!(
            self.available && identity == &self.a.identity,
            "exact A unavailable"
        );
        Ok(self.a.clone())
    }
    async fn catalog(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
}

#[tokio::test]
async fn admission_recovery_orphan_steer_available_rejected_before_dispatch() {
    recovered_input(InputMode::Steer, true).await;
}
#[tokio::test]
async fn admission_recovery_orphan_steer_unavailable_rejected_before_dispatch() {
    recovered_input(InputMode::Steer, false).await;
}
#[tokio::test]
async fn admission_recovery_follow_up_restores_a_once_after_continue_not_b() {
    recovered_input(InputMode::FollowUp, true).await;
}

#[tokio::test]
async fn admission_recovery_follow_up_unavailable_never_falls_back_to_b() {
    recovered_input(InputMode::FollowUp, false).await;
}

async fn recovered_input(mode: InputMode, available: bool) {
    use super::super::directory_selection::{SelectionAdapter, SelectionAuthority};
    use crate::runtime::model_binding::InputBinding;
    use crate::tools::{ToolExecutionBinding, ToolProcessRunner, ToolRegistry};
    let temp = TempDir::new().unwrap();
    let stores = stores(&temp);
    let a = MockLlmProvider::new();
    let b = MockLlmProvider::new();
    let llmfs = alan_llmfs::LlmFs::new();
    llmfs.register_connection("A", Box::new(a.clone()));
    llmfs.register_connection("B", Box::new(b.clone()));
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    core.llm_provider = crate::config::LlmProvider::Chatgpt;
    core.chatgpt_model = "A".into();
    core.model_reasoning_effort = Some(alan_agent_protocol::ReasoningEffort::Medium);
    assert!(core.effective_model_info().is_none());
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/agent/2",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let make_callable = |model: &str| CapturedCallable {
        identity: CallableIdentity {
            profile: "exact-profile".into(),
            provider: "chatgpt".into(),
            model: model.into(),
            credential_ref: Some("managed-credential-reference".into()),
            revision: "exact-revision".into(),
        },
        root: root.clone(),
        connection: model.into(),
        config: core.clone(),
    };
    let callable_a = make_callable("A");
    let binding = InputBinding {
        callable_binding: callable_a.identity.clone(),
        request_controls: crate::resolve_runtime_request_controls(
            &core,
            crate::provider_capabilities_for_config(&core),
            Default::default(),
        )
        .unwrap(),
    };
    let source = AgentMachine::new_with_recorder_in_dir("/agent/1", "A", &stores.rollouts)
        .await
        .unwrap();
    let input = Submission {
        id: format!("original-{mode:?}-{available}"),
        intent: Default::default(),
        op: Op::Input {
            parts: vec![ContentPart::text("preserve A")],
            mode,
        },
    };
    source
        .input_queue()
        .lock()
        .unwrap()
        .bindings
        .insert(input.id.clone(), binding.clone());
    source.admit_input(&input).await.unwrap();
    let selected_b = make_callable("B");
    crate::agent_machine::input_queue::persist_input_event(
        source.input_recorder().as_ref(),
        "machine_model_selected_v1",
        serde_json::json!({"submission_id": "persisted-selection-B", "callable_binding": selected_b.identity, "request_controls": binding.request_controls}),
    ).await.unwrap();
    let source_path = source.rollout_path().unwrap().clone();
    source.input_recorder().unwrap().close().await.unwrap();
    let probe =
        AgentMachine::load_from_rollout_in_dir(&source_path, "/agent/probe", "B", &stores.rollouts)
            .await
            .unwrap();
    {
        let queue = probe.input_queue();
        let queue = queue.lock().unwrap();
        assert!(queue.paused);
        assert_eq!(queue.bindings.get(&input.id), Some(&binding));
        assert!(
            matches!(queue.pending.front(), Some(QueuedRuntimeItem::Submission(s)) if serde_json::to_value(s).unwrap() == serde_json::to_value(&input).unwrap())
        );
    }
    let restored = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(RecoveryAuthority {
        a: callable_a.clone(),
        b: make_callable("B"),
        available,
        restored: restored.clone(),
    });
    let runner = ToolProcessRunner::from_registry(&ToolRegistry::new());
    runner.register_process_binding(
        2,
        ToolExecutionBinding::awaiting_host_projection("/mnt/new".into(), temp.path().into())
            .with_adapter(Arc::new(SelectionAdapter("/mnt/new".into()))),
    );
    runner.register_process_authority(2, Arc::new(SelectionAuthority));
    let env = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/2", "B")
        .with_namespace_cwd("/mnt/new")
        .with_tool_process_context(2, runner)
        .with_connection_authority(authority);
    let shell = alan_shell::Shell::new(root);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(stores.clone()),
            recovery_rollout_path: Some(source_path),
            ..Default::default()
        },
        env.clone(),
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let path = runtime
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    let models: alan_agent_protocol::UiModelSnapshot =
        serde_json::from_slice(&shell.cat("/agent/2/machine/ui/models").await.unwrap()).unwrap();
    assert_eq!(
        models.process_path,
        env.process_files().process_path().unwrap()
    );
    assert_ne!(models.process_path, "/proc/1");
    assert!(models.known && models.publication_version > 0 && models.active.is_none());
    assert_eq!(models.selected_next.as_ref().unwrap().model, "B");
    assert_eq!(
        models.selected_next.as_ref().unwrap().reasoning,
        binding.request_controls.reasoning
    );
    assert_eq!(models.admitted.len(), 1);
    assert_eq!(models.admitted[0].submission_id, input.id);
    assert_eq!(models.admitted[0].binding.as_ref().unwrap().model, "A");
    assert_eq!(
        models.admitted[0].binding.as_ref().unwrap().reasoning,
        binding.request_controls.reasoning
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    let paused: alan_agent_protocol::UiActivitySnapshot =
        serde_json::from_slice(&shell.cat("/agent/2/machine/ui/activity").await.unwrap()).unwrap();
    assert_eq!(paused.state, alan_agent_protocol::UiActivityState::Paused);
    assert!(a.recorded_requests().is_empty() && b.recorded_requests().is_empty());
    assert!(
        restored.lock().unwrap().is_empty(),
        "restore only on explicit continue"
    );
    shell
        .write("/agent/2/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    let outcome = settlement_at(&shell, "/agent/2", &input.id).await;
    runtime.shutdown().await.unwrap();
    let (status, _) = outcome.unwrap();
    assert_eq!(restored.lock().unwrap().as_slice(), &[callable_a.identity]);
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    let dispatches = history.iter().filter(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1" && e.payload["submission_id"] == input.id)).count();
    let rejected = mode == InputMode::Steer || !available;
    assert_eq!(
        dispatches,
        usize::from(!rejected),
        "orphan must be removed BEFORE dispatch evidence"
    );
    assert_eq!(
        status,
        if rejected {
            alan_agent_protocol::UiInputStatus::Failed
        } else {
            alan_agent_protocol::UiInputStatus::Completed
        }
    );
    assert_eq!(a.recorded_requests().len(), usize::from(!rejected));
    assert!(b.recorded_requests().is_empty(), "never fall back to B");
    if !rejected {
        assert_eq!(
            a.recorded_requests()[0].reasoning,
            binding.request_controls.reasoning
        );
        assert_eq!(
            callable_a.config.effective_model(),
            binding.callable_binding.model
        );
        let admitted = history
            .iter()
            .find_map(|item| match item {
                crate::rollout::RolloutItem::Event(e)
                    if e.event_type == "machine_input_admitted_v1"
                        && e.payload["id"] == input.id =>
                {
                    Some(&e.payload)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            admitted["callable_binding"],
            serde_json::to_value(&binding.callable_binding).unwrap()
        );
        assert_eq!(
            admitted["request_controls"],
            serde_json::to_value(&binding.request_controls).unwrap()
        );
        assert_eq!(
            a.recorded_requests()[0]
                .messages
                .iter()
                .filter(|m| m.content == "preserve A")
                .count(),
            1
        );
        assert_eq!(
            a.recorded_requests()[0].reasoning.effort,
            binding.request_controls.reasoning_effort()
        );
    }
    if rejected {
        assert!(history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_inputs_removed_v1" && e.payload["submission_ids"] == serde_json::json!([input.id]))));
    }
    let recovered =
        AgentMachine::load_from_rollout_in_dir(&path, "/agent/check", "B", &stores.rollouts)
            .await
            .unwrap();
    assert!(recovered.input_queue().lock().unwrap().pending.is_empty());
    assert!(recovered.input_queue().lock().unwrap().bindings.is_empty());
    assert!(env.model_bindings.lock().await.captured.is_empty());
}
