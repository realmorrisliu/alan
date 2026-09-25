use super::*;

#[tokio::test]
async fn test_os_backend_still_blocks_out_of_host_mount_reads() {
    // Seatbelt denies writes/network but permits reads, so the parser must still
    // contain reads that could expose host secrets to tool output.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for command in [
        "cat ~/.ssh/id_rsa",
        "cat /etc/passwd",
        "bash -lc 'cat /etc/passwd'",
    ] {
        let result = sandbox
            .exec_with_timeout_and_capability(
                command,
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Read),
            )
            .await;
        assert!(
            result.is_err(),
            "out-of-host_mount read not blocked: {command}"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside host_mount"),
            "wrong rejection for: {command}"
        );
    }
}

#[tokio::test]
async fn seatbelt_rejects_makefile_commands_with_uninspectable_path_input() {
    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("host-only-marker.txt");
    std::fs::write(&outside_file, "host-only-marker\n").unwrap();
    let makefile = format!("all:\n\tcat {}\n", outside_file.display());
    std::fs::write(mount.path().join("Makefile"), &makefile).unwrap();
    std::fs::write(mount.path().join("custom.mk"), makefile).unwrap();

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for command in [
        "make",
        "make -f custom.mk",
        "gmake -f custom.mk",
        "bmake -f custom.mk",
    ] {
        let error = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err("makefile recipes can read paths hidden from ProtectedOnly validation");

        assert!(
            error
                .to_string()
                .contains("uninspectable path-bearing input"),
            "{command}: {error}"
        );
    }
}

#[tokio::test]
async fn absolute_path_executable_on_host_path_is_not_a_project_file_operand() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("inside.txt"), "inside mount")
        .await
        .unwrap();
    let search_path = std::env::var_os("PATH").expect("test PATH");
    let executable = std::env::split_paths(&search_path)
        .map(|directory| directory.join("cat"))
        .find(|path| {
            std::fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .expect("cat executable on PATH");

    let command = format!("'{}' inside.txt", executable.display());
    assert_eq!(sandbox.bash_path_guard_reason(&command, temp.path()), None);
    let result = sandbox
        .exec_with_timeout_and_capability(
            &command,
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "inside mount");
    let path_as_operand_then_command = format!(
        "cat '{}' ; '{}' inside.txt",
        executable.display(),
        executable.display()
    );
    assert!(
        sandbox
            .bash_path_guard_reason(&path_as_operand_then_command, temp.path())
            .is_some_and(|reason| reason.contains("outside host_mount"))
    );
    assert!(
        sandbox
            .bash_path_guard_reason("cat /etc/hosts", temp.path())
            .is_some_and(|reason| reason.contains("outside host_mount"))
    );
}
