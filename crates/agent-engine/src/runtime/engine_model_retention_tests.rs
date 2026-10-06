//! Binding retention follows real Runtime settlement and durable recovery disposition.
use super::*;

#[tokio::test]
async fn settled_binding_retention_runtime_and_recovery() {
    let temp = TempDir::new().unwrap();
    let mock = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection(
        "default",
        Box::new(GatedFirstGeneration {
            mock,
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
        "default",
    );
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
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
    let input = |text: &str| {
        Submission::new(Op::Input {
            parts: vec![ContentPart::text(text)],
            mode: InputMode::FollowUp,
        })
    };
    let active = input("active");
    let removed = input("remove");
    let pending = input("preserve");
    let result: anyhow::Result<_> = async {
        runtime.handle.submission_tx.send(active.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), started.notified()).await?;
        runtime.handle.submission_tx.send(removed.clone()).await?;
        runtime.handle.submission_tx.send(pending.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if env.model_bindings.lock().await.captured.contains_key(&pending.id) { break; }
                tokio::task::yield_now().await;
            }
        }).await?;
        runtime.handle.submission_tx.send(Submission::new(Op::InterruptSubmission { submission_id: removed.id.clone() })).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let text = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await?)?;
                if text.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line), Ok(alan_agent_protocol::UiEvent::InputCompleted { submission_ids, .. }) if submission_ids.contains(&removed.id))) { break Ok::<_,anyhow::Error>(()); }
                tokio::task::yield_now().await;
            }
        }).await??;
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let text = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await?)?;
                if text.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line), Ok(alan_agent_protocol::UiEvent::InputCompleted { submission_ids, .. }) if submission_ids.contains(&active.id))) { break Ok::<_,anyhow::Error>(()); }
                tokio::task::yield_now().await;
            }
        }).await??;
        Ok(env.model_bindings.lock().await.captured.keys().cloned().collect::<std::collections::HashSet<_>>())
    }.await;
    release.notify_one();
    runtime.shutdown().await.unwrap();
    let retained = result.unwrap();
    let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/proc/2", "test", temp.path())
        .await
        .unwrap();
    let queue = recovered.input_queue();
    let queue = queue.lock().unwrap();
    assert_eq!(queue.pending.len(), 1);
    assert_eq!(
        queue
            .bindings
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>(),
        std::collections::HashSet::from([pending.id.clone()]),
        "recovery must retain only pending binding"
    );
    assert_eq!(
        retained,
        std::collections::HashSet::from([pending.id.clone()]),
        "settled captures must be released while paused pending remains"
    );
    assert!(
        queue.admitted_ids.contains(&active.id) && queue.admitted_ids.contains(&removed.id),
        "dedup evidence survives cleanup"
    );
}
