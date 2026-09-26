    #[tokio::test]
    async fn rejected_explicit_command_terminates_without_generation_and_records_failure() {
        let executions = Arc::new(AtomicUsize::new(0));
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "bash",
            capability: ToolCapability::Unknown,
            counter: Arc::clone(&executions),
        });
        let provider = MockLlmProvider::new();
        let provider_probe = provider.clone();
        let mut state = create_test_state_with_machine_tools_and_provider(
            AgentMachine::new(),
            tools,
            provider,
        )
        .await;
        let submission_id = "d6d22d91-5791-4d88-8642-e01d40ba58e8";
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        state.machine.accept_submission(submission_id);

        handle_submission_with_cancel(
            &mut state,
            Submission::with_id_and_intent(
                submission_id,
                Op::Input {
                    parts: vec![ContentPart::text("sudo ls")],
                    mode: InputMode::FollowUp,
                },
                InputIntent::Command,
            ),
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();
        let request_id = state
            .machine
            .pending_request_ids()
            .into_iter()
            .next()
            .expect("confirmation request id");
        state.machine.finish_submission();

        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            async {}
        };
        handle_submission_with_cancel(
            &mut state,
            Submission::new(Op::Resume {
                request_id,
                content: vec![ContentPart::structured(json!({"choice": "reject"}))],
            }),
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert!(provider_probe.recorded_requests().is_empty());
        assert_eq!(
            state.machine.turn_activity(),
            crate::agent_machine::TurnActivityState::Idle
        );
        let result = state
            .machine
            .tool_payload_by_call_id(submission_id)
            .expect("rejected command has a correlated Tool response");
        assert_eq!(result["success"], false);
        assert!(result["error"].as_str().unwrap().contains("rejected"));
        assert_eq!(state.agent_files().action_ids().await.unwrap().len(), 1);
        assert!(events.iter().any(|event| matches!(
            event,
            Event::ToolCallCompleted {
                id,
                success: Some(false),
                ..
            } if id == submission_id
        )));
    }

    #[tokio::test]
    async fn next_turn_command_retains_identity_without_execution_or_generation() {
        let executions = Arc::new(AtomicUsize::new(0));
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "bash", capability: ToolCapability::Read, counter: Arc::clone(&executions),
        });
        let provider = MockLlmProvider::new();
        let provider_probe = provider.clone();
        let mut state = create_test_state_with_machine_tools_and_provider(
            AgentMachine::new(), tools, provider,
        ).await;
        let command = "printf '%s\\n' ':literal'";
        handle_submission_with_cancel(
            &mut state,
            Submission::with_id_and_intent("deferred-command", Op::Input {
                parts: vec![ContentPart::text(command)], mode: InputMode::NextTurn,
            }, InputIntent::Command),
            &mut |_event: Event| async {}, &CancellationToken::new(),
        ).await.unwrap();
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert!(provider_probe.recorded_requests().is_empty());
        assert!(state.machine.messages().is_empty());
        assert!(state.agent_files().action_ids().await.unwrap().is_empty());
        // Context projection must never consume or reinterpret a deferred command.
        assert!(state.machine.drain_next_turn_inputs().is_empty());
        let pending = state.machine.take_next_turn_commands();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "deferred-command");
        assert_eq!(pending[0].intent, InputIntent::Command);
        let Op::Input { parts, mode } = &pending[0].op else { panic!("missing input") };
        assert_eq!(*mode, InputMode::NextTurn);
        assert_eq!(alan_agent_protocol::parts_to_text(parts), command);
    }

    #[tokio::test]
    async fn idle_command_steering_records_correlated_failure() {
        let executions = Arc::new(AtomicUsize::new(0));
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "bash",
            capability: ToolCapability::Read,
            counter: Arc::clone(&executions),
        });
        let provider = MockLlmProvider::new();
        let provider_probe = provider.clone();
        let mut state = create_test_state_with_machine_tools_and_provider(
            AgentMachine::new(),
            tools,
            provider,
        )
        .await;
        let submission_id = "7717a288-a00f-470e-bf58-255a667f6b22";
        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};

        handle_submission_with_cancel(
            &mut state,
            Submission::with_id_and_intent(
                submission_id,
                Op::Input {
                    parts: vec![ContentPart::text("git status")],
                    mode: InputMode::Steer,
                },
                InputIntent::Command,
            ),
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert!(provider_probe.recorded_requests().is_empty());
        let result = state
            .machine
            .tool_payload_by_call_id(submission_id)
            .expect("rejected command has a terminal Tool result");
        assert_eq!(result["success"], false);
        assert!(result["error"]
            .as_str()
            .unwrap()
            .contains("requires an active or pending turn"));
        assert_eq!(state.agent_files().action_ids().await.unwrap().len(), 1);
        assert!(matches!(
            state.machine.messages().get(1),
            Some(crate::tape::Message::Assistant { tool_requests, .. })
                if tool_requests.len() == 1 && tool_requests[0].id == submission_id
        ));
        assert!(matches!(
            state.machine.messages().get(2),
            Some(crate::tape::Message::Tool { responses })
                if responses.len() == 1 && responses[0].id == submission_id
        ));
    }

    #[tokio::test]
    async fn queued_explicit_command_is_preserved_for_command_dispatch() {
        let mut state = create_test_state();
        let broker = TurnInputBroker::default();
        assert!(
            broker
                .push(Submission::with_id_and_intent(
                    "queued-command-id",
                    Op::Input {
                        parts: vec![ContentPart::text("git status")],
                        mode: InputMode::Steer,
                    },
                    InputIntent::Command,
                ))
                .await
        );
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            async {}
        };

        let handled = handle_queued_steering_inputs(
            &state.agent_files(),
            &mut state.machine,
            &[],
            0,
            Some(&broker),
            &mut emit,
        )
        .await
        .unwrap();

        assert!(handled, "command steering requests the next safe Tool boundary");
        assert!(state.machine.messages().is_empty());
        assert!(events.is_empty());
        let queued = state
            .machine
            .pop_buffered_inband_submission()
            .expect("command intent must reach the explicit-command handler");
        assert_eq!(queued.id, "queued-command-id");
        assert_eq!(queued.intent, InputIntent::Command);
        assert!(matches!(
            queued.op,
            Op::Input {
                mode: InputMode::Steer,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn command_steering_skips_old_batch_and_preserves_parent_correlation() {
        for decision in [None, Some("approve"), Some("reject")] {
            let needs_approval = decision.is_some();
            let executions = Arc::new(AtomicUsize::new(0));
            let commands = Arc::new(AtomicUsize::new(0));
            let mut tools = ToolRegistry::new();
            tools.register(CountingEffectTool {
                name: "read_file", capability: ToolCapability::Read,
                counter: Arc::clone(&executions),
            });
            tools.register(CountingEffectTool {
                name: "bash",
                capability: if needs_approval { ToolCapability::Unknown } else { ToolCapability::Read },
                counter: Arc::clone(&commands),
            });
            let provider = MockLlmProvider::new().with_response(GenerationResponse {
                content: "continued parent turn".into(), ..reviewer_response("approve")
            });
            let provider_probe = provider.clone();
            let mut state = create_test_state_with_machine_tools_and_provider(
                AgentMachine::new(), tools, provider,
            ).await;
            state.machine.accept_submission("parent-turn");
            state.machine.set_turn_activity(crate::agent_machine::TurnActivityState::Running);
            let broker = TurnInputBroker::default();
            broker.push(Submission::with_id_and_intent("steering-command", Op::Input {
                parts: vec![ContentPart::text(if needs_approval { "sudo ls" } else { "git status" })],
                mode: InputMode::Steer,
            }, InputIntent::Command)).await;
            let calls = ["first-tool", "skipped-tool"].map(|id| NormalizedToolCall {
                id: id.into(), name: "read_file".into(), arguments: json!({"path": "file.txt"}),
            });
            let result = orchestrate_tool_batch(
                &mut ToolLoopGuard::new(None, 4), &mut state, &calls,
                ToolOrchestratorInputs { cancel: &CancellationToken::new(), steering_broker: Some(&broker) },
                &mut |_event: Event| async {},
            ).await.unwrap();
            assert_eq!(executions.load(Ordering::SeqCst), 1);
            assert_eq!(state.machine.current_submission_id().as_deref(), Some("parent-turn"));
            assert!(provider_probe.recorded_requests().is_empty());
            assert_eq!(state.machine.tool_payload_by_call_id("skipped-tool").unwrap()["status"], "skipped_due_to_steering");
            if needs_approval {
                assert!(matches!(result, ToolBatchOrchestratorOutcome::PauseTurn));
                assert_eq!(commands.load(Ordering::SeqCst), 0);
                let request_id = state.machine.pending_request_ids().into_iter().next().unwrap();
                handle_submission_with_cancel(
                    &mut state,
                    Submission::new(Op::Resume {
                        request_id,
                        content: vec![ContentPart::structured(json!({"choice": decision.unwrap()}))],
                    }),
                    &mut |_event: Event| async {}, &CancellationToken::new(),
                ).await.unwrap();
                let approved = decision == Some("approve");
                assert_eq!(commands.load(Ordering::SeqCst), usize::from(approved));
                assert_eq!(state.machine.tool_payload_by_call_id("steering-command").unwrap()["success"], approved);
                assert_eq!(provider_probe.recorded_requests().len(), 1);
                assert_eq!(state.machine.messages().last().unwrap().submission_id(), Some("parent-turn"));
            } else {
                assert!(matches!(result, ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. }));
                assert_eq!(commands.load(Ordering::SeqCst), 1);
                assert!(state.machine.tool_payload_by_call_id("steering-command").is_some());
            }
        }
    }
