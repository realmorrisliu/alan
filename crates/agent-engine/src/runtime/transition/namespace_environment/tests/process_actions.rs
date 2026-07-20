use super::*;

#[tokio::test]
async fn engine_spawns_process_with_explicit_namespace_manifest() {
    let procfs = ProcFs::new();
    let agentfs = Arc::new(AgentFs::new());
    let llmfs = Arc::new(LlmFs::new());
    let binfs = Arc::new(alan_ap::reference::MemFs::new());

    let mut child_namespace = Namespace::new();
    child_namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    child_namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        Access::ReadWrite,
    );
    child_namespace.mount("/bin", InProcessTransport::new(binfs), Access::ReadOnly);

    let spawner_procfs =
        Arc::new(procfs.for_spawner(None, child_namespace, Credentials::user("root-agent")));
    let mut root_namespace = Namespace::new();
    root_namespace.mount(
        "/proc",
        InProcessTransport::new(spawner_procfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(root_namespace)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/root", "default");

    let pid = environment
        .process_files()
        .spawn_process(
            "/bin/agent",
            Vec::<String>::new(),
            vec![ProcessNamespaceMount::new(
                "/bin",
                ProcessNamespaceAccess::ReadOnly,
            )],
        )
        .await
        .unwrap();

    assert_eq!(pid, "1");
    assert_eq!(
        String::from_utf8(shell.cat(&format!("/proc/{pid}/status")).await.unwrap()).unwrap(),
        "running\n"
    );
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/proc/{pid}/credentials"))
                .await
                .unwrap()
        )
        .unwrap(),
        "root-agent"
    );
    let namespace =
        String::from_utf8(shell.cat(&format!("/proc/{pid}/namespace")).await.unwrap()).unwrap();
    assert_eq!(namespace, "/bin ro");
}

#[tokio::test]
async fn engine_runs_tool_as_process_and_projects_action_files() {
    let (environment, shell) = tool_test_environment(Arc::new(EchoRunner)).await;

    let action = environment
        .tool_execution()
        .run_action(
            "echo",
            "/bin/greeting",
            ["hello".to_string(), "from-process".to_string()],
        )
        .await
        .unwrap();

    assert_eq!(action.pid, "2");
    assert_eq!(action.action_id, "a0");
    assert_eq!(action.output, "hello from-process\n");
    assert_eq!(action.exit_code, 0);
    assert_eq!(
        environment
            .process_files()
            .read_process_exit_code("2")
            .await
            .unwrap(),
        Some(0)
    );
    assert_eq!(
        String::from_utf8(shell.cat("/proc/2/status").await.unwrap()).unwrap(),
        "exited\n"
    );
    assert_eq!(
        String::from_utf8(shell.cat("/proc/2/io/output").await.unwrap()).unwrap(),
        "hello from-process\n"
    );
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/actions/a0/name").await.unwrap()).unwrap(),
        "echo"
    );
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/actions/a0/status").await.unwrap()).unwrap(),
        "completed"
    );
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/actions/a0/output").await.unwrap()).unwrap(),
        "hello from-process\n"
    );
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/actions/a0/result").await.unwrap()).unwrap(),
        r#"{"exit_code":0}"#
    );
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/actions/a0/process").await.unwrap()).unwrap(),
        "/proc/2"
    );
}

#[tokio::test]
async fn run_tool_action_projects_logical_payload_failure_as_failed_action() {
    let (environment, shell) = tool_test_environment(Arc::new(LogicalFailureRunner)).await;

    let action = environment
        .tool_execution()
        .run_action("bash", "/bin/bash", ["{}"])
        .await
        .unwrap();

    assert_eq!(action.pid, "2");
    assert_eq!(action.action_id, "a0");
    assert_eq!(action.exit_code, 2);
    assert_eq!(
        String::from_utf8(shell.cat("/proc/2/exit").await.unwrap()).unwrap(),
        "0"
    );
    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/actions/a0/status").await.unwrap()).unwrap(),
        "failed"
    );
    let result = String::from_utf8(shell.cat("/agent/1/actions/a0/result").await.unwrap()).unwrap();
    let result: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["exit_code"], 2);
    assert_eq!(result["process_exit_code"], 0);
}

