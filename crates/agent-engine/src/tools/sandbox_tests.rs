use super::*;
use std::collections::BTreeSet;
use tempfile::TempDir;

#[test]
fn sensitive_read_denylist_for_home_includes_core_secret_stores() {
    let home = Path::new("/Users/alice");
    let denylist = SandboxSpec::sensitive_read_denylist_for_home(home);

    for expected in [
        ".alan",
        ".alan-dev",
        ".ssh",
        ".aws",
        ".config/gcloud",
        ".config/gh",
        ".docker",
        ".gnupg",
        ".kube",
        ".netrc",
        ".npmrc",
        ".pypirc",
        "Library/Keychains",
        "Library/Safari",
        "Library/Application Support/Arc",
        "Library/Application Support/BraveSoftware",
        "Library/Application Support/Chromium",
        "Library/Application Support/Firefox",
        "Library/Application Support/Google/Chrome",
        "Library/Application Support/com.apple.Safari",
    ] {
        assert!(
            denylist.contains(&home.join(expected)),
            "denylist missing {}",
            home.join(expected).display()
        );
    }

    let unique = denylist.iter().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), denylist.len());
}

#[test]
fn sandbox_spec_seed_includes_default_sensitive_read_denylist() {
    let workspace = PathBuf::from("/workspace");
    let spec = SandboxSpec::seed(workspace.clone());

    assert_eq!(spec.writable_roots, vec![workspace]);
    assert_eq!(
        spec.read_denylist,
        SandboxSpec::default_sensitive_read_denylist()
    );
    assert_eq!(spec.network, NetworkPosture::Deny);
}

#[test]
fn sandbox_spec_excludes_exact_writable_root_read_denies() {
    let home = Path::new("/Users/alice");
    let workspace = home.join(".alan");
    let sandbox = Sandbox::from_spec(SandboxSpec {
        writable_roots: vec![workspace.clone()],
        read_denylist: SandboxSpec::sensitive_read_denylist_for_home(home),
        network: NetworkPosture::Deny,
    });

    assert!(!sandbox.spec.read_denylist.contains(&workspace));
    assert!(sandbox.spec.read_denylist.contains(&home.join(".ssh")));
    assert!(sandbox.spec.read_denylist.contains(&home.join(".alan-dev")));
}

#[test]
fn sandbox_spec_preserves_parent_read_denies_for_nested_writable_roots() {
    let home = Path::new("/Users/alice");
    let sensitive_parent = home.join(".ssh");
    let workspace = sensitive_parent.join("project");
    let sandbox = Sandbox::from_spec(SandboxSpec {
        writable_roots: vec![workspace],
        read_denylist: vec![sensitive_parent.clone()],
        network: NetworkPosture::Deny,
    });

    assert!(sandbox.spec.read_denylist.contains(&sensitive_parent));
}

#[tokio::test]
async fn test_sandbox_read_write() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    // Write a file
    let file_path = temp.path().join("test.txt");
    sandbox.write(&file_path, b"hello world").await.unwrap();

    // Read it back
    let content = sandbox.read_string(&file_path).await.unwrap();
    assert_eq!(content, "hello world");
}

#[tokio::test]
async fn sandbox_allows_each_writable_root() {
    let workspace = TempDir::new().unwrap();
    let approved = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec(SandboxSpec {
        writable_roots: vec![
            workspace.path().to_path_buf(),
            approved.path().to_path_buf(),
        ],
        read_denylist: Vec::new(),
        network: NetworkPosture::Deny,
    });

    let approved_file = approved.path().join("notes.txt");
    sandbox.write(&approved_file, b"hello mount").await.unwrap();
    assert_eq!(
        sandbox.read_string(&approved_file).await.unwrap(),
        "hello mount"
    );
    assert!(
        sandbox
            .list_dir(approved.path())
            .await
            .unwrap()
            .iter()
            .any(|entry| entry.file_name() == "notes.txt")
    );
    assert!(sandbox.is_in_workspace(&workspace.path().join("a.txt")));
    assert!(sandbox.is_in_workspace(&approved.path().join("b.txt")));
    assert!(!sandbox.is_in_workspace(&outside.path().join("c.txt")));

    let outside_read = sandbox.read(&outside.path().join("secret.txt")).await;
    assert!(outside_read.is_err());
    assert!(
        outside_read
            .unwrap_err()
            .to_string()
            .contains("outside workspace")
    );
}

