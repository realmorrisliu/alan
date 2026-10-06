use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn send(
    paths: &HostEndpointPaths,
    request: &serde_json::Value,
    reply: bool,
) -> Option<serde_json::Value> {
    let mut socket = tokio::net::UnixStream::connect(&paths.socket)
        .await
        .unwrap();
    let bytes = serde_json::to_vec(request).unwrap();
    socket.write_u32(bytes.len() as u32).await.unwrap();
    socket.write_all(&bytes).await.unwrap();
    if !reply {
        return None;
    }
    let size = socket.read_u32().await.unwrap();
    let mut bytes = vec![0; size as usize];
    socket.read_exact(&mut bytes).await.unwrap();
    Some(serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn project_reply_loss_reconciles_same_grant_and_never_reauthorizes_revoked_operation() {
    let _guard = TEST_HOST_LOCK.lock().await;
    let runtime = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    let host = AlanOsHost::boot(
        mount_request_config(
            &runtime.path().join("store"),
            Arc::new(AtomicBool::new(false)),
        ),
        paths.clone(),
    )
    .await
    .unwrap();
    let boot = host.status().boot_id;
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    let server = tokio::spawn(async move { host.serve_until(stop.cancelled_owned()).await });
    let attachment = LocalAttachment::new(paths.clone()).connect().await.unwrap();
    let shell = alan_shell::Shell::new(attachment.root);
    let request = serde_json::json!({"op":"mount_project", "operation_id":uuid::Uuid::new_v4(), "expected_boot":boot, "host_path":project.path(), "access":"read_only"});
    send(&paths, &request, false).await;
    let observed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let grants = shell.ls("/mnt/host-mount/grants").await.unwrap();
            if grants.len() == 1 {
                break grants[0].clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let reply = send(&paths, &request, true).await.unwrap();
    let first = observed.unwrap();
    let control = HostCommandPlane::new(paths.clone());
    let client_retry = control
        .mount_project(
            uuid::Uuid::parse_str(request["operation_id"].as_str().unwrap()).unwrap(),
            boot,
            project.path().to_owned(),
            HostMountAccess::ReadOnly,
        )
        .await
        .unwrap();
    let same = reply["grant"]["id"].as_str() == Some(&first);
    let mut changed = request.clone();
    changed["access"] = "read_write".into();
    let changed_reply = send(&paths, &changed, true).await.unwrap();
    let mut changed_path = request.clone();
    changed_path["host_path"] = runtime.path().to_string_lossy().into_owned().into();
    let changed_path_reply = send(&paths, &changed_path, true).await.unwrap();
    let mut wrong_boot = request.clone();
    wrong_boot["expected_boot"] = uuid::Uuid::new_v4().to_string().into();
    let wrong_boot_reply = send(&paths, &wrong_boot, true).await.unwrap();
    let (a, b) = tokio::join!(send(&paths, &request, true), send(&paths, &request, true));
    let concurrent_same = [a.unwrap(), b.unwrap()]
        .iter()
        .all(|r| r["grant"]["id"].as_str() == Some(&first));
    // Collect after cleanup so a regression never leaves the Host fixture live.
    let revoke = HostCommandPlane::new(paths.clone())
        .revoke_host_mount(first.clone())
        .await;
    let revoked_reply = send(&paths, &request, true).await.unwrap();
    let count = shell.ls("/mnt/host-mount/grants").await.unwrap().len();
    shutdown.cancel();
    server.await.unwrap().unwrap();
    assert!(
        same,
        "lost reply must resolve original grant, reply={reply}"
    );
    assert_eq!(client_retry.grant.id, first);
    assert!(changed_path_reply["error"].is_string());
    assert!(changed_reply["error"].is_string());
    assert!(wrong_boot_reply["error"].is_string());
    assert!(concurrent_same);
    revoke.unwrap();
    assert!(
        revoked_reply["error"].is_string(),
        "revoked operation must not create new authority"
    );
    assert_eq!(count, 1);
}
