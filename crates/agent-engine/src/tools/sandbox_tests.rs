use super::*;
use tempfile::TempDir;

#[path = "sandbox/native_shell_tests.rs"]
mod native_shell_tests;

#[path = "sandbox/reified_tests.rs"]
mod reified_tests;

#[cfg(target_os = "linux")]
#[path = "sandbox/rustup_tests.rs"]
mod rustup_tests;

#[cfg(target_os = "linux")]
#[path = "sandbox/development_tests.rs"]
mod development_tests;

#[path = "sandbox/spec_tests.rs"]
mod spec_tests;

#[path = "sandbox/manifest_operand_tests.rs"]
mod manifest_operand_tests;

#[path = "sandbox/command_shape_tests.rs"]
mod command_shape_tests;

#[path = "sandbox/interpreter_wrapper_tests.rs"]
mod interpreter_wrapper_tests;

#[path = "sandbox/awk_projection_tests.rs"]
mod awk_projection_tests;

#[tokio::test]
async fn test_sandbox_exec() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox.exec("echo hello", temp.path()).await.unwrap();
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.exit_code, 0);
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn sandbox_preserves_a_namespace_path_written_as_printf_data() {
    use crate::tools::reified_namespace::ReifiedMountAccess;
    use std::path::PathBuf;

    let mount = TempDir::new().unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, crate::tools::SandboxBackendKind::Seatbelt);

    sandbox
        .exec_with_timeout_and_capability(
            "printf '%s' /mnt/project > ./path.txt",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(mount.path().join("path.txt")).unwrap(),
        "/mnt/project"
    );
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
        "bash -c 'echo x > .git/config'",
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

#[path = "sandbox/os_boundary_tests.rs"]
mod os_boundary_tests;

#[cfg(feature = "tree-sitter-sandbox")]
#[path = "sandbox/standalone_cd_tests.rs"]
mod standalone_cd_tests;

#[test]
fn sensitive_read_deny_hint_formats_non_leaking_hints_and_keeps_denylist_intact() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("users/secret_alice");
    std::fs::create_dir_all(home.join(".config/gh")).unwrap();
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    let denylist = SandboxSpec::sensitive_read_denylist_for_home(&home);
    assert!(denylist.contains(&home.join(".config/gh")));
    assert!(denylist.contains(&home.join("Library/Keychains")));

    let sandbox = Sandbox::from_spec(SandboxSpec {
        host_mounts: Vec::new(),
        readable_roots: vec![temp.path().to_path_buf()],
        writable_roots: vec![temp.path().to_path_buf()],
        read_denylist: denylist,
        network: NetworkPosture::Deny,
    });
    // Security invariant: read_denylist preserves .config/gh and Library/Keychains.
    assert!(
        sandbox
            .spec()
            .read_denylist
            .contains(&home.join(".config/gh"))
    );
    assert!(
        sandbox
            .spec()
            .read_denylist
            .contains(&home.join("Library/Keychains"))
    );

    // Raw home path + operation not permitted -> ~/.config/gh hint with zero home/username leak.
    let gh_stderr = format!(
        "open {}/hosts.yml: operation not permitted",
        home.join(".config/gh").display()
    );
    let gh_hint = sandbox
        .sensitive_read_deny_hint_with_home(&gh_stderr, Some(&home))
        .expect("expected gh credential hint");
    assert_eq!(
        gh_hint,
        "hint: OS sandbox blocks host credentials (~/.config/gh). Mount /mnt/github (--fileserver github-fileserver:ro:net), pass a scoped GH_TOKEN, or run with --unsafe-host-account."
    );
    assert!(!gh_hint.contains("secret_alice"));
    assert!(!gh_hint.contains(&home.display().to_string()));

    // Canonicalized path (e.g. /private/var vs /var on macOS) + Permission denied also matches.
    let canonical_ssh = dunce::canonicalize(home.join(".ssh")).unwrap();
    let ssh_stderr = format!("cat: {}/id_rsa: Permission denied", canonical_ssh.display());
    let ssh_hint = sandbox
        .sensitive_read_deny_hint_with_home(&ssh_stderr, Some(&home))
        .expect("expected ssh credential hint");
    assert_eq!(
        ssh_hint,
        "hint: OS sandbox blocks host credentials (~/.ssh). Pass explicit scoped credentials or run with --unsafe-host-account."
    );
    assert!(!ssh_hint.contains("secret_alice"));

    // Path-boundary check: `.alan-dev` matches `.alan-dev`, and non-denied `.ssh_backup` returns None.
    let non_denied = format!(
        "cat: {}_backup/id_rsa: Operation not permitted",
        home.join(".ssh").display()
    );
    assert_eq!(
        sandbox.sensitive_read_deny_hint_with_home(&non_denied, Some(&home)),
        None
    );

    // Non-permission errors return None.
    let enoent = format!(
        "open {}/hosts.yml: no such file or directory",
        home.join(".config/gh").display()
    );
    assert_eq!(
        sandbox.sensitive_read_deny_hint_with_home(&enoent, Some(&home)),
        None
    );

    // Unsandboxed posture returns None.
    let unsandboxed = sandbox.with_trust_posture(crate::policy::TrustPosture::HostAccountTrusted);
    assert_eq!(
        unsandboxed.sensitive_read_deny_hint_with_home(&gh_stderr, Some(&home)),
        None
    );
}
