
    #[tokio::test]
    async fn test_handle_rollback_reports_actual_removed_turns_when_history_is_shorter() {
        let mut state = create_test_state();
        state.machine.add_user_message("u1");
        state.machine.add_assistant_message("a1", None);

        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Rollback { turns: 10 };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                assert!(events.iter().any(|e| {
                    matches!(
                        e,
                        Event::MachineRolledBack {
                            turns: 1,
                            removed_messages: 2,
                        }
                    )
                }));
                assert!(events.iter().any(|e| matches!(
                    e,
                    Event::TextDelta { chunk, is_final }
                        if *is_final
                            && chunk.contains("Rolled back 1 turn(s) out of requested 10 turn(s), removed 2 message(s).")
                )));
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_rollback_clears_plan_snapshot() {
        let mut state = create_test_state();
        state.machine.add_user_message("u1");
        state.machine.add_assistant_message("a1", None);
        state.machine.set_plan_snapshot(
            Some("Stale plan".to_string()),
            vec![alan_agent_protocol::PlanItem {
                id: "plan-1".to_string(),
                content: "This should be cleared on rollback".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        );

        let cancel = CancellationToken::new();

        let mut emit = |_event: Event| async {};

        let op = Op::Rollback { turns: 1 };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), RuntimeOpAction::NoTurn));
        assert!(state.machine.plan_snapshot().is_none());
    }

    #[tokio::test]
    async fn test_handle_cancel() {
        let mut state = create_test_state();
        let (environment, _shell) = namespace_environment_with_live_process_for_test().await;
        state.environment = environment;
        state.machine.activate_task();
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Interrupt;

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::NoTurn => {
                // Task should be cancelled
                assert!(!state.machine.has_active_task());
            }
            _ => panic!("Expected NoTurn"),
        }
    }

    // Tests for parse_replay_tool_call_from_confirmation_details
    #[test]
    fn test_parse_replay_tool_call_valid() {
        let details = json!({
            "replay_tool_call": {
                "call_id": "call_123",
                "tool_name": "read_file",
                "arguments": {"path": "test.txt"}
            }
        });

        let result = parse_replay_tool_call_from_confirmation_details(&details);
        assert!(result.is_some());

        let call = result.unwrap();
        assert_eq!(call.id, "call_123");
        assert_eq!(call.name, "read_file");
        assert_eq!(call.arguments, json!({"path": "test.txt"}));
    }

    #[test]
    fn test_parse_replay_tool_call_missing_replay() {
        let details = json!({
            "other_field": "value"
        });

        assert!(parse_replay_tool_call_from_confirmation_details(&details).is_none());
    }

    #[test]
    fn test_parse_replay_tool_call_empty_call_id() {
        let details = json!({
            "replay_tool_call": {
                "call_id": "  ",
                "tool_name": "read_file",
                "arguments": {}
            }
        });

        assert!(parse_replay_tool_call_from_confirmation_details(&details).is_none());
    }

    #[test]
    fn test_parse_replay_tool_call_empty_tool_name() {
        let details = json!({
            "replay_tool_call": {
                "call_id": "call_123",
                "tool_name": "",
                "arguments": {}
            }
        });

        assert!(parse_replay_tool_call_from_confirmation_details(&details).is_none());
    }

    #[test]
    fn test_parse_replay_tool_call_missing_arguments() {
        let details = json!({
            "replay_tool_call": {
                "call_id": "call_123",
                "tool_name": "read_file"
            }
        });

        assert!(parse_replay_tool_call_from_confirmation_details(&details).is_none());
    }

    // ========================================================================
    // Tests for new Phase 2 Op variants
    // ========================================================================

    #[tokio::test]
    async fn test_handle_turn_op() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Turn {
            parts: vec![ContentPart::text("Hello from Turn")],
            context: None,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                assert!(matches!(turn_kind, TurnRunKind::NewTurn));
                let text = alan_agent_protocol::parts_to_text(&user_input.unwrap());
                assert!(text.contains("Hello from Turn"));
                assert!(activate_task);
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_input_op() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Input {
            parts: vec![ContentPart::text("follow up")],
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
    async fn test_handle_follow_up_without_active_turn_starts_new_turn() {
        let mut state = create_test_state();
        state.machine.set_active_turn_request_control_intent(
            crate::RequestControlIntent::reasoning_effort(Some(
                alan_agent_protocol::ReasoningEffort::High,
            )),
        );
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Input {
            parts: vec![ContentPart::text("run after current")],
            mode: InputMode::FollowUp,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                assert!(matches!(turn_kind, TurnRunKind::NewTurn));
                assert_eq!(
                    user_input,
                    Some(vec![ContentPart::text("run after current")])
                );
                assert!(activate_task);
                assert!(
                    state
                        .machine
                        .active_turn_request_control_intent()
                        .is_empty()
                );
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_next_turn_is_queue_only_and_applies_on_next_turn() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let queue_op = Op::Input {
            parts: vec![ContentPart::text("context for next turn")],
            mode: InputMode::NextTurn,
        };
        let queue_result =
            handle_runtime_op_with_cancel(&mut state, queue_op, &mut emit, &cancel).await;
        assert!(queue_result.is_ok());
        assert!(matches!(queue_result.unwrap(), RuntimeOpAction::NoTurn));
        assert_eq!(state.machine.queued_next_turn_input_count(), 1);

        let turn_op = Op::Turn {
            parts: vec![ContentPart::text("explicit turn")],
            context: None,
        };
        let turn_result = handle_runtime_op_with_cancel(&mut state, turn_op, &mut emit, &cancel)
            .await
            .unwrap();

        match turn_result {
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                assert!(matches!(turn_kind, TurnRunKind::NewTurn));
                assert!(activate_task);
                let merged_text = alan_agent_protocol::parts_to_text(&user_input.unwrap());
                assert!(merged_text.contains("context for next turn"));
                assert!(merged_text.contains("explicit turn"));
            }
            _ => panic!("Expected RunTurn"),
        }
        assert_eq!(state.machine.queued_next_turn_input_count(), 0);
    }

    #[tokio::test]
    async fn test_handle_next_turn_overflow_emits_recoverable_error() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        for _ in 0..16 {
            let result = handle_runtime_op_with_cancel(
                &mut state,
                Op::Input {
                    parts: vec![ContentPart::text("queued")],
                    mode: InputMode::NextTurn,
                },
                &mut emit,
                &cancel,
            )
            .await
            .unwrap();
            assert!(matches!(result, RuntimeOpAction::NoTurn));
        }

        let overflow_result = handle_runtime_op_with_cancel(
            &mut state,
            Op::Input {
                parts: vec![ContentPart::text("overflow")],
                mode: InputMode::NextTurn,
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();
        assert!(matches!(overflow_result, RuntimeOpAction::NoTurn));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Error { message, recoverable }
                if *recoverable && message.contains("Too many queued next_turn inputs")
        )));
    }

    #[tokio::test]
    async fn test_handle_input_op_during_active_turn_uses_resume_turn() {
        let mut state = create_test_state();
        state
            .machine
            .set_turn_activity(crate::agent_machine::TurnActivityState::Running);
        state.machine.activate_task();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Input {
            parts: vec![ContentPart::text("steer current turn")],
            mode: InputMode::Steer,
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn {
                turn_kind,
                user_input,
                activate_task,
            } => {
                assert!(matches!(turn_kind, TurnRunKind::ResumeTurn));
                assert_eq!(
                    user_input,
                    Some(vec![ContentPart::text("steer current turn")])
                );
                assert!(!activate_task);
            }
            _ => panic!("Expected RunTurn"),
        }
    }

    #[tokio::test]
    async fn test_handle_interrupt_op() {
        let mut state = create_test_state();
        let (environment, _shell) = namespace_environment_with_live_process_for_test().await;
        state.environment = environment;
        state.machine.activate_task();
        state
            .machine
            .set_turn_activity(crate::agent_machine::TurnActivityState::Running);
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Interrupt;

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(!state.machine.has_active_task());
    }

    #[tokio::test]
    async fn test_handle_interrupt_op_keeps_agent_process_running() {
        let (environment, shell) = namespace_environment_with_live_process_for_test().await;
        let mut state = create_test_state();
        state.environment = environment;
        state.machine.activate_task();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result =
            handle_runtime_op_with_cancel(&mut state, Op::Interrupt, &mut emit, &cancel).await;

        assert!(result.is_ok());
        assert!(!state.machine.has_active_task());
        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/status").await.unwrap()).unwrap(),
            "running\n"
        );
        let agent_events = String::from_utf8(shell.cat("/agent/1/events").await.unwrap()).unwrap();
        assert!(
            !agent_events.contains("ctl:"),
            "generic interrupt must not be routed through machine/ctl: {agent_events:?}"
        );
    }

    #[tokio::test]
    async fn test_handle_resume_no_pending_yields_error() {
        let mut state = create_test_state();
        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "nonexistent".to_string(),
            content: vec![ContentPart::structured(
                serde_json::json!({"choice": "approve"}),
            )],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), RuntimeOpAction::NoTurn));

        // Should have emitted an error event
        let has_error = events.iter().any(
            |e| matches!(e, Event::Error { message, .. } if message.contains("does not match")),
        );
        assert!(has_error);
    }

    #[tokio::test]
    async fn test_handle_resume_with_pending_confirmation() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.machine.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp-1".to_string(),
            checkpoint_type: "review".to_string(),
            summary: "Review this".to_string(),
            details: json!({}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let op = Op::Resume {
            request_id: "cp-1".to_string(),
            content: vec![ContentPart::structured(json!({"choice": "approve"}))],
        };

        let result = handle_runtime_op_with_cancel(&mut state, op, &mut emit, &cancel).await;
        assert!(result.is_ok());

        match result.unwrap() {
            RuntimeOpAction::RunTurn { turn_kind, .. } => {
                assert!(matches!(turn_kind, TurnRunKind::ResumeTurn));
            }
            _ => panic!("Expected RunTurn with ResumeTurn"),
        }
    }

    #[tokio::test]
    async fn test_tool_escalation_resume_records_structured_trace_message() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.machine.set_confirmation(PendingConfirmation {
            checkpoint_id: "tool_escalation_tool_123".to_string(),
            checkpoint_type: "tool_escalation".to_string(),
            summary: "Approve?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "tool_escalation_tool_123".to_string(),
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

        let messages = state.machine.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_user());
        match messages[0].parts().first() {
            Some(ContentPart::Structured { data }) => {
                assert_eq!(
                    data.get("__alan_internal_control")
                        .and_then(|marker| marker.get("kind"))
                        .and_then(serde_json::Value::as_str),
                    Some("tool_escalation_confirmation")
                );
            }
            _ => panic!("expected structured control message"),
        }
    }

    #[tokio::test]
    async fn test_effect_replay_resume_records_structured_trace_message() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.machine.set_confirmation(PendingConfirmation {
            checkpoint_id: "effect_replay_call-123".to_string(),
            checkpoint_type: "effect_replay_confirmation".to_string(),
            summary: "Replay side effect?".to_string(),
            details: json!({"effect_status":"unknown"}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "effect_replay_call-123".to_string(),
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

        let messages = state.machine.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_user());
        match messages[0].parts().first() {
            Some(ContentPart::Structured { data }) => {
                assert_eq!(
                    data.get("__alan_internal_control")
                        .and_then(|marker| marker.get("kind"))
                        .and_then(serde_json::Value::as_str),
                    Some("effect_replay_confirmation")
                );
            }
            _ => panic!("expected structured control message"),
        }
    }

    #[tokio::test]
    async fn test_non_tool_escalation_resume_still_records_tool_message() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.machine.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp-1".to_string(),
            checkpoint_type: "review".to_string(),
            summary: "Review?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let op = Op::Resume {
            request_id: "cp-1".to_string(),
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

        let messages = state.machine.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is_tool());
        assert_eq!(messages[0].tool_responses()[0].id, "cp-1");
    }

    #[tokio::test]
    async fn test_tool_escalation_replay_batch_does_not_bypass_unknown_without_unknown_marker() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.machine.set_confirmation(PendingConfirmation {
            checkpoint_id: "tool_escalation_call-1".to_string(),
            checkpoint_type: "tool_escalation".to_string(),
            summary: "Approve policy escalation".to_string(),
            details: json!({
                "reason": "policy requires approval",
                "replay_tool_call": {
                    "call_id": "call-1",
                    "tool_name": "write_file",
                    "arguments": {"path":"notes.txt","payload":"hello"}
                }
            }),
            options: vec!["approve".to_string(), "reject".to_string()],
        });
        state.machine.set_tool_replay_batch(
            "tool_escalation_call-1",
            vec![NormalizedToolCall {
                id: "call-1".to_string(),
                name: "write_file".to_string(),
                arguments: json!({"path":"notes.txt","payload":"hello"}),
            }],
        );

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "tool_escalation_call-1".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        match result {
            RuntimeOpAction::ReplayApprovedToolBatch {
                approved_unknown_effect_call_id,
                approved_tool_escalation_call_id,
                ..
            } => {
                assert_eq!(approved_unknown_effect_call_id, None);
                assert_eq!(approved_tool_escalation_call_id.as_deref(), Some("call-1"));
            }
            _ => panic!("expected replay batch action"),
        }
    }

    #[tokio::test]
    async fn test_effect_replay_confirmation_marks_unknown_bypass_for_unknown_effect() {
        use crate::approval::PendingConfirmation;

        let mut state = create_test_state();
        state.machine.set_confirmation(PendingConfirmation {
            checkpoint_id: "effect_replay_call-1".to_string(),
            checkpoint_type: "effect_replay_confirmation".to_string(),
            summary: "Approve unknown-effect replay".to_string(),
            details: json!({
                "effect_status": "unknown",
                "replay_tool_call": {
                    "call_id": "call-1",
                    "tool_name": "write_file",
                    "arguments": {"path":"notes.txt","payload":"hello"}
                }
            }),
            options: vec!["approve".to_string(), "reject".to_string()],
        });
        state.machine.set_tool_replay_batch(
            "effect_replay_call-1",
            vec![NormalizedToolCall {
                id: "call-1".to_string(),
                name: "write_file".to_string(),
                arguments: json!({"path":"notes.txt","payload":"hello"}),
            }],
        );

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = handle_runtime_op_with_cancel(
            &mut state,
            Op::Resume {
                request_id: "effect_replay_call-1".to_string(),
                content: vec![ContentPart::structured(json!({"choice": "approve"}))],
            },
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        match result {
            RuntimeOpAction::ReplayApprovedToolBatch {
                approved_unknown_effect_call_id,
                approved_tool_escalation_call_id,
                ..
            } => {
                assert_eq!(approved_unknown_effect_call_id.as_deref(), Some("call-1"));
                assert_eq!(approved_tool_escalation_call_id, None);
            }
            _ => panic!("expected replay batch action"),
        }
    }
