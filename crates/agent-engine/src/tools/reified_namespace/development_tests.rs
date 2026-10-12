use super::*;

#[test]
fn native_system_rust_provider_precedes_later_rustup_proxies() {
    let (Some(proxy), Some(runtime)) = (
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
    ) else {
        eprintln!("skipping native system Rust provider: explicit owned runtime inputs absent");
        return;
    };
    assert!(probe_linux_reification().is_selectable());
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let system = root.join("system-runtime");
    let project = root.join("project");
    let home = root.join("rustup-without-settings");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir(&home).unwrap();
    let proxy = PathBuf::from(proxy);
    // Genuine runtime executables act as an ordinary installation in this owned fixture.
    assert!(
        Command::new("/bin/cp")
            .args(["-a", "--reflink=auto"])
            .arg(runtime)
            .arg(&system)
            .status()
            .unwrap()
            .success()
    );
    let manifest = "[package]\nname = \"alan_system_provider_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";
    let source = "#[test] fn actual_compiler() { assert_eq!(2 + 2, 4); }\n";
    let lock =
        "version = 4\n[[package]]\nname = \"alan_system_provider_fixture\"\nversion = \"0.1.0\"\n";
    std::fs::write(project.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(project.join("src/lib.rs"), source).unwrap();
    std::fs::write(project.join("Cargo.lock"), lock).unwrap();
    let path = format!("{}/bin:{}:/usr/bin:/bin", system.display(), proxy.display());
    let commands = vec![
        vec!["rustc".into(), "--version".into()],
        vec![
            "cargo".into(),
            "test".into(),
            "--offline".into(),
            "--locked".into(),
        ],
    ];
    let mut input = ReifiedNamespacePlanInput::new(
        vec![ReifiedMountDeclaration::host(&project, &project, ReifiedMountAccess::ReadOnly)],
        &project,
        vec!["/bin/sh".into(), "-c".into(),
            "set -eu; rustc --version; cargo test --offline --locked; test -d \"$CARGO_TARGET_DIR/debug/deps\"; if (: >> src/lib.rs); then exit 43; fi".into()],
        NetworkPosture::Deny,
    );
    let environment = rustup::discover(&path, &home, None, Some((&input, &commands)))
        .unwrap()
        .unwrap();
    assert!(environment.toolchain_mounts.is_empty());
    assert!(environment.metadata_hashes.is_empty());
    let mut substrate = default_execution_substrate();
    substrate.push(ReifiedExecutionSubstrateMount::new(&system, &system));
    substrate.extend(environment.proxy_mounts.iter().cloned());
    input = input
        .with_scratch_tmp_namespace_path("/.alan-system-provider-tmp")
        .with_execution_substrate(substrate)
        .with_command_path(path);
    input.rustup = Some(environment);
    let plan = ReifiedNamespacePlan::derive(input).unwrap();
    let result = LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
        .run(&plan)
        .unwrap();
    assert_eq!(result.exit_code, 0, "{} {}", result.stdout, result.stderr);
    assert!(result.stdout.contains("1 passed"));
    assert_eq!(
        std::fs::read_to_string(project.join("Cargo.toml")).unwrap(),
        manifest
    );
    assert_eq!(
        std::fs::read_to_string(project.join("Cargo.lock")).unwrap(),
        lock
    );
    assert_eq!(
        std::fs::read_to_string(project.join("src/lib.rs")).unwrap(),
        source
    );
    assert!(!project.join("target").exists());
    assert!(!home.join("settings.toml").exists());
    let rustdoc = system.join("bin/rustdoc");
    let rustdoc_bytes = std::fs::read(&rustdoc).unwrap();
    let rustdoc_permissions = std::fs::metadata(&rustdoc).unwrap().permissions();
    std::fs::remove_file(&rustdoc).unwrap();
    for command in ["doc", "test"] {
        let mut inspection = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                &project,
                &project,
                ReifiedMountAccess::ReadOnly,
            )],
            &project,
            vec![
                "/bin/sh".into(),
                "-c".into(),
                format!("cargo {command} --offline --locked"),
            ],
            NetworkPosture::Deny,
        );
        let commands = vec![vec![
            "cargo".into(),
            command.into(),
            "--offline".into(),
            "--locked".into(),
        ]];
        let path = format!("{}/bin:{}:/usr/bin:/bin", system.display(), proxy.display());
        let environment = rustup::discover(&path, &home, None, Some((&inspection, &commands)));
        if let Ok(Some(environment)) = environment {
            let mut substrate = default_execution_substrate();
            substrate.push(ReifiedExecutionSubstrateMount::new(&system, &system));
            substrate.extend(environment.proxy_mounts.iter().cloned());
            inspection = inspection
                .with_scratch_tmp_namespace_path("/.alan-system-provider-tmp")
                .with_execution_substrate(substrate)
                .with_command_path(path);
            inspection.rustup = Some(environment);
            let bad_plan = ReifiedNamespacePlan::derive(inspection).unwrap();
            let result =
                LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
                    .run(&bad_plan)
                    .unwrap();
            panic!(
                "missing ordinary rustdoc reached cargo {command}: exit {}, stderr {}",
                result.exit_code, result.stderr
            );
        }
        assert!(environment.unwrap_err().contains("mixed system Cargo"));
    }
    assert!(rustup::revalidate(plan.rustup.as_ref().unwrap()).is_err());
    std::fs::write(&rustdoc, rustdoc_bytes).unwrap();
    std::fs::set_permissions(&rustdoc, rustdoc_permissions).unwrap();
    eprintln!(
        "ordinary Cargo doc/test refuse later proxy rustdoc; selected helper loss refuses before execution"
    );
    std::fs::remove_file(system.join("bin/rustc")).unwrap();
    let error = LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
        .run(&plan)
        .unwrap_err()
        .to_string();
    assert!(error.contains("provider changed"), "{error}");
    let commands = vec![
        vec!["cargo".into(), "metadata".into(), "--no-deps".into()],
        vec!["cargo".into(), "locate-project".into()],
        vec!["cargo".into(), "new".into()],
        vec!["cargo".into(), "init".into()],
        vec!["cargo".into(), "fmt".into(), "--help".into()],
        vec!["cargo".into(), "fmt".into(), "-h".into()],
        vec!["cargo".into(), "help".into(), "fmt".into()],
        vec!["cargo".into(), "clippy".into(), "--help".into()],
        vec!["cargo".into(), "clippy".into(), "-h".into()],
        vec!["cargo".into(), "help".into(), "clippy".into()],
        vec!["cargo".into(), "clippy".into(), "--version".into()],
        vec!["cargo".into(), "clippy".into(), "-V".into()],
    ];
    for name in ["rustfmt", "clippy-driver"] {
        std::fs::remove_file(system.join("bin").join(name)).unwrap();
    }
    let path = format!("{}/bin:{}:/usr/bin:/bin", system.display(), proxy.display());
    let mut input = ReifiedNamespacePlanInput::new(
        vec![ReifiedMountDeclaration::host(&project, &project, ReifiedMountAccess::ReadOnly)],
        &project,
        vec!["/bin/sh".into(), "-c".into(),
            "set -eu; cargo metadata --no-deps --format-version 1; cargo locate-project; cargo new --vcs none \"$TMPDIR/new-provider-project\"; cargo init --vcs none \"$TMPDIR/init-provider-project\"; cargo fmt --help; cargo fmt -h; cargo help fmt; cargo clippy --help; cargo clippy -h; cargo help clippy; cargo clippy --version; cargo clippy -V".into()],
        NetworkPosture::Deny,
    );
    let environment = rustup::discover(&path, &home, None, Some((&input, &commands)))
        .unwrap()
        .unwrap();
    assert!(environment.toolchain_mounts.is_empty());
    assert!(!environment.cargo_config_files.is_empty());
    assert!(
        environment
            .cargo_config_files
            .iter()
            .all(|file| file.digest.is_none())
    );
    assert!(
        !environment
            .command_providers
            .iter()
            .any(|binding| binding.executable == "rustc")
    );
    let mut substrate = default_execution_substrate();
    substrate.push(ReifiedExecutionSubstrateMount::new(&system, &system));
    substrate.extend(environment.proxy_mounts.iter().cloned());
    input = input
        .with_scratch_tmp_namespace_path("/.alan-system-provider-tmp")
        .with_execution_substrate(substrate)
        .with_command_path(&path);
    input.rustup = Some(environment);
    let free_plan = ReifiedNamespacePlan::derive(input).unwrap();
    let free = LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
        .run(&free_plan)
        .unwrap();
    assert_eq!(free.exit_code, 0, "{} {}", free.stdout, free.stderr);
    assert!(free.stdout.contains("alan_system_provider_fixture"));
    for name in ["cargo-fmt", "cargo-clippy"] {
        std::fs::remove_file(system.join("bin").join(name)).unwrap();
    }
    let commands: Vec<Vec<String>> = ["--help", "-h", "--version", "-V", "--list"]
        .iter()
        .flat_map(|flag| {
            ["fmt", "clippy", "custom"].map(|name| {
                vec![
                    "cargo".into(),
                    (*flag).into(),
                    name.into(),
                    "--target=absent".into(),
                ]
            })
        })
        .collect();
    let script = format!(
        "set -eu; {}",
        commands
            .iter()
            .map(|words| words.join(" "))
            .collect::<Vec<_>>()
            .join("; ")
    );
    let mut input = ReifiedNamespacePlanInput::new(
        vec![ReifiedMountDeclaration::host(
            &project,
            &project,
            ReifiedMountAccess::ReadOnly,
        )],
        &project,
        vec!["/bin/sh".into(), "-c".into(), script],
        NetworkPosture::Deny,
    );
    let environment = rustup::discover(&path, &home, None, Some((&input, &commands)))
        .unwrap()
        .unwrap();
    assert_eq!(environment.command_providers.len(), 1);
    assert!(environment.toolchain_mounts.is_empty());
    let mut substrate = default_execution_substrate();
    substrate.push(ReifiedExecutionSubstrateMount::new(&system, &system));
    substrate.extend(environment.proxy_mounts.iter().cloned());
    input = input
        .with_scratch_tmp_namespace_path("/.alan-system-provider-tmp")
        .with_execution_substrate(substrate)
        .with_command_path(path);
    input.rustup = Some(environment);
    let query_plan = ReifiedNamespacePlan::derive(input).unwrap();
    let queries = LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
        .run(&query_plan)
        .unwrap();
    assert_eq!(
        queries.exit_code, 0,
        "{} {}",
        queries.stdout, queries.stderr
    );
    eprintln!(
        "fifteen confined global queries pass with ordinary frontends/compilers removed and only Cargo bound"
    );
    eprintln!(
        "system Cargo metadata --no-deps/locate-project/new/init pass with only a later proxy rustc and no projected Rustup runtime"
    );
    eprintln!(
        "eight ordinary fmt/Clippy help/version queries pass with later proxy compilers/drivers and inspected config absence"
    );
    eprintln!(
        "system-first genuine Cargo/Rustc compile passed; Rustup metadata/runtime count 0; source/lock unchanged"
    );
}

