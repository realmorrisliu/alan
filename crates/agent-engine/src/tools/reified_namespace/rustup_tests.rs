use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

fn installation() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let fixture = tempfile::tempdir().unwrap();
    let fixture_root = std::fs::canonicalize(fixture.path()).unwrap();
    let proxies = fixture_root.join("proxies");
    let home = fixture_root.join("rustup");
    let project = fixture_root.join("project");
    std::fs::create_dir_all(&proxies).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    executable(&proxies.join("rustup"));
    for name in ["cargo", "rustc", "rustdoc"] {
        symlink("rustup", proxies.join(name)).unwrap();
    }
    std::fs::write(home.join("settings.toml"), "version = \"12\"\ndefault_toolchain = \"stable-testhost\"\ndefault_host_triple = \"testhost\"\nprivate_unknown_field = \"do not project\"\n").unwrap();
    for name in ["stable-testhost", "1.97.0-testhost"] {
        let root = home.join("toolchains").join(name);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("lib/rustlib")).unwrap();
        for name in ["cargo", "rustc", "rustdoc"] {
            executable(&root.join("bin").join(name));
        }
        std::fs::write(
            root.join("lib/rustlib/multirust-config.toml"),
            "config_version = \"1\"\n",
        )
        .unwrap();
    }
    (fixture, proxies, home, project)
}

fn executable(path: &Path) {
    std::fs::write(path, b"\x7fELFmock test executable").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn input(project: &Path) -> ReifiedNamespacePlanInput {
    ReifiedNamespacePlanInput::new(
        vec![super::super::ReifiedMountDeclaration::host(
            project,
            project,
            super::super::ReifiedMountAccess::ReadOnly,
        )],
        project,
        vec!["/bin/sh".into()],
        super::super::NetworkPosture::Deny,
    )
}

#[test]
fn preserves_selector_precedence_and_private_metadata() {
    let (_fixture, proxies, home, project) = installation();
    let input = input(&project);
    std::fs::write(
        project.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.97.0\"\n",
    )
    .unwrap();
    let commands = vec![vec!["cargo".into(), "test".into()]];
    let environment = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input, &commands)),
    )
    .unwrap()
    .unwrap();
    assert!(!environment.settings.contains("do not project"));
    assert_eq!(environment.proxy_mounts.len(), 1);
    assert_eq!(environment.toolchain_mounts.len(), 2);
    assert_eq!(environment.metadata_hashes.len(), 4);
    revalidate(&environment).unwrap();
    std::fs::write(
        project.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"missing\"\n",
    )
    .unwrap();
    assert!(
        revalidate(&environment)
            .unwrap_err()
            .contains("metadata changed")
    );
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands))
        )
        .unwrap_err()
        .contains("selected Rust runtime absent")
    );
    discover(
        proxies.to_str().unwrap(),
        &home,
        Some("1.97.0".into()),
        Some((&input, &commands)),
    )
    .unwrap();
    discover(
        proxies.to_str().unwrap(),
        &home,
        Some("missing".into()),
        Some((
            &input,
            &[vec!["cargo".into(), "+1.97.0".into(), "test".into()]],
        )),
    )
    .unwrap();
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((
                &input,
                &[vec!["cargo".into(), "+missing".into(), "test".into()]]
            ))
        )
        .unwrap_err()
        .contains("selected Rust runtime absent")
    );
}

#[test]
fn directory_override_proximity_and_legacy_file_match_rustup() {
    let (_fixture, proxies, home, project) = installation();
    let settings = format!(
        "version = \"12\"\ndefault_toolchain = \"stable-testhost\"\ndefault_host_triple = \"testhost\"\n[overrides]\n{} = \"1.97.0\"\n",
        toml::Value::String(project.to_str().unwrap().into())
    );
    std::fs::write(home.join("settings.toml"), settings).unwrap();
    std::fs::write(
        project.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"missing\"\n",
    )
    .unwrap();
    let commands = vec![vec!["rustc".into(), "--version".into()]];
    discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input(&project), &commands)),
    )
    .unwrap();
    let child = project.join("child");
    std::fs::create_dir(&child).unwrap();
    std::fs::write(child.join("rust-toolchain"), "missing\n").unwrap();
    std::fs::write(
        child.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.97.0\"\n",
    )
    .unwrap();
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&child), &commands))
        )
        .unwrap_err()
        .contains("selected Rust runtime absent")
    );
    std::fs::write(child.join("rust-toolchain"), "1.97.0\n").unwrap();
    discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input(&child), &commands)),
    )
    .unwrap();
}