#[tokio::test]
async fn run_tool_action_cancels_spawned_process_on_cancel() {
    let started = Arc::new(Notify::new());
    let dropped = Arc::new(Notify::new());
    let (environment, shell) = tool_test_environment(Arc::new(AbortObservedRunner {
        started: Arc::clone(&started),
        dropped: Arc::clone(&dropped),
    }))
    .await;
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let environment = environment.clone();
        let cancel = cancel.clone();
        async move {
            environment
                .tool_execution()
                .run_action_with_cancel("blocked", "/bin/blocked", Vec::<String>::new(), &cancel)
                .await
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
        .await
        .expect("tool runner should start");
    cancel.cancel();
    let err = task.await.unwrap().unwrap_err();
    assert!(err.to_string().contains("cancelled"), "{err:#}");
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped.notified())
        .await
        .expect("tool runner future should be aborted");
    assert_eq!(
        String::from_utf8(shell.cat("/proc/2/status").await.unwrap()).unwrap(),
        "exited\n"
    );
    assert_eq!(
        String::from_utf8(shell.cat("/proc/2/exit").await.unwrap()).unwrap(),
        "130"
    );
}

#[tokio::test]
async fn run_tool_action_cancels_spawned_process_on_wait_timeout() {
    let started = Arc::new(Notify::new());
    let dropped = Arc::new(Notify::new());
    let (environment, shell) = tool_test_environment(Arc::new(AbortObservedRunner {
        started: Arc::clone(&started),
        dropped: Arc::clone(&dropped),
    }))
    .await;
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let environment = environment.clone();
        let cancel = cancel.clone();
        async move {
            environment
                .tool_execution()
                .run_action_with_cancel_and_timeout(
                    "blocked",
                    "/bin/blocked",
                    Vec::<String>::new(),
                    &cancel,
                    1,
                )
                .await
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
        .await
        .expect("tool runner should start");
    let err = tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .expect("tool wait should use the configured timeout")
        .unwrap()
        .unwrap_err();
    let err = format!("{err:#}");
    assert!(err.contains("timed out waiting 1s"), "{err}");
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped.notified())
        .await
        .expect("tool runner future should be aborted on wait timeout");
    assert_eq!(
        String::from_utf8(shell.cat("/proc/2/status").await.unwrap()).unwrap(),
        "exited\n"
    );
    assert_eq!(
        String::from_utf8(shell.cat("/proc/2/exit").await.unwrap()).unwrap(),
        "130"
    );
}

#[tokio::test]
async fn run_tool_action_reads_output_larger_than_initial_read() {
    let (environment, _shell) = tool_test_environment(Arc::new(LargeOutputRunner)).await;

    let action = environment
        .tool_execution()
        .run_action("large", "/bin/large", Vec::<String>::new())
        .await
        .unwrap();

    assert_eq!(action.output.len(), 70 * 1024);
    assert!(action.output.bytes().all(|byte| byte == b'x'));
    assert_eq!(action.exit_code, 0);
}

#[tokio::test]
async fn run_tool_action_reads_exact_chunk_output_without_waiting_for_more() {
    let (environment, _shell) = tool_test_environment(Arc::new(ExactChunkOutputRunner)).await;

    let action = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        environment
            .tool_execution()
            .run_action("exact", "/bin/exact", Vec::<String>::new()),
    )
    .await
    .expect("exact chunk output should not wait at the live stream edge")
    .unwrap();

    assert_eq!(action.output.len(), 64 * 1024);
    assert!(action.output.bytes().all(|byte| byte == b'y'));
    assert_eq!(action.exit_code, 0);
}
