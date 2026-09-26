use super::*;

struct GatedFirstGeneration {
    mock: MockLlmProvider,
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl LlmProvider for GatedFirstGeneration {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        let first = self.mock.recorded_requests().is_empty();
        let response = self.mock.generate(request).await?;
        if first {
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

    async fn chat(&mut self, system: Option<&str>, user: &str) -> anyhow::Result<String> {
        self.mock.chat(system, user).await
    }

    fn provider_name(&self) -> &'static str {
        "gated_first_generation"
    }
}

#[tokio::test]
async fn ordinary_follow_up_does_not_overtake_an_older_outer_submission() {
    let mock = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection(
        "default",
        Box::new(GatedFirstGeneration {
            mock: mock.clone(),
            started: started.clone(),
            release: release.clone(),
        }),
    );
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core_config =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core_config.memory.enabled = false;
    core_config.streaming_mode = crate::config::StreamingMode::Off;
    let config = AgentProcessConfig {
        agent_config: crate::AgentConfig::from(core_config),
        ..AgentProcessConfig::default()
    };
    let capabilities = crate::provider_capabilities_for_config(&config.agent_config.core_config);
    let mut controller = spawn_with_namespace_environment(
        config,
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    controller.wait_until_ready().await.unwrap();
    let tx = &controller.handle.submission_tx;
    let capacity = tx.capacity();
    tx.send(Submission::new(Op::Turn {
        parts: vec![ContentPart::text("first")],
        context: None,
    }))
    .await
    .unwrap();
    let begun = tokio::time::timeout(Duration::from_secs(5), started.notified()).await;
    assert!(
        begun.is_ok(),
        "{:?}",
        String::from_utf8(shell.cat("/agent/1/machine/ui/notice").await.unwrap())
    );
    tx.send(Submission::new(Op::Turn {
        parts: vec![ContentPart::text("second")],
        context: None,
    }))
    .await
    .unwrap();
    tx.send(Submission::new(Op::Input {
        parts: vec![ContentPart::text("third")],
        mode: InputMode::FollowUp,
    }))
    .await
    .unwrap();
    // Wait until both later submissions have been consumed by the input pump.
    tokio::time::timeout(Duration::from_secs(5), async {
        while tx.capacity() != capacity {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.recorded_requests().len() < 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let order = mock
        .recorded_requests()
        .iter()
        .map(|request| {
            request
                .messages
                .iter()
                .rev()
                .find(|message| message.role == alan_llm::MessageRole::User)
                .unwrap()
                .content
                .clone()
        })
        .collect::<Vec<_>>();
    controller.shutdown().await.unwrap();
    assert_eq!(order, ["first", "second", "third"]);
}
