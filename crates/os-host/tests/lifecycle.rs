use std::os::unix::fs::{FileTypeExt, PermissionsExt, symlink};
use std::time::Duration;

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry};
use alan_llm::{GenerationResponse, MockLlmProvider};
use alan_os_host::{
    AlanOsHost, FixedBootConfig, HostEndpointPaths, HostStorePaths, LocalAttachment,
    SystemStorePaths,
};
use tokio_util::sync::CancellationToken;

static TEST_HOST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn response(content: &str) -> GenerationResponse {
    GenerationResponse {
        content: content.to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

fn config() -> FixedBootConfig {
    config_for("test")
}

fn config_for(channel_id: &str) -> FixedBootConfig {
    FixedBootConfig::ephemeral(
        channel_id,
        AgentProcessConfig::default(),
        LlmClient::new(
            MockLlmProvider::new().with_responses(vec![response("one"), response("two")]),
        ),
        ToolRegistry::new(),
    )
}

async fn wait_for_turn_idle(events: &mut alan_shell::Tail, phase: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut pending = String::new();
        let mut saw_running = false;
        loop {
            pending.push_str(&String::from_utf8(events.read(4096).await.unwrap()).unwrap());
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].to_string();
                pending.drain(..=newline);
                let event: serde_json::Value = serde_json::from_str(&line).unwrap();
                let state = event
                    .get("snapshot")
                    .and_then(|snapshot| snapshot.get("state"))
                    .and_then(serde_json::Value::as_str);
                saw_running |= state == Some("running");
                if saw_running && state == Some("idle") {
                    return;
                }
            }
        }
    })
    .await
    .unwrap_or_else(|error| panic!("Root Agent {phase} turn did not reach idle: {error}"));
}

