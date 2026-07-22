
#[tokio::test]
async fn proc_output_observers_see_direct_writes() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    let sink = Arc::new(RecordingOutputSink::new());
    fs.subscribe_process_output(&pid, sink.clone())
        .await
        .unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(11),
        &[pid.clone(), "io".into(), "output".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(11), OpenMode::Write).await.unwrap();
    fs.write(Fid(11), 0, b"hello proc").await.unwrap();
    fs.clunk(Fid(11)).await.unwrap();

    assert_eq!(sink.wait_for(1).await, vec![(pid, 10)]);
}

#[tokio::test]
async fn proc_input_observers_see_direct_writes() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    let sink = Arc::new(RecordingInputSink::new());
    fs.subscribe_process_input(&pid, sink.clone())
        .await
        .unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(11),
        &[pid.clone(), "io".to_string(), "input".to_string()],
    )
    .await
    .unwrap();
    fs.open(Fid(11), OpenMode::Write).await.unwrap();
    fs.write(Fid(11), 0, b"hello proc").await.unwrap();
    fs.clunk(Fid(11)).await.unwrap();

    assert_eq!(sink.wait_for(1).await, vec![(pid, 10)]);
}

#[tokio::test]
async fn registered_runner_writes_process_output_and_exit() {
    let fs = ProcFs::new().with_runner(Arc::new(EchoRunner));
    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    let pid = spawn_exec(
        &spawner,
        Fid(10),
        "/bin/greeting",
        vec!["hello".into(), "tool".into()],
        0,
        serde_json::json!([{"path": "/bin", "access": "ro"}]),
    )
    .await;

    for attempt in 0..50 {
        let status = String::from_utf8(
            read_at(&fs, &[&pid, "status"], Fid(100 + attempt))
                .await
                .unwrap(),
        )
        .unwrap();
        if status.trim() == "exited" {
            let output = String::from_utf8(
                read_at(&fs, &[&pid, "io", "output"], Fid(200))
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(output, "hello tool\n");
            let snapshot = fs
                .observe_process_files(Pid(pid.parse().unwrap()))
                .await
                .unwrap();
            assert_eq!(snapshot.status, alan_kernel::Status::Exited);
            assert_eq!(snapshot.exit_code, Some(0));
            assert_eq!(snapshot.output, b"hello tool\n");
            assert_eq!(snapshot.output_offset, 11);
            let io_events = String::from_utf8(
                read_at(&fs, &[&pid, "io", "events"], Fid(202))
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(io_events, "output:11\n");
            let exit =
                String::from_utf8(read_at(&fs, &[&pid, "exit"], Fid(201)).await.unwrap()).unwrap();
            assert_eq!(exit, "0");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("runner did not exit process {pid}");
}

#[tokio::test]
async fn proc_output_observers_see_runner_output() {
    let runner = Arc::new(DelayedOutputRunner::new("runner output\n"));
    let fs = ProcFs::new().with_runner(runner.clone());
    let pid = spawn(&fs, Fid(10)).await;
    let sink = Arc::new(RecordingOutputSink::new());
    fs.subscribe_process_output(&pid, sink.clone())
        .await
        .unwrap();

    runner.release();

    assert_eq!(sink.wait_for(1).await, vec![(pid, 14)]);
}

// Spawning requires write intent: opening /proc/clone read-only is rejected, and
// a ctl write needs write authority (PR #574 review).
#[tokio::test]
async fn write_surfaces_require_write_intent() {
    let fs = proc();

    // Read-only open of clone cannot allocate a (would-be leaked) pending slot.
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(10), OpenMode::Read).await,
        Err(ErrorCode::NoAccess)
    );

    // ctl opened read-only cannot cancel the process.
    let pid = spawn(&fs, Fid(20)).await;
    fs.walk(Fid::ROOT, Fid(21), &[pid.clone(), "ctl".into()])
        .await
        .unwrap();
    fs.open(Fid(21), OpenMode::Read).await.unwrap();
    assert_eq!(
        fs.write(Fid(21), 0, b"cancel").await,
        Err(ErrorCode::NoAccess)
    );
    // Still running — the read-only cancel did not take effect.
    fs.walk(Fid::ROOT, Fid(22), &[pid, "status".into()])
        .await
        .unwrap();
    fs.open(Fid(22), OpenMode::Read).await.unwrap();
    assert_eq!(
        String::from_utf8(fs.read(Fid(22), 0, 64).await.unwrap())
            .unwrap()
            .trim(),
        "running"
    );
}

// walk rejects reused/reserved newfids; open rejects reopening a live fid — so a
// retry cannot clobber a pending clone slot (PR #574 review).
#[tokio::test]
async fn fid_reuse_and_reopen_are_rejected() {
    let fs = proc();
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    // Reusing a live fid is rejected, not a silent clobber.
    assert_eq!(
        fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()]).await,
        Err(ErrorCode::BadRequest)
    );
    // Reopening a live fid before clunk is rejected.
    fs.open(Fid(10), OpenMode::ReadWrite).await.unwrap();
    assert_eq!(
        fs.open(Fid(10), OpenMode::ReadWrite).await,
        Err(ErrorCode::BadRequest)
    );
}

