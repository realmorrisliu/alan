use super::*;

#[test]
fn runtime_handle_is_cloneable() {
    let (submission_tx, _submission_rx) = mpsc::channel(10);
    let handle = RuntimeHandle::new(submission_tx, None);

    let cloned = handle.clone();

    drop(cloned);
    drop(handle);
}

#[test]
fn runtime_handle_exposes_submission_channel() {
    let (submission_tx, _submission_rx) = mpsc::channel::<Submission>(10);
    let handle = RuntimeHandle::new(submission_tx, None);

    assert!(!handle.submission_tx.is_closed());
}

#[tokio::test]
async fn runtime_handle_shutdown_requires_channel() {
    let (submission_tx, _submission_rx) = mpsc::channel::<Submission>(10);
    let handle = RuntimeHandle::new(submission_tx, None);

    assert!(handle.shutdown().await.is_err());
}

#[tokio::test]
async fn runtime_handle_shutdown_signals_channel() {
    let (submission_tx, _submission_rx) = mpsc::channel::<Submission>(10);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let handle = RuntimeHandle::new(submission_tx, Some(shutdown_tx));

    handle.shutdown().await.unwrap();

    assert!(shutdown_rx.recv().await.is_some());
}

#[tokio::test]
async fn action_retention_waits_for_durable_journal() {
    let (submission_tx, _) = mpsc::channel(1);
    let handle = RuntimeHandle::new(submission_tx, None);
    assert!(
        handle
            .record_action_retention("/agent/1", "a0", "age_limit")
            .await
            .is_err()
    );
    let dir = tempfile::TempDir::new().unwrap();
    let recorder = RolloutRecorder::new_in_dir("/proc/1", "test", dir.path())
        .await
        .unwrap();
    let path = recorder.path().to_path_buf();
    assert!(handle.recorder.set(Some(recorder)).is_ok());
    handle
        .clone()
        .record_action_retention("/agent/1", "a0", "age_limit")
        .await
        .unwrap();
    let items = RolloutRecorder::load_history(&path).await.unwrap();
    assert!(items.iter().any(|item| matches!(item, RolloutItem::Event(event)
        if event.event_type == "agent_action_retention_v1"
        && event.payload == serde_json::json!({"agent_path":"/agent/1", "action_id":"a0", "cause":"age_limit"}))));
}

#[tokio::test]
async fn shutdown_drains_and_joins_the_rollout_writer() {
    let dir = tempfile::TempDir::new().unwrap();
    let recorder = RolloutRecorder::new_in_dir("/proc/1", "test", dir.path())
        .await
        .unwrap();
    let path = recorder.path().to_path_buf();
    let recorder_clone = recorder.clone();
    for index in 0..256 {
        recorder
            .record_event_item_nowait(EventRecord {
                event_type: "queued_before_shutdown".into(),
                payload: serde_json::json!({"index": index}),
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
            .unwrap();
    }

    let (submission_tx, _) = mpsc::channel(1);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
    let (ready_tx, ready_rx) = oneshot::channel();
    drop(ready_tx);
    let task = tokio::spawn(async move {
        shutdown_rx.recv().await;
    });
    let recorder_state = Arc::new(OnceLock::new());
    recorder_state.set(Some(recorder)).unwrap();
    let controller =
        RuntimeController::spawned(submission_tx, shutdown_tx, task, ready_rx, recorder_state);

    controller.shutdown().await.unwrap();

    assert!(
        recorder_clone
            .record_nowait(RolloutItem::Event(EventRecord {
                event_type: "after_shutdown".into(),
                payload: serde_json::Value::Null,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }))
            .is_err()
    );
    let items = RolloutRecorder::load_history(&path).await.unwrap();
    let queued = items
        .iter()
        .filter(|item| {
            matches!(item, RolloutItem::Event(event)
            if event.event_type == "queued_before_shutdown")
        })
        .count();
    assert_eq!(queued, 256);
}
