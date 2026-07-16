use super::*;
use tempfile::TempDir;

#[path = "sandbox/reified_tests.rs"]
mod reified_tests;

#[path = "sandbox/spec_tests.rs"]
mod spec_tests;

#[path = "sandbox/command_shape_tests.rs"]
mod command_shape_tests;

#[tokio::test]
async fn test_sandbox_exec() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox.exec("echo hello", temp.path()).await.unwrap();
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_sandbox_exec_blocks_outside_host_mount_path_reference() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox.exec("cat /etc/passwd", temp.path()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_sandbox_exec_allows_host_mount_relative_paths() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    let file = temp.path().join("in_host_mount.txt");
    tokio::fs::write(&file, "ok").await.unwrap();

    let result = sandbox.exec("cat ./in_host_mount.txt", temp.path()).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().stdout.trim(), "ok");
}

#[tokio::test]
async fn test_sandbox_exec_allows_dev_null_redirection() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox.exec("echo ok > /dev/null", temp.path()).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().exit_code, 0);
}

#[tokio::test]
async fn test_sandbox_blocks_write_to_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected = temp.path().join(".git/config");
    tokio::fs::create_dir_all(protected.parent().unwrap())
        .await
        .unwrap();

    let result = sandbox.write(&protected, b"[core]\n").await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_sandbox_allows_read_from_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    let protected = temp.path().join(".alan/agents/default/policy.yaml");
    tokio::fs::create_dir_all(protected.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&protected, "rules: []\n").await.unwrap();

    let result = sandbox.read_string(&protected).await;
    assert_eq!(result.unwrap(), "rules: []\n");
}

#[tokio::test]
async fn read_denylist_blocks_in_process_reads_and_sensitive_listing() {
    let home = TempDir::new().unwrap();
    let secret_dir = home.path().join(".ssh");
    let secret_file = secret_dir.join("id_rsa");
    tokio::fs::create_dir_all(&secret_dir).await.unwrap();
    tokio::fs::write(&secret_file, "secret").await.unwrap();
    let sandbox = Sandbox::from_spec(SandboxSpec {
        host_mounts: Vec::new(),
        readable_roots: vec![home.path().to_path_buf()],
        writable_roots: vec![home.path().to_path_buf()],
        read_denylist: vec![secret_dir.clone()],
        network: NetworkPosture::Deny,
    });

    let root_entries = sandbox.list_dir(home.path()).await.unwrap();
    assert!(root_entries.iter().any(|entry| entry.file_name() == ".ssh"));

    let read = sandbox.read_string(&secret_file).await.unwrap_err();
    assert!(
        read.to_string().contains("sensitive read-deny path"),
        "{read}"
    );
    let listed = sandbox.list_dir(&secret_dir).await.unwrap_err();
    assert!(
        listed.to_string().contains("sensitive read-deny path"),
        "{listed}"
    );
}

#[tokio::test]
async fn test_sandbox_blocks_legacy_and_unknown_runtime_control_paths() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    for relative in [
        ".alan/memory/MEMORY.md",
        ".alan/runtime/canary/memory/MEMORY.md",
        ".alan/agent/persona/USER.md",
    ] {
        let err = sandbox
            .write(&temp.path().join(relative), b"must stay blocked\n")
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("protected subpath .alan"),
            "unexpected result for {relative}: {err}"
        );
    }
}

#[tokio::test]
async fn test_sandbox_blocks_write_with_parent_dir_bypass_into_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::create_dir_all(temp.path().join(".alan/agents/default"))
        .await
        .unwrap();

    let bypass_path = temp
        .path()
        .join(".alan/agents/default/persona/../policy.yaml");
    let result = sandbox.write(&bypass_path, b"rules: []\n").await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("protected subpath .alan")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_parent_dir_bypass_into_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::create_dir_all(temp.path().join(".alan/agents/default"))
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "touch .alan/agents/default/persona/../policy.yaml",
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
            .contains("protected subpath .alan")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_mutating_command_for_protected_path() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected = temp.path().join(".alan/config.toml");
    tokio::fs::create_dir_all(protected.parent().unwrap())
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "touch .alan/config.toml",
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
            .contains("protected subpath .alan")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_read_only_command_for_protected_path() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected = temp.path().join(".git/HEAD");
    tokio::fs::create_dir_all(protected.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&protected, "ref: refs/heads/main\n")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "cat .git/HEAD",
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
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_mutating_cwd_inside_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".agents");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "touch state.txt",
            &protected_dir,
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("protected subpath .agents")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_bare_protected_directory_token() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "rm -rf .git",
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
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_sandbox_blocks_symlink_alias_into_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();
    let alias = temp.path().join("safe");
    std::os::unix::fs::symlink(&protected_dir, &alias).unwrap();

    let result = sandbox.write(&alias.join("config"), b"[core]\n").await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_sandbox_blocks_symlink_alias_outside_writable_roots() {
    let host_mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        host_mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let outside_file = outside.path().join("secret.txt");
    tokio::fs::write(&outside_file, "secret").await.unwrap();
    let alias = host_mount.path().join("safe.txt");
    std::os::unix::fs::symlink(&outside_file, &alias).unwrap();

    let read = sandbox.read_string(&alias).await.unwrap_err();
    assert!(read.to_string().contains("outside host_mount"), "{read}");

    let write = sandbox.write(&alias, b"changed").await.unwrap_err();
    assert!(write.to_string().contains("outside host_mount"), "{write}");
}

