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
    // `/bin/sh` is bash on macOS (expands `{a,b}`) and POSIX dash on Ubuntu (prints literal `{a,b}`).
    assert!(
        matches!(
            glob_out.stdout.trim(),
            "ok cmd bt alpha.rs a b" | "ok cmd bt alpha.rs {a,b}"
        ),
        "unexpected stdout: {}",
        glob_out.stdout.trim()
    );

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

#[tokio::test]
async fn confined_trusted_retains_os_sandbox_and_never_silently_elevates_network() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("outside.txt");

    // HostMountPathGuard refuses implicit Network elevation in ConfinedTrusted.
    let guard_trusted = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    )
    .with_trust_posture(crate::policy::TrustPosture::ConfinedTrusted);
    let net_err = guard_trusted
        .exec_with_timeout_and_capability(
            "curl https://example.com",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Network),
        )
        .await
        .unwrap_err();
    assert!(
        net_err.to_string().contains("network access is confined"),
        "unexpected error: {net_err}"
    );

    if cfg!(target_os = "macos") {
        let seatbelt_trusted = Sandbox::with_backend(
            temp.path().to_path_buf(),
            crate::tools::SandboxBackendKind::Seatbelt,
        )
        .with_trust_posture(crate::policy::TrustPosture::ConfinedTrusted);
        // Out-of-mount writes stay blocked by protected checks / OS sandbox.
        let escaped_write = seatbelt_trusted
            .exec_with_timeout_and_capability(
                &format!("touch {}", outside_file.display()),
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Write),
            )
            .await;
        assert!(escaped_write.is_err() || !outside_file.exists());

        // Seatbelt keeps (deny network*) even for Network-classified commands under --trust!
        let local_net = seatbelt_trusted
            .exec_with_timeout_and_capability(
                "/usr/bin/python3 -c 'import socket; s=socket.socket(); s.settimeout(0.2); s.connect((\"127.0.0.1\", 1))'",
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Network),
            )
            .await;
        if let Ok(out) = local_net {
            assert_ne!(
                out.exit_code, 0,
                "Seatbelt must keep network denied in ConfinedTrusted; stdout={} stderr={}",
                out.stdout, out.stderr
            );
        }
    }
}

#[tokio::test]
async fn host_account_trusted_runs_unsandboxed_shell_while_preserving_explicit_file_handle_bounds()
{
    let mounted = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("outside_secret.txt");
    std::fs::write(&outside_file, "host-secret").unwrap();

    let sandbox = Sandbox::new(mounted.path().to_path_buf())
        .with_trust_posture(crate::policy::TrustPosture::HostAccountTrusted);

    // Unsandboxed shell executes from host-account authority without Seatbelt/path-guard.
    let res = sandbox
        .exec_with_timeout_and_capability(
            &format!("cat {}", outside_file.display()),
            mounted.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(res.exit_code, 0, "{}", res.stderr);
    assert_eq!(res.stdout, "host-secret");

    // Explicit file-tool / Host-Mount handle bounds remain enforced (no fabricated `/` mount).
    let file_err = sandbox.read(&outside_file).await.unwrap_err();
    assert!(
        file_err.to_string().contains("outside host_mount roots"),
        "unexpected file bound error: {file_err}"
    );
}

#[tokio::test]
async fn protected_only_allows_inline_interpreter_code_slash_literals_without_weakening_guard() {
    let temp = TempDir::new().unwrap();
    let mounted_file = temp.path().join("sample.txt");
    std::fs::write(&mounted_file, "hello /a/b\n").unwrap();

    // Seatbelt / ProtectedOnly must not reject `/a` inside inline code or regex patterns.
    let seatbelt = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    seatbelt
        .validate_command_paths(
            "python3 -c 'html = \"<a href=\\\"/a\\\">x</a>\"; print(html)'",
            temp.path(),
            crate::tools::sandbox::PathCheckMode::ProtectedOnly,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .unwrap();
    seatbelt
        .validate_command_paths(
            "node -e 'console.log(\"/a/b\")'",
            temp.path(),
            crate::tools::sandbox::PathCheckMode::ProtectedOnly,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .unwrap();
    seatbelt
        .validate_command_paths(
            "grep -E '/a/b' sample.txt",
            temp.path(),
            crate::tools::sandbox::PathCheckMode::ProtectedOnly,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .unwrap();

    // Additional path operand outside host_mount after inline code must STILL be rejected!
    let extra_arg = seatbelt.validate_command_paths(
        "python3 -c 'print(1)' /etc/passwd",
        temp.path(),
        crate::tools::sandbox::PathCheckMode::ProtectedOnly,
        Some(alan_agent_protocol::ToolCapability::Read),
    );
    assert!(
        extra_arg.is_err(),
        "extra path operand after python3 -c must still be blocked: {extra_arg:?}"
    );

    // HostMountPathGuard (Full) must keep rejecting opaque inline scripts & literals.
    let guard = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    let full_err = guard
        .exec_with_timeout_and_capability(
            "python3 -c 'print(\"/a\")'",
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await;
    assert!(full_err.is_err());

    if cfg!(target_os = "macos") {
        let out = seatbelt
            .exec_with_timeout_and_capability(
                "python3 -c 'print(\"<a href=/a>ok</a>\")'",
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Read),
            )
            .await
            .unwrap();
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert_eq!(out.stdout.trim(), "<a href=/a>ok</a>");
    }
}
