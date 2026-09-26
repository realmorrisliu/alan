use super::*;
use alan_agent_engine::{AgentRuntimeStoreBindings, InputIntent, InputMode, UserInputRecord};
use alan_shell::Shell;
use serde_json::Value;

#[derive(Debug)]
struct WaitingProvider;

#[async_trait::async_trait]
impl LlmProvider for WaitingProvider {
    async fn generate(
        &mut self,
        _: alan_llm::GenerationRequest,
    ) -> Result<alan_llm::GenerationResponse> {
        std::future::pending().await
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> Result<String> {
        std::future::pending().await
    }
    async fn generate_stream(
        &mut self,
        _: alan_llm::GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        std::future::pending().await
    }
    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

async fn boot(stores: AgentRuntimeStoreBindings, client: LlmClient) -> ServiceManager {
    ServiceManager::boot(ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig {
            store_bindings: Some(stores),
            ..AgentProcessConfig::default()
        },
        ProcessLaunchContext::root(),
        client,
        ToolRegistry::new(),
    ))
    .await
    .unwrap()
}

async fn activity(shell: &Shell) -> Value {
    serde_json::from_slice(&shell.cat("/agent/root/machine/ui/activity").await.unwrap()).unwrap()
}

async fn submit(shell: &Shell, body: &str) -> String {
    let record = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, body);
    shell
        .write("/agent/root/io/input", &record.encode_payload().unwrap())
        .await
        .unwrap();
    record.submission_id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn root_and_host_restart_keep_pending_input_paused() {
    let dir = tempfile::tempdir().unwrap();
    let stores = AgentRuntimeStoreBindings {
        rollouts: dir.path().join("rollouts"),
        checkpoints: dir.path().join("checkpoints"),
        cache: dir.path().join("cache"),
        tmp: dir.path().join("tmp"),
        metadata: dir.path().join("metadata"),
    };
    let manager = boot(stores.clone(), LlmClient::new(WaitingProvider)).await;
    let shell = Shell::new(manager.root_namespace.clone());
    let active = submit(&shell, "never complete this turn").await;
    let pending = submit(&shell, "keep this follow-up queued").await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = activity(&shell).await;
            if snapshot["active_submission"]["submission_id"] == active
                && snapshot["pending_submissions"][0]["submission_id"] == pending
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let before = std::fs::read(stores.metadata.join("root-rollout.json")).unwrap();
    let old_pid = manager.root_pid();
    manager.terminate_unit("root-agent", 1).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while manager.root_pid() == old_pid || manager.root_pid().0 == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = activity(&shell).await;
    assert_eq!(snapshot["queue_paused"], true);
    assert!(snapshot["active_submission"].is_null());
    assert_eq!(snapshot["pending_submissions"][0]["submission_id"], pending);
    assert_ne!(
        std::fs::read(stores.metadata.join("root-rollout.json")).unwrap(),
        before
    );
    manager.shutdown().await.unwrap();

    let provider = MockLlmProvider::new();
    let probe = provider.clone();
    let manager = boot(stores, LlmClient::new(provider)).await;
    let shell = Shell::new(manager.root_namespace.clone());
    let snapshot = activity(&shell).await;
    assert_eq!(snapshot["queue_paused"], true);
    assert!(snapshot["active_submission"].is_null());
    assert_eq!(snapshot["pending_submissions"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["pending_submissions"][0]["submission_id"], pending);
    assert!(probe.recorded_requests().is_empty());
    shell
        .write("/agent/root/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = activity(&shell).await;
            if snapshot["queue_paused"] == false
                && snapshot["state"] == "idle"
                && snapshot["pending_submissions"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        probe.recorded_requests().len(),
        1,
        "active input must not replay"
    );
    manager.shutdown().await.unwrap();
}
