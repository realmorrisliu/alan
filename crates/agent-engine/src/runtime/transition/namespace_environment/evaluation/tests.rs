use super::*;
use alan_ap::{ErrorCode, FileKind, FileServer, InProcessTransport, Qid, Stat};
use alan_kernel::{Access, MountFs, Namespace};
use alan_llm::{EvaluationCandidate, LlmProvider};
use alan_llmfs::{ConnectionProfile, LlmFs};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Notify;

struct Evaluator {
    calls: Arc<AtomicUsize>,
    started: Arc<Notify>,
    hold: bool,
}
#[async_trait::async_trait]
impl LlmProvider for Evaluator {
    fn provider_name(&self) -> &'static str {
        "test"
    }
    fn supports_generation(&self) -> bool {
        false
    }
    fn supports_choice_evaluation(&self) -> bool {
        true
    }
    async fn evaluate_choice(
        &mut self,
        request: ChoiceEvaluationRequest,
    ) -> Result<ChoiceEvaluationResponse> {
        assert_eq!(request.input, "  pwd\n");
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        if request.candidates[0].id == "provider-error" {
            anyhow::bail!("synthetic provider outage");
        }
        if self.hold {
            std::future::pending::<()>().await;
        }
        Ok(ChoiceEvaluationResponse {
            selection: EvaluationSelection::Selected("command".into()),
            usage: None,
        })
    }
}
fn identity() -> CallableIdentity {
    CallableIdentity {
        profile: "test".into(),
        provider: "test".into(),
        model: "pinned".into(),
        credential_ref: Some("ref".into()),
        revision: "captured".into(),
    }
}

fn setup(
    hold: bool,
) -> (
    NamespaceRuntimeEnvironment,
    CallableIdentity,
    Arc<AtomicUsize>,
    Arc<Notify>,
) {
    let calls = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(Notify::new());
    let fs = LlmFs::new();
    fs.register_connection_profile(
        "test",
        ConnectionProfile::new("test", "pinned", "ref"),
        Box::new(Evaluator {
            calls: calls.clone(),
            started: started.clone(),
            hold,
        }),
    );
    let mut ns = Namespace::new();
    // This is the actual mounted operation. No command/Tool/Host file route exists.
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(fs)),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let identity = identity();
    (
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "test"),
        identity,
        calls,
        started,
    )
}
fn request() -> ChoiceEvaluationRequest {
    ChoiceEvaluationRequest {
        input: "  pwd\n".into(),
        candidates: vec![EvaluationCandidate {
            id: "command".into(),
            description: "literal command syntax, advice only".into(),
        }],
    }
}

