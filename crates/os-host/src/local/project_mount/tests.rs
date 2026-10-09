use super::*;

#[tokio::test]
async fn project_reply_requires_boot_operation_and_valid_error_shape() {
    let runtime = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path()).unwrap();
    paths.prepare_private_root().unwrap();
    let boot = Uuid::new_v4();
    let mut status = HostStatus {
        version: STATUS_VERSION,
        local_attachment_protocol_version: 3,

        boot_id: boot,
        pid: std::process::id(),
        readiness: HostReadiness::Ready,
        socket: paths.socket.clone(),
    };
    write_status(&paths.status, &status).unwrap();
    let client = HostCommandPlane::new(paths.clone());
    let operation = Uuid::new_v4();
    let old = client
        .mount_project(
            operation,
            boot,
            "/fixture".into(),
            HostMountAccess::ReadOnly,
        )
        .await
        .unwrap_err();
    assert!(
        old.downcast_ref::<ProjectMountRejected>().is_some(),
        "old Host must reject before sending"
    );
    status.local_attachment_protocol_version = LOCAL_ATTACHMENT_PROTOCOL_VERSION;
    write_status(&paths.status, &status).unwrap();
    let listener = UnixListener::bind(&paths.socket).unwrap();
    let server = tokio::spawn(async move {
        for mode in 0..4 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let LocalRequest::MountProject(request) =
                read_local_request(&mut stream).await.unwrap()
            else {
                panic!("project request")
            };
            if mode == 2 {
                continue;
            } // transport EOF is unknown, never a rejection.
            write_local_response(
                &mut stream,
                LocalResponse {
                    operation_id: Some(if mode == 0 {
                        Uuid::new_v4()
                    } else {
                        request.operation_id
                    }),
                    boot_id: boot,
                    grant: None,
                    host_path: if mode == 1 {
                        Some("/contradictory".into())
                    } else {
                        None
                    },
                    error: Some("known rejection".into()),
                },
            )
            .await
            .unwrap();
        }
    });
    for mode in 0..4 {
        let error = client
            .mount_project(
                operation,
                boot,
                "/fixture".into(),
                HostMountAccess::ReadOnly,
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProjectMountRejected>().is_some(),
            mode == 3,
            "only correlated, internally consistent rejection releases the operation"
        );
    }
    server.await.unwrap();
}
