#[tokio::test]
async fn failed_namespace_mutations_preserve_diagnostics_without_diff() {
    let (mut state, shell) = create_namespace_test_state_and_shell().await;
    for name in ["edit_file", "write_file"] {
        execute_single_tool_call(&mut state, name, name, json!({"path":"denied.txt", "old_string":"old", "new_string":"new", "content":"requested"})).await;
        let ids = state.agent_files().action_ids().await.unwrap();
        let base = format!("/agent/1/actions/{}", ids.last().unwrap());
        assert_eq!(
            shell.cat(&format!("{base}/status")).await.unwrap(),
            b"failed"
        );
        let result: Value =
            serde_json::from_slice(&shell.cat(&format!("{base}/result")).await.unwrap()).unwrap();
        assert_eq!(result["call_id"], name);
        assert_eq!(result["exit_code"], 1);
        assert!(
            result.get("presentation").is_none(),
            "failed {name} must not claim a requested Diff: {result}"
        );
        assert_eq!(
            result["result_preview"],
            "error: permission denied: denied.txt"
        );
    }
}
