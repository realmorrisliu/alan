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
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    std::fs::create_dir_all(&paths.root).unwrap();
    let status = serde_json::json!({
        "version": STATUS_VERSION,
        "channel_id": paths.channel_id,
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
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    std::fs::create_dir_all(&paths.root).unwrap();
    let status = serde_json::json!({
        "version": STATUS_VERSION,
        "channel_id": paths.channel_id,
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
