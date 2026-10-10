use super::tests::{approve, binding, service};
use super::*;
use alan_agent_engine::Config;
use alan_agent_engine::tools::{Tool, ToolContext, ToolExecutionAuthority};
use alan_kernel::{LiveNamespace, Namespace, Pid};
use alan_tools::BashTool;
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
