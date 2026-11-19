use super::super::*;
use tempfile::TempDir;

#[tokio::test]
async fn test_landlock_relaxes_static_shape_parser_while_preserving_protected_guards() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Landlock,
    );
    sandbox
        .validate_command_paths(
            "python -c 'print(1)'",
            temp.path(),
            sandbox.path_check_mode(),
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .unwrap();
    let protected = sandbox
        .exec_with_timeout_and_capability(
            "bash -lc 'echo x > .git/config'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap_err();
    assert!(protected.to_string().contains("landlock"));
    assert!(protected.to_string().contains("protected subpath"));

    let fallback = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let fallback_err = fallback
        .exec_with_timeout_and_capability(
            "python -c 'open(\".git/config\",\"w\").write(\"x\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap_err();
    assert!(fallback_err.to_string().contains("host_mount_path_guard"));
}

#[tokio::test]
async fn test_os_backend_and_explicit_trust_allow_shell_expansions_and_attribute_real_backend() {
    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("alpha.rs"), "fn a() {}\n").unwrap();
    let path_guard = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    for cmd in [
        "cat $HOME/.ssh/id_rsa",
        "ls $HOME",
        "cat $(echo /etc/passwd)",
        "echo `pwd`",
        "echo *.rs",
        "echo {a,b}",
    ] {
        let result = path_guard
            .exec_with_timeout_and_capability(
                cmd,
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Read),
            )
            .await;
        assert!(result.is_err(), "path_guard fallback must reject: {cmd}");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("host_mount_path_guard"), "{cmd}: {msg}");
        assert!(msg.contains("expansion"), "{cmd}: {msg}");
    }

    let trusted = path_guard.with_explicit_trust(true);
    let glob_out = trusted
        .exec_with_timeout_and_capability(
            "VAR=ok; echo \"$VAR\" $(printf cmd) `printf bt` *.rs {a,b}",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(glob_out.exit_code, 0, "{}", glob_out.stderr);
    assert_eq!(glob_out.stdout.trim(), "ok cmd bt alpha.rs a b");

    let seatbelt = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let err = seatbelt
        .exec_with_timeout_and_capability(
            "bash -lc 'echo x > .git/config'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("Sandbox backend seatbelt"), "{err}");
    assert!(!err.contains("host_mount_path_guard"), "{err}");
    assert!(err.contains("protected subpath .git"), "{err}");

    if cfg!(target_os = "macos") {
        let out = seatbelt
            .exec_with_timeout_and_capability(
                "VAR=ok; echo \"$VAR\" $(printf cmd) `printf bt` *.rs {a,b}",
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Read),
            )
            .await
            .unwrap();
        assert_eq!(out.exit_code, 0, "{}", out.stderr);
        assert_eq!(out.stdout.trim(), "ok cmd bt alpha.rs a b");
    }
}

#[tokio::test]
async fn test_os_backend_still_blocks_out_of_host_mount_reads() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for cmd in [
        "cat ~/.ssh/id_rsa",
        "cat /etc/passwd",
        "bash -c 'cat /etc/passwd'",
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
async fn test_os_backend_unwraps_transparent_wrappers_for_protected_and_reads() {
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );

    let protected = sandbox
        .exec_with_timeout_and_capability(
            "env bash -c 'echo x > .git/config'",
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
            "command bash -c 'cat ~/.ssh/id_rsa'",
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
