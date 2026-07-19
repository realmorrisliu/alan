
    #[tokio::test]
    async fn test_first_approved_mount_creates_process_tool_binding() {
        let system_store = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment = namespace_environment_with_mount_applicator_for_test(applicator)
            .with_launch_context(crate::ProcessLaunchContext::root());
        state.runtime_config.store_bindings = Some(crate::AgentRuntimeStoreBindings {
            rollouts: system_store.path().join("rollouts"),
            checkpoints: system_store.path().join("checkpoints"),
            cache: system_store.path().join("cache"),
            tmp: system_store.path().join("tmp"),
            metadata: system_store.path().join("metadata"),
        });
        state
            .machine
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_write",
                "Need project files",
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

        let binding = state
            .tool_execution()
            .execution_binding()
            .expect("first approved Host Mount should create a Tool binding");
        assert_eq!(
            binding.cwd,
            dunce::canonicalize(approved_host.path()).unwrap()
        );
        assert_eq!(binding.namespace_cwd, std::path::Path::new("/mnt/project"));
        assert_eq!(
            state
                .child_launch()
                .launch_context()
                .expect("the Process Launch Context should remain available")
                .cwd,
            "/"
        );
        assert_eq!(binding.host_mounts.len(), 1);
        assert_eq!(binding.host_mounts[0].namespace_path, "/mnt/project");
        assert_eq!(
            binding.host_mounts[0].resolve_host_path("/mnt/project/file.txt"),
            Some(approved_host.path().join("file.txt"))
        );
        let sandbox = binding.sandbox_spec.as_ref().unwrap();
        assert!(
            !sandbox
                .readable_roots
                .iter()
                .any(|root| root == &system_store.path().join("tmp"))
        );
        let execution = crate::tools::Sandbox::from_spec_with_backend(
            sandbox.clone(),
            crate::tools::SandboxBackendKind::HostMountPathGuard,
        )
        .exec("pwd", &binding.cwd)
        .await
        .unwrap();
        assert_eq!(execution.exit_code, 0, "{execution:?}");
        assert_eq!(
            execution.stdout.trim(),
            binding.cwd.to_string_lossy().as_ref()
        );
        let result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(result["tool_sandbox_applied"], true);
        assert_eq!(result["tool_sandbox_projection_changed"], true);
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_read_only_applies_namespace_only() {
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
                "read_only",
                "Need to inspect project files",
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
        assert_eq!(tool_result["tool_sandbox_applied"], true);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], true);
        assert_eq!(
            state.tool_execution().sandbox_writable_roots(),
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );
        let grants = applicator.grants();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].access, ApprovedMountGrantAccess::ReadOnly);
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_reports_namespace_apply_failure() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::failing("mount failed"));
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
        assert_eq!(tool_result["namespace_applied"], false);
        assert_eq!(tool_result["namespace_error"], "mount failed");
        assert_eq!(tool_result["tool_sandbox_applied"], false);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], false);
        assert_eq!(
            state.tool_execution().sandbox_writable_roots(),
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );
        let grants = applicator.grants();
        assert_eq!(grants.len(), 1);
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_duplicate_read_write_grant_is_idempotent() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment = namespace_environment_with_mount_applicator_for_test(applicator);
        bind_test_source_mount(&mut state, host_mount_root.path());
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        for _ in 0..2 {
            state
                .machine
                .set_confirmation(mount_escalation_pending_confirmation_with(
                    approved_host.path().to_str().unwrap(),
                    "read_write",
                    "Need to edit project files",
                ));
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
        }

        let roots = state.tool_execution().sandbox_writable_roots();
        assert_eq!(
            roots,
            vec![
                dunce::canonicalize(host_mount_root.path()).unwrap(),
                dunce::canonicalize(approved_host.path()).unwrap()
            ]
        );
        let latest = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(latest["tool_sandbox_applied"], true);
        assert_eq!(latest["tool_sandbox_projection_changed"], false);
    }

    #[tokio::test]
    async fn test_reapproved_namespace_path_replaces_persisted_host_grant() {
        let host_mount_root = TempDir::new().unwrap();
        let first_host = TempDir::new().unwrap();
        let replacement_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment = namespace_environment_with_mount_applicator_for_test(applicator);
        bind_test_source_mount(&mut state, host_mount_root.path());
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        for host in [first_host.path(), replacement_host.path()] {
            state
                .machine
                .set_confirmation(mount_escalation_pending_confirmation_with(
                    host.to_str().unwrap(),
                    "read_write",
                    "Replace project mount",
                ));
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
        }

        let launch_context = state
            .child_launch()
            .launch_context()
            .cloned()
            .expect("approved grant should remain in the Process Launch Context");
        assert_eq!(
            launch_context.host_path("/mnt/project/file.txt"),
            Some(replacement_host.path().join("file.txt"))
        );
        assert_eq!(
            state.tool_execution().sandbox_writable_roots(),
            vec![
                dunce::canonicalize(host_mount_root.path()).unwrap(),
                dunce::canonicalize(replacement_host.path()).unwrap()
            ]
        );
        let latest = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(latest["tool_sandbox_applied"], true);
        assert_eq!(latest["tool_sandbox_projection_changed"], true);
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_read_only_grant_does_not_expand_tool_sandbox() {
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let applicator = Arc::new(RecordingMountGrantApplicator::default());
        let mut state = create_test_state();
        state.environment = namespace_environment_with_mount_applicator_for_test(applicator);
        bind_test_source_mount(&mut state, host_mount_root.path());
        state
            .machine
            .set_confirmation(mount_escalation_pending_confirmation_with(
                approved_host.path().to_str().unwrap(),
                "read_only",
                "Need to inspect project files",
            ));
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        let result = handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "mount_escalation_call_mount".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await;
        assert!(result.is_ok());

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["status"], "approved");
        assert_eq!(tool_result["tool_sandbox_applied"], true);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], true);
        assert_eq!(
            state.tool_execution().sandbox_writable_roots(),
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_reject_returns_tool_result_without_grant() {
        let temp = TempDir::new().unwrap();
        let host_mount_root = TempDir::new().unwrap();
        let approved_host = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine =
            AgentMachine::new_with_recorder_in_dir("mount-reject", "test-model", temp.path())
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

        let tool_result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(tool_result["status"], "rejected");
        assert_eq!(tool_result["approved"], false);
        assert_eq!(tool_result["tool_sandbox_applied"], false);
        assert_eq!(tool_result["tool_sandbox_projection_changed"], false);
        assert_eq!(tool_result["live_applied"], false);
        assert_eq!(
            state.tool_execution().sandbox_writable_roots(),
            vec![dunce::canonicalize(host_mount_root.path()).unwrap()]
        );

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        assert!(!items.iter().any(|item| matches!(
            item,
            RolloutItem::Event(event) if event.event_type == "host_mount_grant"
        )));
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_missing_choice_defaults_to_reject() {
        let temp = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine = AgentMachine::new_with_recorder_in_dir(
            "mount-default-reject",
            "test-model",
            temp.path(),
        )
        .await
        .unwrap();
        let host_path = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        let host_path = host_path.display().to_string();
        state
            .machine
            .set_confirmation(mount_escalation_pending_confirmation_with(
                &host_path,
                "read_write",
                "Need to edit project files",
            ));
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "mount_escalation_call_mount".to_string(),
            content: vec![ContentPart::structured(json!({}))],
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

        let tool_result = tool_result_text_for_call(&state, "call_mount");
        assert!(tool_result.contains("\"status\":\"rejected\""));
        assert!(tool_result.contains("\"choice\":\"reject\""));
        assert!(tool_result.contains("\"approved\":false"));

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        assert!(!items.iter().any(|item| matches!(
            item,
            RolloutItem::Event(event) if event.event_type == "host_mount_grant"
        )));
    }

    #[tokio::test]
    async fn test_handle_mount_escalation_resume_rejects_forged_checkpoint() {
        let temp = TempDir::new().unwrap();
        let mut state = create_test_state();
        state.machine =
            AgentMachine::new_with_recorder_in_dir("mount-forged", "test-model", temp.path())
                .await
                .unwrap();
        state
            .machine
            .set_confirmation(crate::approval::PendingConfirmation {
                checkpoint_id: "forged_mount".to_string(),
                checkpoint_type: crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE.to_string(),
                summary: "Approve forged mount?".to_string(),
                details: json!({
                    "kind": "mount_escalation",
                    "tool_call_id": "call_mount",
                    "tool_name": "request_confirmation",
                    "mount_request": {
                        "namespace_path": "/mnt/project",
                        "host_path": "relative/path",
                        "access": "read_write",
                        "reason": "forged"
                    },
                    "live_applied": false
                }),
                options: vec!["approve".to_string(), "reject".to_string()],
            });
        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "forged_mount".to_string(),
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

        let tool_result = tool_result_text_for_call(&state, "forged_mount");
        assert!(tool_result.contains("\"status\":\"invalid_mount_escalation_checkpoint\""));
        assert!(tool_result.contains("\"approved\":false"));

        state.machine.flush().await;
        let rollout_path = state.machine.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        assert!(!items.iter().any(|item| matches!(
            item,
            RolloutItem::Event(event) if event.event_type == "host_mount_grant"
        )));
    }

    #[tokio::test]
    async fn test_handle_user_input() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Input {
            parts: vec![ContentPart::text("Hello world")],
            mode: InputMode::Steer,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(|e| {
                    matches!(e, Event::Error { message, .. } if message.contains("Use Op::Turn"))
                });
                assert!(
                    has_error,
                    "Expected guidance error for Input without active turn"
                );
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_structured_user_input_no_pending() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "req_123".to_string(),
            content: vec![ContentPart::structured(json!({"answers": []}))],
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
    async fn test_handle_structured_user_input_wrong_id() {
        let mut state = create_test_state();
        state
            .machine
            .set_structured_input(crate::approval::PendingStructuredInputRequest {
                request_id: "other_id".to_string(),
                title: "Test".to_string(),
                prompt: "Test".to_string(),
                questions: vec![],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "req_123".to_string(),
            content: vec![ContentPart::structured(json!({"answers": []}))],
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
    async fn test_handle_structured_user_input_success() {
        let mut state = create_test_state();
        state
            .machine
            .set_structured_input(crate::approval::PendingStructuredInputRequest {
                request_id: "req_123".to_string(),
                title: "Test".to_string(),
                prompt: "Test".to_string(),
                questions: vec![],
            });
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "req_123".to_string(),
            content: vec![ContentPart::structured(json!({
                "answers": [{"question_id": "q1", "value": "answer1"}]
            }))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                user_input,
                activate_task,
                turn_kind,
            } => {
                assert!(!activate_task);
                assert!(user_input.is_none());
                assert!(matches!(turn_kind, TurnRunKind::ResumeTurn));
            }
            _ => panic!("Expected RunTurn"),
        }

        // Verify tool message was recorded
        assert!(!state.machine.messages().is_empty());
    }

    #[tokio::test]
    async fn test_handle_compact_without_focus() {
        let mut state = create_test_state();
        // Add some messages to make compaction meaningful
        for i in 0..10 {
            state.machine.add_user_message(&format!("Message {}", i));
        }
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::CompactWithOptions { focus: None };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                // Compaction completed
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_compact_with_options() {
        let mut state = create_test_state();
        for i in 0..10 {
            state.machine.add_user_message(&format!("Message {}", i));
        }
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::CompactWithOptions {
            focus: Some("preserve todos".to_string()),
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), RuntimeOpAction::NoTurn));
    }

    #[tokio::test]
    async fn test_handle_rollback_invalid_zero() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Rollback { turns: 0 };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_error = events.iter().any(|e| {
                    matches!(e, Event::Error { message, .. } if message.contains("turns must be >= 1"))
                });
                assert!(has_error);
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_rollback_success() {
        let mut state = create_test_state();
        state.machine.add_user_message("u1");
        state.machine.add_assistant_message("a1", None);
        state.machine.add_user_message("u2");
        state.machine.add_assistant_message("a2", None);

        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Rollback { turns: 1 };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                let has_machine_rolled_back = events.iter().any(|e| {
                    matches!(
                        e,
                        Event::MachineRolledBack {
                            turns: 1,
                            removed_messages: 2,
                        }
                    )
                });
                assert!(has_machine_rolled_back);
                let has_confirmation = events.iter().any(
                    |e| matches!(
                        e,
                        Event::TextDelta { chunk, is_final }
                            if *is_final && chunk.contains("Rolled back 1 turn(s), removed 2 message(s).")
                    ),
                );
                assert!(has_confirmation);
                let has_warning = events.iter().any(|e| {
                    matches!(
                        e,
                        Event::Warning { message }
                            if message == ROLLBACK_NON_DURABLE_WARNING
                    )
                });
                assert!(has_warning);
            }
            _ => panic!("Expected NoTurn"),
        }
    }