#[tokio::test]
async fn allocated_operation_waits_for_commit_and_returns_typed_advice_once() {
    let (environment, identity, calls, _) = setup(false);
    let cancel = CancellationToken::new();
    let operation = environment
        .allocate_choice_evaluation(identity, request(), 30_000, &cancel)
        .await
        .unwrap();
    assert!(!operation.operation_id().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let result = operation.commit(&cancel).await.unwrap();
    assert_eq!(
        result.selection,
        EvaluationSelection::Selected("command".into())
    );
    assert!(result.usage.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancellation_and_spent_durability_budget_do_not_commit() {
    for cancelled in [true, false] {
        let (environment, identity, calls, _) = setup(false);
        let cancel = CancellationToken::new();
        let mut operation = environment
            .allocate_choice_evaluation(identity, request(), 30_000, &cancel)
            .await
            .unwrap();
        if cancelled {
            cancel.cancel();
        } else {
            operation.expires = Instant::now();
        }
        let error = operation.commit(&cancel).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<NamespaceEvaluationFailure>(),
            Some(&if cancelled { Cancelled } else { TimedOut })
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn explicit_abort_and_inflight_cancel_never_retry() {
    let (environment, identity, calls, started) = setup(true);
    let cancel = CancellationToken::new();
    let operation = environment
        .allocate_choice_evaluation(identity.clone(), request(), 30_000, &cancel)
        .await
        .unwrap();
    operation.abort().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let operation = environment
        .allocate_choice_evaluation(identity, request(), 30_000, &cancel)
        .await
        .unwrap();
    let task_cancel = cancel.clone();
    let task = tokio::spawn(async move { operation.commit(&task_cancel).await });
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .unwrap();
    cancel.cancel();
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(
        error.downcast_ref::<NamespaceEvaluationFailure>(),
        Some(&Cancelled)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn captured_model_mismatch_rejects_the_result() {
    let (environment, mut identity, calls, _) = setup(false);
    identity.model = "different".into();
    let cancel = CancellationToken::new();
    let operation = environment
        .allocate_choice_evaluation(identity, request(), 30_000, &cancel)
        .await
        .unwrap();
    let error = operation.commit(&cancel).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<NamespaceEvaluationFailure>(),
        Some(&Malformed)
    );
    assert!(
        error
            .downcast_ref::<NamespaceEvaluationUncertainty>()
            .is_none()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[derive(Clone, Copy, PartialEq)]
enum GateAt {
    BufferedWrite,
    AllocationRead,
    AllocationOpen,
    CleanupClunk,
}

struct EvaluationGate {
    fs: LlmFs,
    data_fids: std::sync::Mutex<std::collections::HashSet<Fid>>,
    buffered: Arc<Notify>,
    gate_at: GateAt,
    allocation_fids: std::sync::Mutex<std::collections::HashSet<Fid>>,
    read_once: std::sync::atomic::AtomicBool,
    allocated_id: std::sync::Mutex<Option<String>>,
}
#[async_trait::async_trait]
impl FileServer for EvaluationGate {
    async fn walk(
        &self,
        fid: Fid,
        newfid: Fid,
        names: &[String],
    ) -> std::result::Result<Qid, ErrorCode> {
        let result = self.fs.walk(fid, newfid, names).await?;
        if names.last().is_some_and(|name| name == "data") {
            self.data_fids.lock().unwrap().insert(newfid);
        }
        if names.last().is_some_and(|name| name == "evaluate") {
            self.allocation_fids.lock().unwrap().insert(newfid);
        }
        Ok(result)
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> std::result::Result<Qid, ErrorCode> {
        let result = self.fs.open(fid, mode).await?;
        let allocation = self.allocation_fids.lock().unwrap().contains(&fid);
        if allocation && self.gate_at == GateAt::AllocationOpen {
            self.buffered.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(result)
    }
    async fn read(
        &self,
        fid: Fid,
        offset: u64,
        count: u32,
    ) -> std::result::Result<Vec<u8>, ErrorCode> {
        let bytes = self.fs.read(fid, offset, count).await?;
        let allocation = self.allocation_fids.lock().unwrap().contains(&fid);
        if allocation {
            *self.allocated_id.lock().unwrap() =
                Some(String::from_utf8(bytes.clone()).unwrap().trim().into());
            if self.gate_at == GateAt::AllocationRead
                && !self.read_once.swap(true, Ordering::SeqCst)
            {
                self.buffered.notify_one();
                std::future::pending::<()>().await;
            }
        }
        Ok(bytes)
    }
    async fn write(
        &self,
        fid: Fid,
        offset: u64,
        data: &[u8],
    ) -> std::result::Result<u32, ErrorCode> {
        let count = self.fs.write(fid, offset, data).await?;
        let is_data = self.data_fids.lock().unwrap().contains(&fid);
        if is_data && self.gate_at != GateAt::AllocationRead {
            self.buffered.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(count)
    }
    async fn stat(&self, fid: Fid) -> std::result::Result<Stat, ErrorCode> {
        self.fs.stat(fid).await
    }
    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> std::result::Result<Qid, ErrorCode> {
        self.fs.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: Fid) -> std::result::Result<(), ErrorCode> {
        self.fs.remove(fid).await
    }
    async fn clunk(&self, fid: Fid) -> std::result::Result<(), ErrorCode> {
        let data = self.data_fids.lock().unwrap().remove(&fid);
        self.allocation_fids.lock().unwrap().remove(&fid);
        let result = self.fs.clunk(fid).await;
        if data && self.gate_at == GateAt::CleanupClunk {
            std::future::pending::<()>().await;
        }
        result
    }
}

#[tokio::test]
async fn cancellation_with_a_complete_buffer_aborts_before_cleanup_clunk() {
    for gate_at in [GateAt::BufferedWrite, GateAt::CleanupClunk] {
        let identity = identity();
        let calls = Arc::new(AtomicUsize::new(0));
        let fs = LlmFs::new();
        fs.register_connection_profile(
            "test",
            ConnectionProfile::new("test", "pinned", "ref"),
            Box::new(Evaluator {
                calls: calls.clone(),
                started: Arc::new(Notify::new()),
                hold: false,
            }),
        );
        let buffered = Arc::new(Notify::new());
        let gate = Arc::new(EvaluationGate {
            fs,
            data_fids: Default::default(),
            buffered: buffered.clone(),
            gate_at,
            allocation_fids: Default::default(),
            read_once: false.into(),
            allocated_id: Default::default(),
        });
        let mut ns = Namespace::new();
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(gate.clone()),
            Access::ReadWrite,
        );
        let environment = NamespaceRuntimeEnvironment::new(
            InProcessTransport::new(Arc::new(MountFs::new(ns))),
            "/agent/1",
            "test",
        );
        let cancel = CancellationToken::new();
        let operation = environment
            .allocate_choice_evaluation(identity, request(), 30_000, &cancel)
            .await
            .unwrap();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { operation.commit(&task_cancel).await });
        tokio::time::timeout(Duration::from_secs(2), buffered.notified())
            .await
            .unwrap();
        cancel.cancel();
        let error = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<NamespaceEvaluationFailure>(),
            Some(&Cancelled)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(gate.data_fids.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn mounted_failures_retain_malformed_and_unavailable_categories() {
    let (mut environment, mut identity, calls, _) = setup(false);
    let cancel = CancellationToken::new();
    for (candidate, failure) in [("agent", Malformed), ("provider-error", Unavailable)] {
        let mut input = request();
        input.candidates[0].id = candidate.into();
        let operation = environment
            .allocate_choice_evaluation(identity.clone(), input, 30_000, &cancel)
            .await
            .unwrap();
        let error = operation.commit(&cancel).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<NamespaceEvaluationFailure>(),
            Some(&failure)
        );
        assert!(
            error
                .downcast_ref::<NamespaceEvaluationUncertainty>()
                .is_none(),
            "an acknowledged terminal error must not become abort uncertainty: {error:#}"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    environment.llm_connection = "absent".into();
    identity.profile = "absent".into();
    let error = match environment
        .allocate_choice_evaluation(identity, request(), 30_000, &cancel)
        .await
    {
        Ok(_) => panic!("absent Connection allocated an evaluation"),
        Err(error) => error,
    };
    assert_eq!(
        error.downcast_ref::<NamespaceEvaluationFailure>(),
        Some(&Unavailable)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cancelled_allocator_read_reconciles_the_same_operation_and_aborts() {
    for gate_at in [GateAt::AllocationOpen, GateAt::AllocationRead] {
        let identity = identity();
        let calls = Arc::new(AtomicUsize::new(0));
        let fs = LlmFs::new();
        fs.register_connection_profile(
            "test",
            ConnectionProfile::new("test", "pinned", "ref"),
            Box::new(Evaluator {
                calls: calls.clone(),
                started: Arc::new(Notify::new()),
                hold: false,
            }),
        );
        let buffered = Arc::new(Notify::new());
        let gate = Arc::new(EvaluationGate {
            fs,
            data_fids: Default::default(),
            buffered: buffered.clone(),
            gate_at,
            allocation_fids: Default::default(),
            read_once: false.into(),
            allocated_id: Default::default(),
        });
        let mut ns = Namespace::new();
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(gate.clone()),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "test");
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            environment
                .allocate_choice_evaluation(identity, request(), 30_000, &task_cancel)
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), buffered.notified())
            .await
            .unwrap();
        cancel.cancel();
        let error = match task.await.unwrap() {
            Ok(_) => panic!("cancelled allocation succeeded"),
            Err(error) => error,
        };
        assert_eq!(
            error.downcast_ref::<NamespaceEvaluationFailure>(),
            Some(&Cancelled)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(gate.allocation_fids.lock().unwrap().is_empty());
        let id = gate.allocated_id.lock().unwrap().clone().unwrap();
        let status = NamespaceClient::new(root)
            .read_file(&format!("/mnt/llm/connections/test/{id}/status"))
            .await
            .unwrap();
        assert!(String::from_utf8(status).unwrap().contains("aborted"));
    }
}

#[tokio::test]
async fn invalid_deadlines_are_typed_rejections_before_allocation() {
    let (environment, identity, calls, _) = setup(false);
    let cancel = CancellationToken::new();
    for deadline in [0, 30_001] {
        let error = match environment
            .allocate_choice_evaluation(identity.clone(), request(), deadline, &cancel)
            .await
        {
            Ok(_) => panic!("invalid deadline allocated an evaluation"),
            Err(error) => error,
        };
        assert_eq!(
            error.downcast_ref::<NamespaceEvaluationFailure>(),
            Some(&Malformed)
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let listing = NamespaceClient::new(environment.root.clone())
        .read_file("/mnt/llm/connections/test")
        .await
        .unwrap();
    assert!(
        !String::from_utf8(listing)
            .unwrap()
            .lines()
            .any(|name| name.starts_with('g'))
    );
}
