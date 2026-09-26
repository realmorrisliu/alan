use super::*;

struct GatedFirstGeneration {
    mock: MockLlmProvider,
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl LlmProvider for GatedFirstGeneration {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        let first = self.mock.recorded_requests().is_empty();
        let response = self.mock.generate(request).await?;
        if first {
            self.started.notify_one();
            self.release.notified().await;
        }
        Ok(response)
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        Ok(response_stream(self.generate(request).await?))
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> anyhow::Result<String> {
        self.mock.chat(system, user).await
    }

    fn provider_name(&self) -> &'static str {
        "gated_first_generation"
    }
}

#[tokio::test]
async fn ordinary_input_order_and_interrupt_queue_controls() {
    for control in [None, Some("continue"), Some("discard")] {
        let mock = MockLlmProvider::new();
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(GatedFirstGeneration {
                mock: mock.clone(),
                started: started.clone(),
                release: release.clone(),
            }),
        );
        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
        let shell = alan_shell::Shell::new(root.clone());
        let mut core_config = crate::Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("test-model"),
        );
        core_config.memory.enabled = false;
        core_config.streaming_mode = crate::config::StreamingMode::Off;
        let config = AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core_config),
            ..AgentProcessConfig::default()
        };
        let capabilities =
            crate::provider_capabilities_for_config(&config.agent_config.core_config);
        let mut controller = spawn_with_namespace_environment(
            config,
            NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            crate::skills::SkillHostCapabilities::default(),
            capabilities,
        )
        .unwrap();
        controller.wait_until_ready().await.unwrap();
        let tx = &controller.handle.submission_tx;
        let capacity = tx.capacity();
        tx.send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text("first")],
            context: None,
        }))
        .await
        .unwrap();
        let begun = tokio::time::timeout(Duration::from_secs(5), started.notified()).await;
        assert!(
            begun.is_ok(),
            "{:?}",
            String::from_utf8(shell.cat("/agent/1/machine/ui/notice").await.unwrap())
        );
        tx.send(Submission::new(Op::ContinueQueue)).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notice =
                    String::from_utf8(shell.cat("/agent/1/machine/ui/notice").await.unwrap())
                        .unwrap();
                if notice.contains("active input has not settled yet") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        let second = Submission::new(Op::Turn {
            parts: vec![ContentPart::text("second")],
            context: None,
        });
        let third = Submission::new(Op::Input {
            parts: vec![ContentPart::text("third")],
            mode: InputMode::FollowUp,
        });
        tx.send(second.clone()).await.unwrap();
        tx.send(third.clone()).await.unwrap();
        // Wait until both later submissions have been consumed by the input pump.
        tokio::time::timeout(Duration::from_secs(5), async {
            while tx.capacity() != capacity {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if let Some(control) = control {
            if control == "continue" {
                shell
                    .write("/agent/1/machine/ctl", b"interrupt")
                    .await
                    .unwrap();
            } else {
                tx.send(Submission::new(Op::Interrupt)).await.unwrap();
            }
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let activity: serde_json::Value = serde_json::from_slice(
                        &shell.cat("/agent/1/machine/ui/activity").await.unwrap(),
                    )
                    .unwrap();
                    if activity["state"] == "paused" {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
            release.notify_one();
            assert!(
                tokio::time::timeout(Duration::from_millis(50), async {
                    while mock.recorded_requests().len() == 1 {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                })
                .await
                .is_err(),
                "interrupted queue must not generate again"
            );
            if control == "continue" {
                shell
                    .write("/agent/1/machine/ctl", b"queue-v1 continue")
                    .await
                    .unwrap();
            } else {
                let fourth_id = uuid::Uuid::new_v4().to_string();
                let record = serde_json::json!({"version":1,"submission_id":fourth_id,"intent":"agent","mode":"follow_up","body":"fourth"});
                shell
                    .write(
                        "/agent/1/io/input",
                        format!("alan-input-v1\n{record}").as_bytes(),
                    )
                    .await
                    .unwrap();
                // No intake wait: the control must include the just-committed file frame.
                shell
                    .write("/agent/1/machine/ctl", b"queue-v1 discard")
                    .await
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        let notice = String::from_utf8(
                            shell.cat("/agent/1/machine/ui/notice").await.unwrap(),
                        )
                        .unwrap();
                        if notice.contains("Discarded 3 queued inputs") {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                })
                .await
                .unwrap();
                let mut discarded = Vec::new();
                for action in shell.ls("/agent/1/actions").await.unwrap() {
                    if !action.starts_with('a') {
                        continue;
                    }
                    let result: serde_json::Value = serde_json::from_slice(
                        &shell
                            .cat(&format!("/agent/1/actions/{action}/result"))
                            .await
                            .unwrap(),
                    )
                    .unwrap();
                    discarded.push(result["submission_id"].as_str().unwrap().to_owned());
                }
                assert_eq!(discarded, [second.id, third.id, fourth_id]);
                tx.send(Submission::new(Op::Input {
                    parts: vec![ContentPart::text("fresh")],
                    mode: InputMode::FollowUp,
                }))
                .await
                .unwrap();
            }
        } else {
            release.notify_one();
        }
        let expected = if control == Some("discard") {
            vec!["first", "fresh"]
        } else {
            vec!["first", "second", "third"]
        };
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            while mock.recorded_requests().len() < expected.len() {
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(
            completed.is_ok(),
            "control={control:?}, generated={}, activity={}, notice={}",
            mock.recorded_requests().len(),
            String::from_utf8_lossy(&shell.cat("/agent/1/machine/ui/activity").await.unwrap()),
            String::from_utf8_lossy(&shell.cat("/agent/1/machine/ui/notice").await.unwrap())
        );
        let order = mock
            .recorded_requests()
            .iter()
            .map(|request| {
                request
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == alan_llm::MessageRole::User)
                    .unwrap()
                    .content
                    .clone()
            })
            .collect::<Vec<_>>();
        controller.shutdown().await.unwrap();
        assert_eq!(order, expected);
    }
}

#[tokio::test]
async fn ordered_control_boundaries_preserve_later_inputs_and_machine_controls() {
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let files = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default").agent_files();
    let mut queues = RuntimeSubmissionQueues::default();
    assert!(
        !queues
            .handle_control(&Submission::new(Op::Interrupt), &files, None)
            .await
    );
    assert!(
        !queues.is_paused(),
        "idle interruption must not pause later work"
    );
    let cancel = CancellationToken::new();
    for op in [
        Op::Interrupt,
        Op::ContinueQueue,
        Op::DiscardQueue,
        Op::Resume {
            request_id: "pending".into(),
            content: vec![],
        },
    ] {
        let mut invalid = Submission::new(op);
        invalid.intent = alan_agent_protocol::InputIntent::Command;
        assert!(queues.handle_control(&invalid, &files, Some(&cancel)).await);
        assert!(!cancel.is_cancelled());
        assert!(!queues.is_paused());
        let ids = files.action_ids().await.unwrap();
        let action = ids.last().unwrap();
        let result: serde_json::Value = serde_json::from_slice(
            &shell
                .cat(&format!("/agent/1/actions/{action}/result"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["call_id"], invalid.id);
        assert_eq!(result["exit_code"], 1);
    }
    // The aggregate event stream orders committed file input before its control.
    for body in ["first", "second", "third"] {
        shell
            .write("/agent/1/io/input", body.as_bytes())
            .await
            .unwrap();
    }
    shell
        .write("/agent/1/machine/ctl", b"interrupt")
        .await
        .unwrap();
    while let Some(input) = files.read_next_runtime_submission().await.unwrap() {
        if matches!(input.op, Op::Interrupt) {
            queues.handle_control(&input, &files, None).await;
            break;
        }
        queues.push_outer_submission(input);
    }
    assert!(queues.is_paused());
    let (sender, mut receiver) = mpsc::channel(4);
    sender
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text("api")],
            context: None,
        }))
        .await
        .unwrap();
    let compact = Submission::new(Op::CompactWithOptions { focus: None });
    let compact_id = compact.id.clone();
    sender.send(compact).await.unwrap();
    sender
        .send(Submission::new(Op::DiscardQueue))
        .await
        .unwrap();
    let fresh = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("fresh")],
        context: None,
    });
    let fresh_id = fresh.id.clone();
    sender.send(fresh).await.unwrap();
    let discard = queues.admit_api_before_dispatch(&mut receiver).unwrap();
    assert!(matches!(discard.op, Op::DiscardQueue));
    assert_eq!(
        receiver.len(),
        1,
        "later API input stays after the discard boundary"
    );
    assert_eq!(queues.outer_queue.lock().unwrap().pending.len(), 5);
    queues.handle_control(&discard, &files, None).await;
    assert!(!queues.is_paused());
    match queues.pop_outer().unwrap() {
        QueuedRuntimeItem::Submission(input) => assert_eq!(input.id, compact_id),
        _ => panic!("expected retained Machine control"),
    }
    assert!(queues.pop_outer().is_none());
    let actions = shell
        .ls("/agent/1/actions")
        .await
        .unwrap()
        .into_iter()
        .filter(|id| id.starts_with('a'))
        .collect::<Vec<_>>();
    assert_eq!(
        actions.len(),
        8,
        "four rejected commands and four discarded inputs receive results"
    );
    let events = shell.cat("/agent/1/machine/ui/events").await.unwrap();
    let cancelled = std::str::from_utf8(&events)
        .unwrap()
        .lines()
        .filter_map(|line| {
            match serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap() {
                alan_agent_protocol::UiEvent::InputCompleted {
                    submission_ids,
                    status: alan_agent_protocol::UiInputStatus::Cancelled,
                    ..
                } => Some(submission_ids),
                _ => None,
            }
        })
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(cancelled.len(), 4);
    assert!(
        !cancelled.contains(&fresh_id),
        "later input is not cancelled"
    );
    assert!(
        !cancelled.contains(&compact_id),
        "Machine control is not user input"
    );
    for action in actions {
        assert_eq!(
            shell
                .cat(&format!("/agent/1/actions/{action}/status"))
                .await
                .unwrap(),
            b"failed"
        );
    }
    assert!(queues.admit_api_before_dispatch(&mut receiver).is_none());
    assert!(
        matches!(queues.pop_outer(), Some(QueuedRuntimeItem::Submission(input)) if input.id == fresh_id)
    );
    shell
        .write("/agent/1/machine/ctl", b"queue-v1 discard")
        .await
        .unwrap();
    shell
        .write("/agent/1/io/input", b"later file input")
        .await
        .unwrap();
    assert!(matches!(
        files
            .read_next_runtime_submission()
            .await
            .unwrap()
            .unwrap()
            .op,
        Op::DiscardQueue
    ));
    assert!(matches!(
        files
            .read_next_runtime_submission()
            .await
            .unwrap()
            .unwrap()
            .op,
        Op::Input { .. }
    ));
}

