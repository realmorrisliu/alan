use super::*;

#[tokio::test]
async fn native_shell_keeps_multiline_pipeline_redirection_and_partial_failure() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox
        .exec_with_timeout_and_capability(
            "printf '%s\\n' 'quoted value' | tr 'a-z' 'A-Z'\nprintf '%s\\n' 'kept after failure' > partial.txt\nfalse",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.stdout, "QUOTED VALUE\n");
    assert_eq!(result.exit_code, 1);
    assert_eq!(
        tokio::fs::read_to_string(temp.path().join("partial.txt"))
            .await
            .unwrap(),
        "kept after failure\n"
    );
}
