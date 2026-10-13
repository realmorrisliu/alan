use super::*;

#[test]
fn native_opaque_writers_refuse_before_configuration_or_compiler_effects() {
    let Some(runtime) = std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME") else {
        eprintln!("skipping native sequence: explicit owned runtime input absent");
        return;
    };
    assert!(probe_linux_reification().is_selectable());
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let system = root.join("runtime");
    assert!(
        Command::new("/bin/cp")
            .args(["-a", "--reflink=auto"])
            .arg(runtime)
            .arg(&system)
            .status()
            .unwrap()
            .success()
    );
    let path = format!("{}/bin:/usr/bin:/bin", system.display());
    let mut admitted = Vec::new();
    for writer in ["git", "tar", "python", "wrapper"] {
        let project = root.join(writer);
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname='sequence_fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        std::fs::write(
            project.join("Cargo.lock"),
            "version=4\n[[package]]\nname='sequence_fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(
            project.join("src/lib.rs"),
            "#[test] fn actual() { assert_eq!(2+2,4); }\n",
        )
        .unwrap();
        let compiler = project.join("compiler");
        std::fs::write(&compiler, format!("#!/bin/sh\nprintf effect >> /mnt/project/compiler-marker\nexec {}/bin/rustc \"$@\"\n", system.display())).unwrap();
        std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = "[build]\nrustc='/mnt/project/compiler'\n";
        let command = match writer {
            "git" => {
                let git = |args: &[&str]| {
                    let result = Command::new("/usr/bin/git")
                        .arg("-C")
                        .arg(&project)
                        .args([
                            "-c",
                            "user.name=Alan fixture",
                            "-c",
                            "user.email=fixture@localhost",
                            "-c",
                            "commit.gpgsign=false",
                        ])
                        .args(args)
                        .output()
                        .unwrap();
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    String::from_utf8(result.stdout).unwrap().trim().to_owned()
                };
                git(&["init", "-q"]);
                std::fs::create_dir(project.join(".cargo")).unwrap();
                std::fs::write(project.join(".cargo/config.toml"), config).unwrap();
                std::fs::write(project.join("writer-marker"), "effect").unwrap();
                git(&["add", "."]);
                git(&["commit", "-qm", "configuration fixture"]);
                let commit = git(&["rev-parse", "HEAD"]);
                git(&["rm", "-q", ".cargo/config.toml", "writer-marker"]);
                git(&["commit", "-qm", "safe fixture"]);
                format!(
                    "git checkout {commit} -- .cargo/config.toml writer-marker; cargo test --offline --locked"
                )
            }
            "tar" => {
                let payload = root.join("payload");
                std::fs::create_dir_all(payload.join(".cargo")).unwrap();
                std::fs::write(payload.join(".cargo/config.toml"), config).unwrap();
                std::fs::write(payload.join("writer-marker"), "effect").unwrap();
                assert!(
                    Command::new("/usr/bin/tar")
                        .arg("-cf")
                        .arg(project.join("payload.tar"))
                        .arg("-C")
                        .arg(payload)
                        .args([".cargo", "writer-marker"])
                        .status()
                        .unwrap()
                        .success()
                );
                "tar -xf payload.tar; cargo test --offline --locked".into()
            }
            "python" => {
                std::fs::write(project.join("writer.py"), format!("import os,pathlib\npathlib.Path(os.environ['CARGO_HOME'],'config.toml').write_text({config:?})\npathlib.Path('writer-marker').write_text('effect')\n")).unwrap();
                "python3 writer.py; cargo test --offline --locked".into()
            }
            _ => {
                let wrapper = project.join("timeout");
                std::fs::write(&wrapper, format!("#!/bin/sh\nmkdir -p .cargo\nprintf '%s' \"{config}\" > .cargo/config.toml\nprintf effect > writer-marker\nshift\nexec \"$@\"\n")).unwrap();
                std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
                "/mnt/project/timeout 10 cargo test --offline --locked".into()
            }
        };
        let mut substrate = default_execution_substrate();
        substrate.push(ReifiedExecutionSubstrateMount::new(&system, &system));
        let mut input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/project",
                &project,
                ReifiedMountAccess::ReadWrite,
            )],
            &project,
            vec!["/bin/sh".into(), "-c".into(), command.clone()],
            NetworkPosture::Deny,
        )
        .with_scratch_tmp_namespace_path("/.alan-sequence-tmp")
        .with_execution_substrate(substrate)
        .with_command_path(&path);
        ReifiedNamespacePlan::derive(input.clone()).unwrap();
        let commands = crate::tools::sandbox::shell_commands(&command).unwrap();
        match rustup::discover(&path, &root.join("home"), None, Some((&input, &commands))) {
            Ok(environment) => {
                input.rustup = environment;
                let plan = ReifiedNamespacePlan::derive(input).unwrap();
                let result = LinuxReifiedNamespaceRunner::with_fallback_backend(
                    SandboxBackendKind::Landlock,
                )
                .run(&plan)
                .unwrap();
                eprintln!(
                    "admitted {writer}: exit {}, writer {}, compiler {}; {} {}",
                    result.exit_code,
                    project.join("writer-marker").exists(),
                    project.join("compiler-marker").exists(),
                    result.stdout,
                    result.stderr
                );
                admitted.push(writer);
            }
            Err(error) => {
                assert!(error.contains("separate requests"), "{writer}: {error}");
                assert!(!project.join("writer-marker").exists());
                assert!(!project.join("compiler-marker").exists());
                assert!(!project.join(".cargo/config.toml").exists());
                eprintln!("{writer} refused before writer/configuration/compiler effects");
            }
        }
    }
    assert!(
        admitted.is_empty(),
        "uninspected writers admitted: {admitted:?}"
    );
}
