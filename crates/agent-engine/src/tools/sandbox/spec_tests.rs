use super::super::*;
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
    let host_mount = PathBuf::from("/host_mount");
    let spec = SandboxSpec::seed(host_mount.clone());

    assert_eq!(spec.writable_roots, vec![host_mount]);
    assert_eq!(
        spec.read_denylist,
        SandboxSpec::default_sensitive_read_denylist()
    );
    assert_eq!(spec.network, NetworkPosture::Deny);
}

#[test]
fn sandbox_spec_excludes_exact_writable_root_read_denies() {
    let home = Path::new("/Users/alice");
    let host_mount = home.join(".alan");
    let sandbox = Sandbox::from_spec(SandboxSpec {
        host_mounts: Vec::new(),
        readable_roots: vec![host_mount.clone()],
        writable_roots: vec![host_mount.clone()],
        read_denylist: SandboxSpec::sensitive_read_denylist_for_home(home),
        network: NetworkPosture::Deny,
    });

    assert!(!sandbox.spec.read_denylist.contains(&host_mount));
    assert!(sandbox.spec.read_denylist.contains(&home.join(".ssh")));
    assert!(sandbox.spec.read_denylist.contains(&home.join(".alan-dev")));
}

#[test]
fn sandbox_spec_preserves_parent_read_denies_for_nested_writable_roots() {
    let home = Path::new("/Users/alice");
    let sensitive_parent = home.join(".ssh");
    let host_mount = sensitive_parent.join("project");
    let sandbox = Sandbox::from_spec(SandboxSpec {
        host_mounts: Vec::new(),
        readable_roots: vec![host_mount.clone()],
        writable_roots: vec![host_mount],
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
    let host_mount = TempDir::new().unwrap();
    let approved = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec(SandboxSpec {
        host_mounts: Vec::new(),
        readable_roots: vec![
            host_mount.path().to_path_buf(),
            approved.path().to_path_buf(),
        ],
        writable_roots: vec![
            host_mount.path().to_path_buf(),
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
    assert!(sandbox.is_writable(&host_mount.path().join("a.txt")));
    assert!(sandbox.is_writable(&approved.path().join("b.txt")));
    assert!(!sandbox.is_writable(&outside.path().join("c.txt")));

    let outside_read = sandbox.read(&outside.path().join("secret.txt")).await;
    assert!(outside_read.is_err());
    assert!(
        outside_read
            .unwrap_err()
            .to_string()
            .contains("outside host_mount")
    );
}

#[tokio::test]
async fn sandbox_exec_accepts_cwd_under_secondary_writable_root() {
    let host_mount = TempDir::new().unwrap();
    let approved = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![
                host_mount.path().to_path_buf(),
                approved.path().to_path_buf(),
            ],
            writable_roots: vec![
                host_mount.path().to_path_buf(),
                approved.path().to_path_buf(),
            ],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::HostMountPathGuard,
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
            .contains("outside host_mount")
    );
}

#[tokio::test]
async fn sandbox_exec_read_capability_allows_read_only_mount_paths() {
    let writable = TempDir::new().unwrap();
    let read_only = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![
                writable.path().to_path_buf(),
                read_only.path().to_path_buf(),
            ],
            writable_roots: vec![writable.path().to_path_buf()],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let document = read_only.path().join("notes.txt");
    tokio::fs::write(&document, "read-only mount\n")
        .await
        .unwrap();

    let relative = sandbox
        .exec_with_timeout_and_capability(
            "cat ./notes.txt",
            read_only.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(relative.stdout, "read-only mount\n");

    let absolute = sandbox
        .exec_with_timeout_and_capability(
            &format!("cat '{}'", document.display()),
            writable.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(absolute.stdout, "read-only mount\n");
}

#[tokio::test]
async fn sandbox_exec_write_capability_rejects_read_only_mount_paths() {
    let writable = TempDir::new().unwrap();
    let read_only = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![
                writable.path().to_path_buf(),
                read_only.path().to_path_buf(),
            ],
            writable_roots: vec![writable.path().to_path_buf()],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let target = read_only.path().join("created.txt");

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!("touch '{}'", target.display()),
            writable.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("outside host_mount")
    );
    assert!(!target.exists());
}

#[tokio::test]
async fn sandbox_protects_reserved_subpaths_under_secondary_writable_root() {
    let host_mount = TempDir::new().unwrap();
    let approved = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![
                host_mount.path().to_path_buf(),
                approved.path().to_path_buf(),
            ],
            writable_roots: vec![
                host_mount.path().to_path_buf(),
                approved.path().to_path_buf(),
            ],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::HostMountPathGuard,
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
async fn test_sandbox_blocks_outside_host_mount() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );

    // Try to read outside host_mount
    let outside_path = PathBuf::from("/etc/passwd");
    let result = sandbox.read(&outside_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn sandbox_spec_writable_roots_allow_host_absolute_paths() {
    let host_mount = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![host_mount.path().to_path_buf(), host.path().to_path_buf()],
            writable_roots: vec![host_mount.path().to_path_buf(), host.path().to_path_buf()],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let target = host.path().join("created.txt");

    assert!(sandbox.is_writable(host.path()));
    let result = sandbox
        .exec(&format!("touch {}", target.display()), host_mount.path())
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
    assert!(target.exists());
}

#[tokio::test]
async fn sandbox_spec_writable_roots_still_block_host_protected_subpaths() {
    let host_mount = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![host_mount.path().to_path_buf(), host.path().to_path_buf()],
            writable_roots: vec![host_mount.path().to_path_buf(), host.path().to_path_buf()],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::HostMountPathGuard,
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
    let host_mount = TempDir::new().unwrap();
    let host = TempDir::new().unwrap();
    let protected_root = host.path().join(".git");
    tokio::fs::create_dir_all(&protected_root).await.unwrap();
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![host_mount.path().to_path_buf(), protected_root.clone()],
            writable_roots: vec![host_mount.path().to_path_buf(), protected_root.clone()],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        },
        crate::tools::SandboxBackendKind::HostMountPathGuard,
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
