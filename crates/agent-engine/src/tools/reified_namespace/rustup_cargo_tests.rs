use super::*;
use crate::tools::reified_namespace::{ReifiedMountAccess, ReifiedMountDeclaration};

#[test]
fn compiler_overrides_and_configuration_store_changes_refuse() {
    let (_fixture, proxies, home, project) = installation();
    let inspect = |words: Vec<&str>| {
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((
                &input(&project),
                &[words.into_iter().map(String::from).collect()],
            )),
        )
    };
    for words in [
        vec!["RUSTC=/usr/bin/other-rustc", "cargo", "test"],
        vec!["LD_PRELOAD=/project/plugin.so", "cargo", "--version"],
        vec!["env", "LD_LIBRARY_PATH=/project", "cargo", "--version"],
        vec!["RUSTUP_FORCE_ARG0=rustup", "cargo", "--version"],
        vec!["env", "RUSTC_WRAPPER=/usr/bin/wrapper", "cargo", "test"],
        vec![
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER=/usr/bin/wrapper",
            "cargo",
            "test",
        ],
        vec!["RUSTDOC=/usr/bin/other-rustdoc", "cargo", "doc"],
        vec!["export", "RUSTC=/usr/bin/other-rustc"],
        vec!["printf", "-v", "RUSTC", "/usr/bin/other-rustc"],
        vec!["printf", "-vRUSTC_WRAPPER", "/usr/bin/wrapper"],
        vec!["CARGO_HOME=other", "cargo", "test"],
        vec!["env", "-uRUSTC", "cargo", "test"],
        vec!["env", "--unset=RUSTC_WRAPPER", "cargo", "test"],
        vec!["env", "-0uRUSTDOC", "cargo", "test"],
        vec!["env", "-u", "CARGO_HOME", "cargo", "test"],
        vec!["cargo", "-C", "child", "test"],
        vec!["cargo", "-vCchild", "test"],
        vec!["cargo", "-VCchild", "custom"],
        vec!["cargo", "test", "-vCchild"],
        vec!["rustup", "run", "1.97.0", "cargo", "-vCchild", "test"],
        vec![
            "cargo",
            "--config",
            "build.rustc='/usr/bin/other-rustc'",
            "test",
        ],
        vec![
            "cargo",
            "--config=build.rustc-wrapper='/usr/bin/wrapper'",
            "test",
        ],
        vec![
            "cargo",
            "--config",
            "env.RUSTC='/usr/bin/other-rustc'",
            "test",
        ],
        vec!["cargo", "--config", "include=['other.toml']", "test"],
        vec![
            "cargo",
            "--config",
            "alias.t='test --config build.rustc=other'",
            "t",
        ],
        vec![
            "rustup",
            "run",
            "1.97.0",
            "cargo",
            "--config",
            "build.rustc='other'",
            "test",
        ],
    ] {
        assert!(inspect(words.clone()).is_err(), "{words:?}");
    }
}

#[test]
fn visible_project_configs_are_inspected_and_frozen_without_ambient_parent_reads() {
    let (_fixture, proxies, home, project) = installation();
    let commands = vec![vec!["cargo".into(), "test".into()]];
    let inspect = || {
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands)),
        )
        .unwrap()
        .unwrap()
    };
    let ambient = project.parent().unwrap().join(".cargo");
    std::fs::create_dir(&ambient).unwrap();
    std::fs::write(
        ambient.join("config.toml"),
        "[build]\nrustc = 'unmounted-compiler'\n",
    )
    .unwrap();
    let old = inspect();
    assert_eq!(old.cargo_config_files.len(), 2);
    std::fs::create_dir(project.join(".cargo")).unwrap();
    let config = project.join(".cargo/config.toml");
    std::fs::write(
        &config,
        "[build]\nrustflags = ['--cfg', 'qualified']\n[env]\nPROBE = 'safe'\n",
    )
    .unwrap();
    assert!(
        revalidate(&old)
            .unwrap_err()
            .contains("configuration changed")
    );
    let current = inspect();
    revalidate(&current).unwrap();
    std::fs::write(&config, "[build]\nrustc = 'other'\n").unwrap();
    assert!(
        revalidate(&current)
            .unwrap_err()
            .contains("configuration changed")
    );
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands))
        )
        .unwrap_err()
        .contains("compiler/wrapper")
    );
    std::fs::write(project.join(".cargo/config"), "[build]\nrustflags = []\n").unwrap();
    revalidate(&inspect()).unwrap(); // Cargo's legacy config wins over config.toml.
}

