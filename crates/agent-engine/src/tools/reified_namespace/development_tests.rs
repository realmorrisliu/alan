use super::*;

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
