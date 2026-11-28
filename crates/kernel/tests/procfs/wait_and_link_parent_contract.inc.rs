// /proc/<pid>/wait & /proc/self/wait stream and ExecSpec.link_parent cascade
// cancellation contract tests (ADR-0063 / Tasks 1.1–1.3).

async fn spawn_exec_with_link(
    fs: &ProcFs,
    clone_fid: Fid,
    executable: &str,
    mounts: serde_json::Value,
    link_parent: bool,
) -> String {
    fs.walk(Fid::ROOT, clone_fid, &["clone".to_string()])
        .await
        .unwrap();
    fs.open(clone_fid, OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(fs.read(clone_fid, 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": executable,
        "args": [],
        "namespace": { "generation": 0, "mounts": mounts },
        "link_parent": link_parent,
    })
    .to_string();
    fs.write(clone_fid, 0, exec.as_bytes()).await.unwrap();
    fs.clunk(clone_fid).await.unwrap();
    pid
}

async fn wait_stream_read(fs: &ProcFs, path: &[&str], fid: Fid) -> Vec<u8> {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await.unwrap();
    fs.open(fid, OpenMode::Read).await.unwrap();
    let bytes = fs.read(fid, 0, 4096).await.unwrap();
    fs.clunk(fid).await.unwrap();
    bytes
}

fn assert_wait_record(bytes: &[u8], expected_pid: u64, expected_code: i32, expected_reason: &str) {
    let text = String::from_utf8(bytes.to_vec()).expect("wait record must be valid UTF-8");
    assert!(
        text.ends_with('\n'),
        "wait record must be newline-delimited: {text:?}"
    );
    assert_eq!(
        text.lines().count(),
        1,
        "wait stream must emit exactly one JSON line: {text:?}"
    );
    let expected = format!(
        r#"{{"pid":{expected_pid},"status":"exited","exit_code":{expected_code},"reason":"{expected_reason}"}}"#
    );
    assert_eq!(text.trim_end_matches('\n'), expected);
    let parsed: alan_kernel::ProcessWaitRecord = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed.pid, expected_pid);
    assert_eq!(parsed.status, "exited");
    assert_eq!(parsed.exit_code, expected_code);
}

struct BlockingWaitRunner {
    started: Arc<Mutex<Vec<Pid>>>,
    aborted: Arc<Mutex<Vec<Pid>>>,
    started_notify: Arc<Notify>,
    aborted_notify: Arc<Notify>,
    release: Arc<Notify>,
    exit_code: i32,
}

impl BlockingWaitRunner {
    fn new(exit_code: i32) -> Self {
        Self {
            started: Arc::new(Mutex::new(Vec::new())),
            aborted: Arc::new(Mutex::new(Vec::new())),
            started_notify: Arc::new(Notify::new()),
            aborted_notify: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
            exit_code,
        }
    }

    async fn wait_started(&self, pid: Pid) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if self.started.lock().unwrap().contains(&pid) {
                    return;
                }
                self.started_notify.notified().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("runner did not start for pid {}", pid.0));
    }

    async fn wait_aborted(&self, pid: Pid) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if self.aborted.lock().unwrap().contains(&pid) {
                    return;
                }
                self.aborted_notify.notified().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("runner task was not aborted for pid {}", pid.0));
    }
}

struct AbortRecorder {
    pid: Pid,
    aborted: Arc<Mutex<Vec<Pid>>>,
    notify: Arc<Notify>,
    completed: bool,
}

impl Drop for AbortRecorder {
    fn drop(&mut self) {
        if !self.completed {
            self.aborted.lock().unwrap().push(self.pid);
            self.notify.notify_waiters();
        }
    }
}

#[async_trait::async_trait]
impl ProcessRunner for BlockingWaitRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        let mut guard = AbortRecorder {
            pid: invocation.pid,
            aborted: Arc::clone(&self.aborted),
            notify: Arc::clone(&self.aborted_notify),
            completed: false,
        };
        self.started.lock().unwrap().push(invocation.pid);
        self.started_notify.notify_waiters();
        self.release.notified().await;
        guard.completed = true;
        ProcessOutcome::exited(self.exit_code, Vec::new())
    }
}

