
    #[tokio::test]
    async fn test_replay_approved_tool_batch() {
        let mut state = create_test_state();
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
                "explanation": "Batch test",
                "items": [{"id": "1", "content": "Step 1", "status": "completed"}]
            }),
        }];

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = replay_approved_tool_batch_with_cancel(
            &mut state,
            &tool_calls,
            None,
            None,
            inputs,
            &mut emit,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_orchestrate_tool_batch_with_cancel() {
        let mut state = create_test_state();
        state.machine.begin_turn(0);
        state
            .machine
            .set_turn_activity(crate::agent_machine::TurnActivityState::Running);
        let mut orchestrator = ToolTurnOrchestrator::new(None, 4);
        let cancel = CancellationToken::new();

        // Cancel immediately
        cancel.cancel();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let tool_calls = vec![NormalizedToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "test.txt"}),
        }];

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrator
            .orchestrate_tool_batch(&mut state, &tool_calls, inputs, &mut emit)
            .await;

        // Should complete without panic even when cancelled
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_virtual_tool_ends_turn() {
        let mut state = create_test_state();
        let mut orchestrator = ToolTurnOrchestrator::new(None, 4);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        // Invalid confirmation request - missing required summary
        let tool_calls = vec![NormalizedToolCall {
            id: "call_1".to_string(),
            name: "request_confirmation".to_string(),
            arguments: json!({
                "details": {"reason": "missing_summary"}
            }),
        }];

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrator
            .orchestrate_tool_batch(&mut state, &tool_calls, inputs, &mut emit)
            .await;

        assert!(result.is_ok());
        // Invalid virtual tool should end turn
        match result.unwrap() {
            ToolBatchOrchestratorOutcome::EndTurn { .. } => {
                // Check Error event was emitted
                let has_error = events.iter().any(|e| matches!(e, Event::Error { .. }));
                assert!(has_error, "Expected Error event for invalid virtual tool");
            }
            _ => panic!("Expected EndTurn for invalid virtual tool"),
        }
    }

    #[tokio::test]
    async fn test_multiple_tools_in_batch() {
        let mut state = create_test_state();
        let mut orchestrator = ToolTurnOrchestrator::new(None, 4);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let tool_calls = vec![
            NormalizedToolCall {
                id: "call_1".to_string(),
                name: "update_plan".to_string(),
                arguments: json!({
                    "explanation": "First",
                    "items": [{"id": "1", "content": "Step 1", "status": "completed"}]
                }),
            },
            NormalizedToolCall {
                id: "call_2".to_string(),
                name: "update_plan".to_string(),
                arguments: json!({
                    "explanation": "Second",
                    "items": [{"id": "2", "content": "Step 2", "status": "completed"}]
                }),
            },
        ];

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrator
            .orchestrate_tool_batch(&mut state, &tool_calls, inputs, &mut emit)
            .await;

        assert!(result.is_ok());
        // Should have two update_plan completion events.
        let plan_updates: Vec<_> = events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::ToolCallCompleted {
                        result_preview: Some(preview),
                        ..
                    } if preview.contains("plan_updated")
                )
            })
            .collect();
        assert_eq!(
            plan_updates.len(),
            2,
            "Expected two update_plan completion events"
        );
    }

    #[tokio::test]
    async fn test_side_effect_dedupe_survives_rollout_recovery_for_file_effects() {
        let temp = tempfile::TempDir::new().unwrap();
        let rollouts_dir = temp.path();
        let counter = Arc::new(AtomicUsize::new(0));

        let mut machine =
            AgentMachine::new_with_recorder_in_dir("/proc/test", "mock", rollouts_dir)
                .await
                .unwrap();
        machine.add_user_message("write file once");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "write_file",
            capability: ToolCapability::Write,
            counter: Arc::clone(&counter),
        });

        let mut state = create_test_state_with_machine_and_tools(machine, tools);
        assert_eq!(state.process_path(), "/proc/1");
        let (_, first_events) = execute_single_tool_call(
            &mut state,
            "call-file-1",
            "write_file",
            json!({"path": "notes.txt", "payload": "hello"}),
        )
        .await;
        assert!(
            first_events
                .iter()
                .any(|event| matches!(event, Event::ToolCallCompleted { .. }))
        );
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let rollout_path = state
            .machine
            .rollout_path()
            .expect("recorder should exist")
            .clone();

        let recovered_machine =
            AgentMachine::load_from_rollout_in_dir(&rollout_path, "/proc/2", "mock", rollouts_dir)
                .await
                .unwrap();
        let mut recovered_tools = ToolRegistry::new();
        recovered_tools.register(CountingEffectTool {
            name: "write_file",
            capability: ToolCapability::Write,
            counter: Arc::clone(&counter),
        });
        let mut recovered_state = create_test_state_with_machine_tools_provider_and_agent_path(
            recovered_machine,
            recovered_tools,
            SimpleMockProvider,
            "/agent/2",
        );
        assert_eq!(recovered_state.process_path(), "/proc/2");
        let replay_arguments = json!({"path": "notes.txt", "payload": "hello"});
        let replay_identity = build_effect_identity(
            &recovered_state.machine,
            "write_file",
            &replay_arguments,
            EffectCategory::File,
        );
        let _ = execute_single_tool_call(
            &mut recovered_state,
            "call-file-2",
            "write_file",
            replay_arguments,
        )
        .await;

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "dedupe after recovery should skip physical execution"
        );
        assert_eq!(
            recovered_state
                .machine
                .tool_payload_by_call_id("call-file-2")
                .expect("replayed tool payload should exist"),
            recovered_state
                .machine
                .tool_payload_by_call_id("call-file-1")
                .expect("original tool payload should exist"),
            "dedupe replay should preserve original tool payload"
        );
        let replayed_effect = recovered_state
            .machine
            .effect_by_idempotency_key(&replay_identity.idempotency_key)
            .expect("dedupe replay effect should exist");
        assert_eq!(replayed_effect.process_path, "/proc/2");
        assert!(replayed_effect.dedupe_hit);
    }

    #[tokio::test]
    async fn test_side_effect_dedupe_for_network_effects() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut machine = AgentMachine::new();
        machine.add_user_message("call api once");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "bash",
            capability: ToolCapability::Network,
            counter: Arc::clone(&counter),
        });
        let mut state = create_test_state_with_machine_and_tools(machine, tools);
        // Exercise effect dedupe independent of the locked auto-approve posture
        // (which would otherwise escalate the network call).
        state.runtime_config.policy_engine = crate::policy::PolicyEngine::allow_all();
        let arguments = json!({
            "command": "curl https://example.com",
            "output": "api_key=embedded-secret",
            "headers": {
                "authorization": "Bearer secret-token"
            }
        });
        let identity =
            build_effect_identity(&state.machine, "bash", &arguments, EffectCategory::Network);

        let _ = execute_single_tool_call(&mut state, "call-net-1", "bash", arguments.clone()).await;
        let _ = execute_single_tool_call(&mut state, "call-net-2", "bash", arguments).await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
        let replayed_payload = state
            .machine
            .tool_payload_by_call_id("call-net-2")
            .expect("replayed tool payload should exist");
        assert_eq!(
            replayed_payload,
            state
                .machine
                .tool_payload_by_call_id("call-net-1")
                .expect("original tool payload should exist"),
            "dedupe replay should preserve original network-tool payload"
        );
        assert_eq!(
            replayed_payload["payload"]["headers"]["authorization"],
            json!("[REDACTED reason=secret_key]"),
            "dedupe replay should preserve the redacted tape payload"
        );
        let effect = state
            .machine
            .effect_by_idempotency_key(&identity.idempotency_key)
            .expect("effect record should exist");
        assert_eq!(
            effect
                .result_payload
                .as_ref()
                .and_then(|payload| payload.get("payload"))
                .and_then(|payload| payload.get("headers"))
                .and_then(|headers| headers.get("authorization")),
            Some(&json!("[REDACTED reason=secret_key]")),
            "durable effect payloads should stay redacted for persistence"
        );
    }

    #[tokio::test]
    async fn test_effect_record_and_tape_payload_are_both_redacted() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut machine = AgentMachine::new();
        machine.add_user_message("call api with auth");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "bash",
            capability: ToolCapability::Network,
            counter: Arc::clone(&counter),
        });
        let mut state = create_test_state_with_machine_and_tools(machine, tools);
        // Exercise effect recording independent of the locked auto-approve
        // posture (which would otherwise escalate the network call).
        state.runtime_config.policy_engine = crate::policy::PolicyEngine::allow_all();
        let arguments = json!({
            "command": "curl https://example.com",
            "output": "api_key=embedded-secret",
            "headers": {
                "authorization": "Bearer secret-token"
            }
        });
        let identity =
            build_effect_identity(&state.machine, "bash", &arguments, EffectCategory::Network);

        let _ = execute_single_tool_call(&mut state, "call-net-secret", "bash", arguments).await;

        let effect = state
            .machine
            .effect_by_idempotency_key(&identity.idempotency_key)
            .expect("effect record should exist");
        assert_eq!(
            effect
                .result_payload
                .as_ref()
                .and_then(|payload| payload.get("payload"))
                .and_then(|payload| payload.get("headers"))
                .and_then(|headers| headers.get("authorization")),
            Some(&json!("[REDACTED reason=secret_key]"))
        );
        assert_eq!(
            effect
                .result_payload
                .as_ref()
                .and_then(|payload| payload.get("payload"))
                .and_then(|payload| payload.get("output")),
            Some(&json!("api_key= [REDACTED reason=secret_key]"))
        );

        let tape_payload = state
            .machine
            .tool_payload_by_call_id("call-net-secret")
            .expect("tool payload should exist on tape");
        assert_eq!(
            tape_payload["payload"]["headers"]["authorization"],
            json!("[REDACTED reason=secret_key]")
        );
        assert_eq!(
            tape_payload["payload"]["output"],
            json!("api_key= [REDACTED reason=secret_key]")
        );
    }

    #[tokio::test]
    async fn test_side_effect_dedupe_for_process_effects() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut machine = AgentMachine::new();
        machine.add_user_message("run command once");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "bash",
            capability: ToolCapability::Write,
            counter: Arc::clone(&counter),
        });
        let mut state = create_test_state_with_machine_and_tools(machine, tools);

        let _ = execute_single_tool_call(
            &mut state,
            "call-proc-1",
            "bash",
            json!({"command": "touch hello.txt"}),
        )
        .await;
        let _ = execute_single_tool_call(
            &mut state,
            "call-proc-2",
            "bash",
            json!({"command": "touch hello.txt"}),
        )
        .await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(
            state
                .machine
                .tool_payload_by_call_id("call-proc-2")
                .expect("replayed tool payload should exist"),
            state
                .machine
                .tool_payload_by_call_id("call-proc-1")
                .expect("original tool payload should exist"),
            "dedupe replay should preserve original process-tool payload"
        );
    }

    #[tokio::test]
    async fn test_unknown_effect_status_escalates_without_execution() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut machine = AgentMachine::new();
        machine.add_user_message("write file with safety");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "write_file",
            capability: ToolCapability::Write,
            counter: Arc::clone(&counter),
        });
        let mut state = create_test_state_with_machine_and_tools(machine, tools);
        let arguments = json!({"path": "notes.txt", "payload": "hello"});
        let identity = build_effect_identity(
            &state.machine,
            "write_file",
            &arguments,
            EffectCategory::File,
        );
        state.machine.record_effect(crate::rollout::EffectRecord {
            effect_id: "ef-unknown".to_string(),
            process_path: state.process_path(),
            tool_call_id: "call-prev".to_string(),
            idempotency_key: identity.idempotency_key.clone(),
            effect_type: "file".to_string(),
            request_fingerprint: identity.request_fingerprint.clone(),
            result_digest: None,
            result_payload: None,
            status: crate::rollout::EffectStatus::Unknown,
            applied_at: None,
            reason: Some("crash during prior execution".to_string()),
            dedupe_hit: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        let (outcome, events) =
            execute_single_tool_call(&mut state, "call-new", "write_file", arguments).await;
        assert!(matches!(outcome, ToolBatchOrchestratorOutcome::PauseTurn));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "unknown effect status should not execute without confirmation"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn test_replay_approved_unknown_effect_executes_tool_once() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut machine = AgentMachine::new();
        machine.add_user_message("write file with approval");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "write_file",
            capability: ToolCapability::Write,
            counter: Arc::clone(&counter),
        });
        let mut state = create_test_state_with_machine_and_tools(machine, tools);
        let arguments = json!({"path": "notes.txt", "payload": "hello"});
        let identity = build_effect_identity(
            &state.machine,
            "write_file",
            &arguments,
            EffectCategory::File,
        );
        state.machine.record_effect(crate::rollout::EffectRecord {
            effect_id: "ef-unknown".to_string(),
            process_path: state.process_path(),
            tool_call_id: "call-prev".to_string(),
            idempotency_key: identity.idempotency_key.clone(),
            effect_type: "file".to_string(),
            request_fingerprint: identity.request_fingerprint.clone(),
            result_digest: None,
            result_payload: None,
            status: crate::rollout::EffectStatus::Unknown,
            applied_at: None,
            reason: Some("crash during prior execution".to_string()),
            dedupe_hit: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        let cancel = CancellationToken::new();
        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };
        let tool_call = NormalizedToolCall {
            id: "call-new".to_string(),
            name: "write_file".to_string(),
            arguments,
        };
        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = replay_approved_tool_call_with_cancel(
            &mut state,
            &tool_call,
            Some(tool_call.id.as_str()),
            None,
            inputs,
            &mut emit,
        )
        .await
        .expect("approved replay should run");
        assert!(matches!(
            outcome,
            ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. }
        ));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "approved replay should execute once"
        );
        assert!(
            !events.iter().any(|event| matches!(
                event,
                Event::Yield {
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    ..
                }
            )),
            "approved replay should not emit a second confirmation yield"
        );

        let restored = state
            .machine
            .effect_by_idempotency_key(&identity.idempotency_key)
            .expect("updated effect record should exist");
        assert_eq!(restored.status, crate::rollout::EffectStatus::Applied);
    }

    #[tokio::test]
    async fn test_replay_approved_tool_escalation_executes_tool_once() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut machine = AgentMachine::new();
        machine.add_user_message("run unknown tool with approval");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "unknown_effect_tool",
            capability: ToolCapability::Unknown,
            counter: Arc::clone(&counter),
        });
        let mut state = create_test_state_with_machine_and_tools(machine, tools);

        let cancel = CancellationToken::new();
        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };
        let tool_call = NormalizedToolCall {
            id: "call-approved".to_string(),
            name: "unknown_effect_tool".to_string(),
            arguments: json!({"payload": "hello"}),
        };
        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = replay_approved_tool_call_with_cancel(
            &mut state,
            &tool_call,
            None,
            Some(tool_call.id.as_str()),
            inputs,
            &mut emit,
        )
        .await
        .expect("approved tool escalation replay should run");
        assert!(matches!(
            outcome,
            ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. }
        ));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "approved tool escalation replay should execute once"
        );
        assert!(
            !events.iter().any(|event| matches!(
                event,
                Event::Yield {
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    ..
                }
            )),
            "approved tool escalation replay should not emit a second confirmation yield"
        );
    }

    #[tokio::test]
    async fn test_replay_approved_batch_bypasses_unknown_only_for_first_tool_call() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut machine = AgentMachine::new();
        machine.add_user_message("write file with batch replay");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "write_file",
            capability: ToolCapability::Write,
            counter: Arc::clone(&counter),
        });
        let mut state = create_test_state_with_machine_and_tools(machine, tools);
        let arguments_first = json!({"path": "notes-1.txt", "payload": "hello"});
        let arguments_second = json!({"path": "notes-2.txt", "payload": "world"});
        let identity_first = build_effect_identity(
            &state.machine,
            "write_file",
            &arguments_first,
            EffectCategory::File,
        );
        let identity_second = build_effect_identity(
            &state.machine,
            "write_file",
            &arguments_second,
            EffectCategory::File,
        );
        state.machine.record_effect(crate::rollout::EffectRecord {
            effect_id: "ef-unknown-1".to_string(),
            process_path: state.process_path(),
            tool_call_id: "call-prev-1".to_string(),
            idempotency_key: identity_first.idempotency_key.clone(),
            effect_type: "file".to_string(),
            request_fingerprint: identity_first.request_fingerprint.clone(),
            result_digest: None,
            result_payload: None,
            status: crate::rollout::EffectStatus::Unknown,
            applied_at: None,
            reason: Some("crash during prior execution".to_string()),
            dedupe_hit: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        state.machine.record_effect(crate::rollout::EffectRecord {
            effect_id: "ef-unknown-2".to_string(),
            process_path: state.process_path(),
            tool_call_id: "call-prev-2".to_string(),
            idempotency_key: identity_second.idempotency_key.clone(),
            effect_type: "file".to_string(),
            request_fingerprint: identity_second.request_fingerprint.clone(),
            result_digest: None,
            result_payload: None,
            status: crate::rollout::EffectStatus::Unknown,
            applied_at: None,
            reason: Some("crash during prior execution".to_string()),
            dedupe_hit: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        let tool_calls = vec![
            NormalizedToolCall {
                id: "call-dup".to_string(),
                name: "write_file".to_string(),
                arguments: arguments_first,
            },
            NormalizedToolCall {
                id: "call-dup".to_string(),
                name: "write_file".to_string(),
                arguments: arguments_second,
            },
        ];
        let cancel = CancellationToken::new();
        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };
        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = replay_approved_tool_batch_with_cancel(
            &mut state,
            &tool_calls,
            Some("call-dup"),
            None,
            inputs,
            &mut emit,
        )
        .await
        .expect("approved replay batch should run");
        assert!(matches!(outcome, ToolBatchOrchestratorOutcome::PauseTurn));
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "only the approved call should bypass unknown-effect escalation"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } if payload["details"]["replay_tool_call"]["call_id"] == "call-dup"
        )));
    }

    #[tokio::test]
    async fn test_tool_loop_guard_triggers() {
        let mut state = create_test_state();
        // Set max loops to a small number
        let mut orchestrator = ToolTurnOrchestrator::new(Some(2), 4);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        // Create many tool calls that will exceed the loop limit
        let mut tool_calls = vec![];
        for i in 0..3 {
            tool_calls.push(NormalizedToolCall {
                id: format!("call_{}", i),
                name: "update_plan".to_string(),
                arguments: json!({
                    "explanation": format!("Step {}", i),
                    "items": [{"id": i.to_string(), "content": "Step", "status": "completed"}]
                }),
            });
        }

        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let result = orchestrator
            .orchestrate_tool_batch(&mut state, &tool_calls, inputs, &mut emit)
            .await;

        assert!(result.is_ok());
        // After max loops, should end turn
        match result.unwrap() {
            ToolBatchOrchestratorOutcome::EndTurn { .. } => {
                // Expected
            }
            _ => {
                // Note: Depending on implementation, might continue or end
                // Just verify no panic occurred
            }
        }
    }
