use super::*;

fn checked(script: &str) -> Result<Vec<ReifiedRustCommandProvider>, String> {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(root.path()).unwrap();
    let project = base.join("project");
    let bin = base.join("proxies");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("rustup"), b"\x7fELFfixture").unwrap();
    std::fs::set_permissions(bin.join("rustup"), std::fs::Permissions::from_mode(0o755)).unwrap();
    for name in ["rustc", "cargo", "rustdoc", "rustfmt"] {
        std::fs::hard_link(bin.join("rustup"), bin.join(name)).unwrap();
    }
    super::super::validate_proxies(&bin).unwrap();
    let input = ReifiedNamespacePlanInput::new(
        vec![super::super::super::ReifiedMountDeclaration::host(
            &project,
            &project,
            super::super::super::ReifiedMountAccess::ReadWrite,
        )],
        &project,
        vec!["/bin/sh".into(), "-c".into(), script.into()],
        super::super::super::NetworkPosture::Deny,
    );
    inspect(
        &input,
        &format!("{}:/usr/bin:/bin", bin.display()),
        &[ReifiedExecutionSubstrateMount::new(&bin, &bin)],
    )
}

#[test]
fn opaque_programs_and_repeat_work_require_separate_requests() {
    for script in [
        "git checkout other -- .cargo/config.toml; cargo test",
        "tar -xf payload.tar; cargo test",
        "python3 writer.py; cargo test",
        "./writer; rustc --version",
        "cargo test && cargo run",
        "setsid cargo test; writer",
        "setsid -f cargo test; writer",
        "setsid -w cargo test",
        "cargo test; rustc --version",
        "cargo test && printf code | rustc -",
        "for PATH in /other/bin; do :; done; cargo test",
        "for i in 1 2; do cd child; done; cargo test",
        "while true; do cargo test; done",
        "unset PATH; cargo test",
        "export PATH; cargo test",
        "test -v 'BASH_VERSINFO[$(writer)]'; cargo test",
        "[ -v 'BASH_VERSINFO[$(writer)]' ]; cargo test",
        "(cd child); cargo test",
        "cd child | true; cargo test",
        "cargo test | writer",
        "cargo test > marker",
        "printf code | rustc - | writer",
        "(cd child && printf code) | rustc -",
        "(set -eu; printf code) | rustc -",
        "writer & cargo test",
        "cargo test; writer &",
        "printf '%n' PATH; cargo test",
        "env printf hi; cargo test",
        "/project/printf hi; cargo test",
        "FOO=bar; cargo test",
        "RUSTUP_TOOLCHAIN=$OTHER cargo test",
        "/project/timeout 10 cargo test",
        "/project/env cargo test",
    ] {
        assert!(checked(script).is_err(), "{script}");
    }
}

#[test]
fn literal_selection_queries_and_bounded_preludes_keep_their_bindings() {
    for script in [
        "cargo test",
        "printf 'fn main() {}' | rustc -",
        "(cat src/lib.rs; printf 'fn main() {}') | rustc - && ./result",
        "{ cat src/lib.rs; printf 'fn main() {}'; } | rustc - && ./result",
        "cargo test && cat src/lib.rs && git diff",
        "set -eu; cargo test",
        "cd child && cargo test",
        "set -eu || cd child; cargo test",
        "rustc --version; cargo --version; cargo test",
        "rustc -vV; rustdoc --version; cargo --help fmt; cargo test",
        "RUSTUP_TOOLCHAIN=1.97.0 cargo test",
        "command cargo test",
        "exec cargo test",
        "env -- RUSTUP_TOOLCHAIN=1.97.0 cargo test",
        "cargo --help fmt; cargo --version custom; cargo --list",
    ] {
        assert!(checked(script).is_ok(), "{script}: {:?}", checked(script));
    }
    let env = checked("env -- RUSTUP_TOOLCHAIN=1.97.0 cargo test").unwrap();
    assert_eq!(env.len(), 1);
    assert_eq!(
        Path::new(&env[0].executable),
        &env[0].resolved.as_ref().unwrap().0
    );
    assert!(env[0].resolved.is_some());
}

#[test]
fn fake_namespace_wrapper_sources_are_not_standard_providers() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let fake = root.path().join("fake-bin");
    std::fs::create_dir(&fake).unwrap();
    std::fs::write(fake.join("env"), "#!/bin/sh\nexit 0\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(fake.join("env"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let input = ReifiedNamespacePlanInput::new(
        vec![super::super::super::ReifiedMountDeclaration::host(
            &project,
            &project,
            super::super::super::ReifiedMountAccess::ReadWrite,
        )],
        &project,
        vec!["/bin/sh".into(), "-c".into(), "env cargo test".into()],
        super::super::super::NetworkPosture::Deny,
    )
    .with_execution_substrate(vec![ReifiedExecutionSubstrateMount::new("/usr/bin", &fake)]);
    assert!(super::super::super::ReifiedNamespacePlan::derive(input.clone()).is_ok());
    assert!(inspect(&input, "/usr/bin", &[]).is_err());
    let absent =
        input.with_execution_substrate(vec![ReifiedExecutionSubstrateMount::new("/aux", &fake)]);
    assert!(super::super::super::ReifiedNamespacePlan::derive(absent.clone()).is_ok());
    assert!(inspect(&absent, "/usr/bin:/aux", &[]).is_err());
}