#[test]
fn project_config_env_wrappers_includes_aliases_and_escaping_links_refuse() {
    let (_fixture, proxies, home, project) = installation();
    let config_dir = project.join(".cargo");
    std::fs::create_dir(&config_dir).unwrap();
    for bytes in [
        "[build]\nrustc-wrapper = 'other'\n",
        "[build]\nrustc-workspace-wrapper = 'other'\n",
        "[build]\nrustdoc = 'other'\n",
        "[env]\nRUSTC = { value = 'other', force = true }\n",
        "[env]\nCARGO_BUILD_RUSTC_WRAPPER = 'other'\n",
        "[env]\nRUSTUP_TOOLCHAIN = 'missing'\n",
        "include = ['other.toml']\n",
        "[alias]\ntest = 'build --config build.rustc=other'\n",
    ] {
        std::fs::write(config_dir.join("config.toml"), bytes).unwrap();
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&input(&project), &[vec!["cargo".into(), "test".into()]]))
            )
            .is_err(),
            "{bytes}"
        );
    }
    std::fs::write(config_dir.join("config.toml"), "[build]\nrustflags=[]\n").unwrap();
    let environment = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input(&project), &[vec!["cargo".into(), "test".into()]])),
    )
    .unwrap()
    .unwrap();
    std::fs::remove_file(config_dir.join("config.toml")).unwrap();
    std::fs::remove_dir(&config_dir).unwrap();
    let outside = project.parent().unwrap().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("config.toml"), "[build]\nrustflags=[]\n").unwrap();
    symlink(&outside, &config_dir).unwrap();
    assert!(revalidate(&environment).is_err());
}

#[test]
fn explicit_config_authority_context_and_literal_rewrites_are_checked() {
    let (_fixture, proxies, home, project) = installation();
    std::fs::write(project.join("extra.toml"), "[build]\nrustc='other'\n").unwrap();
    for commands in [
        vec![vec!["cargo", "--config", "extra.toml", "test"]],
        vec![vec!["cargo", "--config", "/unmounted/config.toml", "test"]],
        vec![vec!["cd", "-"], vec!["cargo", "+1.97.0", "test"]],
        vec![
            vec!["printf", "compiler override", ">", ".cargo/config.toml"],
            vec!["cargo", "test"],
        ],
        vec![
            vec!["cp", "other", ".cargo/config.toml"],
            vec!["cargo", "test"],
        ],
    ] {
        let commands: Vec<Vec<String>> = commands
            .into_iter()
            .map(|v| v.into_iter().map(String::from).collect())
            .collect();
        let mut inspection = input(&project);
        inspection.argv = vec![
            "/bin/sh".into(),
            "-c".into(),
            commands
                .iter()
                .map(|words| words.join(" "))
                .collect::<Vec<_>>()
                .join("; "),
        ];
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&inspection, &commands))
            )
            .is_err()
        );
    }
    let commands = vec![vec![
        "cargo".into(),
        "--config".into(),
        "env.PROBE='safe'".into(),
        "test".into(),
        "--".into(),
        "--config".into(),
        "build.rustc=ignored".into(),
    ]];
    discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&input(&project), &commands)),
    )
    .unwrap();
}

#[test]
fn ordinary_cargo_checks_compilers_only_for_compiler_using_commands() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    executable(&system.join("cargo"));
    let path = format!("{}:{}", system.display(), proxies.display());
    for args in [
        vec!["locate-project"],
        vec!["new", "new-project"],
        vec!["init"],
        vec!["help"],
        vec!["version"],
        vec!["read-manifest"],
        vec!["--list"],
        vec!["test", "--help"],
        vec!["metadata", "--no-deps"],
        vec!["help", "test"],
    ] {
        let mut words = vec!["cargo".into()];
        words.extend(args.iter().map(|s| String::from(*s)));
        let environment = discover(&path, &home, None, Some((&input(&project), &[words])))
            .unwrap()
            .unwrap();
        assert!(environment.toolchain_mounts.is_empty());
        assert!(
            !environment
                .command_providers
                .iter()
                .any(|binding| binding.executable == "rustc")
        );
        revalidate(&environment).unwrap();
    }
    for args in [
        vec!["metadata"],
        vec!["clean"],
        vec!["fetch"],
        vec!["tree"],
        vec!["generate-lockfile"],
        vec!["test", "--", "--help"],
    ] {
        let mut words = vec!["cargo".into()];
        words.extend(args.iter().map(|s| String::from(*s)));
        assert!(
            discover(&path, &home, None, Some((&input(&project), &[words])))
                .unwrap_err()
                .contains("mixed system Cargo"),
            "{args:?}"
        );
    }
}

