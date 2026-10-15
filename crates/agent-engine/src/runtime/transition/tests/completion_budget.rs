use super::*;
use crate::agent_machine::task_budget::{Admission, Phase, TaskBudget};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Notify;

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