#[tokio::test]
async fn ready_namespace_batch_includes_interrupt_after_input_burst() {
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let files = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default").agent_files();
    for index in 0..128 {
        shell
            .write("/agent/1/io/input", format!("input-{index}").as_bytes())
            .await
            .unwrap();
    }
    shell
        .write("/agent/1/machine/ctl", b"interrupt")
        .await
        .unwrap();
    let batch = files.read_ready_runtime_submissions().await.unwrap();
    assert_eq!(batch.len(), 129, "one wake must reach the ready interrupt");
    for (index, input) in batch.into_iter().enumerate() {
        match input.unwrap().op {
            Op::Input { parts, .. } => {
                assert_eq!(parts, vec![ContentPart::text(format!("input-{index}"))])
            }
            Op::Interrupt => assert_eq!(index, 128),
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert!(
        files
            .read_ready_runtime_submissions()
            .await
            .unwrap()
            .is_empty()
    );
    shell.write("/agent/1/io/input", b"later").await.unwrap();
    assert_eq!(
        files.read_ready_runtime_submissions().await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn namespace_rollback_stays_between_earlier_and_later_input() {
    let mock = MockLlmProvider::new();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    // Commit the complete ordered sequence before the Process pump starts.
    shell
        .write("/agent/1/io/input", b"before rollback")
        .await
        .unwrap();
    shell
        .write("/agent/1/machine/ctl", b"rollback")
        .await
        .unwrap();
    shell
        .write("/agent/1/io/input", b"after rollback")
        .await
        .unwrap();
    let mut core_config =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core_config.memory.enabled = false;
    core_config.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core_config);
    let mut controller = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core_config),
            ..Default::default()
        },
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    controller.wait_until_ready().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.recorded_requests().len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let requests = mock.recorded_requests();
    controller.shutdown().await.unwrap();
    let users = |request: &GenerationRequest| {
        request
            .messages
            .iter()
            .filter(|message| message.role == alan_llm::MessageRole::User)
            .map(|message| message.content.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(users(&requests[0]), ["before rollback"]);
    assert_eq!(users(&requests[1]), ["after rollback"]);
}

#[tokio::test]
async fn continuous_file_input_does_not_starve_dispatch() {
    let mock = MockLlmProvider::new();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    shell.write("/agent/1/io/input", b"first").await.unwrap();
    let writer = tokio::spawn(async move {
        loop {
            shell.write("/agent/1/io/input", b"queued").await.unwrap();
            tokio::task::yield_now().await;
        }
    });
    let mut core_config =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core_config.memory.enabled = false;
    core_config.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core_config);
    let mut controller = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core_config),
            ..Default::default()
        },
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    controller.wait_until_ready().await.unwrap();
    let dispatched = tokio::time::timeout(Duration::from_secs(5), async {
        while mock.recorded_requests().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    writer.abort();
    let _ = writer.await;
    controller.shutdown().await.unwrap();
    assert!(
        dispatched.is_ok(),
        "continuous file arrivals must not starve queued dispatch"
    );
}