#[test]
fn intermediate_config_on_every_matching_projection_is_inspected() {
    let (_fixture, proxies, home, project) = installation();
    let child = project.join("a/b");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::create_dir(project.join("a/.cargo")).unwrap();
    std::fs::write(
        project.join("a/.cargo/config.toml"),
        "[build]\nrustc='other'\n",
    )
    .unwrap();
    let mut inspection = input(&project);
    inspection.declarations = vec![
        ReifiedMountDeclaration::host("/mnt/project", &project, ReifiedMountAccess::ReadOnly),
        ReifiedMountDeclaration::host("/mnt/b", &child, ReifiedMountAccess::ReadOnly),
    ];
    let commands = vec![
        vec!["cd".into(), "a/b".into()],
        vec!["cargo".into(), "test".into()],
    ];
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&inspection, &commands))
        )
        .unwrap_err()
        .contains("compiler/wrapper")
    );
}

#[test]
fn direct_cargo_frontends_and_custom_help_cannot_skip_config_admission() {
    let (_fixture, proxies, home, project) = installation();
    std::fs::create_dir(project.join(".cargo")).unwrap();
    std::fs::write(
        project.join(".cargo/config.toml"),
        "[build]\nrustc-wrapper='other'\n",
    )
    .unwrap();
    for words in [
        vec!["cargo-clippy", "clippy", "--offline"],
        vec!["rustup", "run", "1.97.0", "cargo-clippy", "--offline"],
        vec!["cargo-fmt", "--all"],
        vec!["cargo", "custom", "--help"],
        vec!["cargo", "help", "custom"],
        vec!["cargo", "help", "--", "-custom"],
    ] {
        let commands = vec![words.iter().map(|s| String::from(*s)).collect()];
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&input(&project), &commands))
            )
            .unwrap_err()
            .contains("compiler/wrapper"),
            "{words:?}"
        );
    }
    std::fs::write(
        project.join(".cargo/config.toml"),
        "[alias]\ncustom='test'\n",
    )
    .unwrap();
    let commands = vec![vec!["cargo".into(), "custom".into(), "--help".into()]];
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), &commands))
        )
        .unwrap_err()
        .contains("selected aliases")
    );
}

#[test]
fn configurable_alias_queries_do_not_skip_compiler_admission() {
    let (_fixture, proxies, home, project) = installation();
    std::fs::create_dir(project.join(".cargo")).unwrap();
    std::fs::write(
        project.join(".cargo/config.toml"),
        "[alias]\nb='test'\ncustom='test'\n",
    )
    .unwrap();
    for words in [
        vec!["cargo", "b", "--help"],
        vec!["cargo", "--version", "custom"],
        vec!["cargo", "--list", "custom"],
        vec!["cargo", "help", "custom"],
        vec!["cargo", "help", "--color", "never", "custom"],
        vec!["cargo", "help", "--config", "env.PROBE='safe'", "custom"],
    ] {
        let commands = vec![words.iter().map(|s| String::from(*s)).collect()];
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&input(&project), &commands))
            )
            .unwrap_err()
            .contains("selected aliases"),
            "{words:?}"
        );
    }
}

#[test]
fn absolute_namespace_cd_inspects_its_delegated_host_configuration() {
    let (_fixture, proxies, home, project) = installation();
    let child = project.join("child");
    std::fs::create_dir_all(child.join(".cargo")).unwrap();
    std::fs::write(
        child.join(".cargo/config.toml"),
        "[build]\nrustc-wrapper='other'\n",
    )
    .unwrap();
    let mut inspection = input(&project);
    inspection.declarations = vec![ReifiedMountDeclaration::host(
        "/mnt/project",
        &project,
        ReifiedMountAccess::ReadOnly,
    )];
    let commands = vec![
        vec!["cd".into(), "/mnt/project/child".into()],
        vec!["cargo".into(), "+1.97.0".into(), "test".into()],
    ];
    assert!(
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&inspection, &commands))
        )
        .unwrap_err()
        .contains("compiler/wrapper")
    );
    std::fs::write(child.join(".cargo/config.toml"), "[build]\nrustflags=[]\n").unwrap();
    let environment = discover(
        proxies.to_str().unwrap(),
        &home,
        None,
        Some((&inspection, &commands)),
    )
    .unwrap()
    .unwrap();
    assert!(
        environment
            .cargo_config_files
            .iter()
            .any(|config| config.path == child.join(".cargo/config.toml"))
    );
}

