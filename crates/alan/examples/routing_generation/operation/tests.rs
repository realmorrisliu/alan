use super::*;
use alan_ap::{ErrorCode, FileKind, FileServer, Offset, Qid, Stat};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

#[test]
fn advice_requires_a_clean_terminal_result_not_just_label_text() {
    let done = json!({"status":"done"});
    let clean = json!([{"version":1,"text":"command"},{"version":1,"done":true}]);
    assert_eq!(classify("command", &clean, &done), "success");
    assert_eq!(
        classify("command", &clean, &json!({"status":"error"})),
        "unavailable"
    );
    assert_eq!(
        classify(
            "command",
            &json!([{"version":1,"tool_call":{}},{"version":1,"done":true}]),
            &done
        ),
        "malformed"
    );
    assert_eq!(
        classify("command", &json!([{"version":1,"error":"failed"}]), &done),
        "malformed"
    );
    assert_eq!(classify("command then execute", &clean, &done), "malformed");
}

struct PendingStatusFs {
    inner: alan_llmfs::LlmFs,
    pending: Mutex<std::collections::HashSet<Fid>>,
    reads: AtomicUsize,
    hold_all: bool,
}

#[async_trait::async_trait]
impl FileServer for PendingStatusFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        if names.last().is_some_and(|name| name == "status") {
            let first = self.reads.fetch_add(1, Ordering::SeqCst) == 0;
            if first || self.hold_all {
                self.pending.lock().unwrap().insert(newfid);
            }
        }
        Ok(qid)
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if self.pending.lock().unwrap().contains(&fid) {
            let bytes = br#"{"status":"running"}"#;
            let start = (offset as usize).min(bytes.len());
            return Ok(bytes[start..(start + count as usize).min(bytes.len())].to_vec());
        }
        self.inner.read(fid, offset, count).await
    }
    async fn write(&self, fid: Fid, offset: Offset, bytes: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, bytes).await
    }
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid).await
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.pending.lock().unwrap().remove(&fid);
        self.inner.clunk(fid).await
    }
}

struct FinishedProvider(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl alan_llm::LlmProvider for FinishedProvider {
    fn provider_name(&self) -> &'static str {
        "chatgpt"
    }
    async fn generate_stream(
        &mut self,
        _: alan_llm::GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        sender
            .send(alan_llm::StreamChunk {
                text: Some("agent".into()),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: None,
                provider_response_id: None,
                provider_response_status: None,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: None,
            })
            .await
            .unwrap();
        Ok(receiver)
    }
}

#[tokio::test]
async fn terminal_event_waits_for_status_publication_within_the_caller_deadline() {
    for hold_all in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = alan_llmfs::LlmFs::new();
        inner.register_connection("generation", Box::new(FinishedProvider(calls.clone())));
        let server = Arc::new(PendingStatusFs {
            inner,
            pending: Mutex::default(),
            reads: AtomicUsize::new(0),
            hold_all,
        });
        let root = InProcessTransport::new(server.clone());
        let shell = alan_shell::Shell::new(root.clone());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("receipt.json");
        std::fs::write(&path, b"{}").unwrap();
        let mut attempt = Attempt::default();
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            generate(
                &root,
                &shell,
                "/connections/generation",
                &advice_body("data", "advice"),
                0,
                &mut attempt,
                &path,
            ),
        )
        .await;
        if hold_all {
            assert!(
                result.is_err(),
                "unsettled status must respect the original deadline"
            );
        } else {
            result.unwrap().unwrap();
            assert_eq!(
                classify(&attempt.text, &json!(attempt.events), &attempt.status),
                "success"
            );
        }
        assert!(server.reads.load(Ordering::SeqCst) >= 2);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(
            attempt
                .events
                .last()
                .is_some_and(|event| event["done"] == true)
        );
        let cleanup = settle_attempt(&root, &shell, &mut attempt).await;
        assert_eq!(cleanup["abort"], "already_terminal");
        assert_eq!(cleanup["tail_closed"], true);
    }
}