#[tokio::test]
async fn proc_wait_is_read_only_stream_on_pid_and_self_and_blocks_until_exit() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    let pid_num: u64 = pid.parse().unwrap();

    // Directory listing exposes `wait`.
    let listing = String::from_utf8(read_at(&fs, &[&pid], Fid(11)).await.unwrap()).unwrap();
    assert!(
        listing.lines().any(|entry| entry == "wait"),
        "expected `wait` in /proc/{pid} listing: {listing:?}"
    );

    // `/proc/<pid>/wait` stat reports read-only `FileKind::Stream` with length 0 while alive.
    fs.walk(Fid::ROOT, Fid(12), &[pid.clone(), "wait".into()])
        .await
        .unwrap();
    let stat = fs.stat(Fid(12)).await.unwrap();
    assert_eq!(stat.qid.kind, alan_ap::FileKind::Stream);
    assert!(!stat.writable);
    assert_eq!(stat.length, 0);
    fs.clunk(Fid(12)).await.unwrap();

    // `/proc/self/wait` exposes the same read-only stream and rejects all write-intent opens at dial time.
    let self_view = fs.for_spawner(Some(Pid(pid_num)), Namespace::new(), Credentials::user("alan"));
    let self_listing =
        String::from_utf8(read_at(&self_view, &["self"], Fid(13)).await.unwrap()).unwrap();
    assert!(self_listing.lines().any(|entry| entry == "wait"));

    for (index, mode) in [
        OpenMode::Write,
        OpenMode::WriteExclusive,
        OpenMode::ReadWrite,
    ]
    .into_iter()
    .enumerate()
    {
        let fid_pid = Fid(20 + index as u64);
        fs.walk(Fid::ROOT, fid_pid, &[pid.clone(), "wait".into()])
            .await
            .unwrap();
        assert_eq!(
            fs.open(fid_pid, mode).await,
            Err(ErrorCode::NoAccess),
            "/proc/<pid>/wait must reject {mode:?} at dial time"
        );
        fs.clunk(fid_pid).await.unwrap();

        let fid_self = Fid(30 + index as u64);
        self_view
            .walk(Fid::ROOT, fid_self, &["self".into(), "wait".into()])
            .await
            .unwrap();
        assert_eq!(
            self_view.open(fid_self, mode).await,
            Err(ErrorCode::NoAccess),
            "/proc/self/wait must reject {mode:?} at dial time"
        );
        self_view.clunk(fid_self).await.unwrap();
    }

    // Pre-exit blocking readers on both `/proc/<pid>/wait` and `/proc/self/wait`.
    let fs_waiter = fs.clone();
    let pid_waiter = pid.clone();
    let pid_reader =
        tokio::spawn(async move { wait_stream_read(&fs_waiter, &[&pid_waiter, "wait"], Fid(40)).await });

    let self_waiter = self_view.clone();
    let self_reader =
        tokio::spawn(async move { wait_stream_read(&self_waiter, &["self", "wait"], Fid(41)).await });

    // Neither reader resolves while the process is still running.
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    assert!(!pid_reader.is_finished());
    assert!(!self_reader.is_finished());

    // Normal exit unblocks both waiters immediately with the exact terminal JSON line.
    fs.record_exit(Pid(pid_num), 0).await;

    let pid_bytes = tokio::time::timeout(std::time::Duration::from_secs(1), pid_reader)
        .await
        .expect("/proc/<pid>/wait must unblock immediately on exit")
        .unwrap();
    let self_bytes = tokio::time::timeout(std::time::Duration::from_secs(1), self_reader)
        .await
        .expect("/proc/self/wait must unblock immediately on exit")
        .unwrap();

    assert_wait_record(&pid_bytes, pid_num, 0, "normal");
    assert_wait_record(&self_bytes, pid_num, 0, "normal");

    // `stat.length` reflects the retained terminal JSON record length after exit.
    fs.walk(Fid::ROOT, Fid(42), &[pid.clone(), "wait".into()])
        .await
        .unwrap();
    assert_eq!(fs.stat(Fid(42)).await.unwrap().length, pid_bytes.len() as u64);
    fs.clunk(Fid(42)).await.unwrap();
}