#[test]
fn namespace_parent_traversal_cannot_choose_an_uninspected_sibling() {
    let (_fixture, proxies, home, project) = installation();
    let sibling = project.parent().unwrap().join("different-backing");
    std::fs::create_dir_all(sibling.join(".cargo")).unwrap();
    std::fs::write(
        sibling.join(".cargo/config.toml"),
        "[build]\nrustc='other'\n",
    )
    .unwrap();
    let mut inspection = input(&project);
    inspection.declarations = vec![
        ReifiedMountDeclaration::host("/mnt/project", &project, ReifiedMountAccess::ReadOnly),
        ReifiedMountDeclaration::host("/mnt/dependency", &sibling, ReifiedMountAccess::ReadOnly),
    ];
    for target in ["../dependency", "/mnt/project/../dependency"] {
        let commands = vec![
            vec!["cd".into(), target.into()],
            vec!["cargo".into(), "+1.97.0".into(), "test".into()],
        ];
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&inspection, &commands))
            )
            .is_err(),
            "{target}"
        );
    }
}

#[test]
fn shell_lookup_definitions_and_new_directory_selection_refuse() {
    let (_fixture, proxies, home, project) = installation();
    for script in [
        "cargo() { true; }; cargo test",
        "sh -c 'cargo() { true; }; cargo test'",
        "sh -c 'cargo test'",
        "cargo () (true); cargo test",
        "function cargo { true; }; cargo test",
        "function cargo() { true; }; cargo test",
        "cd() { true; }; cargo test",
        "car\\\ngo() { true; }; cargo test",
        "alias cargo=true\ncargo test",
        "alias cd=true\ncd child\ncargo test",
        "hash -p /bin/true cargo; cargo test",
        "mkdir -p child/.cargo; echo override > child/.cargo/config.toml; cd child; cargo test",
    ] {
        let mut inspection = input(&project);
        inspection.argv = vec!["/bin/sh".into(), "-c".into(), script.into()];
        let commands = crate::tools::sandbox::shell_commands(script).unwrap();
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&inspection, &commands))
            )
            .is_err(),
            "{script}"
        );
    }
    let literal_path = format!("{}:/usr/bin:/bin", proxies.display());
    for script in [
        "printf 'cargo() { true; }'; cargo test",
        "# cargo() { true; }\ncargo test",
        "printf 'fn main() {}'; cargo test",
        "cat <<'TEXT'\ncargo() { true; }\nTEXT\ncargo test",
        "cat <<'TEXT'\nalias cargo=true\nTEXT\ncargo test",
        "cat <<'TEXT'\nsh -c 'printf literal'\nTEXT\ncargo test",
    ] {
        let mut inspection = input(&project);
        inspection.argv = vec!["/bin/sh".into(), "-c".into(), script.into()];
        // Here-document text is not an execution request.
        let commands = vec![vec!["cargo".into(), "test".into()]];
        discover(&literal_path, &home, None, Some((&inspection, &commands))).unwrap();
    }
    for script in [
        "helper() { true; }; helper",
        "alias helper=true\nhelper",
        "alias cargo=true",
    ] {
        let mut inspection = input(&project);
        inspection.argv = vec!["/bin/sh".into(), "-c".into(), script.into()];
        let commands = crate::tools::sandbox::shell_commands(script).unwrap();
        let environment = discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&inspection, &commands)),
        )
        .unwrap()
        .unwrap();
        assert!(environment.toolchain_mounts.is_empty(), "{script}");
    }
}

