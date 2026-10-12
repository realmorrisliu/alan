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

#[test]
fn command_projection_mounts_only_selected_runtimes() {
    let (_fixture, proxies, home, project) = installation();
    let input = input(&project);
    for (commands, expected) in [
        (vec![vec!["printf", "hello"]], vec![]),
        (
            vec![vec!["cargo", "+1.97.0", "test"]],
            vec!["1.97.0-testhost"],
        ),
        (
            vec![vec!["rustup", "run", "1.97.0", "cargo", "test"]],
            vec!["1.97.0-testhost"],
        ),
        (
            vec![vec!["cargo", "test"], vec!["rustc", "+1.97.0", "--version"]],
            vec!["1.97.0-testhost", "stable-testhost"],
        ),
    ] {
        let commands: Vec<Vec<String>> = commands
            .into_iter()
            .map(|words| words.into_iter().map(String::from).collect())
            .collect();
        let environment = discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap()
        .unwrap();
        let names: Vec<_> = environment
            .toolchain_mounts
            .iter()
            .map(|mount| mount.host_path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, expected);
        for (path, _) in environment
            .metadata_hashes
            .iter()
            .chain(&environment.executable_hashes)
        {
            if let Ok(relative) = path.strip_prefix(home.join("toolchains")) {
                assert!(expected.iter().any(|name| relative.starts_with(name)));
            }
        }
        revalidate(&environment).unwrap();
    }
}

#[test]
fn literal_compile_targets_are_checked_before_effects() {
    let (_fixture, proxies, home, project) = installation();
    let root = home.join("toolchains/1.97.0-testhost");
    std::fs::write(
        root.join("lib/rustlib/multirust-config.toml"),
        "config_version = \"1\"\n[[components]]\npkg = \"rustc\"\ntarget = \"host-testhost\"\n[[components]]\npkg = \"rust-std\"\ntarget = \"host-testhost\"\n",
    )
    .unwrap();
    let input = input(&project);
    for words in [
        vec!["cargo", "+1.97.0", "build", "--target", "missing"],
        vec!["cargo", "+1.97.0", "check", "--target=missing"],
        vec!["rustc", "+1.97.0", "--target", "missing", "source.rs"],
        vec!["rustdoc", "+1.97.0", "--target=missing", "source.rs"],
        vec![
            "cargo",
            "+1.97.0",
            "test",
            "--target",
            "host-testhost",
            "--target=missing",
        ],
        vec!["cargo", "+1.97.0", "rustc", "--", "--target=missing"],
        vec!["cargo", "+1.97.0", "rustc", "--", "--target=host-tuple"],
        vec![
            "cargo",
            "+1.97.0",
            "--quiet",
            "rustc",
            "--",
            "--target=missing",
        ],
        vec![
            "rustup",
            "run",
            "1.97.0",
            "cargo",
            "build",
            "--target=missing",
        ],
        vec!["cargo", "+1.97.0", "build", "--target"],
        vec!["cargo", "+1.97.0", "build", "--target="],
        vec!["cargo", "+1.97.0", "build", "--target=custom.json"],
    ] {
        let commands = vec![words.iter().map(|word| (*word).into()).collect()];
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&input, &commands))
            )
            .is_err(),
            "{words:?} must refuse before execution"
        );
    }
    for words in [
        vec!["cargo", "+1.97.0", "build", "--target", "host-testhost"],
        vec!["cargo", "+1.97.0", "build", "--target=host-tuple"],
        vec!["rustc", "+1.97.0", "--target=host-testhost", "source.rs"],
        vec![
            "rustdoc",
            "+1.97.0",
            "--target",
            "host-testhost",
            "source.rs",
        ],
        vec![
            "cargo",
            "+1.97.0",
            "test",
            "--target=host-testhost",
            "--target",
            "host-testhost",
        ],
        vec!["cargo", "+1.97.0", "rustc", "--", "--target=host-testhost"],
        vec![
            "rustup",
            "run",
            "1.97.0",
            "cargo",
            "build",
            "--target=host-testhost",
        ],
        vec!["cargo", "+1.97.0", "run", "--", "--target=missing"],
        vec!["cargo", "+1.97.0", "test", "--", "--target=missing"],
        vec![
            "cargo",
            "+1.97.0",
            "--config",
            "env.PROBE='clippy'",
            "test",
            "--",
            "--target=missing",
        ],
    ] {
        let commands = vec![words.iter().map(|word| (*word).into()).collect()];
        let environment = discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap()
        .unwrap();
        revalidate(&environment).unwrap();
    }
}

