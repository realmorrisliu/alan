//! Cancellation closes the actual pending mount Tool request before correction intake.
use super::*;

#[tokio::test]
async fn pending_mount_interrupt_then_new_agent_input_has_correlated_terminal_tool_response() {
    pending_mount_terminal_notice_lifecycle(false).await;
}

#[tokio::test]
async fn pending_mount_normal_approval_resumes_and_retires_wait_notice() {
    pending_mount_terminal_notice_lifecycle(true).await;
}

async fn pending_mount_terminal_notice_lifecycle(approved: bool) {
    let temp = TempDir::new().unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let mut mount_response = mock_generation_response("");
    mount_response.tool_calls = vec![alan_llm::ToolCall {
        id: Some("call-pending-mount".into()),
        name: "request_mount".into(),
        arguments: serde_json::json!({"namespace_path":"/mnt/project","access":"read_write","reason":"Need project files","label":"Project"}),
    }];
    let mock = MockLlmProvider::new().with_responses(vec![
        mount_response,
        mock_generation_response("corrected answer"),
    ]);
    let host_mount = crate::runtime::test_host_mount::TestHostMountFs::new();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/host-mount",
        InProcessTransport::new(host_mount.clone()),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            store_bindings: Some(stores.clone()),
            ..Default::default()
        },
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    let path = runtime
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    shell
        .write("/agent/1/io/input", b"Ask for project access")
        .await
        .unwrap();
    let request_id = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let history = crate::rollout::RolloutRecorder::load_history(&path)
                .await
                .unwrap();
            if let Some(id) = history.iter().find_map(|item| match item {
                crate::rollout::RolloutItem::Event(event)
                    if event.event_type == "host_mount_request_waiting" =>
                {
                    event.payload["request_id"].as_str().map(str::to_owned)
                }
                _ => None,
            }) {
                break id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual request_mount should reach a logical wait");
    assert_eq!(
        host_mount.status(&request_id).await.as_deref(),
        Some("pending")
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let notice = shell.cat("/agent/1/machine/ui/notice").await.unwrap();
            let notice: alan_agent_protocol::UiNoticeSnapshot =
                serde_json::from_slice(&notice).unwrap();
            if notice.message
                == "Waiting for Host Mount authorization; Ctrl+C cancels current input"
            {
                assert_eq!(notice.kind, alan_agent_protocol::UiNoticeKind::Warning);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual pending Host Mount must publish actionable safe wait notice");
    if approved {
        host_mount
            .settle(&request_id, "approved", Some("approved-grant"), None)
            .await;
    } else {
        shell
            .write("/agent/1/machine/ctl", b"interrupt")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let events =
                    String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap())
                        .unwrap();
                if events.lines().any(|line| {
                    matches!(
                        serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
                        alan_agent_protocol::UiEvent::InputCompleted {
                            status: alan_agent_protocol::UiInputStatus::Cancelled,
                            ..
                        }
                    )
                }) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("ordinary interrupt settles pending input");
        assert_eq!(
            host_mount.status(&request_id).await.as_deref(),
            Some("cancelled")
        );
        wait_for_retired_mount_notice(&shell).await;
        let interrupted_history = crate::rollout::RolloutRecorder::load_history(&path)
            .await
            .unwrap();
        assert!(interrupted_history.iter().any(|item| matches!(item,
        crate::rollout::RolloutItem::Message(record) if matches!(&record.message,
            Some(crate::tape::Message::Tool { responses }) if responses.iter().any(|response| response.id == "call-pending-mount")))),
        "interrupt must close the pending mount Tool request before accepting correction");
        // Interrupt pauses scheduling independently of pending-request settlement.
        shell
            .write("/agent/1/machine/ctl", b"queue-v1 continue")
            .await
            .unwrap();
        shell
            .write(
                "/agent/1/io/input",
                b"Correction: answer without project access",
            )
            .await
            .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let events =
                String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
            if events.lines().any(|line| {
                matches!(
                    serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
                    alan_agent_protocol::UiEvent::InputCompleted {
                        status: alan_agent_protocol::UiInputStatus::Completed,
                        ..
                    }
                )
            }) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("correction intake completes");
    wait_for_retired_mount_notice(&shell).await;
    runtime.shutdown().await.unwrap();
    assert_eq!(
        mock.recorded_requests().len(),
        2,
        "no replay or resumed mount generation"
    );
    let recovered =
        AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test-model", &stores.rollouts)
            .await
            .unwrap();
    let responses: Vec<_> = recovered
        .messages()
        .iter()
        .filter_map(|message| match message {
            crate::tape::Message::Tool { responses } => Some(responses),
            _ => None,
        })
        .flatten()
        .filter(|response| response.id == "call-pending-mount")
        .collect();
    assert_eq!(
        responses.len(),
        1,
        "interrupted request_mount must have exactly one correlated Tool response"
    );
    let result: serde_json::Value = serde_json::from_str(&responses[0].text_content()).unwrap();
    assert_eq!(
        result["status"],
        if approved { "approved" } else { "cancelled" }
    );
    assert_eq!(result["approved"], approved);
    assert_eq!(result["request_reference"], request_id);
    if approved {
        assert_eq!(result["grant_reference"], "approved-grant");
    } else {
        assert!(
            result["grant_reference"].is_null(),
            "never invent approval or a grant"
        );
    }
    assert!(!recovered.has_pending_interaction());
    let next_request = format!("{:?}", mock.recorded_requests()[1]);
    assert!(
        next_request.contains("call-pending-mount")
            && next_request.contains(if approved { "approved" } else { "cancelled" }),
        "next provider request must include terminal response: {next_request}"
    );
}

async fn wait_for_retired_mount_notice(shell: &alan_shell::Shell) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let notice: alan_agent_protocol::UiNoticeSnapshot =
                serde_json::from_slice(&shell.cat("/agent/1/machine/ui/notice").await.unwrap())
                    .unwrap();
            if notice.kind != alan_agent_protocol::UiNoticeKind::Warning
                || notice.message
                    != "Waiting for Host Mount authorization; Ctrl+C cancels current input"
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("terminal mount path retires only its wait notice");
}
