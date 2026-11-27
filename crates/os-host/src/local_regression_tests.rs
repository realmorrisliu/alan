use std::os::unix::fs::PermissionsExt;

use super::{
    HostEndpointPaths, LocalAttachment, STATUS_VERSION, SingletonLock,
    UnsupportedProcesslessAttachment,
};
use tokio::net::UnixListener;
use uuid::Uuid;

#[tokio::test]
async fn processless_attach_rejects_a_live_legacy_host_without_sending_legacy_attach() {
    let runtime = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path()).unwrap();
    std::fs::create_dir_all(&paths.root).unwrap();
    let status = serde_json::json!({
        "version": STATUS_VERSION,
        "boot_id": Uuid::new_v4(),
        "pid": 1,
        "readiness": "ready",
        "socket": paths.socket,
    });
    std::fs::write(&paths.status, serde_json::to_vec(&status).unwrap()).unwrap();
    std::fs::set_permissions(&paths.status, std::fs::Permissions::from_mode(0o600)).unwrap();
    let _listener = UnixListener::bind(&paths.socket).unwrap();
    let _lock = SingletonLock::acquire(&paths.lock).unwrap();

    let error = LocalAttachment::new(paths).connect().await.err().unwrap();
    assert!(
        error
            .downcast_ref::<UnsupportedProcesslessAttachment>()
            .is_some()
    );
    assert!(error.to_string().contains("run `alan host stop` and retry"));
}

#[tokio::test]
async fn stale_legacy_status_is_not_reported_as_protocol_incompatibility() {
    let runtime = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path()).unwrap();
    std::fs::create_dir_all(&paths.root).unwrap();
    let status = serde_json::json!({
        "version": STATUS_VERSION,
        "boot_id": Uuid::new_v4(),
        "pid": 1,
        "readiness": "ready",
        "socket": paths.socket,
    });
    std::fs::write(&paths.status, serde_json::to_vec(&status).unwrap()).unwrap();
    std::fs::set_permissions(&paths.status, std::fs::Permissions::from_mode(0o600)).unwrap();

    let error = LocalAttachment::new(paths).connect().await.err().unwrap();
    assert!(
        error
            .downcast_ref::<UnsupportedProcesslessAttachment>()
            .is_none()
    );
}

#[tokio::test]
async fn namespace_ap_sock_attachments_enforce_client_actor_view() {
    use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry};
    use alan_ap::{ErrorCode, Fid, OpenMode, Request, Response};
    use alan_llm::MockLlmProvider;
    use tokio_util::sync::CancellationToken;

    let runtime = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path()).unwrap();
    let host = super::AlanOsHost::boot(
        crate::HostBootConfig::ephemeral(
            AgentProcessConfig::default(),
            LlmClient::new(MockLlmProvider::new()),
            ToolRegistry::new(),
        ),
        paths.clone(),
    )
    .await
    .unwrap();
    let shutdown = CancellationToken::new();
    let shutdown_req = shutdown.clone();
    let server =
        tokio::spawn(async move { host.serve_until(shutdown_req.cancelled_owned()).await });

    let client_attach = LocalAttachment::new(paths.clone()).connect().await.unwrap();
    let shell_attach = LocalAttachment::new(paths)
        .connect_shell_process()
        .await
        .unwrap();

    for (label, transport) in [
        ("attach_client", client_attach.root.clone()),
        ("attach_shell", shell_attach.root.clone()),
    ] {
        for (idx, path) in [
            "/agent/root/io/output",
            "/agent/root/machine/tape",
            "/agent/root/machine/ui/activity",
            "/agent/root/requests/clone",
            "/agent/root/actions/clone",
        ]
        .into_iter()
        .enumerate()
        {
            let fid = Fid(500 + idx as u64);
            let names: Vec<String> = path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            transport
                .call(Request::Walk {
                    fid: Fid::ROOT,
                    newfid: fid,
                    names,
                })
                .await
                .unwrap();
            let Response::Stat { stat } = transport.call(Request::Stat { fid }).await.unwrap()
            else {
                panic!("expected Stat for {path}");
            };
            assert!(
                !stat.writable,
                "{label}: {path} must report stat.writable == false"
            );
            for mode in [
                OpenMode::Write,
                OpenMode::WriteExclusive,
                OpenMode::ReadWrite,
            ] {
                assert_eq!(
                    transport.call(Request::Open { fid, mode }).await,
                    Err(ErrorCode::NoAccess),
                    "{label}: {path} open({mode:?}) must return NoAccess"
                );
            }
            transport.call(Request::Clunk { fid }).await.unwrap();
        }
        let shell = alan_shell::Shell::new(transport);
        assert!(shell.cat("/agent/root/io/output").await.is_ok());
        shell
            .write("/agent/root/machine/ctl", b"interrupt")
            .await
            .unwrap();
    }

    drop(shell_attach);
    drop(client_attach);
    shutdown.cancel();
    server.await.unwrap().unwrap();
}