#[test]
fn native_cargo_compiler_overrides_refuse_before_any_command_effects() {
    let (Some(proxy), Some(runtime)) = (
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
    ) else {
        eprintln!("skipping native Cargo override refusal: explicit owned runtime inputs absent");
        return;
    };
    assert!(probe_linux_reification().is_selectable());
    let proxy = PathBuf::from(proxy);
    let runtime = PathBuf::from(runtime);
    let home = runtime.parent().unwrap().parent().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let project = std::fs::canonicalize(fixture.path()).unwrap();
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir(project.join(".cargo")).unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname='alan_override_fixture'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn answer() -> u32 { 4 }\n").unwrap();
    let config = project.join(".cargo/config.toml");
    let extra = project.join("extra.toml");
    for (case, script, words, contents, extra_contents) in [
        (
            "compiler env",
            "printf effect > marker; RUSTC=/usr/bin/true cargo test --offline",
            vec!["RUSTC=/usr/bin/true", "cargo", "test", "--offline"],
            None,
            None,
        ),
        (
            "inline config",
            "printf effect > marker; cargo --config 'build.rustc=\"/usr/bin/true\"' test --offline",
            vec!["cargo", "--config", "build.rustc=\"/usr/bin/true\"", "test"],
            None,
            None,
        ),
        (
            "project config",
            "printf effect > marker; cargo test --offline",
            vec!["cargo", "test"],
            Some("[build]\nrustc='/usr/bin/true'\n"),
            None,
        ),
        (
            "direct Clippy",
            "printf effect > marker; cargo-clippy clippy --offline",
            vec!["cargo-clippy", "clippy", "--offline"],
            Some("[build]\nrustc-wrapper='/usr/bin/true'\n"),
            None,
        ),
        (
            "explicit config",
            "printf effect > marker; cargo --config extra.toml test --offline",
            vec!["cargo", "--config", "extra.toml", "test"],
            None,
            Some("[build]\nrustc='/usr/bin/true'\n"),
        ),
        (
            "literal config rewrite",
            "printf effect > marker; printf '[build]\\nrustc=\"/usr/bin/true\"\\n' >.cargo/config.toml; cargo test --offline",
            vec!["cargo", "test"],
            None,
            None,
        ),
    ] {
        for path in [&config, &extra] {
            if path.exists() {
                std::fs::remove_file(path).unwrap();
            }
        }
        if let Some(contents) = contents {
            std::fs::write(&config, contents).unwrap();
        }
        if let Some(contents) = extra_contents {
            std::fs::write(&extra, contents).unwrap();
        }
        let mut input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                &project,
                &project,
                ReifiedMountAccess::ReadWrite,
            )],
            &project,
            vec!["/bin/sh".into(), "-c".into(), script.into()],
            NetworkPosture::Deny,
        );
        let commands = vec![words.into_iter().map(String::from).collect()];
        let environment = rustup::discover(
            proxy.to_str().unwrap(),
            home,
            Some(runtime.file_name().unwrap().to_str().unwrap().into()),
            Some((&input, &commands)),
        );
        if let Ok(Some(environment)) = environment {
            let mut substrate = default_execution_substrate();
            substrate.extend(
                environment
                    .proxy_mounts
                    .iter()
                    .chain(&environment.toolchain_mounts)
                    .cloned(),
            );
            input = input
                .with_execution_substrate(substrate)
                .with_command_path(format!("{}:/usr/bin:/bin", proxy.display()));
            input.rustup = Some(environment);
            let plan = ReifiedNamespacePlan::derive(input).unwrap();
            let result =
                LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
                    .run(&plan)
                    .unwrap();
            panic!(
                "unsupported {case} entered native execution: exit {}, marker {}, stderr {}",
                result.exit_code,
                project.join("marker").exists(),
                result.stderr
            );
        }
        assert!(environment.is_err(), "{case}");
        assert!(!project.join("marker").exists(), "{case}");
        eprintln!("native {case} refused before marker/user effects");
    }
}