#[test]
fn rustup_run_inspects_the_direct_command_and_refuses_selection_mutations() {
    let (_fixture, proxies, home, project) = installation();
    let input = input(&project);
    for words in [
        vec!["rustup", "run", "1.97.0", "cargo", "+missing", "test"],
        vec!["rustup", "run", "1.97.0", "cargo", "fmt"],
        vec!["rustup", "run", "1.97.0", "cargo", "clippy"],
        vec!["cargo", "+1.97.0", "--quiet", "fmt"],
        vec!["cargo", "+1.97.0", "--color", "never", "clippy"],
        vec!["rustup", "run", "1.97.0", "sh", "-c", "cargo test"],
        vec![
            "rustup", "run", "1.97.0", "rustup", "run", "missing", "cargo",
        ],
        vec!["rustup", "override", "unset"],
        vec!["rustup", "default", "1.97.0"],
        vec!["rustup", "toolchain", "link", "custom", "/tmp/custom"],
    ] {
        let commands = vec![words.iter().map(|word| (*word).into()).collect()];
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&input, &commands))
            )
            .is_err(),
            "{words:?} must fail before execution"
        );
    }
    for name in ["cargo-fmt", "rustfmt", "cargo-clippy", "clippy-driver"] {
        executable(&home.join("toolchains/1.97.0-testhost/bin").join(name));
    }
    for subcommand in ["test", "fmt", "clippy"] {
        let commands = vec![
            ["rustup", "run", "1.97.0", "cargo", subcommand]
                .into_iter()
                .map(String::from)
                .collect(),
        ];
        let environment = discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(environment.toolchain_mounts.len(), 1);
        revalidate(&environment).unwrap();
    }
}

#[test]
fn unselected_custom_alias_does_not_block_startup_or_ordinary_commands() {
    let (_fixture, proxies, home, project) = installation();
    symlink(&project, home.join("toolchains/custom-linked")).unwrap();
    let commands = vec![vec!["git".into(), "status".into()]];
    for inspection in [None, Some((&input(&project), commands.as_slice()))] {
        let environment = discover(proxies.to_str().unwrap(), &home, None, inspection)
            .unwrap()
            .unwrap();
        assert!(environment.toolchain_mounts.is_empty());
        revalidate(&environment).unwrap();
        let environment = discover(
            proxies.to_str().unwrap(),
            &home.join("absent"),
            None,
            inspection,
        )
        .unwrap()
        .unwrap();
        assert!(environment.metadata_hashes.is_empty());
        assert!(environment.toolchain_mounts.is_empty());
        revalidate(&environment).unwrap();
    }
    let commands = vec![vec!["cargo".into(), "+custom-linked".into(), "test".into()]];
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands))
        )
        .unwrap_err()
        .contains("unsafe alias")
    );
}

#[test]
fn sequential_cd_preserves_missing_target_failure_branches() {
    let (_fixture, proxies, home, project) = installation();
    std::fs::create_dir_all(project.join("a/b")).unwrap();
    std::fs::write(project.join("a/b/rust-toolchain"), "1.97.0\n").unwrap();
    let commands = vec![
        vec!["cd".into(), "a".into()],
        vec!["cd".into(), "b".into()],
        vec!["cargo".into(), "test".into()],
    ];
    let environment = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input(&project), &commands)),
    )
    .unwrap()
    .unwrap();
    assert_eq!(environment.toolchain_mounts.len(), 2);
    assert!(
        environment
            .metadata_hashes
            .iter()
            .any(|(path, _)| path == &project.join("a/b/rust-toolchain"))
    );
    assert!(!project.join("b").exists());
}

