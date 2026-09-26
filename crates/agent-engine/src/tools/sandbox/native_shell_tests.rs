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

#[test]
fn nested_shells_reject_startup_files_before_execution() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    for script in [
        "bash -lc 'printf selected'",
        "bash -c 'set -a; printf -v BASH_ENV %s ./hook; bash -c true'",
        "bash -c 'set -a; printf -vBASH_ENV %s ./hook; bash -c true'",
        "bash -c 'set -a; BASH_ENV+=./hook; bash -c true'",
        "bash -c 'declare -n ref=BASH_ENV; set -a; ref=./hook; bash -c true'",
        "bash -c 'set -a; let BASH_ENV=1; bash -c true'",
        "bash -c 'set -a; read arr[BASH_ENV=1] < input.txt; bash -c true'",
        "bash -c 'set -a; getopts h BASH_ENV -h; bash -c true'",
        "eval 'BASH_ENV=./hook; export BASH_ENV'; bash -c 'printf selected'",
        "command eval 'BASH_ENV=./hook; export BASH_ENV'; bash -c 'printf selected'",
        "builtin source ./hook; bash -c 'printf selected'",
        ". ./hook; bash -c 'printf selected'",
        "bash -c \"eval 'BASH_ENV=./hook; export BASH_ENV'; bash -c 'printf selected'\"",
        "bash +H -c 'echo x > .git/config'",
        "env bash +H -c 'echo x > .git/config'",
        "bash +o history -c 'echo x > .git/config'",
        "sh -c \"bash +H -c 'echo x > .git/config'\"",
        "exec -l bash -c 'printf selected'",
        "exec -cl bash -c 'printf selected'",
        "exec -a -bash bash -c 'printf selected'",
        "exec -a-bash bash -c 'printf selected'",
        "exec -ca-bash bash -c 'printf selected'",
        "env --argv0=-bash bash -c 'printf selected'",
        "env -a -bash bash -c 'printf selected'",
        "bash -c \"exec -l bash -c 'printf selected'\"",
        "command exec -l bash -c 'printf selected'",
        "zsh -c 'printf selected'",
        "zsh -f -o rcs -c 'printf selected'",
        "env BASH_ENV=./hook bash -c 'printf selected'",
        "BASH_ENV=./hook bash -c 'printf selected'",
        "BASH_ENV=./hook; bash -c 'printf selected'",
        "export BASH_ENV=./hook; bash -c 'printf selected'",
        "declare -x BASH_ENV=./hook; bash -c 'printf selected'",
        "env ENV=./hook command sh -c 'printf selected'",
        "env -S 'BASH_ENV=./hook bash' -c 'printf selected'",
        "sh -c \"env BASH_ENV=./hook bash -c 'printf selected'\"",
        "bash -o posix -lc 'printf selected'",
        "env bash --login -c 'printf selected'",
        "command bash -ic 'printf selected'",
        "bash --rcfile=profile -c 'printf selected'",
        "sh -c \"bash -lc 'printf selected'\"",
    ] {
        let error = sandbox
            .validate_command_paths(script, temp.path(), PathCheckMode::ProtectedOnly, None)
            .unwrap_err();
        assert!(
            error.to_string().contains("startup files"),
            "{script}: {error}"
        );
    }
    sandbox
        .validate_command_paths(
            "bash -c 'printf selected'",
            temp.path(),
            PathCheckMode::ProtectedOnly,
            None,
        )
        .unwrap();
}

#[test]
fn startup_variable_text_remains_ordinary_command_data() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    sandbox
        .validate_command_paths(
            "env printf 'BASH_ENV=./hook'",
            temp.path(),
            PathCheckMode::ProtectedOnly,
            None,
        )
        .unwrap();
}

#[test]
fn zsh_requires_disabled_startup_files() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::new(temp.path().to_path_buf());
    for script in ["zsh -fc 'printf selected'", "zsh -f -c 'printf selected'"] {
        sandbox
            .validate_command_paths(script, temp.path(), PathCheckMode::ProtectedOnly, None)
            .unwrap();
    }
}

#[test]
fn environment_help_does_not_start_a_nested_shell() {
    for flag in ["--help", "--version"] {
        assert!(
            shell_wrapper_inline_script(&["env".into(), flag.into()])
                .unwrap()
                .is_none()
        );
    }
}
