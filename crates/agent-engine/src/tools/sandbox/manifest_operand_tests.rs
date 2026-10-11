use super::super::*;
use alan_agent_protocol::ToolCapability;
use tempfile::TempDir;

fn fixture(backend: crate::tools::SandboxBackendKind) -> (TempDir, Sandbox) {
    let root = TempDir::new().unwrap();
    for directory in ["scratch", "dependency", "outside"] {
        std::fs::create_dir(root.path().join(directory)).unwrap();
        std::fs::write(root.path().join(directory).join("Cargo.toml"), b"fixture").unwrap();
    }
    let sandbox = Sandbox::from_spec_with_backend(
        SandboxSpec::from_host_mounts(&[
            SandboxHostMount {
                namespace_path: "/mnt/scratch".into(),
                host_path: root.path().join("scratch"),
                access: crate::tools::ReifiedMountAccess::ReadWrite,
            },
            SandboxHostMount {
                namespace_path: "/mnt/dependency".into(),
                host_path: root.path().join("dependency"),
                access: crate::tools::ReifiedMountAccess::ReadOnly,
            },
        ]),
        backend,
    );
    (root, sandbox)
}

#[test]
fn enforced_cargo_manifest_operand_reads_explicit_read_only_grant() {
    for backend in [
        crate::tools::SandboxBackendKind::Seatbelt,
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
    ] {
        let (root, sandbox) = fixture(backend);
        let manifest = root.path().join("dependency/Cargo.toml");
        for mode in [PathCheckMode::Full, PathCheckMode::ProtectedOnly] {
            for command in ["build", "check", "test", "run"] {
                for operand in [
                    format!("--manifest-path '{}'", manifest.display()),
                    format!("--manifest-path='{}'", manifest.display()),
                    "--manifest-path ../dependency/Cargo.toml".into(),
                ] {
                    let script = format!(
                        "cargo test --target-dir target && cargo {command} {operand} --target-dir target/project"
                    );
                    sandbox
                        .validate_command_paths(
                            &script,
                            &root.path().join("scratch"),
                            mode,
                            Some(ToolCapability::Write),
                        )
                        .unwrap_or_else(|error| panic!("{backend:?}: {script}: {error}"));
                }
            }
        }
    }
}

#[test]
fn manifest_read_role_does_not_authorize_writes_or_other_operands() {
    let (root, mut sandbox) = fixture(crate::tools::SandboxBackendKind::Seatbelt);
    let manifest = root.path().join("dependency/Cargo.toml");
    let outside = root.path().join("outside/Cargo.toml");
    for script in [
        format!(
            "cargo run --manifest-path '{outside}'",
            outside = outside.display()
        ),
        format!(
            "cargo run --manifest-path '{}' --target-dir '{}/target'",
            manifest.display(),
            manifest.parent().unwrap().display()
        ),
        format!(
            "cargo run --manifest-path '{}' > '{}'",
            manifest.display(),
            manifest.display()
        ),
        format!(
            "cargo run --manifest-path '{}' && touch '{}'",
            manifest.display(),
            manifest.display()
        ),
        format!("cargo run -- --manifest-path '{}'", manifest.display()),
        format!("cargo fix --manifest-path '{}'", manifest.display()),
        format!("cargo custom --manifest-path '{}'", manifest.display()),
        format!("other --manifest-path '{}'", manifest.display()),
    ] {
        for mode in [PathCheckMode::Full, PathCheckMode::ProtectedOnly] {
            assert!(
                sandbox
                    .validate_command_paths(
                        &script,
                        &root.path().join("scratch"),
                        mode,
                        Some(ToolCapability::Write)
                    )
                    .is_err(),
                "{script}"
            );
        }
    }
    let script = format!("cargo run --manifest-path '{}'", manifest.display());
    sandbox.spec.read_denylist.push(manifest.clone());
    assert!(
        sandbox
            .validate_command_paths(
                &script,
                &root.path().join("scratch"),
                PathCheckMode::ProtectedOnly,
                Some(ToolCapability::Write)
            )
            .is_err()
    );
    sandbox.spec.read_denylist.clear();
    sandbox
        .spec
        .readable_roots
        .retain(|root| root != &dunce::canonicalize(manifest.parent().unwrap()).unwrap());
    assert!(
        sandbox
            .validate_command_paths(
                &script,
                &root.path().join("scratch"),
                PathCheckMode::ProtectedOnly,
                Some(ToolCapability::Write)
            )
            .is_err()
    );
}

#[test]
fn path_guard_keeps_read_only_manifest_operand_conservative() {
    let (root, sandbox) = fixture(crate::tools::SandboxBackendKind::HostMountPathGuard);
    let script = format!(
        "cargo run --manifest-path '{}'",
        root.path().join("dependency/Cargo.toml").display()
    );
    for mode in [PathCheckMode::Full, PathCheckMode::ProtectedOnly] {
        assert!(
            sandbox
                .validate_command_paths(
                    &script,
                    &root.path().join("scratch"),
                    mode,
                    Some(ToolCapability::Write)
                )
                .is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn manifest_operand_cannot_escape_read_only_grant_through_symlink() {
    let (root, sandbox) = fixture(crate::tools::SandboxBackendKind::Seatbelt);
    let alias = root.path().join("dependency/escape.toml");
    std::os::unix::fs::symlink(root.path().join("outside/Cargo.toml"), &alias).unwrap();
    let script = format!("cargo run --manifest-path '{}'", alias.display());
    assert!(
        sandbox
            .validate_command_paths(
                &script,
                &root.path().join("scratch"),
                PathCheckMode::ProtectedOnly,
                Some(ToolCapability::Write)
            )
            .is_err()
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn cargo_named_program_cannot_turn_manifest_input_into_native_write_authority() {
    use std::os::unix::fs::PermissionsExt;
    let (root, mut sandbox) = fixture(crate::tools::SandboxBackendKind::Seatbelt);
    let manifest = root.path().join("dependency/Cargo.toml");
    let executable = root.path().join("scratch/cargo");
    let output = root.path().join("scratch/output.txt");
    std::fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf permitted > '{}'\ncat '{}'\nprintf changed > '{}'\n",
            output.display(),
            manifest.display(),
            manifest.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    for parent_read_only in [false, true] {
        if parent_read_only {
            sandbox
                .spec
                .readable_roots
                .push(dunce::canonicalize(root.path()).unwrap());
        }
        let result = sandbox
            .exec_with_timeout_and_capability(
                &format!(
                    "'{}' run --manifest-path '{}'",
                    executable.display(),
                    manifest.display()
                ),
                &root.path().join("scratch"),
                Some(std::time::Duration::from_secs(10)),
                Some(ToolCapability::Write),
            )
            .await
            .unwrap();
        assert_eq!(result.stdout, "fixture");
        assert_ne!(result.exit_code, 0);
        assert_eq!(std::fs::read(&manifest).unwrap(), b"fixture");
        assert_eq!(std::fs::read(&output).unwrap(), b"permitted");
    }
}
