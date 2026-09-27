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
        .spawn(r#"{"executable":"/bin/agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
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
        .spawn(r#"{"executable":"/bin/agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
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

    let client = NamespaceClient::new(root.clone());
    let events_path = "/agent/1/actions/events";
    let length = client.stat_path(events_path).await.unwrap().length;
    let events = String::from_utf8(
        client
            .read_file_range(events_path, 0, length)
            .await
            .unwrap(),
    )
    .unwrap();
    let fields: Vec<_> = events.lines().collect();
    let terminal = fields
        .iter()
        .position(|event| *event == "a0:status")
        .unwrap();
    for field in ["name", "output", "result", "approval", "process"] {
        let event = format!("a0:{field}");
        assert!(
            fields.iter().position(|item| *item == event).unwrap() < terminal,
            "{field} must be committed before a watcher sees completion: {events}"
        );
    }

    AgentConformanceChecker::new(root)
        .check_agent_process("/agent/1")
        .await
        .assert_ok();
}

#[tokio::test]
async fn action_evidence_survives_rollout_recovery_without_tool_replay() {
    let temp = tempfile::tempdir().unwrap();
    let recorder = crate::rollout::RolloutRecorder::new_in_dir("/proc/1", "mock", temp.path())
        .await
        .unwrap();
    let mut previous = recorder.path().clone();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(AgentFs::new())),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let original = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
        .with_action_recorder(Some(recorder.clone()));
    assert!(
        original
            .agent_files()
            .write_action(
                NamespaceActionRecord::new("oversized", "failed")
                    .with_output("x".repeat((16 << 20) + 1)),
            )
            .await
            .is_err()
    );
    let input_id = uuid::Uuid::new_v4().to_string();
    let action_id = original
        .agent_files()
        .write_action(
            NamespaceActionRecord::new("bash", "failed")
                .with_output(r#"{"stdout":"partial\n","stderr":"diagnostic\n","exit_code":7}"#)
                .with_result(serde_json::json!({"call_id":input_id,"exit_code":7}).to_string())
                .with_process("/proc/9"),
        )
        .await
        .unwrap();
    assert_eq!(
        action_id, "a1",
        "failed projection left an Action number gap"
    );
    let old_path = format!("/agent/1/actions/{action_id}/output");
    for message in [
        crate::tape::Message::tool_structured(
            "projection",
            serde_json::json!({
                "type":"evidence_projection", "reference":{"path":old_path,"offset":0,"length":10},
                "preview":old_path,
            }),
        ),
        crate::tape::Message::tool_text(
            "delegated",
            serde_json::json!({
                "result":{"output_ref":{"path":old_path,"offset":0,"length":10}},
                "plain_path":old_path,
            })
            .to_string(),
        ),
        crate::tape::Message::tool_text("ordinary", old_path.clone()),
    ] {
        recorder.record_tape_message(&message).await.unwrap();
    }
    drop(original);
    for pid in [2, 3] {
        let machine = crate::agent_machine::AgentMachine::load_from_rollout_in_dir(
            &previous,
            &format!("/proc/{pid}"),
            "mock",
            temp.path(),
        )
        .await
        .unwrap();
        let mut namespace = Namespace::new();
        let path = format!("/agent/{pid}");
        namespace.mount(
            &path,
            InProcessTransport::new(Arc::new(AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let shell = Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, &path, "default")
            .with_action_recorder(machine.recorder());
        previous = machine.rollout_path().unwrap().clone();
        environment
            .agent_files()
            .restore_actions(&previous)
            .await
            .unwrap();
        for message in machine.messages() {
            for response in message.tool_responses() {
                let text = response.text_content();
                if response.id == "ordinary" {
                    assert_eq!(text, old_path);
                    continue;
                }
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                let pointer = if response.id == "projection" {
                    "/reference"
                } else {
                    "/result/output_ref"
                };
                let reference = value.pointer(pointer).unwrap();
                assert_eq!(reference["path"], format!("{path}/actions/a0/output"));
                assert_eq!(reference["offset"], 0);
                assert_eq!(reference["length"], 10);
                assert_eq!(
                    value[if response.id == "projection" {
                        "preview"
                    } else {
                        "plain_path"
                    }],
                    old_path
                );
                assert!(
                    !shell
                        .cat(reference["path"].as_str().unwrap())
                        .await
                        .unwrap()
                        .is_empty()
                );
            }
        }
        // No /proc mount or Tool runner exists: recovery can only project evidence.
        let result: serde_json::Value = serde_json::from_slice(
            &shell
                .cat(&format!("{path}/actions/a0/result"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["call_id"], input_id);
        assert_eq!(result["exit_code"], 7);
        assert_eq!(
            shell
                .cat(&format!("{path}/actions/a0/output"))
                .await
                .unwrap(),
            br#"{"stdout":"partial\n","stderr":"diagnostic\n","exit_code":7}"#
        );
        assert_eq!(
            shell
                .cat(&format!("{path}/actions/a0/process"))
                .await
                .unwrap(),
            b"/proc/9"
        );
        let items = crate::rollout::RolloutRecorder::load_history(&previous)
            .await
            .unwrap();
        assert_eq!(items.iter().filter(|item| matches!(item,
            crate::rollout::RolloutItem::Event(event) if event.event_type == "agent_action_v1"
        )).count(), 1, "reprojection must not append duplicate durable evidence");
    }
}

#[tokio::test]
async fn expired_action_evidence_stays_expired_across_repeated_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let recorder = crate::rollout::RolloutRecorder::new_in_dir("/proc/1", "mock", temp.path())
        .await
        .unwrap();
    let mut namespace = Namespace::new();
    let agent = Arc::new(AgentFs::new());
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(agent.clone()),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
        .with_action_recorder(Some(recorder.clone()));
    let id = environment
        .agent_files()
        .write_action(
            NamespaceActionRecord::new("bash", "completed").with_output("expired private evidence"),
        )
        .await
        .unwrap();
    environment
        .agent_files()
        .write_action(
            NamespaceActionRecord::new("bash", "completed").with_output("still retained evidence"),
        )
        .await
        .unwrap();
    let journal = recorder.clone();
    agent.set_retention_recorder(move |id, cause| {
        let journal = journal.clone();
        async move {
            journal.persist_batch(vec![crate::rollout::RolloutItem::Event(
                crate::rollout::EventRecord {
                    event_type: "agent_action_retention_v1".into(),
                    payload: serde_json::json!({"agent_path":"/agent/1", "action_id":id, "cause":cause}),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                }
            )]).await.map_err(|_| alan_ap::ErrorCode::Io)
        }
    }).await;
    agent
        .expire_action_output_for_retention(&id, "age_limit")
        .await
        .unwrap();
    let mut previous = recorder.path().clone();
    for pid in [2, 3] {
        let machine = crate::agent_machine::AgentMachine::load_from_rollout_in_dir(
            &previous,
            &format!("/proc/{pid}"),
            "mock",
            temp.path(),
        )
        .await
        .unwrap();
        previous = machine.rollout_path().unwrap().clone();
        let mut namespace = Namespace::new();
        let path = format!("/agent/{pid}");
        namespace.mount(
            &path,
            InProcessTransport::new(Arc::new(AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let environment = NamespaceRuntimeEnvironment::new(root.clone(), &path, "default");
        environment
            .agent_files()
            .restore_actions(&previous)
            .await
            .unwrap();
        let output = Shell::new(root.clone())
            .cat(&format!("{path}/actions/a0/output"))
            .await
            .unwrap();
        let output: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(output["type"], "evidence_retention_expired");
        assert_eq!(output["cause"], "age_limit");
        assert_eq!(output["reference"], "/actions/a0/output");
        assert_eq!(
            NamespaceClient::new(root)
                .read_file(&format!("{path}/actions/a1/output"))
                .await
                .unwrap(),
            b"still retained evidence"
        );
        let persisted = std::fs::read_to_string(&previous).unwrap();
        assert!(!persisted.contains("expired private evidence"));
        let items = crate::rollout::RolloutRecorder::load_history(&previous)
            .await
            .unwrap();
        assert!(items.iter().any(|item| matches!(item,
            crate::rollout::RolloutItem::Event(event)
            if event.event_type == "agent_action_retention_v1"
            && event.payload["agent_path"] == path && event.payload["action_id"] == "a0"
        )));
    }
}
