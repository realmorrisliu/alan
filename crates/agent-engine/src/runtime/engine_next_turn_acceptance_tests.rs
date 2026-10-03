//! Final acceptance fixtures use the real admission owner and actual broker receive.
use super::*;
use crate::rollout::RolloutItem;
use crate::runtime::model_binding::InputBinding;
use crate::runtime::transition::{RuntimeLoopState, advance_accepted_submission};

struct Fixture {
    state: RuntimeLoopState,
    queues: RuntimeSubmissionQueues,
    provider: MockLlmProvider,
    temp: TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let provider = MockLlmProvider::new();
        let env = crate::runtime::transition::tests::namespace_environment_with_live_process(
            provider.clone(),
        )
        .await;
        let mut core = crate::Config::default();
        core.memory.enabled = false;
        core.streaming_mode = crate::config::StreamingMode::Off;
        let callable = CapturedCallable {
            identity: CallableIdentity {
                profile: "acceptance".into(),
                provider: "openai_responses".into(),
                model: "acceptance-model".into(),
                credential_ref: None,
                revision: "1".into(),
            },
            root: env.root_transport(),
            connection: "default".into(),
            config: core.clone(),
        };
        env.model_bindings.lock().await.confirmed = Some(callable);
        let temp = TempDir::new().unwrap();
        let machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "acceptance", temp.path())
            .await
            .unwrap();
        let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
        queues.environment = Some(env.clone());
        queues.recorder = machine.input_recorder();
        let state = RuntimeLoopState {
            machine,
            environment: env,
            core_config: core,
            runtime_config: Default::default(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        };
        Self {
            state,
            queues,
            provider,
            temp,
        }
    }
    async fn admit(&self, s: &Submission) -> InputBinding {
        self.queues.admit_input(s).await.unwrap();
        let binding = self.state.machine.input_queue().lock().unwrap().bindings[&s.id].clone();
        let history = self.history().await;
        assert!(history.iter().any(|item| matches!(item, RolloutItem::Event(e) if e.event_type == "machine_input_admitted_v1" && e.payload["id"] == s.id && e.payload["op"] == serde_json::to_value(&s.op).unwrap() && e.payload["callable_binding"] == serde_json::to_value(&binding.callable_binding).unwrap() && e.payload["request_controls"] == serde_json::to_value(&binding.request_controls).unwrap())), "exact admission payload and binding required before disposition");
        binding
    }
    async fn history(&self) -> Vec<RolloutItem> {
        crate::rollout::RolloutRecorder::load_history(self.state.machine.rollout_path().unwrap())
            .await
            .unwrap()
    }
    async fn run(&mut self, s: Submission) -> anyhow::Result<()> {
        advance_accepted_submission(
            &mut self.state,
            s,
            &TurnInputBroker::default(),
            &CancellationToken::new(),
        )
        .await
        .result
        .map(|_| ())
    }
    async fn retained(&self, s: &Submission, binding: &InputBinding) {
        let queue = self.state.machine.input_queue();
        {
            let q = queue.lock().unwrap();
            assert!(q.queued_next_turn_inputs.iter().any(|(id, parts)| {
                id.as_deref() == Some(&s.id)
                    && serde_json::to_value(parts).unwrap()
                        == serde_json::to_value(match &s.op {
                            Op::Input { parts, .. } => parts,
                            _ => unreachable!(),
                        })
                        .unwrap()
            }));
            assert_eq!(&q.bindings[&s.id], binding);
        }
        let captures = self.state.environment.model_bindings.lock().await;
        assert_eq!(captures.captured[&s.id].identity, binding.callable_binding);
        drop(captures);
        let recovered = AgentMachine::load_from_rollout_in_dir(
            self.state.machine.rollout_path().unwrap(),
            "/proc/2",
            "test",
            self.temp.path(),
        )
        .await
        .unwrap();
        let q = recovered.input_queue();
        let q = q.lock().unwrap();
        assert!(q.pending.iter().any(|item| matches!(item, QueuedRuntimeItem::Submission(i) if serde_json::to_value(i).unwrap() == serde_json::to_value(s).unwrap())));
        assert_eq!(&q.bindings[&s.id], binding);
    }
    async fn excluded(&self, s: &Submission) {
        let recovered = AgentMachine::load_from_rollout_in_dir(
            self.state.machine.rollout_path().unwrap(),
            "/proc/2",
            "test",
            self.temp.path(),
        )
        .await
        .unwrap();
        assert!(
            !recovered
                .input_queue()
                .lock()
                .unwrap()
                .pending
                .iter()
                .any(|i| matches!(i, QueuedRuntimeItem::Submission(i) if i.id == s.id))
        );
        let events = String::from_utf8(
            alan_shell::Shell::new(self.state.environment.root_transport())
                .cat("/agent/1/machine/ui/events")
                .await
                .unwrap(),
        )
        .unwrap();
        let receipts: Vec<_> = events
            .lines()
            .filter_map(|line| {
                match serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap() {
                    alan_agent_protocol::UiEvent::InputCompleted {
                        submission_ids,
                        status,
                        ..
                    } if submission_ids.contains(&s.id) => Some((submission_ids, status)),
                    _ => None,
                }
            })
            .collect();
        assert_eq!(
            receipts,
            vec![(
                vec![s.id.clone()],
                alan_agent_protocol::UiInputStatus::Failed
            )]
        );
    }
    async fn recovery_retains(&self, s: &Submission, binding: &InputBinding) {
        let recovered = AgentMachine::load_from_rollout_in_dir(
            self.state.machine.rollout_path().unwrap(),
            "/proc/2",
            "test",
            self.temp.path(),
        )
        .await
        .unwrap();
        let q = recovered.input_queue();
        let q = q.lock().unwrap();
        assert!(q.pending.iter().any(|item| matches!(item, QueuedRuntimeItem::Submission(i) if serde_json::to_value(i).unwrap() == serde_json::to_value(s).unwrap())));
        assert_eq!(&q.bindings[&s.id], binding);
    }
    async fn no_dispatch(&self, ids: &[String]) {
        assert!(self.provider.recorded_requests().is_empty());
        assert!(!self.history().await.iter().any(|item| matches!(item, RolloutItem::Event(e) if e.event_type.contains("dispatched") && (ids.iter().any(|id| e.payload["submission_id"] == *id) || e.payload["submission_ids"].as_array().is_some_and(|a| ids.iter().any(|id| a.contains(&serde_json::json!(id))))))));
    }
    async fn no_receipt(&self, ids: &[String]) {
        let events = String::from_utf8(
            alan_shell::Shell::new(self.state.environment.root_transport())
                .cat("/agent/1/machine/ui/events")
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(), alan_agent_protocol::UiEvent::InputCompleted {submission_ids, ..} if ids.iter().any(|id| submission_ids.contains(id)))));
    }
}
// Same descriptor-level forwarding fault pattern as the namespace cancellation
// evidence fixtures; only writes to the real AgentFS notice snapshot fail.
struct NoticeWriteFaultFs {
    inner: alan_agentfs::AgentFs,
    notice_path: u64,
    failures: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl alan_ap::FileServer for NoticeWriteFaultFs {
    async fn walk(
        &self,
        fid: alan_ap::Fid,
        newfid: alan_ap::Fid,
        names: &[String],
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.walk(fid, newfid, names).await
    }
    async fn open(
        &self,
        fid: alan_ap::Fid,
        mode: alan_ap::OpenMode,
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(
        &self,
        fid: alan_ap::Fid,
        offset: u64,
        count: u32,
    ) -> Result<Vec<u8>, alan_ap::ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn write(
        &self,
        fid: alan_ap::Fid,
        offset: u64,
        data: &[u8],
    ) -> Result<u32, alan_ap::ErrorCode> {
        if self.inner.stat(fid).await?.qid.path == self.notice_path {
            self.failures
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(alan_ap::ErrorCode::Io);
        }
        self.inner.write(fid, offset, data).await
    }
    async fn stat(&self, fid: alan_ap::Fid) -> Result<alan_ap::Stat, alan_ap::ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn create(
        &self,
        fid: alan_ap::Fid,
        newfid: alan_ap::Fid,
        name: &str,
        kind: alan_ap::FileKind,
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: alan_ap::Fid) -> Result<(), alan_ap::ErrorCode> {
        self.inner.remove(fid).await
    }
    async fn clunk(&self, fid: alan_ap::Fid) -> Result<(), alan_ap::ErrorCode> {
        self.inner.clunk(fid).await
    }
}

#[tokio::test]
async fn acceptance_next_turn_notice_snapshot_write_fault_is_nonterminal() {
    use alan_ap::FileServer;
    let mut f = Fixture::new().await;
    let inner = alan_agentfs::AgentFs::new();
    let notice_path = inner
        .walk(
            alan_ap::Fid::ROOT,
            alan_ap::Fid(9),
            &["machine".into(), "ui".into(), "notice".into()],
        )
        .await
        .unwrap()
        .path;
    inner.clunk(alan_ap::Fid(9)).await.unwrap();
    let fault = Arc::new(NoticeWriteFaultFs {
        inner,
        notice_path,
        failures: Default::default(),
    });
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let mut env = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    env.model_bindings = f.state.environment.model_bindings.clone();
    env.active_binding = f.state.environment.active_binding.clone();
    f.state.environment = env.clone();
    f.queues.environment = Some(env);
    let n = next();
    let binding = f.admit(&n).await;
    let capture = f.state.environment.model_bindings.lock().await.captured[&n.id].clone();
    let error = f.run(n.clone()).await.unwrap_err();
    assert!(
        fault.failures.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "real notice snapshot write must fail"
    );
    assert!(!error.to_string().is_empty());
    f.retained(&n, &binding).await;
    let captures = f.state.environment.model_bindings.lock().await;
    let retained = &captures.captured[&n.id];
    assert_eq!(retained.identity, capture.identity);
    assert_eq!(retained.connection, capture.connection);
    assert_eq!(
        serde_json::to_value(&retained.config).unwrap(),
        serde_json::to_value(&capture.config).unwrap()
    );
    drop(captures);
    f.no_dispatch(std::slice::from_ref(&n.id)).await;
    assert!(!f.history().await.iter().any(
        |item| matches!(item, RolloutItem::Event(e) if e.event_type == "machine_input_removed_v1")
    ));
    assert!(
        f.state.agent_files().action_ids().await.unwrap().is_empty(),
        "no tool action"
    );
    f.no_receipt(std::slice::from_ref(&n.id)).await;
    let events = String::from_utf8(
        alan_shell::Shell::new(f.state.environment.root_transport())
            .cat("/agent/1/machine/ui/events")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        !events.lines().any(|line| matches!(
            serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
            alan_agent_protocol::UiEvent::InputCompleted { .. }
        )),
        "queued notice failure must emit no InputCompleted of any result"
    );
    assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(), alan_agent_protocol::UiEvent::Error { message, recoverable: true } if message.contains(&n.id) && message.contains("retained"))), "retained input must have an observable nonterminal error even while notice writes fail");
}

