use super::*;
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Qid, Stat};
use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;

struct ControlFs {
    inner: Arc<dyn FileServer>,
    control_fids: std::sync::Mutex<std::collections::HashSet<Fid>>,
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
        let qid = self.inner.walk(fid, newfid, names).await?;
        if names.last().is_some_and(|name| name == "ctl") {
            self.control_fids.lock().unwrap().insert(newfid);
        }
        Ok(qid)
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
        let control = self.control_fids.lock().unwrap().remove(&fid);
        self.inner.clunk(fid).await?;
        if !control {
            return Ok(());
        }
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
        inner: Arc::new(alan_agentfs::AgentFs::new()),
        control_fids: Default::default(),
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
async fn allocation_without_identity_never_reopens_admission_or_confirms_cleanup() {
    for explicit_retirement in [false, true] {
        let (cleanup, server, client) = fixture(false);
        let mut operation = cleanup.lease(client.clone(), b"cancel").unwrap();
        if explicit_retirement {
            assert!(operation.retire().await.is_err());
            assert!(
                cleanup
                    .check_admission()
                    .unwrap_err()
                    .is::<NamespaceOperationUncertainty>()
            );
        }
        drop(operation);
        assert!(
            cleanup
                .check_admission()
                .unwrap_err()
                .is::<NamespaceOperationUncertainty>()
        );
        assert!(cleanup.lease(client, b"cancel").is_err());
        assert!(
            cleanup
                .finish()
                .await
                .unwrap_err()
                .is::<NamespaceOperationUncertainty>()
        );
        assert!(
            cleanup
                .check_admission()
                .unwrap_err()
                .is::<NamespaceOperationUncertainty>()
        );
        assert_eq!(server.walks.load(Ordering::SeqCst), 0);
    }
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
        inner: Arc::new(alan_agentfs::AgentFs::new()),
        control_fids: Default::default(),
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
    drop(
        environment
            .operation_cleanup
            .lease(NamespaceClient::new(environment.root.clone()), b"cancel")
            .unwrap(),
    );
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

#[tokio::test(start_paused = true)]
async fn normal_generation_cancellation_is_bounded_when_control_acknowledgement_hangs() {
    for streaming in [false, true] {
        let provider = alan_llm::MockLlmProvider::new();
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(provider.clone()));
        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        let server = Arc::new(ControlFs {
            inner: Arc::new(alan_kernel::MountFs::new(namespace)),
            control_fids: Default::default(),
            entered: Notify::new(),
            release: Notify::new(),
            walks: AtomicUsize::new(0),
            fail: false,
            panic: false,
        });
        let environment = super::super::NamespaceRuntimeEnvironment::new(
            InProcessTransport::new(server.clone()),
            "/agent/1",
            "default",
        );
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        let request = alan_llm::GenerationRequest::new().with_user_message("unexecuted");
        let operation = async {
            if streaming {
                environment
                    .generation()
                    .generate_with_text_events_controlled(&request, &mut |_| async {}, 0, &cancel)
                    .await
                    .map(|_| ())
            } else {
                environment
                    .generation()
                    .generate_controlled(&request, 0, &cancel)
                    .await
                    .map(|_| ())
            }
        };
        let outcome = tokio::time::timeout(Duration::from_secs(2), operation).await;
        assert!(
            outcome.is_ok(),
            "normal cancellation hung awaiting its control acknowledgement"
        );
        let error = outcome.unwrap().unwrap_err();
        assert!(error.is::<NamespaceOperationUncertainty>());
        assert!(!crate::retry::is_retryable(&error));
        assert!(format!("{error:#}").contains("cleanup is unconfirmed"));
        assert!(provider.recorded_requests().is_empty());
        let walks = server.walks.load(Ordering::SeqCst);
        assert!(
            environment
                .generation()
                .generate_controlled(&request, 0, &tokio_util::sync::CancellationToken::new())
                .await
                .is_err()
        );
        assert_eq!(server.walks.load(Ordering::SeqCst), walks);
        let mut state = crate::runtime::transition::RuntimeLoopState {
            machine: crate::agent_machine::AgentMachine::new(),
            environment: environment.clone(),
            core_config: Default::default(),
            runtime_config: Default::default(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        };
        let mut completions = 0;
        let result = crate::runtime::transition::handle_submission_with_cancel(
            &mut state,
            alan_agent_protocol::Submission::new(alan_agent_protocol::Op::Interrupt),
            &mut |event| {
                if matches!(event, alan_agent_protocol::Event::TurnCompleted { .. }) {
                    completions += 1;
                }
                async {}
            },
            &tokio_util::sync::CancellationToken::new(),
        )
        .await;
        assert!(result.unwrap_err().is::<NamespaceOperationUncertainty>());
        assert_eq!(
            completions, 0,
            "uncertain cleanup cannot publish a completed cancellation"
        );
        assert!(environment.operation_cleanup.finish().await.is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn normal_tool_cancellation_retains_pid_and_uncertainty_when_control_acknowledgement_hangs() {
    let started = Arc::new(Notify::new());
    let dropped = Arc::new(Notify::new());
    let root =
        super::super::tests::tool_test_root(Arc::new(super::super::tests::AbortObservedRunner {
            started: started.clone(),
            dropped: dropped.clone(),
        }))
        .await;
    let server = Arc::new(ControlFs {
        inner: root,
        control_fids: Default::default(),
        entered: Notify::new(),
        release: Notify::new(),
        walks: AtomicUsize::new(0),
        fail: false,
        panic: false,
    });
    let environment = super::super::NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(server.clone()),
        "/agent/1",
        "default",
    );
    let cancel = tokio_util::sync::CancellationToken::new();
    let operation = tokio::spawn({
        let environment = environment.clone();
        let cancel = cancel.clone();
        async move {
            environment
                .tool_execution()
                .run_action_with_cancel("held", "/bin/held", Vec::<String>::new(), &cancel)
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(1), started.notified())
        .await
        .expect("the actual Tool runner must start");
    cancel.cancel();
    let error = tokio::time::timeout(Duration::from_secs(2), operation)
        .await
        .expect("normal Tool cancellation must not hang on control acknowledgement")
        .unwrap()
        .unwrap_err();
    assert!(error.is::<NamespaceOperationUncertainty>());
    assert!(!crate::retry::is_retryable(&error));
    let process = error
        .downcast_ref::<super::super::NamespaceToolProcessError>()
        .unwrap();
    assert_eq!(process.pid, "2");
    assert_eq!(
        process.category,
        super::super::NamespaceToolObservationFailure::Cancelled
    );
    tokio::time::timeout(Duration::from_millis(100), dropped.notified())
        .await
        .expect("the actual Tool runner must stop even though its control acknowledgement hangs");
    let walks = server.walks.load(Ordering::SeqCst);
    assert!(
        environment
            .tool_execution()
            .run_action("another", "/bin/held", Vec::<String>::new())
            .await
            .is_err()
    );
    assert_eq!(server.walks.load(Ordering::SeqCst), walks);
    assert!(environment.operation_cleanup.finish().await.is_err());
}
