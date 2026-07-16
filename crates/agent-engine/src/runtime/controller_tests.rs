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