// The clone exec-spec write honors byte offsets, so out-of-order chunks build the
// addressed document (PR #574 review).
#[tokio::test]
async fn clone_exec_spec_write_honors_offset() {
    let fs = proc();
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    fs.open(Fid(10), OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(fs.read(Fid(10), 0, 64).await.unwrap()).unwrap();
    // Write the tail first (at offset 14), then the head (offset 0).
    fs.write(
        Fid(10),
        14,
        br#""/bin/agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#,
    )
        .await
        .unwrap();
    fs.write(Fid(10), 0, br#"{"executable":"#).await.unwrap();
    assert_eq!(fs.clunk(Fid(10)).await, Ok(()));
    // Committed cleanly → the process is public.
    let listing = String::from_utf8(read_at(&fs, &[], Fid(11)).await.unwrap()).unwrap();
    assert!(
        listing.lines().any(|l| l == pid),
        "offset-assembled spec spawned the process"
    );
}

#[tokio::test]
async fn clone_rejects_missing_namespace_manifest_without_leaking_a_process() {
    let fs = proc();
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    fs.open(Fid(10), OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(fs.read(Fid(10), 0, 64).await.unwrap()).unwrap();
    fs.write(
        Fid(10),
        0,
        br#"{"executable":"/bin/agent","args":[]}"#,
    )
    .await
    .unwrap();

    assert_eq!(fs.clunk(Fid(10)).await, Err(ErrorCode::BadRequest));
    let listing = String::from_utf8(read_at(&fs, &[], Fid(11)).await.unwrap()).unwrap();
    assert!(!listing.lines().any(|entry| entry == pid));
}

// stat reports the readable byte length, so clients can size reads (PR #574).
#[tokio::test]
async fn stat_reports_readable_length() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    fs.walk(Fid::ROOT, Fid(11), &[pid, "status".into()])
        .await
        .unwrap();
    let st = fs.stat(Fid(11)).await.unwrap();
    // status reads "running\n" (8 bytes); stat must not report 0.
    assert_eq!(st.length, 8, "stat length matches the bytes read returns");
}

// The pre-bound /proc root fid can be opened directly (no redundant empty walk),
// matching SrvFs and the reference server (PR #574 review).
#[tokio::test]
async fn root_fid_is_openable_directly() {
    let fs = proc();
    fs.open(Fid::ROOT, OpenMode::Read)
        .await
        .expect("root fid opens directly");
    let listing = String::from_utf8(fs.read(Fid::ROOT, 0, 64).await.unwrap()).unwrap();
    assert!(
        listing.lines().any(|l| l == "clone"),
        "root listing is readable via the root fid"
    );
}

// /proc/<pid>/namespace renders the process's mounted capability set
// (PR #574 review).
#[tokio::test]
async fn proc_exposes_the_process_namespace() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    // The file exists and is listed in the process directory.
    let dir = String::from_utf8(read_at(&fs, &[&pid], Fid(11)).await.unwrap()).unwrap();
    assert!(
        dir.lines().any(|l| l == "namespace"),
        "namespace is listed: {dir:?}"
    );
    // And it reads (empty for a system-spawned process with an empty namespace).
    read_at(&fs, &[&pid, "namespace"], Fid(12))
        .await
        .expect("namespace is readable");
}

#[tokio::test]
async fn proc_exposes_only_valid_descriptors_bound_inside_the_committed_namespace() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/definition",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::system());

    spawner
        .walk(Fid::ROOT, Fid(13), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(13), OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(spawner.read(Fid(13), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/alan-agent",
        "namespace": {"generation": 0,"mounts": [{"path": "/definition", "access": "ro"}]},
        "descriptors": {"3": "/definition"}
    });
    spawner
        .write(Fid(13), 0, exec.to_string().as_bytes())
        .await
        .unwrap();
    assert_eq!(spawner.clunk(Fid(13)).await, Ok(()));

    let directory = String::from_utf8(read_at(&fs, &[&pid], Fid(14)).await.unwrap()).unwrap();
    assert!(directory.lines().any(|entry| entry == "descriptors"));
    assert_eq!(
        serde_json::from_slice::<std::collections::BTreeMap<u32, String>>(
            &read_at(&fs, &[&pid, "descriptors"], Fid(15)).await.unwrap()
        )
        .unwrap(),
        [(3, "/definition".to_string())].into_iter().collect()
    );
}

#[tokio::test]
async fn clone_rejects_reserved_or_unreachable_descriptors_without_leaking_a_process() {
    for (fid, descriptors) in [
        (Fid(16), serde_json::json!({"2": "/definition"})),
        (Fid(17), serde_json::json!({"3": "/outside"})),
    ] {
        let fs = proc();
        let mut namespace = Namespace::new();
        namespace.mount(
            "/definition",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            Access::ReadOnly,
        );
        let spawner = fs.for_spawner(None, namespace, Credentials::system());
        spawner
            .walk(Fid::ROOT, fid, &["clone".to_string()])
            .await
            .unwrap();
        spawner.open(fid, OpenMode::ReadWrite).await.unwrap();
        let exec = serde_json::json!({
            "executable": "/bin/alan-agent",
            "namespace": {"generation": 0,"mounts": [{"path": "/definition", "access": "ro"}]},
            "descriptors": descriptors
        });
        spawner
            .write(fid, 0, exec.to_string().as_bytes())
            .await
            .unwrap();
        assert_eq!(spawner.clunk(fid).await, Err(ErrorCode::BadRequest));
        assert_eq!(
            String::from_utf8(fs.read(Fid::ROOT, 0, 64).await.unwrap()).unwrap(),
            "clone"
        );
    }
}

#[tokio::test]
async fn clone_uses_spawner_identity_and_explicit_namespace_delegation() {
    let fs = proc();
    let parent = spawn(&fs, Fid(10)).await;

    let mut namespace = Namespace::new();
    namespace.mount(
        "/data",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    let spawner = fs.for_spawner(
        Some(Pid(parent.parse::<u64>().unwrap())),
        namespace,
        Credentials::user("alan"),
    );

    let child = spawn_with_mounts(
        &spawner,
        Fid(20),
        serde_json::json!([{"path": "/data", "access": "ro"}]),
    )
    .await;

    let recorded_parent =
        String::from_utf8(read_at(&fs, &[&child, "parent"], Fid(21)).await.unwrap()).unwrap();
    assert_eq!(recorded_parent, parent);

    let credentials = String::from_utf8(
        read_at(&fs, &[&child, "credentials"], Fid(22))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(credentials, "alan");

    let namespace =
        String::from_utf8(read_at(&fs, &[&child, "namespace"], Fid(23)).await.unwrap()).unwrap();
    assert!(
        namespace.lines().any(|line| line == "/data ro"),
        "child namespace contains the explicitly delegated mount: {namespace:?}"
    );
}

#[tokio::test]
async fn live_spawner_namespace_reads_and_children_snapshot_explicit_live_mounts() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/data",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    let live_namespace = LiveNamespace::new(namespace);
    let spawner = fs.for_live_spawner(None, live_namespace.clone(), Credentials::user("alan"));

    let pid = spawn_with_mounts(
        &spawner,
        Fid(60),
        serde_json::json!([{"path": "/data", "access": "ro"}]),
    )
    .await;
    let pid_value = Pid(pid.parse::<u64>().unwrap());
    fs.walk(Fid::ROOT, Fid(61), &[pid.clone(), "namespace".to_string()])
        .await
        .unwrap();
    fs.open(Fid(61), OpenMode::Read).await.unwrap();
    let committed_generation = fs.stat(Fid(61)).await.unwrap().qid.version;

    fs.bind_live_namespace(pid_value, live_namespace.clone())
        .await;

    let bound_generation = fs.stat(Fid(61)).await.unwrap().qid.version;
    assert_ne!(bound_generation, committed_generation);
    assert_eq!(bound_generation, live_namespace.generation());

    live_namespace.mount(
        "/mnt/project",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );

    let after = fs.stat(Fid(61)).await.unwrap().qid.version;
    assert_ne!(after, bound_generation);
    let namespace = String::from_utf8(fs.read(Fid(61), 0, 4096).await.unwrap()).unwrap();
    assert!(
        namespace.lines().any(|line| line == "/mnt/project rw"),
        "live process namespace should include approved grant: {namespace:?}"
    );

    let child = spawn_with_mounts_at_generation(
        &spawner,
        Fid(62),
        live_namespace.generation(),
        serde_json::json!([
            {"path": "/data", "access": "ro"},
            {"path": "/mnt/project", "access": "rw"}
        ]),
    )
    .await;
    let child_namespace =
        String::from_utf8(read_at(&fs, &[&child, "namespace"], Fid(63)).await.unwrap()).unwrap();
    assert!(
        child_namespace
            .lines()
            .any(|line| line == "/mnt/project rw"),
        "child namespace should snapshot explicitly delegated live grants: {child_namespace:?}"
    );
}

#[tokio::test]
async fn clone_exec_namespace_manifest_must_match_spawner_namespace() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/data",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    namespace.mount(
        "/scratch",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(30), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(30), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(30), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/scratch", "access": "rw"},
                {"path": "/data", "access": "ro"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(30), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(30)).await, Ok(()));

    let namespace = String::from_utf8(
        read_at(&fs, &[&pid_name, "namespace"], Fid(31))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        namespace.lines().any(|line| line == "/data ro"),
        "committed namespace includes /data: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/scratch rw"),
        "committed namespace includes /scratch: {namespace:?}"
    );
}

