use super::super::*;
use tempfile::TempDir;

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
async fn test_reified_backend_accepts_namespace_host_mount_paths_for_validation() {
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
            "cat /mnt/source/Cargo.toml > /dev/null",
            temp.path(),
            Some(std::time::Duration::from_millis(50)),
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;

    if let Err(err) = result {
        let message = err.to_string();
        assert!(
            !message.contains("outside host_mount"),
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
            "cat /mnt/source/.git/config",
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
fn test_reified_backend_translates_host_host_mount_paths_in_command_argv() {
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
            "cat /mnt/source/Cargo.toml > /dev/null".to_string()
        ]
    );
}

#[test]
fn test_reified_backend_translates_embedded_host_paths_in_wrapper_script() {
    let temp = TempDir::new().unwrap();
    let host_manifest = temp.path().join("Cargo.toml");
    let host_copy = temp.path().join("copy.toml");
    std::fs::write(&host_manifest, "[package]\n").unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let plan = sandbox
        .reified_namespace_plan_for_command(
            &format!(
                "bash -lc 'cp {} {}'",
                host_manifest.display(),
                host_copy.display()
            ),
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
            "bash -lc 'cp /mnt/source/Cargo.toml /mnt/source/copy.toml'".to_string()
        ]
    );
}

#[test]
fn test_reified_backend_translates_embedded_host_paths_with_intervening_flag() {
    let temp = TempDir::new().unwrap();
    let host_manifest = temp.path().join("Cargo.toml");
    let host_output_dir = temp.path().join("out");
    std::fs::write(&host_manifest, "[package]\n").unwrap();
    std::fs::create_dir_all(&host_output_dir).unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let plan = sandbox
        .reified_namespace_plan_for_command(
            &format!(
                "bash -lc 'cp {} -t {}'",
                host_manifest.display(),
                host_output_dir.display()
            ),
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
            "bash -lc 'cp /mnt/source/Cargo.toml -t /mnt/source/out'".to_string()
        ]
    );
}

#[test]
fn test_reified_backend_translates_quoted_spaced_wrapper_operand_before_second_path() {
    let temp = TempDir::new().unwrap();
    let docs_dir = temp.path().join("My Project");
    let host_doc = docs_dir.join("Project Notes.txt");
    let host_output_dir = temp.path().join("out");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::create_dir_all(&host_output_dir).unwrap();
    std::fs::write(&host_doc, "notes\n").unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let plan = sandbox
        .reified_namespace_plan_for_command(
            &format!(
                "bash -lc \"cp '{}' {}\"",
                host_doc.display(),
                host_output_dir.display()
            ),
            temp.path(),
            false,
        )
        .unwrap();

    let command = &plan.argv[3];
    assert!(
        command.contains("/mnt/source/My Project/Project Notes.txt"),
        "spaced source path was not translated: {command}"
    );
    assert!(
        command.contains("/mnt/source/out"),
        "second host path was not translated: {command}"
    );
    assert!(
        !command.contains(&host_doc.display().to_string()),
        "host source path leaked into namespace command: {command}"
    );
    assert!(
        !command.contains(&host_output_dir.display().to_string()),
        "host output path leaked into namespace command: {command}"
    );
}

#[test]
fn test_reified_backend_preserves_assignment_words_for_quoted_spaced_paths() {
    let temp = TempDir::new().unwrap();
    let docs_dir = temp.path().join("My Project");
    let host_doc = docs_dir.join("Project Notes.txt");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(&host_doc, "notes\n").unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let plan = sandbox
        .reified_namespace_plan_for_command(
            &format!("bash -lc \"FOO='{}' env\"", host_doc.display()),
            temp.path(),
            false,
        )
        .unwrap();

    let command = &plan.argv[3];
    assert!(
        command.contains("FOO="),
        "translated assignment no longer has assignment syntax: {command}"
    );
    assert!(
        command.contains("/mnt/source/My Project/Project Notes.txt"),
        "assignment value path was not translated: {command}"
    );
    assert!(
        !command.contains("'FOO=/mnt/source"),
        "entire assignment word was quoted instead of only its value: {command}"
    );
    assert!(
        !command.contains(&host_doc.display().to_string()),
        "host assignment value leaked into namespace command: {command}"
    );
}

#[test]
fn test_reified_backend_translates_colon_separated_assignment_paths() {
    let temp = TempDir::new().unwrap();
    let pkg_dir = temp.path().join("pkg");
    let tests_dir = temp.path().join("tests");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let host_path_list = format!("{}:{}", pkg_dir.display(), tests_dir.display());
    let plan = sandbox
        .reified_namespace_plan_for_command(
            &format!("bash -lc 'PYTHONPATH={host_path_list} python -c \"import sys\"'"),
            temp.path(),
            false,
        )
        .unwrap();

    let command = &plan.argv[3];
    assert!(
        command.contains("PYTHONPATH=/mnt/source/pkg:/mnt/source/tests"),
        "colon-separated assignment paths were not fully translated: {command}"
    );
    assert!(
        !command.contains(&pkg_dir.display().to_string()),
        "host pkg path leaked into namespace command: {command}"
    );
    assert!(
        !command.contains(&tests_dir.display().to_string()),
        "host tests path leaked into namespace command: {command}"
    );
}

#[test]
fn test_reified_backend_translates_quoted_host_host_mount_paths_with_spaces() {
    let temp = TempDir::new().unwrap();
    let host_mount = temp.path().join("My Project");
    let docs_dir = host_mount.join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    let host_doc = docs_dir.join("Project Notes.txt");
    std::fs::write(&host_doc, "notes\n").unwrap();
    let sandbox = Sandbox::with_backend(
        host_mount.clone(),
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    );

    let plan = sandbox
        .reified_namespace_plan_for_command(
            &format!("cat '{}' > /dev/null", host_doc.display()),
            &host_mount,
            false,
        )
        .unwrap();

    assert_eq!(
        plan.argv,
        vec![
            "sh".to_string(),
            "-f".to_string(),
            "-c".to_string(),
            "cat '/mnt/source/docs/Project Notes.txt' > /dev/null".to_string()
        ]
    );
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
