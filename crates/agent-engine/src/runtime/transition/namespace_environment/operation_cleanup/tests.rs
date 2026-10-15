use super::*;
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Qid, Stat};
use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;

struct ControlFs {
    inner: alan_agentfs::AgentFs,
    entered: Notify,
    release: Notify,
    walks: AtomicUsize,
    fail: bool,
    panic: bool,
}

#[async_trait::async_trait]
impl FileServer for ControlFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.walks.fetch_add(1, Ordering::SeqCst);
        self.inner.walk(fid, newfid, names).await
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn write(&self, fid: Fid, offset: u64, data: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data).await
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
        self.inner.clunk(fid).await?;
        self.entered.notify_one();
        self.release.notified().await;
        assert!(!self.panic, "test cleanup server panic");
        if self.fail {
            Err(ErrorCode::Io)
        } else {
            Ok(())
        }
    }
}

fn fixture(fail: bool) -> (NamespaceOperationCleanup, Arc<ControlFs>, NamespaceClient) {
    let server = Arc::new(ControlFs {
        inner: alan_agentfs::AgentFs::new(),
        entered: Notify::new(),
        release: Notify::new(),
        walks: AtomicUsize::new(0),
        fail,
        panic: false,
    });
    let client = NamespaceClient::new(InProcessTransport::new(server.clone()));
    (NamespaceOperationCleanup::default(), server, client)
}

fn armed(
    cleanup: &NamespaceOperationCleanup,
    client: NamespaceClient,
    path: String,
    command: &'static [u8],
) -> OperationLease {
    let mut operation = cleanup.lease(client, command).unwrap();
    operation.capture(path).unwrap();
    operation
}

#[tokio::test]
async fn retirement_waits_for_control_acknowledgement_and_closes_admission() {
    let (cleanup, server, client) = fixture(false);
    drop(armed(
        &cleanup,
        client.clone(),
        "/machine/ctl".into(),
        b"compact",
    ));
    server.entered.notified().await;
    let finish = cleanup.finish();
    tokio::pin!(finish);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut finish)
            .await
            .is_err()
    );
    server.release.notify_one();
    finish.await.unwrap();
    assert!(cleanup.lease(client, b"compact").is_err());
}

#[tokio::test(start_paused = true)]
async fn retirement_failure_and_timeout_never_report_success() {
    for fail in [false, true] {
        let (cleanup, server, client) = fixture(fail);
        if fail {
            server.release.notify_one();
        }
        drop(armed(&cleanup, client, "/machine/ctl".into(), b"compact"));
        let error = cleanup.finish().await.unwrap_err();
        assert!(error.to_string().contains("cleanup is unconfirmed"));
    }
}

#[tokio::test]
async fn panicked_cleanup_is_reported_as_uncertain() {
    let server = Arc::new(ControlFs {
        inner: alan_agentfs::AgentFs::new(),
        entered: Notify::new(),
        release: Notify::new(),
        walks: AtomicUsize::new(0),
        fail: false,
        panic: true,
    });
    server.release.notify_one();
    let cleanup = NamespaceOperationCleanup::default();
    let client = NamespaceClient::new(InProcessTransport::new(server));
    drop(armed(&cleanup, client, "/machine/ctl".into(), b"compact"));
    assert!(cleanup.finish().await.is_err());
}

#[tokio::test]
async fn already_terminal_model_operation_is_confirmed_without_reviving_it() {
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(alan_llm::MockLlmProvider::new()));
    let client = NamespaceClient::new(InProcessTransport::new(llmfs));
    let id = client
        .clone_via_open("/connections/default/clone")
        .await
        .unwrap();
    let base = format!("/connections/default/{id}");
    let generation = alan_llm::GenerationRequest::new().with_user_message("done");
    let request =
        super::super::generation::LlmRequestDoc::from_generation_request(&generation).unwrap();
    client
        .write_document(
            &format!("{base}/data"),
            &serde_json::to_vec(&request).unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let status: serde_json::Value =
                serde_json::from_slice(&client.read_file(&format!("{base}/status")).await.unwrap())
                    .unwrap();
            if status["status"] == "done" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let cleanup = NamespaceOperationCleanup::default();
    drop(armed(&cleanup, client, format!("{base}/ctl"), b"abort"));
    cleanup.finish().await.unwrap();
}

