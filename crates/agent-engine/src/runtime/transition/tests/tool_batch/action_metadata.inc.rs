#[tokio::test]
async fn terminal_action_metadata_is_visible_and_restores_without_tool_replay() {
    let (mut state, shell) = create_namespace_test_state_and_shell().await;
    // The runner returns logical failure in stdout JSON with native exit zero;
    // Action status and typed Command presentation must both preserve that failure.
    let dir = tempfile::tempdir().unwrap();
    let recorder = crate::rollout::RolloutRecorder::new_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    state.environment = state
        .environment
        .clone()
        .with_action_recorder(Some(recorder.clone()));
    for (id, name, args, title) in [
        (
            "read",
            "read_file",
            json!({"path":"sample.txt"}),
            "Read sample.txt",
        ),
        (
            "edit",
            "edit_file",
            json!({"path":"sample.txt", "old_string":"old", "new_string":"new"}),
            "Edit sample.txt",
        ),
        (
            "bash",
            "bash",
            json!({"command":"echo readable"}),
            "Bash echo readable",
        ),
        ("failed", "bash", json!({"command":"exit 7"}), "Bash exit 7"),
    ] {
        execute_single_tool_call(&mut state, id, name, args.clone()).await;
        let ids = state.agent_files().action_ids().await.unwrap();
        let action = ids.last().unwrap();
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/actions/{action}/status"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            if id == "failed" {
                "failed"
            } else {
                "completed"
            }
        );
        let result: Value = serde_json::from_slice(
            &shell
                .cat(&format!("/agent/1/actions/{action}/result"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["call_id"], id);
        assert_eq!(
            result["title"], title,
            "metadata must exist when terminal status is visible"
        );
        if name == "read_file" {
            assert_eq!(result["result_preview"], "from namespace read_file");
        } else {
            let presentation: alan_agent_protocol::ToolResultPresentation =
                serde_json::from_value(result["presentation"].clone()).unwrap();
            match presentation {
                alan_agent_protocol::ToolResultPresentation::Command {
                    cmdline,
                    exit_code,
                    stdout,
                    stderr,
                    ..
                } => {
                    assert_eq!(cmdline, args["command"].as_str().unwrap());
                    assert_eq!(exit_code, Some(if id == "failed" { 7 } else { 0 }));
                    assert_eq!(
                        stdout,
                        if id == "failed" {
                            ""
                        } else {
                            "readable output"
                        }
                    );
                    assert_eq!(stderr, if id == "failed" { "command failed" } else { "" });
                }
                alan_agent_protocol::ToolResultPresentation::Diff { path, hunks } => {
                    assert_eq!(path, "sample.txt");
                    assert!(!hunks.is_empty());
                }
                other => panic!("unexpected presentation: {other:?}"),
            }
        }
    }
    recorder.flush().await.unwrap();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/2",
        InProcessTransport::new(Arc::new(AgentFs::new())),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let restored_shell = Shell::new(root.clone());
    let restored = NamespaceRuntimeEnvironment::new(root, "/agent/2", "default");
    restored
        .agent_files()
        .restore_actions(recorder.path())
        .await
        .unwrap();
    for id in state.agent_files().action_ids().await.unwrap() {
        assert_eq!(
            shell
                .cat(&format!("/agent/1/actions/{id}/result"))
                .await
                .unwrap(),
            restored_shell
                .cat(&format!("/agent/2/actions/{id}/result"))
                .await
                .unwrap()
        );
    }
}