#[tokio::test]
async fn proc_wait_post_exit_read_is_immediate_and_covers_failed_and_cancelled_reasons() {
    let fs = proc();

    // Case 1: failed exit recorded before any consumer attaches to `wait`.
    let failed_pid = spawn(&fs, Fid(10)).await;
    let failed_num: u64 = failed_pid.parse().unwrap();
    fs.record_exit(Pid(failed_num), 42).await;

    // Repeated late reads from offset 0 return immediately without blocking.
    for fid in [Fid(11), Fid(12)] {
        let bytes = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            wait_stream_read(&fs, &[&failed_pid, "wait"], fid),
        )
        .await
        .expect("post-exit /proc/<pid>/wait read must succeed immediately");
        assert_wait_record(&bytes, failed_num, 42, "failed");
    }

    // Offset-chunked read also works cleanly from the retained stream.
    fs.walk(Fid::ROOT, Fid(13), &[failed_pid.clone(), "wait".into()])
        .await
        .unwrap();
    fs.open(Fid(13), OpenMode::Read).await.unwrap();
    let first_chunk = fs.read(Fid(13), 0, 10).await.unwrap();
    let rest = fs.read(Fid(13), first_chunk.len() as u64, 4096).await.unwrap();
    fs.clunk(Fid(13)).await.unwrap();
    let mut combined = first_chunk;
    combined.extend_from_slice(&rest);
    assert_wait_record(&combined, failed_num, 42, "failed");

    // Case 2: cancelled via `/proc/<pid>/ctl` ("cancel" and "interrupt").
    for (idx, cmd) in [b"cancel".as_slice(), b"interrupt".as_slice()]
        .into_iter()
        .enumerate()
    {
        let base = 20 + (idx as u64) * 10;
        let cancelled_pid = spawn(&fs, Fid(base)).await;
        let cancelled_num: u64 = cancelled_pid.parse().unwrap();

        fs.walk(Fid::ROOT, Fid(base + 1), &[cancelled_pid.clone(), "ctl".into()])
            .await
            .unwrap();
        fs.open(Fid(base + 1), OpenMode::Write).await.unwrap();
        fs.write(Fid(base + 1), 0, cmd).await.unwrap();
        fs.clunk(Fid(base + 1)).await.unwrap();

        // Duplicate cancel must not append a second JSON line or clobber the wait stream.
        fs.walk(Fid::ROOT, Fid(base + 2), &[cancelled_pid.clone(), "ctl".into()])
            .await
            .unwrap();
        fs.open(Fid(base + 2), OpenMode::Write).await.unwrap();
        fs.write(Fid(base + 2), 0, b"cancel").await.unwrap();
        fs.clunk(Fid(base + 2)).await.unwrap();

        let bytes = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            wait_stream_read(&fs, &[&cancelled_pid, "wait"], Fid(base + 3)),
        )
        .await
        .expect("post-cancel /proc/<pid>/wait read must succeed immediately");
        assert_wait_record(&bytes, cancelled_num, 130, "cancelled");
    }
}