#[tokio::test]
async fn clone_exec_namespace_manifest_may_restrict_to_a_spawner_subset() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/data",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    namespace.mount(
        "/scratch",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(35), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(35), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(35), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/data", "access": "ro"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(35), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(35)).await, Ok(()));

    let namespace = String::from_utf8(
        read_at(&fs, &[&pid_name, "namespace"], Fid(36))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        namespace.lines().any(|line| line == "/data ro"),
        "committed namespace keeps the requested mount: {namespace:?}"
    );
    assert!(
        !namespace.lines().any(|line| line == "/scratch rw"),
        "committed namespace drops inherited mounts omitted from the manifest: {namespace:?}"
    );
}

#[tokio::test]
async fn clone_exec_namespace_manifest_may_downgrade_rw_mounts_to_read_only() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/scratch",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(37), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(37), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(37), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/scratch", "access": "ro"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(37), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(37)).await, Ok(()));

    let namespace = String::from_utf8(
        read_at(&fs, &[&pid_name, "namespace"], Fid(38))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        namespace.lines().any(|line| line == "/scratch ro"),
        "committed namespace downgrades RW authority to requested RO: {namespace:?}"
    );
    assert!(
        !namespace.lines().any(|line| line == "/scratch rw"),
        "committed namespace must not retain write authority after RO downgrade: {namespace:?}"
    );
}

