use super::super::*;
use crate::tools::reified_namespace::ReifiedMountAccess;

#[tokio::test]
async fn native_rustup_selection_through_sandbox_adapter() {
    const CASE: &str = "ALAN_TEST_NATIVE_RUSTUP_CASE";
    if let Ok(case) = std::env::var(CASE) {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("project");
        let dependency = fixture.path().join("dependency");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::create_dir_all(project.join(".cargo")).unwrap();
        std::fs::create_dir_all(dependency.join("src")).unwrap();
        let files = [
            (
                project.join("Cargo.toml"),
                "[package]\nname = \"alan_adapter_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nalan_adapter_dep = { path = \"../dependency\" }\n",
            ),
            (
                project.join("Cargo.lock"),
                "version = 4\n[[package]]\nname = \"alan_adapter_fixture\"\nversion = \"0.1.0\"\ndependencies = [\"alan_adapter_dep\"]\n[[package]]\nname = \"alan_adapter_dep\"\nversion = \"0.1.0\"\n",
            ),
            (
                project.join(".cargo/config.toml"),
                "[build]\nrustflags = [\"--cfg\", \"alan_adapter_fixture\", \"--check-cfg\", \"cfg(alan_adapter_fixture)\"]\nrustdocflags = [\"--cfg\", \"alan_adapter_fixture\", \"--check-cfg\", \"cfg(alan_adapter_fixture)\"]\n",
            ),
            (
                project.join("src/lib.rs"),
                "#[cfg(not(alan_adapter_fixture))]\ncompile_error!(\"project config lost\");\n/// ```\n/// assert_eq!(alan_adapter_fixture::answer(), 3);\n/// ```\npub fn answer() -> u32 {\n    alan_adapter_dep::answer()\n}\n#[test]\nfn local_dependency() {\n    assert_eq!(answer(), 3);\n}\n",
            ),
            (
                dependency.join("Cargo.toml"),
                "[package]\nname = \"alan_adapter_dep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                dependency.join("src/lib.rs"),
                "pub fn answer() -> u32 {\n    3\n}\n",
            ),
        ];
        for (path, bytes) in &files {
            std::fs::write(path, bytes).unwrap();
        }
        let toolchain = project.join("rust-toolchain.toml");
        std::fs::write(
            &toolchain,
            if case == "env" || case == "cli" {
                "[toolchain]\nchannel = \"missing\"\n"
            } else {
                "[toolchain]\nchannel = \"1.97.0\"\ncomponents = [\"clippy\", \"rustfmt\"]\n"
            },
        )
        .unwrap();
        if case == "missing_component" {
            std::fs::write(
                &toolchain,
                "[toolchain]\nchannel = \"1.97.0\"\ncomponents = [\"alan-missing-component\"]\n",
            )
            .unwrap();
        }
        let positive = matches!(case.as_str(), "project" | "env" | "cli");
        let sandbox = Sandbox::from_spec_with_backend(
            SandboxSpec::from_host_mounts(&[
                SandboxHostMount {
                    namespace_path: project.clone(),
                    host_path: project.clone(),
                    access: if positive {
                        ReifiedMountAccess::ReadOnly
                    } else {
                        ReifiedMountAccess::ReadWrite
                    },
                },
                SandboxHostMount {
                    namespace_path: dependency.clone(),
                    host_path: dependency.clone(),
                    access: ReifiedMountAccess::ReadOnly,
                },
            ]),
            crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
        );
        if positive {
            let readiness =
                crate::tools::sandbox_backend::linux_reified_namespace_backend_readiness();
            assert_eq!(
                readiness.selected_backend,
                crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
                "{readiness:?}"
            );
            let selector = if case == "cli" { " +1.97.0" } else { "" };
            let command = format!(
                "rustc{selector} --version && cargo{selector} --version && rustdoc{selector} --version && cargo{selector} test --offline --locked && cargo{selector} fmt -- --check && cargo{selector} clippy --offline --locked -- -D warnings"
            );
            let result = sandbox.exec(&command, &project).await.unwrap();
            assert_eq!(result.exit_code, 0, "{} {}", result.stdout, result.stderr);
            for name in ["rustc", "cargo", "rustdoc"] {
                assert!(
                    result.stdout.contains(&format!("{name} 1.97.0")),
                    "{}",
                    result.stdout
                );
            }
            assert_eq!(
                result.stdout.matches("1 passed").count(),
                2,
                "{}",
                result.stdout
            );
            assert!(!project.join("target").exists());
            assert!(!dependency.join("target").exists());
            eprintln!(
                "native automatic Rustup {case}:\n{}{}",
                result.stdout, result.stderr
            );
        } else {
            if case == "changed" {
                let result = sandbox.exec("rustc --version", &project).await.unwrap();
                assert_eq!(result.exit_code, 0, "{}", result.stderr);
                assert!(result.stdout.contains("rustc 1.97.0"));
                std::fs::write(&toolchain, "[toolchain]\nchannel = \"missing\"\n").unwrap();
            }
            let cargo = if case == "missing_cli" {
                "cargo +missing test"
            } else {
                "cargo test"
            };
            let error = sandbox
                .exec(&format!("printf effect > marker; {cargo}"), &project)
                .await
                .unwrap_err();
            let reason = format!("{error:#}");
            assert!(
                reason.contains(if case == "alias" {
                    "unsafe alias"
                } else if case == "missing_component" {
                    "required Rust component/target absent"
                } else {
                    "selected Rust runtime absent"
                }),
                "{reason}"
            );
            assert!(!project.join("marker").exists());
            eprintln!("native automatic Rustup {case}: {reason}");
        }
        for (path, bytes) in &files {
            assert_eq!(std::fs::read_to_string(path).unwrap(), *bytes);
        }
        return;
    }
    let (Some(proxy), Some(runtime)) = (
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
        std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
    ) else {
        eprintln!("skipping native automatic Rustup fixture: explicit inputs absent");
        return;
    };
    assert!(crate::tools::sandbox_backend::probe_linux_reification().is_selectable());
    let proxy = PathBuf::from(proxy);
    let runtime = PathBuf::from(runtime);
    let home = runtime.parent().unwrap().parent().unwrap();
    let settings = std::fs::read(home.join("settings.toml")).unwrap();
    let original_proxy = std::fs::read(proxy.join("rustup")).unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let fake_home = fixture.path().join("rustup");
    std::fs::create_dir_all(fake_home.join("toolchains")).unwrap();
    let name = runtime.file_name().unwrap();
    let mut fake_settings = toml::Table::new();
    fake_settings.insert("version".into(), "12".into());
    fake_settings.insert("default_toolchain".into(), name.to_str().unwrap().into());
    std::fs::write(
        fake_home.join("settings.toml"),
        toml::to_string(&fake_settings).unwrap(),
    )
    .unwrap();
    std::os::unix::fs::symlink(&runtime, fake_home.join("toolchains").join(name)).unwrap();
    let name = concat!(
        module_path!(),
        "::native_rustup_selection_through_sandbox_adapter"
    );
    let (_, name) = name.split_once("::").unwrap();
    for case in [
        "project",
        "env",
        "cli",
        "missing_env",
        "missing_cli",
        "missing_component",
        "changed",
        "alias",
    ] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", name, "--nocapture"])
            .env(CASE, case)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", proxy.display()),
            )
            .env(
                "RUSTUP_HOME",
                if case == "alias" {
                    fake_home.as_path()
                } else {
                    home
                },
            )
            .env_remove("RUSTUP_TOOLCHAIN");
        if case == "env" {
            child.env("RUSTUP_TOOLCHAIN", "1.97.0");
        }
        if case == "cli" || case == "missing_env" {
            child.env("RUSTUP_TOOLCHAIN", "missing");
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{case}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }
    assert_eq!(std::fs::read(home.join("settings.toml")).unwrap(), settings);
    assert_eq!(std::fs::read(proxy.join("rustup")).unwrap(), original_proxy);
}
