#[tokio::test]
async fn command_handler_projects_namespace_result_and_missing_tool_diagnostic() {
    for mounted in [true, false] {
        let (mut state, shell) = create_namespace_test_state_and_shell_with_bin(mounted).await;
        let id = "handler-command";
        state.machine.accept_submission(id.to_string());
        let mut emit = |_event: Event| async {};
        handle_submission_with_cancel(
            &mut state,
            Submission {
                id: id.to_string(),
                intent: alan_agent_protocol::InputIntent::Command,
                op: Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text("exit 7")],
                    mode: InputMode::FollowUp,
                },
            },
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let ids = state.agent_files().action_ids().await.unwrap();
        assert_eq!(ids.len(), 1);
        let base = format!("/agent/1/actions/{}", ids[0]);
        let result: Value =
            serde_json::from_slice(&shell.cat(&format!("{base}/result")).await.unwrap()).unwrap();
        assert_eq!(result["call_id"], id);
        assert_eq!(result["title"], "Bash exit 7");
        assert_eq!(
            shell.cat(&format!("{base}/status")).await.unwrap(),
            b"failed"
        );
        assert_eq!(
            shell.cat(&format!("{base}/approval")).await.unwrap(),
            b"not_required"
        );
        if mounted {
            assert_eq!(result["exit_code"], 7);
            assert_eq!(result["presentation"]["exit_code"], 7);
            assert_eq!(result["presentation"]["stderr"], "command failed");
            assert!(
                !shell
                    .cat(&format!("{base}/process"))
                    .await
                    .unwrap()
                    .is_empty()
            );
        } else {
            assert!(result.get("presentation").is_none());
            assert!(
                result["result_preview"]
                    .as_str()
                    .unwrap()
                    .starts_with("error: ")
            );
            assert!(result["outcome"]["error"].is_string());
            assert!(
                shell
                    .cat(&format!("{base}/process"))
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[tokio::test]
async fn action_metadata_redaction_and_recorder_failure_use_existing_durable_owner() {
    let (mut state, shell) = create_namespace_test_state_and_shell().await;
    let dir = tempfile::tempdir().unwrap();
    let recorder = crate::rollout::RolloutRecorder::new_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    state.environment = state
        .environment
        .clone()
        .with_action_recorder(Some(recorder.clone()));
    // URL query redaction applies to titles, previews and Diff lines alike.
    let secret = "https://example.invalid/?token=fake-private-value";
    execute_single_tool_call(
        &mut state,
        "private",
        "write_file",
        json!({"path":secret, "content":secret}),
    )
    .await;
    let bytes = shell.cat("/agent/1/actions/a0/result").await.unwrap();
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    for field in ["title", "result_preview", "presentation"] {
        let text = result[field].to_string();
        assert!(!text.contains("fake-private-value"), "{field}: {text}");
        assert!(text.contains("REDACTED"), "{field}: {text}");
    }
    recorder.flush().await.unwrap();
    let (fresh, fresh_shell) = create_namespace_test_state_and_shell_with_bin(false).await;
    fresh
        .agent_files()
        .restore_actions(recorder.path())
        .await
        .unwrap();
    assert_eq!(
        fresh_shell.cat("/agent/1/actions/a0/result").await.unwrap(),
        bytes
    );
    assert_eq!(
        fresh_shell.cat("/agent/1/actions/a0/output").await.unwrap(),
        shell.cat("/agent/1/actions/a0/output").await.unwrap()
    );

    let (probe, mut observed) = recorder.batch_failure_probe(false);
    state.environment = state
        .environment
        .clone()
        .with_action_recorder(Some(probe.clone()));
    let error = state
        .tool_execution()
        .run_action_with_cancel_and_timeout(
            "write_file",
            Some(NamespaceToolActionEvidence {
                call_id: "uncommitted",
                approval: "not_required",
                arguments: &json!({"path":"safe.txt", "content":"requested"}),
                submission_id: None,
            }),
            "/bin/write_file",
            [json!({"path":"safe.txt", "content":"requested"}).to_string()],
            &CancellationToken::new(),
            30,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("persist Action evidence"));
    let items = observed.recv().await.unwrap();
    assert!(
        matches!(&items[0], crate::rollout::RolloutItem::Event(event) if event.payload["record"]["result"].as_str().unwrap().contains("presentation"))
    );
    assert_ne!(
        shell.cat("/agent/1/actions/a1/status").await.unwrap(),
        b"completed"
    );
    let events = String::from_utf8(shell.cat("/agent/1/actions/events").await.unwrap()).unwrap();
    assert!(!events.lines().any(|line| line == "a1:status"));
    let history = crate::rollout::RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(event) if event.event_type == "agent_action_v1" && event.payload["action_id"] == "a1")));
    probe.close().await.unwrap();
}