#[tokio::test]
async fn clone_exec_namespace_manifest_preserves_restrictive_overmounts() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/data",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    namespace.mount(
        "/data/secrets",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(39), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(39), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(39), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/data", "access": "rw"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(39), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(39)).await, Ok(()));

    let namespace = String::from_utf8(
        read_at(&fs, &[&pid_name, "namespace"], Fid(40))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        namespace.lines().any(|line| line == "/data rw"),
        "committed namespace keeps the requested broad mount: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/data/secrets ro"),
        "committed namespace keeps the restrictive overmount masking the broad mount: {namespace:?}"
    );
}

#[tokio::test]
async fn clone_exec_namespace_manifest_rejects_omitted_nonrestrictive_descendants() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/mnt",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(41), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(41), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(41), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/mnt", "access": "rw"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(41), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(41)).await, Err(ErrorCode::BadRequest));

    let listing = String::from_utf8(read_at(&fs, &[], Fid(42)).await.unwrap()).unwrap();
    assert!(
        !listing.lines().any(|line| line == pid_name),
        "rejected manifest leaks nothing into public /proc"
    );
}

#[tokio::test]
async fn clone_exec_namespace_manifest_rejects_omitted_same_access_masks() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/data",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    namespace.mount(
        "/data/secrets",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(43), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(43), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(43), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/data", "access": "ro"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(43), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(43)).await, Err(ErrorCode::BadRequest));

    let listing = String::from_utf8(read_at(&fs, &[], Fid(44)).await.unwrap()).unwrap();
    assert!(
        !listing.lines().any(|line| line == pid_name),
        "same-access overmount masks must be requested explicitly or the manifest is rejected"
    );
}

