use super::*;
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Qid, Stat};
use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;

struct ControlFs {
    inner: Arc<dyn FileServer>,
    control_fids: std::sync::Mutex<std::collections::HashSet<Fid>>,
    held_file: &'static str,
    hold_read: bool,
    cancel_on_data: Option<tokio_util::sync::CancellationToken>,
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
        if let Some(cancel) = &self.cancel_on_data
            && names.last().is_some_and(|name| name == "data")
        {
            cancel.cancel();
            return std::future::pending().await;
        }
        let qid = self.inner.walk(fid, newfid, names).await?;
        if names.last().is_some_and(|name| name == self.held_file) {
            self.control_fids.lock().unwrap().insert(newfid);
        }
        Ok(qid)
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if self.hold_read && self.control_fids.lock().unwrap().contains(&fid) {
            self.entered.notify_one();
            self.release.notified().await;
        }
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
        if !control || self.hold_read {
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
        held_file: "ctl",
        hold_read: false,
        cancel_on_data: None,
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

fn allocation_fixture(
    inner: Arc<dyn FileServer>,
    hold_read: bool,
) -> (super::super::NamespaceRuntimeEnvironment, Arc<ControlFs>) {
    let server = Arc::new(ControlFs {
        inner,
        control_fids: Default::default(),
        held_file: "clone",
        hold_read,
        cancel_on_data: None,
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
    (environment, server)
}

async fn allocation_budget(seconds: u64) -> crate::agent_machine::task_budget::TaskBudget {
    let limits = crate::TaskBudgetLimits {
        execution_seconds: std::num::NonZeroU64::new(seconds).unwrap(),
        ..Default::default()
    };
    crate::agent_machine::task_budget::TaskBudget::start("allocation".into(), limits, None)
        .await
        .unwrap()
}

struct ReleasedRunner {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait::async_trait]
impl alan_kernel::ProcessRunner for ReleasedRunner {
    async fn run(&self, _: alan_kernel::ProcessInvocation) -> alan_kernel::ProcessOutcome {
        self.started.notify_one();
        self.release.notified().await;
        alan_kernel::ProcessOutcome::exited(0, b"done".to_vec())
    }
}

#[tokio::test(start_paused = true)]
async fn zero_tool_timeout_preserves_async_completion_and_cancellation() {
    for cancelled in [false, true] {
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let root = super::super::tests::tool_test_root(Arc::new(ReleasedRunner {
            started: started.clone(),
            release: release.clone(),
        }))
        .await;
        let environment = super::super::NamespaceRuntimeEnvironment::new(
            InProcessTransport::new(root),
            "/agent/1",
            "default",
        );
        let cancel = tokio_util::sync::CancellationToken::new();
        let operation = tokio::spawn({
            let tool = environment.tool_execution();
            let cancel = cancel.clone();
            async move {
                tool.run_action_with_cancel_and_timeout(
                    "held",
                    None,
                    "/bin/held",
                    Vec::<String>::new(),
                    &cancel,
                    0,
                )
                .await
            }
        });
        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .expect("zero timeout must allow allocation");
        tokio::time::advance(Duration::from_secs(5)).await;
        assert!(
            !operation.is_finished(),
            "zero must permit asynchronous work"
        );
        if cancelled {
            cancel.cancel();
        } else {
            release.notify_one();
        }
        let result = tokio::time::timeout(Duration::from_secs(2), operation)
            .await
            .unwrap()
            .unwrap();
        if cancelled {
            let error = result.unwrap_err();
            let process = error
                .downcast_ref::<super::super::NamespaceToolProcessError>()
                .unwrap();
            assert_eq!(process.pid, "2");
            assert_eq!(
                process.category,
                super::super::NamespaceToolObservationFailure::Cancelled
            );
            assert!(!error.is::<NamespaceOperationUncertainty>());
        } else {
            let result = result.unwrap();
            assert_eq!(result.output, "done");
            assert_eq!(result.exit_code, 0);
        }
        environment.operation_cleanup.check_admission().unwrap();
    }
}

#[tokio::test]
async fn cancellation_before_allocation_does_not_poison_a_healthy_scope_or_perform_io() {
    let (_, server, _) = fixture(false);
    let environment = super::super::NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(server.clone()),
        "/agent/1",
        "default",
    );
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let request = alan_llm::GenerationRequest::new().with_user_message("unexecuted");
    assert!(
        environment
            .generation()
            .generate_controlled(&request, 0, &cancel)
            .await
            .is_err()
    );
    assert!(
        environment
            .generation()
            .generate_with_text_events_controlled(&request, &mut |_| async {}, 0, &cancel)
            .await
            .is_err()
    );
    assert!(
        environment
            .tool_execution()
            .run_action_with_cancel("held", "/bin/held", Vec::<String>::new(), &cancel)
            .await
            .is_err()
    );
    environment.operation_cleanup.check_admission().unwrap();
    assert_eq!(server.walks.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn generation_allocation_obeys_cancel_and_budget_deadline_without_losing_known_identity() {
    for streaming in [false, true] {
        for hold_read in [false, true] {
            for stop in ["cancel", "budget", "request"] {
                let mock = alan_llm::MockLlmProvider::new();
                let llmfs = Arc::new(alan_llmfs::LlmFs::new());
                llmfs.register_connection("default", Box::new(mock.clone()));
                let mut ns = alan_kernel::Namespace::new();
                ns.mount(
                    "/mnt/llm",
                    InProcessTransport::new(llmfs),
                    alan_kernel::Access::ReadWrite,
                );
                let (environment, server) =
                    allocation_fixture(Arc::new(alan_kernel::MountFs::new(ns)), hold_read);
                let budget = allocation_budget(if stop == "budget" { 1 } else { 10 }).await;
                let cancel = tokio_util::sync::CancellationToken::new();
                let operation = tokio::spawn({
                    let generation = environment
                        .generation()
                        .with_task_budget(Some(budget.clone()));
                    let cancel = cancel.clone();
                    async move {
                        let request =
                            alan_llm::GenerationRequest::new().with_user_message("unexecuted");
                        if streaming {
                            generation
                                .generate_with_text_events_controlled(
                                    &request,
                                    &mut |_| async {},
                                    if stop == "request" { 1 } else { 0 },
                                    &cancel,
                                )
                                .await
                                .map(|_| ())
                        } else {
                            generation
                                .generate_controlled(
                                    &request,
                                    if stop == "request" { 1 } else { 0 },
                                    &cancel,
                                )
                                .await
                                .map(|_| ())
                        }
                    }
                });
                server.entered.notified().await;
                if stop != "cancel" {
                    tokio::time::advance(Duration::from_secs(1)).await;
                } else {
                    cancel.cancel();
                }
                let error = tokio::time::timeout(Duration::from_secs(2), operation)
                    .await
                    .expect("allocation ignored cancellation/deadline")
                    .unwrap()
                    .unwrap_err();
                assert_eq!(error.is::<NamespaceOperationUncertainty>(), hold_read);
                assert_eq!(
                    environment.operation_cleanup.check_admission().is_err(),
                    hold_read
                );
                assert_eq!(budget.snapshot().await.model_calls, 1);
                assert!(mock.recorded_requests().is_empty());
                if !hold_read {
                    let status = NamespaceClient::new(environment.root.clone())
                        .read_file("/mnt/llm/connections/default/g0/status")
                        .await
                        .unwrap();
                    let status: serde_json::Value = serde_json::from_slice(&status).unwrap();
                    assert_eq!(status["status"], "aborted");
                }
                server.release.notify_one();
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn tool_allocation_obeys_cancel_and_budget_deadline_without_losing_known_pid() {
    for hold_read in [false, true] {
        for stop in ["cancel", "budget", "request"] {
            let started = Arc::new(Notify::new());
            let dropped = Arc::new(Notify::new());
            let root = super::super::tests::tool_test_root(Arc::new(
                super::super::tests::AbortObservedRunner {
                    started: started.clone(),
                    dropped: dropped.clone(),
                },
            ))
            .await;
            let (environment, server) = allocation_fixture(root, hold_read);
            let budget = allocation_budget(if stop == "budget" { 1 } else { 10 }).await;
            budget
                .reserve(crate::agent_machine::task_budget::Admission::Tool)
                .await
                .unwrap();
            let cancel = tokio_util::sync::CancellationToken::new();
            let operation = tokio::spawn({
                let tool = environment.tool_execution();
                let budget = budget.clone();
                let cancel = cancel.clone();
                let timeout_secs = if stop == "request" { 1 } else { 30 };
                async move {
                    budget
                        .run(&cancel, |stop| async move {
                            tool.run_action_with_cancel_and_timeout(
                                "held",
                                None,
                                "/bin/held",
                                Vec::<String>::new(),
                                &stop,
                                timeout_secs,
                            )
                            .await
                        })
                        .await
                }
            });
            server.entered.notified().await;
            if !hold_read {
                started.notified().await;
            }
            if stop != "cancel" {
                tokio::time::advance(Duration::from_secs(1)).await;
            } else {
                cancel.cancel();
            }
            let error = tokio::time::timeout(Duration::from_secs(2), operation)
                .await
                .expect("Tool allocation ignored cancellation/deadline")
                .unwrap()
                .unwrap_err();
            assert_eq!(error.is::<NamespaceOperationUncertainty>(), hold_read);
            assert_eq!(
                environment.operation_cleanup.check_admission().is_err(),
                hold_read
            );
            assert_eq!(budget.snapshot().await.tool_calls, 1);
            if !hold_read {
                let process = error
                    .downcast_ref::<super::super::NamespaceToolProcessError>()
                    .unwrap();
                assert_eq!(process.pid, "2");
                tokio::time::timeout(Duration::from_millis(100), dropped.notified())
                    .await
                    .expect("known Tool must stop");
            }
            server.release.notify_one();
        }
    }
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
        held_file: "ctl",
        hold_read: false,
        cancel_on_data: None,
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
        let cancel = tokio_util::sync::CancellationToken::new();
        let server = Arc::new(ControlFs {
            inner: Arc::new(alan_kernel::MountFs::new(namespace)),
            control_fids: Default::default(),
            held_file: "ctl",
            hold_read: false,
            cancel_on_data: Some(cancel.clone()),
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
        assert!(
            tokio::time::timeout(Duration::ZERO, server.entered.notified())
                .await
                .is_ok(),
            "the actual retirement control must be attempted"
        );
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
        held_file: "ctl",
        hold_read: false,
        cancel_on_data: None,
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
