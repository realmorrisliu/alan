use super::*;
use alan_agent_engine::runtime::EvaluationSurface;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct Evaluator(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl LlmProvider for Evaluator {
    fn provider_name(&self) -> &'static str {
        "typesafe"
    }
    fn supports_generation(&self) -> bool {
        false
    }
    fn supports_choice_evaluation(&self) -> bool {
        true
    }
    async fn evaluate_choice(
        &mut self,
        request: alan_llm::ChoiceEvaluationRequest,
    ) -> Result<alan_llm::ChoiceEvaluationResponse> {
        assert_eq!(request.input, "pwd");
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(alan_llm::ChoiceEvaluationResponse {
            selection: alan_llm::EvaluationSelection::Selected("command".into()),
            usage: None,
        })
    }
}

#[derive(Debug)]
struct Factory {
    calls: Arc<AtomicUsize>,
    generation: MockLlmProvider,
}
impl LlmClientFactory for Factory {
    fn create(
        &self,
        _: &alan_agent_engine::Config,
        selected: Option<&str>,
        _: &ConnectionsFile,
    ) -> Result<LlmClient> {
        match selected {
            Some("evaluation") => Ok(LlmClient::new(Evaluator(self.calls.clone()))),
            Some("main") => Ok(LlmClient::new(self.generation.clone())),
            _ => anyhow::bail!("unknown test profile"),
        }
    }
}

#[tokio::test]
async fn explicit_root_evaluator_publishes_advice_without_replacing_generation() {
    for surface in [
        EvaluationSurface::Interactive,
        EvaluationSurface::Redirected,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let metadata = temp.path().join("connections.toml");
        let connections: ConnectionsFile = serde_json::from_value(serde_json::json!({
            "version":1,"default_profile":"main",
            "profiles":{
                "main":{"provider":"openai_responses","credential_id":"main-key","created_at":"2026-10-07T00:00:00Z","updated_at":"2026-10-07T00:00:00Z","source":"managed","settings":{"model":"gpt-5.4"}},
                "evaluation":{"provider":"typesafe","credential_id":"eval-key","created_at":"2026-10-07T00:00:00Z","updated_at":"2026-10-07T00:00:00Z","source":"managed","settings":{"model":"jev-1.13.0"}}
            },
            "credentials":{
                "main-key":{"kind":"secret_string","provider_family":"openai_responses","label":"fixture","backend":"host_credential_store"},
                "eval-key":{"kind":"secret_string","provider_family":"typesafe","label":"fixture","backend":"host_credential_store"}
            }
        })).unwrap();
        connections.save_to_path(&metadata).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let generation = MockLlmProvider::new();
        let mut config = ServiceManagerConfig::ephemeral(
            "test",
            AgentProcessConfig::default(),
            ProcessLaunchContext::root(),
            LlmClient::new(generation.clone()),
            ToolRegistry::new(),
        );
        config.process.agent_config.core_config.memory.enabled = false;
        config.connection_store = Some(ConnectionStoreBindings::new(metadata).unwrap());
        config.llm_factory = Arc::new(Factory {
            calls: calls.clone(),
            generation: generation.clone(),
        });
        config.input_shadow = Some(InputShadowSelection {
            profile: "evaluation".into(),
            surface: surface.clone(),
        });
        config.process.store_bindings = Some(alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
            metadata: temp.path().join("runtime-metadata"),
        });
        let manager = ServiceManager::boot(config).await.unwrap();
        let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
        let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
        let record = alan_agent_protocol::UserInputRecord::new(
            alan_agent_protocol::InputIntent::Agent,
            alan_agent_protocol::InputMode::FollowUp,
            "pwd",
        );
        shell
            .write("/agent/root/io/input", &record.encode_payload().unwrap())
            .await
            .unwrap();
        let observation = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot: serde_json::Value = serde_json::from_slice(
                    &shell.cat("/agent/root/machine/evaluation").await.unwrap(),
                )
                .unwrap();
                if snapshot["observation"]["outcome"]["state"] == "selected"
                    && !generation.recorded_requests().is_empty()
                {
                    break snapshot["observation"].clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        let profile = manager.connection().selected_profile(manager.root_pid().0);
        let default_profile = manager.connection().default_profile();
        manager.shutdown().await.unwrap();
        let observation = observation.expect("Root evaluation was not published");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            observation["identity"]["submission_id"],
            record.submission_id
        );
        assert_eq!(
            observation["identity"]["surface"],
            serde_json::to_value(surface).unwrap()
        );
        assert_eq!(observation["identity"]["callable"]["profile"], "evaluation");
        assert_eq!(observation["outcome"]["candidate_id"], "command");
        assert_eq!(profile.as_deref(), Some("main"));
        assert_eq!(default_profile.as_deref(), Some("main"));
    }
}