#[tokio::test]
async fn test_sandbox_blocks_hardlink_alias_into_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected = temp.path().join(".git/config");
    tokio::fs::create_dir_all(protected.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&protected, "[core]\n").await.unwrap();
    let alias = temp.path().join("config-alias");
    std::fs::hard_link(&protected, &alias).unwrap();

    let result = sandbox.write(&alias, b"[user]\n").await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("multiply-linked file")
    );
}

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

#[tokio::test]
async fn test_sandbox_exec_ignores_absolute_path_literals_inside_comments() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox
        .exec_with_timeout_and_capability(
            "echo ok # /etc/passwd",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "ok");
}

#[tokio::test]
async fn test_sandbox_exec_ignores_shell_features_inside_comments() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox
        .exec("echo ok # $HOME * {a,b}", temp.path())
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "ok");
}

#[tokio::test]
async fn test_sandbox_exec_allows_bracket_test_syntax() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    tokio::fs::write(temp.path().join("README.md"), "ok")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "[ -f README.md ]",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_sandbox_exec_blocks_protected_redirection_without_whitespace() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "echo x>.git/config",
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
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_protected_path_with_line_continuation() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "rm -rf .g\\\nit",
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
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_post_comment_line_continuation_nested_eval() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "echo ok #\\\nsh -c 'rm -rf .git'",
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
async fn test_sandbox_exec_blocks_eval_wrapper_name_with_line_continuation() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "s\\\nh -c 'rm -rf .git'",
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
async fn test_sandbox_exec_blocks_wrapper_query_with_line_continuation() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "time\\\nout --ver\\\nsion",
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
async fn test_sandbox_exec_blocks_attached_short_option_path_argument() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();
    tokio::fs::write(temp.path().join("payload"), "ok")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "cp -t.git payload",
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
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_hardlink_process_path_reference() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected = temp.path().join(".git/config");
    tokio::fs::create_dir_all(protected.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&protected, "[core]\n").await.unwrap();
    let alias = temp.path().join("config-alias");
    std::fs::hard_link(&protected, &alias).unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "echo x > config-alias",
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
            .contains("multiply-linked file")
    );
}

#[tokio::test]
async fn test_sandbox_exec_blocks_protected_path_built_from_quoted_segments() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "rm -rf .g''it",
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
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_sandbox_exec_allows_quoted_relative_glob_path_patterns() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    let python_bin = temp.path().join("venv/bin/python");
    tokio::fs::create_dir_all(python_bin.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&python_bin, "#!/usr/bin/env python\n")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            r#"find . -maxdepth 3 -type f -path "*/bin/python""#,
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .expect("quoted relative path pattern should stay host_mount-safe");
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("./venv/bin/python"));
}

#[tokio::test]
async fn test_sandbox_exec_blocks_protected_path_in_option_assignment() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let protected_dir = temp.path().join(".git");
    tokio::fs::create_dir_all(&protected_dir).await.unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "git --git-dir=.git config alan.test true",
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
            .contains("protected subpath .git")
    );
}

#[tokio::test]
async fn test_os_backend_still_blocks_protected_subpath_redirection() {
    // With an OS sandbox active the shape parser is dropped, but the OS profile
    // allows writes anywhere under the host_mount (and Landlock can't carve out
    // protected subdirs), so explicit writes to .git/.alan/.agents must still be
    // blocked before the command runs.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for cmd in [
        // Direct redirection.
        "echo x > .git/config",
        "echo x > .alan/agents/default/policy.yaml",
        // Nested/quoted wrapper form — the inner script is inspected recursively.
        "bash -lc 'echo x > .git/config'",
        "sh -c \"echo x > .alan/agents/default/policy.yaml\"",
    ] {
        let result = sandbox
            .exec_with_timeout_and_capability(
                cmd,
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Write),
            )
            .await;
        assert!(result.is_err(), "protected write not blocked: {cmd}");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("protected subpath"),
            "wrong rejection for: {cmd}"
        );
    }
}

