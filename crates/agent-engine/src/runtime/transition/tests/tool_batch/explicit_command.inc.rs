#[tokio::test]
async fn explicit_command_execution_and_approval_do_not_generate_agent_turns() {
    use alan_agent_protocol::{ContentPart, InputIntent};
    let large_script = format!("printf '%s' '{}'", "x".repeat(crate::evidence::MAX_INLINE_EVIDENCE_BYTES + 1));
    for (choice, script) in [
        (None, "pwd"),
        (None, large_script.as_str()),
        (Some("approve"), "sudo ls"),
        (Some("reject"), "sudo ls"),
        (Some("approve"), "rm -rf build"),
        (Some("reject"), "git reset --hard"),
    ] {
        let executions = Arc::new(AtomicUsize::new(0));
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            name: "bash",
            capability: if choice.is_some() {
                ToolCapability::Unknown
            } else {
                ToolCapability::Read
            },
            counter: Arc::clone(&executions),
        });
        let provider = alan_llm::MockLlmProvider::new();
        let probe = provider.clone();
        let mut state =
            create_test_state_with_machine_tools_and_provider(AgentMachine::new(), tools, provider)
                .await;
        state.machine.set_active_skills(vec![crate::skills::ActiveSkillEnvelope::available(
            crate::skills::SkillMetadata {
                id: "deploy".to_string(),
                package_id: Some("skill:deploy".to_string()),
                name: "Deploy".to_string(),
                description: "Deploy service".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/tmp/deploy/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: crate::skills::SkillScope::Descriptor,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: crate::skills::SkillContentSource::File(std::path::PathBuf::from(
                    "/tmp/deploy/SKILL.md",
                )),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            crate::skills::SkillActivationReason::ExplicitMention {
                mention: "deploy".to_string(),
            },
        )]);
        assert!(!state.machine.record_guardian_review(true));
        assert!(!state.machine.record_guardian_review(true));
        let id = uuid::Uuid::new_v4().to_string();
        state.machine.accept_submission(id.clone());
        let files = state.agent_files();
        let mut emit = |event: Event| {
            let files = files.clone();
            async move {
                if matches!(event, Event::ToolCallStarted { .. }) {
                    assert_eq!(
                        files.read_ui_activity_snapshot().await.unwrap().state,
                        alan_agent_protocol::UiActivityState::Running,
                        "approved commands must publish running before Tool execution"
                    );
                }
            }
        };
        let cancel = CancellationToken::new();
        handle_submission_with_cancel(
            &mut state,
            Submission {
                id: id.clone(),
                intent: InputIntent::Command,
                op: Op::Input {
                    parts: vec![ContentPart::text(script)],
                    mode: InputMode::FollowUp,
                },
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();
        assert!(state.machine.active_skills().is_empty());
        assert!(!state.machine.record_guardian_review(true), "prior-turn denials must be cleared");
        if let Some(choice) = choice {
            assert_eq!(executions.load(Ordering::SeqCst), 0);
            let request_id = state
                .machine
                .pending_request_ids()
                .into_iter()
                .next()
                .expect("approval required");
            let second_id = uuid::Uuid::new_v4().to_string();
            let error = handle_submission_with_cancel(
                &mut state,
                Submission {
                    id: second_id,
                    intent: InputIntent::Command,
                    op: Op::Input {
                        parts: vec![ContentPart::text("pwd")],
                        mode: InputMode::FollowUp,
                    },
                },
                &mut emit,
                &cancel,
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains("idle Machine"));
            assert_eq!(
                state.machine.pending_request_ids(),
                vec![request_id.clone()]
            );
            assert_eq!(executions.load(Ordering::SeqCst), 0);
            state.machine.finish_submission();
            handle_submission_with_cancel(
                &mut state,
                Submission::new(Op::Resume {
                    request_id,
                    content: vec![ContentPart::structured(json!({"choice":choice}))],
                }),
                &mut emit,
                &cancel,
            )
            .await
            .unwrap();
        }
        assert_eq!(
            executions.load(Ordering::SeqCst),
            usize::from(choice != Some("reject"))
        );
        assert!(
            probe.recorded_requests().is_empty(),
            "explicit command must not generate: {choice:?}"
        );
        assert!(!state.machine.has_pending_interaction());
        assert_eq!(
            state.machine.turn_activity(),
            crate::agent_machine::TurnActivityState::Idle
        );
        let payload = state
            .machine
            .tool_payload_by_call_id(&id)
            .expect("correlated result");
        let metadata = if script == large_script {
            assert_eq!(payload["type"], "evidence_projection");
            &payload["metadata"]
        } else {
            &payload
        };
        assert_eq!(metadata["success"], choice != Some("reject"));
        let shell = alan_shell::Shell::new(state.environment.root_transport());
        let mut matching = Vec::new();
        for action in state.agent_files().action_ids().await.unwrap() {
            let base = format!("{}/actions/{action}", state.environment.agent_path());
            let result: Value = serde_json::from_slice(&shell.cat(&format!("{base}/result")).await.unwrap()).unwrap();
            if result["call_id"] == id { matching.push(base); }
        }
        assert_eq!(matching.len(), 1, "one terminal Action per command");
        let base = &matching[0];
        if choice == Some("reject") {
            assert!(
                shell
                    .cat(&format!("{base}/process"))
                    .await
                    .unwrap()
                    .is_empty()
            );
        } else {
            assert_eq!(
                shell.cat(&format!("{base}/approval")).await.unwrap(),
                if choice == Some("approve") {
                    b"approved".as_slice()
                } else {
                    b"not_required".as_slice()
                }
            );
        }
    }
}

#[tokio::test]
async fn cancelled_explicit_command_has_failed_action_without_execution_or_generation() {
    let executions = Arc::new(AtomicUsize::new(0));
    let mut tools = ToolRegistry::new();
    tools.register(CountingEffectTool {
        name: "bash",
        capability: ToolCapability::Read,
        counter: Arc::clone(&executions),
    });
    let provider = alan_llm::MockLlmProvider::new();
    let probe = provider.clone();
    let mut state =
        create_test_state_with_machine_tools_and_provider(AgentMachine::new(), tools, provider)
            .await;
    let id = uuid::Uuid::new_v4().to_string();
    state.machine.accept_submission(id.clone());
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut emit = |_event: Event| async {};
    handle_submission_with_cancel(
        &mut state,
        Submission {
            id: id.clone(),
            intent: alan_agent_protocol::InputIntent::Command,
            op: Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("pwd")],
                mode: InputMode::FollowUp,
            },
        },
        &mut emit,
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert!(probe.recorded_requests().is_empty());
    assert_eq!(
        state.machine.tool_payload_by_call_id(&id).unwrap()["success"],
        false
    );
    assert_eq!(state.agent_files().action_ids().await.unwrap().len(), 1);
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    assert_eq!(shell.cat(&format!("{}/actions/a0/approval", state.environment.agent_path())).await.unwrap(), b"not_required");
}

