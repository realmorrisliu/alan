use std::{
    num::NonZeroU64,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use super::*;
use crate::agent_machine::task_budget::{
    Exhausted, Extension, Snapshot, TaskBudgetLimits, recover,
};
use crate::rollout::EventRecord;
use tokio_util::sync::CancellationToken;

fn limits(seconds: u64, tools: u64, models: u64) -> TaskBudgetLimits {
    TaskBudgetLimits {
        execution_seconds: NonZeroU64::new(seconds).unwrap(),
        tool_calls: NonZeroU64::new(tools).unwrap(),
        model_calls: NonZeroU64::new(models).unwrap(),
    }
}

async fn start(binding: &TaskBudgetBinding, id: &str, limits: TaskBudgetLimits) -> TaskBudget {
    let mut budget = TaskBudget::start_with_parent(id.into(), limits, None, binding.parent())
        .await
        .unwrap();
    binding.bind(&mut budget).await.unwrap();
    budget
}

#[tokio::test]
async fn siblings_share_parent_admission_and_local_extension_cannot_widen_it() {
    let binding = TaskBudgetBinding::default();
    assert!(binding.child().is_err());
    let parent = start(&binding, "parent", limits(10, 8, 2)).await;
    let child_a = start(&binding.child().unwrap(), "child-a", limits(10, 8, 8)).await;
    let child_b = start(&binding.child().unwrap(), "child-b", limits(10, 8, 8)).await;
    child_a.reserve(Admission::Model).await.unwrap();
    child_b.reserve(Admission::Model).await.unwrap();
    let error = child_a.reserve(Admission::Tool).await.unwrap_err();
    assert!(error.to_string().contains("Parent task budget"));
    assert!(
        !error.is::<Exhausted>(),
        "must not offer an irrelevant child-local extension"
    );
    child_a
        .extend(&Extension {
            model_calls: 20,
            ..Default::default()
        })
        .await
        .unwrap();
    child_a.resume().await.unwrap();
    assert!(child_a.reserve(Admission::Model).await.is_err());
    let snapshot = parent.snapshot().await;
    assert_eq!((snapshot.model_calls, snapshot.tool_calls), (2, 0));
    assert_eq!(child_a.snapshot().await.model_calls, 1);
    assert_eq!(child_b.snapshot().await.model_calls, 1);
}

#[tokio::test]
async fn concurrent_grandchildren_charge_each_ancestor_once_without_overspending() {
    let binding = TaskBudgetBinding::default();
    let parent = start(&binding, "parent", limits(10, 7, 8)).await;
    parent.reserve(Admission::Tool).await.unwrap();
    let child_binding = binding.child().unwrap();
    let child = start(&child_binding, "child", limits(10, 20, 20)).await;
    child.reserve(Admission::Tool).await.unwrap();
    let grandchild = start(
        &child_binding.child().unwrap(),
        "grandchild",
        limits(10, 20, 20),
    )
    .await;
    let mut jobs = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let budget = grandchild.clone();
        jobs.spawn(async move { budget.reserve(Admission::Tool).await });
    }
    let mut admitted = 0;
    while let Some(result) = jobs.join_next().await {
        admitted += usize::from(result.unwrap().is_ok());
    }
    assert_eq!(admitted, 5);
    assert_eq!(parent.snapshot().await.tool_calls, 7);
    assert_eq!(child.snapshot().await.tool_calls, 6);
    let snapshot = grandchild.snapshot().await;
    assert_eq!(snapshot.tool_calls, 5);
    assert_eq!(snapshot.parent_tasks, ["parent", "child"]);
}

#[tokio::test]
async fn child_capture_never_moves_to_a_new_parent_task_or_background_allowance() {
    let binding = TaskBudgetBinding::default();
    let old = start(&binding, "old", limits(10, 8, 8)).await;
    let captured = binding.child().unwrap();
    old.finish_foreground().await.unwrap();
    let new = start(&binding, "new", limits(10, 8, 8)).await;
    let child = start(&captured, "child", limits(10, 8, 8)).await;
    assert!(child.reserve(Admission::Model).await.is_err());
    old.resume().await.unwrap(); // A captured deferred job may still use the old ledger.
    assert!(child.reserve(Admission::Tool).await.is_err());
    assert_eq!(child.snapshot().await.parent_tasks, ["old"]);
    assert_eq!(new.snapshot().await.model_calls, 0);
    assert_eq!(old.snapshot().await.tool_calls, 0);
}