#[test]
fn native_remapped_rustup_overrides_match_host_selection() {
    let (Some(proxy), Some(runtime)) = (
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
    ) else {
        eprintln!("skipping native Rustup remap: explicit owned runtime inputs absent");
        return;
    };
    assert!(probe_linux_reification().is_selectable());
    let proxy = PathBuf::from(proxy);
    let runtime = PathBuf::from(runtime);
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let project = root.join("project");
    let home = root.join("rustup");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(home.join("toolchains")).unwrap();
    // Copy into fixture-owned metadata; never mutate the installed Host tree.
    assert!(
        Command::new("/bin/cp")
            .arg("-a")
            .arg("--reflink=auto")
            .arg(&runtime)
            .arg(home.join("toolchains").join(runtime.file_name().unwrap()))
            .status()
            .unwrap()
            .success()
    );
    let manifest =
        "[package]\nname = \"alan_remap_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";
    let source = "#[test] fn expected_answer() { assert_eq!(2 + 2, 4); }\n";
    std::fs::write(project.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(project.join("src/lib.rs"), source).unwrap();
    let lock = "version = 4\n[[package]]\nname = \"alan_remap_fixture\"\nversion = \"0.1.0\"\n";
    std::fs::write(project.join("Cargo.lock"), lock).unwrap();
    let runtime_bytes = std::fs::read(runtime.join("bin/rustc")).unwrap();
    let alias = root.join("project-alias");
    std::os::unix::fs::symlink(&project, &alias).unwrap();
    for (override_path, selector, file_selector, task_root) in [
        (&project, "1.97.0", Some("missing"), &project),
        (&root, "1.97.0", None, &project),
        (&root, "missing", Some("1.97.0"), &project),
        (&project, "1.97.0", Some("missing"), &alias),
    ] {
        let settings = format!(
            "version = \"12\"\ndefault_toolchain = \"missing\"\n[overrides]\n{} = {selector:?}\n",
            toml::Value::String(override_path.to_str().unwrap().into())
        );
        std::fs::write(home.join("settings.toml"), &settings).unwrap();
        let file = project.join("rust-toolchain");
        if let Some(selector) = file_selector {
            std::fs::write(&file, format!("{selector}\n")).unwrap();
        } else {
            std::fs::remove_file(&file).unwrap();
        }
        let host = Command::new(proxy.join("rustc"))
            .arg("--version")
            .current_dir(task_root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("RUSTUP_HOME", &home)
            .env("RUSTUP_AUTO_INSTALL", "0")
            .output()
            .unwrap();
        assert!(
            host.status.success(),
            "{}",
            String::from_utf8_lossy(&host.stderr)
        );
        let mut input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host("/mnt/project", task_root, ReifiedMountAccess::ReadOnly)],
            task_root,
            vec!["/bin/sh".into(), "-c".into(),
                "set -eu; rustc --version; cargo test --offline --locked; test -d \"$CARGO_TARGET_DIR/debug/deps\"; if (: >> src/lib.rs); then exit 43; fi".into()],
            NetworkPosture::Deny,
        );
        let commands = vec![
            vec!["rustc".into(), "--version".into()],
            vec![
                "cargo".into(),
                "test".into(),
                "--offline".into(),
                "--locked".into(),
            ],
        ];
        let environment = rustup::discover(
            proxy.to_str().unwrap(),
            &home,
            None,
            Some((&input, &commands)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(environment.toolchain_mounts.len(), 1);
        let mut substrate = default_execution_substrate();
        substrate.extend(
            environment
                .proxy_mounts
                .iter()
                .chain(&environment.toolchain_mounts)
                .cloned(),
        );
        input = input
            .with_scratch_tmp_namespace_path("/.alan-rustup-test-tmp")
            .with_execution_substrate(substrate)
            .with_command_path(format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", proxy.display()));
        input.rustup = Some(environment);
        let plan = ReifiedNamespacePlan::derive(input).unwrap();
        let result =
            LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
                .run(&plan)
                .unwrap();
        assert_eq!(result.exit_code, 0, "{} {}", result.stdout, result.stderr);
        assert!(
            result
                .stdout
                .starts_with(std::str::from_utf8(&host.stdout).unwrap())
        );
        assert!(result.stdout.contains("1 passed"));
        eprintln!(
            "remapped override {} -> {selector}; Host and confined {}compile passed; selected runtime count 1",
            override_path.display(),
            std::str::from_utf8(&host.stdout).unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(home.join("settings.toml")).unwrap(),
            settings
        );
        assert_eq!(
            std::fs::read_to_string(project.join("Cargo.toml")).unwrap(),
            manifest
        );
        assert_eq!(
            std::fs::read_to_string(project.join("src/lib.rs")).unwrap(),
            source
        );
        assert!(!project.join("target").exists());
        assert_eq!(
            std::fs::read_to_string(project.join("Cargo.lock")).unwrap(),
            lock
        );
    }
    assert_eq!(
        std::fs::read(runtime.join("bin/rustc")).unwrap(),
        runtime_bytes
    );
}

#[test]
fn native_readonly_rust_build_uses_private_output_and_project_config() {
    let (Some(proxy), Some(runtime)) = (
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
    ) else {
        eprintln!("skipping native Rust fixture: explicit task-owned proxy/runtime inputs absent");
        return;
    };
    assert!(
        probe_linux_reification().is_selectable(),
        "native Rust acceptance requires namespace readiness"
    );
    let proxy = PathBuf::from(proxy);
    let runtime = PathBuf::from(runtime);
    let mut names = std::fs::read_dir(&proxy)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            "cargo",
            "cargo-clippy",
            "cargo-fmt",
            "clippy-driver",
            "rustc",
            "rustdoc",
            "rustfmt",
            "rustup",
        ]
        .map(std::ffi::OsString::from)
    );
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let dependency = fixture.path().join("dependency");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join(".cargo")).unwrap();
    std::fs::create_dir_all(dependency.join("src")).unwrap();
    let files = [
        (
            project.join("Cargo.toml"),
            "[package]\nname = \"alan_private_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nalan_private_dep = { path = \"../dependency\" }\n",
        ),
        (
            project.join("Cargo.lock"),
            "version = 4\n[[package]]\nname = \"alan_private_fixture\"\nversion = \"0.1.0\"\ndependencies = [\"alan_private_dep\"]\n[[package]]\nname = \"alan_private_dep\"\nversion = \"0.1.0\"\n",
        ),
        (
            project.join(".cargo/config.toml"),
            "[build]\nrustflags = [\"--cfg\", \"alan_private_fixture\", \"--check-cfg\", \"cfg(alan_private_fixture)\"]\nrustdocflags = [\"--cfg\", \"alan_private_fixture\", \"--check-cfg\", \"cfg(alan_private_fixture)\"]\n",
        ),
        (
            project.join("src/lib.rs"),
            "#[cfg(not(alan_private_fixture))]\ncompile_error!(\"project configuration missing\");\n/// ```\n/// assert_eq!(alan_private_fixture::answer(), 3);\n/// ```\npub fn answer() -> u32 { alan_private_dep::answer() }\n#[test]\nfn selected_dependency() { assert_eq!(answer(), 3); }\n",
        ),
        (
            dependency.join("Cargo.toml"),
            "[package]\nname = \"alan_private_dep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        ),
        (
            dependency.join("src/lib.rs"),
            "pub fn answer() -> u32 { 3 }\n",
        ),
    ];
    for (path, bytes) in &files {
        std::fs::write(path, bytes).unwrap();
    }
    let script = r#"
set -eu
RUSTUP_TOOLCHAIN=/opt/alan-rust
export RUSTUP_TOOLCHAIN
test "$RUSTUP_AUTO_INSTALL" = 0
test "$CARGO_HOME" = /tmp/.alan-env-0/cargo
test "$CARGO_TARGET_DIR" = /tmp/.alan-env-0/target
test ! -e "$CARGO_HOME/credentials.toml"
test ! -e /home
rustc --version
cargo --version
rustdoc --version
cargo test --offline --locked
test -d "$CARGO_TARGET_DIR/debug/deps"
if (: >> /opt/alan-rust/bin/rustc); then exit 42; fi
if (: >> /mnt/project/src/lib.rs); then exit 43; fi
printf 'private-rust-build-ok\n'
"#;
    let mut substrate = default_execution_substrate();
    substrate.extend([
        ReifiedExecutionSubstrateMount::new("/opt/alan-proxies", proxy),
        ReifiedExecutionSubstrateMount::new("/opt/alan-rust", &runtime),
    ]);
    let plan = ReifiedNamespacePlan::derive(
        ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    &project,
                    ReifiedMountAccess::ReadOnly,
                ),
                ReifiedMountDeclaration::host(
                    "/mnt/dependency",
                    &dependency,
                    ReifiedMountAccess::ReadOnly,
                ),
            ],
            &project,
            vec!["/bin/sh".into(), "-c".into(), script.into()],
            NetworkPosture::Deny,
        )
        .with_execution_substrate(substrate)
        .with_command_path("/opt/alan-proxies:/usr/bin:/bin:/usr/sbin:/sbin"),
    )
    .unwrap();
    let runtime_bytes = std::fs::read(runtime.join("bin/rustc")).unwrap();
    let result = LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
        .run(&plan)
        .unwrap();
    assert_eq!(result.exit_code, 0, "{} {}", result.stdout, result.stderr);
    assert!(result.stdout.contains("rustc 1.97.0"));
    assert!(result.stdout.contains("cargo 1.97.0"));
    assert!(result.stdout.contains("rustdoc 1.97.0"));
    assert_eq!(result.stdout.matches("1 passed").count(), 2);
    assert!(result.stdout.ends_with("private-rust-build-ok\n"));
    eprintln!(
        "native Rust fixture stdout:\n{}stderr:\n{}",
        result.stdout, result.stderr
    );
    for (path, bytes) in &files {
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            *bytes,
            "{}",
            path.display()
        );
    }
    assert_eq!(
        std::fs::read(runtime.join("bin/rustc")).unwrap(),
        runtime_bytes
    );
    assert!(!project.join("target").exists());
    assert!(!dependency.join("target").exists());
    assert_eq!(
        std::fs::read_dir(project.join(".cargo")).unwrap().count(),
        1
    );
}

