//! AgentFS projection checks on real A/B Process execution and settled pause.
use super::*;

async fn snapshot(
    shell: &alan_shell::Shell,
) -> anyhow::Result<alan_agent_protocol::UiModelSnapshot> {
    Ok(serde_json::from_slice(
        &shell.cat("/agent/1/machine/ui/models").await?,
    )?)
}

#[tokio::test]
async fn active_and_admitted_a_survive_selection_b_then_settled_pause() {
    let temp = TempDir::new().unwrap();
    let a = MockLlmProvider::new();
    let b = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection(
        "A",
        Box::new(GatedFirstGeneration {
            mock: a.clone(),
            started: started.clone(),
            release: release.clone(),
        }),
    );
    registry.register_connection("B", Box::new(b.clone()));
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
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let make = |model: &str, effort| {
        let mut config = core.clone();
        config.model_reasoning_effort = Some(effort);
        CapturedCallable {
            identity: CallableIdentity {
                profile: "managed".into(),
                provider: "openai_responses".into(),
                model: model.into(),
                credential_ref: Some("PRIVATE_REF".into()),
                revision: "PRIVATE_REV".into(),
            },
            root: root.clone(),
            connection: model.into(),
            config,
        }
    };
    let authority = Arc::new(GatedSelection {
        a: make("A", alan_agent_protocol::ReasoningEffort::Low),
        b: make("B", alan_agent_protocol::ReasoningEffort::High),
        started: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Notify::new()),
    });
    authority.release.notify_one();
    let env = NamespaceRuntimeEnvironment::new(root, "/agent/1", "A")
        .with_connection_authority(authority.clone());
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(crate::AgentRuntimeStoreBindings {
                rollouts: temp.path().join("rollouts"),
                checkpoints: temp.path().join("checkpoints"),
                cache: temp.path().join("cache"),
                tmp: temp.path().join("tmp"),
                metadata: temp.path().join("metadata"),
            }),
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
        parts: vec![ContentPart::text("active A")],
        mode: InputMode::FollowUp,
    });
    let queued = Submission::new(Op::Input {
        parts: vec![ContentPart::text("queued A")],
        mode: InputMode::FollowUp,
    });
    let selection = Submission::new(Op::SelectModel { model: "B".into() });
    let result: anyhow::Result<()> = async {
        let initial = snapshot(&shell).await?;
        runtime.handle.submission_tx.send(active.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), started.notified()).await?;
        runtime.handle.submission_tx.send(queued.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if snapshot(&shell)
                    .await?
                    .admitted
                    .iter()
                    .any(|entry| entry.submission_id == queued.id)
                {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::task::yield_now().await;
            }
        })
        .await??;
        runtime.handle.submission_tx.send(selection.clone()).await?;
        completed(&shell, &selection.id).await?;
        let selected = snapshot(&shell).await?;
        anyhow::ensure!(selected.publication_version > initial.publication_version);
        anyhow::ensure!(selected.active.as_ref().unwrap().model == "A");
        anyhow::ensure!(
            selected.active.as_ref().unwrap().reasoning == a.recorded_requests()[0].reasoning
        );
        anyhow::ensure!(selected.selected_next.as_ref().unwrap().model == "B");
        anyhow::ensure!(
            selected.selected_next.as_ref().unwrap().reasoning.effort
                == Some(alan_agent_protocol::ReasoningEffort::High)
        );
        anyhow::ensure!(
            selected.admitted.len() == 1 && selected.admitted[0].submission_id == queued.id
        );
        anyhow::ensure!(
            selected.admitted[0].binding.as_ref().unwrap() == selected.active.as_ref().unwrap()
        );
        anyhow::ensure!(a.recorded_requests().len() == 1 && b.recorded_requests().is_empty());
        runtime
            .handle
            .submission_tx
            .send(Submission::new(Op::Interrupt))
            .await?;
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let queue: alan_agent_protocol::UiQueueSnapshot =
                    serde_json::from_slice(&shell.cat("/agent/1/machine/ui/queue").await?)?;
                let models = snapshot(&shell).await?;
                if queue.paused && models.active.is_none() {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::task::yield_now().await;
            }
        })
        .await??;
        authority.release.notify_one();
        let paused_select = Submission::new(Op::SelectModel { model: "B".into() });
        runtime
            .handle
            .submission_tx
            .send(paused_select.clone())
            .await?;
        completed(&shell, &paused_select.id).await?;
        let paused = snapshot(&shell).await?;
        let queue: alan_agent_protocol::UiQueueSnapshot =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/queue").await?)?;
        anyhow::ensure!(queue.paused && paused.active.is_none());
        anyhow::ensure!(
            paused.admitted.len() == 1 && paused.admitted[0].submission_id == queued.id
        );
        anyhow::ensure!(paused.selected_next.as_ref().unwrap().model == "B");
        anyhow::ensure!(paused.publication_version > selected.publication_version);
        anyhow::ensure!(a.recorded_requests().len() == 1 && b.recorded_requests().is_empty());
        let raw = serde_json::to_string(&paused)?;
        for private in ["revision", "credential_ref", "PRIVATE_REV", "PRIVATE_REF"] {
            anyhow::ensure!(!raw.contains(private));
        }
        Ok(())
    }
    .await;
    release.notify_one();
    authority.release.notify_one();
    runtime.shutdown().await.unwrap();
    result.unwrap();
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    for id in [&active.id, &queued.id] {
        for (kind, field) in [
            ("machine_input_admitted_v1", "id"),
            ("machine_input_dispatched_v1", "submission_id"),
        ] {
            assert_eq!(history.iter().filter(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == kind && e.payload[field] == *id)).count(), usize::from(kind == "machine_input_admitted_v1" || id == &active.id));
        }
    }
}
