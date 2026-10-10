use super::super::*;
use crate::tools::reified_namespace::ReifiedMountAccess;
use std::net::TcpListener;
use std::os::unix::fs::{MetadataExt, symlink};

#[tokio::test]
async fn native_development_build_diff_authority_and_lifecycle() {
    const CHILD: &str = "ALAN_TEST_NATIVE_DEVELOPMENT_CHILD";
    const EFFECT_DELAY_SECS: u64 = 15;
    if std::env::var_os(CHILD).is_none() {
        let (Some(proxy), Some(runtime), Some(git_root)) = (
            std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
            std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
            std::env::var_os("ALAN_LINUX_QUALIFICATION_GIT_ROOT"),
        ) else {
            eprintln!("skipping native development fixture: explicit task-owned tools absent");
            return;
        };
        let runtime = PathBuf::from(runtime);
        let home = runtime.parent().unwrap().parent().unwrap();
        let settings = std::fs::read(home.join("settings.toml")).unwrap();
        let name = concat!(
            module_path!(),
            "::native_development_build_diff_authority_and_lifecycle"
        );
        let (_, name) = name.split_once("::").unwrap();
        let proxy = PathBuf::from(proxy);
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", name, "--nocapture"])
            .env(CHILD, "1")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", proxy.display()),
            )
            .env("RUSTUP_HOME", home)
            .env_remove("RUSTUP_TOOLCHAIN")
            .env("ALAN_LINUX_QUALIFICATION_GIT_ROOT", git_root);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        assert_eq!(std::fs::read(home.join("settings.toml")).unwrap(), settings);
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        let unsupported = tempfile::tempdir().unwrap();
        command.env(CHILD, "unsupported-path").env(
            "PATH",
            format!(
                "{}:{}:/usr/bin:/bin:/usr/sbin:/sbin",
                unsupported.path().display(),
                proxy.display()
            ),
        );
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        assert_eq!(std::fs::read(home.join("settings.toml")).unwrap(), settings);
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        return;
    }

    let readiness = crate::tools::sandbox_backend::linux_reified_namespace_backend_readiness();
    if std::env::var(CHILD).unwrap() == "unsupported-path" {
        assert_ne!(
            readiness.selected_backend,
            crate::tools::SandboxBackendKind::LinuxReifiedNamespace
        );
        assert!(!readiness.selected_backend.permits_autonomous_bash());
        let project = tempfile::tempdir().unwrap();
        let sandbox = Sandbox::from_spec_with_backend(
            SandboxSpec::seed(project.path().to_path_buf()),
            crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
        );
        let error = sandbox
            .exec("printf effect > marker", project.path())
            .await
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("PATH entry outside the reified execution substrate"),
            "{error:#}"
        );
        assert!(!project.path().join("marker").exists());
        eprintln!(
            "native development unavailable: {readiness:?}; required backend refused before effects: {error:#}"
        );
        return;
    }
    assert_eq!(
        readiness.selected_backend,
        crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
        "{readiness:?}"
    );
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let dependency = fixture.path().join("dependency");
    let outside = fixture.path().join("ungranted-home");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(dependency.join("src")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let credential = outside.join("credentials.toml");
    std::fs::write(&credential, "fixture credential canary").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let runtime = PathBuf::from(std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME").unwrap());
    let runtime_bytes = std::fs::read(runtime.join("bin/rustc")).unwrap();
    let source = format!(
        r#"pub fn answer() -> u32 {{ alan_development_dep::answer() - 1 }}
#[cfg(test)]
mod tests {{
    use std::fs::{{self, OpenOptions}};
    use std::io::Write;
    use std::path::PathBuf;
    #[test]
    fn expected_answer() {{ assert_eq!(super::answer(), 3); }}
    #[test]
    fn isolated_environment() {{
        assert!(fs::read({credential:?}).is_err());
        assert!(OpenOptions::new().append(true).open({runtime_file:?}).is_err());
        assert!(OpenOptions::new().append(true).open("../dependency/src/lib.rs").is_err());
        assert!(std::net::TcpStream::connect(("127.0.0.1", {port})).is_err());
        let home = PathBuf::from(std::env::var("CARGO_HOME").unwrap());
        assert!(!home.join("credentials.toml").exists());
        assert!(!home.join("previous-command").exists());
        fs::write(home.join("previous-command"), "private").unwrap();
        if std::env::var_os("CARGO_TARGET_DIR").is_some() {{
            assert!(OpenOptions::new().append(true).open("src/lib.rs").is_err());
        }}
    }}
    #[test]
    #[ignore]
    fn delayed_effect() {{
        let home = PathBuf::from(std::env::var("CARGO_HOME").unwrap());
        fs::write(home.join("lifecycle"), "private scratch in use").unwrap();
        OpenOptions::new().create(true).append(true).open("starts.log").unwrap().write_all(b"started\n").unwrap();
        let mut child = std::process::Command::new("/bin/sh").args(["-c", "printf armed > ready; sleep {delay}; printf late > late; sleep 60"]).spawn().unwrap();
        child.wait().unwrap();
    }}
}}
"#,
        credential = credential.to_str().unwrap(),
        runtime_file = runtime.join("bin/rustc").to_str().unwrap(),
        port = listener.local_addr().unwrap().port(),
        delay = EFFECT_DELAY_SECS,
    );
    let files = [
        (project.join("Cargo.toml"), "[package]\nname = \"alan_development_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nalan_development_dep = { path = \"../dependency\" }\n".to_string()),
        (project.join("Cargo.lock"), "version = 4\n[[package]]\nname = \"alan_development_fixture\"\nversion = \"0.1.0\"\ndependencies = [\"alan_development_dep\"]\n[[package]]\nname = \"alan_development_dep\"\nversion = \"0.1.0\"\n".to_string()),
        (project.join("rust-toolchain.toml"), "[toolchain]\nchannel = \"1.97.0\"\n".to_string()),
        (project.join(".gitignore"), "/target/\n/ready\n/late\n/starts.log\n".to_string()),
        (project.join("src/lib.rs"), source.clone()),
        (dependency.join("Cargo.toml"), "[package]\nname = \"alan_development_dep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n".to_string()),
        (dependency.join("src/lib.rs"), "pub fn answer() -> u32 { 3 }\n".to_string()),
    ];
    for (path, content) in &files {
        std::fs::write(path, content).unwrap();
    }
    let git_root = PathBuf::from(std::env::var_os("ALAN_LINUX_QUALIFICATION_GIT_ROOT").unwrap());
    let git = git_root.join("bin/git");
    let git_prefix = format!(
        "GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_EXEC_PATH='{}' '{}'",
        git_root.join("lib/git-core").display(),
        git.display()
    );
    for args in [
        vec![
            "init".to_string(),
            "--initial-branch=main".into(),
            format!(
                "--template={}",
                git_root.join("share/git-core/templates").display()
            ),
        ],
        vec!["add".into(), ".".into()],
        vec![
            "-c".into(),
            "user.name=Alan fixture".into(),
            "-c".into(),
            "user.email=fixture@example.invalid".into(),
            "commit".into(),
            "--no-gpg-sign".into(),
            "-m".into(),
            "fixture baseline".into(),
        ],
    ] {
        let output = std::process::Command::new(&git)
            .args(args)
            .current_dir(&project)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_EXEC_PATH", git_root.join("lib/git-core"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let make_sandbox = |include_dependency, access| {
        let mut mounts = vec![SandboxHostMount {
            namespace_path: project.clone(),
            host_path: project.clone(),
            access,
        }];
        if include_dependency {
            mounts.push(SandboxHostMount {
                namespace_path: dependency.clone(),
                host_path: dependency.clone(),
                access: ReifiedMountAccess::ReadOnly,
            });
        }
        for root in [
            git_root.join("bin"),
            git_root.join("lib/git-core"),
            git_root.join("share/git-core/templates"),
        ] {
            mounts.push(SandboxHostMount {
                namespace_path: root.clone(),
                host_path: root,
                access: ReifiedMountAccess::ReadOnly,
            });
        }
        Sandbox::from_spec_with_backend(
            SandboxSpec::from_host_mounts(&mounts),
            crate::tools::SandboxBackendKind::LinuxReifiedNamespace,
        )
    };
    let sandbox = make_sandbox(true, ReifiedMountAccess::ReadWrite);
    let versions = sandbox
        .exec_with_timeout_and_capability(
            &format!("{git_prefix} --version && rustc --version && cargo --version"),
            &project,
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(versions.exit_code, 0, "{}", versions.stderr);
    assert!(versions.stdout.contains("git version 2.53.0"));
    assert!(versions.stdout.contains("rustc 1.97.0") && versions.stdout.contains("cargo 1.97.0"));
    record("tool-identity", &versions);

    let red = sandbox
        .exec("cargo test --offline --locked", &project)
        .await
        .unwrap();
    assert_eq!(red.exit_code, 101, "{} {}", red.stdout, red.stderr);
    assert!(red.stdout.contains("expected_answer ... FAILED"));
    assert!(red.stdout.contains("isolated_environment ... ok"));
    record("red", &red);
    let corrected = source.replacen(
        "alan_development_dep::answer() - 1",
        "alan_development_dep::answer()",
        1,
    );
    sandbox
        .write(&project.join("src/lib.rs"), corrected.as_bytes())
        .await
        .unwrap();
    let green = sandbox
        .exec("cargo test --offline --locked", &project)
        .await
        .unwrap();
    assert_eq!(green.exit_code, 0, "{} {}", green.stdout, green.stderr);
    assert!(green.stdout.contains("2 passed; 0 failed; 1 ignored"));
    record("green", &green);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let diff = sandbox
        .exec_with_timeout_and_capability(
            &format!("{git_prefix} diff --no-ext-diff -- src/lib.rs"),
            &project,
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(diff.exit_code, 0, "{}", diff.stderr);
    assert!(
        diff.stdout
            .contains("-pub fn answer() -> u32 { alan_development_dep::answer() - 1 }")
    );
    assert!(
        diff.stdout
            .contains("+pub fn answer() -> u32 { alan_development_dep::answer() }")
    );
    let status = sandbox
        .exec_with_timeout_and_capability(
            &format!("{git_prefix} status --porcelain=v1 --untracked-files=all"),
            &project,
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    assert_eq!(status.stdout, " M src/lib.rs\n");
    record("git-diff", &diff);

    let revoked = make_sandbox(false, ReifiedMountAccess::ReadWrite);
    let denied = revoked
        .exec("cargo test --offline --locked", &project)
        .await
        .unwrap();
    assert_ne!(denied.exit_code, 0);
    assert!(
        denied
            .stderr
            .contains("failed to load source for dependency"),
        "{}",
        denied.stderr
    );
    record("dependency-revoked", &denied);
    let unbuilt = make_sandbox(false, ReifiedMountAccess::ReadOnly);
    let denied = unbuilt
        .exec("cargo test --offline --locked", &project)
        .await
        .unwrap();
    assert_ne!(denied.exit_code, 0);
    record("dependency-missing", &denied);
    let read_only = make_sandbox(true, ReifiedMountAccess::ReadOnly);
    let protected = read_only
        .exec("cargo test --offline --locked", &project)
        .await
        .unwrap();
    assert_eq!(
        protected.exit_code, 0,
        "{} {}",
        protected.stdout, protected.stderr
    );
    record("read-only-build", &protected);
    let denied = read_only
        .exec("printf unauthorized >> src/lib.rs", &project)
        .await;
    assert!(denied.is_err() || denied.as_ref().is_ok_and(|result| result.exit_code != 0));
    assert!(
        read_only
            .write(&project.join("src/lib.rs"), b"unauthorized")
            .await
            .is_err()
    );
    assert!(
        sandbox
            .write(&dependency.join("src/lib.rs"), b"unauthorized")
            .await
            .is_err()
    );

    let original_dependency = std::fs::read(dependency.join("src/lib.rs")).unwrap();
    std::fs::write(
        outside.join("lib.rs"),
        "compile_error!(\"escaped dependency executed\");\n",
    )
    .unwrap();
    std::fs::remove_file(dependency.join("src/lib.rs")).unwrap();
    symlink(outside.join("lib.rs"), dependency.join("src/lib.rs")).unwrap();
    let denied = sandbox
        .exec("cargo test --offline --locked", &project)
        .await
        .unwrap();
    assert_ne!(denied.exit_code, 0);
    assert!(!denied.stderr.contains("escaped dependency executed"));
    record("dependency-escape", &denied);
    std::fs::remove_file(dependency.join("src/lib.rs")).unwrap();
    std::fs::write(dependency.join("src/lib.rs"), original_dependency).unwrap();
    let restored = sandbox
        .exec("cargo test --offline --locked --no-run", &project)
        .await
        .unwrap();
    assert_eq!(restored.exit_code, 0, "{}", restored.stderr);

    for timeout in [false, true] {
        let started = std::time::Instant::now();
        let running_sandbox = sandbox.clone();
        let cwd = project.clone();
        let mut execution = tokio::spawn(async move {
            running_sandbox
                .exec_with_timeout(
                    "cargo test --offline --locked delayed_effect -- --ignored",
                    &cwd,
                    // Two seconds expired during namespace setup, before Cargo's descendant.
                    timeout.then_some(Duration::from_secs(10)),
                )
                .await
        });
        let ready = tokio::time::timeout(Duration::from_secs(20), async {
            while !project.join("ready").exists() {
                if execution.is_finished() {
                    let result = (&mut execution).await;
                    panic!(
                        "development command ended before descendant started after {:?}: {result:?}",
                        started.elapsed()
                    );
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        if ready.is_err() {
            execution.abort();
            let result = execution.await;
            panic!("development descendant never started: {result:?}");
        }
        eprintln!(
            "native development descendant ready after {:?}",
            started.elapsed()
        );
        assert!(!project.join("late").exists());
        assert!(
            !project_processes(&project).is_empty(),
            "native developer processes must actually be live"
        );
        let runner_roots = native_runner_roots();
        assert_eq!(runner_roots.len(), 1, "{runner_roots:?}");
        assert!(runner_roots[0].join("root").is_dir());
        assert!(runner_roots[0].join("setup-ok").exists());
        if timeout {
            let error = execution.await.unwrap().unwrap_err();
            assert!(format!("{error:#}").contains("timed out"), "{error:#}");
        } else {
            execution.abort();
            assert!(execution.await.unwrap_err().is_cancelled());
        }
        tokio::time::sleep(Duration::from_secs(EFFECT_DELAY_SECS) + Duration::from_millis(500))
            .await;
        assert!(
            !project.join("late").exists(),
            "descendant wrote after interruption"
        );
        assert!(
            project_processes(&project).is_empty(),
            "developer processes survived interruption: {:?}",
            project_processes(&project)
        );
        assert_eq!(
            std::fs::read_to_string(project.join("starts.log")).unwrap(),
            "started\n"
        );
        assert!(runner_roots.iter().all(|root| !root.exists()));
        eprintln!(
            "native development {}: started once, delayed effect absent",
            if timeout { "timeout" } else { "cancel" }
        );
        std::fs::remove_file(project.join("ready")).unwrap();
        std::fs::remove_file(project.join("starts.log")).unwrap();
    }
    for (path, content) in &files {
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            if path == &project.join("src/lib.rs") {
                corrected.as_str()
            } else {
                content.as_str()
            }
        );
    }
    assert_eq!(
        std::fs::read_to_string(credential).unwrap(),
        "fixture credential canary"
    );
    assert_eq!(
        std::fs::read(runtime.join("bin/rustc")).unwrap(),
        runtime_bytes
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("lib.rs")).unwrap(),
        "compile_error!(\"escaped dependency executed\");\n"
    );
    eprintln!(
        "native development matrix complete: exact one-line diff; independent read-only dependency; home/runtime/source/network/cache checks; cancel and timeout"
    );
}

fn record(name: &str, result: &ExecResult) {
    eprintln!(
        "native development {name}: exit {}\nstdout:\n{}\nstderr:\n{}",
        result.exit_code, result.stdout, result.stderr
    );
}

fn project_processes(project: &Path) -> Vec<String> {
    let project = std::fs::metadata(project).unwrap();
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            name.parse::<u32>().ok()?;
            let cwd = std::fs::metadata(entry.path().join("cwd")).ok()?;
            (cwd.dev() == project.dev() && cwd.ino() == project.ino()).then_some(name)
        })
        .collect()
}

fn native_runner_roots() -> Vec<PathBuf> {
    let prefix = format!("alan-reified-runner-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .collect()
}
