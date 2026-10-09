use super::*;
#[test]
fn usage_is_unknown_when_missing_and_invalid_counts_fail() {
    assert!(usage(&[]).unwrap().is_none());
    assert!(usage(&[json!({"usage":{"prompt_tokens":-1}})]).is_err());
    let u = json!({"usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6}});
    assert_eq!(usage(&[u]).unwrap().unwrap().total_tokens, 6);
}

struct WaitingProvider {
    calls: Arc<std::sync::atomic::AtomicUsize>,
    sender: Option<tokio::sync::mpsc::Sender<alan_llm::StreamChunk>>,
}
#[async_trait::async_trait]
impl LlmProvider for WaitingProvider {
    fn provider_name(&self) -> &'static str {
        "chatgpt"
    }
    async fn generate_stream(
        &mut self,
        _: alan_llm::GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        self.sender = Some(sender);
        Ok(receiver)
    }
}

#[tokio::test]
async fn dropping_outer_choice_aborts_nested_generation_and_retains_cleanup_receipt() {
    use alan_kernel::{Access, MountFs, Namespace};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let llmfs = alan_llmfs::LlmFs::new();
    llmfs.register_connection_profile(
        "generation",
        alan_llmfs::ConnectionProfile::new("chatgpt", "gpt-6.1-sol", "fixture"),
        Box::new(WaitingProvider {
            calls: calls.clone(),
            sender: None,
        }),
    );
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(llmfs)),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let temp = tempfile::tempdir().unwrap();
    let receipt = temp.path().join("generation.json");
    let job = GenerationJob::default();
    let mut advice = GenerationAdvice {
        root: root.clone(),
        connection_path: "/mnt/llm/connections/generation".into(),
        receipt: receipt.clone(),
        job: job.clone(),
    };
    let request = ChoiceEvaluationRequest {
        input: "literal data".into(),
        candidates: vec![alan_llm::EvaluationCandidate {
            id: "agent".into(),
            description: "Agent advice".into(),
        }],
    };
    let outer = tokio::spawn(async move { advice.evaluate_choice(request).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let record = std::fs::read(&receipt)
                .ok()
                .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok());
            if record.is_some_and(|record| record["events_tail_opened"] == true) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    outer.abort();
    assert!(outer.await.unwrap_err().is_cancelled());
    let task = job.lock().unwrap().take().unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let record: Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
    assert_eq!(record["outcome"], "unavailable");
    assert_eq!(record["cleanup"]["abort"], "acknowledged");
    assert_eq!(record["cleanup"]["tail_closed"], true);
    assert_eq!(
        record["cleanup"]["operation_status_after_cleanup"]["status"],
        "aborted"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(record["cost_microusd"].is_null());
}
