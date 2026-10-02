//! NextTurn is admission, not consumption: exercise the real serialized Process loop.
use super::*;

async fn next_turn_status(
    shell: &alan_shell::Shell,
    id: &str,
) -> anyhow::Result<alan_agent_protocol::UiInputStatus> {
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

async fn next_turn_flow(select_b: bool) {
    let temp = TempDir::new().unwrap();
    let a = MockLlmProvider::new();
    let b = MockLlmProvider::new();
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection("A", Box::new(a.clone()));
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
    core.model_reasoning_effort = Some(alan_agent_protocol::ReasoningEffort::Low);
    let make = |model: &str| CapturedCallable {
        identity: CallableIdentity {
            profile: "managed".into(),
            provider: "openai_responses".into(),
            model: model.into(),
            credential_ref: None,
            revision: model.into(),
        },
        root: root.clone(),
        connection: model.into(),
        config: core.clone(),
    };
    let authority = Arc::new(GatedSelection {
        a: make("A"),
        b: make("B"),
        started: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Notify::new()),
    });
    authority.release.notify_one();
    let env = NamespaceRuntimeEnvironment::new(root, "/agent/1", "A")
        .with_connection_authority(authority);
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
    let next = Submission::new(Op::Input {
        parts: vec![ContentPart::text("queued A payload")],
        mode: InputMode::NextTurn,
    });
    let turn = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("explicit payload")],
        context: None,
    });
    let result: anyhow::Result<()> = async {
        runtime.handle.submission_tx.send(next.clone()).await?;
        // Barrier control proves the NextTurn handler has completed, not just intake.
        let selection = Submission::new(Op::SelectModel { model: if select_b { "B" } else { "A" }.into() });
        runtime.handle.submission_tx.send(selection.clone()).await?;
        completed(&shell, &selection.id).await?;
        let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/proc/2", "test", temp.path()).await?;
        let queue = recovered.input_queue();
        {
            let queue = queue.lock().unwrap();
            anyhow::ensure!(queue.pending.iter().any(|item| matches!(item, QueuedRuntimeItem::Submission(s) if s.id == next.id && serde_json::to_value(s).unwrap() == serde_json::to_value(&next).unwrap())), "exact NextTurn ID and payload lost before consumption");
            anyhow::ensure!(queue.bindings[&next.id].callable_binding.model == "A", "recovery rebound A");
        }
        anyhow::ensure!(env.model_bindings.lock().await.captured.contains_key(&next.id), "private callable A released before consumption");
        runtime.handle.submission_tx.send(turn.clone()).await?;
        if select_b {
            let status = next_turn_status(&shell, &turn.id).await?;
            anyhow::ensure!(status == alan_agent_protocol::UiInputStatus::Failed, "incompatible merge must fail explicitly");
            anyhow::ensure!(a.recorded_requests().is_empty() && b.recorded_requests().is_empty(), "incompatible inputs generated");
            anyhow::ensure!(env.model_bindings.lock().await.captured.contains_key(&next.id), "unconsumed A not retained");
            let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/proc/3", "test", temp.path()).await?;
            let queue = recovered.input_queue();
            let queue = queue.lock().unwrap();
            anyhow::ensure!(queue.pending.iter().any(|item| matches!(item, QueuedRuntimeItem::Submission(s) if s.id == next.id)), "incompatible merge lost queued A recovery");
            anyhow::ensure!(!queue.pending.iter().any(|item| matches!(item, QueuedRuntimeItem::Submission(s) if s.id == turn.id)), "rejected turn replayed");
        } else {
            completed(&shell, &turn.id).await?;
            completed(&shell, &next.id).await?;
            let history = crate::rollout::RolloutRecorder::load_history(&path).await?;
            anyhow::ensure!(history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_inputs_dispatched_v1" && e.payload["submission_ids"].as_array().is_some_and(|ids| ids.contains(&serde_json::json!(next.id)) && ids.contains(&serde_json::json!(turn.id))))), "consumption lacks correlated dispatch evidence");
            let requests = a.recorded_requests();
            anyhow::ensure!(requests.len() == 1 && b.recorded_requests().is_empty(), "compatible merge routing");
            anyhow::ensure!(requests[0].reasoning.effort == Some(alan_agent_protocol::ReasoningEffort::Low), "captured controls changed");
            let text = format!("{:?}", requests[0]);
            anyhow::ensure!(text.contains("queued A payload") && text.contains("explicit payload"), "merge lost payload");
            let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/proc/3", "test", temp.path()).await?;
            anyhow::ensure!(recovered.input_queue().lock().unwrap().pending.is_empty(), "consumed IDs replay on recovery");
        }
        Ok(())
    }.await;
    runtime.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn next_turn_selected_b_rejects_merge_retaining_exact_a() {
    next_turn_flow(true).await;
}
#[tokio::test]
async fn next_turn_compatible_merge_completes_and_recovers_correlated_ids() {
    next_turn_flow(false).await;
}