#[test]
fn native_private_environment_keeps_delegated_scratch_paths_separate() {
    if !probe_linux_reification().is_selectable() {
        eprintln!("skipping native private environment: namespace unavailable");
        return;
    }
    let project = tempfile::tempdir().unwrap();
    let delegated = tempfile::tempdir().unwrap();
    let ambient = tempfile::tempdir().unwrap();
    std::fs::write(delegated.path().join("keep"), "delegated").unwrap();
    std::fs::write(ambient.path().join("credentials.toml"), "private canary").unwrap();
    let script = r#"
set -eu
test "${HOME-unset}" = /tmp/.alan-env-1/home
test "$CARGO_HOME" = /tmp/.alan-env-1/cargo
test "$RUSTUP_HOME" = /tmp/.alan-env-1/rustup
test "$TMPDIR" = /tmp/.alan-env-1/tmp
test "$XDG_CACHE_HOME" = /tmp/.alan-env-1/cache
test "$RUSTUP_AUTO_INSTALL" = 0
test -z "${CARGO_TARGET_DIR+x}"
test "$(cat /tmp/.alan-env-0/keep)" = delegated
test ! -e "$1/credentials.toml"
test -r /proc/self/status
test ! -e "/proc/$2"
test -d /proc/1/root
test ! -e "/proc/1/root$1/credentials.toml"
if (: >> /proc/self/comm); then exit 44; fi
for directory in "$HOME" "$CARGO_HOME" "$RUSTUP_HOME" "$TMPDIR" "$XDG_CACHE_HOME"; do
  test ! -e "$directory/marker"
  printf private > "$directory/marker"
done
printf 'private-environment-ok\n'
"#;
    let plan = ReifiedNamespacePlan::derive(ReifiedNamespacePlanInput::new(
        vec![
            ReifiedMountDeclaration::host(
                "/mnt/project",
                project.path(),
                ReifiedMountAccess::ReadWrite,
            ),
            ReifiedMountDeclaration::host(
                "/tmp/.alan-env-0",
                delegated.path(),
                ReifiedMountAccess::ReadOnly,
            ),
        ],
        project.path(),
        vec![
            "/bin/sh".into(),
            "-c".into(),
            script.into(),
            "alan-private-env".into(),
            ambient.path().display().to_string(),
            std::process::id().to_string(),
        ],
        NetworkPosture::Deny,
    ))
    .unwrap();
    let runner = LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock);
    for _ in 0..2 {
        let result = runner.run(&plan).unwrap();
        assert_eq!(result.exit_code, 0, "{}", result.stderr);
        assert_eq!(result.stdout, "private-environment-ok\n");
    }
    assert!(std::fs::read_dir(project.path()).unwrap().next().is_none());
    assert_eq!(
        std::fs::read_to_string(delegated.path().join("keep")).unwrap(),
        "delegated"
    );
    assert_eq!(
        std::fs::read_to_string(ambient.path().join("credentials.toml")).unwrap(),
        "private canary"
    );
    assert_eq!(std::fs::read_dir(ambient.path()).unwrap().count(), 1);
}

