use super::*;
use alan_agent_engine::tools::{Tool, ToolContext, ToolResult};
use alan_agent_protocol::ToolCapability;
use alan_llm::{GenerationResponse, ToolCall};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct Factory {
    main: MockLlmProvider,
    reviewer: MockLlmProvider,
}

impl LlmClientFactory for Factory {
    fn create(
        &self,
        _: &alan_agent_engine::Config,
        profile: Option<&str>,
        _: &ConnectionsFile,
    ) -> Result<LlmClient> {
        match profile {
            Some("main") => Ok(LlmClient::new(self.main.clone())),
            Some("reviewer") => Ok(LlmClient::new(self.reviewer.clone())),
            _ => anyhow::bail!("unexpected fixture profile"),
        }
    }
}

struct CountingTool(Arc<AtomicUsize>);

impl Tool for CountingTool {
    fn name(&self) -> &str {
        "fixture_effect"
    }
    fn description(&self) -> &str {
        "A counted fixture effect"
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{}})
    }
    fn capability(&self, _: &Value) -> ToolCapability {
        ToolCapability::Unknown
    }
    fn execute(&self, _: Value, _: &ToolContext) -> ToolResult {
        let count = self.0.clone();
        Box::pin(async move {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"success":true}))
        })
    }
}

fn response(content: &str, tool: bool) -> GenerationResponse {
    GenerationResponse {
        content: content.into(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: vec![],
        tool_calls: if tool {
            vec![ToolCall {
                id: Some("fixture-call".into()),
                name: "fixture_effect".into(),
                arguments: json!({}),
            }]
        } else {
            vec![]
        },
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: vec![],
    }
}

#[tokio::test]
async fn guardian_shadow_boot_is_explicit_and_never_controls_real_effects() {
    for (enabled, main_decision, shadow_decision, effects) in [
        (true, "allow", "deny", 1),
        (true, "deny", "allow", 0),
        (false, "allow", "deny", 1),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let metadata = temp.path().join("connections.toml");
        let connections: ConnectionsFile = serde_json::from_value(json!({
            "version":1,"default_profile":"main",
            "profiles":{
                "main":{"provider":"openai_responses","credential_id":"key","created_at":"2026-10-23T00:00:00Z","updated_at":"2026-10-23T00:00:00Z","source":"managed","settings":{"model":"gpt-5.4"}},
                "reviewer":{"provider":"openai_responses","credential_id":"key","created_at":"2026-10-23T00:00:00Z","updated_at":"2026-10-23T00:00:00Z","source":"managed","settings":{"model":"gpt-6.1-sol"}}
            },
            "credentials":{"key":{"kind":"secret_string","provider_family":"openai_responses","label":"fixture","backend":"host_credential_store"}}
        })).unwrap();
        connections.save_to_path(&metadata).unwrap();
        let main = MockLlmProvider::new().with_responses(vec![
            response("", true),
            response(&json!({"decision":main_decision}).to_string(), false),
            response("fixture finished", false),
        ]);
        let reviewer = MockLlmProvider::new().with_response(response(
            &json!({"decision":shadow_decision}).to_string(),
            false,
        ));
        let counter = Arc::new(AtomicUsize::new(0));
        let mut tools = ToolRegistry::new();
        tools.register(CountingTool(counter.clone()));
        let mut config = ServiceManagerConfig::ephemeral(
            AgentProcessConfig::default(),
            ProcessLaunchContext::root(),
            LlmClient::new(main.clone()),
            tools,
        );
        config.process.agent_config.core_config.memory.enabled = false;
        config.connection_store = Some(ConnectionStoreBindings::new(metadata.clone()).unwrap());
        config.llm_factory = Arc::new(Factory {
            main: main.clone(),
            reviewer: reviewer.clone(),
        });
        if enabled {
            config.guardian_shadow = Some(GuardianShadowSelection {
                profile: "reviewer".into(),
                policy: Some("CANDIDATE-ONLY policy".into()),
            });
        }
        let rollouts = temp.path().join("rollouts");
        config.process.store_bindings = Some(alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: rollouts.clone(),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
            metadata: temp.path().join("runtime-metadata"),
        });
        let before = std::fs::read(&metadata).unwrap();
        let manager = ServiceManager::boot(config).await.unwrap();
        let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
        let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
        let record = alan_agent_protocol::UserInputRecord::new(
            alan_agent_protocol::InputIntent::Agent,
            alan_agent_protocol::InputMode::FollowUp,
            "do the fixture task",
        );
        shell
            .write("/agent/root/io/input", &record.encode_payload().unwrap())
            .await
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let output = shell.cat("/agent/root/io/output").await.unwrap();
                if String::from_utf8_lossy(&output).contains("fixture finished") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        let selected = manager.connection().selected_profile(manager.root_pid().0);
        manager.shutdown().await.unwrap();
        result.expect("Root fixture did not finish");
        assert_eq!(selected.as_deref(), Some("main"));
        assert_eq!(std::fs::read(&metadata).unwrap(), before);
        assert_eq!(counter.load(Ordering::SeqCst), effects);
        assert_eq!(main.recorded_requests().len(), 3);
        assert_eq!(reviewer.recorded_requests().len(), usize::from(enabled));
        let mut observations = Vec::new();
        for file in std::fs::read_dir(&rollouts).unwrap() {
            let file = file.unwrap().path();
            if file.extension().is_some_and(|ext| ext == "jsonl") {
                for item in alan_agent_engine::RolloutRecorder::load_history(&file)
                    .await
                    .unwrap()
                {
                    if let alan_agent_engine::RolloutItem::Event(event) = item
                        && event.event_type == "guardian_shadow_v1"
                    {
                        observations.push(event.payload);
                    }
                }
            }
        }
        assert_eq!(observations.len(), if enabled { 2 } else { 0 });
        if enabled {
            assert_eq!(
                observations[1]["identity"]["baseline_binding"]["profile"],
                "main"
            );
            assert_eq!(
                observations[1]["identity"]["reviewer"]["profile"],
                "reviewer"
            );
            assert_eq!(
                observations[1]["stage"]["candidate"]["decision"],
                shadow_decision
            );
            assert_eq!(observations[1]["baseline"]["decision"], main_decision);
        }
    }
}
