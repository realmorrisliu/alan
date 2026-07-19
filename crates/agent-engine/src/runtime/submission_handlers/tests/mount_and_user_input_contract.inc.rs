    async fn pending_host_mount_state(
    ) -> (RuntimeLoopState, Arc<TestHostMountFs>, String) {
        let (mut state, host_mount) = create_test_state_with_host_mount();
        let request = json!({
            "namespace_path": "/mnt/project",
            "access": "read_write",
            "reason": "Need project files",
            "label": "Project"
        });
        let request_id = state
            .environment
            .host_mount_requests()
            .create(&serde_json::to_vec(&request).unwrap())
            .await
            .unwrap();
        state.machine.set_host_mount_request(
            crate::agent_machine::PendingHostMountRequest {
                request_id: request_id.clone(),
                tool_call_id: "call_mount".to_string(),
                namespace_path: "/mnt/project".to_string(),
                access: "read_write".to_string(),
                reason: "Need project files".to_string(),
                label: Some("Project".to_string()),
                request_events_offset: 0,
            },
        );
        (state, host_mount, request_id)
    }

    #[tokio::test]
    async fn agent_resume_cannot_approve_pending_host_mount() {
        let (mut state, _host_mount, request_id) = pending_host_mount_state().await;
        let cancel = CancellationToken::new();
        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let action = handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: request_id.clone(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        assert!(matches!(action, RuntimeOpAction::NoTurn));
        assert!(state.machine.pending_host_mount(&request_id).is_some());
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Error { message, recoverable }
                if *recoverable && message.contains("only Host Mount Service can settle")
        )));
        assert!(state
            .machine
            .messages()
            .iter()
            .all(|message| !matches!(message, crate::tape::Message::Tool { .. })));
    }

    #[tokio::test]
    async fn new_turn_cancels_pending_host_mount_before_resetting_machine_state() {
        let (mut state, host_mount, request_id) = pending_host_mount_state().await;
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        let action = handle_runtime_op_with_cancel(
            &mut state,
            Op::Turn {
                parts: vec![ContentPart::text("Start over")],
                context: None,
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        assert!(matches!(
            action,
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::NewTurn,
                ..
            }
        ));
        assert!(state.machine.pending_host_mount(&request_id).is_none());
        assert_eq!(host_mount.status(&request_id).await.as_deref(), Some("cancelled"));
    }

    #[tokio::test]
    async fn approved_service_request_resumes_with_opaque_grant_only() {
        let (mut state, host_mount, request_id) = pending_host_mount_state().await;
        state.environment = state
            .environment
            .clone()
            .with_launch_context(crate::ProcessLaunchContext::root());
        host_mount
            .settle(&request_id, "approved", Some("grant-opaque-1"), None)
            .await;
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        let action = handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: request_id.clone(),
                content: vec![ContentPart::structured(json!({"choice": "reject"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        assert!(matches!(
            action,
            RuntimeOpAction::RunTurn {
                turn_kind: TurnRunKind::ResumeTurn,
                ..
            }
        ));
        assert!(state.machine.pending_host_mount(&request_id).is_none());
        let result = tool_result_json_for_call(&state, "call_mount");
        assert_eq!(result["status"], "approved");
        assert_eq!(result["approved"], true);
        assert_eq!(result["request_reference"], request_id);
        assert_eq!(result["grant_reference"], "grant-opaque-1");
        assert_eq!(result["namespace_path"], "/mnt/project");
        assert!(!result.to_string().contains("host_path"));
        assert!(result.get("namespace_applied").is_none());
        assert!(result.get("tool_sandbox_applied").is_none());
        assert_eq!(
            state
                .environment
                .child_launch()
                .launch_context()
                .unwrap()
                .projected_host_mounts(),
            vec![("/mnt/project".to_string(), alan_kernel::Access::ReadWrite)]
        );
        assert_eq!(
            state
                .environment
                .child_launch()
                .launch_context()
                .unwrap()
                .projected_host_mount_references(),
            vec!["grant-opaque-1".to_string()]
        );
    }

    #[tokio::test]
    async fn every_non_approval_terminal_status_resumes_without_grant() {
        for status in ["rejected", "cancelled", "failed"] {
            let (mut state, host_mount, request_id) = pending_host_mount_state().await;
            host_mount
                .settle(&request_id, status, None, Some("Host authorization did not complete"))
                .await;
            let cancel = CancellationToken::new();
            let mut emit = |_event: Event| async {};

            let action = handle_runtime_op_with_cancel(
                &mut state,
                Op::Resume {
                    request_id: request_id.clone(),
                    content: Vec::new(),
                },
                &mut emit,
                &cancel,
            )
            .await
            .unwrap();

            assert!(matches!(
                action,
                RuntimeOpAction::RunTurn {
                    turn_kind: TurnRunKind::ResumeTurn,
                    ..
                }
            ));
            let result = tool_result_json_for_call(&state, "call_mount");
            assert_eq!(result["status"], status);
            assert_eq!(result["approved"], false);
            assert!(result["grant_reference"].is_null());
            assert_eq!(result["error"], "Host authorization did not complete");
        }
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
