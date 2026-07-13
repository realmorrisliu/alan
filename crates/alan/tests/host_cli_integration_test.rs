use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

#[tokio::test]
async fn cli_exit_detaches_without_stopping_the_host_or_root_agent() {
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

    let temporary_root = runtime.path().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_alan"))
            .env("ALAN_INSTALL_CHANNEL", "stable")
            .env("TMPDIR", temporary_root)
            .env("XDG_RUNTIME_DIR", base)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"exit\n").unwrap();
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    assert!(output.status.success(), "{output:?}");

    let attachment = LocalAttachment::new(paths).connect().await.unwrap();
    let shell = alan_shell::Shell::new(attachment.root);
    assert!(shell.ls("/agent/root").await.is_ok());
    assert_eq!(shell.cat("/proc/1/status").await.unwrap(), b"running\n");

    let _ = shutdown.send(());
    server.await.unwrap().unwrap();
}

#[test]
fn host_status_reports_stopping_without_attaching() {
    let runtime = tempfile::tempdir_in("/tmp").unwrap();
    let base = runtime_base(runtime.path());
    let paths = HostEndpointPaths::from_runtime_dir(&base, "stable").unwrap();
    std::fs::create_dir_all(&paths.root).unwrap();
    let status = HostStatus {
        version: 1,
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
        .env("TMPDIR", runtime.path())
        .env("XDG_RUNTIME_DIR", &base)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let reported: HostStatus = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reported.readiness, HostReadiness::Stopping);
}