#[test]
fn only_seatbelt_permits_autonomous_bash() {
    use crate::tools::SandboxBackendKind;
    // Seatbelt is a complete bash boundary (host_mount fs + network), so wrappers
    // run and escalated bash is reviewer-eligible.
    assert!(SandboxBackendKind::Seatbelt.permits_autonomous_bash());
    // Landlock (network confinement is kernel-conditional), Linux reified
    // namespace (protected subpaths are not carved out of the writable host_mount
    // mount), and the path-guard fallback are treated conservatively: full shape
    // parser, escalated bash to a human.
    assert!(!SandboxBackendKind::LinuxReifiedNamespace.permits_autonomous_bash());
    assert!(!SandboxBackendKind::Landlock.permits_autonomous_bash());
    assert!(!SandboxBackendKind::HostMountPathGuard.permits_autonomous_bash());
}

#[tokio::test]
async fn test_landlock_keeps_shape_parser_for_opaque_writers() {
    // Landlock can't kernel-deny protected subpaths, so opaque writers (which the
    // protected-only check can't inspect) must still be rejected by the shape
    // parser — the same posture as the path-guard fallback.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Landlock,
    );
    let result = sandbox
        .exec_with_timeout_and_capability(
            "python -c 'open(\".git/config\",\"w\").write(\"x\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(result.is_err(), "opaque writer not rejected under Landlock");
}

#[tokio::test]
async fn test_os_backend_still_blocks_out_of_host_mount_reads() {
    // Seatbelt denies writes/network but permits reads, so the parser must still
    // contain reads: an auto-approved `cat ~/.ssh/id_rsa` / `cat /etc/passwd` must
    // not exfiltrate secrets into tool output. ProtectedOnly drops only the shape
    // checks, never path containment.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for cmd in [
        "cat ~/.ssh/id_rsa",
        "cat /etc/passwd",
        "bash -lc 'cat /etc/passwd'",
    ] {
        let result = sandbox
            .exec_with_timeout_and_capability(
                cmd,
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Read),
            )
            .await;
        assert!(result.is_err(), "out-of-host_mount read not blocked: {cmd}");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside host_mount"),
            "wrong rejection for: {cmd}"
        );
    }
}

#[tokio::test]
async fn test_os_backend_rejects_shell_expansion_reads() {
    // Shell expansion defeats static path containment: `$HOME/.ssh/id_rsa` looks
    // host_mount-relative to the parser but `/bin/sh -c` expands it to escape.
    // validate_shell_features must run in ProtectedOnly mode too, rejecting `$`.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for cmd in [
        "cat $HOME/.ssh/id_rsa",
        "ls $HOME",
        "cat $(echo /etc/passwd)",
    ] {
        let result = sandbox
            .exec_with_timeout_and_capability(
                cmd,
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Read),
            )
            .await;
        assert!(result.is_err(), "shell expansion not rejected: {cmd}");
        assert!(
            result.unwrap_err().to_string().contains("expansion"),
            "wrong rejection for: {cmd}"
        );
    }
}

#[tokio::test]
async fn test_os_backend_unwraps_transparent_wrappers_for_protected_and_reads() {
    // Transparent wrappers (`env`, `command`, `timeout`, ...) must be peeled so the
    // inline shell script is still inspected under ProtectedOnly — otherwise the
    // quoted script is opaque and its .git write / out-of-host_mount read escapes.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );

    let protected = sandbox
        .exec_with_timeout_and_capability(
            "env bash -lc 'echo x > .git/config'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(protected.is_err(), "wrapper-hidden .git write not blocked");
    assert!(
        protected
            .unwrap_err()
            .to_string()
            .contains("protected subpath"),
        "wrong rejection for wrapper-hidden .git write"
    );

    let read = sandbox
        .exec_with_timeout_and_capability(
            "command bash -lc 'cat ~/.ssh/id_rsa'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(
        read.is_err(),
        "wrapper-hidden out-of-host_mount read not blocked"
    );
    assert!(
        read.unwrap_err().to_string().contains("outside host_mount"),
        "wrong rejection for wrapper-hidden read"
    );
}
