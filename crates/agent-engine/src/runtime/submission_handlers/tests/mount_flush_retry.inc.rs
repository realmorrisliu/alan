#[tokio::test]
async fn failed_real_flush_retry_preserves_single_mount_tool_pair() {
    let (mut state, host_mount, request_id) = pending_host_mount_state().await;
    let dir = tempfile::tempdir().unwrap();
    let recorder = crate::rollout::RolloutRecorder::new_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    state.machine.set_input_recorder_for_test(recorder.clone());
    state
        .machine
        .add_assistant_message_with_tool_calls_and_reasoning(
            "",
            vec![crate::tape::ToolRequest {
                id: "call_mount".into(),
                name: "request_mount".into(),
                arguments: json!({}),
            }],
            None,
            None,
            &[],
        );
    let pending = state.machine.pending_host_mount(&request_id).unwrap();
    state.machine.record_event(
        crate::agent_machine::HOST_MOUNT_REQUEST_WAITING_EVENT_TYPE,
        serde_json::to_value(&pending).unwrap(),
    );
    let probe = recorder.flush_failure_probe().await;
    state.machine.set_input_recorder_for_test(probe.clone());
    for _ in 0..2 {
        assert!(
            super::super::turn_support::reset_turn_after_cancelling_host_mounts(
                &mut state.machine,
                &state.environment.host_mount_requests(),
            )
            .await
            .is_err()
        );
        assert!(state.machine.pending_host_mount(&request_id).is_some());
    }
    assert_eq!(
        host_mount.status(&request_id).await.as_deref(),
        Some("cancelled")
    );
    let count = state
        .machine
        .messages()
        .iter()
        .filter_map(|message| match message {
            crate::tape::Message::Tool { responses } => Some(responses),
            _ => None,
        })
        .flatten()
        .filter(|response| response.id == "call_mount")
        .count();
    assert_eq!(count, 1);
    let history = crate::rollout::RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    let responses = history
        .iter()
        .filter_map(|item| match item {
            crate::rollout::RolloutItem::Message(record) => record.message.as_ref(),
            _ => None,
        })
        .filter_map(|message| match message {
            crate::tape::Message::Tool { responses } => Some(responses),
            _ => None,
        })
        .flatten()
        .filter(|response| response.id == "call_mount")
        .count();
    assert!(
        responses <= 1,
        "failed-closed writer never appends a duplicate; enqueue-only records may not reach disk after the event flush fails"
    );
    assert!(probe.close().await.is_err());
    let recovered = crate::agent_machine::AgentMachine::load_from_rollout_in_dir(
        recorder.path(),
        "/agent/2",
        "test",
        dir.path(),
    )
    .await
    .unwrap();
    let recovered_responses = recovered
        .messages()
        .iter()
        .flat_map(|message| message.tool_responses())
        .filter(|response| response.id == "call_mount")
        .count();
    assert!(
        recovered.pending_host_mount(&request_id).is_some() || recovered_responses == 1,
        "durable recovery must retain the resumable wait or its paired terminal response"
    );
    assert_eq!(recovered_responses, responses);
    let terminals = history
        .iter()
        .filter(|item| {
            matches!(item,
        crate::rollout::RolloutItem::Event(event)
        if event.event_type == crate::agent_machine::HOST_MOUNT_REQUEST_TERMINAL_EVENT_TYPE)
        })
        .count();
    if terminals > 0 {
        assert_eq!(
            responses, 1,
            "terminal exclusion must never precede its Tool result"
        );
        assert_eq!(
            recovered.tool_payload_by_call_id("call_mount").unwrap()["status"],
            "cancelled"
        );
    }
}