#[tokio::test]
async fn attachment_disconnect_and_host_restart_preserve_only_durable_identity() {
    let _host_guard = TEST_HOST_LOCK.lock().await;
    let runtime = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    let host = AlanOsHost::boot(config(), paths.clone()).await.unwrap();
    assert_eq!(
        std::fs::metadata(&paths.root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&paths.status)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let socket = std::fs::symlink_metadata(&paths.socket).unwrap();
    assert!(socket.file_type().is_socket());
    assert_eq!(socket.permissions().mode() & 0o777, 0o600);
    let first_status = host.status().clone();
    let shutdown = CancellationToken::new();
    let shutdown_request = shutdown.clone();
    let server =
        tokio::spawn(async move { host.serve_until(shutdown_request.cancelled_owned()).await });

    let first = LocalAttachment::new(paths.clone()).connect().await.unwrap();
    let old_reference = first.process_reference(1);
    let input = "/agent/root/io/input";
    let first_shell = alan_shell::Shell::new(first.root.clone());
    let mut activity = first_shell
        .tail("/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut first_tail = first_shell.tail(input).await.unwrap();
    let blocked_read = tokio::spawn(async move {
        let bytes = first_tail.read(4096).await;
        (first_tail, bytes)
    });
    tokio::task::yield_now().await;
    let boot_id = tokio::time::timeout(
        Duration::from_secs(5),
        first_shell.cat("/proc/host/boot_id"),
    )
    .await
    .expect("an unrelated aP call must not wait behind a blocking stream read")
    .unwrap();
    assert_eq!(
        String::from_utf8(boot_id).unwrap().trim(),
        first_status.boot_id.to_string()
    );
    tokio::time::timeout(Duration::from_secs(5), first_shell.write(input, b"first"))
        .await
        .expect("input commit must not wait behind a blocking read")
        .unwrap();
    let (first_tail, first_bytes) = tokio::time::timeout(Duration::from_secs(5), blocked_read)
        .await
        .unwrap()
        .unwrap();
    let first_bytes = first_bytes.unwrap();
    assert_eq!(first_bytes, b"5\nfirst");
    wait_for_turn_idle(&mut activity, "first").await;
    let offset = first_tail.offset();
    let activity_offset = activity.offset();
    first_tail.close().await.unwrap();
    activity.close().await.unwrap();
    drop(first_shell);
    drop(first);

    let reattached = LocalAttachment::new(paths.clone()).connect().await.unwrap();
    assert_eq!(reattached.boot_id, first_status.boot_id);
    old_reference.validate(&reattached).unwrap();
    let second_shell = alan_shell::Shell::new(reattached.root.clone());
    let mut activity = second_shell
        .tail_from("/agent/root/machine/ui/events", activity_offset)
        .await
        .unwrap();
    let mut resumed = second_shell.tail_from(input, offset).await.unwrap();
    second_shell.write(input, b"second").await.unwrap();
    let second_bytes = tokio::time::timeout(Duration::from_secs(5), resumed.read(4096))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second_bytes, b"6\nsecond");
    wait_for_turn_idle(&mut activity, "reattached").await;

    let second_host = AlanOsHost::boot(config(), paths.clone()).await;
    assert!(second_host.is_err(), "a channel must have only one Host");

    drop(resumed);
    activity.close().await.unwrap();
    drop(second_shell);
    drop(reattached);
    shutdown.cancel();
    server.await.unwrap().unwrap();

    let restarted = AlanOsHost::boot(config(), paths.clone()).await.unwrap();
    let restarted_status = restarted.status().clone();
    assert_ne!(restarted_status.boot_id, first_status.boot_id);
    let restart_shutdown = CancellationToken::new();
    let restart_request = restart_shutdown.clone();
    let restarted_server = tokio::spawn(async move {
        restarted
            .serve_until(restart_request.cancelled_owned())
            .await
    });
    let new_attachment = LocalAttachment::new(paths).connect().await.unwrap();
    assert!(old_reference.validate(&new_attachment).is_err());
    restart_shutdown.cancel();
    restarted_server.await.unwrap().unwrap();
}

#[tokio::test]
async fn host_rejects_a_symlinked_runtime_root() {
    let _host_guard = TEST_HOST_LOCK.lock().await;
    let runtime = tempfile::tempdir().unwrap();
    let redirected = runtime.path().join("redirected");
    symlink(runtime.path(), &redirected).unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(&redirected, "test").unwrap();

    assert!(AlanOsHost::boot(config(), paths).await.is_err());
}

#[tokio::test]
async fn attachment_rejects_a_symlinked_status_file() {
    let _host_guard = TEST_HOST_LOCK.lock().await;
    let runtime = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    let host = AlanOsHost::boot(config(), paths.clone()).await.unwrap();
    let redirected = runtime.path().join("redirected-status");
    std::fs::write(&redirected, serde_json::to_vec(host.status()).unwrap()).unwrap();
    std::fs::remove_file(&paths.status).unwrap();
    symlink(&redirected, &paths.status).unwrap();

    assert!(paths.read_status().is_err());
    host.serve_until(async {}).await.unwrap();
}

#[tokio::test]
async fn stable_and_dev_hosts_stores_endpoints_and_clients_are_isolated() {
    let _host_guard = TEST_HOST_LOCK.lock().await;
    let runtime = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let stable_paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "stable").unwrap();
    let dev_paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "dev").unwrap();
    assert_ne!(stable_paths.root, dev_paths.root);
    assert_ne!(stable_paths.socket, dev_paths.socket);

    let stable_system = SystemStorePaths::from_data_dir(data.path(), "stable").unwrap();
    let dev_system = SystemStorePaths::from_data_dir(data.path(), "dev").unwrap();
    let stable_host_store = HostStorePaths::from_data_dir(data.path(), "stable").unwrap();
    let dev_host_store = HostStorePaths::from_data_dir(data.path(), "dev").unwrap();
    assert_ne!(stable_system.root, dev_system.root);
    assert_ne!(stable_host_store.credentials, dev_host_store.credentials);
    assert_ne!(stable_host_store.managed_auth, dev_host_store.managed_auth);

    let stable = AlanOsHost::boot(config_for("stable"), stable_paths.clone())
        .await
        .unwrap();
    let dev = AlanOsHost::boot(config_for("dev"), dev_paths.clone())
        .await
        .unwrap();
    let stable_shutdown = CancellationToken::new();
    let stable_request = stable_shutdown.clone();
    let stable_server =
        tokio::spawn(async move { stable.serve_until(stable_request.cancelled_owned()).await });
    let dev_shutdown = CancellationToken::new();
    let dev_request = dev_shutdown.clone();
    let dev_server =
        tokio::spawn(async move { dev.serve_until(dev_request.cancelled_owned()).await });

    let stable_attachment = LocalAttachment::new(stable_paths.clone())
        .connect()
        .await
        .unwrap();
    let dev_attachment = LocalAttachment::new(dev_paths).connect().await.unwrap();
    assert_eq!(stable_attachment.status.channel_id, "stable");
    assert_eq!(dev_attachment.status.channel_id, "dev");
    assert_ne!(stable_attachment.boot_id, dev_attachment.boot_id);

    let mismatched_client = HostEndpointPaths {
        channel_id: "dev".to_string(),
        root: stable_paths.root,
        socket: stable_paths.socket,
        status: stable_paths.status,
        lock: stable_paths.lock,
    };
    assert!(
        LocalAttachment::new(mismatched_client)
            .connect()
            .await
            .is_err()
    );

    drop(stable_attachment);
    drop(dev_attachment);
    stable_shutdown.cancel();
    dev_shutdown.cancel();
    stable_server.await.unwrap().unwrap();
    dev_server.await.unwrap().unwrap();
}

#[tokio::test]
async fn shell_client_exit_detaches_without_stopping_host_or_root_agent() {
    let _host_guard = TEST_HOST_LOCK.lock().await;
    let runtime = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    let host = AlanOsHost::boot(config(), paths.clone()).await.unwrap();
    let boot_id = host.status().boot_id;
    let shutdown = CancellationToken::new();
    let shutdown_request = shutdown.clone();
    let server =
        tokio::spawn(async move { host.serve_until(shutdown_request.cancelled_owned()).await });

    {
        let attachment = LocalAttachment::new(paths.clone()).connect().await.unwrap();
        let driver = alan_shell::StdioDriver::new(alan_shell::Shell::new(attachment.root));
        driver
            .run(tokio::io::BufReader::new(&b"exit\n"[..]), tokio::io::sink())
            .await
            .unwrap();
    }

    let reattached = LocalAttachment::new(paths).connect().await.unwrap();
    assert_eq!(reattached.boot_id, boot_id);
    let shell = alan_shell::Shell::new(reattached.root);
    assert_eq!(shell.cat("/proc/1/status").await.unwrap(), b"running\n");

    shutdown.cancel();
    server.await.unwrap().unwrap();
}