#[test]
fn rejects_runtime_loss_proxy_escape_and_unknown_wrapper() {
    let (_fixture, proxies, home, project) = installation();
    let environment = discover(proxies.to_str().unwrap(), &home, None, None)
        .unwrap()
        .unwrap();
    let rustdoc = home.join("toolchains/1.97.0-testhost/bin/rustdoc");
    std::fs::remove_file(&rustdoc).unwrap();
    assert!(revalidate(&environment).is_err());
    executable(&rustdoc);
    std::fs::write(&rustdoc, b"\x7fELFchanged executable").unwrap();
    assert!(
        revalidate(&environment)
            .unwrap_err()
            .contains("executable changed")
    );
    executable(&rustdoc);
    std::fs::remove_file(proxies.join("cargo")).unwrap();
    symlink(project.join("outside"), proxies.join("cargo")).unwrap();
    executable(&project.join("outside"));
    assert!(
        discover(proxies.to_str().unwrap(), &home, None, None)
            .unwrap_err()
            .contains("proxy escapes")
    );
    std::fs::remove_file(proxies.join("cargo")).unwrap();
    symlink("rustup", proxies.join("cargo")).unwrap();
    executable(&proxies.join("cargo-extra"));
    assert!(
        discover(proxies.to_str().unwrap(), &home, None, None)
            .unwrap_err()
            .contains("extra file")
    );
    std::fs::remove_file(proxies.join("cargo-extra")).unwrap();
    std::fs::write(proxies.join("rustup"), "#!/bin/sh\nexit 0\n").unwrap();
    assert!(
        discover(proxies.to_str().unwrap(), &home, None, None)
            .unwrap_err()
            .contains("wrapper")
    );
}

#[test]
fn refuses_undelegated_toolchain_file_and_metadata_aliases() {
    let (_fixture, proxies, home, project) = installation();
    std::fs::write(project.parent().unwrap().join("rust-toolchain"), "1.97.0\n").unwrap();
    let commands = vec![vec!["cargo".into(), "test".into()]];
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands))
        )
        .unwrap_err()
        .contains("outside delegated")
    );
    std::fs::rename(home.join("settings.toml"), home.join("original-settings")).unwrap();
    symlink("original-settings", home.join("settings.toml")).unwrap();
    assert!(
        discover(proxies.to_str().unwrap(), &home, None, None)
            .unwrap_err()
            .contains("unsafe or oversized")
    );
}

#[test]
fn missing_components_and_malformed_paths_refuse_before_effects() {
    let (_fixture, proxies, home, project) = installation();
    let commands = vec![vec!["cargo".into(), "test".into()]];
    std::fs::write(
        project.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.97.0\"\ncomponents = [\"missing\"]\n",
    )
    .unwrap();
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands))
        )
        .unwrap_err()
        .contains("component/target absent")
    );
    std::fs::write(
        project.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.97.0\"\npath = 42\n",
    )
    .unwrap();
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands))
        )
        .unwrap_err()
        .contains("invalid Rust toolchain path")
    );
}

#[test]
fn revalidation_checks_helper_authority_and_component_metadata() {
    let (_fixture, proxies, home, project) = installation();
    let root = home.join("toolchains/1.97.0-testhost");
    let helper = root.join("bin/cargo-fmt");
    executable(&helper);
    executable(&root.join("bin/rustfmt"));
    let commands = vec![vec!["cargo".into(), "+1.97.0".into(), "fmt".into()]];
    let environment = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input(&project), &commands)),
    )
    .unwrap()
    .unwrap();
    let outside = project.join("same-bytes-outside-runtime");
    executable(&outside);
    std::fs::remove_file(&helper).unwrap();
    symlink(&outside, &helper).unwrap();
    assert!(revalidate(&environment).unwrap_err().contains("escapes"));
    std::fs::remove_file(&helper).unwrap();
    executable(&helper);
    std::fs::write(
        root.join("lib/rustlib/multirust-config.toml"),
        "config_version = \"1\"\ncomponents = []\n",
    )
    .unwrap();
    assert!(
        revalidate(&environment)
            .unwrap_err()
            .contains("metadata changed")
    );
}

#[test]
fn component_metadata_parent_alias_cannot_escape_runtime() {
    let (_fixture, proxies, home, project) = installation();
    let root = home.join("toolchains/1.97.0-testhost");
    let library = root.join("lib/rustlib");
    std::fs::rename(&library, root.join("lib/original-rustlib")).unwrap();
    std::fs::write(project.join("multirust-config.toml"), "private canary").unwrap();
    symlink(&project, &library).unwrap();
    assert!(
        discover(proxies.to_str().unwrap(), &home, None, None)
            .unwrap_err()
            .contains("component metadata escapes")
    );
}
