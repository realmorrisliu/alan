use super::*;

#[tokio::test]
async fn descriptor_redirection_does_not_invent_a_numeric_executable() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    for (script, stdout, stderr) in [
        ("printf error >&2", "", "error"),
        ("printf output 2>&1", "output", ""),
        ("printf error 1>&2; printf output", "output", "error"),
        ("printf output 2>&-", "output", ""),
    ] {
        let result = sandbox
            .exec_with_timeout_and_capability(
                script,
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Write),
            )
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0, "{script}");
        assert_eq!(result.stdout, stdout, "{script}");
        assert_eq!(result.stderr, stderr, "{script}");
    }
}

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

#[cfg(unix)]
#[tokio::test]
async fn outer_shell_ignores_path_replacements_and_inherited_functions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let fake_shell = temp.path().join("sh");
    std::fs::write(&fake_shell, b"#!/bin/sh\nprintf hijacked").unwrap();
    std::fs::set_permissions(&fake_shell, std::fs::Permissions::from_mode(0o755)).unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    let mut backends = vec![crate::tools::SandboxBackendKind::HostMountPathGuard];
    if cfg!(target_os = "macos") {
        backends.push(crate::tools::SandboxBackendKind::Seatbelt);
    }
    for backend in backends {
        for (search_path, script) in [
            (temp.path().as_os_str(), "printf selected"),
            (
                std::ffi::OsStr::new("/usr/bin:/bin"),
                "sh -c 'printf selected'",
            ),
        ] {
            let mut command = sandbox
                .build_confined_command(script, false, backend)
                .unwrap();
            command
                .env("PATH", search_path)
                .env("BASH_FUNC_printf%%", "() { echo inherited; }")
                .current_dir(temp.path())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let result = super::super::command_process::output(command, None)
                .await
                .unwrap();
            assert!(result.status.success(), "{backend:?}: {:?}", result.stderr);
            assert_eq!(result.stdout, b"selected", "{backend:?}");
        }
    }
}