#[test]
fn native_proc_conflicts_refuse_before_user_effects() {
    if !probe_linux_reification().is_selectable() {
        eprintln!("skipping native proc conflict: namespace unavailable");
        return;
    }
    let project = tempfile::tempdir().unwrap();
    for namespace in ["/proc", "/proc/project"] {
        let plan = ReifiedNamespacePlan::derive(ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                namespace,
                project.path(),
                ReifiedMountAccess::ReadWrite,
            )],
            project.path(),
            vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf effect > marker".into(),
            ],
            NetworkPosture::Deny,
        ))
        .unwrap();
        let error =
            LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
                .run(&plan)
                .unwrap_err();
        assert!(error.reason.contains("conflicts with private native proc"));
        assert!(!project.path().join("marker").exists());
    }
}

#[test]
fn native_rust_shell_lookup_and_new_cwd_refuse_before_effects() {
    let (Some(proxy), Some(runtime)) = (
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
    ) else {
        eprintln!("skipping native lookup: explicit owned runtime inputs absent");
        return;
    };
    assert!(probe_linux_reification().is_selectable());
    let proxy = PathBuf::from(proxy);
    let runtime = PathBuf::from(runtime);
    let home = runtime.parent().unwrap().parent().unwrap();
    let mut unsupported = Vec::new();
    for (case, script) in [
        (
            "function",
            "printf effect > marker; cargo() { true; }; cargo test --offline",
        ),
        (
            "alias",
            "printf effect > marker; alias cargo=true\ncargo test --offline",
        ),
        (
            "new cwd",
            "printf effect > marker; mkdir -p child/.cargo; printf '[build]\\nrustc=\"/usr/bin/true\"\\n' > child/.cargo/config.toml; cd child; cargo test --offline",
        ),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        let project = std::fs::canonicalize(fixture.path()).unwrap();
        std::fs::create_dir(project.join("src")).unwrap();
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname='alan_lookup_fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        std::fs::write(project.join("src/lib.rs"), "pub fn answer() -> u32 {4}\n").unwrap();
        let mut input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                &project,
                &project,
                ReifiedMountAccess::ReadWrite,
            )],
            &project,
            vec!["/bin/sh".into(), "-c".into(), script.into()],
            NetworkPosture::Deny,
        );
        let commands = crate::tools::sandbox::shell_commands(script).unwrap();
        let environment = rustup::discover(
            proxy.to_str().unwrap(),
            home,
            Some(runtime.file_name().unwrap().to_str().unwrap().into()),
            Some((&input, &commands)),
        );
        if let Ok(Some(environment)) = environment {
            let mut substrate = default_execution_substrate();
            substrate.extend(
                environment
                    .proxy_mounts
                    .iter()
                    .chain(&environment.toolchain_mounts)
                    .cloned(),
            );
            input = input
                .with_execution_substrate(substrate)
                .with_command_path(format!("{}:/usr/bin:/bin", proxy.display()));
            input.rustup = Some(environment);
            let plan = ReifiedNamespacePlan::derive(input).unwrap();
            let result =
                LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock)
                    .run(&plan)
                    .unwrap();
            eprintln!(
                "unsupported {case} executed: exit {}, marker {}, stdout {}, stderr {}",
                result.exit_code,
                project.join("marker").exists(),
                result.stdout,
                result.stderr
            );
            unsupported.push(case);
        } else {
            assert!(!project.join("marker").exists(), "{case}");
            eprintln!("native {case} refused before marker/user effects");
        }
    }
    assert!(
        unsupported.is_empty(),
        "unsupported native lookup contexts admitted: {unsupported:?}"
    );
}