#[tokio::test]
async fn link_parent_cascades_multi_level_cancellation_across_all_parent_exit_reasons() {
    enum ParentExitMode {
        Normal,
        Failed,
        Cancelled,
    }

    for mode in [
        ParentExitMode::Normal,
        ParentExitMode::Failed,
        ParentExitMode::Cancelled,
    ] {
        let runner = Arc::new(BlockingWaitRunner::new(0));
        let fs = ProcFs::new().with_runner(runner.clone());

        // Spawn root parent A.
        let root_str = spawn_exec_with_link(&fs, Fid(10), "/bin/root", serde_json::json!([]), false).await;
        let root_pid = Pid(root_str.parse().unwrap());
        runner.wait_started(root_pid).await;

        // Spawn B (linked to A) and U (unlinked control child of A) via ProcFs::for_spawner.
        let a_spawner = fs.for_spawner(Some(root_pid), Namespace::new(), Credentials::user("alan"));
        let b_str =
            spawn_exec_with_link(&a_spawner, Fid(11), "/bin/b", serde_json::json!([]), true).await;
        let b_pid = Pid(b_str.parse().unwrap());
        let u_str =
            spawn_exec_with_link(&a_spawner, Fid(12), "/bin/u", serde_json::json!([]), false).await;
        let u_pid = Pid(u_str.parse().unwrap());
        runner.wait_started(b_pid).await;
        runner.wait_started(u_pid).await;

        // Spawn C (linked to B) and D (linked to C) -> 3 levels deep below A.
        let b_spawner = fs.for_spawner(Some(b_pid), Namespace::new(), Credentials::user("alan"));
        let c_str =
            spawn_exec_with_link(&b_spawner, Fid(13), "/bin/c", serde_json::json!([]), true).await;
        let c_pid = Pid(c_str.parse().unwrap());
        runner.wait_started(c_pid).await;

        let c_spawner = fs.for_spawner(Some(c_pid), Namespace::new(), Credentials::user("alan"));
        let d_str =
            spawn_exec_with_link(&c_spawner, Fid(14), "/bin/d", serde_json::json!([]), true).await;
        let d_pid = Pid(d_str.parse().unwrap());
        runner.wait_started(d_pid).await;

        // Spawn U_child (linked to U) to prove an unlinked parent insulates its subtree.
        let u_spawner = fs.for_spawner(Some(u_pid), Namespace::new(), Credentials::user("alan"));
        let u_child_str = spawn_exec_with_link(
            &u_spawner,
            Fid(15),
            "/bin/u-child",
            serde_json::json!([]),
            true,
        )
        .await;
        let u_child_pid = Pid(u_child_str.parse().unwrap());
        runner.wait_started(u_child_pid).await;

        // Attach pre-exit blocking waiters on B, C, D, and unlinked control U.
        let fs_b = fs.clone();
        let b_wait_str = b_str.clone();
        let b_waiter = tokio::spawn(async move {
            wait_stream_read(&fs_b, &[&b_wait_str, "wait"], Fid(100)).await
        });
        let fs_c = fs.clone();
        let c_wait_str = c_str.clone();
        let c_waiter = tokio::spawn(async move {
            wait_stream_read(&fs_c, &[&c_wait_str, "wait"], Fid(101)).await
        });
        let fs_d = fs.clone();
        let d_wait_str = d_str.clone();
        let d_waiter = tokio::spawn(async move {
            wait_stream_read(&fs_d, &[&d_wait_str, "wait"], Fid(102)).await
        });
        let fs_u = fs.clone();
        let u_wait_str = u_str.clone();
        let u_waiter = tokio::spawn(async move {
            wait_stream_read(&fs_u, &[&u_wait_str, "wait"], Fid(103)).await
        });

        // Exit parent A with the mode under test.
        let (expected_root_code, expected_root_reason) = match mode {
            ParentExitMode::Normal => {
                fs.record_exit(root_pid, 0).await;
                (0, "normal")
            }
            ParentExitMode::Failed => {
                fs.record_exit(root_pid, 9).await;
                (9, "failed")
            }
            ParentExitMode::Cancelled => {
                fs.walk(Fid::ROOT, Fid(200), &[root_str.clone(), "ctl".into()])
                    .await
                    .unwrap();
                fs.open(Fid(200), OpenMode::Write).await.unwrap();
                fs.write(Fid(200), 0, b"cancel").await.unwrap();
                fs.clunk(Fid(200)).await.unwrap();
                (130, "cancelled")
            }
        };

        // Parent A's wait stream reflects its own exit code and reason.
        let root_wait = wait_stream_read(&fs, &[&root_str, "wait"], Fid(201)).await;
        assert_wait_record(&root_wait, root_pid.0, expected_root_code, expected_root_reason);

        // Linked descendants B, C, D all unblock immediately with cancelled records and aborted tasks.
        let b_wait = tokio::time::timeout(std::time::Duration::from_secs(1), b_waiter)
            .await
            .expect("linked child B must unblock on parent exit")
            .unwrap();
        let c_wait = tokio::time::timeout(std::time::Duration::from_secs(1), c_waiter)
            .await
            .expect("linked grandchild C must unblock on parent exit")
            .unwrap();
        let d_wait = tokio::time::timeout(std::time::Duration::from_secs(1), d_waiter)
            .await
            .expect("linked great-grandchild D must unblock on parent exit")
            .unwrap();

        assert_wait_record(&b_wait, b_pid.0, 130, "cancelled");
        assert_wait_record(&c_wait, c_pid.0, 130, "cancelled");
        assert_wait_record(&d_wait, d_pid.0, 130, "cancelled");

        runner.wait_aborted(b_pid).await;
        runner.wait_aborted(c_pid).await;
        runner.wait_aborted(d_pid).await;

        // Unlinked control child U and its linked child U_child remain running.
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        assert!(
            !u_waiter.is_finished(),
            "link_parent: false child U must remain running when parent exits"
        );
        assert_eq!(
            String::from_utf8(read_at(&fs, &[&u_str, "status"], Fid(202)).await.unwrap()).unwrap(),
            "running\n"
        );
        assert_eq!(
            String::from_utf8(
                read_at(&fs, &[&u_child_str, "status"], Fid(203))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "running\n"
        );

        // Exiting U now cascades cleanly to U_child.
        fs.record_exit(u_pid, 0).await;
        let u_wait = tokio::time::timeout(std::time::Duration::from_secs(1), u_waiter)
            .await
            .unwrap()
            .unwrap();
        assert_wait_record(&u_wait, u_pid.0, 0, "normal");
        let u_child_wait = wait_stream_read(&fs, &[&u_child_str, "wait"], Fid(204)).await;
        assert_wait_record(&u_child_wait, u_child_pid.0, 130, "cancelled");
        runner.wait_aborted(u_child_pid).await;
    }
}

#[tokio::test]
async fn link_parent_cascades_through_rebound_proc_clone_commits_in_child_namespaces() {
    struct SpawnGrandchildRunner {
        grandchild_pid: Arc<Mutex<Option<String>>>,
        grandchild_ready: Arc<Notify>,
        release_parent: Arc<Notify>,
        parent_pid: Pid,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for SpawnGrandchildRunner {
        async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
            if invocation.exec.executable == "/bin/child" {
                let proc_clone = invocation
                    .namespace
                    .resolve("/proc/clone")
                    .expect("child namespace exposes rebound /proc/clone");
                proc_clone
                    .call(Request::Walk {
                        fid: Fid::ROOT,
                        newfid: Fid(900),
                        names: proc_clone.rel.clone(),
                    })
                    .await
                    .unwrap();
                proc_clone
                    .call(Request::Open {
                        fid: Fid(900),
                        mode: OpenMode::ReadWrite,
                    })
                    .await
                    .unwrap();
                let grandchild = match proc_clone
                    .call(Request::Read {
                        fid: Fid(900),
                        offset: 0,
                        count: 64,
                    })
                    .await
                    .unwrap()
                {
                    Response::Read { data } => String::from_utf8(data).unwrap(),
                    other => panic!("unexpected response: {other:?}"),
                };
                let spec = serde_json::json!({
                    "executable": "/bin/grandchild",
                    "args": [],
                    "namespace": { "generation": 0, "mounts": [] },
                    "link_parent": true,
                })
                .to_string();
                proc_clone
                    .call(Request::Write {
                        fid: Fid(900),
                        offset: 0,
                        data: spec.into_bytes(),
                    })
                    .await
                    .unwrap();
                proc_clone
                    .call(Request::Clunk { fid: Fid(900) })
                    .await
                    .unwrap();
                *self.grandchild_pid.lock().unwrap() = Some(grandchild);
                self.grandchild_ready.notify_one();
                std::future::pending::<ProcessOutcome>().await
            } else if invocation.pid == self.parent_pid {
                self.release_parent.notified().await;
                ProcessOutcome::exited(0, Vec::new())
            } else {
                std::future::pending::<ProcessOutcome>().await
            }
        }
    }

    let grandchild_pid = Arc::new(Mutex::new(None));
    let grandchild_ready = Arc::new(Notify::new());
    let release_parent = Arc::new(Notify::new());

    let runner = Arc::new(SpawnGrandchildRunner {
        grandchild_pid: Arc::clone(&grandchild_pid),
        grandchild_ready: Arc::clone(&grandchild_ready),
        release_parent: Arc::clone(&release_parent),
        parent_pid: Pid(1),
    });
    let fs = ProcFs::new().with_runner(runner);

    let parent_str =
        spawn_exec_with_link(&fs, Fid(10), "/bin/parent", serde_json::json!([]), false).await;
    let parent_pid = Pid(parent_str.parse().unwrap());
    assert_eq!(parent_pid, Pid(1));

    let mut parent_ns = Namespace::new();
    parent_ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(fs.for_spawner(
            Some(parent_pid),
            Namespace::new(),
            Credentials::user("alan"),
        ))),
        Access::ReadWrite,
    );
    let spawner = fs.for_spawner(Some(parent_pid), parent_ns, Credentials::user("alan"));
    let child_str = spawn_exec_with_link(
        &spawner,
        Fid(20),
        "/bin/child",
        serde_json::json!([{"path": "/proc", "access": "rw"}]),
        true,
    )
    .await;
    let child_pid: u64 = child_str.parse().unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(2), grandchild_ready.notified())
        .await
        .expect("child should spawn linked grandchild through rebound /proc/clone");
    let grandchild_str = grandchild_pid.lock().unwrap().clone().unwrap();
    let grandchild_num: u64 = grandchild_str.parse().unwrap();

    // Parent exits naturally via its ProcessRunner returning exit_code 0.
    release_parent.notify_one();

    let parent_wait = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        wait_stream_read(&fs, &[&parent_str, "wait"], Fid(30)),
    )
    .await
    .unwrap();
    let child_wait = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        wait_stream_read(&fs, &[&child_str, "wait"], Fid(31)),
    )
    .await
    .unwrap();
    let grandchild_wait = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        wait_stream_read(&fs, &[&grandchild_str, "wait"], Fid(32)),
    )
    .await
    .unwrap();

    assert_wait_record(&parent_wait, parent_pid.0, 0, "normal");
    assert_wait_record(&child_wait, child_pid, 130, "cancelled");
    assert_wait_record(&grandchild_wait, grandchild_num, 130, "cancelled");
}