#[tokio::test]
async fn restricted_manifest_rebinds_delegated_proc_clone_to_the_restricted_namespace() {
    let runner = Arc::new(CaptureRunner::new());
    let fs = ProcFs::new().with_runner(runner.clone());
    let parent = spawn(&fs, Fid(10)).await;
    let parent_pid = Pid(parent.parse::<u64>().unwrap());

    let mut namespace = Namespace::new();
    namespace.mount(
        "/proc/clone",
        InProcessTransport::new(Arc::new(fs.clone_file_for_spawner(
            Some(parent_pid),
            Namespace::new(),
            Credentials::user("alan"),
        ))),
        Access::ReadWrite,
    );
    namespace.mount(
        "/scratch",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    let spawner = fs.for_spawner(Some(parent_pid), namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(60), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(60), OpenMode::ReadWrite).await.unwrap();
    let child = String::from_utf8(spawner.read(Fid(60), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/child",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/proc/clone", "access": "rw"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(60), 0, exec.as_bytes()).await.unwrap();
    spawner.clunk(Fid(60)).await.unwrap();

    let child_pid = Pid(child.parse::<u64>().unwrap());
    let child_invocation = runner.wait_for(child_pid).await;
    assert!(
        child_invocation.namespace.resolve("/scratch").is_err(),
        "restricted child namespace must drop omitted mounts"
    );
    let proc_clone = child_invocation
        .namespace
        .resolve("/proc/clone")
        .expect("restricted child keeps delegated /proc/clone");

    proc_clone
        .call(Request::Open {
            fid: Fid::ROOT,
            mode: OpenMode::ReadWrite,
        })
        .await
        .unwrap();
    let grandchild = match proc_clone
        .call(Request::Read {
            fid: Fid::ROOT,
            offset: 0,
            count: 64,
        })
        .await
        .unwrap()
    {
        Response::Read { data } => String::from_utf8(data).unwrap(),
        other => panic!("unexpected response: {other:?}"),
    };
    proc_clone
        .call(Request::Write {
            fid: Fid::ROOT,
            offset: 0,
            data: br#"{"executable":"/bin/grandchild","args":[],"namespace":{"generation": 0,"mounts":[{"path":"/proc/clone","access":"rw"}]}}"#.to_vec(),
        })
        .await
        .unwrap();
    proc_clone
        .call(Request::Clunk { fid: Fid::ROOT })
        .await
        .unwrap();

    let grandchild_pid = Pid(grandchild.parse::<u64>().unwrap());
    let grandchild_invocation = runner.wait_for(grandchild_pid).await;
    assert!(
        grandchild_invocation.namespace.resolve("/scratch").is_err(),
        "grandchild spawned through restricted /proc/clone must not regain omitted mounts"
    );
}

#[tokio::test]
async fn clone_exec_namespace_manifest_mismatch_is_rejected_without_leaking_pid() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/data",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadOnly,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(40), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(40), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(40), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/data", "access": "rw"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(40), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(40)).await, Err(ErrorCode::BadRequest));

    let listing = String::from_utf8(read_at(&fs, &[], Fid(41)).await.unwrap()).unwrap();
    assert!(
        !listing.lines().any(|line| line == pid_name),
        "rejected manifest leaks nothing into public /proc"
    );
}

