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
    async fn unsupported_explicit_command_mode_records_correlated_failure() {
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
            .contains("follow_up scheduling"));
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