#[tokio::test]
async fn linked_child_committed_after_parent_exit_publishes_cancelled_wait_immediately() {
    let runner = Arc::new(CaptureRunner::new());
    let fs = ProcFs::new().with_runner(runner.clone());

    let parent_str = spawn(&fs, Fid(10)).await;
    let parent_pid = Pid(parent_str.parse().unwrap());
    let spawner = fs.for_spawner(Some(parent_pid), Namespace::new(), Credentials::user("alan"));

    // Begin clone while parent is alive.
    spawner
        .walk(Fid::ROOT, Fid(11), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(11), OpenMode::ReadWrite).await.unwrap();
    let child_str = String::from_utf8(spawner.read(Fid(11), 0, 64).await.unwrap()).unwrap();
    let child_num: u64 = child_str.parse().unwrap();
    let exec = serde_json::json!({
        "executable": "/bin/late-child",
        "args": [],
        "namespace": { "generation": 0, "mounts": [] },
        "link_parent": true,
    })
    .to_string();
    spawner.write(Fid(11), 0, exec.as_bytes()).await.unwrap();

    // Parent exits before child commits.
    fs.record_exit(parent_pid, 0).await;

    // Commit linked child -> immediately cancelled, wait stream ready, runner never invoked.
    spawner.clunk(Fid(11)).await.unwrap();

    let child_wait = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        wait_stream_read(&fs, &[&child_str, "wait"], Fid(12)),
    )
    .await
    .expect("late-committed linked child must immediately expose cancelled wait record");
    assert_wait_record(&child_wait, child_num, 130, "cancelled");
    assert!(
        !runner
            .invocations
            .lock()
            .unwrap()
            .iter()
            .any(|inv| inv.pid == Pid(child_num)),
        "runner must not execute a linked child whose parent already exited"
    );
}