#[test]
fn system_cargo_helpers_follow_path_order_and_revalidate() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    executable(&system.join("cargo"));
    executable(&system.join("rustc"));
    for name in ["cargo-fmt", "rustfmt", "cargo-clippy", "clippy-driver"] {
        symlink("rustup", proxies.join(name)).unwrap();
    }
    let path = format!("{}:{}", system.display(), proxies.display());
    for args in [
        vec!["doc"],
        vec!["rustdoc"],
        vec!["d"],
        vec!["test"],
        vec!["fmt"],
        vec!["clippy"],
        vec!["help", "fmt"],
    ] {
        let mut words = vec!["cargo".into()];
        words.extend(args.iter().map(|s| String::from(*s)));
        assert!(
            discover(&path, &home, None, Some((&input(&project), &[words])))
                .unwrap_err()
                .contains("mixed system Cargo"),
            "{args:?}"
        );
    }
    for name in [
        "rustdoc",
        "cargo-fmt",
        "rustfmt",
        "cargo-clippy",
        "clippy-driver",
        "cargo-custom",
    ] {
        executable(&system.join(name));
    }
    for args in [
        vec!["doc"],
        vec!["test"],
        vec!["fmt"],
        vec!["clippy"],
        vec!["help", "custom"],
    ] {
        let mut words = vec!["cargo".into()];
        words.extend(args.iter().map(|s| String::from(*s)));
        let environment = discover(&path, &home, None, Some((&input(&project), &[words])))
            .unwrap()
            .unwrap();
        assert!(environment.toolchain_mounts.is_empty());
        revalidate(&environment).unwrap();
        let helper = environment
            .command_providers
            .iter()
            .find(|p| !matches!(p.executable.as_str(), "cargo" | "rustc"))
            .unwrap();
        let file = system.join(&helper.executable);
        let original = std::fs::read(&file).unwrap();
        std::fs::write(&file, "replaced executable").unwrap();
        assert!(revalidate(&environment).is_err());
        std::fs::write(&file, &original).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(revalidate(&environment).is_err());
        std::fs::write(&file, &original).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    for args in [
        vec!["test", "--lib"],
        vec!["test", "--no-run"],
        vec!["metadata", "--no-deps"],
    ] {
        let mut words = vec!["cargo".into()];
        words.extend(args.iter().map(|s| String::from(*s)));
        let environment = discover(&path, &home, None, Some((&input(&project), &[words])))
            .unwrap()
            .unwrap();
        assert!(
            !environment
                .command_providers
                .iter()
                .any(|p| p.executable == "rustdoc")
        );
    }
}