#[test]
fn standard_aliases_revalidate_the_frozen_host_identity() {
    let root = tempfile::tempdir().unwrap();
    let input = ReifiedNamespacePlanInput::new(
        vec![super::super::super::ReifiedMountDeclaration::host(
            root.path(),
            root.path(),
            super::super::super::ReifiedMountAccess::ReadWrite,
        )],
        root.path(),
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "/mnt/std/git --version; cargo test".into(),
        ],
        super::super::super::NetworkPosture::Deny,
    )
    .with_execution_substrate(vec![ReifiedExecutionSubstrateMount::new(
        "/mnt/std", "/usr/bin",
    )]);
    let bindings = inspect(&input, "/mnt/std", &[]).unwrap();
    assert_eq!(bindings.len(), 1);
    let binding = &bindings[0];
    assert_eq!(
        provider::snapshot(&binding.executable, &binding.path, &[]).unwrap(),
        binding.resolved
    );
}

#[test]
fn host_alias_identity_cannot_replace_actual_namespace_alias_resolution() {
    let root = tempfile::tempdir().unwrap();
    for directory in ["project", "native", "source", "bridge", "fake-bridge"] {
        std::fs::create_dir(root.path().join(directory)).unwrap();
    }
    let native = root.path().join("native");
    let source = root.path().join("source");
    let bridge = root.path().join("bridge");
    let fake = root.path().join("fake-bridge");
    std::os::unix::fs::symlink("/usr/bin/git", native.join("git")).unwrap();
    std::os::unix::fs::symlink("/usr/bin/git", bridge.join("git")).unwrap();
    std::os::unix::fs::symlink("../bridge/git", source.join("git")).unwrap();
    std::fs::write(fake.join("git"), "#!/bin/sh\nexit 0\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(fake.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let project = root.path().join("project");
    let mut substrate = super::super::super::default_execution_substrate();
    substrate.extend([
        ReifiedExecutionSubstrateMount::new(&native, &source),
        ReifiedExecutionSubstrateMount::new(&bridge, &fake),
    ]);
    let input = ReifiedNamespacePlanInput::new(
        vec![super::super::super::ReifiedMountDeclaration::host(
            &project,
            &project,
            super::super::super::ReifiedMountAccess::ReadWrite,
        )],
        &project,
        vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("{}/git --version; cargo test", native.display()),
        ],
        super::super::super::NetworkPosture::Deny,
    )
    .with_scratch_tmp_namespace_path("/.alan-alias-test-tmp")
    .with_execution_substrate(substrate);
    super::super::super::ReifiedNamespacePlan::derive(input.clone()).unwrap();
    assert!(inspect(&input, "/usr/bin:/bin", &[]).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn system_cross_root_alias_requires_its_actual_target_projection() {
    if !std::fs::read_link("/usr/bin/env").is_ok_and(|target| target.starts_with("../lib")) {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut input = ReifiedNamespacePlanInput::new(
        vec![super::super::super::ReifiedMountDeclaration::host(
            root.path(),
            root.path(),
            super::super::super::ReifiedMountAccess::ReadWrite,
        )],
        root.path(),
        vec!["/bin/sh".into(), "-c".into(), "env cargo test".into()],
        super::super::super::NetworkPosture::Deny,
    );
    input
        .execution_substrate
        .retain(|mount| !matches!(mount.namespace_path.to_str(), Some("/usr/lib" | "/lib")));
    super::super::super::ReifiedNamespacePlan::derive(input.clone()).unwrap();
    assert!(inspect(&input, "/usr/bin:/bin", &[]).is_err());
}

#[test]
fn ordinary_rust_names_and_project_proxy_layouts_are_not_query_proof() {
    use std::os::unix::fs::PermissionsExt;
    for alias in [false, true] {
        for access in [
            super::super::super::ReifiedMountAccess::ReadOnly,
            super::super::super::ReifiedMountAccess::ReadWrite,
        ] {
            let root = tempfile::tempdir().unwrap();
            let project = std::fs::canonicalize(root.path()).unwrap();
            let bin = project.join("bin");
            std::fs::create_dir(&bin).unwrap();
            std::fs::write(bin.join("rustup"), b"\x7fELFproject-program").unwrap();
            std::fs::set_permissions(bin.join("rustup"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
            for name in ["rustc", "cargo", "rustdoc", "rustfmt"] {
                std::fs::hard_link(bin.join("rustup"), bin.join(name)).unwrap();
            }
            super::super::validate_proxies(&bin).unwrap();
            let namespace = if alias {
                Path::new("/mnt/project")
            } else {
                &project
            };
            let proxy = ReifiedExecutionSubstrateMount::new(namespace.join("bin"), &bin);
            for name in ["rustc", "cargo", "rustdoc", "rustfmt", "rustup"] {
                let executable = namespace.join("bin").join(name);
                for proxies in [&[][..], std::slice::from_ref(&proxy)] {
                    let input = ReifiedNamespacePlanInput::new(
                        vec![super::super::super::ReifiedMountDeclaration::host(
                            namespace, &project, access,
                        )],
                        &project,
                        vec![
                            "/bin/sh".into(),
                            "-c".into(),
                            format!("{} --version; cargo test", executable.display()),
                        ],
                        super::super::super::NetworkPosture::Deny,
                    );
                    assert!(
                        super::super::super::ReifiedNamespacePlan::derive(input.clone()).is_ok()
                    );
                    assert!(
                        inspect(&input, "/usr/bin:/bin", proxies).is_err(),
                        "{name}, {alias}, {access:?}"
                    );
                    let single = ReifiedNamespacePlanInput {
                        argv: vec![
                            "/bin/sh".into(),
                            "-c".into(),
                            format!("{} --version", executable.display()),
                        ],
                        ..input
                    };
                    assert!(inspect(&single, "/usr/bin:/bin", proxies).is_ok());
                }
            }
        }
    }
}

#[test]
fn forwarded_rustup_execution_requires_a_separate_request() {
    assert!(checked("rustup run stable cargo --version; cargo test").is_err());
    assert!(checked("rustup run stable rustc --version; cargo test").is_err());
    assert!(checked("rustup run stable rustc --version").is_ok());
}

#[test]
fn an_external_hard_link_does_not_make_a_project_controlled_proxy_trusted() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(root.path()).unwrap();
    let project = base.join("project");
    let bin = base.join("proxies");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("rustup"), b"\x7fELFproject-linked-program").unwrap();
    std::fs::set_permissions(bin.join("rustup"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::hard_link(bin.join("rustup"), bin.join("rustc")).unwrap();
    symlink("rustup", bin.join("cargo")).unwrap();
    super::super::validate_proxies(&bin).unwrap();
    let input = ReifiedNamespacePlanInput::new(
        vec![super::super::super::ReifiedMountDeclaration::host(
            &project,
            &project,
            super::super::super::ReifiedMountAccess::ReadWrite,
        )],
        &project,
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "rustc --version; cargo test".into(),
        ],
        super::super::super::NetworkPosture::Deny,
    );
    assert!(super::super::super::ReifiedNamespacePlan::derive(input.clone()).is_ok());
    let proxies = [ReifiedExecutionSubstrateMount::new(&bin, &bin)];
    let path = format!("{}:/usr/bin:/bin", bin.display());
    assert!(inspect(&input, &path, &proxies).is_ok());
    let linked = project.join("linked-program");
    std::fs::hard_link(bin.join("rustup"), &linked).unwrap();
    assert!(inspect(&input, &path, &proxies).is_err());
    let single = ReifiedNamespacePlanInput {
        argv: vec!["/bin/sh".into(), "-c".into(), "rustc --version".into()],
        ..input.clone()
    };
    assert!(inspect(&single, &path, &proxies).is_ok());
    std::fs::remove_file(linked).unwrap();
    assert!(inspect(&input, &path, &proxies).is_ok());
    let linked_file = base.join("granted-file");
    std::fs::hard_link(bin.join("rustup"), &linked_file).unwrap();
    for access in [
        super::super::super::ReifiedMountAccess::ReadOnly,
        super::super::super::ReifiedMountAccess::ReadWrite,
    ] {
        let mut file_grant = input.clone();
        file_grant
            .declarations
            .push(super::super::super::ReifiedMountDeclaration::host(
                &linked_file,
                &linked_file,
                access,
            ));
        assert!(super::super::super::ReifiedNamespacePlan::derive(file_grant.clone()).is_ok());
        assert!(inspect(&file_grant, &path, &proxies).is_err());
    }
}

#[test]
fn a_file_grant_inside_a_proxy_directory_does_not_make_queries_trusted() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(root.path()).unwrap();
    let cwd = base.join("scratch");
    let bin = base.join("proxies");
    std::fs::create_dir(&cwd).unwrap();
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("rustup"), b"\x7fELFfile-granted-program").unwrap();
    std::fs::set_permissions(bin.join("rustup"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::hard_link(bin.join("rustup"), bin.join("rustc")).unwrap();
    super::super::validate_proxies(&bin).unwrap();
    let input = ReifiedNamespacePlanInput::new(
        vec![
            super::super::super::ReifiedMountDeclaration::host(
                &cwd,
                &cwd,
                super::super::super::ReifiedMountAccess::ReadWrite,
            ),
            super::super::super::ReifiedMountDeclaration::host(
                bin.join("rustup"),
                bin.join("rustup"),
                super::super::super::ReifiedMountAccess::ReadOnly,
            ),
        ],
        &cwd,
        vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("{} --version; cargo test", bin.join("rustc").display()),
        ],
        super::super::super::NetworkPosture::Deny,
    );
    assert!(super::super::super::ReifiedNamespacePlan::derive(input.clone()).is_ok());
    assert!(
        inspect(
            &input,
            "/usr/bin:/bin",
            &[ReifiedExecutionSubstrateMount::new(&bin, &bin)]
        )
        .is_err()
    );
}
