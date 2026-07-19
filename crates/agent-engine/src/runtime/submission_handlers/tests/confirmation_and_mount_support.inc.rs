    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::{
        agent_machine::AgentMachine,
        config::Config,
        rollout::{RolloutItem, RolloutRecorder},
        runtime::{MountGrantApplicator, NamespaceRuntimeEnvironment, RuntimeConfig},
        tape::ContentPart,
        tools::ToolRegistry,
    };
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace, ProcFs};
    use alan_shell::Shell;
    use tempfile::TempDir;

    #[derive(Debug, Default)]
    struct RecordingMountGrantApplicator {
        grants: Mutex<Vec<ApprovedMountGrant>>,
        fail_with: Option<&'static str>,
    }

    impl RecordingMountGrantApplicator {
        fn failing(message: &'static str) -> Self {
            Self {
                grants: Mutex::new(Vec::new()),
                fail_with: Some(message),
            }
        }

        fn grants(&self) -> Vec<ApprovedMountGrant> {
            self.grants.lock().unwrap().clone()
        }
    }

    impl MountGrantApplicator for RecordingMountGrantApplicator {
        fn apply_mount_grant(&self, grant: &ApprovedMountGrant) -> Result<Namespace> {
            self.grants.lock().unwrap().push(grant.clone());
            if let Some(message) = self.fail_with {
                anyhow::bail!(message);
            }
            let access = match grant.access {
                ApprovedMountGrantAccess::ReadOnly => Access::ReadOnly,
                ApprovedMountGrantAccess::ReadWrite => Access::ReadWrite,
            };
            let mut namespace = Namespace::new();
            namespace.mount(
                &grant.namespace_path,
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
                access,
            );
            Ok(namespace)
        }
    }

    fn namespace_environment_for_test() -> NamespaceRuntimeEnvironment {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        attach_test_process_context(NamespaceRuntimeEnvironment::new(
            root, "/agent/1", "default",
        ))
    }

    fn namespace_environment_with_mount_applicator_for_test(
        applicator: Arc<dyn MountGrantApplicator>,
    ) -> NamespaceRuntimeEnvironment {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        attach_test_process_context(
            NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
                .with_mount_grant_applicator(applicator),
        )
    }

    fn attach_test_process_context(
        environment: NamespaceRuntimeEnvironment,
    ) -> NamespaceRuntimeEnvironment {
        let runner = crate::tools::ToolProcessRunner::from_registry(&ToolRegistry::new());
        environment.with_tool_process_context(alan_kernel::Pid(1), runner)
    }

    async fn namespace_environment_with_live_process_for_test()
    -> (NamespaceRuntimeEnvironment, Shell) {
        let procfs = Arc::new(ProcFs::new());
        let agentfs = Arc::new(alan_agentfs::AgentFs::new());
        let mut namespace = Namespace::new();
        namespace.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let shell = Shell::new(root.clone());
        let pid = shell
            .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
            .await
            .unwrap();
        assert_eq!(pid, "1");
        (
            NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            shell,
        )
    }

    fn create_test_state() -> RuntimeLoopState {
        let config = Config::default();
        let machine = AgentMachine::new();
        let runtime_config = RuntimeConfig::default();

        RuntimeLoopState {
            machine,
            environment: namespace_environment_for_test(),
            core_config: config,
            runtime_config,
            definition_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        }
    }

    fn bind_test_source_mount(state: &mut RuntimeLoopState, source: &std::path::Path) {
        let launch_context = crate::ProcessLaunchContext::new(
            Namespace::new(),
            alan_kernel::Credentials::user("test-agent"),
            "/mnt/source",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new("/mnt/source", source, alan_kernel::Access::ReadWrite)
                .unwrap(),
        );
        state.environment = state
            .environment
            .clone()
            .with_launch_context(launch_context.clone());
        assert!(
            state.namespace_environment().set_tool_execution_binding(
                crate::tools::ToolExecutionBinding::from_launch_context(
                    &launch_context,
                    source.join("scratch"),
                )
                .unwrap(),
            )
        );
    }

    fn mount_escalation_pending_confirmation_with(
        host_path: &str,
        access: &str,
        reason: &str,
    ) -> crate::approval::PendingConfirmation {
        crate::approval::PendingConfirmation {
            checkpoint_id: "mount_escalation_call_mount".to_string(),
            checkpoint_type: crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE.to_string(),
            summary: "Approve host mount?".to_string(),
            details: json!({
                "kind": "mount_escalation",
                "tool_call_id": "call_mount",
                "tool_name": "request_mount",
                "mount_request": {
                    "namespace_path": "/mnt/project",
                    "host_path": host_path,
                    "access": access,
                    "reason": reason
                },
                "live_applied": false
            }),
            options: vec!["approve".to_string(), "reject".to_string()],
        }
    }

    fn tool_result_text_for_call(state: &RuntimeLoopState, call_id: &str) -> String {
        state
            .machine.messages()
            .iter()
            .rev()
            .find_map(|message| match message {
                crate::tape::Message::Tool { responses } => responses
                    .iter()
                    .rev()
                    .find(|response| response.id == call_id)
                    .map(crate::tape::ToolResponse::text_content),
                _ => None,
            })
            .expect("expected tool result")
    }

    fn tool_result_json_for_call(state: &RuntimeLoopState, call_id: &str) -> serde_json::Value {
        serde_json::from_str(&tool_result_text_for_call(state, call_id))
            .expect("tool result should be json")
    }

    #[tokio::test]
    async fn test_turn_context_has_no_process_identity_gate() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![ContentPart::text("test input")],
            context: Some(alan_agent_protocol::TurnContext {
                ..alan_agent_protocol::TurnContext::default()
            }),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        assert!(matches!(result.unwrap(), RuntimeOpAction::RunTurn { .. }));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Error { .. }))
        );
    }

    #[tokio::test]
    async fn test_handle_start_task_correct_agent() {
        let mut state = create_test_state();
        state.machine.add_user_message("existing message");
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![ContentPart::text("test input")],
            context: Some(alan_agent_protocol::TurnContext {
                reasoning_effort: Some(alan_agent_protocol::ReasoningEffort::High),
            }),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                user_input,
                activate_task,
                ..
            } => {
                assert!(activate_task);
                assert!(user_input.is_some());
                let text = alan_agent_protocol::parts_to_text(&user_input.unwrap());
                assert!(text.contains("test input"));
                // Turn should preserve existing conversation history.
                assert_eq!(state.machine.messages().len(), 1);
                assert_eq!(
                    state.machine.messages()[0].text_content(),
                    "existing message"
                );
                assert_eq!(
                    state
                        .machine
                        .active_turn_request_control_intent()
                        .reasoning_effort,
                    Some(alan_agent_protocol::ReasoningEffort::High)
                );
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_start_task_preserves_attachments_without_identity_field() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![
                ContentPart::text("test input"),
                ContentPart::Attachment {
                    hash: "doc1.pdf".to_string(),
                    mime_type: "application/pdf".to_string(),
                    metadata: serde_json::Value::Null,
                },
                ContentPart::Attachment {
                    hash: "doc2.pdf".to_string(),
                    mime_type: "application/pdf".to_string(),
                    metadata: serde_json::Value::Null,
                },
            ],
            context: Some(alan_agent_protocol::TurnContext::default()),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn { user_input, .. } => {
                let parts = user_input.unwrap();
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].as_text(), Some("test input"));
                assert!(matches!(parts[1], ContentPart::Attachment { .. }));
                assert!(matches!(parts[2], ContentPart::Attachment { .. }));
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_confirm_no_pending() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "chk_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(
                    |e| matches!(e, Event::Error { message, .. } if message.contains("does not match")),
                );
                assert!(has_error);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_confirm_wrong_checkpoint() {
        let mut state = create_test_state();
        state
            .machine
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "other_checkpoint".to_string(),
                checkpoint_type: "test".to_string(),
                summary: "Test".to_string(),
                details: json!({}),
                options: vec!["approve".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "chk_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(|e| {
                    matches!(e, Event::Error { message, .. } if message.contains("does not match"))
                });
                assert!(has_error);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_confirm_approve() {
        let mut state = create_test_state();
        state
            .machine
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "chk_123".to_string(),
                checkpoint_type: "test".to_string(),
                summary: "Test".to_string(),
                details: json!({
                    "replay_tool_call": {
                        "call_id": "call_1",
                        "tool_name": "read_file",
                        "arguments": {"path": "test.txt"}
                    }
                }),
                options: vec!["approve".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "chk_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        // Tool message should be recorded
        let messages = state.machine.messages();
        assert!(!messages.is_empty());
        assert!(messages[0].text_content().contains("approve"));
    }

    #[tokio::test]
    async fn test_handle_confirm_with_modifications() {
        let mut state = create_test_state();
        state
            .machine
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "chk_123".to_string(),
                checkpoint_type: "test".to_string(),
                summary: "Test".to_string(),
                details: json!({}),
                options: vec!["approve".to_string(), "modify".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "chk_123".to_string(),
            content: vec![ContentPart::structured(json!({
                "choice": "modify",
                "modifications": "Changed something"
            }))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        // Tool message should contain modifications
        let messages = state.machine.messages();
        assert!(!messages.is_empty());
        assert!(messages[0].text_content().contains("modify"));
    }

    #[tokio::test]
    async fn test_runtime_confirmation_resume_persists_checkpoint_with_knowledge_root() {
        let temp = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine = AgentMachine::new_with_recorder_in_dir(
            "runtime-confirmation-checkpoint-with-root",
            "test-model",
            temp.path(),
        )
        .await
        .unwrap();
        let (environment, _shell) = namespace_environment_with_live_process_for_test().await;
        state.environment = environment;
        state
            .agent_files()
            .write_user_state("seed confirmation context")
            .await
            .unwrap();
        let expected_root = state
            .agent_files()
            .current_tape_checkpoint()
            .await
            .unwrap();
        state
            .machine
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "tool_escalation_call_123".to_string(),
                checkpoint_type: crate::approval::TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary: "Approve tool escalation?".to_string(),
                details: json!({}),
                options: vec!["approve".to_string(), "reject".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "tool_escalation_call_123".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "reject"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let checkpoint = items
            .iter()
            .find_map(|item| match item {
                RolloutItem::Checkpoint(checkpoint)
                    if checkpoint.checkpoint_id == "tool_escalation_call_123" =>
                {
                    Some(checkpoint)
                }
                _ => None,
            })
            .expect("expected persisted runtime confirmation checkpoint");
        assert_eq!(
            checkpoint.checkpoint_type,
            crate::approval::TOOL_ESCALATION_CHECKPOINT_TYPE
        );
        assert_eq!(checkpoint.choice.as_deref(), Some("rejected"));
        assert_eq!(
            checkpoint.knowledge_root.as_deref(),
            Some(expected_root.as_str())
        );
    }

    #[tokio::test]
    async fn test_runtime_confirmation_resume_persists_checkpoint_without_knowledge_root_on_read_failure()
     {
        let temp = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine = AgentMachine::new_with_recorder_in_dir(
            "runtime-confirmation-checkpoint-no-root",
            "test-model",
            temp.path(),
        )
        .await
        .unwrap();
        let root = InProcessTransport::new(Arc::new(MountFs::new(Namespace::new())));
        state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        state
            .machine
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "tool_escalation_call_456".to_string(),
                checkpoint_type: crate::approval::TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary: "Approve tool escalation?".to_string(),
                details: json!({}),
                options: vec!["approve".to_string(), "reject".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "tool_escalation_call_456".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "reject"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let checkpoint = items
            .iter()
            .find_map(|item| match item {
                RolloutItem::Checkpoint(checkpoint)
                    if checkpoint.checkpoint_id == "tool_escalation_call_456" =>
                {
                    Some(checkpoint)
                }
                _ => None,
            })
            .expect("expected persisted runtime confirmation checkpoint");
        assert_eq!(checkpoint.choice.as_deref(), Some("rejected"));
        assert!(checkpoint.knowledge_root.is_none());
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_approve_records_grant_and_tool_result() {
        let temp = TempDir::new().unwrap();
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine =
            AgentMachine::new_with_recorder_in_dir("mount-approve", "test-model", temp.path())
                .await
                .unwrap();
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .machine
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_write",
                "Need to edit project files",
            ));
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "mount_escalation_call_mount".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["status"], "approved");
        assert_eq!(tool_result["tool_sandbox_applied"], false);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], false);
        assert_eq!(tool_result["namespace_applied"], false);
        assert_eq!(
            tool_result["namespace_error"],
            "live namespace mount applicator unavailable"
        );
        assert_eq!(tool_result["live_applied"], false);
        assert_eq!(
            tool_result["mount_request"]["namespace_path"],
            "/mnt/project"
        );
        let roots = state.namespace_environment().tool_sandbox_writable_roots();
        assert_eq!(
            roots,
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let grant = items
            .iter()
            .find_map(|item| match item {
                RolloutItem::Event(event) if event.event_type == "host_mount_grant" => Some(event),
                _ => None,
            })
            .expect("expected approved mount grant event");
        assert_eq!(grant.payload["namespace_path"], "/mnt/project");
        assert_eq!(
            grant.payload["host_path"],
            approved_host.path().to_str().unwrap()
        );
        assert_eq!(grant.payload["access"], "read_write");
        assert_eq!(grant.payload["reason"], "Need to edit project files");
        assert_eq!(
            grant.payload["checkpoint_id"],
            "mount_escalation_call_mount"
        );
        assert_eq!(grant.payload["approved"], true);
        assert_eq!(grant.payload["live_applied"], false);
        assert_eq!(grant.payload["namespace_applied"], false);
        assert_eq!(
            grant.payload["namespace_error"],
            "live namespace mount applicator unavailable"
        );
        assert_eq!(grant.payload["tool_sandbox_applied"], false);
        assert_eq!(grant.payload["tool_sandbox_projection_changed"], false);
        assert_eq!(grant.payload["tool_call_id"], "call_mount");
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_applies_namespace_with_applicator() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment =
            namespace_environment_with_mount_applicator_for_test(applicator.clone());
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .machine
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_write",
                "Need to edit project files",
            ));
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "mount_escalation_call_mount".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["namespace_applied"], true);
        assert_eq!(tool_result["namespace_error"], serde_json::Value::Null);
        assert_eq!(tool_result["tool_sandbox_applied"], true);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], true);
        assert_eq!(
            state.namespace_environment().tool_sandbox_writable_roots(),
            vec![
                dunce::canonicalize(host_mount_root.path()).unwrap(),
                dunce::canonicalize(approved_host.path()).unwrap()
            ]
        );
        let grants = applicator.grants();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].namespace_path, "/mnt/project");
        assert_eq!(grants[0].host_path, approved_host.path());
        assert_eq!(grants[0].access, ApprovedMountGrantAccess::ReadWrite);
        assert_eq!(grants[0].reason, "Need to edit project files");

        let child_context = state
            .namespace_environment()
            .launch_context()
            .expect("approved grant should persist in the Process Launch Context")
            .child();
        assert_eq!(
            child_context.host_path("/mnt/project/file.txt"),
            Some(approved_host.path().join("file.txt"))
        );
        assert!(
            child_context
                .namespace
                .describe()
                .iter()
                .any(|(path, access)| path == "/mnt/project" && *access == Access::ReadWrite)
        );
    }