#[tokio::test]
async fn sandbox_exec_accepts_cwd_under_secondary_writable_root() {
    let workspace = TempDir::new().unwrap();
    let approved = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            writable_roots: vec![
                workspace.path().to_path_buf(),
                approved.path().to_path_buf(),
            ],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
    );
    tokio::fs::write(approved.path().join("in_mount.txt"), "ok")
        .await
        .unwrap();

    let result = sandbox.exec("cat ./in_mount.txt", approved.path()).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().stdout.trim(), "ok");

    let outside_result = sandbox.exec("echo no", outside.path()).await;
    assert!(outside_result.is_err());
    assert!(
        outside_result
            .unwrap_err()
            .to_string()
            .contains("outside workspace")
    );
}

#[tokio::test]
async fn sandbox_protects_reserved_subpaths_under_secondary_writable_root() {
    let workspace = TempDir::new().unwrap();
    let approved = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            writable_roots: vec![
                workspace.path().to_path_buf(),
                approved.path().to_path_buf(),
            ],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
    );
    let protected = approved.path().join(".git/config");
    tokio::fs::create_dir_all(protected.parent().unwrap())
        .await
        .unwrap();

    let result = sandbox.write(&protected, b"[core]\n").await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("protected subpath")
    );
}

#[tokio::test]
async fn test_sandbox_blocks_outside_workspace() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
    );

    // Try to read outside workspace
    let outside_path = PathBuf::from("/etc/passwd");
    let result = sandbox.read(&outside_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn sandbox_spec_writable_roots_allow_host_absolute_paths() {
    let workspace = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            writable_roots: vec![workspace.path().to_path_buf(), host.path().to_path_buf()],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
    );
    let target = host.path().join("created.txt");

    assert!(sandbox.is_in_workspace(host.path()));
    let result = sandbox
        .exec(&format!("touch {}", target.display()), workspace.path())
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
    assert!(target.exists());
}

#[tokio::test]
async fn sandbox_spec_writable_roots_still_block_host_protected_subpaths() {
    let workspace = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            writable_roots: vec![workspace.path().to_path_buf(), host.path().to_path_buf()],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
    );
    let protected = host.path().join(".git/config");
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
async fn sandbox_spec_writable_roots_block_protected_roots_themselves() {
    let workspace = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    let protected_root = host.path().join(".git");
    tokio::fs::create_dir_all(&protected_root).await.unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            writable_roots: vec![workspace.path().to_path_buf(), protected_root.clone()],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
    );

    let result = sandbox
        .write(&protected_root.join("config"), b"[core]\n")
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
async fn test_sandbox_exec() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());

    let result = sandbox.exec("echo hello", temp.path()).await.unwrap();
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_sandbox_exec_blocks_outside_workspace_path_reference() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
    );

    let result = sandbox.exec("cat /etc/passwd", temp.path()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_sandbox_exec_allows_workspace_relative_paths() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    let file = temp.path().join("in_workspace.txt");
    tokio::fs::write(&file, "ok").await.unwrap();

    let result = sandbox.exec("cat ./in_workspace.txt", temp.path()).await;
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
async fn test_sandbox_allows_write_to_workspace_persona_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    let persona_file = temp.path().join(".alan/agents/default/persona/USER.md");

    sandbox
        .write(&persona_file, b"# USER\n- Preferred name: Test\n")
        .await
        .unwrap();

    let written = tokio::fs::read_to_string(&persona_file).await.unwrap();
    assert!(written.contains("Preferred name"));
}

