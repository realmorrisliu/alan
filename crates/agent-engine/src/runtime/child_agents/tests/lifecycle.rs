use super::*;
use crate::runtime::child_runs::ChildRunStatus;

#[tokio::test]
async fn child_runtime_join_keeps_running_while_activity_file_is_fresh() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("finished after file heartbeat");
    let parent = make_parent_state(&temp, requests.clone(), response.clone());
    let spec = launch_spec(temp.path().join("definition"));
    let mut child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
        Ok(LlmClient::new(
            RecordingProvider::new(requests.clone(), response.clone())
                .with_delay(Duration::from_millis(250)),
        ))
    })
    .await
    .unwrap();
    child.set_timeout_for_test(Duration::from_millis(200));
    let environment = child.process_environment_for_test().clone();
    tokio::spawn(async move {
        for _ in 0..5 {
            crate::runtime::ui_surfaces::heartbeat(&environment)
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(35)).await;
        }
    });

    let result = child.join().await.unwrap();
    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    assert_eq!(result.output_text, "finished after file heartbeat");
}

#[tokio::test]
async fn spawn_child_runtime_cancellable_aborts_pre_cancelled_launch() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("This should never run.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let spec = launch_spec(root_dir);
    let cancel = CancellationToken::new();
    cancel.cancel();

    let err = match spawn_child_runtime_cancellable(&parent, spec, &cancel).await {
        Ok(_) => {
            panic!("pre-cancelled launch should abort before returning a child controller")
        }
        Err(err) => err,
    };

    assert!(
        err.to_string()
            .contains("Child Agent Process launch cancelled")
    );
}

#[test]
fn child_run_status_for_launch_error_maps_cancelled_launches_to_cancelled() {
    let cancelled = anyhow::anyhow!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
    let failed = anyhow::anyhow!("Failed to submit initial child Agent Process turn");

    assert_eq!(
        child_run_status_for_launch_error(&cancelled),
        ChildRunStatus::Cancelled
    );
    assert_eq!(
        child_run_status_for_launch_error(&failed),
        ChildRunStatus::Failed
    );
}

#[tokio::test]
async fn child_runtime_join_returns_promptly_after_timeout() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("This should not finish before timeout.");
    let parent = make_parent_state(&temp, requests.clone(), response.clone());
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.launch.timeout_secs = Some(1);

    let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
        Ok(LlmClient::new(
            RecordingProvider::new(requests.clone(), response.clone())
                .with_delay(Duration::from_secs(30)),
        ))
    })
    .await
    .unwrap();
    let process_environment = child.process_environment_for_test().clone();
    let process_pid = child.process_pid_for_test().to_string();

    let started_at = std::time::Instant::now();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::TimedOut);
    assert_eq!(
        process_environment
            .read_process_exit_code(&process_pid)
            .await
            .unwrap(),
        Some(124)
    );
    assert!(
        started_at.elapsed() < Duration::from_secs(8),
        "timed-out child join should abort promptly instead of waiting for graceful shutdown"
    );
}
