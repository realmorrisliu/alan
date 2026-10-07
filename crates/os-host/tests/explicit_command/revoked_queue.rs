use super::*;
use alan_service_manager::HostMountAccess;

#[tokio::test]
async fn paused_native_command_cannot_reuse_revoked_cwd_authority() {
    let runtime = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    let mut tools = ToolRegistry::new();
    tools.register(alan_tools::BashTool::new());
    let provider = MockLlmProvider::new();
    let probe = provider.clone();
    let process = AgentProcessConfig {
        store_bindings: Some(AgentRuntimeStoreBindings {
            rollouts: runtime.path().join("rollouts"),
            checkpoints: runtime.path().join("checkpoints"),
            cache: runtime.path().join("cache"),
            tmp: runtime.path().join("tmp"),
            metadata: runtime.path().join("metadata"),
        }),
        ..AgentProcessConfig::default()
    };
    let host = AlanOsHost::boot(
        HostBootConfig::ephemeral("test", process, LlmClient::new(provider), tools),
        paths.clone(),
    )
    .await
    .unwrap();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move { host.serve_until(stopped.cancelled_owned()).await });
    let shell = Shell::new(
        LocalAttachment::new(paths.clone())
            .connect()
            .await
            .unwrap()
            .root,
    );
    let control = HostCommandPlane::new(paths.clone());
    let mounted = control
        .mount_project(
            uuid::Uuid::new_v4(),
            paths.read_status().unwrap().boot_id,
            project.path().to_owned(),
            HostMountAccess::ReadWrite,
        )
        .await
        .unwrap();
    let selected = command(&shell, &format!("cd {}", mounted.grant.namespace_path)).await;
    assert_eq!(selected["exit_code"], 0, "{selected}");
    let running = submit_command(&shell, "printf saved > completed; sleep 30").await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !project.path().join("completed").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("native action must start before interruption");
    let queued = submit_command(&shell, "printf forbidden >> queued-effect").await;
    wait_pending_input(&shell, &queued, false).await;
    shell
        .write("/agent/root/machine/ctl", b"interrupt")
        .await
        .unwrap();
    assert_ne!(command_result(&shell, &running).await["exit_code"], 0);
    wait_pending_input(&shell, &queued, true).await;
    control.revoke_host_mount(mounted.grant.id).await.unwrap();
    shell
        .write("/agent/root/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    let rejected = command_result(&shell, &queued).await;
    assert_ne!(rejected["exit_code"], 0, "{rejected}");
    let process = rejected["process"].as_str().unwrap();
    assert_eq!(
        shell.cat(&format!("{process}/status")).await.unwrap(),
        b"exited\n"
    );
    assert!(
        rejected["result_preview"]
            .as_str()
            .is_some_and(|text| text.contains("Process cwd grant was revoked or replaced")),
        "{rejected}"
    );
    assert!(!project.path().join("queued-effect").exists());
    assert_eq!(
        std::fs::read(project.path().join("completed")).unwrap(),
        b"saved"
    );
    assert!(probe.recorded_requests().is_empty());
    stop.cancel();
    server.await.unwrap().unwrap();
}
