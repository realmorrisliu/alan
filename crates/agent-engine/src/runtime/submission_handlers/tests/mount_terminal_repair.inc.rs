#[tokio::test]
async fn mount_retry_repairs_missing_terminal_evidence_without_duplicate_response() {
    let (mut state, host_mount, request_id) = pending_host_mount_state().await;
    host_mount
        .settle(&request_id, "approved", Some("grant-repair"), None)
        .await;
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
    // The real response reaches disk, but its flush fails before terminal evidence.
    state.machine.add_tool_message(
        "call_mount",
        "request_mount",
        json!({
            "status":"approved", "approved":true, "grant_reference":"grant-repair",
            "request_reference":request_id,
        }),
    );
    assert!(state.machine.flush_recorder().await.is_err());
    assert!(probe.close().await.is_err());
    let before = crate::agent_machine::AgentMachine::load_from_rollout_in_dir(
        recorder.path(),
        "/agent/2",
        "test",
        dir.path(),
    )
    .await
    .unwrap();
    assert!(before.pending_host_mount(&request_id).is_some());
    assert_eq!(
        before.tool_payload_by_call_id("call_mount").unwrap()["grant_reference"],
        "grant-repair"
    );
    state.machine = before;
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    handle_runtime_op_with_cancel(
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
    state.machine.flush_recorder().await.unwrap();
    let owning_path = state.machine.rollout_path().unwrap().clone();
    let history = crate::rollout::RolloutRecorder::load_history(&owning_path)
        .await
        .unwrap();
    assert_eq!(
        history
            .iter()
            .filter_map(|item| match item {
                crate::rollout::RolloutItem::Message(record) => record.message.as_ref(),
                _ => None,
            })
            .flat_map(|message| message.tool_responses())
            .filter(|response| response.id == "call_mount")
            .count(),
        1,
        "healthy replacement rollout must not re-enqueue an already durable response"
    );
    let recovered = crate::agent_machine::AgentMachine::load_from_rollout_in_dir(
        &owning_path,
        "/agent/3",
        "test",
        dir.path(),
    )
    .await
    .unwrap();
    assert!(
        recovered.pending_host_mount(&request_id).is_none(),
        "dedup must not skip evidence repair"
    );
    assert_eq!(
        recovered
            .messages()
            .iter()
            .flat_map(|message| message.tool_responses())
            .filter(|response| response.id == "call_mount")
            .count(),
        1
    );
    let payload = recovered.tool_payload_by_call_id("call_mount").unwrap();
    assert_eq!(payload["status"], "approved");
    assert_eq!(payload["grant_reference"], "grant-repair");
    assert_eq!(
        host_mount.status(&request_id).await.as_deref(),
        Some("approved")
    );
}