#[tokio::test]
async fn test_sandbox_allows_write_to_workspace_memory_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    let memory_file = temp.path().join(".alan/memory/MEMORY.md");

    sandbox.write(&memory_file, b"# Memory\n").await.unwrap();

    let written = tokio::fs::read_to_string(&memory_file).await.unwrap();
    assert_eq!(written, "# Memory\n");
}

#[tokio::test]
async fn test_sandbox_blocks_write_with_parent_dir_bypass_into_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
async fn test_sandbox_exec_allows_direct_command_for_workspace_memory_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    let memory_dir = temp.path().join(".alan/memory");
    tokio::fs::create_dir_all(&memory_dir).await.unwrap();
    tokio::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n")
        .await
        .unwrap();

    let result = sandbox
        .exec_with_timeout_and_capability(
            "ls .alan/memory",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("MEMORY.md"));
}

#[tokio::test]
async fn test_sandbox_exec_blocks_parent_dir_bypass_into_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        workspace.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
    );
    let outside_file = outside.path().join("secret.txt");
    tokio::fs::write(&outside_file, "secret").await.unwrap();
    let alias = workspace.path().join("safe.txt");
    std::os::unix::fs::symlink(&outside_file, &alias).unwrap();

    let read = sandbox.read_string(&alias).await.unwrap_err();
    assert!(read.to_string().contains("outside workspace"), "{read}");

    let write = sandbox.write(&alias, b"changed").await.unwrap_err();
    assert!(write.to_string().contains("outside workspace"), "{write}");
}

#[tokio::test]
async fn test_sandbox_blocks_hardlink_alias_into_protected_subpath() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
async fn test_sandbox_exec_blocks_mutating_variable_expansion() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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

#[tokio::test]
async fn test_sandbox_exec_blocks_python_script_file_interpreter() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
        .expect("quoted relative path pattern should stay workspace-safe");
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("./venv/bin/python"));
}

#[tokio::test]
async fn test_sandbox_exec_blocks_protected_path_in_option_assignment() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::WorkspacePathGuard,
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
    // allows writes anywhere under the workspace (and Landlock can't carve out
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

#[tokio::test]
async fn test_os_backend_wrapper_honors_memory_carve_out() {
    // The recursive wrapper inspection must honor the same carve-outs as direct
    // commands: `.alan/memory` is agent-writable even though `.alan` is protected.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let memory_dir = temp.path().join(".alan/memory");
    tokio::fs::create_dir_all(&memory_dir).await.unwrap();
    tokio::fs::write(memory_dir.join("MEMORY.md"), "# Memory\n")
        .await
        .unwrap();
    // A wrapper writing into the carved-out memory subpath must pass validation
    // (it should fail later at sandbox-exec on non-macOS, not at the protected
    // check), so assert it is NOT rejected for a protected-subpath reason.
    let result = sandbox
        .exec_with_timeout_and_capability(
            "bash -lc 'echo hi > .alan/memory/NOTES.md'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    if let Err(err) = result {
        assert!(
            !err.to_string().contains("protected subpath"),
            "memory subpath wrongly blocked: {err}"
        );
    }
}

#[test]
fn only_seatbelt_permits_autonomous_bash() {
    use crate::tools::SandboxBackendKind;
    // Seatbelt is a complete bash boundary (workspace fs + network), so wrappers
    // run and escalated bash is reviewer-eligible.
    assert!(SandboxBackendKind::Seatbelt.permits_autonomous_bash());
    // Landlock (network confinement is kernel-conditional) and the path-guard
    // fallback are treated conservatively: full shape parser, escalated bash to a
    // human.
    assert!(!SandboxBackendKind::Landlock.permits_autonomous_bash());
    assert!(!SandboxBackendKind::WorkspacePathGuard.permits_autonomous_bash());
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

#[cfg(not(target_os = "linux"))]
#[tokio::test]
async fn test_reified_backend_fails_closed_without_non_linux_runner() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "touch created-by-ambient-shell.txt",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;

    assert!(result.is_err(), "reified backend should fail closed");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("only available on Linux")
    );
    assert!(!temp.path().join("created-by-ambient-shell.txt").exists());
}

