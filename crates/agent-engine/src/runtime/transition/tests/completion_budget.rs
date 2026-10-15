use super::*;
use crate::agent_machine::task_budget::{Admission, Phase, TaskBudget};
use alan_ap::{ErrorCode, FileKind, InProcessTransport, Qid, Stat};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Notify;

struct UnidentifiedAllocationFs {
    inner: alan_llmfs::LlmFs,
    allocators: Mutex<std::collections::HashSet<Fid>>,
    allocations: AtomicUsize,
    cancel: Option<CancellationToken>,
}

#[async_trait::async_trait]
impl FileServer for UnidentifiedAllocationFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        if names.last().is_some_and(|name| name == "clone") {
            self.allocators.lock().unwrap().insert(newfid);
        }
        Ok(qid)
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let qid = self.inner.open(fid, mode).await?;
        if self.allocators.lock().unwrap().contains(&fid) {
            self.allocations.fetch_add(1, Ordering::SeqCst);
        }
        Ok(qid)
    }
    async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if self.allocators.lock().unwrap().contains(&fid) {
            if let Some(cancel) = &self.cancel {
                cancel.cancel();
            }
            return Err(ErrorCode::Io);
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
        self.allocators.lock().unwrap().remove(&fid);
        self.inner.clunk(fid).await
    }
}

#[tokio::test]
async fn unidentified_allocation_never_becomes_a_clean_budget_pause_or_cancellation() {
    for cancelled in [false, true] {
        let cancel = CancellationToken::new();
        let mock = alan_llm::MockLlmProvider::new();
        let inner = alan_llmfs::LlmFs::new();
        inner.register_connection("default", Box::new(mock.clone()));
        let llmfs = Arc::new(UnidentifiedAllocationFs {
            inner,
            allocators: Default::default(),
            allocations: AtomicUsize::new(0),
            cancel: cancelled.then(|| cancel.clone()),
        });
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs.clone()),
            alan_kernel::Access::ReadWrite,
        );
        ns.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        let mut state = runtime_state_with_environment(NamespaceRuntimeEnvironment::new(
            InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
            "/agent/1",
            "default",
        ));
        state.core_config.memory.enabled = false;
        state.core_config.task_budget.model_calls = std::num::NonZeroU64::new(1).unwrap();
        let input = Submission::new(Op::Turn {
            parts: vec![alan_agent_protocol::ContentPart::text("hello")],
            context: None,
        });
        state.machine.accept_submission(input.id.clone());
        let mut events = Vec::new();
        let result = handle_submission_with_cancel(
            &mut state,
            input,
            &mut |event| {
                events.push(event);
                async {}
            },
            &cancel,
        )
        .await;
        assert!(result.unwrap_err().is::<NamespaceOperationUncertainty>());
        assert!(state.machine.task_budget_wait.is_none());
        assert!(!state.machine.has_pending_interaction());
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::TurnCompleted { .. }))
        );
        let spent = state.machine.task_budget.as_ref().unwrap().snapshot().await;
        assert_eq!((spent.model_calls, spent.tool_calls), (1, 0));
        assert!(!spent.foreground_complete);
        assert_eq!(llmfs.allocations.load(Ordering::SeqCst), 1);
        assert!(mock.recorded_requests().is_empty());
    }
}

pub(super) async fn held_descendant(
    parent: TaskBudget,
) -> (
    Arc<Notify>,
    Arc<Notify>,
    tokio::task::JoinHandle<Result<()>>,
) {
    let child = TaskBudget::start_with_parent(
        "held-descendant".into(),
        parent.snapshot().await.limits,
        None,
        Some(parent),
    )
    .await
    .unwrap();
    child.reserve(Admission::Tool).await.unwrap();
    let started = Arc::new(Notify::new());
    let cleanup = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let operation = tokio::spawn({
        let started = started.clone();
        let cleanup = cleanup.clone();
        let release = release.clone();
        async move {
            child
                .run(&CancellationToken::new(), |cancel| async move {
                    started.notify_one();
                    cancel.cancelled().await;
                    cleanup.notify_one();
                    release.notified().await;
                    Ok(())
                })
                .await
        }
    });
    started.notified().await;
    (cleanup, release, operation)
}

#[tokio::test]
async fn answer_fallback_and_cancel_completion_waits_for_descendant_cleanup() {
    for (name, answer, interrupt) in [
        ("answer", "parent completed", false),
        ("fallback", "", false),
        ("cancel", "unused", true),
    ] {
        let mut state = runtime_state_with_provider(DelayedMockProvider::new(
            std::time::Duration::ZERO,
            answer,
        ));
        state.core_config.memory.enabled = false;
        let input = Submission {
            id: name.into(),
            intent: alan_agent_protocol::InputIntent::ForceAgent,
            op: if interrupt {
                Op::Interrupt
            } else {
                Op::Turn {
                    parts: vec![alan_agent_protocol::ContentPart::text("Say finished")],
                    context: None,
                }
            },
        };
        state.machine.accept_submission(input.id.clone());
        crate::runtime::task_budget::begin(&mut state)
            .await
            .unwrap();
        let budget = state.machine.task_budget.clone().unwrap();
        let (cleanup, release, child) = held_descendant(budget.clone()).await;
        let completions = Arc::new(AtomicUsize::new(0));
        let mut emit = |event| {
            if matches!(event, Event::TurnCompleted { .. }) {
                completions.fetch_add(1, Ordering::SeqCst);
            }
            async {}
        };
        let cancel = CancellationToken::new();
        let transition = handle_submission_with_cancel(&mut state, input, &mut emit, &cancel);
        let inspect = async {
            cleanup.notified().await;
            let early = completions.load(Ordering::SeqCst);
            release.notify_one();
            early
        };
        let (result, early) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(transition, inspect)
        })
        .await
        .unwrap();
        result.unwrap();
        assert!(child.await.unwrap().is_err());
        assert_eq!(early, 0, "{name}: completion preceded child cleanup");
        assert_eq!(completions.load(Ordering::SeqCst), 1, "{name}");
        let snapshot = budget.snapshot().await;
        assert!(!snapshot.descendants_active);
        assert_eq!(
            snapshot.phase,
            if interrupt {
                Phase::Completed
            } else {
                Phase::Paused
            }
        );
    }
}
