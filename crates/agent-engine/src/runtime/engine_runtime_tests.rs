use super::*;

#[test]
fn test_push_outer_submission_inserts_before_existing_deferred_actions() {
    let mut queues = RuntimeSubmissionQueues::default();

    let first_submission = Submission::new(Op::Interrupt);
    let second_submission = Submission::new(Op::CompactWithOptions { focus: None });
    let first_submission_id = first_submission.id.clone();
    let second_submission_id = second_submission.id.clone();

    queues.push_outer_submission(first_submission);
    queues.push_outer_deferred(make_deferred_action_for_test());
    queues.push_outer_deferred(make_deferred_action_for_test());
    queues.push_outer_submission(second_submission);

    let outer_queue = std::iter::from_fn(|| queues.pop_outer()).collect::<VecDeque<_>>();
    assert_eq!(
        queue_item_kinds(&outer_queue),
        vec!["submission", "submission", "deferred", "deferred"]
    );

    let queued_submission_ids = outer_queue
        .iter()
        .filter_map(|item| match item {
            QueuedRuntimeItem::Submission(submission) => Some(submission.id.clone()),
            QueuedRuntimeItem::Deferred(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        queued_submission_ids,
        vec![first_submission_id, second_submission_id]
    );
}

#[tokio::test]
async fn test_requeue_active_turn_leftovers_inserts_before_existing_deferred_actions() {
    let mut queues = RuntimeSubmissionQueues::default();
    queues.push_outer_deferred(make_deferred_action_for_test());

    let mut machine = AgentMachine::new();
    let buffered_submission = Submission::new(Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("follow up")],
        mode: alan_agent_protocol::InputMode::FollowUp,
    });
    let buffered_submission_id = buffered_submission.id.clone();
    machine.push_buffered_inband_submission(buffered_submission);

    let requeued = queues.requeue_active_turn_leftovers(&mut machine).await;

    assert_eq!(requeued, 1);
    let outer_queue = std::iter::from_fn(|| queues.pop_outer()).collect::<VecDeque<_>>();
    assert_eq!(
        queue_item_kinds(&outer_queue),
        vec!["submission", "deferred"]
    );

    match outer_queue.front() {
        Some(QueuedRuntimeItem::Submission(submission)) => {
            assert_eq!(submission.id, buffered_submission_id);
        }
        _ => panic!("expected buffered submission at queue front"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_runtime_shutdown_drains_deferred_memory_promotion_actions() {
    let temp = TempDir::new().unwrap();
    let system_store = temp.path().join("system-store");
    let memory_dir = system_store.join("memory");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    let store_bindings = crate::AgentRuntimeStoreBindings {
        rollouts: system_store.join("rollouts"),
        checkpoints: system_store.join("checkpoints"),
        cache: system_store.join("cache"),
        tmp: system_store.join("tmp"),
        metadata: system_store.join("metadata"),
    };
    for path in [
        &store_bindings.rollouts,
        &store_bindings.checkpoints,
        &store_bindings.cache,
        &store_bindings.tmp,
        &store_bindings.metadata,
    ] {
        std::fs::create_dir_all(path).unwrap();
    }

    let mut core_config =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core_config.memory.enabled = true;
    core_config.memory.store_dir = Some(memory_dir.clone());
    core_config.streaming_mode = crate::config::StreamingMode::Off;

    let mut agent_config = crate::AgentConfig::from(core_config);
    agent_config.runtime_config.streaming_mode = crate::config::StreamingMode::Off;

    let config = AgentProcessConfig {
        agent_config,
        memory_store_bound: true,
        store_bindings: Some(store_bindings),
        memory_store_backing: Some(memory_dir.clone()),
        ..AgentProcessConfig::default()
    };
    let call_count = Arc::new(Mutex::new(0));
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection(
        "default",
        Box::new(ShutdownDrainMemoryPromotionProvider {
            call_count: Arc::clone(&call_count),
            deferred_delay: Duration::from_millis(100),
        }),
    );
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut output = shell.tail("/agent/1/io/output").await.unwrap();
    let environment = crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let generation_capabilities =
        crate::provider_capabilities_for_config(&config.agent_config.core_config);
    let mut controller = spawn_with_namespace_environment(
        config,
        environment,
        crate::skills::SkillHostCapabilities::default(),
        generation_capabilities,
    )
    .unwrap();
    controller.wait_until_ready().await.unwrap();

    let submission = Submission::new(Op::Turn {
        parts: vec![alan_agent_protocol::ContentPart::text("My name is Morris.")],
        context: None,
    });
    controller
        .handle
        .submission_tx
        .send(submission.clone())
        .await
        .unwrap();

    let output = tokio::time::timeout(Duration::from_secs(15), output.read(4096))
        .await
        .expect("turn output did not arrive")
        .unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "Noted.");

    controller.shutdown().await.unwrap();

    let user_memory =
        tokio::fs::read_to_string(memory_dir.join(crate::prompts::MEMORY_USER_FILENAME))
            .await
            .unwrap();
    assert!(
        user_memory.contains("Name: Morris"),
        "provider_calls={}, user_memory={user_memory:?}",
        *call_count.lock().unwrap()
    );
}

#[tokio::test]
async fn test_spawn_with_namespace_environment_reaches_ready_without_store_bindings() {
    let core_config = crate::Config::default();
    let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
    let config = AgentProcessConfig {
        agent_config: crate::AgentConfig::from(core_config),
        ..AgentProcessConfig::default()
    };
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let namespace_environment =
        crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

    let mut controller = spawn_with_namespace_environment(
        config,
        namespace_environment,
        crate::skills::SkillHostCapabilities::default(),
        generation_capabilities,
    )
    .unwrap();
    let ready = controller.wait_until_ready().await.unwrap();

    assert_eq!(ready.process_path, "/proc/1");
    assert_eq!(ready.agent_path, "/agent/1");
    assert!(ready.rollout_id.is_none());
    assert!(!ready.durability.durable);
    controller.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_namespace_io_input_frame_drives_runtime_turn_without_api_submission() {
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let mock = MockLlmProvider::new().with_responses(vec![
        GenerationResponse {
            content: "first namespace response".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
        GenerationResponse {
            content: "second namespace response".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
    ]);
    let mock_probe = mock.clone();
    llmfs.register_connection("default", Box::new(mock));

    let procfs = Arc::new(alan_kernel::ProcFs::new());
    let agent_root = Arc::new(alan_agentfs::AgentRootFs::new(procfs.clone()));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/proc",
        alan_ap::InProcessTransport::new(procfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/agent",
        alan_ap::InProcessTransport::new(agent_root.clone()),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    for path in ["/bin", "/lib", "/man", "/mnt"] {
        ns.mount(
            path,
            alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            alan_kernel::Access::ReadOnly,
        );
    }
    for name in ["read_file", "write_file", "search_files", "run_command"] {
        let manifest = crate::runtime::ToolPackageManifest::from_tool(
            &PackageTestTool {
                name,
                description: "Host-mounted test Tool",
            },
            30,
        )
        .unwrap();
        ns.mount(
            &format!("/bin/{name}"),
            alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            alan_kernel::Access::ReadOnly,
        );
        ns.mount(
            &format!("/lib/exec/{name}"),
            alan_ap::InProcessTransport::new(Arc::new(
                alan_ap::reference::MemFs::with_read_only_file(
                    "manifest",
                    serde_json::to_vec(&manifest).unwrap(),
                ),
            )),
            alan_kernel::Access::ReadOnly,
        );
    }
    let live_namespace = alan_kernel::LiveNamespace::new(ns);
    let root = alan_ap::InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::from_live_namespace(live_namespace.clone()),
    ));
    let bootstrap_shell = alan_shell::Shell::new(root.clone());
    let pid = bootstrap_shell
        .spawn(r#"{"executable":"/bin/agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
        .await
        .unwrap();
    assert_eq!(pid, "1");
    agent_root.bind_process(pid.clone(), agentfs).await;
    agent_root.set_root_process(pid).await;
    let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);
    let attachment_mount = Arc::new(alan_kernel::MountFs::from_live_namespace(live_namespace));
    let server_task = tokio::spawn(alan_ap::export_file_server(
        attachment_mount,
        tokio::io::BufReader::new(server_read),
        server_write,
    ));
    let imported = Arc::new(alan_ap::ImportedFileServer::new(
        tokio::io::BufReader::new(client_read),
        client_write,
    ));
    let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(imported));

    let mut core_config = crate::Config::default();
    core_config.memory.enabled = false;
    let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
    let store = TempDir::new().unwrap();
    let store_bindings = crate::AgentRuntimeStoreBindings {
        rollouts: store.path().join("rollouts"),
        checkpoints: store.path().join("checkpoints"),
        cache: store.path().join("cache"),
        tmp: store.path().join("tmp"),
        metadata: store.path().join("metadata"),
    };
    for path in [
        &store_bindings.rollouts,
        &store_bindings.checkpoints,
        &store_bindings.cache,
        &store_bindings.tmp,
        &store_bindings.metadata,
    ] {
        std::fs::create_dir_all(path).unwrap();
    }
    let config = AgentProcessConfig {
        agent_config: crate::AgentConfig::from(core_config),
        store_bindings: Some(store_bindings),
        ..AgentProcessConfig::default()
    };
    let namespace_environment =
        crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let mut controller = spawn_with_namespace_environment(
        config,
        namespace_environment,
        crate::skills::SkillHostCapabilities::default(),
        generation_capabilities,
    )
    .unwrap();
    controller.wait_until_ready().await.unwrap();

    let mut ui_events = shell.tail("/agent/1/machine/ui/events").await.unwrap();
    shell
        .write("/agent/1/io/input", b"hello through files")
        .await
        .unwrap();

    wait_for_ui_turn_completion(&mut ui_events, Duration::from_secs(5)).await;

    let output = String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap();
    assert_eq!(output, "first namespace response");

    shell
        .write("/agent/1/io/input", b"second input through files")
        .await
        .unwrap();
    wait_for_ui_turn_completion(&mut ui_events, Duration::from_secs(5)).await;
    let output = String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap();
    assert_eq!(output, "first namespace responsesecond namespace response");
    assert_eq!(mock_probe.recorded_requests().len(), 2);

    controller.shutdown().await.unwrap();
    drop(shell);
    server_task.abort();
}

#[tokio::test]
async fn test_outer_idle_reads_answered_namespace_request_response() {
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let namespace_environment =
        crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

    let request_id = namespace_environment
        .agent_files()
        .write_request(crate::runtime::transition::NamespaceRequestRecord::new(
            "structured_input",
            "Provide the missing value",
        ))
        .await
        .unwrap();
    let mut machine = AgentMachine::new();
    machine.set_structured_input(crate::approval::PendingStructuredInputRequest {
        request_id: request_id.clone(),
        title: "Missing value".to_string(),
        prompt: "Provide the missing value".to_string(),
        questions: Vec::new(),
    });
    let state = RuntimeLoopState {
        machine,
        environment: namespace_environment,
        core_config: crate::Config::default(),
        runtime_config: RuntimeConfig::default(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    assert!(
        read_pending_namespace_resume_submission(&state)
            .await
            .is_none(),
        "unanswered request should not create a resume submission"
    );

    shell
        .write(
            &format!("/agent/1/requests/{request_id}/response"),
            br#"{"answers":[{"question_id":"q1","value":"from file"}]}"#,
        )
        .await
        .unwrap();

    let submission = read_pending_namespace_resume_submission(&state)
        .await
        .expect("answered namespace request should be observed")
        .unwrap();
    match submission.op {
        Op::Resume {
            request_id: resumed_id,
            content,
        } => {
            assert_eq!(resumed_id, request_id);
            assert_eq!(
                content,
                vec![ContentPart::structured(serde_json::json!({
                    "answers": [{"question_id": "q1", "value": "from file"}]
                }))]
            );
        }
        other => panic!("expected Op::Resume from namespace response, got {other:?}"),
    }
}

#[tokio::test]
async fn test_namespace_machine_ctl_drives_runtime_submission_without_api_submission() {
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection(
        "default",
        Box::new(MockLlmProvider::new().with_response(GenerationResponse {
            content: "hello from namespace runtime".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        })),
    );

    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());

    let core_config = crate::Config::default();
    let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
    let config = AgentProcessConfig {
        agent_config: crate::AgentConfig::from(core_config),
        ..AgentProcessConfig::default()
    };
    let namespace_environment =
        crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let mut controller = spawn_with_namespace_environment(
        config,
        namespace_environment,
        crate::skills::SkillHostCapabilities::default(),
        generation_capabilities,
    )
    .unwrap();
    controller.wait_until_ready().await.unwrap();

    let mut ui_events = shell.tail("/agent/1/machine/ui/events").await.unwrap();
    shell
        .write("/agent/1/io/input", b"hello through files")
        .await
        .unwrap();

    wait_for_ui_turn_completion(&mut ui_events, Duration::from_secs(5)).await;

    shell
        .write("/agent/1/machine/ctl", b"rollback")
        .await
        .unwrap();

    let rollback_notice = tokio::time::timeout(Duration::from_secs(5), async {
        let mut pending = String::new();
        'events: loop {
            pending.push_str(&String::from_utf8(ui_events.read(4096).await.unwrap()).unwrap());
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].to_string();
                pending.drain(..=newline);
                let event: alan_agent_protocol::UiEvent = serde_json::from_str(&line).unwrap();
                if let alan_agent_protocol::UiEvent::Notice { snapshot } = event
                    && snapshot.kind == alan_agent_protocol::UiNoticeKind::Rollback
                {
                    break 'events snapshot.message;
                }
            }
        }
    })
    .await
    .expect("namespace machine/ctl should drive rollback submission");
    assert_eq!(rollback_notice, "rolled back 1 turns");

    controller.shutdown().await.unwrap();
}

struct QueueGateProvider {
    calls: Arc<Mutex<usize>>,
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl LlmProvider for QueueGateProvider {
    async fn generate(&mut self, _request: GenerationRequest) -> Result<GenerationResponse> {
        let first = {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            *calls == 1
        };
        if first {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(mock_generation_response("done"))
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> Result<String> {
        anyhow::bail!("unused chat path")
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        Ok(response_stream(self.generate(request).await?))
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

async fn wait_for_queue_activity(
    shell: &alan_shell::Shell,
    predicate: impl Fn(&alan_agent_protocol::UiActivitySnapshot) -> bool,
) -> alan_agent_protocol::UiActivitySnapshot {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let bytes = shell.cat("/agent/1/machine/ui/activity").await.unwrap();
            let activity = serde_json::from_slice(&bytes).unwrap();
            if predicate(&activity) {
                return activity;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("runtime did not publish the expected queue state")
}

#[tokio::test]
async fn runtime_file_controls_keep_accepted_inputs_paused_until_continue_or_discard() {
    use alan_agent_protocol::{InputIntent, UiActivityState, UiEvent, UserInputRecord};
    for verb in ["continue", "discard"] {
        let calls = Arc::new(Mutex::new(0));
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(QueueGateProvider {
                calls: calls.clone(),
                entered: entered.clone(),
                release: release.clone(),
            }),
        );
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());
        let mut core = crate::Config::default();
        core.memory.enabled = false;
        core.streaming_mode = crate::config::StreamingMode::Off;
        let capabilities = crate::provider_capabilities_for_config(&core);
        let config = AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            ..AgentProcessConfig::default()
        };
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        let mut controller = spawn_with_namespace_environment(
            config,
            environment,
            crate::skills::SkillHostCapabilities::default(),
            capabilities,
        )
        .unwrap();
        controller.wait_until_ready().await.unwrap();
        let inputs = ["active", "second", "third"]
            .map(|text| UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, text));
        shell
            .write("/agent/1/io/input", &inputs[0].encode_payload().unwrap())
            .await
            .unwrap();
        if tokio::time::timeout(Duration::from_secs(10), entered.notified())
            .await
            .is_err()
        {
            panic!(
                "active generation never started: {}",
                String::from_utf8_lossy(&shell.cat("/agent/1/machine/ui/events").await.unwrap())
            );
        }
        for input in &inputs[1..] {
            shell
                .write("/agent/1/io/input", &input.encode_payload().unwrap())
                .await
                .unwrap();
        }
        let queued = wait_for_queue_activity(&shell, |a| a.pending_submissions.len() == 2).await;
        assert_eq!(
            queued
                .pending_submissions
                .iter()
                .map(|s| s.submission_id.as_str())
                .collect::<Vec<_>>(),
            inputs[1..]
                .iter()
                .map(|s| s.submission_id.as_str())
                .collect::<Vec<_>>()
        );
        shell
            .write(
                "/agent/1/machine/ctl",
                format!("queue-v1 interrupt {}", inputs[0].submission_id).as_bytes(),
            )
            .await
            .unwrap();
        wait_for_queue_activity(&shell, |a| {
            a.queue_paused && a.active_submission.is_none() && a.state == UiActivityState::Idle
        })
        .await;
        release.notify_one();
        assert_eq!(
            *calls.lock().unwrap(),
            1,
            "paused inputs must not reach generation"
        );
        let before = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
        assert!(!before.contains(&inputs[1].submission_id));
        assert!(!before.contains(&inputs[2].submission_id));
        shell
            .write(
                "/agent/1/machine/ctl",
                format!("queue-v1 {verb}").as_bytes(),
            )
            .await
            .unwrap();
        wait_for_queue_activity(&shell, |a| {
            !a.queue_paused
                && a.active_submission.is_none()
                && a.pending_submissions.is_empty()
                && a.state == UiActivityState::Idle
        })
        .await;
        let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        assert!(events.lines().any(|line| matches!(serde_json::from_str::<UiEvent>(line).unwrap(), UiEvent::Error { submission_id: Some(id), .. } if id == inputs[0].submission_id)));
        if verb == "continue" {
            assert_eq!(*calls.lock().unwrap(), 3);
            let ids = tape
                .lines()
                .filter_map(|line| {
                    let record: serde_json::Value = serde_json::from_str(line).unwrap();
                    (record["role"] == "user")
                        .then(|| record["submission_id"].as_str().unwrap().to_owned())
                })
                .collect::<Vec<_>>();
            // A cancelled generation has no assistant response/Tape projection; its UI error
            // above owns the result. Both later turns must project in accepted order.
            assert_eq!(
                ids,
                inputs[1..]
                    .iter()
                    .map(|s| s.submission_id.clone())
                    .collect::<Vec<_>>()
            );
        } else {
            assert_eq!(*calls.lock().unwrap(), 1);
            for input in &inputs[1..] {
                assert!(!tape.contains(&input.submission_id));
                assert!(events.lines().any(|line| matches!(serde_json::from_str::<UiEvent>(line).unwrap(), UiEvent::Error { submission_id: Some(id), .. } if id == input.submission_id)));
            }
        }
        controller.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn recovered_runtime_waits_for_explicit_continue_and_does_not_replay_unknown_work() {
    use alan_agent_protocol::{InputIntent, UiActivityState};
    let store = TempDir::new().unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: store.path().join("rollouts"),
        checkpoints: store.path().join("checkpoints"),
        cache: store.path().join("cache"),
        tmp: store.path().join("tmp"),
        metadata: store.path().join("metadata"),
    };
    for path in [
        &stores.rollouts,
        &stores.checkpoints,
        &stores.cache,
        &stores.tmp,
        &stores.metadata,
    ] {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut previous = AgentMachine::new_with_recorder_options(
        "/proc/old",
        "model",
        Some(&stores.rollouts),
        None,
        None,
    )
    .await
    .unwrap();
    let unknown_id = uuid::Uuid::new_v4().to_string();
    previous.accept_submission(&unknown_id);
    let pending = Submission::with_intent(
        Op::Input {
            parts: vec![ContentPart::text("recovered input")],
            mode: InputMode::FollowUp,
        },
        InputIntent::Agent,
    );
    let pending_id = pending.id.clone();
    previous.input_broker().checkpoint_cwd("/".into());
    previous.input_broker().push_outer_submission(pending);
    previous.input_broker().persist().await.unwrap();
    let source = previous.rollout_path().unwrap().clone();
    let provider = MockLlmProvider::new();
    let probe = provider.clone();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(provider));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let config = AgentProcessConfig {
        agent_config: crate::AgentConfig::from(core),
        store_bindings: Some(stores),
        recovery_rollout_path: Some(source),
        ..AgentProcessConfig::default()
    };
    let mut controller = spawn_with_namespace_environment(
        config,
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    controller.wait_until_ready().await.unwrap();
    let paused = wait_for_queue_activity(&shell, |a| {
        a.queue_paused && a.pending_submissions.len() == 1
    })
    .await;
    assert_eq!(paused.pending_submissions[0].submission_id, pending_id);
    assert!(paused.active_submission.is_none());
    assert!(probe.recorded_requests().is_empty());
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    assert!(events.contains(&unknown_id) && events.contains("outcome is unknown"));
    shell
        .write("/agent/1/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    wait_for_queue_activity(&shell, |a| {
        !a.queue_paused
            && a.active_submission.is_none()
            && a.pending_submissions.is_empty()
            && a.state == UiActivityState::Idle
    })
    .await;
    assert_eq!(probe.recorded_requests().len(), 1);
    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    assert!(tape.contains(&pending_id));
    assert!(!tape.contains(&unknown_id));
    controller.shutdown().await.unwrap();
}