#[test]
fn command_prefixes_preserve_rustup_selection_and_validate_before_effects() {
    let (_fixture, proxies, home, project) = installation();
    let input = input(&project);
    std::fs::write(project.join("rust-toolchain"), "missing\n").unwrap();
    for words in [
        vec!["RUSTUP_TOOLCHAIN=missing", "cargo", "test"],
        vec!["env", "--", "RUSTUP_TOOLCHAIN=missing", "cargo", "test"],
        vec![
            "env",
            "RUSTUP_TOOLCHAIN=missing",
            "timeout",
            "10",
            "cargo",
            "test",
        ],
        vec!["command", "cargo", "+missing", "test"],
        vec!["rustup", "run", "missing", "cargo", "test"],
    ] {
        let commands = vec![words.iter().map(|word| (*word).into()).collect()];
        let error = discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap_err();
        assert!(
            error.contains("selected Rust runtime absent"),
            "{words:?}: {error}"
        );
    }
    for words in [
        vec!["RUSTUP_TOOLCHAIN=1.97.0", "cargo", "test"],
        vec![
            "RUSTUP_TOOLCHAIN=missing",
            "RUSTUP_TOOLCHAIN=1.97.0",
            "cargo",
            "test",
        ],
        vec!["env", "--", "RUSTUP_TOOLCHAIN=1.97.0", "cargo", "test"],
        vec![
            "env",
            "RUSTUP_TOOLCHAIN=1.97.0",
            "timeout",
            "--preserve-status",
            "10",
            "cargo",
            "test",
        ],
        vec![
            "RUSTUP_TOOLCHAIN=missing",
            "command",
            "cargo",
            "+1.97.0",
            "test",
        ],
        vec!["FOO=anything", "printf", "RUSTUP_TOOLCHAIN=missing"],
        vec!["printf", "PATH=anything"],
    ] {
        let commands = vec![words.iter().map(|word| (*word).into()).collect()];
        discover(
            proxies.to_str().unwrap(),
            &home,
            Some("missing".into()),
            Some((&input, &commands)),
        )
        .unwrap_or_else(|error| panic!("{words:?}: {error}"));
    }
}

#[test]
fn refuses_environment_changes_that_escape_the_inspected_selection() {
    let (_fixture, proxies, home, project) = installation();
    let input = input(&project);
    for words in [
        vec!["PATH=/bin", "cargo", "test"],
        vec!["env", "RUSTUP_HOME=/tmp/other", "cargo", "test"],
        vec!["RUSTUP_TOOLCHAIN=1.97.0"],
        vec!["export", "RUSTUP_TOOLCHAIN=1.97.0"],
        vec!["unset", "PATH"],
        vec!["read", "RUSTUP_TOOLCHAIN"],
    ] {
        let commands = vec![words.iter().map(|word| (*word).into()).collect()];
        let error = discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap_err();
        assert!(
            error.contains("inspected Linux tool environment"),
            "{words:?}: {error}"
        );
    }
}

#[test]
fn shell_directory_changes_inspect_possible_project_selections() {
    let (_fixture, proxies, home, project) = installation();
    let input = input(&project);
    let child = project.join("child");
    std::fs::create_dir(&child).unwrap();
    std::fs::write(child.join("rust-toolchain"), "missing\n").unwrap();
    let mut commands = vec![
        vec!["cd".into(), "child".into()],
        vec!["cargo".into(), "test".into()],
    ];
    let error = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input, &commands)),
    )
    .unwrap_err();
    assert!(error.contains("selected Rust runtime absent"), "{error}");
    std::fs::write(child.join("rust-toolchain"), "1.97.0\n").unwrap();
    let environment = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input, &commands)),
    )
    .unwrap()
    .unwrap();
    assert!(
        environment
            .metadata_hashes
            .iter()
            .any(|(path, _)| path == &child.join("rust-toolchain"))
    );
    revalidate(&environment).unwrap();
    std::fs::write(child.join("rust-toolchain"), "missing\n").unwrap();
    assert!(
        revalidate(&environment)
            .unwrap_err()
            .contains("metadata changed")
    );
    commands[0] = vec!["cd".into()];
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands))
        )
        .unwrap_err()
        .contains("Rust selection after shell cd")
    );
    discover(
        proxies.to_str().unwrap(),
        &home,
        Some("1.97.0".into()),
        Some((&input, &commands)),
    )
    .unwrap();
    for words in [
        vec!["cargo", "+1.97.0", "test"],
        vec!["RUSTUP_TOOLCHAIN=1.97.0", "cargo", "test"],
        vec!["printf", "cargo"],
    ] {
        commands[1] = words.iter().map(|word| (*word).into()).collect();
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap();
    }
    commands.swap(0, 1);
    commands[0] = vec!["cargo".into(), "test".into()];
    discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input, &commands)),
    )
    .unwrap();
}