#[tokio::test]
async fn interrupt_while_command_awaits_approval_publishes_correlated_failure() {
    let executions = Arc::new(AtomicUsize::new(0));
    let mut tools = ToolRegistry::new();
    tools.register(CountingEffectTool {
        name: "bash",
        capability: ToolCapability::Unknown,
        counter: Arc::clone(&executions),
    });
    let provider = alan_llm::MockLlmProvider::new();
    let probe = provider.clone();
    let mut state =
        create_test_state_with_machine_tools_and_provider(AgentMachine::new(), tools, provider)
            .await;
    let id = uuid::Uuid::new_v4().to_string();
    let submission = Submission {
        id: id.clone(),
        intent: alan_agent_protocol::InputIntent::Command,
        op: Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("sudo ls")],
            mode: InputMode::FollowUp,
        },
    };
    let files = state.agent_files();
    let cancel = CancellationToken::new();
    let broker = TurnInputBroker::default();
    let interrupt = async {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let ids = files.request_ids().await.unwrap();
                if files.pending_request_id(&ids).await.unwrap().is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
    };
    let (outcome, ()) = tokio::join!(
        advance_accepted_submission(&mut state, submission, &broker, &cancel),
        interrupt
    );
    outcome.result.unwrap();
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert!(probe.recorded_requests().is_empty());
    assert!(!state.machine.has_pending_interaction());
    assert_eq!(files.action_ids().await.unwrap().len(), 1);
    assert_eq!(
        state.machine.tool_payload_by_call_id(&id).unwrap()["success"],
        false
    );
}