#[test]
fn global_queries_bind_only_cargo_without_ignoring_selected_aliases() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    executable(&system.join("cargo"));
    for name in ["cargo-fmt", "rustfmt", "cargo-clippy", "clippy-driver"] {
        symlink("rustup", proxies.join(name)).unwrap();
    }
    let path = format!("{}:{}", system.display(), proxies.display());
    for flag in [
        "--help",
        "-h",
        "--version",
        "-V",
        "--list",
        "-vh",
        "-Vv",
        "-hV",
        "-hZunstable-options",
        "-VZunstable-options",
        "-vVZunstable-options",
        "-hX",
        "-hCmissing",
        "-VhZunstable-options",
    ] {
        for command in [
            "fmt", "clippy", "custom", "test", "help", "b", "c", "d", "t", "r", "rm",
        ] {
            let words = vec!["cargo".into(), flag.into(), command.into()];
            if matches!(command, "b" | "c" | "d" | "t" | "r" | "rm")
                && !matches!(
                    flag,
                    "--help"
                        | "-h"
                        | "-vh"
                        | "-hV"
                        | "-hZunstable-options"
                        | "-hX"
                        | "-hCmissing"
                        | "-VhZunstable-options"
                )
            {
                assert!(
                    discover(&path, &home, None, Some((&input(&project), &[words])))
                        .unwrap_err()
                        .contains("default aliases")
                );
                continue;
            }
            let environment = discover(&path, &home, None, Some((&input(&project), &[words])))
                .unwrap()
                .unwrap();
            assert!(environment.toolchain_mounts.is_empty());
            assert_eq!(environment.command_providers.len(), 1);
            assert_eq!(environment.command_providers[0].executable, "cargo");
            revalidate(&environment).unwrap();
            if !matches!(command, "test" | "help") {
                let pure_help = matches!(
                    flag,
                    "--help"
                        | "-h"
                        | "-vh"
                        | "-hV"
                        | "-hZunstable-options"
                        | "-hX"
                        | "-hCmissing"
                        | "-VhZunstable-options"
                );
                assert_eq!(environment.cargo_config_files.is_empty(), pure_help);
                let config = project.join(".cargo");
                std::fs::create_dir_all(&config).unwrap();
                std::fs::write(
                    config.join("config.toml"),
                    format!("[alias]\n{command}='other'\n"),
                )
                .unwrap();
                assert_eq!(revalidate(&environment).is_ok(), pure_help);
                let words = vec!["cargo".into(), flag.into(), command.into()];
                assert_eq!(
                    discover(&path, &home, None, Some((&input(&project), &[words]))).is_ok(),
                    pure_help
                );
                std::fs::remove_file(config.join("config.toml")).unwrap();
                let words = vec![
                    "cargo".into(),
                    "--config".into(),
                    format!("alias.{command}='other'"),
                    flag.into(),
                    command.into(),
                ];
                assert_eq!(
                    discover(&path, &home, None, Some((&input(&project), &[words]))).is_ok(),
                    pure_help
                );
            }
        }
    }
    for args in [
        vec!["--color", "--help", "fmt"],
        vec!["--", "--help", "fmt"],
        vec!["fmt", "--", "--help"],
    ] {
        let mut words = vec!["cargo".into()];
        words.extend(args.iter().map(|s| String::from(*s)));
        assert!(discover(&path, &home, None, Some((&input(&project), &[words]))).is_err());
    }
    std::fs::write(
        project.join(".cargo/config.toml"),
        "[alias]\ncustom='test'\nb='test'\nfmt='test'\n",
    )
    .unwrap();
    for args in [
        vec!["--version", "--help", "custom"],
        vec!["--help", "--version", "b"],
        vec!["--list", "--help", "fmt"],
        vec!["-Vh", "custom"],
    ] {
        let mut words = vec!["cargo".into()];
        words.extend(args.into_iter().map(String::from));
        let environment = discover(&path, &home, None, Some((&input(&project), &[words])))
            .unwrap()
            .unwrap();
        assert_eq!(environment.command_providers.len(), 1);
        assert!(environment.cargo_config_files.is_empty());
        revalidate(&environment).unwrap();
    }
}

#[test]
fn proxy_global_queries_need_only_cargo_without_frontend_components() {
    let (_fixture, proxies, home, project) = installation();
    for flag in [
        "--help",
        "-h",
        "--version",
        "-V",
        "--list",
        "-vh",
        "-Vv",
        "-hV",
        "-hZunstable-options",
        "-VZunstable-options",
        "-vVZunstable-options",
        "-hX",
        "-hCmissing",
        "-VhZunstable-options",
    ] {
        for command in ["fmt", "clippy", "custom"] {
            let words = vec![
                "cargo".into(),
                flag.into(),
                command.into(),
                "--target=absent".into(),
            ];
            let environment = discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&input(&project), &[words])),
            )
            .unwrap()
            .unwrap();
            assert_eq!(environment.toolchain_mounts.len(), 1);
            assert!(!environment.executable_hashes.iter().any(|(p, _)| matches!(
                p.file_name().and_then(|n| n.to_str()),
                Some("cargo-fmt" | "rustfmt" | "cargo-clippy" | "clippy-driver")
            )));
            revalidate(&environment).unwrap();
        }
    }
    for command in ["fmt", "clippy"] {
        let words = vec!["cargo".into(), command.into()];
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&input(&project), &[words]))
            )
            .is_err()
        );
    }
}