#[tokio::test]
async fn runtime_exit_reports_uncertain_cleanup_and_still_closes_its_writer() {
    let (cleanup, server, client) = fixture(true);
    server.release.notify_one();
    let lease = armed(&cleanup, client, "/machine/ctl".into(), b"compact");
    let task = tokio::spawn(async move {
        let _lease = lease;
        std::future::pending::<()>().await;
    });
    let dir = tempfile::tempdir().unwrap();
    let recorder = crate::rollout::RolloutRecorder::new_in_dir("/proc/test", "test", dir.path())
        .await
        .unwrap();
    let retained = recorder.clone();
    let recorders = Arc::new(std::sync::OnceLock::new());
    recorders.set(Some(recorder)).unwrap();
    let (submission, _) = tokio::sync::mpsc::channel(1);
    let (shutdown, _) = tokio::sync::mpsc::channel(1);
    let (_, ready) = tokio::sync::oneshot::channel();
    let controller = crate::runtime::RuntimeController::spawned(
        submission, shutdown, task, ready, recorders, cleanup,
    );
    let handle = controller.handle.clone();
    drop(controller);
    assert!(
        handle
            .finish_after_exit()
            .await
            .unwrap_err()
            .to_string()
            .contains("cleanup is unconfirmed")
    );
    assert!(
        retained
            .record_nowait(crate::rollout::RolloutItem::Event(
                crate::rollout::EventRecord {
                    event_type: "after_exit".into(),
                    payload: serde_json::json!({}),
                    timestamp: "test".into(),
                }
            ))
            .is_err()
    );
}

#[tokio::test]
async fn uncertain_cleanup_rejects_tool_and_models_before_namespace_io() {
    let (_, server, _) = fixture(false);
    let environment = super::super::NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(server.clone()),
        "/agent/1",
        "default",
    );
    environment
        .operation_cleanup
        .uncertain
        .store(true, Ordering::Release);
    let result = environment
        .tool_execution()
        .run_action("test", "/bin/test", Vec::<String>::new())
        .await;
    assert!(result.is_err());
    let cancel = tokio_util::sync::CancellationToken::new();
    assert!(
        environment
            .generation()
            .generate_controlled(
                &alan_llm::GenerationRequest::new().with_user_message("test"),
                1,
                &cancel
            )
            .await
            .is_err()
    );
    let identity = crate::runtime::model_binding::CallableIdentity {
        profile: "default".into(),
        provider: "mock".into(),
        model: "pinned".into(),
        credential_ref: None,
        revision: "captured".into(),
    };
    let request = alan_llm::ChoiceEvaluationRequest {
        input: "test".into(),
        candidates: vec![alan_llm::EvaluationCandidate {
            id: "one".into(),
            description: "one".into(),
        }],
    };
    assert!(
        environment
            .allocate_choice_evaluation(identity, request, 30_000, &cancel)
            .await
            .is_err()
    );
    assert_eq!(
        server.walks.load(Ordering::SeqCst),
        0,
        "uncertain admission must be rejected before Tool namespace/spawn IO"
    );
}

#[tokio::test]
async fn identity_is_retired_even_if_admission_changes_during_allocation() {
    let (cleanup, server, client) = fixture(false);
    let mut operation = cleanup.lease(client, b"compact").unwrap();
    cleanup.uncertain.store(true, Ordering::Release);
    assert!(operation.capture("/machine/ctl".into()).is_err());
    drop(operation);
    tokio::time::timeout(Duration::from_secs(1), server.entered.notified())
        .await
        .unwrap();
    server.release.notify_one();
    assert!(cleanup.finish().await.is_err());
}