#[tokio::test]
async fn native_public_sandbox_rejects_executable_heredoc_lookup_changes() {
    if !probe_linux_reification().is_selectable() {
        eprintln!("skipping executable heredoc: native namespace unavailable");
        return;
    }
    let mut admitted = Vec::new();
    for script in [
        "cat <<EOF\n'$(cargo() { printf effect > marker; }; cargo test)'\nEOF",
        "cat <<EOF\n'$(printf effect > marker; RUSTC=other-rustc cargo test)'\nEOF",
    ] {
        let fixture = tempfile::tempdir().unwrap();
        let project = std::fs::canonicalize(fixture.path()).unwrap();
        let sandbox = crate::tools::Sandbox::with_backend(
            project.clone(),
            SandboxBackendKind::LinuxReifiedNamespace,
        );
        let result = sandbox
            .exec_with_timeout_and_capability(
                script,
                &project,
                Some(std::time::Duration::from_secs(30)),
                Some(alan_agent_protocol::ToolCapability::Write),
            )
            .await;
        match result {
            Ok(result) => {
                eprintln!(
                    "public Sandbox admitted executable heredoc: exit {}, marker {}, stdout {}, stderr {}",
                    result.exit_code,
                    project.join("marker").exists(),
                    result.stdout,
                    result.stderr
                );
                admitted.push(script);
            }
            Err(error) => {
                assert!(error.to_string().contains("inspected Rust"), "{error}");
                assert!(!project.join("marker").exists());
                eprintln!(
                    "public Sandbox refuses executable heredoc Rust lookup before marker effects"
                );
            }
        }
    }
    assert!(
        admitted.is_empty(),
        "executable heredoc reached public Sandbox effects: {admitted:?}"
    );
}
