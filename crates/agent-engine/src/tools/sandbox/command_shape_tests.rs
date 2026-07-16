use super::super::*;
use tempfile::TempDir;

#[tokio::test]
async fn test_sandbox_exec_blocks_mutating_variable_expansion() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "d=.git && rm -rf \"$d\"",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell variable, command, brace, or glob expansion")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_globbed_process_paths() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "rm -rf .g*",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_err());
    assert!(protected_dir.exists());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell variable, command, brace, or glob expansion")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_set_plus_f_glob_bypass_attempt() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "set +f; rm -rf .g*",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(protected_dir.exists());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell variable, command, brace, or glob expansion")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_read_only_variable_expansion() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "f=/etc/passwd && cat \"$f\"",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell variable, command, brace, or glob expansion")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_brace_expansion() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "rm -rf .{git,alan}",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell variable, command, brace, or glob expansion")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_multiline_nested_shell_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "echo ok\nsh -c 'rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like sh -c")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_nested_shell_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "sh -c 'rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like sh -c")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_nested_python_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "python3 -c 'print(\"hi\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like python3 -c")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_shell_eval_wrapper_with_leading_option() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "bash --noprofile -c 'rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like bash -c")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_python_eval_wrapper_with_leading_option() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "python3 -B -c 'open(\".git/config\", \"w\").write(\"x\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like python3 -c")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_node_print_eval_wrapper_with_leading_option() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "node --trace-warnings -p 'require(\"fs\").writeFileSync(\".git/config\", \"x\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like node -p")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_node_inline_long_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "node --eval='require(\"fs\").writeFileSync(\".git/config\", \"x\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like node --eval=")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_node_inline_long_print_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "node --print='require(\"fs\").writeFileSync(\".git/config\", \"x\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like node --print=")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_shell_inline_long_command_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "sh --command='rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like sh --command=")
    );
}

#[tokio::test]
async fn test_sandbox_exec_allows_literal_sh_dash_c_arguments() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox
        .exec_with_timeout_and_capability(
            "printf '%s %s' sh -c",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "sh -c");
}

#[tokio::test]
async fn test_sandbox_exec_blocks_eval_builtin() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "eval 'rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like eval")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_command_eval_builtin() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "command eval 'rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell wrappers like command")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_source_builtin() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            ". ./script.sh",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects nested command evaluators like .")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_env_shell_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "env FOO=bar sh -c 'rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell wrappers like env")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_bang_prefixed_nested_shell_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "! sh -c 'rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell control flow like !")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_if_prefixed_nested_shell_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "if sh -c 'rm -rf .git'; then :; fi",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell control flow like if")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_env_split_string_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "env -S 'sh -c rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects shell wrappers like env")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_xargs_dispatcher() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "printf x | xargs sh -c 'rm -rf .git'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects opaque command dispatchers like xargs")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_find_exec_dispatcher() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "find . -exec sh -c 'rm -rf .git' \\;",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("rejects opaque command dispatchers like find -exec")
    );
}

#[tokio::test]
async fn test_sandbox_exec_allows_find_without_dispatch_clause() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    tokio::fs::write(temp.path().join("README.md"), "ok")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "find . -name 'README.md'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_sandbox_exec_allows_find_name_literal_that_looks_like_exec_flag() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    tokio::fs::write(temp.path().join("-exec"), "ok")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "find . -name '-exec' -o -name '+'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_sandbox_exec_does_not_treat_non_find_exec_flag_as_dispatcher() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox
        .exec_with_timeout_and_capability(
            "printf '%s\n' -exec ';'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_ok());
}
