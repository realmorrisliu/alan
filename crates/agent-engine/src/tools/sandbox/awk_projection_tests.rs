#![cfg(target_os = "macos")]

use super::super::*;
use crate::tools::{SandboxBackendKind, reified_namespace::ReifiedMountAccess};
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn sandbox_runs_awk_with_native_paths_and_preserves_path_data() {
    let mount = TempDir::new().unwrap();
    let script_path = mount.path().join("script.awk");
    let input_path = mount.path().join("input.tsv");
    let output_path = mount.path().join("output.tsv");
    std::fs::write(&script_path, "{ print $0 }\n").unwrap();
    std::fs::write(&input_path, "payload\n").unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!(
                "awk -f '{}' '{}' > '{}'",
                script_path.display(),
                input_path.display(),
                output_path.display()
            ),
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(std::fs::read_to_string(&output_path).unwrap(), "payload\n");

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!(
                "env -u HOME awk -v root=/mnt/project 'BEGIN {{ print root }}' > '{}'",
                mount.path().join("data.txt").display()
            ),
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
            &format!(
                "awk '{{ print root }}' root=/mnt/project '{}'",
                input_path.display()
            ),
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "/mnt/project\n");
}