#[tokio::test]
async fn test_reified_backend_accepts_namespace_workspace_paths_for_validation() {
    let temp = TempDir::new().unwrap();
    tokio::fs::write(temp.path().join("Cargo.toml"), "[package]\n")
        .await
        .unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "cat /mnt/workspace/Cargo.toml > /dev/null",
            temp.path(),
            Some(std::time::Duration::from_millis(50)),
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;

    if let Err(err) = result {
        let message = err.to_string();
        assert!(
            !message.contains("outside workspace"),
            "reified namespace path was not translated for validation: {message}"
        );
    }
}

#[tokio::test]
async fn test_reified_backend_translates_namespace_paths_for_protected_checks() {
    let temp = TempDir::new().unwrap();
    tokio::fs::create_dir_all(temp.path().join(".git"))
        .await
        .unwrap();
    tokio::fs::write(temp.path().join(".git/config"), "[core]\n")
        .await
        .unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "cat /mnt/workspace/.git/config",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;

    assert!(
        result.is_err(),
        "protected namespace path should be blocked"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("protected subpath"),
        "reified namespace path should be translated before protected checks"
    );
}

#[test]
fn test_reified_backend_translates_host_workspace_paths_in_command_argv() {
    let temp = TempDir::new().unwrap();
    let host_manifest = temp.path().join("Cargo.toml");
    std::fs::write(&host_manifest, "[package]\n").unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let plan = sandbox
        .reified_namespace_plan_for_command(
            &format!("cat {} > /dev/null", host_manifest.display()),
            temp.path(),
            false,
        )
        .unwrap();

    assert_eq!(
        plan.argv,
        vec![
            "sh".to_string(),
            "-f".to_string(),
            "-c".to_string(),
            "cat /mnt/workspace/Cargo.toml > /dev/null".to_string()
        ]
    );
}

#[test]
fn test_reified_backend_translates_quoted_host_workspace_paths_with_spaces() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path().join("My Project");
    let docs_dir = workspace.join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    let host_doc = docs_dir.join("Project Notes.txt");
    std::fs::write(&host_doc, "notes\n").unwrap();
    let sandbox = Sandbox::with_backend(
        workspace.clone(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let plan = sandbox
        .reified_namespace_plan_for_command(
            &format!("cat '{}' > /dev/null", host_doc.display()),
            &workspace,
            false,
        )
        .unwrap();

    assert_eq!(
        plan.argv,
        vec![
            "sh".to_string(),
            "-f".to_string(),
            "-c".to_string(),
            "cat '/mnt/workspace/docs/Project Notes.txt' > /dev/null".to_string()
        ]
    );
}

#[tokio::test]
async fn test_os_backend_still_blocks_out_of_workspace_reads() {
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
        assert!(result.is_err(), "out-of-workspace read not blocked: {cmd}");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside workspace"),
            "wrong rejection for: {cmd}"
        );
    }
}

#[tokio::test]
async fn test_os_backend_rejects_shell_expansion_reads() {
    // Shell expansion defeats static path containment: `$HOME/.ssh/id_rsa` looks
    // workspace-relative to the parser but `/bin/sh -c` expands it to escape.
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
    // quoted script is opaque and its .git write / out-of-workspace read escapes.
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
        "wrapper-hidden out-of-workspace read not blocked"
    );
    assert!(
        read.unwrap_err().to_string().contains("outside workspace"),
        "wrong rejection for wrapper-hidden read"
    );
}
