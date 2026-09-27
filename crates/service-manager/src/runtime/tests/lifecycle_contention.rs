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

#[tokio::test]
async fn root_exit_is_terminal_without_automatic_replacement() {
    for exit_code in [0, 7] {
        let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
            "test",
            AgentProcessConfig::default(),
            ProcessLaunchContext::root(),
            LlmClient::new(MockLlmProvider::new()),
            ToolRegistry::new(),
        ))
        .await
        .unwrap();
        let old_pid = manager.root_pid();
        let namespace = manager.local_entry().namespace_for_local_client();
        let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
        manager
            .terminate_unit("root-agent", exit_code)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while manager.root_pid() != Pid(0) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            manager.procfs.try_observe_process_lifecycle(old_pid),
            Some((Status::Exited, Some(exit_code)))
        );
        let unit = manager.state().lock().await.unit("root-agent").unwrap();
        assert_eq!(unit.attempts, 1);
        assert_eq!(unit.status, crate::UnitStatus::Exited);
        assert!(unit.pid.is_none());
        assert!(shell.ls("/agent/root").await.is_err());
        assert_eq!(
            shell.cat("/mnt/service-manager/status").await.unwrap(),
            b"degraded\n"
        );
        manager.shutdown().await.unwrap();
    }
}
