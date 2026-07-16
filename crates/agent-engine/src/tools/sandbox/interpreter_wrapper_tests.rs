use super::super::*;
use tempfile::TempDir;

#[tokio::test]
async fn test_sandbox_exec_blocks_python_script_file_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.py"), "print('ok')")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "python3 script.py",
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
            .contains("rejects opaque script interpreters like python3 script.py")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_python_module_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "python3 -m http.server",
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
            .contains("rejects opaque script interpreters like python3 -m")
    );
}

#[test]
fn test_bash_preflight_allows_python_module_pytest() {
    assert!(Sandbox::bash_preflight_reason("python3 -m pytest -q test_requests.py").is_none());
}

#[tokio::test]
async fn test_sandbox_exec_blocks_wrapped_python_script_file_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.py"), "print('ok')")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "env FOO=bar python3 script.py",
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
async fn test_sandbox_exec_blocks_shell_script_file_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.sh"), "echo ok")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "bash script.sh",
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
            .contains("rejects opaque script interpreters like bash script.sh")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_node_script_file_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.js"), "console.log('ok')")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "node script.js",
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
            .contains("rejects opaque script interpreters like node script.js")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_node_stdin_interpreter_via_pipe() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "printf 'console.log(1)' | node",
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
            .contains("rejects opaque script interpreters like node <stdin>")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_awk_script_file_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.awk"), "{ print $0 }")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "awk -f script.awk input.txt",
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
            .contains("rejects opaque script interpreters like awk -f")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_inline_awk_script_file_option_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.awk"), "{ print $0 }")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "awk --file=script.awk input.txt",
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
            .contains("rejects opaque script interpreters like awk -f")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_inline_php_script_file_option_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.php"), "<?php echo 'ok';")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "php --file=script.php",
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
            .contains("rejects opaque script interpreters like php -f")
    );
}

#[tokio::test]
async fn test_sandbox_exec_allows_python_query_mode_without_script_execution() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox
        .exec_with_timeout_and_capability(
            "python3 --version",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_sandbox_exec_allows_direct_command_with_leading_env_assignment() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox
        .exec_with_timeout_and_capability(
            "ALAN_TEST=1 pwd",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_sandbox_exec_blocks_nice_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "nice -n 5 sh -c 'rm -rf .git'",
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
            .contains("rejects shell wrappers like nice")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_timeout_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.py"), "print('ok')")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "timeout --signal=TERM 5 python3 script.py",
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
            .contains("rejects shell wrappers like timeout")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_chained_wrapped_shell_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "timeout --signal=TERM 5 nice -n 5 sh -c 'rm -rf .git'",
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
            .contains("rejects shell wrappers like timeout")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_nohup_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("script.sh"), "echo ok")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "nohup bash script.sh",
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
            .contains("rejects shell wrappers like nohup")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_stdbuf_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "stdbuf -oL sh -c 'rm -rf .git'",
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
            .contains("rejects shell wrappers like stdbuf")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_setsid_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "setsid sh -c 'rm -rf .git'",
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
            .contains("rejects shell wrappers like setsid")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_time_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "time sh -c 'rm -rf .git'",
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
            .contains("rejects shell wrappers like time")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_timeout_query_mode_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "timeout --version",
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
            .contains("rejects shell wrappers like timeout")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_clustered_env_split_string_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "env -iS 'sh -c rm -rf .git'",
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
async fn test_sandbox_exec_blocks_command_wrapper_with_leading_option() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "command -p sh -c 'rm -rf .git'",
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
async fn test_sandbox_exec_blocks_command_query_mode_with_eval_like_argv() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "command -v sh -c",
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
            .contains("rejects shell wrappers like command")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_builtin_eval_after_end_of_options() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "builtin -- eval 'rm -rf .git'",
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
            .contains("rejects shell wrappers like builtin")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_exec_shell_eval_wrapper() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "exec sh -c 'rm -rf .git'",
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
            .contains("rejects shell wrappers like exec")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_exec_shell_eval_wrapper_with_argv0_option() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "exec -a alan sh -c 'rm -rf .git'",
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
            .contains("rejects shell wrappers like exec")
    );
}
