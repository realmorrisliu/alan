use super::*;

#[tokio::test]
async fn m2_shell_talks_to_agent_through_files() {
    let procfs = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = procfs.clone();
    let agentfs = Arc::new(AgentFs::new());
    let agent_root = Arc::new(AgentRootFs::new(proc_server));
    let llmfs = Arc::new(LlmFs::new());
    llmfs.register_connection(
        "default",
        Box::new(MockLlmProvider::new().with_response(GenerationResponse {
            content: "hello from llmfs".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        })),
    );

    let mut ns = Namespace::new();
    ns.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
    ns.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());

    let pid = shell
        .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    assert_eq!(pid, "1");
    agent_root.bind_process(pid.clone(), agentfs.clone()).await;
    agent_root.set_root_process(pid.clone()).await;

    shell
        .write("/agent/1/io/input", b"hello agent")
        .await
        .unwrap();
    let mut output_tail = shell.tail("/agent/1/io/output").await.unwrap();

    let mut runtime = NamespaceTurnRuntime::new(
        root.clone(),
        NamespaceTurnRuntimeConfig::new("/agent/1", "default")
            .with_system_prompt("You are an M2 test agent."),
    );
    let turn = runtime.run_next_turn().await.unwrap();

    assert_eq!(turn.input, "hello agent");
    assert_eq!(turn.response, "hello from llmfs");
    assert!(!turn.generation_id.is_empty());

    let streamed = output_tail.read(64 * 1024).await.unwrap();
    output_tail.close().await.unwrap();
    assert_eq!(String::from_utf8(streamed).unwrap(), "hello from llmfs");

    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    assert!(tape.contains(r#""role":"user""#), "{tape}");
    assert!(tape.contains(r#""content":"hello agent""#), "{tape}");
    assert!(tape.contains(r#""role":"assistant""#), "{tape}");
    assert!(tape.contains(r#""content":"hello from llmfs""#), "{tape}");
    let checkpoint = runtime.current_tape_checkpoint().await.unwrap();
    let checkpoint_file = String::from_utf8(
        shell
            .cat("/agent/1/machine/checkpoints/current")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(checkpoint, checkpoint_file.trim());
    assert!(checkpoint.starts_with("sha256:"), "{checkpoint}");

    AgentConformanceChecker::new(root)
        .check_agent_process("/agent/1")
        .await
        .assert_ok();
}

#[tokio::test]
async fn engine_writes_requests_and_actions_as_agent_files() {
    let procfs = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = procfs.clone();
    let agentfs = Arc::new(AgentFs::new());
    let agent_root = Arc::new(AgentRootFs::new(proc_server));
    let mut ns = Namespace::new();
    ns.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
    ns.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let pid = shell
        .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    assert_eq!(pid, "1");
    agent_root.bind_process(pid.clone(), agentfs.clone()).await;
    agent_root.set_root_process(pid).await;
    let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
    let agent_files = environment.agent_files();

    let request_id = agent_files
        .write_request(
            NamespaceRequestRecord::new("confirmation", "approve this action?")
                .with_options(r#"{"choices":["approve","deny"]}"#),
        )
        .await
        .unwrap();
    assert_eq!(request_id, "r0");
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/requests/{request_id}/kind"))
                .await
                .unwrap()
        )
        .unwrap(),
        "confirmation"
    );
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/requests/{request_id}/prompt"))
                .await
                .unwrap()
        )
        .unwrap(),
        "approve this action?"
    );
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/requests/{request_id}/options"))
                .await
                .unwrap()
        )
        .unwrap(),
        r#"{"choices":["approve","deny"]}"#
    );

    let action_id = agent_files
        .write_action(
            NamespaceActionRecord::new("read", "completed")
                .with_output("file contents")
                .with_result(r#"{"ok":true}"#)
                .with_approval("not_required")
                .with_process("/proc/42"),
        )
        .await
        .unwrap();
    assert_eq!(action_id, "a0");
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/actions/{action_id}/name"))
                .await
                .unwrap()
        )
        .unwrap(),
        "read"
    );
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/actions/{action_id}/status"))
                .await
                .unwrap()
        )
        .unwrap(),
        "completed"
    );
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/actions/{action_id}/output"))
                .await
                .unwrap()
        )
        .unwrap(),
        "file contents"
    );
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/actions/{action_id}/result"))
                .await
                .unwrap()
        )
        .unwrap(),
        r#"{"ok":true}"#
    );
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/actions/{action_id}/approval"))
                .await
                .unwrap()
        )
        .unwrap(),
        "not_required"
    );
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/1/actions/{action_id}/process"))
                .await
                .unwrap()
        )
        .unwrap(),
        "/proc/42"
    );

    AgentConformanceChecker::new(root)
        .check_agent_process("/agent/1")
        .await
        .assert_ok();
}