fn next() -> Submission {
    Submission::new(Op::Input {
        parts: vec![ContentPart::text("exact queued payload")],
        mode: InputMode::NextTurn,
    })
}
fn turn() -> Submission {
    Submission::new(Op::Turn {
        parts: vec![ContentPart::text("exact trigger")],
        context: None,
    })
}
fn batch(items: &[RolloutItem], kind: &str, ids: &[String]) {
    assert_eq!(items.len(), 1);
    assert!(
        matches!(&items[0], RolloutItem::Event(e) if e.event_type == kind && e.payload["submission_ids"] == serde_json::json!(ids)),
        "fault must hit exact correlated disposition batch"
    );
}

#[tokio::test]
async fn acceptance_busy_lease_preserves_exact_next_turn_evidence() {
    for removal_fault in [false, true] {
        let mut f = Fixture::new().await;
        let n = next();
        let binding = f.admit(&n).await;
        f.run(n.clone()).await.unwrap();
        let t = turn();
        let trigger_binding = f.admit(&t).await;
        let lease = f
            .state
            .environment
            .agent_files()
            .begin_tape_generation()
            .await
            .unwrap();
        let observed = if removal_fault {
            let (probe, observed) = f
                .state
                .machine
                .input_recorder()
                .unwrap()
                .batch_failure_probe(false);
            f.state.machine.set_input_recorder_for_test(probe);
            Some(observed)
        } else {
            None
        };
        assert!(f.run(t.clone()).await.is_err());
        f.retained(&n, &binding).await;
        f.no_dispatch(&[n.id.clone(), t.id.clone()]).await;
        f.no_receipt(std::slice::from_ref(&n.id)).await;
        if let Some(mut observed) = observed {
            batch(
                &observed.recv().await.unwrap(),
                "machine_inputs_removed_v1",
                std::slice::from_ref(&t.id),
            );
            f.recovery_retains(&t, &trigger_binding).await;
            f.no_receipt(std::slice::from_ref(&t.id)).await;
            let events = String::from_utf8(
                alan_shell::Shell::new(f.state.environment.root_transport())
                    .cat("/agent/1/machine/ui/events")
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(), alan_agent_protocol::UiEvent::Error { message, .. } if message.contains(&t.id) && message.contains("uncertain"))));
        } else {
            assert!(f.history().await.iter().any(|i| matches!(i, RolloutItem::Event(e) if e.event_type == "machine_inputs_removed_v1" && e.payload["submission_ids"] == serde_json::json!([t.id]))));
            f.excluded(&t).await;
        }
        lease.finish().await.unwrap();
    }
}

