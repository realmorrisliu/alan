
    use alan_agent_protocol::{InputMode, Op};
    use crate::runtime::turn_driver::MAX_BUFFERED_INBAND_USER_INPUTS;

    #[tokio::test]
    async fn namespace_tool_call_spawns_bin_executable_and_records_action() {
        let (mut state, shell) = create_namespace_test_state_and_shell();

        let (outcome, events) = execute_single_tool_call(
            &mut state,
            "call-read",
            "read_file",
            json!({ "path": "sample.txt" }),
        )
        .await;

        assert!(matches!(
            outcome,
            ToolBatchOrchestratorOutcome::ContinueTurnLoop {
                refresh_context: false
            }
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::ToolCallCompleted {
                id,
                name,
                success: Some(true),
                ..
            } if id == "call-read" && name.as_deref() == Some("read_file")
        )));
        let payload = state
            .machine
            .tool_payload_by_call_id("call-read")
            .expect("tool response payload should be recorded on tape");
        assert_eq!(payload["success"], json!(true));
        assert_eq!(payload["content"], json!("from namespace read_file"));
        assert_eq!(payload["arguments"], json!({ "path": "sample.txt" }));
        assert_eq!(payload["exit_code"], json!(0));
        assert_eq!(payload["process"], json!("/proc/1"));
        assert_eq!(payload["action_id"], json!("a0"));

        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/status").await.unwrap()).unwrap(),
            "exited\n"
        );
        let action_output =
            String::from_utf8(shell.cat("/agent/1/actions/a0/output").await.unwrap()).unwrap();
        let action_payload: Value = serde_json::from_str(action_output.trim()).unwrap();
        assert_eq!(action_payload["content"], json!("from namespace read_file"));
        assert_eq!(
            String::from_utf8(shell.cat("/agent/1/actions/a0/process").await.unwrap()).unwrap(),
            "/proc/1"
        );
        assert_eq!(
            String::from_utf8(shell.cat("/agent/1/actions/a0/result").await.unwrap()).unwrap(),
            r#"{"exit_code":0}"#
        );
    }

    #[tokio::test]
    async fn long_tool_output_projects_to_resolvable_action_output_reference() {
        let (mut state, shell) = create_namespace_test_state_and_shell();
        let long_text = "x".repeat(crate::evidence::MAX_INLINE_EVIDENCE_BYTES + 1);

        execute_single_tool_call(
            &mut state,
            "call-long",
            "read_file",
            json!({ "content": long_text }),
        )
        .await;

        let payload = state
            .machine
            .tool_payload_by_call_id("call-long")
            .expect("projected tool payload should be on tape");
        assert_eq!(payload["type"], "evidence_projection");
        assert_eq!(payload["reference"]["path"], "/agent/1/actions/a0/output");
        assert_eq!(payload["truncation"]["full_content_recoverable"], true);
        let full = read_shell_utf8(&shell, "/agent/1/actions/a0/output").await;
        assert!(full.len() > crate::evidence::MAX_INLINE_EVIDENCE_BYTES);
        assert!(full.contains(&"x".repeat(1024)));
    }

    #[tokio::test]
    async fn short_tool_output_redacts_embedded_secrets_before_tape() {
        let (mut state, _shell) = create_namespace_test_state_and_shell();

        execute_single_tool_call(
            &mut state,
            "call-redacted-short",
            "read_file",
            json!({
                "output": "api_key=short-secret\nAuthorization: Bearer short-token"
            }),
        )
        .await;

        let payload = state
            .machine
            .tool_payload_by_call_id("call-redacted-short")
            .expect("inline tool payload should be on tape");
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains("short-secret"));
        assert!(!serialized.contains("short-token"));
        assert!(serialized.contains("[REDACTED reason=secret_key]"));
    }

    #[tokio::test]
    async fn redaction_expansion_still_projects_tool_payload_to_bounded_evidence() {
        let (mut state, _shell) = create_namespace_test_state_and_shell();

        execute_single_tool_call(
            &mut state,
            "call-redaction-expanded",
            "read_file",
            json!({
                "output": "Bearer x ".repeat(1_000)
            }),
        )
        .await;

        let payload = state
            .machine
            .tool_payload_by_call_id("call-redaction-expanded")
            .expect("expanded payload should be projected");
        assert_eq!(payload["type"], "evidence_projection");
        assert_eq!(payload["reference"]["path"], "/agent/1/actions/a0/output");
        assert!(
            payload["truncation"]["original_bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes > crate::evidence::MAX_INLINE_EVIDENCE_BYTES as u64)
        );
    }

    #[tokio::test]
    async fn long_tool_output_uses_runtime_action_id_for_evidence_reference() {
        let (mut state, _shell) = create_namespace_test_state_and_shell();

        execute_single_tool_call(
            &mut state,
            "call-forged-action-id",
            "read_file",
            json!({
                "action_id": "forged",
                "content": "x".repeat(crate::evidence::MAX_INLINE_EVIDENCE_BYTES + 1)
            }),
        )
        .await;

        let payload = state
            .machine
            .tool_payload_by_call_id("call-forged-action-id")
            .expect("projected tool payload should be on tape");
        assert_eq!(payload["metadata"]["action_id"], "a0");
        assert_eq!(payload["reference"]["path"], "/agent/1/actions/a0/output");
    }

    #[tokio::test]
    async fn long_tool_output_without_action_path_uses_marked_inline_fallback() {
        let state = create_test_state();
        let payload = json!({
            "content": "x".repeat(crate::evidence::MAX_INLINE_EVIDENCE_BYTES + 1)
        });

        let projected = tool_payload_for_tape(&state.agent_files(), &payload).await;

        assert!(projected.get("reference").is_none());
        assert_eq!(
            projected["truncation"]["fallback_reason"],
            "reference_unresolvable"
        );
        assert_eq!(projected["truncation"]["full_content_recoverable"], false);
    }

    #[tokio::test]
    async fn durable_action_evidence_marks_secret_redaction_separately_from_truncation() {
        let (mut state, shell) = create_namespace_test_state_and_shell();
        execute_single_tool_call(
            &mut state,
            "call-redacted-long",
            "read_file",
            json!({
                "authorization": "Bearer top-secret",
                "content": "x".repeat(crate::evidence::MAX_INLINE_EVIDENCE_BYTES + 1)
            }),
        )
        .await;

        let payload = state
            .machine
            .tool_payload_by_call_id("call-redacted-long")
            .unwrap();
        assert!(!payload["preview"].as_str().unwrap().contains("top-secret"));
        assert!(
            payload["preview"]
                .as_str()
                .unwrap()
                .contains("[REDACTED reason=secret_key]")
        );
        assert_eq!(payload["redactions"][0]["reason_class"], "secret_key");
        assert_eq!(payload["truncation"]["full_content_recoverable"], true);
        let full = read_shell_utf8(&shell, "/agent/1/actions/a0/output").await;
        assert!(full.contains("[REDACTED reason=secret_key]"));
        assert!(!full.contains("top-secret"));
    }

    #[tokio::test]
    async fn namespace_tool_call_fails_when_bin_tool_is_withheld() {
        let (mut state, shell) = create_namespace_test_state_and_shell_with_bin(false);

        let (outcome, events) = execute_single_tool_call(
            &mut state,
            "call-read",
            "read_file",
            json!({ "path": "sample.txt" }),
        )
        .await;

        assert!(matches!(
            outcome,
            ToolBatchOrchestratorOutcome::ContinueTurnLoop {
                refresh_context: false
            }
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::ToolCallCompleted {
                id,
                name,
                success: Some(false),
                ..
            } if id == "call-read" && name.as_deref() == Some("read_file")
        )));
        let payload = state
            .machine
            .tool_payload_by_call_id("call-read")
            .expect("failed tool response payload should be recorded on tape");
        assert_eq!(payload["success"], json!(false));
        assert!(
            payload["error"]
                .as_str()
                .is_some_and(|error| error.contains("not both mounted")),
            "payload should explain the withheld executable: {payload}"
        );
        assert!(
            shell.cat("/proc/1/status").await.is_err(),
            "a withheld executable must not spawn"
        );
    }

    #[tokio::test]
    async fn namespace_tool_call_does_not_spawn_executable_without_manifest() {
        let (mut state, shell) = create_namespace_test_state_and_shell_with_package(true, false);

        let (outcome, events) = execute_single_tool_call(
            &mut state,
            "call-read",
            "read_file",
            json!({ "path": "sample.txt" }),
        )
        .await;

        assert!(matches!(
            outcome,
            ToolBatchOrchestratorOutcome::ContinueTurnLoop {
                refresh_context: false
            }
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::ToolCallCompleted {
                id,
                success: Some(false),
                ..
            } if id == "call-read"
        )));
        let payload = state
            .machine
            .tool_payload_by_call_id("call-read")
            .expect("failed Tool response should be recorded on tape");
        assert_eq!(payload["success"], json!(false));
        assert!(
            payload["error"]
                .as_str()
                .is_some_and(|error| error.contains("valid manifest"))
        );
        assert!(
            shell.cat("/proc/1/status").await.is_err(),
            "an executable without a valid Tool manifest must not spawn"
        );
    }

    #[tokio::test]
    async fn namespace_execution_target_spawns_every_builtin_bin_tool() {
        let (state, shell) = create_namespace_test_state_and_shell();
        let tools = state.tool_execution();
        let cancel = CancellationToken::new();

        for (idx, tool_name) in BUILTIN_BIN_TOOLS.iter().enumerate() {
            let payload = execute_tool_effect(
                tools.clone(),
                tool_name,
                json!({ "tool": tool_name, "call_index": idx }),
                &cancel,
                30,
            )
            .await
            .expect("namespace tool effect should execute through /bin");
            let pid = idx + 1;
            let action_id = format!("a{idx}");

            assert_eq!(payload["success"], json!(true));
            assert_eq!(payload["tool"], json!(tool_name));
            assert_eq!(
                payload["content"],
                json!(format!("from namespace {tool_name}"))
            );
            assert_eq!(
                payload["arguments"],
                json!({ "tool": tool_name, "call_index": idx })
            );
            assert_eq!(payload["exit_code"], json!(0));
            assert_eq!(payload["process"], json!(format!("/proc/{pid}")));
            assert_eq!(payload["action_id"], json!(action_id));

            assert_eq!(
                String::from_utf8(
                    shell
                        .cat(&format!("/agent/1/actions/{action_id}/name"))
                        .await
                        .unwrap()
                )
                .unwrap(),
                *tool_name
            );
            assert_eq!(
                String::from_utf8(
                    shell
                        .cat(&format!("/agent/1/actions/{action_id}/process"))
                        .await
                        .unwrap()
                )
                .unwrap(),
                format!("/proc/{pid}")
            );
        }
    }

    #[tokio::test]
    async fn test_tool_loop_guard_new() {
        let loop_guard = ToolLoopGuard::new(Some(10), 4);
        // Verify the guard was created with the correct settings.
        // Just test that it doesn't panic
        let _ = loop_guard;
    }

    #[tokio::test]
    async fn test_orchestrate_empty_tool_batch() {
        let mut state = create_test_state();
        let mut loop_guard = ToolLoopGuard::new(None, 4);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let tool_calls: Vec<NormalizedToolCall> = vec![];
        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrate_tool_batch(
            &mut loop_guard,
            &mut state,
            &tool_calls,
            inputs,
            &mut emit,
        )
        .await;

        assert!(result.is_ok());
        match result.unwrap() {
            ToolBatchOrchestratorOutcome::ContinueTurnLoop { refresh_context } => {
                assert!(!refresh_context);
            }
            _ => panic!("Expected ContinueTurnLoop"),
        }
    }

    #[tokio::test]
    async fn test_handle_queued_steering_inputs_enforces_buffer_cap_for_follow_up() {
        let mut state = create_test_state();
        for idx in 0..MAX_BUFFERED_INBAND_USER_INPUTS {
            state
                .machine
                .push_buffered_inband_submission(alan_agent_protocol::Submission::new(Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text(format!(
                        "buffered-{idx}"
                    ))],
                    mode: InputMode::FollowUp,
                }));
        }
        let broker = TurnInputBroker::default();
        assert!(
            broker
                .push(alan_agent_protocol::Submission::new(Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text("overflow-follow-up")],
                    mode: InputMode::FollowUp,
                }))
                .await
        );

        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let handled = handle_queued_steering_inputs(
            &mut state.machine,
            &[],
            0,
            Some(&broker),
            &mut emit,
        )
        .await
        .unwrap();
        assert!(!handled);
        assert_eq!(
            state.machine.buffered_inband_user_input_count(),
            MAX_BUFFERED_INBAND_USER_INPUTS
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Error { message, recoverable }
                if *recoverable && message.contains("Too many queued in-turn user inputs")
        )));
    }

    #[tokio::test]
    async fn queued_steering_input_invalidates_earlier_active_plan() {
        let mut state = create_test_state();
        state
            .machine
            .begin_turn(state.machine.messages().len());
        state.machine.add_user_message("Initial task");
        state.machine.set_plan_snapshot_at_message_count(
            Some("Initial plan".to_string()),
            Vec::new(),
            state.machine.messages().len(),
        );
        assert!(state.machine.plan_snapshot_is_from_active_turn());

        let broker = TurnInputBroker::default();
        assert!(
            broker
                .push(alan_agent_protocol::Submission::new(Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text(
                        "Steer to the new task"
                    )],
                    mode: InputMode::Steer,
                }))
                .await
        );
        let mut emit = |_event: Event| async {};

        let handled = handle_queued_steering_inputs(
            &mut state.machine,
            &[],
            0,
            Some(&broker),
            &mut emit,
        )
        .await
        .unwrap();

        assert!(handled);
        assert!(!state.machine.plan_snapshot_is_from_active_turn());
        assert_eq!(
            state.machine.messages().last().unwrap().text_content(),
            "Steer to the new task"
        );
    }

    #[tokio::test]
    async fn test_orchestrate_tool_batch_with_virtual_update_plan() {
        let mut state = create_test_state();
        let mut loop_guard = ToolLoopGuard::new(None, 4);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let tool_calls = vec![NormalizedToolCall {
            id: "call_1".to_string(),
            name: "update_plan".to_string(),
            arguments: json!({
                "explanation": "Test plan",
                "items": [
                    {"id": "1", "content": "Step 1", "status": "in_progress"}
                ]
            }),
        }];

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrate_tool_batch(
            &mut loop_guard,
            &mut state,
            &tool_calls,
            inputs,
            &mut emit,
        )
        .await;

        assert!(result.is_ok());
        let has_update_plan_completion = events.iter().any(|event| {
            matches!(
                event,
                Event::ToolCallCompleted {
                    id,
                    result_preview: Some(preview),
                    ..
                } if id == "call_1" && preview.contains("plan_updated")
            )
        });
        assert!(
            has_update_plan_completion,
            "Expected update_plan ToolCallCompleted preview"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::PlanUpdated { explanation, items }
                if explanation.as_deref() == Some("Test plan")
                    && items.len() == 1
                    && items[0].content == "Step 1"
        )));
    }

    #[tokio::test]
    async fn test_orchestrate_tool_batch_with_virtual_confirmation() {
        let mut state = create_test_state();
        let mut loop_guard = ToolLoopGuard::new(None, 4);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let tool_calls = vec![NormalizedToolCall {
            id: "call_1".to_string(),
            name: "request_confirmation".to_string(),
            arguments: json!({
                "checkpoint_id": "chk_123",
                "checkpoint_type": "test",
                "summary": "Test confirmation",
                "details": {"key": "value"}
            }),
        }];

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrate_tool_batch(
            &mut loop_guard,
            &mut state,
            &tool_calls,
            inputs,
            &mut emit,
        )
        .await;

        assert!(result.is_ok());
        match result.unwrap() {
            ToolBatchOrchestratorOutcome::PauseTurn => {
                // Expected
            }
            _ => panic!("Expected PauseTurn"),
        }

        // Check that Yield Confirmation event was emitted
        let has_confirmation = events.iter().any(|e| {
            matches!(
                e,
                Event::Yield {
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    ..
                }
            )
        });
        assert!(has_confirmation, "Expected Yield Confirmation event");
    }

    #[tokio::test]
    async fn test_orchestrate_tool_batch_with_virtual_user_input() {
        let mut state = create_test_state();
        let mut loop_guard = ToolLoopGuard::new(None, 4);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let tool_calls = vec![NormalizedToolCall {
            id: "call_1".to_string(),
            name: "request_user_input".to_string(),
            arguments: json!({
                "title": "Test Input",
                "prompt": "Enter something",
                "questions": [
                    {"id": "q1", "label": "Question 1", "prompt": "What?", "required": true}
                ]
            }),
        }];

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrate_tool_batch(
            &mut loop_guard,
            &mut state,
            &tool_calls,
            inputs,
            &mut emit,
        )
        .await;

        assert!(result.is_ok());
        match result.unwrap() {
            ToolBatchOrchestratorOutcome::PauseTurn => {
                // Expected
            }
            _ => panic!("Expected PauseTurn"),
        }

        // Check that Yield event was emitted
        let has_input_request = events.iter().any(|e| {
            matches!(
                e,
                Event::Yield {
                    kind: alan_agent_protocol::YieldKind::StructuredInput,
                    ..
                }
            )
        });
        assert!(has_input_request, "Expected Yield StructuredInput event");
    }

    #[tokio::test]
    async fn test_orchestrate_tool_batch_with_builtin_tool() {
        let mut state = create_test_state();
        let mut loop_guard = ToolLoopGuard::new(None, 4);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        // Test with read_file tool - requires sandbox setup, will likely fail but tests the path
        let tool_calls = vec![NormalizedToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "test.txt"}),
        }];

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrate_tool_batch(
            &mut loop_guard,
            &mut state,
            &tool_calls,
            inputs,
            &mut emit,
        )
        .await;

        // Tool execution may fail due to sandbox restrictions, but orchestration should complete
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_replay_approved_tool_call() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let tool_call = NormalizedToolCall {
            id: "call_1".to_string(),
            name: "update_plan".to_string(),
            arguments: json!({
                "explanation": "Replay test",
                "items": [{"id": "1", "content": "Step", "status": "completed"}]
            }),
        };

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = replay_approved_tool_call_with_cancel(
            &mut state, &tool_call, None, None, inputs, &mut emit,
        )
        .await;

        assert!(result.is_ok());
    }