#[tokio::test]
async fn clone_expands_child_pid_placeholder_before_manifest_validation() {
    let fs = proc();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/<child-pid>",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm/connections/default",
        alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    let spawner = fs.for_spawner(None, namespace, Credentials::user("alan"));

    spawner
        .walk(Fid::ROOT, Fid(50), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(50), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(spawner.read(Fid(50), 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/agent",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": format!("/agent/{pid_name}"), "access": "rw"},
                {"path": "/mnt/llm/connections/default", "access": "rw"}
            ]
        }
    })
    .to_string();
    spawner.write(Fid(50), 0, exec.as_bytes()).await.unwrap();
    assert_eq!(spawner.clunk(Fid(50)).await, Ok(()));

    let namespace = String::from_utf8(
        read_at(&fs, &[&pid_name, "namespace"], Fid(51))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        namespace
            .lines()
            .any(|line| line == format!("/agent/{pid_name} rw")),
        "child pid placeholder is expanded in the committed namespace: {namespace:?}"
    );
    assert!(
        !namespace.lines().any(|line| line.contains("<child-pid>")),
        "placeholder must not leak into the public namespace file: {namespace:?}"
    );
}

// §5.1 — qid versions bump when content changes, so a cached qid/version goes
// stale: the /proc listing on process appearance, a process's files on exit.
#[tokio::test]
async fn proc_qid_versions_bump_on_change() {
    let fs = proc();
    let v0 = fs.stat(Fid::ROOT).await.unwrap().qid.version;
    let pid = spawn(&fs, Fid(10)).await;
    let v1 = fs.stat(Fid::ROOT).await.unwrap().qid.version;
    assert_eq!(v1, v0 + 1, "/proc listing changed when a process appeared");

    // A process's status qid version bumps when it exits.
    fs.walk(Fid::ROOT, Fid(11), &[pid.clone(), "status".to_string()])
        .await
        .unwrap();
    let s0 = fs.stat(Fid(11)).await.unwrap().qid.version;
    fs.walk(Fid::ROOT, Fid(12), &[pid.clone(), "ctl".to_string()])
        .await
        .unwrap();
    fs.open(Fid(12), OpenMode::Write).await.unwrap();
    fs.write(Fid(12), 0, b"cancel").await.unwrap();
    fs.clunk(Fid(12)).await.unwrap();
    fs.walk(Fid::ROOT, Fid(13), &[pid, "status".to_string()])
        .await
        .unwrap();
    let s1 = fs.stat(Fid(13)).await.unwrap().qid.version;
    assert_eq!(s1, s0 + 1, "status changed when the process exited");
}
