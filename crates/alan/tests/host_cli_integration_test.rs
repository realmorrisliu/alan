use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry};
use alan_llm::{GenerationResponse, MockLlmProvider};
use alan_os_host::{
    AlanOsHost, HostBootConfig, HostEndpointPaths, HostReadiness, HostStatus, LocalAttachment,
};

fn runtime_base(root: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let uid = String::from_utf8(Command::new("id").arg("-u").output().unwrap().stdout).unwrap();
        root.join(format!("alan-os-{}", uid.trim()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        root.join("runtime")
    }
}

fn spawn_blocked_bare_cli(runtime: &Path, runtime_dir: Option<&Path>) -> Child {
    let home = runtime.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_alan"));
    command
        .env("ALAN_INSTALL_CHANNEL", "stable")
        .env("HOME", home)
        .env("XDG_DATA_HOME", runtime.join("data"))
        .env("TMPDIR", runtime)
        .env_remove("ALAN_CONFIG_PATH");
    if let Some(runtime_dir) = runtime_dir {
        command.env("ALAN_INSTANCE_RUNTIME_DIR", runtime_dir);
    } else {
        command.env_remove("ALAN_INSTANCE_RUNTIME_DIR");
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

struct ForegroundChild(Child);

impl Drop for ForegroundChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

async fn wait_for_host_ready(paths: &HostEndpointPaths) -> bool {
    for _ in 0..400 {
        if paths
            .read_status()
            .is_ok_and(|status| status.readiness == HostReadiness::Ready)
        {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}

async fn wait_for_child_exit(child: &mut Child) -> Option<ExitStatus> {
    for _ in 0..400 {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    None
}

#[tokio::test]
async fn bare_cli_uses_an_independent_foreground_instance() {
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let base = runtime_base(runtime.path());
    let paths = HostEndpointPaths::from_runtime_dir(&base, "stable").unwrap();
    let response = GenerationResponse {
        content: "unused".into(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    };
    let host = AlanOsHost::boot(
        HostBootConfig::ephemeral(
            "stable",
            AgentProcessConfig::default(),
            LlmClient::new(MockLlmProvider::new().with_response(response)),
            ToolRegistry::new(),
        ),
        paths.clone(),
    )
    .await
    .unwrap();
    let (shutdown, shutdown_request) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(host.serve_until(async move {
        let _ = shutdown_request.await;
    }));

    let observer = LocalAttachment::new(paths.clone()).connect().await.unwrap();
    let observer_shell = alan_shell::Shell::new(observer.root.clone());
    let processes_before = observer_shell.ls("/proc").await.unwrap();
    assert!(observer_shell.write("/proc/clone", b"").await.is_err());

    let temporary_root = runtime.path().to_owned();
    let home = runtime.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let data_home = runtime.path().join("data");
    let output = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_alan"))
            .env("ALAN_INSTALL_CHANNEL", "stable")
            .env("HOME", home)
            .env("TMPDIR", temporary_root)
            .env("XDG_RUNTIME_DIR", base)
            .env("XDG_DATA_HOME", data_home)
            .env_remove("ALAN_CONFIG_PATH")
            .env_remove("ALAN_INSTANCE_RUNTIME_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"\n").unwrap();
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("stdin input body is empty"),
        "{output:?}"
    );
    assert!(
        std::fs::read_dir(runtime.path())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .strip_prefix("alan-")
                .is_some_and(|name| !name.starts_with("os-"))),
        "temporary foreground endpoint was not removed"
    );

    let processes_after = observer_shell.ls("/proc").await.unwrap();
    let added_processes = processes_after
        .iter()
        .filter(|pid| !processes_before.contains(pid))
        .collect::<Vec<_>>();
    let mut added_process_details = Vec::new();
    for pid in added_processes {
        let parent = String::from_utf8(
            observer_shell
                .cat(&format!("/proc/{pid}/parent"))
                .await
                .unwrap(),
        )
        .unwrap();
        let credentials = String::from_utf8(
            observer_shell
                .cat(&format!("/proc/{pid}/credentials"))
                .await
                .unwrap(),
        )
        .unwrap();
        added_process_details.push(format!(
            "pid={pid}, parent={parent}, credentials={credentials}"
        ));
    }
    assert_eq!(
        processes_after, processes_before,
        "bare `alan` must not attach to the ambient instance: {added_process_details:?}"
    );
    assert!(observer_shell.ls("/agent/root").await.is_ok());
    assert_eq!(
        observer_shell.cat("/proc/1/status").await.unwrap(),
        b"running\n"
    );
    assert_eq!(paths.read_status().unwrap().boot_id, observer.boot_id);

    drop(observer_shell);
    drop(observer);
    let _ = shutdown.send(());
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn host_stop_gracefully_stops_bare_foreground_instance() {
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let runtime_dir = runtime.path().join("foreground");
    let paths = HostEndpointPaths::from_runtime_dir(&runtime_dir, "stable").unwrap();
    let home = runtime.path().join("home");
    let mut foreground = spawn_blocked_bare_cli(runtime.path(), Some(&runtime_dir));
    if !wait_for_host_ready(&paths).await {
        let _ = foreground.kill();
        let _ = foreground.wait();
        panic!("foreground Alan instance did not become ready");
    }

    let stop = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["host", "stop", "--json"])
        .env("ALAN_INSTALL_CHANNEL", "stable")
        .env("ALAN_INSTANCE_RUNTIME_DIR", &runtime_dir)
        .env("HOME", &home)
        .env("TMPDIR", runtime.path())
        .output()
        .unwrap();
    if !stop.status.success() {
        let _ = foreground.kill();
        let _ = foreground.wait();
    }
    assert!(stop.status.success(), "{stop:?}");

    let Some(exited) = wait_for_child_exit(&mut foreground).await else {
        let _ = foreground.kill();
        let _ = foreground.wait();
        panic!("foreground Alan instance did not exit after host stop");
    };

    assert_eq!(exited.code(), Some(143));
    assert!(!paths.status.exists());
    assert!(!paths.socket.exists());
}

#[tokio::test]
async fn simultaneous_bare_cli_instances_have_independent_endpoints_and_shutdown() {
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let legacy_metadata = runtime.path().join("home/.alan/connections.toml");
    std::fs::create_dir_all(legacy_metadata.parent().unwrap()).unwrap();
    std::fs::write(&legacy_metadata, "version = 1\n").unwrap();
    let mut first = ForegroundChild(spawn_blocked_bare_cli(runtime.path(), None));
    let mut second = ForegroundChild(spawn_blocked_bare_cli(runtime.path(), None));

    let instances = async {
        for _ in 0..400 {
            let ready = std::fs::read_dir(runtime.path())
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("alan-"))
                .filter_map(|entry| {
                    let paths =
                        HostEndpointPaths::from_runtime_dir(&entry.path(), "stable").ok()?;
                    let status = paths.read_status().ok()?;
                    (status.readiness == HostReadiness::Ready).then_some((paths, status))
                })
                .collect::<Vec<_>>();
            if ready.len() == 2 {
                return ready;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Vec::new()
    }
    .await;
    assert_eq!(
        instances.len(),
        2,
        "both foreground instances must become ready"
    );
    assert_ne!(instances[0].1.boot_id, instances[1].1.boot_id);
    assert_ne!(instances[0].0.socket, instances[1].0.socket);
    assert!(
        !legacy_metadata.exists(),
        "concurrent first boot must finish shared legacy migration"
    );

    let first_instance = instances
        .iter()
        .find(|(_, status)| status.pid == first.0.id())
        .expect("first CLI must own a distinct Host endpoint");
    let second_instance = instances
        .iter()
        .find(|(_, status)| status.pid == second.0.id())
        .expect("second CLI must own a distinct Host endpoint");
    for (paths, _) in &instances {
        let attachment = LocalAttachment::new(paths.clone()).connect().await.unwrap();
        let shell = alan_shell::Shell::new(attachment.root);
        assert_eq!(shell.cat("/proc/1/status").await.unwrap(), b"running\n");
        assert!(shell.ls("/agent/root").await.is_ok());
    }

    let terminate_first = Command::new("/bin/kill")
        .args(["-TERM", &first.0.id().to_string()])
        .status()
        .unwrap();
    assert!(terminate_first.success());
    let Some(first_exit) = wait_for_child_exit(&mut first.0).await else {
        panic!("first foreground instance did not stop");
    };
    assert_eq!(first_exit.code(), Some(143));
    assert!(!first_instance.0.status.exists());
    assert!(!first_instance.0.socket.exists());
    assert_eq!(
        second_instance.0.read_status().unwrap().readiness,
        HostReadiness::Ready,
        "stopping one instance must leave the other available"
    );
    assert!(second.0.try_wait().unwrap().is_none());

    let terminate_second = Command::new("/bin/kill")
        .args(["-TERM", &second.0.id().to_string()])
        .status()
        .unwrap();
    assert!(terminate_second.success());
    let Some(second_exit) = wait_for_child_exit(&mut second.0).await else {
        panic!("second foreground instance did not stop");
    };
    assert_eq!(second_exit.code(), Some(143));
    assert!(!second_instance.0.status.exists());
    assert!(!second_instance.0.socket.exists());
}

#[tokio::test]
async fn ctrl_c_stops_bare_foreground_instance_while_stdin_is_open() {
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let runtime_dir = runtime.path().join("foreground");
    let paths = HostEndpointPaths::from_runtime_dir(&runtime_dir, "stable").unwrap();
    let mut foreground = spawn_blocked_bare_cli(runtime.path(), Some(&runtime_dir));
    if !wait_for_host_ready(&paths).await {
        let _ = foreground.kill();
        let _ = foreground.wait();
        panic!("foreground Alan instance did not become ready");
    }

    let interrupt = Command::new("/bin/kill")
        .args(["-INT", &foreground.id().to_string()])
        .status()
        .unwrap();
    if !interrupt.success() {
        let _ = foreground.kill();
        let _ = foreground.wait();
    }
    assert!(interrupt.success());

    let Some(exited) = wait_for_child_exit(&mut foreground).await else {
        let _ = foreground.kill();
        let _ = foreground.wait();
        panic!("foreground Alan instance did not exit after Ctrl-C");
    };

    assert_eq!(exited.code(), Some(130));
    assert!(!paths.status.exists());
    assert!(!paths.socket.exists());
}

#[tokio::test]
async fn sigterm_before_one_shot_input_exits_with_signal_status_and_removes_runtime_files() {
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let runtime_dir = runtime.path().join("foreground");
    let paths = HostEndpointPaths::from_runtime_dir(&runtime_dir, "stable").unwrap();
    let mut foreground = spawn_blocked_bare_cli(runtime.path(), Some(&runtime_dir));
    if !wait_for_host_ready(&paths).await {
        let _ = foreground.kill();
        let _ = foreground.wait();
        panic!("foreground Alan instance did not become ready");
    }

    let terminate = Command::new("/bin/kill")
        .args(["-TERM", &foreground.id().to_string()])
        .status()
        .unwrap();
    if !terminate.success() {
        let _ = foreground.kill();
        let _ = foreground.wait();
    }
    assert!(terminate.success());

    let Some(exited) = wait_for_child_exit(&mut foreground).await else {
        let _ = foreground.kill();
        let _ = foreground.wait();
        panic!("foreground Alan instance did not exit after SIGTERM");
    };

    assert_eq!(exited.code(), Some(143));
    assert!(!paths.status.exists());
    assert!(!paths.socket.exists());
}

#[test]
fn host_status_reports_stopping_without_attaching() {
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let base = runtime_base(runtime.path());
    let paths = HostEndpointPaths::from_runtime_dir(&base, "stable").unwrap();
    std::fs::create_dir_all(&paths.root).unwrap();
    let status = HostStatus {
        version: 1,
        local_attachment_protocol_version: 2,
        channel_id: "stable".to_string(),
        boot_id: uuid::Uuid::new_v4(),
        pid: std::process::id(),
        readiness: HostReadiness::Stopping,
        socket: paths.socket,
    };
    std::fs::write(&paths.status, serde_json::to_vec(&status).unwrap()).unwrap();
    std::fs::set_permissions(&paths.status, std::fs::Permissions::from_mode(0o600)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["host", "status", "--json"])
        .env("ALAN_INSTALL_CHANNEL", "stable")
        .env("ALAN_INSTANCE_RUNTIME_DIR", &base)
        .env("TMPDIR", runtime.path().join("unused"))
        .env("XDG_RUNTIME_DIR", runtime.path().join("unused"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let reported: HostStatus = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reported.readiness, HostReadiness::Stopping);
}

#[test]
fn live_host_commands_require_an_explicit_instance() {
    for args in [
        vec!["host", "status"],
        vec!["host", "stop"],
        vec!["host", "mount", "list"],
        vec!["host", "mount", "approve", "request", "/tmp"],
        vec!["host", "mount", "revoke", "grant"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_alan"))
            .env_remove("ALAN_INSTANCE_RUNTIME_DIR")
            .args(&args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("select a live Alan instance with ALAN_INSTANCE_RUNTIME_DIR"),
            "{output:?}"
        );
    }
}

#[test]
fn dedicated_host_start_guides_users_to_foreground_alan() {
    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["host", "start"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("run bare `alan` to start a foreground instance"),
        "{output:?}"
    );
}