#[test]
fn auxiliary_help_preserves_frontends_without_requiring_compilers() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    for name in ["cargo", "cargo-fmt", "cargo-clippy"] {
        executable(&system.join(name));
    }
    for name in ["cargo-fmt", "rustfmt", "cargo-clippy", "clippy-driver"] {
        symlink("rustup", proxies.join(name)).unwrap();
    }
    let path = format!("{}:{}", system.display(), proxies.display());
    for frontend in ["fmt", "clippy"] {
        for args in [
            vec![frontend, "--help"],
            vec![frontend, "-h"],
            vec!["help", frontend],
        ] {
            let mut words = vec!["cargo".into()];
            words.extend(args.iter().map(|s| String::from(*s)));
            let environment = discover(&path, &home, None, Some((&input(&project), &[words])))
                .unwrap()
                .unwrap();
            assert!(!super::super::cargo::uses_compiler(
                &args.iter().map(|s| String::from(*s)).collect::<Vec<_>>()
            ));
            assert!(environment.toolchain_mounts.is_empty());
            assert_eq!(
                environment
                    .command_providers
                    .iter()
                    .map(|p| p.executable.as_str())
                    .collect::<Vec<_>>(),
                vec![format!("cargo-{frontend}"), "cargo".into()]
            );
            revalidate(&environment).unwrap();
            let helper = system.join(format!("cargo-{frontend}"));
            let original = std::fs::read(&helper).unwrap();
            std::fs::write(&helper, "replaced helper").unwrap();
            assert!(revalidate(&environment).is_err());
            std::fs::write(&helper, original).unwrap();
            let config = project.join(".cargo");
            std::fs::create_dir_all(&config).unwrap();
            std::fs::write(
                config.join("config.toml"),
                format!("[alias]\n{frontend}='custom'\n"),
            )
            .unwrap();
            let mut selected = vec!["cargo".into()];
            selected.extend(args.iter().map(|s| String::from(*s)));
            assert!(discover(&path, &home, None, Some((&input(&project), &[selected]))).is_err());
            std::fs::remove_file(config.join("config.toml")).unwrap();
            std::fs::write(config.join("config.toml"), "[build]\nrustc='other'\n").unwrap();
            let mut selected = vec!["cargo".into()];
            selected.extend(args.iter().map(|s| String::from(*s)));
            assert!(discover(&path, &home, None, Some((&input(&project), &[selected]))).is_err());
            std::fs::remove_file(config.join("config.toml")).unwrap();
            let inline = vec![
                "cargo".into(),
                "--config".into(),
                format!("alias.{frontend}='custom'"),
                frontend.into(),
                "--help".into(),
            ];
            assert!(discover(&path, &home, None, Some((&input(&project), &[inline]))).is_err());
        }
        for args in [
            vec![frontend],
            vec![frontend, "--", "--help"],
            vec![frontend, "--manifest-path", "--help"],
        ] {
            let mut words = vec!["cargo".into()];
            words.extend(args.iter().map(|s| String::from(*s)));
            assert!(discover(&path, &home, None, Some((&input(&project), &[words]))).is_err());
        }
    }
    for args in [vec!["clippy", "--version"], vec!["clippy", "-V"]] {
        let mut words = vec!["cargo".into()];
        words.extend(args.into_iter().map(String::from));
        let environment = discover(&path, &home, None, Some((&input(&project), &[words])))
            .unwrap()
            .unwrap();
        assert_eq!(environment.command_providers.len(), 2);
        assert!(
            environment
                .command_providers
                .iter()
                .any(|p| p.executable == "cargo-clippy")
        );
        revalidate(&environment).unwrap();
    }
    assert!(
        discover(
            &path,
            &home,
            None,
            Some((
                &input(&project),
                &[vec!["cargo".into(), "fmt".into(), "--version".into()]]
            ))
        )
        .is_err()
    );
}

#[test]
fn executable_heredoc_lookup_changes_refuse_but_quoted_examples_remain_literal() {
    let (_fixture, proxies, home, project) = installation();
    for script in [
        "cat <<EOF\n'$(cargo() { true; }; cargo test)'\nEOF\ncargo test",
        "cat <<EOF\n$(cargo() { true; }; cargo test)\nEOF",
        "cat <<EOF\n'$(RUSTC=other-rustc cargo test)'\nEOF",
    ] {
        let mut inspection = input(&project);
        inspection.argv = vec!["/bin/sh".into(), "-c".into(), script.into()];
        let commands = crate::tools::sandbox::shell_commands(script).unwrap();
        assert!(
            discover(
                proxies.to_str().unwrap(),
                &home,
                None,
                Some((&inspection, &commands))
            )
            .is_err(),
            "{script}"
        );
    }
    let script = "cat <<'EOF'\n'$(cargo() { true; }; cargo test)'\nEOF\ncargo test";
    let mut inspection = input(&project);
    inspection.argv = vec!["/bin/sh".into(), "-c".into(), script.into()];
    let commands = vec![vec!["cargo".into(), "test".into()]];
    discover(
        &format!("{}:/usr/bin:/bin", proxies.display()),
        &home,
        None,
        Some((&inspection, &commands)),
    )
    .unwrap();
}
