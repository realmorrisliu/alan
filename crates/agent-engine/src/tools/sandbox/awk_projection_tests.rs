#![cfg(target_os = "macos")]

use super::super::*;
use crate::tools::{SandboxBackendKind, reified_namespace::ReifiedMountAccess};
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn sandbox_preserves_native_awk_operands_and_literal_data() {
    let mount = TempDir::new().unwrap();
    std::fs::write(mount.path().join("script.awk"), "{ print $0 }\n").unwrap();
    std::fs::write(mount.path().join("input.tsv"), "payload\n").unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    let result = sandbox
        .exec_with_timeout_and_capability(
            "awk '{ print $0 }' ./input.tsv > ./output.tsv",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(
        std::fs::read_to_string(mount.path().join("output.tsv")).unwrap(),
        "payload\n"
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "env -u HOME awk -v root=/mnt/project 'BEGIN { print root }' > ./data.txt",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(
        std::fs::read_to_string(mount.path().join("data.txt")).unwrap(),
        "/mnt/project\n"
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            "awk '{ print root }' root=/mnt/project ./input.tsv",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "/mnt/project\n");
}

#[tokio::test]
async fn sandbox_rejects_opaque_paths_consumed_from_literal_data() {
    let mount = TempDir::new().unwrap();
    std::fs::write(
        mount.path().join("script.awk"),
        "BEGIN { system(\"true\") }",
    )
    .unwrap();
    let sandbox = Sandbox::with_backend(mount.path().to_path_buf(), SandboxBackendKind::Seatbelt);
    for (command, reason) in [
        ("awk -f ./script.awk", "opaque AWK script files"),
        (
            "gawk --include=./script.awk 'BEGIN {}'",
            "opaque AWK script files",
        ),
        (
            "awk -v p=/etc/passwd 'BEGIN { getline x < p; print x }'",
            "AWK getline file paths",
        ),
        (
            "awk -v p=/etc/passwd 'BEGIN { getline $10 < p; print $10 }'",
            "AWK getline file paths",
        ),
        (
            "awk 'BEGIN { getline x < \"\\057etc/passwd\"; print x }'",
            "AWK getline file paths",
        ),
        (
            "awk 'BEGIN { getline x < \"/etc/passwd\"; print x }'",
            "outside host_mount",
        ),
        (
            "awk 'BEGIN { getline x < \"../../../../../../etc/passwd\"; print x }'",
            "outside host_mount",
        ),
        (
            "awk -v p=.git/config 'BEGIN { print 1 > p }'",
            "uninspectable file or command I/O",
        ),
        (
            "awk 'BEGIN { system(\"true\") }'",
            "uninspectable file or command I/O",
        ),
        (
            "awk 'BEGIN { ARGV[1] = \"/etc/passwd\" }'",
            "uninspectable file or command I/O",
        ),
        (
            "printf '/etc/passwd\\n' | xargs cat",
            "opaque command dispatchers",
        ),
        (
            "printf 'print(open(\"/etc/passwd\").read())' | python3",
            "opaque script interpreters",
        ),
    ] {
        let error = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err(command);
        assert!(error.to_string().contains(reason), "{command}: {error}");
    }
}

#[tokio::test]
async fn sandbox_allows_inspectable_awk_io_and_regexes() {
    let mount = TempDir::new().unwrap();
    std::fs::write(mount.path().join("input.tsv"), "payload\n").unwrap();
    let sandbox = Sandbox::with_backend(mount.path().to_path_buf(), SandboxBackendKind::Seatbelt);
    for command in [
        "awk 'BEGIN { getline x < \"input.tsv\"; print x }'",
        "awk '/payload/ { print $0 }' ./input.tsv",
        "awk '{ if (length($0) > 0) print $0 }' ./input.tsv",
    ] {
        let result = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Read),
            )
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0, "{command}: {}", result.stderr);
        assert_eq!(result.stdout, "payload\n");
    }
}
