//! Actual turn-end promotion uses the retained callable after input settlement.
use super::*;

struct GatedPromotion {
    mock: MockLlmProvider,
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
#[async_trait]
impl LlmProvider for GatedPromotion {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        let response = self.mock.generate(request).await?;
        if self.mock.recorded_requests().len() == 2 {
            self.started.notify_one();
            self.release.notified().await;
        }
        Ok(response)
    }
    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        Ok(response_stream(self.generate(request).await?))
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("unused")
    }
    fn provider_name(&self) -> &'static str {
        "gated_promotion"
    }
}

#[tokio::test]
async fn deferred_generation_observes_actual_a_while_next_b_and_clears_on_idle() {
    let temp = TempDir::new().unwrap();
    let memory = temp.path().join("memory");
    crate::prompts::ensure_memory_store_layout_at(&memory).unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let a = MockLlmProvider::new().with_responses(vec![
        mock_generation_response("Noted."),
        mock_generation_response("{\"writes\":[]}"),
    ]);
    let b = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection(
        "A",
        Box::new(GatedPromotion {
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
    core.memory.enabled = true;
    core.memory.store_dir = Some(memory.clone());
    core.streaming_mode = crate::config::StreamingMode::Off;
    // Promotion requests intentionally have their own default controls, not turn overrides.
    core.model_reasoning_effort = None;
    let make = |model: &str| CapturedCallable {
        identity: CallableIdentity {
            profile: "managed".into(),
            provider: "openai_responses".into(),
            model: model.into(),
            credential_ref: Some("PRIVATE_REF".into()),
            revision: "PRIVATE_REV".into(),
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
            memory_store_bound: true,
            memory_store_backing: Some(memory),
            store_bindings: Some(stores),
            ..Default::default()
        },
        env,
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let metadata = runtime.wait_until_ready().await.unwrap();
    let input = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("My name is Morris.")],
        context: None,
    });
    let selection = Submission::new(Op::SelectModel { model: "B".into() });
    let result: anyhow::Result<()> = async {
        runtime.handle.submission_tx.send(input.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), started.notified()).await?;
        completed(&shell, &input.id).await?;
        runtime.handle.submission_tx.send(selection.clone()).await?;
        completed(&shell, &selection.id).await?;
        let snapshot: alan_agent_protocol::UiModelSnapshot =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/models").await?)?;
        anyhow::ensure!(
            snapshot.selected_next.as_ref().map(|v| v.model.as_str()) == Some("B"),
            "next B: {snapshot:?}"
        );
        anyhow::ensure!(
            snapshot.active.as_ref().map(|v| v.model.as_str()) == Some("A"),
            "actual deferred A absent: {snapshot:?}"
        );
        anyhow::ensure!(
            snapshot.active.as_ref().unwrap().reasoning == a.recorded_requests()[1].reasoning,
            "deferred request controls differ"
        );
        anyhow::ensure!(
            snapshot.admitted.is_empty(),
            "settled input must not be re-admitted"
        );
        anyhow::ensure!(
            a.recorded_requests().len() == 2 && b.recorded_requests().is_empty(),
            "routing changed"
        );
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let idle: alan_agent_protocol::UiModelSnapshot =
                    serde_json::from_slice(&shell.cat("/agent/1/machine/ui/models").await?)?;
                if idle.active.is_none() && idle.publication_version > snapshot.publication_version
                {
                    anyhow::ensure!(idle.selected_next.as_ref().unwrap().model == "B");
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;
        Ok(())
    }
    .await;
    release.notify_one();
    runtime.shutdown().await.unwrap();
    result.unwrap();
    assert_eq!(a.recorded_requests().len(), 2);
    assert!(b.recorded_requests().is_empty());
    let history = crate::rollout::RolloutRecorder::load_history(&metadata.rollout_path.unwrap())
        .await
        .unwrap();
    assert_eq!(history.iter().filter(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1" && e.payload["submission_id"] == input.id)).count(), 1);
}
