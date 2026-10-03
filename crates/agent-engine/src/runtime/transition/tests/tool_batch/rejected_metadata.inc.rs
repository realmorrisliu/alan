#[tokio::test]
async fn rejected_command_handler_records_runtime_metadata_without_execution() {
    for (id, op, message) in [
        (
            "empty-command",
            Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("  ")],
                mode: InputMode::FollowUp,
            },
            "missing command after ! prefix",
        ),
        (
            "command-control",
            Op::Interrupt,
            "command intent requires an input operation",
        ),
    ] {
        let (mut state, shell) = create_namespace_test_state_and_shell().await;
        let mut emit = |_event: Event| async {};
        let error = handle_submission_with_cancel(
            &mut state,
            Submission {
                id: id.to_string(),
                intent: alan_agent_protocol::InputIntent::Command,
                op,
            },
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), message);
        assert_eq!(state.agent_files().action_ids().await.unwrap(), vec!["a0"]);
        let result: Value =
            serde_json::from_slice(&shell.cat("/agent/1/actions/a0/result").await.unwrap())
                .unwrap();
        assert_eq!(result["call_id"], id);
        assert_eq!(result["exit_code"], 1);
        assert_eq!(result["outcome"], json!({"success":false,"error":message}));
        assert!(result.get("presentation").is_none());
        assert!(result.get("command").is_none());
        assert!(
            shell
                .cat("/agent/1/actions/a0/process")
                .await
                .unwrap()
                .is_empty()
        );
        let output: Value =
            serde_json::from_slice(&shell.cat("/agent/1/actions/a0/output").await.unwrap())
                .unwrap();
        assert_eq!(output, json!({"stdout":"", "stderr":message}));
        assert_eq!(
            shell.cat("/agent/1/actions/a0/status").await.unwrap(),
            b"failed"
        );
        assert_eq!(
            shell.cat("/agent/1/actions/a0/approval").await.unwrap(),
            b"not_required"
        );
        assert_eq!(result["title"], "Bash");
        assert_eq!(result["result_preview"], format!("error: {message}"));
    }
}
