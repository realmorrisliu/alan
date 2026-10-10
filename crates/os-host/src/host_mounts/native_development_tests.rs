use super::tests::{approve, binding, service};
use super::*;
use alan_agent_engine::Config;
use alan_agent_engine::tools::{Tool, ToolContext, ToolExecutionAuthority};
use alan_kernel::{LiveNamespace, Namespace, Pid};
use alan_tools::{BashTool, EditFileTool, ReadFileTool};
use std::net::TcpListener;

#[tokio::test]
async fn native_bash_read_only_project_isolation_and_revocation() {
    const CHILD: &str = "ALAN_TEST_NATIVE_BASH_DEVELOPMENT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let (Some(proxy), Some(runtime)) = (
            std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
            std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
        ) else {
            eprintln!("skipping native Bash development: explicit task-owned Rust inputs absent");
            return;
        };
        let runtime = PathBuf::from(runtime);
        let home = runtime.parent().unwrap().parent().unwrap();
        let settings = std::fs::read(home.join("settings.toml")).unwrap();
        let name = concat!(
            module_path!(),
            "::native_bash_read_only_project_isolation_and_revocation"
        );
        let (_, name) = name.split_once("::").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env(CHILD, "1")
            .env(
                "PATH",
                format!(
                    "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                    PathBuf::from(proxy).display()
                ),
            )
            .env("RUSTUP_HOME", home)
            .env_remove("RUSTUP_TOOLCHAIN")
            .output()
            .unwrap();
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
    let readiness = alan_agent_engine::tools::linux_reified_namespace_backend_readiness();
    assert_eq!(
        readiness.selected_backend,
        alan_agent_engine::tools::SandboxBackendKind::LinuxReifiedNamespace,
        "{readiness:?}"
    );
    eprintln!("native Bash actual readiness: {readiness:?}");
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let outside = fixture.path().join("ungranted-home");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("dependency/src")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let credential = outside.join("credentials.toml");
    std::fs::write(&credential, "fixture credential canary").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let runtime = PathBuf::from(std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME").unwrap());
    let runtime_file = runtime.join("bin/rustc");
    let runtime_bytes = std::fs::read(&runtime_file).unwrap();
    let source = format!(
        r#"#[test]
fn authorized_build_is_isolated() {{
    use std::fs::{{self, OpenOptions}};
    use std::path::PathBuf;
    assert_eq!(alan_bash_fixture_dep::answer(), 42);
    assert!(fs::read({credential:?}).is_err());
    for path in ["src/lib.rs", "dependency/src/lib.rs", {runtime_file:?}] {{
        assert!(OpenOptions::new().append(true).open(path).is_err());
    }}
    assert!(std::net::TcpStream::connect(("127.0.0.1", {port})).is_err());
    let cargo = PathBuf::from(std::env::var("CARGO_HOME").unwrap());
    assert!(!cargo.join("credentials.toml").exists());
    assert!(!cargo.join("previous-command").exists());
    fs::write(cargo.join("previous-command"), "private").unwrap();
    assert!(std::env::var_os("CARGO_TARGET_DIR").is_some());
}}
"#,
        credential = credential.to_str().unwrap(),
        runtime_file = runtime_file.to_str().unwrap(),
        port = listener.local_addr().unwrap().port()
    );
    let files = [
        ("Cargo.toml", "[package]\nname = \"alan_bash_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nalan_bash_fixture_dep = { path = \"dependency\" }\n".to_string()),
        ("Cargo.lock", "version = 4\n[[package]]\nname = \"alan_bash_fixture\"\nversion = \"0.1.0\"\ndependencies = [\"alan_bash_fixture_dep\"]\n[[package]]\nname = \"alan_bash_fixture_dep\"\nversion = \"0.1.0\"\n".to_string()),
        ("rust-toolchain.toml", "[toolchain]\nchannel = \"1.97.0\"\n".to_string()),
        ("src/lib.rs", source),
        ("dependency/Cargo.toml", "[package]\nname = \"alan_bash_fixture_dep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n".to_string()),
        ("dependency/src/lib.rs", "pub fn answer() -> u32 { 42 }\n".to_string()),
    ];
    for (path, content) in &files {
        std::fs::write(project.join(path), content).unwrap();
    }
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    let grant = approve(
        &service,
        7,
        "/mnt/project",
        HostMountAccess::ReadOnly,
        &project,
    )
    .await;
    let config = Arc::new(Config::default());
    let mut current = service.reconcile(7, binding("/mnt/project")).unwrap();
    for command in [
        "rustc --version && cargo --version",
        "cargo test --offline --locked",
        "cargo test --offline --locked",
    ] {
        current = service.reconcile(7, current).unwrap();
        let context = ToolContext::from_binding(current.clone(), Arc::clone(&config));
        assert!(!context.shell_sandbox().unwrap().is_writable(&project));
        let started = std::time::Instant::now();
        let result = BashTool::new()
            .execute(serde_json::json!({"command": command}), &context)
            .await
            .unwrap();
        assert_eq!(result["exit_code"], 0, "{result}");
        assert_eq!(result["success"], true);
        let stdout = result["stdout"].as_str().unwrap();
        if command.contains("--version") {
            assert!(stdout.contains("rustc 1.97.0") && stdout.contains("cargo 1.97.0"));
        } else {
            assert!(stdout.contains("1 passed; 0 failed"));
        }
        assert!(
            !result["stderr"]
                .as_str()
                .unwrap()
                .contains(project.to_str().unwrap())
        );
        eprintln!(
            "native Bash {command} after {:?}: {result}",
            started.elapsed()
        );
    }
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(!project.join("target").exists());
    for (path, content) in &files {
        assert_eq!(
            std::fs::read_to_string(project.join(path)).unwrap(),
            *content
        );
    }
    assert_eq!(
        std::fs::read_to_string(&credential).unwrap(),
        "fixture credential canary"
    );
    assert_eq!(std::fs::read(runtime_file).unwrap(), runtime_bytes);
    service.revoke(&grant.id, "native Bash fixture").unwrap();
    assert!(service.reconcile(7, current).is_err());
    let missing = service.reconcile(7, binding("/mnt/project")).unwrap();
    let context = ToolContext::from_binding(missing, config);
    assert!(
        BashTool::new()
            .execute(
                serde_json::json!({"command": "printf effect > marker"}),
                &context
            )
            .await
            .is_err()
    );
    assert!(!project.join("marker").exists());
    eprintln!(
        "native Bash isolation complete: live read-only grant, private output/cache, no home/runtime/source/network effects; revoked selected grant refused before next execution"
    );
}

