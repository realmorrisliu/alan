#[tokio::test]
async fn loop_guard_stop_reaches_file_backed_notice_and_retained_history() {
    for repeated in [true, false] {
        let (mut state, shell) = create_namespace_test_state_and_shell().await;
        let files = state.agent_files();
        crate::runtime::ui_surfaces::turn_started(&files)
            .await
            .unwrap();
        let mut guard = ToolLoopGuard::new((!repeated).then_some(1), 1);
        let cancel = CancellationToken::new();
        let calls = ["first", "refused"].map(|id| NormalizedToolCall {
            id: id.into(),
            name: "read_file".into(),
            arguments: json!({"path": "sample.txt"}),
        });
        let mut events = Vec::new();
        let writer = files.begin_tape_generation().await.unwrap();
        let outcome = orchestrate_tool_batch(
            &mut guard,
            &mut state,
            &calls[..if repeated { 2 } else { 1 }],
            ToolOrchestratorInputs {
                explicit_command: false,
                cancel: &cancel,
                steering_broker: None,
            },
            &writer,
            &mut |event| {
                events.push(event);
                async {}
            },
        )
        .await
        .unwrap();
        writer.finish().await.unwrap();
        assert!(matches!(
            outcome,
            ToolBatchOrchestratorOutcome::EndTurn { .. }
        ));
        assert_eq!(files.action_ids().await.unwrap().len(), 1);
        assert!(state.machine.tool_payload_by_call_id("first").is_some());
        assert!(state.machine.tool_payload_by_call_id("refused").is_none());
        assert_eq!(read_shell_utf8(&shell, "/proc/2/status").await, "exited\n");
        let reason = events
            .iter()
            .find_map(|event| match event {
                Event::Error { message, .. } => Some(message.as_str()),
                _ => None,
            })
            .expect("legacy error still identifies the same guard stop");
        crate::runtime::ui_surfaces::turn_completed(&files, false)
            .await
            .unwrap();
        let notice = files.read_ui_notice_snapshot().await.unwrap();
        assert_eq!(notice.kind, alan_agent_protocol::UiNoticeKind::Error);
        assert_eq!(notice.message, reason);
        assert!(!reason.contains("TOOL_REPEAT_LIMIT"));
        assert!(!reason.contains("MAX_TOOL_LOOPS"));
        assert_eq!(
            files.read_ui_activity_snapshot().await.unwrap().state,
            alan_agent_protocol::UiActivityState::Idle
        );
        crate::runtime::ui_surfaces::turn_started(&files)
            .await
            .unwrap();
        assert_eq!(
            files.read_ui_notice_snapshot().await.unwrap().kind,
            alan_agent_protocol::UiNoticeKind::None
        );
        let retained = read_shell_utf8(&shell, "/agent/1/machine/ui/events").await;
        let errors = retained
            .lines()
            .filter_map(|line| serde_json::from_str::<alan_agent_protocol::UiEvent>(line).ok())
            .filter(|event| {
                matches!(event,
                alan_agent_protocol::UiEvent::Error { message, .. } if message == reason)
            })
            .count();
        assert_eq!(errors, 1, "one durable error survives the next input");
        assert_eq!(files.action_ids().await.unwrap().len(), 1);
    }
}
