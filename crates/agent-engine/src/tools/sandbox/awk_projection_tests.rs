#![cfg(target_os = "macos")]

use super::super::*;
use crate::tools::{SandboxBackendKind, reified_namespace::ReifiedMountAccess};
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn sandbox_projects_awk_script_and_input_paths_before_execution() {
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
            "awk -f /mnt/project/script.awk /mnt/project/input.tsv > /mnt/project/output.tsv",
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
            "env -u HOME awk -v root=/mnt/project 'BEGIN { print root }' > /mnt/project/data.txt",
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
            "awk '{ print root }' root=/mnt/project /mnt/project/input.tsv",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "/mnt/project\n");
}