#[tokio::test(start_paused = true)]
async fn parent_pause_cancels_inflight_child_and_awaits_cleanup() {
    let binding = TaskBudgetBinding::default();
    let parent = start(&binding, "parent", limits(10, 8, 8)).await;
    let child = start(&binding.child().unwrap(), "child", limits(10, 8, 8)).await;
    child.reserve(Admission::Model).await.unwrap();
    let cleaned = Arc::new(AtomicBool::new(false));
    let child_cleaned = cleaned.clone();
    let cancel = CancellationToken::new();
    let operation = child.run(&cancel, |cancel| async move {
        cancel.cancelled().await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        child_cleaned.store(true, Ordering::SeqCst);
        Ok(())
    });
    let pause = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        parent.settle(false).await.unwrap();
    };
    let (result, _) = tokio::join!(operation, pause);
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Parent task budget stopped")
    );
    assert!(cleaned.load(Ordering::SeqCst));
    assert_eq!(parent.snapshot().await.elapsed_millis, 3000);
    assert_eq!(parent.snapshot().await.model_calls, 1);
    assert!(child.reserve(Admission::Tool).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn shorter_parent_deadline_is_enforced_without_a_child_extension_prompt() {
    let binding = TaskBudgetBinding::default();
    let parent = start(&binding, "parent", limits(2, 8, 8)).await;
    let child = start(&binding.child().unwrap(), "child", limits(10, 8, 8)).await;
    child.reserve(Admission::Model).await.unwrap();
    let error = child
        .run(&CancellationToken::new(), |cancel| async move {
            cancel.cancelled().await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok(())
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Parent task budget"));
    assert!(!error.is::<Exhausted>());
    assert_eq!(parent.snapshot().await.elapsed_millis, 4000);
    assert_eq!(child.snapshot().await.elapsed_millis, 4000);
}

#[tokio::test]
async fn child_recovery_requires_the_original_ancestry_and_retains_spending() {
    let binding = TaskBudgetBinding::default();
    let parent = start(&binding, "parent", limits(10, 8, 8)).await;
    let child_binding = binding.child().unwrap();
    let child = start(&child_binding, "child", limits(10, 8, 8)).await;
    child.reserve(Admission::Model).await.unwrap();
    child.settle(false).await.unwrap();
    let snapshot = child.snapshot().await;
    let event = |snapshot: &Snapshot| EventRecord {
        event_type: super::super::EVENT_TYPE.into(),
        payload: serde_json::to_value(snapshot).unwrap(),
        timestamp: "test".into(),
    };
    let recovered = recover(&[event(&snapshot)]).unwrap().unwrap();
    let mut restored = TaskBudget::restore(recovered, None).unwrap();
    assert!(restored.resume().await.is_err());
    assert!(
        TaskBudgetBinding::default()
            .bind(&mut restored)
            .await
            .is_err()
    );
    let other = TaskBudgetBinding::default();
    start(&other, "other", limits(10, 8, 8)).await;
    assert!(other.child().unwrap().bind(&mut restored).await.is_err());
    child_binding.bind(&mut restored).await.unwrap();
    restored.resume().await.unwrap();
    restored.reserve(Admission::Tool).await.unwrap();
    assert_eq!(restored.snapshot().await.model_calls, 1);
    assert_eq!(parent.snapshot().await.model_calls, 1);
    assert_eq!(parent.snapshot().await.tool_calls, 1);
    let mut changed = snapshot.clone();
    changed.parent_tasks.clear();
    assert!(recover(&[event(&snapshot), event(&changed)]).is_err());
}

#[tokio::test(start_paused = true)]
async fn terminal_parent_settlement_waits_for_child_cleanup_before_recording_completion() {
    for complete in [false, true] {
        let binding = TaskBudgetBinding::default();
        let parent = start(&binding, "parent", limits(10, 8, 8)).await;
        let child = start(&binding.child().unwrap(), "child", limits(10, 8, 8)).await;
        let cancel = CancellationToken::new();
        let operation = child.run(&cancel, |cancel| async move {
            cancel.cancelled().await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok(())
        });
        let finish = async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if complete {
                parent.settle(true).await.unwrap();
            } else {
                parent.finish_foreground().await.unwrap();
            }
        };
        let observe = async {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let snapshot = parent.snapshot().await;
            assert_eq!(snapshot.phase, Phase::Paused);
            assert!(!snapshot.foreground_complete);
            assert!(snapshot.descendants_active);
        };
        let (result, _, _) = tokio::join!(operation, finish, observe);
        assert!(result.is_err());
        let snapshot = parent.snapshot().await;
        assert_eq!(snapshot.elapsed_millis, 3000);
        assert!(!snapshot.descendants_active);
        assert_eq!(
            snapshot.phase,
            if complete {
                Phase::Completed
            } else {
                Phase::Paused
            }
        );
        assert_eq!(snapshot.foreground_complete, !complete);
    }
}

#[tokio::test]
async fn durable_pause_with_unsettled_descendants_recovers_a_conservative_time_bound() {
    use crate::rollout::{RolloutItem, RolloutRecorder};
    let temp = tempfile::tempdir().unwrap();
    let recorder = RolloutRecorder::new_in_dir("/proc/parent", "mock", temp.path())
        .await
        .unwrap();
    let binding = TaskBudgetBinding::default();
    let mut parent = TaskBudget::start_with_parent(
        "parent".into(),
        limits(30, 8, 8),
        Some(recorder.clone()),
        None,
    )
    .await
    .unwrap();
    binding.bind(&mut parent).await.unwrap();
    let child = start(&binding.child().unwrap(), "child", limits(30, 8, 8)).await;
    child.reserve(Admission::Model).await.unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let executing = Arc::new(tokio::sync::Notify::new());
    let operation = tokio::spawn({
        let entered = entered.clone();
        let release = release.clone();
        let executing = executing.clone();
        async move {
            child
                .run(&CancellationToken::new(), |cancel| async move {
                    executing.notify_one();
                    cancel.cancelled().await;
                    entered.notify_one();
                    release.notified().await;
                    Ok(())
                })
                .await
        }
    });
    executing.notified().await;
    let settling = tokio::spawn({
        let parent = parent.clone();
        async move { parent.settle(false).await }
    });
    entered.notified().await;
    let history = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    let events: Vec<_> = history
        .into_iter()
        .filter_map(|item| match item {
            RolloutItem::Event(event) => Some(event),
            _ => None,
        })
        .collect();
    let pending = recover(&events).unwrap().unwrap();
    assert_eq!(pending.phase, Phase::Paused);
    assert!(pending.descendants_active);
    let recovered = TaskBudget::restore(pending, None).unwrap().snapshot().await;
    assert!(recovered.recovered_time_upper_bound);
    assert_eq!(recovered.elapsed_millis, 30_000);
    assert_eq!(recovered.model_calls, 1);
    assert!(!recovered.foreground_complete);
    release.notify_one();
    assert!(operation.await.unwrap().is_err());
    settling.await.unwrap().unwrap();
    recorder.close().await.unwrap();
    let snapshot = parent.snapshot().await;
    assert!(!snapshot.descendants_active);
    assert!(!snapshot.recovered_time_upper_bound);
}

#[tokio::test]
async fn failed_child_admission_acknowledgement_never_refunds_ancestor_spending() {
    use crate::rollout::RolloutRecorder;
    let binding = TaskBudgetBinding::default();
    let parent = start(&binding, "parent", limits(30, 8, 8)).await;
    let temp = tempfile::tempdir().unwrap();
    let recorder = RolloutRecorder::new_in_dir("/proc/child", "mock", temp.path())
        .await
        .unwrap();
    let child = TaskBudget::start_with_parent(
        "child".into(),
        limits(30, 8, 8),
        Some(recorder.clone()),
        binding.child().unwrap().parent(),
    )
    .await
    .unwrap();
    recorder.close().await.unwrap();
    assert!(child.reserve(Admission::Model).await.is_err());
    assert_eq!(parent.snapshot().await.model_calls, 1);
    assert!(child.reserve(Admission::Model).await.is_err());
    assert_eq!(parent.snapshot().await.model_calls, 1);
    assert_eq!(child.snapshot().await.model_calls, 0);
}

#[tokio::test(start_paused = true)]
async fn completion_drain_keeps_final_io_time_charged_and_recovers_conservatively() {
    let binding = TaskBudgetBinding::default();
    let parent = start(&binding, "parent", limits(10, 8, 8)).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    parent.prepare_completion().await.unwrap();
    assert!(parent.reserve(Admission::Tool).await.is_err());
    tokio::time::advance(Duration::from_secs(4)).await;
    let snapshot = parent.snapshot().await;
    assert_eq!(snapshot.phase, Phase::Settling);
    assert_eq!(snapshot.elapsed_millis, 5000);
    let recovered = TaskBudget::restore(snapshot, None)
        .unwrap()
        .snapshot()
        .await;
    assert_eq!(recovered.phase, Phase::Paused);
    assert_eq!(recovered.elapsed_millis, 10_000);
    assert!(recovered.recovered_time_upper_bound);
    assert!(!recovered.foreground_complete);
    parent.finish_foreground().await.unwrap();
    tokio::time::advance(Duration::from_secs(600)).await;
    let snapshot = parent.snapshot().await;
    assert_eq!(snapshot.phase, Phase::Paused);
    assert_eq!(snapshot.elapsed_millis, 5000);
    assert!(snapshot.foreground_complete);
}
