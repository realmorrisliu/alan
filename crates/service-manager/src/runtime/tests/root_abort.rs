use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Notify;

#[derive(Debug, Default)]
struct StreamGate {
    entered: Notify,
    release: Notify,
    settled: AtomicBool,
    calls: AtomicUsize,
}

#[derive(Debug)]
struct StreamingProvider(Arc<StreamGate>);

#[async_trait::async_trait]
impl LlmProvider for StreamingProvider {
    async fn generate_stream(
        &mut self,
        _: alan_llm::GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        self.0.calls.fetch_add(1, Ordering::SeqCst);
        let gate = self.0.clone();
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            for _ in 0..2 {
                if sender
                    .send(alan_llm::StreamChunk {
                        text: Some("still working".into()),
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            gate.entered.notify_one(); // The bounded channel proves the drain is running.
            tokio::select! { _ = sender.closed() => {}, _ = gate.release.notified() => {} }
            gate.settled.store(true, Ordering::SeqCst);
        });
        Ok(receiver)
    }
    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

async fn boot(
    provider: impl LlmProvider + 'static,
    tools: ToolRegistry,
) -> (tempfile::TempDir, ServiceManager) {
    let temp = tempfile::tempdir().unwrap();
    let stores = alan_agent_engine::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        metadata: temp.path().join("metadata"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
    };
    std::fs::create_dir_all(&stores.tmp).unwrap();
    let mut process = AgentProcessConfig {
        store_bindings: Some(stores),
        ..Default::default()
    };
    process.agent_config.core_config.memory.enabled = false;
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        process,
        ProcessLaunchContext::root(),
        LlmClient::new(provider),
        tools,
    ))
    .await
    .unwrap();
    (temp, manager)
}

async fn settled(flag: &AtomicBool) -> bool {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !flag.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok()
}

#[tokio::test]
async fn root_abort_stops_its_own_streaming_model_operation() {
    let gate = Arc::new(StreamGate::default());
    let (_temp, manager) = boot(StreamingProvider(gate.clone()), ToolRegistry::new()).await;
    let shell = alan_shell::Shell::new(manager.root_namespace.clone());
    shell
        .write("/agent/root/io/input", b"hold the model stream")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), gate.entered.notified())
        .await
        .unwrap();
    let pid = manager.root_pid();
    shell
        .write(&format!("/proc/{}/ctl", pid.0), b"cancel")
        .await
        .unwrap();
    let stopped = settled(&gate.settled).await;
    let calls = gate.calls.load(Ordering::SeqCst);
    gate.release.notify_one();
    manager.shutdown().await.unwrap();
    assert!(stopped, "Root exit must stop its live stream drain");
    assert_eq!(calls, 1, "Root exit must not retry the model operation");
}

#[tokio::test]
async fn root_abort_stops_its_own_tool_process() {
    let (provider, tools, entered, release, cleaned) = child_budget::admitted_parent_fixture();
    let (_temp, manager) = boot(provider, tools).await;
    let shell = alan_shell::Shell::new(manager.root_namespace.clone());
    shell
        .write("/agent/root/io/input", b"hold the tool")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    let pid = manager.root_pid();
    shell
        .write(&format!("/proc/{}/ctl", pid.0), b"cancel")
        .await
        .unwrap();
    let stopped = settled(&cleaned).await;
    release.notify_one();
    manager.shutdown().await.unwrap();
    assert!(stopped, "Root exit must stop its admitted Tool Process");
}
