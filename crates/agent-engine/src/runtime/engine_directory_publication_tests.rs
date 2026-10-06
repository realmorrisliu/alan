//! Faults traverse real AgentFS descriptors and the running Runtime loop.
use super::*;
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, OpenMode, Qid, Stat};
use futures::FutureExt;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub(super) struct CountedSelectionAuthority(pub(super) Arc<std::sync::atomic::AtomicUsize>);
impl ToolExecutionAuthority for CountedSelectionAuthority {
    fn reconcile(
        &self,
        pid: u64,
        binding: ToolExecutionBinding,
    ) -> anyhow::Result<ToolExecutionBinding> {
        self.0.fetch_add(1, Ordering::SeqCst);
        SelectionAuthority.reconcile(pid, binding)
    }
}

pub(super) struct ActionFaultFs {
    inner: alan_agentfs::AgentFs,
    field: Option<String>,
    paths: Mutex<HashSet<u64>>,
    fail: AtomicBool,
    lost_ack: bool,
    fail_status_read: AtomicBool,
    failures: std::sync::atomic::AtomicUsize,
    terminal_writes: std::sync::atomic::AtomicUsize,
    clunk_fault: bool,
    permanent: AtomicBool,
}
impl ActionFaultFs {
    fn release(&self) {
        self.permanent.store(false, Ordering::SeqCst);
        self.fail.store(false, Ordering::SeqCst);
    }
    pub(super) fn failures(&self) -> usize {
        self.failures.load(Ordering::SeqCst)
    }
    pub(super) fn terminal_writes(&self) -> usize {
        self.terminal_writes.load(Ordering::SeqCst)
    }
    pub(super) fn new(field: Option<&str>) -> Self {
        Self {
            inner: alan_agentfs::AgentFs::new(),
            field: field.map(|f| {
                match f {
                    "status-lost-ack" => "status",
                    "output-clunk" => "output",
                    "clone-clunk" => "clone",
                    "result-permanent" => "result",
                    f => f,
                }
                .to_owned()
            }),
            paths: Default::default(),
            fail: AtomicBool::new(field.is_some()),
            lost_ack: field == Some("status-lost-ack"),
            fail_status_read: AtomicBool::new(false),
            failures: Default::default(),
            terminal_writes: Default::default(),
            clunk_fault: matches!(field, Some("output-clunk" | "clone-clunk")),
            permanent: AtomicBool::new(field == Some("result-permanent")),
        }
    }
}
#[async_trait]
impl FileServer for ActionFaultFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        if names.last() == self.field.as_ref() && names.iter().any(|n| n == "actions") {
            self.paths.lock().unwrap().insert(qid.path);
        }
        Ok(qid)
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let path = self.inner.stat(fid).await?.qid.path;
        let targeted = self.paths.lock().unwrap().contains(&path);
        if targeted && self.fail_status_read.swap(false, Ordering::SeqCst) {
            return Err(ErrorCode::Io);
        }
        self.inner.read(fid, offset, count).await
    }
    async fn write(&self, fid: Fid, offset: u64, data: &[u8]) -> Result<u32, ErrorCode> {
        let path = self.inner.stat(fid).await?.qid.path;
        let targeted = self.paths.lock().unwrap().contains(&path);
        if targeted
            && !self.clunk_fault
            && (self.permanent.load(Ordering::SeqCst) || self.fail.swap(false, Ordering::SeqCst))
        {
            self.failures.fetch_add(1, Ordering::SeqCst);
            if self.lost_ack {
                self.inner.write(fid, offset, data).await?;
                self.terminal_writes.fetch_add(1, Ordering::SeqCst);
                self.fail_status_read.store(true, Ordering::SeqCst);
            }
            return Err(ErrorCode::Io);
        }
        let written = self.inner.write(fid, offset, data).await?;
        if targeted && self.field.as_deref() == Some("status") {
            self.terminal_writes.fetch_add(1, Ordering::SeqCst);
        }
        Ok(written)
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
        let path = self.inner.stat(fid).await?.qid.path;
        let targeted = self.paths.lock().unwrap().contains(&path);
        self.inner.clunk(fid).await?;
        if targeted && self.clunk_fault && self.fail.swap(false, Ordering::SeqCst) {
            self.failures.fetch_add(1, Ordering::SeqCst);
            return Err(ErrorCode::Io);
        }
        Ok(())
    }
}

#[tokio::test]
async fn directory_result_write_failure_retains_executed_outcome() {
    directory_selection_preserves_recovered_paused_work_until_continue(true, Some("result")).await;
}
#[tokio::test]
async fn directory_terminal_write_failure_reuses_action_identity() {
    directory_selection_preserves_recovered_paused_work_until_continue(true, Some("status")).await;
}

#[tokio::test]
async fn directory_terminal_lost_ack_and_read_failure_do_not_republish() {
    directory_selection_preserves_recovered_paused_work_until_continue(
        true,
        Some("status-lost-ack"),
    )
    .await;
}
#[tokio::test]
async fn directory_active_rejection_write_failure_retains_failed_action() {
    directory_selection_rejects_running(Some("result")).await;
}

