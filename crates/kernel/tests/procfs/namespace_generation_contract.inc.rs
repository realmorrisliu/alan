#[tokio::test]
async fn live_spawner_view_exposes_current_namespace_at_proc_self() {
    let fs = ProcFs::new();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );
    let bootstrap = fs.for_spawner(None, namespace.clone(), Credentials::system());
    let parent = spawn_with_mounts(
        &bootstrap,
        Fid(70),
        serde_json::json!([{"path": "/bin", "access": "ro"}]),
    )
    .await;
    let live_namespace = LiveNamespace::new(namespace);
    let current = fs.for_live_spawner(
        Some(Pid(parent.parse().unwrap())),
        live_namespace.clone(),
        Credentials::user("alan"),
    );

    let listing = String::from_utf8(read_at(&current, &[], Fid(71)).await.unwrap()).unwrap();
    assert!(listing.lines().any(|name| name == "self"));
    assert_eq!(
        read_at(&current, &["self", "namespace"], Fid(72))
            .await
            .unwrap(),
        b"/bin ro"
    );
    live_namespace.mount(
        "/mnt/project",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadWrite,
    );
    assert_eq!(
        read_at(&current, &["self", "namespace"], Fid(74))
            .await
            .unwrap(),
        b"/bin ro\n/mnt/project rw"
    );
    assert_eq!(
        read_at(&fs, &["self", "namespace"], Fid(73)).await,
        Err(ErrorCode::NotFound),
        "a bootstrap /proc view without a current Process has no /proc/self"
    );
}

#[tokio::test]
async fn clone_exec_namespace_manifest_preserves_mixed_access_union_order() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    namespace.mount(
        "/bin",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    let manifest = alan_kernel::ExecNamespaceManifest::from_snapshot(&namespace, 0);
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(32), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(32), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(32), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": manifest
    })
    .to_string();
    spawner.write(Fid(32), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(32)).await, Ok(()));

    let namespace = String::from_utf8(
        read_at(&fs, &[&pid_name, "namespace"], Fid(33))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        namespace
            .lines()
            .filter(|line| line.starts_with("/bin "))
            .collect::<Vec<_>>(),
        vec!["/bin rw", "/bin ro"]
    );
}

#[tokio::test]
async fn clone_rejects_a_stale_live_namespace_generation_and_accepts_a_fresh_retry() {
    let fs = ProcFs::new();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/mnt",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadWrite,
    );
    let live_namespace = LiveNamespace::new(namespace);
    let spawner = fs.for_live_spawner(None, live_namespace.clone(), Credentials::system());
    let (stale_snapshot, stale_generation) = live_namespace.snapshot_with_generation();

    live_namespace.mount(
        "/mnt/project",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadWrite,
    );

    spawner
        .walk(Fid::ROOT, Fid(80), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(80), OpenMode::ReadWrite).await.unwrap();
    let stale_pid = String::from_utf8(spawner.read(Fid(80), 0, 64).await.unwrap()).unwrap();
    let stale_exec = alan_kernel::ExecSpec {
        executable: "/bin/agent".to_string(),
        args: Vec::new(),
        namespace: alan_kernel::ExecNamespaceManifest::from_snapshot(
            &stale_snapshot,
            stale_generation,
        ),
        descriptors: Default::default(),
    };
    spawner
        .write(Fid(80), 0, &serde_json::to_vec(&stale_exec).unwrap())
        .await
        .unwrap();
    assert_eq!(spawner.clunk(Fid(80)).await, Err(ErrorCode::BadRequest));
    assert!(
        !String::from_utf8(fs.read(Fid::ROOT, 0, 4096).await.unwrap())
            .unwrap()
            .lines()
            .any(|entry| entry == stale_pid),
        "a stale launch must not publish its pending Process"
    );

    let (fresh_snapshot, fresh_generation) = live_namespace.snapshot_with_generation();
    spawner
        .walk(Fid::ROOT, Fid(81), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(81), OpenMode::ReadWrite).await.unwrap();
    let fresh_pid = String::from_utf8(spawner.read(Fid(81), 0, 64).await.unwrap()).unwrap();
    let fresh_exec = alan_kernel::ExecSpec {
        executable: "/bin/agent".to_string(),
        args: Vec::new(),
        namespace: alan_kernel::ExecNamespaceManifest::from_snapshot(
            &fresh_snapshot,
            fresh_generation,
        ),
        descriptors: Default::default(),
    };
    spawner
        .write(Fid(81), 0, &serde_json::to_vec(&fresh_exec).unwrap())
        .await
        .unwrap();
    assert_eq!(spawner.clunk(Fid(81)).await, Ok(()));

    let child_namespace = String::from_utf8(
        read_at(&fs, &[&fresh_pid, "namespace"], Fid(82))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(child_namespace.lines().any(|line| line == "/mnt rw"));
    assert!(
        child_namespace
            .lines()
            .any(|line| line == "/mnt/project rw")
    );
}
