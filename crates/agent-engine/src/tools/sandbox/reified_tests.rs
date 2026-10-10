use super::super::*;
use tempfile::TempDir;

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_reified_backend_carries_current_path_and_rejects_unknown_entries() {
    const CASE: &str = "ALAN_TEST_REIFIED_PATH_CASE";
    if let Ok(case) = std::env::var(CASE) {
        let project = TempDir::new().unwrap();
        let sandbox = Sandbox::with_backend(
            project.path().to_path_buf(),
            crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
        );
        if case == "supported" {
            let result = sandbox.exec("printenv PATH", project.path()).await.unwrap();
            assert_eq!(result.exit_code, 0, "{}", result.stderr);
            assert_eq!(
                result.stdout,
                format!("{}\n", std::env::var("PATH").unwrap())
            );
        } else {
            assert_eq!(case, "unsupported");
            let error = sandbox
                .exec("printf effect > marker", project.path())
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("linux command environment unavailable"),
                "{error:#}"
            );
            assert!(!project.path().join("marker").exists());
        }
        return;
    }
    if !crate::tools::sandbox_backend::probe_linux_reification().is_selectable() {
        eprintln!("skipping native current-PATH adapter test: namespace unavailable");
        return;
    }
    let runtime = TempDir::new().unwrap();
    let name = concat!(
        module_path!(),
        "::test_reified_backend_carries_current_path_and_rejects_unknown_entries"
    );
    let (_, name) = name.split_once("::").unwrap();
    for (case, path) in [
        ("supported", "/bin:/usr/bin:/sbin:/usr/sbin".to_string()),
        (
            "unsupported",
            format!("/bin:{}/missing:/usr/bin", runtime.path().display()),
        ),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env(CASE, case)
            .env("PATH", path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{case}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
}

#[tokio::test]
async fn test_reified_backend_keeps_shape_parser_for_opaque_writers() {
    // Linux reified namespace still bind-mounts the writable host_mount as a whole,
    // so protected subpath integrity depends on the full parser until those
    // subpaths are carved out of the namespace.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );
    let result = sandbox
        .exec_with_timeout_and_capability(
            "python -c 'open(\".git/config\",\"w\").write(\"x\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;
    assert!(
        result.is_err(),
        "opaque writer not rejected under Linux reified namespace"
    );
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
async fn test_reified_backend_accepts_native_host_paths_for_validation() {
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
            &format!(
                "cat '{}' > /dev/null",
                temp.path().join("Cargo.toml").display()
            ),
            temp.path(),
            Some(std::time::Duration::from_millis(50)),
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;

    if let Err(err) = result {
        let message = err.to_string();
        assert!(
            !message.contains("outside host_mount"),
            "native Host path was rejected during validation: {message}"
        );
    }
}

#[tokio::test]
async fn test_reified_backend_checks_protected_native_host_paths() {
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
            &format!("cat '{}'", temp.path().join(".git/config").display()),
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;

    assert!(
        result.is_err(),
        "protected native Host path should be blocked"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("protected subpath"),
        "native Host path should be checked before execution"
    );
}

#[test]
fn test_reified_backend_preserves_native_paths_and_script_body() {
    let temp = TempDir::new().unwrap();
    let docs_dir = temp.path().join("My Project");
    let pkg_dir = temp.path().join("pkg");
    let tests_dir = temp.path().join("tests");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    let host_doc = docs_dir.join("Project Notes.txt");
    let host_copy = docs_dir.join("Copy Notes.txt");
    std::fs::write(&host_doc, "notes\n").unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );
    let python_path = format!("{}:{}", pkg_dir.display(), tests_dir.display());
    let script = format!(
        "bash -c \"FOO='{}' PYTHONPATH='{}' cp '{}' '{}'\"",
        host_doc.display(),
        python_path,
        host_doc.display(),
        host_copy.display()
    );

    let plan = sandbox
        .reified_namespace_plan_for_command(&script, temp.path(), false)
        .unwrap();

    assert_eq!(&plan.argv[..4], &["/bin/sh", "-p", "-f", "-c"]);
    assert_eq!(plan.argv.last(), Some(&script));
    assert_eq!(plan.cwd, temp.path());
    assert_eq!(plan.declared_host_mounts[0].namespace_path, plan.cwd);
}

#[tokio::test]
async fn test_reified_backend_exec_validates_quoted_host_host_mount_paths_with_spaces() {
    let temp = TempDir::new().unwrap();
    let host_mount = temp.path().join("My Project");
    let docs_dir = host_mount.join("docs");
    tokio::fs::create_dir_all(&docs_dir).await.unwrap();
    let host_doc = docs_dir.join("Project Notes.txt");
    tokio::fs::write(&host_doc, "notes\n").await.unwrap();
    let sandbox = Sandbox::with_backend(
        host_mount.clone(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!("cat '{}' > /dev/null", host_doc.display()),
            &host_mount,
            Some(std::time::Duration::from_millis(50)),
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;

    if let Err(err) = result {
        let message = err.to_string();
        assert!(
            !message.contains("outside host_mount"),
            "quoted host host_mount path with spaces should not be truncated during validation: {message}"
        );
    }
}