#[tokio::test]
async fn directory_output_clunk_lost_ack_keeps_immutable_output() {
    directory_selection_preserves_recovered_paused_work_until_continue(true, Some("output-clunk"))
        .await;
}
#[tokio::test]
async fn directory_clone_clunk_lost_ack_keeps_read_action_id() {
    directory_selection_preserves_recovered_paused_work_until_continue(true, Some("clone-clunk"))
        .await;
}

#[tokio::test]
async fn directory_unpublished_rejection_does_not_block_request_interrupt() {
    pending_request_boundary(true).await;
}
#[tokio::test]
async fn directory_unpublished_rejection_does_not_block_namespace_answer() {
    pending_request_boundary(false).await;
}

async fn pending_request_boundary(interrupt: bool) {
    let temp = TempDir::new().unwrap();
    let mut response = mock_generation_response("");
    response.tool_calls = vec![alan_llm::ToolCall {
        id: Some("actual-question".into()),
        name: "request_user_input".into(),
        arguments: serde_json::json!({"title":"Question","prompt":"Enter value",
            "questions":[{"id":"q1","label":"Q1","prompt":"What?"}]}),
    }];
    let mock =
        MockLlmProvider::new().with_responses(vec![response, mock_generation_response("answered")]);
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let fault = Arc::new(ActionFaultFs::new(Some("result-permanent")));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/2",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            store_bindings: Some(crate::AgentRuntimeStoreBindings {
                rollouts: temp.path().join("rollouts"),
                checkpoints: temp.path().join("checkpoints"),
                cache: temp.path().join("cache"),
                tmp: temp.path().join("tmp"),
                metadata: temp.path().join("metadata"),
            }),
            ..Default::default()
        },
        NamespaceRuntimeEnvironment::new(root, "/agent/2", "default"),
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    runtime.wait_until_ready().await.unwrap();
    let original = Submission::new(Op::Input {
        parts: vec![ContentPart::text("ask")],
        mode: InputMode::FollowUp,
    });
    runtime
        .handle
        .submission_tx
        .send(original.clone())
        .await
        .unwrap();
    let observed = tokio::time::timeout(Duration::from_secs(5), std::panic::AssertUnwindSafe(async {
        loop {
            let activity: alan_agent_protocol::UiActivitySnapshot = serde_json::from_slice(
                &shell.cat("/agent/2/machine/ui/activity").await.unwrap()).unwrap();
            if activity.state == alan_agent_protocol::UiActivityState::Paused { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        runtime.handle.submission_tx.send(Submission::new(Op::SelectProjectDirectory {path:"/mnt/new".into()})).await.unwrap();
        while fault.failures() < 2 { tokio::time::sleep(Duration::from_millis(10)).await; }
        if interrupt {
            runtime.handle.submission_tx.send(Submission::new(Op::Interrupt)).await.unwrap();
            loop {
                let events = shell.cat("/agent/2/machine/ui/events").await.unwrap();
                if String::from_utf8(events).unwrap().lines().any(|line| matches!(
                    serde_json::from_str::<alan_agent_protocol::UiEvent>(line),
                    Ok(alan_agent_protocol::UiEvent::InputCompleted {submission_ids,status:alan_agent_protocol::UiInputStatus::Cancelled,..})
                    if submission_ids.contains(&original.id))) { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        } else {
            shell.write("/agent/2/requests/r0/response", br#"{"answers":[{"question_id":"q1","value":"from file"}]}"#).await.unwrap();
            while mock.recorded_requests().len() < 2 { tokio::time::sleep(Duration::from_millis(10)).await; }
            let next = Submission::new(Op::Input {
                parts: vec![ContentPart::text("after answer")], mode: InputMode::FollowUp,
            });
            runtime.handle.submission_tx.send(next.clone()).await.unwrap();
            loop {
                let queue: alan_agent_protocol::UiQueueSnapshot = serde_json::from_slice(
                    &shell.cat("/agent/2/machine/ui/queue").await.unwrap()).unwrap();
                if queue.pending_submission_ids.contains(&next.id) && queue.active_submission_ids.is_empty() { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert_eq!(mock.recorded_requests().len(), 2, "ordinary task must not dispatch before retained directory outcome is observable");
            fault.release();
            while mock.recorded_requests().len() < 3 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

        }
    }).catch_unwind()).await;
    let stopped = tokio::time::timeout(Duration::from_secs(5), runtime.shutdown()).await;
    assert!(
        stopped.is_ok(),
        "unpublished directory result must not prevent shutdown"
    );
    stopped.unwrap().unwrap();
    match observed {
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(error) => panic!("directory publication request boundary timed out: {error}"),
        Ok(Ok(())) => {}
    }
    assert_eq!(
        mock.recorded_requests().len(),
        if interrupt { 1 } else { 3 }
    );
    assert!(
        fault.failures() >= 2,
        "permanent real Action fault remains retained without effect replay"
    );
}
