use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn process_table_contention_does_not_restart_a_running_root() {
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    let root_pid = manager.root_pid();
    let procfs = manager.procfs.clone();
    let reader = tokio::spawn(async move {
        loop {
            assert!(procfs.observe_process_files(root_pid).await.is_some());
        }
    });
    for _ in 0..300 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(
            manager.root_pid(),
            root_pid,
            "contention is not evidence of Process exit"
        );
    }
    reader.abort();
    let _ = reader.await;
    let files = manager
        .procfs
        .observe_process_files(root_pid)
        .await
        .unwrap();
    assert_eq!(files.status, Status::Running);
    manager.shutdown().await.unwrap();
}
