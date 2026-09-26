use super::*;

#[cfg(unix)]
#[tokio::test]
async fn outer_shell_ignores_path_replacements_and_inherited_functions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let fake_shell = temp.path().join("sh");
    std::fs::write(&fake_shell, b"#!/bin/sh\nprintf hijacked").unwrap();
    std::fs::set_permissions(&fake_shell, std::fs::Permissions::from_mode(0o755)).unwrap();
    let startup = temp.path().join("startup.sh");
    std::fs::write(&startup, b"printf startup-hook").unwrap();
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
                "bash -c 'printf selected'",
            ),
        ] {
            let mut command = sandbox
                .build_confined_command(script, false, backend)
                .unwrap();
            command
                .env("PATH", search_path)
                .env("BASH_FUNC_printf%%", "() { echo inherited; }")
                .env("BASH_ENV", &startup)
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