#[test]
fn shell_directory_inspection_stays_bounded_and_inside_grants() {
    let (_fixture, _, _, project) = installation();
    let input = input(&project);
    let outside = project.parent().unwrap().join("outside");
    std::fs::create_dir(&outside).unwrap();
    symlink(outside, project.join("escape")).unwrap();
    assert!(cd_directories(&input, std::slice::from_ref(&project), &["escape".into()]).is_none());
    let directories = (0..64)
        .map(|index| {
            let directory = project.join(index.to_string());
            std::fs::create_dir_all(directory.join("child")).unwrap();
            directory
        })
        .collect::<Vec<_>>();
    assert!(cd_directories(&input, &directories, &["child".into()]).is_none());
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
    assert_eq!(environment.toolchain_mounts.len(), 1);
    assert_eq!(environment.metadata_hashes.len(), 3);
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
fn remapped_directories_preserve_inspected_rustup_selection() {
    let (_fixture, proxies, home, project) = installation();
    let child = project.join("child");
    std::fs::create_dir(&child).unwrap();
    let commands = vec![
        vec!["rustc".into(), "--version".into()],
        vec!["cd".into(), "child".into()],
        vec!["cargo".into(), "--version".into()],
    ];
    let mut input = input(&project);
    input.declarations[0].namespace_path = "/mnt/project".into();
    input
        .declarations
        .push(super::super::ReifiedMountDeclaration::host(
            "/mnt/child",
            &child,
            super::super::ReifiedMountAccess::ReadOnly,
        ));
    for override_path in [&project, project.parent().unwrap()] {
        let original = format!(
            "version = \"12\"\ndefault_toolchain = \"missing\"\ndefault_host_triple = \"testhost\"\n[overrides]\n{} = \"1.97.0\"\n",
            toml::Value::String(override_path.to_str().unwrap().into())
        );
        std::fs::write(home.join("settings.toml"), &original).unwrap();
        if override_path == project {
            std::fs::write(project.join("rust-toolchain"), "missing\n").unwrap();
        } else {
            std::fs::remove_file(project.join("rust-toolchain")).unwrap();
        }
        std::fs::write(child.join("rust-toolchain"), "stable-testhost\n").unwrap();
        let environment = discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap()
        .unwrap();
        let settings: toml::Value = toml::from_str(&environment.settings).unwrap();
        assert_eq!(
            settings["overrides"]["/mnt/project"].as_str(),
            Some("1.97.0")
        );
        for path in ["/mnt/project/child", "/mnt/child"] {
            assert_eq!(
                settings["overrides"][path].as_str(),
                Some("stable-testhost")
            );
        }
        assert!(
            settings["overrides"]
                .get(override_path.to_str().unwrap())
                .is_none()
        );
        assert_eq!(environment.toolchain_mounts.len(), 2);
        revalidate(&environment).unwrap();
        assert_eq!(
            std::fs::read_to_string(home.join("settings.toml")).unwrap(),
            original
        );
        std::fs::write(home.join("settings.toml"), format!("{original}# changed\n")).unwrap();
        assert!(
            revalidate(&environment)
                .unwrap_err()
                .contains("metadata changed")
        );
    }
    let alias = project.parent().unwrap().join("project-alias");
    symlink(&project, &alias).unwrap();
    input.cwd = alias.clone();
    input.declarations[0].source = super::super::ReifiedMountSource::Host(alias);
    let environment = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input, &commands)),
    )
    .unwrap()
    .unwrap();
    let settings: toml::Value = toml::from_str(&environment.settings).unwrap();
    assert_eq!(
        settings["overrides"]["/mnt/project"].as_str(),
        Some("1.97.0")
    );
    std::fs::write(project.join("rust-toolchain"), "missing\n").unwrap();
    for commands in [
        vec![vec!["cargo".into(), "+1.97.0".into(), "test".into()]],
        vec![vec![
            "RUSTUP_TOOLCHAIN=1.97.0".into(),
            "cargo".into(),
            "test".into(),
        ]],
        vec![vec![
            "rustup".into(),
            "run".into(),
            "1.97.0".into(),
            "cargo".into(),
            "test".into(),
        ]],
    ] {
        let environment = discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(environment.toolchain_mounts.len(), 1);
        let settings: toml::Value = toml::from_str(&environment.settings).unwrap();
        assert!(settings["overrides"].as_table().unwrap().is_empty());
    }
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
    let commands = vec![vec!["cargo".into(), "+1.97.0".into(), "test".into()]];
    let environment = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input(&project), &commands)),
    )
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
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands))
        )
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
    let commands = vec![vec!["cargo".into(), "+1.97.0".into(), "test".into()]];
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands))
        )
        .unwrap_err()
        .contains("component metadata escapes")
    );
}