#[tokio::test]
async fn native_bash_git_red_green_uses_selected_project_grant() {
    const CHILD: &str = "ALAN_TEST_NATIVE_BASH_GIT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let (Some(proxy), Some(runtime), Some(git_root)) = (
            std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_PROXY_DIR"),
            std::env::var_os("ALAN_LINUX_QUALIFICATION_RUST_RUNTIME"),
            std::env::var_os("ALAN_LINUX_QUALIFICATION_GIT_ROOT"),
        ) else {
            eprintln!("skipping native Bash Git: explicit task-owned Rust/Git inputs absent");
            return;
        };
        let runtime = PathBuf::from(runtime);
        let home = runtime.parent().unwrap().parent().unwrap();
        let settings = std::fs::read(home.join("settings.toml")).unwrap();
        let git_root = PathBuf::from(git_root);
        let git_bytes = std::fs::read(git_root.join("bin/git")).unwrap();
        let name = concat!(
            module_path!(),
            "::native_bash_git_red_green_uses_selected_project_grant"
        );
        let (_, name) = name.split_once("::").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env(CHILD, "1")
            .env(
                "PATH",
                format!(
                    "{}:{}/bin:/usr/bin:/bin:/usr/sbin:/sbin",
                    PathBuf::from(proxy).display(),
                    git_root.display()
                ),
            )
            .env("RUSTUP_HOME", home)
            .env_remove("RUSTUP_TOOLCHAIN")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        assert_eq!(std::fs::read(home.join("settings.toml")).unwrap(), settings);
        assert_eq!(std::fs::read(git_root.join("bin/git")).unwrap(), git_bytes);
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        return;
    }
    let readiness = alan_agent_engine::tools::linux_reified_namespace_backend_readiness();
    assert_eq!(
        readiness.selected_backend,
        alan_agent_engine::tools::SandboxBackendKind::LinuxReifiedNamespace,
        "{readiness:?}"
    );
    eprintln!("native Bash Git actual readiness: {readiness:?}");
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("dependency/src")).unwrap();
    let source = "pub fn answer() -> u32 { alan_git_fixture_dep::answer() - 1 }\n#[test]\nfn expected_answer() { assert_eq!(answer(), 3); }\n";
    let corrected = source.replace("::answer() - 1", "::answer()");
    let files = [
        (
            "Cargo.toml",
            "[package]\nname = \"alan_git_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nalan_git_fixture_dep = { path = \"dependency\" }\n",
        ),
        (
            "Cargo.lock",
            "version = 4\n[[package]]\nname = \"alan_git_fixture\"\nversion = \"0.1.0\"\ndependencies = [\"alan_git_fixture_dep\"]\n[[package]]\nname = \"alan_git_fixture_dep\"\nversion = \"0.1.0\"\n",
        ),
        ("rust-toolchain.toml", "[toolchain]\nchannel = \"1.97.0\"\n"),
        (".gitignore", "/target/\n"),
        ("src/lib.rs", source),
        (
            "dependency/Cargo.toml",
            "[package]\nname = \"alan_git_fixture_dep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        ),
        ("dependency/src/lib.rs", "pub fn answer() -> u32 { 3 }\n"),
    ];
    for (path, content) in &files {
        std::fs::write(project.join(path), content).unwrap();
    }
    let git_root = PathBuf::from(std::env::var_os("ALAN_LINUX_QUALIFICATION_GIT_ROOT").unwrap());
    let git = git_root.join("bin/git");
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
    let git_config = std::fs::read(project.join(".git/config")).unwrap();
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    let grant = approve(
        &service,
        7,
        "/mnt/project",
        HostMountAccess::ReadWrite,
        &project,
    )
    .await;
    let config = Arc::new(Config::default());
    let mut current = service.reconcile(7, binding("/mnt/project")).unwrap();
    for (command, exit, expected) in [
        (
            "git --version && rustc --version && cargo --version",
            0,
            "git version 2.53.0",
        ),
        ("git status --porcelain && git diff -- src/lib.rs", 0, ""),
        ("cargo test --offline --locked", 101, "FAILED"),
        ("cargo test --offline --locked", 0, "1 passed; 0 failed"),
        ("git diff -- src/lib.rs", 0, "@@ -1,3 +1,3 @@"),
        ("git status --porcelain", 0, " M src/lib.rs\n"),
    ] {
        current = service.reconcile(7, current).unwrap();
        let context = ToolContext::from_binding(current.clone(), Arc::clone(&config));
        assert!(context.shell_sandbox().unwrap().is_writable(&project));
        let started = std::time::Instant::now();
        let result = BashTool::new()
            .execute(serde_json::json!({"command": command}), &context)
            .await
            .unwrap();
        assert_eq!(result["exit_code"], exit, "{result}");
        assert_eq!(result["success"], exit == 0);
        let stdout = result["stdout"].as_str().unwrap();
        if command.contains("--version") {
            assert!(stdout.contains("rustc 1.97.0") && stdout.contains("cargo 1.97.0"));
        }
        if expected.is_empty() || command == "git status --porcelain" {
            assert_eq!(stdout, expected);
        } else {
            assert!(stdout.contains(expected), "{result}");
        }
        if command == "git diff -- src/lib.rs" {
            assert_eq!(
                stdout
                    .lines()
                    .filter(|line| line.starts_with("diff --git"))
                    .count(),
                1
            );
            assert!(
                stdout.contains("-pub fn answer() -> u32 { alan_git_fixture_dep::answer() - 1 }")
            );
            assert!(stdout.contains("+pub fn answer() -> u32 { alan_git_fixture_dep::answer() }"));
        }
        assert!(
            !result["stderr"]
                .as_str()
                .unwrap()
                .contains(project.to_str().unwrap())
        );
        eprintln!(
            "native Bash Git {command} after {:?}: {result}",
            started.elapsed()
        );
        if exit == 101 {
            current = service.reconcile(7, current).unwrap();
            let context = ToolContext::from_binding(current.clone(), Arc::clone(&config));
            let result = EditFileTool::new().execute(serde_json::json!({"path": "/mnt/project/src/lib.rs", "old_string": "::answer() - 1", "new_string": "::answer()"}), &context).await.unwrap();
            assert_eq!(result["success"], true);
            assert_eq!(result["path"], "/mnt/project/src/lib.rs");
            assert_eq!(result["replacements"], 1);
            assert_eq!(
                std::fs::read_to_string(project.join("src/lib.rs")).unwrap(),
                corrected
            );
        }
    }
    assert!(project.join("target").is_dir());
    for (path, content) in &files {
        assert_eq!(
            std::fs::read_to_string(project.join(path)).unwrap(),
            if *path == "src/lib.rs" {
                corrected.as_str()
            } else {
                content
            }
        );
    }
    assert_eq!(
        std::fs::read(project.join(".git/config")).unwrap(),
        git_config
    );
    let outside = fixture.path().join("outside-dependency");
    std::fs::create_dir_all(outside.join("src")).unwrap();
    std::fs::write(outside.join("Cargo.toml"), files[5].1).unwrap();
    std::fs::write(outside.join("src/lib.rs"), files[6].1).unwrap();
    current = service.reconcile(7, current).unwrap();
    let context = ToolContext::from_binding(current.clone(), Arc::clone(&config));
    EditFileTool::new().execute(serde_json::json!({"path": "/mnt/project/Cargo.toml", "old_string": "path = \"dependency\"", "new_string": "path = \"../outside-dependency\""}), &context).await.unwrap();
    for case in ["missing", "revoked"] {
        if case == "revoked" {
            let dependency = approve(
                &service,
                7,
                "/mnt/dependency",
                HostMountAccess::ReadOnly,
                &outside,
            )
            .await;
            current = service.reconcile(7, current).unwrap();
            let context = ToolContext::from_binding(current.clone(), Arc::clone(&config));
            let result = ReadFileTool::new()
                .execute(
                    serde_json::json!({"path": "/mnt/dependency/src/lib.rs"}),
                    &context,
                )
                .await
                .unwrap();
            assert_eq!(result["content"], files[6].1.trim_end());
            assert!(EditFileTool::new().execute(serde_json::json!({"path": "/mnt/dependency/src/lib.rs", "old_string": "3", "new_string": "4"}), &context).await.is_err());
            service
                .revoke(&dependency.id, "native dependency negative")
                .unwrap();
        }
        current = service.reconcile(7, current).unwrap();
        let context = ToolContext::from_binding(current.clone(), Arc::clone(&config));
        assert!(
            ReadFileTool::new()
                .execute(
                    serde_json::json!({"path": "/mnt/dependency/src/lib.rs"}),
                    &context
                )
                .await
                .is_err()
        );
        let result = BashTool::new()
            .execute(
                serde_json::json!({"command": "cargo test --offline --locked"}),
                &context,
            )
            .await
            .unwrap();
        eprintln!("native Bash dependency {case}: {result}");
        assert_eq!(result["exit_code"], 101, "{result}");
        assert_eq!(result["success"], false);
        assert!(!result["stdout"].as_str().unwrap().contains("1 passed"));
        let stderr = result["stderr"].as_str().unwrap();
        assert!(
            stderr.contains("failed to load source for dependency"),
            "{result}"
        );
        assert!(!stderr.contains(project.to_str().unwrap()), "{result}");
        assert!(!stderr.contains(outside.to_str().unwrap()), "{result}");
        assert!(project.join("target").is_dir());
        assert_eq!(
            std::fs::read_to_string(outside.join("Cargo.toml")).unwrap(),
            files[5].1
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("src/lib.rs")).unwrap(),
            files[6].1
        );
    }
    current = service.reconcile(7, current).unwrap();
    let context = ToolContext::from_binding(current.clone(), Arc::clone(&config));
    EditFileTool::new().execute(serde_json::json!({"path": "/mnt/project/Cargo.toml", "old_string": "path = \"../outside-dependency\"", "new_string": "path = \"dependency\""}), &context).await.unwrap();
    let escaped = fixture.path().join("ungranted-source.rs");
    let payload = "compile_error!(\"ALAN_ESCAPED_DEPENDENCY_PAYLOAD\");\n";
    std::fs::write(&escaped, payload).unwrap();
    let dependency_source = project.join("dependency/src/lib.rs");
    std::fs::remove_file(&dependency_source).unwrap();
    std::os::unix::fs::symlink(&escaped, &dependency_source).unwrap();
    assert!(
        ReadFileTool::new()
            .execute(
                serde_json::json!({"path": "/mnt/project/dependency/src/lib.rs"}),
                &context
            )
            .await
            .is_err()
    );
    let result = BashTool::new()
        .execute(
            serde_json::json!({"command": "cargo test --offline --locked"}),
            &context,
        )
        .await
        .unwrap();
    eprintln!("native Bash dependency escape: {result}");
    assert_eq!(result["exit_code"], 101, "{result}");
    assert_eq!(result["success"], false);
    assert!(!result["stdout"].as_str().unwrap().contains("1 passed"));
    assert!(
        !result["stderr"]
            .as_str()
            .unwrap()
            .contains("ALAN_ESCAPED_DEPENDENCY_PAYLOAD")
    );
    assert_eq!(std::fs::read_to_string(&escaped).unwrap(), payload);
    std::fs::remove_file(dependency_source).unwrap();
    std::fs::write(project.join("dependency/src/lib.rs"), files[6].1).unwrap();
    for (path, content) in &files {
        assert_eq!(
            std::fs::read_to_string(project.join(path)).unwrap(),
            if *path == "src/lib.rs" {
                corrected.as_str()
            } else {
                content
            }
        );
    }
    eprintln!(
        "native Bash dependency negatives complete: missing/revoked external grant refused despite retained in-grant build cache; live read-only file read allowed and edit denied before revocation; ungranted source alias denied without executing payload; no cross-grant Bash positive inferred"
    );
    service
        .revoke(&grant.id, "native Bash Git fixture")
        .unwrap();
    assert!(service.reconcile(7, current).is_err());
    let missing = service.reconcile(7, binding("/mnt/project")).unwrap();
    let context = ToolContext::from_binding(missing, config);
    assert!(
        BashTool::new()
            .execute(
                serde_json::json!({"command": "printf effect > marker"}),
                &context
            )
            .await
            .is_err()
    );
    assert!(!project.join("marker").exists());
    eprintln!(
        "native Bash Git complete: actual Bash RED/GREEN, EditFileTool one-line fix, exact git diff/status and sources; selected writable grant revoked before next effect; in-grant dependency, not disjoint read-only authority"
    );
}
