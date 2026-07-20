use super::*;
use alan_kernel::LiveNamespace;

struct NamespaceRaceProcFs {
    inner: ProcFs,
    namespace: LiveNamespace,
    mutate_before_clone: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl FileServer for NamespaceRaceProcFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.inner.walk(fid, newfid, names).await
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if mode == OpenMode::ReadWrite
            && self
                .mutate_before_clone
                .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.namespace.mount(
                "/mnt/project",
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                Access::ReadWrite,
            );
        }
        self.inner.open(fid, mode).await
    }

    async fn read(
        &self,
        fid: Fid,
        offset: alan_ap::Offset,
        count: u32,
    ) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }

    async fn write(
        &self,
        fid: Fid,
        offset: alan_ap::Offset,
        data: &[u8],
    ) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data).await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.inner.stat(fid).await
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.clunk(fid).await
    }
}

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
        .spawn_process_with_mounts(
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
async fn engine_retries_tool_spawn_after_a_live_namespace_generation_race() {
    let procfs = ProcFs::new();
    let agentfs = Arc::new(AgentFs::new());
    let binfs = Arc::new(alan_ap::reference::MemFs::new());
    let mut process_namespace = Namespace::new();
    process_namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        Access::ReadWrite,
    );
    process_namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs.clone()),
        Access::ReadWrite,
    );
    process_namespace.mount("/bin", InProcessTransport::new(binfs), Access::ReadOnly);
    process_namespace.mount(
        "/mnt",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadWrite,
    );
    let credentials = Credentials::user("root-agent");
    let bootstrap = procfs.for_spawner(None, process_namespace.clone(), credentials.clone());
    bootstrap
        .walk(Fid::ROOT, Fid(9_998), &["clone".to_string()])
        .await
        .unwrap();
    bootstrap
        .open(Fid(9_998), OpenMode::ReadWrite)
        .await
        .unwrap();
    let parent_exec = ExecSpec {
        executable: "/bin/agent".to_string(),
        args: Vec::new(),
        namespace: ExecNamespaceManifest::from_namespace(&process_namespace),
        descriptors: Default::default(),
    };
    bootstrap
        .write(Fid(9_998), 0, &serde_json::to_vec(&parent_exec).unwrap())
        .await
        .unwrap();
    bootstrap.clunk(Fid(9_998)).await.unwrap();

    let live_namespace = LiveNamespace::new(process_namespace);
    let procfs = procfs.with_runner(Arc::new(EchoRunner));
    procfs
        .bind_live_namespace(Pid(1), live_namespace.clone())
        .await;
    let racing_procfs = NamespaceRaceProcFs {
        inner: procfs.for_live_spawner(Some(Pid(1)), live_namespace.clone(), credentials),
        namespace: live_namespace.clone(),
        mutate_before_clone: std::sync::atomic::AtomicBool::new(true),
    };
    live_namespace.replace_mount(
        "/proc",
        InProcessTransport::new(Arc::new(racing_procfs)),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live_namespace)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

    let action = environment
        .tool_execution()
        .run_action("echo", "/bin/greeting", ["retried".to_string()])
        .await
        .unwrap();
    assert_eq!(action.output, "retried\n");
    let child_namespace = String::from_utf8(
        shell
            .cat(&format!("/proc/{}/namespace", action.pid))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        child_namespace
            .lines()
            .any(|line| line == "/mnt/project rw"),
        "Tool Process retry must use one fresh explicit snapshot: {child_namespace:?}"
    );
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