#[tokio::test]
async fn acceptance_overflow_requires_exact_durable_admission() {
    let mut f = Fixture::new().await;
    let mut originals = Vec::new();
    for _ in 0..16 {
        let s = next();
        let b = f.admit(&s).await;
        f.run(s.clone()).await.unwrap();
        originals.push((s, b));
    }
    let s = next();
    f.admit(&s).await;
    assert!(
        f.run(s.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("Too many queued next_turn")
    );
    assert!(f.history().await.iter().any(|i| matches!(i, RolloutItem::Event(e) if e.event_type == "machine_inputs_removed_v1" && e.payload["submission_ids"] == serde_json::json!([s.id]))));
    f.excluded(&s).await;
    for (s, b) in originals {
        f.retained(&s, &b).await;
    }
}

#[tokio::test]
async fn acceptance_missing_binding_actual_broker_receive_no_dispatch() {
    let mut f = Fixture::new().await;
    let n = next();
    let binding = f.admit(&n).await;
    let t = turn();
    f.admit(&t).await;
    f.state
        .machine
        .input_queue()
        .lock()
        .unwrap()
        .bindings
        .remove(&t.id);
    let broker = TurnInputBroker::default();
    assert!(broker.push(t.clone()).await);
    f.state.machine.accept_submission(n.id.clone());
    let mut emit = |_| async {};
    let error = crate::runtime::transition::accepted_submission::drive_turn_submission_with_cancel(
        &mut f.state,
        n.clone(),
        &broker,
        &mut emit,
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Queued next_turn callable or controls incompatible with explicit turn; queued inputs retained"
    );
    assert!(
        broker.try_recv().await.is_none(),
        "must execute later broker receive branch"
    );
    f.no_dispatch(&[n.id.clone(), t.id.clone()]).await;
    f.retained(&n, &binding).await;
}

#[tokio::test]
async fn acceptance_full_controls_incompatible_without_provider_call() {
    let mut f = Fixture::new().await;
    let n = next();
    let b = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    f.state
        .environment
        .model_bindings
        .lock()
        .await
        .runtime_intent = crate::RequestControlIntent::reasoning_effort(Some(
        alan_agent_protocol::ReasoningEffort::High,
    ));
    let t = turn();
    let tb = f.admit(&t).await;
    assert_eq!(b.callable_binding, tb.callable_binding);
    assert_ne!(b.request_controls, tb.request_controls);
    assert_eq!(
        f.run(t.clone()).await.unwrap_err().to_string(),
        "Queued next_turn callable or controls incompatible with explicit turn; queued inputs retained"
    );
    f.no_dispatch(&[n.id.clone(), t.id.clone()]).await;
    f.retained(&n, &b).await;
}

#[tokio::test]
async fn acceptance_removal_fault_exact_ids_and_recovery() {
    let mut f = Fixture::new().await;
    let n = next();
    let b = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    f.state
        .environment
        .model_bindings
        .lock()
        .await
        .runtime_intent = crate::RequestControlIntent::reasoning_effort(Some(
        alan_agent_protocol::ReasoningEffort::High,
    ));
    let t = turn();
    let tb = f.admit(&t).await;
    assert_eq!(b.callable_binding, tb.callable_binding);
    assert_ne!(b.request_controls, tb.request_controls);
    let (probe, mut observed) = f
        .state
        .machine
        .input_recorder()
        .unwrap()
        .batch_failure_probe(false);
    f.state.machine.set_input_recorder_for_test(probe);
    assert!(
        f.run(t.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("removal uncertain")
    );
    batch(
        &observed.recv().await.unwrap(),
        "machine_inputs_removed_v1",
        std::slice::from_ref(&t.id),
    );
    f.no_receipt(std::slice::from_ref(&t.id)).await;
    f.no_dispatch(&[n.id.clone(), t.id.clone()]).await;
    f.retained(&n, &b).await;
    f.recovery_retains(&t, &tb).await;
    assert!(
        f.state
            .machine
            .input_queue()
            .lock()
            .unwrap()
            .queue_uncertain_ids
            .contains(&t.id)
    );
}

#[tokio::test]
async fn acceptance_equal_effort_different_resolved_source_incompatible() {
    let mut f = Fixture::new().await;
    f.state
        .environment
        .model_bindings
        .lock()
        .await
        .confirmed
        .as_mut()
        .unwrap()
        .config
        .model_reasoning_effort = Some(alan_agent_protocol::ReasoningEffort::Low);
    let n = next();
    let b = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    f.state
        .environment
        .model_bindings
        .lock()
        .await
        .runtime_intent = crate::RequestControlIntent::reasoning_effort(Some(
        alan_agent_protocol::ReasoningEffort::Low,
    ));
    let t = turn();
    let tb = f.admit(&t).await;
    assert_eq!(b.callable_binding, tb.callable_binding);
    assert_eq!(b.request_controls.reasoning, tb.request_controls.reasoning);
    assert_ne!(b.request_controls.source, tb.request_controls.source);
    assert_eq!(
        f.run(t.clone()).await.unwrap_err().to_string(),
        "Queued next_turn callable or controls incompatible with explicit turn; queued inputs retained"
    );
    f.no_dispatch(&[n.id.clone(), t.id.clone()]).await;
    f.retained(&n, &b).await;
}

#[tokio::test]
async fn acceptance_missing_binding_direct_actionable_no_dispatch() {
    let mut f = Fixture::new().await;
    let n = next();
    let b = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    let t = turn();
    f.admit(&t).await;
    f.state
        .machine
        .input_queue()
        .lock()
        .unwrap()
        .bindings
        .remove(&t.id);
    assert_eq!(
        f.run(t.clone()).await.unwrap_err().to_string(),
        "Queued next_turn callable or controls incompatible with explicit turn; queued inputs retained"
    );
    f.no_dispatch(&[n.id.clone(), t.id.clone()]).await;
    f.retained(&n, &b).await;
}

#[tokio::test]
async fn acceptance_consumption_dispatch_fault_exact_ids_and_recovery() {
    let mut f = Fixture::new().await;
    let n = next();
    let b = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    let t = turn();
    let tb = f.admit(&t).await;
    assert_eq!(b, tb);
    let (probe, mut observed) = f
        .state
        .machine
        .input_recorder()
        .unwrap()
        .batch_failure_probe(false);
    f.state.machine.set_input_recorder_for_test(probe);
    assert!(f.run(t.clone()).await.is_err());
    batch(
        &observed.recv().await.unwrap(),
        "machine_inputs_dispatched_v1",
        &[n.id.clone(), t.id.clone()],
    );
    f.no_receipt(&[n.id.clone(), t.id.clone()]).await;
    f.no_dispatch(&[n.id.clone(), t.id.clone()]).await;
    f.retained(&n, &b).await;
    f.recovery_retains(&t, &tb).await;
    let q = f.state.machine.input_queue();
    let q = q.lock().unwrap();
    assert!(q.queue_uncertain_ids.contains(&n.id) && q.queue_uncertain_ids.contains(&t.id));
}

#[path = "engine_next_turn_discard_tests.rs"]
mod discard;
