use super::*;

fn limits(tools: u64, models: u64) -> TaskBudgetLimits {
    TaskBudgetLimits {
        execution_seconds: NonZeroU64::new(10).unwrap(),
        tool_calls: NonZeroU64::new(tools).unwrap(),
        model_calls: NonZeroU64::new(models).unwrap(),
    }
}

fn extension(tools: u64, models: u64) -> Extension {
    Extension {
        execution_seconds: 10,
        tool_calls: tools,
        model_calls: models,
    }
}

fn record(snapshot: &Snapshot) -> EventRecord {
    EventRecord {
        event_type: EVENT_TYPE.into(),
        payload: serde_json::to_value(snapshot).unwrap(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

#[tokio::test]
async fn concurrent_admission_cannot_spend_the_same_allowance() {
    let budget = TaskBudget::start("task".into(), limits(3, 2), None)
        .await
        .unwrap();
    let mut attempts = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let budget = budget.clone();
        attempts.spawn(async move { budget.reserve(Admission::Tool).await });
    }
    let mut admitted = 0;
    while let Some(result) = attempts.join_next().await {
        match result.unwrap() {
            Ok(()) => admitted += 1,
            Err(error) => assert_eq!(
                error.downcast_ref::<Exhausted>().unwrap().dimension,
                Dimension::ToolCalls
            ),
        }
    }
    assert_eq!(admitted, 3);
    assert!(
        budget
            .reserve(Admission::Model)
            .await
            .unwrap_err()
            .is::<Exhausted>()
    );
    let models = TaskBudget::start("models".into(), limits(3, 2), None)
        .await
        .unwrap();
    models.reserve(Admission::Model).await.unwrap();
    models.reserve(Admission::Model).await.unwrap();
    assert!(
        models
            .reserve(Admission::Model)
            .await
            .unwrap_err()
            .is::<Exhausted>()
    );
    let snapshot = budget.snapshot().await;
    assert_eq!((snapshot.tool_calls, snapshot.model_calls), (3, 0));
    assert!(
        models
            .reserve(Admission::Tool)
            .await
            .unwrap_err()
            .is::<Exhausted>()
    );
    models
        .run(&CancellationToken::new(), |_| async { Ok(()) })
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn paused_wait_does_not_spend_execution_time_and_extension_never_refunds() {
    let budget = TaskBudget::start("task".into(), limits(1, 1), None)
        .await
        .unwrap();
    budget.reserve(Admission::Tool).await.unwrap();
    tokio::time::advance(Duration::from_secs(4)).await;
    budget.settle(false).await.unwrap();
    tokio::time::advance(Duration::from_secs(600)).await;
    assert_eq!(budget.snapshot().await.elapsed_millis, 4000);
    assert!(budget.remaining_time().await.is_err());
    budget.extend(&extension(2, 3)).await.unwrap();
    budget.resume().await.unwrap();
    let snapshot = budget.snapshot().await;
    assert_eq!((snapshot.tool_calls, snapshot.elapsed_millis), (1, 4000));
    assert_eq!(snapshot.limits.tool_calls.get(), 3);
    assert_eq!(snapshot.limits.model_calls.get(), 4);
    assert_eq!(
        budget.remaining_time().await.unwrap(),
        Duration::from_secs(16)
    );
}

#[tokio::test(start_paused = true)]
async fn time_limit_cancels_and_awaits_cleanup_before_reporting_exhaustion() {
    let budget = TaskBudget::start("task".into(), limits(1, 1), None)
        .await
        .unwrap();
    let cleaned = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cleanup = cleaned.clone();
    let error = budget
        .run(&CancellationToken::new(), |cancel| async move {
            cancel.cancelled().await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            cleanup.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<Exhausted>().unwrap().dimension,
        Dimension::Time
    );
    assert!(cleaned.load(std::sync::atomic::Ordering::SeqCst));
    assert!(budget.snapshot().await.elapsed_millis >= 12000);
}

#[tokio::test]
async fn cancelled_parent_does_not_start_an_operation() {
    let budget = TaskBudget::start("task".into(), limits(1, 1), None)
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = budget
        .run::<(), _, _>(&cancel, |_| async { panic!("operation must not run") })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("cancelled before admission"));
}

#[tokio::test]
async fn cancelling_an_unacknowledged_admission_fails_closed_even_if_writer_commits() {
    for persist_first in [false, true] {
        let directory = tempfile::TempDir::new().unwrap();
        let recorder = RolloutRecorder::new_in_dir("/proc/1", "test", directory.path())
            .await
            .unwrap();
        let mut budget = TaskBudget::start("task".into(), limits(2, 2), Some(recorder.clone()))
            .await
            .unwrap();
        let (gate, mut observed, release) = recorder.batch_gate(persist_first, |items| {
            items.iter().any(
                |item| matches!(item, RolloutItem::Event(event) if event.event_type == EVENT_TYPE),
            )
        });
        budget.recorder = Some(gate.clone());
        let admission = budget.clone();
        let pending = tokio::spawn(async move { admission.reserve(Admission::Model).await });
        observed.recv().await.unwrap();
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        assert!(
            budget
                .reserve(Admission::Tool)
                .await
                .unwrap_err()
                .to_string()
                .contains("uncertain")
        );
        release.send(()).unwrap();
        gate.flush().await.unwrap();
        let items = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        assert!(items.iter().any(|item| matches!(item,
            RolloutItem::Event(event) if event.event_type == EVENT_TYPE && event.payload["model_calls"] == 1)));
        assert!(budget.resume().await.is_err());
    }
}

#[tokio::test]
async fn failed_flush_does_not_admit_work_or_allow_a_retry() {
    let directory = tempfile::TempDir::new().unwrap();
    let recorder = RolloutRecorder::new_in_dir("/proc/1", "test", directory.path())
        .await
        .unwrap();
    let mut budget = TaskBudget::start("task".into(), limits(2, 2), Some(recorder.clone()))
        .await
        .unwrap();
    budget.recorder = Some(recorder.flush_failure_probe().await);
    assert!(budget.reserve(Admission::Model).await.is_err());
    assert!(
        budget
            .reserve(Admission::Tool)
            .await
            .unwrap_err()
            .to_string()
            .contains("uncertain")
    );
    assert!(budget.resume().await.is_err());
    assert!(budget.extend(&extension(1, 1)).await.is_err());
    assert!(budget.remaining_time().await.is_err());
}

#[tokio::test]
async fn recovered_running_interval_requires_explicit_time_extension() {
    let budget = TaskBudget::start("task".into(), limits(2, 2), None)
        .await
        .unwrap();
    budget.reserve(Admission::Model).await.unwrap();
    let recovered = TaskBudget::restore(budget.snapshot().await, None).unwrap();
    let snapshot = recovered.snapshot().await;
    assert!(snapshot.recovered_time_upper_bound);
    assert_eq!((snapshot.model_calls, snapshot.elapsed_millis), (1, 10000));
    recovered.resume().await.unwrap();
    assert!(
        recovered
            .reserve(Admission::Tool)
            .await
            .unwrap_err()
            .is::<Exhausted>()
    );
    recovered.extend(&extension(1, 1)).await.unwrap();
    recovered.resume().await.unwrap();
    recovered.reserve(Admission::Tool).await.unwrap();
    assert_eq!(recovered.snapshot().await.model_calls, 1);
}

#[tokio::test]
async fn real_rollout_recovery_keeps_spending_and_extension() {
    let directory = tempfile::TempDir::new().unwrap();
    let recorder = RolloutRecorder::new_in_dir("/proc/1", "test", directory.path())
        .await
        .unwrap();
    let budget = TaskBudget::start("task".into(), limits(1, 1), Some(recorder.clone()))
        .await
        .unwrap();
    budget.reserve(Admission::Model).await.unwrap();
    budget.settle(false).await.unwrap();
    budget.extend(&extension(2, 2)).await.unwrap();
    let events = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap()
        .into_iter()
        .filter_map(|item| {
            if let RolloutItem::Event(event) = item {
                Some(event)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let snapshot = recover(&events).unwrap().unwrap();
    assert_eq!(snapshot, budget.snapshot().await);
    assert_eq!(snapshot.model_calls, 1);
    assert_eq!(snapshot.limits.model_calls.get(), 3);
    let restored = TaskBudget::restore(snapshot, None).unwrap();
    restored.resume().await.unwrap();
    restored.reserve(Admission::Model).await.unwrap();
    assert_eq!(restored.snapshot().await.model_calls, 2);
}

#[tokio::test]
async fn machine_recovery_across_new_processes_keeps_the_original_task_allowance() {
    let directory = tempfile::TempDir::new().unwrap();
    let mut machine = crate::agent_machine::AgentMachine::new_with_recorder_in_dir(
        "/proc/1",
        "test",
        directory.path(),
    )
    .await
    .unwrap();
    let budget = TaskBudget::start("original-task".into(), limits(3, 3), machine.recorder())
        .await
        .unwrap();
    budget.reserve(Admission::Model).await.unwrap();
    budget.settle(false).await.unwrap();
    machine.task_budget = Some(budget);
    let mut path = machine.rollout_path().unwrap().clone();
    for (pid, expected_calls) in [("/proc/2", 1), ("/proc/3", 2)] {
        let recovered = crate::agent_machine::AgentMachine::load_from_rollout_in_dir(
            &path,
            pid,
            "test",
            directory.path(),
        )
        .await
        .unwrap();
        let budget = recovered.task_budget.as_ref().unwrap();
        let snapshot = budget.snapshot().await;
        assert_eq!(snapshot.task_id, "original-task");
        assert_eq!(snapshot.model_calls, expected_calls);
        assert_eq!(snapshot.limits.model_calls.get(), 3);
        assert_eq!(snapshot.phase, Phase::Paused);
        budget.resume().await.unwrap();
        budget.reserve(Admission::Model).await.unwrap();
        if expected_calls == 2 {
            assert!(
                budget
                    .reserve(Admission::Tool)
                    .await
                    .unwrap_err()
                    .is::<Exhausted>()
            );
        }
        budget.settle(false).await.unwrap();
        path = recovered.rollout_path().unwrap().clone();
    }
}

#[tokio::test]
async fn extending_only_exhausted_dimension_leaves_other_limits_and_spending_unchanged() {
    let budget = TaskBudget::start("task".into(), limits(2, 1), None)
        .await
        .unwrap();
    budget.reserve(Admission::Model).await.unwrap();
    budget.settle(false).await.unwrap();
    let before = budget.snapshot().await;
    assert!(budget.extend(&Extension::default()).await.is_err());
    let overflow = Extension {
        model_calls: u64::MAX,
        ..Default::default()
    };
    assert!(budget.extend(&overflow).await.is_err());
    assert_eq!(budget.snapshot().await, before);
    budget
        .extend(&Extension {
            model_calls: 2,
            ..Default::default()
        })
        .await
        .unwrap();
    let after = budget.snapshot().await;
    assert_eq!(
        after.limits.execution_seconds,
        before.limits.execution_seconds
    );
    assert_eq!(after.limits.tool_calls, before.limits.tool_calls);
    assert_eq!(after.limits.model_calls.get(), 3);
    assert_eq!(
        (after.model_calls, after.tool_calls, after.elapsed_millis),
        (before.model_calls, before.tool_calls, before.elapsed_millis)
    );
    budget.resume().await.unwrap();
    budget.reserve(Admission::Tool).await.unwrap();
}

#[tokio::test]
async fn recovery_refuses_rollback_identity_reuse_and_completed_reactivation() {
    let budget = TaskBudget::start("task".into(), limits(3, 3), None)
        .await
        .unwrap();
    let initial = budget.snapshot().await;
    budget.reserve(Admission::Tool).await.unwrap();
    let spent = budget.snapshot().await;
    assert!(recover(&[record(&spent), record(&initial)]).is_err());
    let mut other = initial.clone();
    other.task_id = "other".into();
    assert!(recover(&[record(&spent), record(&other)]).is_err());
    budget.settle(true).await.unwrap();
    let closed = budget.snapshot().await;
    assert!(recover(&[record(&closed)]).unwrap().is_none());
    assert!(recover(&[record(&closed), record(&spent)]).is_err());
    other.phase = Phase::Completed;
    assert!(recover(&[record(&closed), record(&other), record(&initial)]).is_err());
    assert!(budget.resume().await.is_err());
    assert!(budget.extend(&extension(1, 1)).await.is_err());
    assert!(budget.settle(false).await.is_err());
}

#[test]
fn config_and_recovery_reject_zero_overflow_or_invalid_evidence() {
    assert!(
        serde_json::from_value::<TaskBudgetLimits>(serde_json::json!({"tool_calls": 0})).is_err()
    );
    let snapshot = Snapshot {
        version: 1,
        task_id: "task".into(),
        limits: limits(1, 1),
        tool_calls: 2,
        model_calls: 0,
        elapsed_millis: 0,
        phase: Phase::Paused,
        foreground_complete: false,
        recovered_time_upper_bound: false,
    };
    assert!(TaskBudget::restore(snapshot.clone(), None).is_err());
    let mut overflow = snapshot;
    overflow.tool_calls = 0;
    overflow.limits.execution_seconds = NonZeroU64::new(u64::MAX).unwrap();
    assert!(TaskBudget::restore(overflow.clone(), None).is_err());
    assert!(recover(&[record(&overflow)]).is_err());
}

#[tokio::test]
async fn deferred_old_task_spending_never_replaces_the_new_foreground_budget() {
    let old = TaskBudget::start("old".into(), limits(3, 3), None)
        .await
        .unwrap();
    old.reserve(Admission::Model).await.unwrap();
    old.finish_foreground().await.unwrap();
    let old_foreground = old.snapshot().await;
    let new = TaskBudget::start("new".into(), limits(3, 3), None)
        .await
        .unwrap();
    new.reserve(Admission::Model).await.unwrap();
    let current = new.snapshot().await;
    old.resume().await.unwrap();
    old.reserve(Admission::Model).await.unwrap();
    old.settle(false).await.unwrap();
    let old_retry = old.snapshot().await;
    assert_eq!(old_retry.model_calls, 2);
    let events = [
        record(&old_foreground),
        record(&current),
        record(&old_retry),
    ];
    assert_eq!(recover(&events).unwrap(), Some(current.clone()));
    old.resume().await.unwrap();
    old.reserve(Admission::Model).await.unwrap();
    old.settle(true).await.unwrap();
    let mut events = events.to_vec();
    events.push(record(&old.snapshot().await));
    assert_eq!(recover(&events).unwrap(), Some(current));
    assert_eq!(new.snapshot().await.model_calls, 1);
}

#[tokio::test(start_paused = true)]
async fn deadline_preserves_uncertain_cleanup_in_the_error_chain() {
    #[derive(Debug, thiserror::Error)]
    #[error("cleanup acknowledgement uncertain")]
    struct CleanupUncertain;
    let budget = TaskBudget::start("uncertain".into(), limits(1, 1), None)
        .await
        .unwrap();
    let error = budget
        .run(&CancellationToken::new(), |stop| async move {
            stop.cancelled().await;
            Err::<(), _>(CleanupUncertain.into())
        })
        .await
        .unwrap_err();
    assert!(error.is::<Exhausted>());
    assert!(error.is::<CleanupUncertain>());
}
