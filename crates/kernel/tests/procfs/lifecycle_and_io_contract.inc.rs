use alan_ap::{
    ErrorCode, Fid, FileServer, InProcessTransport, OpenMode, ProcessEvent, ProcessEventSink,
    ProcessEventSource, ProcessInputEventSink, ProcessInputEventSource, ProcessOutputEventSink,
    ProcessOutputEventSource, Request, Response,
};
use alan_kernel::{
    Access, Credentials, LiveNamespace, Namespace, Pid, ProcFs, ProcessInvocation, ProcessOutcome,
    ProcessRunner,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

fn proc() -> ProcFs {
    ProcFs::new()
}

/// Spawn a process via clone-via-open using a distinct fid base; returns its pid.
async fn spawn(fs: &ProcFs, clone_fid: Fid) -> String {
    spawn_exec(
        fs,
        clone_fid,
        "/bin/agent",
        Vec::<String>::new(),
        serde_json::json!([]),
    )
    .await
}

async fn spawn_with_mounts(
    fs: &ProcFs,
    clone_fid: Fid,
    mounts: serde_json::Value,
) -> String {
    spawn_exec(
        fs,
        clone_fid,
        "/bin/agent",
        Vec::<String>::new(),
        mounts,
    )
    .await
}

async fn spawn_exec(
    fs: &ProcFs,
    clone_fid: Fid,
    executable: &str,
    args: Vec<String>,
    mounts: serde_json::Value,
) -> String {
    fs.walk(Fid::ROOT, clone_fid, &["clone".to_string()])
        .await
        .unwrap();
    fs.open(clone_fid, OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(fs.read(clone_fid, 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": executable,
        "args": args,
        "namespace": {"mounts": mounts},
    })
    .to_string();
    fs.write(clone_fid, 0, exec.as_bytes()).await.unwrap();
    fs.clunk(clone_fid).await.unwrap();
    pid
}

async fn read_at(fs: &ProcFs, names: &[&str], fid: Fid) -> Result<Vec<u8>, ErrorCode> {
    let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await?;
    fs.open(fid, OpenMode::Read).await?;
    fs.read(fid, 0, 4096).await
}

struct EchoRunner;

#[derive(Default)]
struct RecordingProcessEventSink {
    events: Mutex<Vec<(String, ProcessEvent)>>,
}

#[async_trait::async_trait]
impl ProcessEventSink for RecordingProcessEventSink {
    async fn process_event(&self, pid: &str, event: ProcessEvent) {
        self.events.lock().unwrap().push((pid.to_string(), event));
    }
}

#[async_trait::async_trait]
impl ProcessRunner for EchoRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        let Ok(resolved) = invocation.namespace.resolve(&invocation.exec.executable) else {
            return ProcessOutcome::exited(127, b"executable is not mounted\n".to_vec());
        };
        let fid = Fid(50_000 + invocation.pid.0);
        let reachable = resolved
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: resolved.rel.clone(),
            })
            .await
            .is_ok();
        let _ = resolved.call(Request::Clunk { fid }).await;
        if !reachable {
            return ProcessOutcome::exited(127, b"executable is not reachable\n".to_vec());
        }
        let mut output = invocation.exec.args.join(" ").into_bytes();
        output.push(b'\n');
        ProcessOutcome::exited(0, output)
    }
}

struct DelayedOutputRunner {
    output: Vec<u8>,
    release: Notify,
}

impl DelayedOutputRunner {
    fn new(output: impl Into<Vec<u8>>) -> Self {
        Self {
            output: output.into(),
            release: Notify::new(),
        }
    }

    fn release(&self) {
        self.release.notify_one();
    }
}

#[async_trait::async_trait]
impl ProcessRunner for DelayedOutputRunner {
    async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
        self.release.notified().await;
        ProcessOutcome::exited(0, self.output.clone())
    }
}

struct RecordingOutputSink {
    records: Mutex<Vec<(String, u32)>>,
    notify: Notify,
}

impl RecordingOutputSink {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            notify: Notify::new(),
        }
    }

    async fn wait_for(&self, expected: usize) -> Vec<(String, u32)> {
        for _ in 0..50 {
            let records = self
                .records
                .lock()
                .expect("output records lock should not be poisoned")
                .clone();
            if records.len() >= expected {
                return records;
            }
            let _ =
                tokio::time::timeout(std::time::Duration::from_millis(10), self.notify.notified())
                    .await;
        }
        self.records
            .lock()
            .expect("output records lock should not be poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl ProcessOutputEventSink for RecordingOutputSink {
    async fn output_appended(&self, pid: &str, count: u32) {
        self.records
            .lock()
            .expect("output records lock should not be poisoned")
            .push((pid.to_string(), count));
        self.notify.notify_waiters();
    }
}

struct RecordingInputSink {
    records: Mutex<Vec<(String, u32)>>,
    notify: Notify,
}

impl RecordingInputSink {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            notify: Notify::new(),
        }
    }

    async fn wait_for(&self, expected: usize) -> Vec<(String, u32)> {
        for _ in 0..50 {
            let records = self
                .records
                .lock()
                .expect("input records lock should not be poisoned")
                .clone();
            if records.len() >= expected {
                return records;
            }
            let _ =
                tokio::time::timeout(std::time::Duration::from_millis(10), self.notify.notified())
                    .await;
        }
        self.records
            .lock()
            .expect("input records lock should not be poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl ProcessInputEventSink for RecordingInputSink {
    async fn input_appended(&self, pid: &str, count: u32) {
        self.records
            .lock()
            .expect("input records lock should not be poisoned")
            .push((pid.to_string(), count));
        self.notify.notify_waiters();
    }
}

struct BlockingProcessEventSink {
    records: Mutex<Vec<ProcessEvent>>,
    first_started: AtomicBool,
    first_entered: Notify,
    release_first: Notify,
    notify: Notify,
}

impl BlockingProcessEventSink {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            first_started: AtomicBool::new(false),
            first_entered: Notify::new(),
            release_first: Notify::new(),
            notify: Notify::new(),
        }
    }

    async fn wait_for(&self, expected: usize) -> Vec<ProcessEvent> {
        for _ in 0..50 {
            let records = self
                .records
                .lock()
                .expect("process event records lock should not be poisoned")
                .clone();
            if records.len() >= expected {
                return records;
            }
            let _ =
                tokio::time::timeout(std::time::Duration::from_millis(10), self.notify.notified())
                    .await;
        }
        self.records
            .lock()
            .expect("process event records lock should not be poisoned")
            .clone()
    }

    async fn wait_until_first_replay_enters(&self) {
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            self.first_entered.notified(),
        )
        .await
        .expect("first replay event should enter the sink");
    }

    fn release_first_replay(&self) {
        self.release_first.notify_one();
    }
}

#[async_trait::async_trait]
impl ProcessEventSink for BlockingProcessEventSink {
    async fn process_event(&self, _pid: &str, event: ProcessEvent) {
        if !self.first_started.swap(true, Ordering::SeqCst) {
            self.first_entered.notify_waiters();
            self.release_first.notified().await;
        }
        self.records
            .lock()
            .expect("process event records lock should not be poisoned")
            .push(event);
        self.notify.notify_waiters();
    }
}

struct CaptureRunner {
    invocations: Mutex<Vec<ProcessInvocation>>,
}

impl CaptureRunner {
    fn new() -> Self {
        Self {
            invocations: Mutex::new(Vec::new()),
        }
    }

    async fn wait_for(&self, pid: Pid) -> ProcessInvocation {
        for _ in 0..50 {
            if let Some(invocation) = self
                .invocations
                .lock()
                .unwrap()
                .iter()
                .find(|invocation| invocation.pid == pid)
                .cloned()
            {
                return invocation;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("runner did not receive invocation for pid {}", pid.0);
    }
}

#[async_trait::async_trait]
impl ProcessRunner for CaptureRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        self.invocations.lock().unwrap().push(invocation);
        ProcessOutcome::exited(0, Vec::new())
    }
}

#[tokio::test]
async fn empty_proc_lists_only_clone() {
    let fs = proc();
    let listing = read_at(&fs, &[], Fid(1)).await.unwrap();
    let text = String::from_utf8(listing).unwrap();
    assert_eq!(text.lines().collect::<Vec<_>>(), vec!["clone"]);
}

#[tokio::test]
async fn spawn_via_clone_open_write_clunk_makes_a_public_process() {
    let fs = proc();

    // open /proc/clone → the pending pid is returned by reading the clone fid.
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    fs.open(Fid(10), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(fs.read(Fid(10), 0, 64).await.unwrap()).unwrap();
    assert!(!pid_name.is_empty());

    // The pending slot is not yet visible in public /proc.
    let before = String::from_utf8(read_at(&fs, &[], Fid(11)).await.unwrap()).unwrap();
    assert!(
        !before.lines().any(|l| l == pid_name),
        "pending slot is fid-private"
    );

    // Write the exec spec (commit-on-clunk) and clunk to start.
    fs.write(
        Fid(10),
        0,
        br#"{"executable":"/bin/agent","args":[],"namespace":{"mounts":[]}}"#,
    )
        .await
        .unwrap();
    assert_eq!(fs.clunk(Fid(10)).await, Ok(()));

    // Now /proc/<pid> is public and its status reads "running".
    let after = String::from_utf8(read_at(&fs, &[], Fid(12)).await.unwrap()).unwrap();
    assert!(
        after.lines().any(|l| l == pid_name),
        "committed process is public: {after:?}"
    );

    let status =
        String::from_utf8(read_at(&fs, &[&pid_name, "status"], Fid(13)).await.unwrap()).unwrap();
    assert_eq!(status.trim(), "running");
}

#[tokio::test]
async fn process_input_commits_one_framed_unit_on_clunk() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    let input_fid = Fid(11);
    fs.walk(
        Fid::ROOT,
        input_fid,
        &[pid.clone(), "io".to_string(), "input".to_string()],
    )
    .await
    .unwrap();
    fs.open(input_fid, OpenMode::Write).await.unwrap();
    fs.write(input_fid, 0, b"hello ").await.unwrap();
    fs.write(input_fid, 6, b"world").await.unwrap();
    assert_eq!(fs.stat(input_fid).await.unwrap().length, 0);
    fs.clunk(input_fid).await.unwrap();

    assert_eq!(
        read_at(&fs, &[&pid, "io", "input"], Fid(12)).await.unwrap(),
        b"11\nhello world"
    );
}

#[tokio::test]
async fn failed_oversize_process_input_is_not_committed_on_clunk() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    let input_fid = Fid(11);
    fs.walk(
        Fid::ROOT,
        input_fid,
        &[pid.clone(), "io".to_string(), "input".to_string()],
    )
    .await
    .unwrap();
    fs.open(input_fid, OpenMode::Write).await.unwrap();
    fs.write(input_fid, 0, &vec![b'a'; 1 << 20]).await.unwrap();
    assert_eq!(
        fs.write(input_fid, 1 << 20, b"b").await,
        Err(ErrorCode::BadRequest)
    );
    assert_eq!(fs.clunk(input_fid).await, Err(ErrorCode::BadRequest));

    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(20),
            read_at(&fs, &[&pid, "io", "input"], Fid(12)),
        )
        .await
        .is_err(),
        "failed input write committed its accepted prefix"
    );
}

#[tokio::test]
async fn child_namespace_rebinds_proc_clone_to_the_child_spawn_context() {
    let runner = Arc::new(CaptureRunner::new());
    let fs = ProcFs::new().with_runner(runner.clone());
    let parent = spawn(&fs, Fid(10)).await;
    let parent_pid = Pid(parent.parse::<u64>().unwrap());

    let mut namespace = Namespace::new();
    namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(fs.for_spawner(
            Some(parent_pid),
            Namespace::new(),
            Credentials::user("alan"),
        ))),
        Access::ReadWrite,
    );
    let spawner = fs.for_spawner(Some(parent_pid), namespace, Credentials::user("alan"));
    let child = spawn_with_mounts(
        &spawner,
        Fid(20),
        serde_json::json!([{"path": "/proc", "access": "rw"}]),
    )
    .await;
    let child_pid = Pid(child.parse::<u64>().unwrap());
    let child_invocation = runner.wait_for(child_pid).await;
    let proc_clone = child_invocation
        .namespace
        .resolve("/proc/clone")
        .expect("child namespace exposes /proc/clone");

    proc_clone
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(30),
            names: proc_clone.rel.clone(),
        })
        .await
        .unwrap();
    proc_clone
        .call(Request::Open {
            fid: Fid(30),
            mode: OpenMode::ReadWrite,
        })
        .await
        .unwrap();
    let grandchild = match proc_clone
        .call(Request::Read {
            fid: Fid(30),
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
            fid: Fid(30),
            offset: 0,
            data: br#"{"executable":"/bin/grandchild","args":[],"namespace":{"mounts":[{"path":"/proc","access":"rw"}]}}"#.to_vec(),
        })
        .await
        .unwrap();
    proc_clone
        .call(Request::Clunk { fid: Fid(30) })
        .await
        .unwrap();

    let recorded_parent = String::from_utf8(
        read_at(&fs, &[&grandchild, "parent"], Fid(31))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        recorded_parent, child,
        "grandchild spawned through child /proc must record the child as parent"
    );
}

#[tokio::test]
async fn late_io_event_subscribers_replay_existing_events() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    let input_fid = Fid(11);
    fs.walk(
        Fid::ROOT,
        input_fid,
        &[pid.clone(), "io".to_string(), "input".to_string()],
    )
    .await
    .unwrap();
    fs.open(input_fid, OpenMode::Write).await.unwrap();
    fs.write(input_fid, 0, b"early input").await.unwrap();
    fs.clunk(input_fid).await.unwrap();

    let output_fid = Fid(12);
    fs.walk(
        Fid::ROOT,
        output_fid,
        &[pid.clone(), "io".to_string(), "output".to_string()],
    )
    .await
    .unwrap();
    fs.open(output_fid, OpenMode::Write).await.unwrap();
    fs.write(output_fid, 0, b"early output").await.unwrap();
    fs.clunk(output_fid).await.unwrap();

    let input_sink = Arc::new(RecordingInputSink::new());
    fs.subscribe_process_input(&pid, input_sink.clone())
        .await
        .unwrap();
    assert_eq!(input_sink.wait_for(1).await, vec![(pid.clone(), 11)]);

    let output_sink = Arc::new(RecordingOutputSink::new());
    fs.subscribe_process_output(&pid, output_sink.clone())
        .await
        .unwrap();
    assert_eq!(output_sink.wait_for(1).await, vec![(pid, 12)]);
}

#[tokio::test]
async fn process_event_replay_precedes_live_events_for_late_subscribers() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    let output_fid = Fid(11);
    fs.walk(
        Fid::ROOT,
        output_fid,
        &[pid.clone(), "io".to_string(), "output".to_string()],
    )
    .await
    .unwrap();
    fs.open(output_fid, OpenMode::Write).await.unwrap();
    fs.write(output_fid, 0, b"early output").await.unwrap();
    fs.clunk(output_fid).await.unwrap();

    let sink = Arc::new(BlockingProcessEventSink::new());
    let subscribe_fs = fs.clone();
    let subscribe_pid = pid.clone();
    let subscribe_sink = sink.clone();
    let subscription = tokio::spawn(async move {
        subscribe_fs
            .subscribe_process_events(&subscribe_pid, subscribe_sink)
            .await
            .unwrap();
    });
    sink.wait_until_first_replay_enters().await;

    let live_fs = fs.clone();
    let live_pid = pid.clone();
    let live_write = tokio::spawn(async move {
        let input_fid = Fid(12);
        live_fs
            .walk(
                Fid::ROOT,
                input_fid,
                &[live_pid, "io".to_string(), "input".to_string()],
            )
            .await
            .unwrap();
        live_fs.open(input_fid, OpenMode::Write).await.unwrap();
        live_fs.write(input_fid, 0, b"live input").await.unwrap();
        live_fs.clunk(input_fid).await.unwrap();
    });
    tokio::task::yield_now().await;
    assert!(
        !live_write.is_finished(),
        "live event delivery should wait while retained event replay is active"
    );

    sink.release_first_replay();
    subscription.await.unwrap();
    live_write.await.unwrap();

    assert_eq!(
        sink.wait_for(3).await,
        vec![
            ProcessEvent::Status {
                status: "running".to_string()
            },
            ProcessEvent::Output { count: 12 },
            ProcessEvent::Input { count: 10 },
        ]
    );
}

#[tokio::test]
async fn host_recorded_exit_publishes_terminal_status_event() {
    let fs = proc();
    let pid = spawn(&fs, Fid(20)).await;
    fs.record_exit(Pid(pid.parse().unwrap()), 0).await;

    let sink = Arc::new(RecordingProcessEventSink::default());
    fs.subscribe_process_events(&pid, sink.clone())
        .await
        .unwrap();

    assert_eq!(
        sink.events.lock().unwrap().as_slice(),
        &[
            (
                pid.clone(),
                ProcessEvent::Status {
                    status: "running".to_string(),
                },
            ),
            (
                pid,
                ProcessEvent::Status {
                    status: "exited".to_string(),
                },
            ),
        ]
    );
}

#[tokio::test]
async fn host_recorded_exit_aborts_active_runner_task() {
    let runner = Arc::new(DelayedOutputRunner::new("late runner output\n"));
    let fs = ProcFs::new().with_runner(runner.clone());
    let pid = spawn(&fs, Fid(30)).await;

    fs.record_exit(Pid(pid.parse().unwrap()), 124).await;
    runner.release();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let output_fid = Fid(31);
    fs.walk(
        Fid::ROOT,
        output_fid,
        &[pid.clone(), "io".to_string(), "output".to_string()],
    )
    .await
    .unwrap();
    let output_stat = fs.stat(output_fid).await.unwrap();
    let exit = String::from_utf8(read_at(&fs, &[&pid, "exit"], Fid(32)).await.unwrap()).unwrap();
    assert_eq!(output_stat.length, 0);
    assert_eq!(exit, "124");
}

#[tokio::test]
async fn delegated_proc_clone_mount_rebinds_to_the_child_spawn_context() {
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
    let spawner = fs.for_spawner(Some(parent_pid), namespace, Credentials::user("alan"));
    let child = spawn_with_mounts(
        &spawner,
        Fid(20),
        serde_json::json!([{"path": "/proc/clone", "access": "rw"}]),
    )
    .await;
    let child_pid = Pid(child.parse::<u64>().unwrap());
    let child_invocation = runner.wait_for(child_pid).await;
    assert!(
        child_invocation.namespace.resolve("/proc").is_err(),
        "delegating /proc/clone must not grant the whole /proc tree"
    );
    let proc_clone = child_invocation
        .namespace
        .resolve("/proc/clone")
        .expect("child namespace keeps delegated /proc/clone");

    proc_clone
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(40),
            names: proc_clone.rel.clone(),
        })
        .await
        .unwrap();
    proc_clone
        .call(Request::Open {
            fid: Fid(40),
            mode: OpenMode::ReadWrite,
        })
        .await
        .unwrap();
    let grandchild = match proc_clone
        .call(Request::Read {
            fid: Fid(40),
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
            fid: Fid(40),
            offset: 0,
            data: br#"{"executable":"/bin/grandchild","args":[],"namespace":{"mounts":[{"path":"/proc/clone","access":"rw"}]}}"#.to_vec(),
        })
        .await
        .unwrap();
    proc_clone
        .call(Request::Clunk { fid: Fid(40) })
        .await
        .unwrap();

    let recorded_parent = String::from_utf8(
        read_at(&fs, &[&grandchild, "parent"], Fid(41))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        recorded_parent, child,
        "grandchild spawned through delegated child /proc/clone must record the child as parent"
    );
}

#[tokio::test]
async fn delegated_proc_clone_root_open_allocates_a_pending_pid() {
    let fs = proc();
    let parent = spawn(&fs, Fid(10)).await;
    let parent_pid = Pid(parent.parse::<u64>().unwrap());
    let proc_clone = fs.clone_file_for_spawner(
        Some(parent_pid),
        Namespace::new(),
        Credentials::user("alan"),
    );

    proc_clone
        .open(Fid::ROOT, OpenMode::ReadWrite)
        .await
        .unwrap();
    let child = String::from_utf8(proc_clone.read(Fid::ROOT, 0, 64).await.unwrap()).unwrap();
    proc_clone
        .write(
            Fid::ROOT,
            0,
            br#"{"executable":"/bin/child","args":[],"namespace":{"mounts":[]}}"#,
        )
        .await
        .unwrap();
    proc_clone.clunk(Fid::ROOT).await.unwrap();

    let recorded_parent =
        String::from_utf8(read_at(&fs, &[&child, "parent"], Fid(42)).await.unwrap()).unwrap();
    assert_eq!(
        recorded_parent, parent,
        "opening a delegated /proc/clone root should spawn under its spawner context"
    );
}

#[tokio::test]
async fn delegated_proc_clone_root_open_does_not_pollute_other_proc_roots() {
    let fs = proc();
    let parent = spawn(&fs, Fid(10)).await;
    let parent_pid = Pid(parent.parse::<u64>().unwrap());
    let proc_clone = fs.clone_file_for_spawner(
        Some(parent_pid),
        Namespace::new(),
        Credentials::user("alan"),
    );

    proc_clone
        .open(Fid::ROOT, OpenMode::ReadWrite)
        .await
        .unwrap();
    let pending = String::from_utf8(proc_clone.read(Fid::ROOT, 0, 64).await.unwrap()).unwrap();
    let listing = String::from_utf8(read_at(&fs, &[], Fid(43)).await.unwrap()).unwrap();
    assert!(
        listing.lines().any(|line| line == "clone"),
        "normal /proc root remains a listing while delegated clone root is open: {listing:?}"
    );
    assert!(
        !listing.lines().any(|line| line == pending),
        "pending clone pid must stay scoped to the delegated clone view"
    );
}

#[tokio::test]
async fn for_spawner_from_clone_view_resets_the_root_to_proc() {
    let fs = proc();
    let parent = spawn(&fs, Fid(10)).await;
    let parent_pid = Pid(parent.parse::<u64>().unwrap());
    let proc_clone = fs.clone_file_for_spawner(
        Some(parent_pid),
        Namespace::new(),
        Credentials::user("alan"),
    );
    let proc_view = proc_clone.for_spawner(
        Some(parent_pid),
        Namespace::new(),
        Credentials::user("alan"),
    );

    proc_view.open(Fid::ROOT, OpenMode::Read).await.unwrap();
    let listing = String::from_utf8(proc_view.read(Fid::ROOT, 0, 64).await.unwrap()).unwrap();
    assert!(
        listing.lines().any(|line| line == "clone"),
        "for_spawner must produce a full /proc view, not another clone-file view"
    );
}

#[tokio::test]
async fn a_malformed_exec_spec_is_rejected_at_clunk_and_leaks_nothing() {
    let fs = proc();

    fs.walk(Fid::ROOT, Fid(20), &["clone".to_string()])
        .await
        .unwrap();
    fs.open(Fid(20), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(fs.read(Fid(20), 0, 64).await.unwrap()).unwrap();

    // Truncated exec spec: rejected at the commit point, not before.
    fs.write(Fid(20), 0, b"{ truncated").await.unwrap();
    assert_eq!(fs.clunk(Fid(20)).await, Err(ErrorCode::BadRequest));

    // The fid-private slot was discarded; public /proc never shows it.
    let listing = String::from_utf8(read_at(&fs, &[], Fid(21)).await.unwrap()).unwrap();
    assert!(
        !listing.lines().any(|l| l == pid_name),
        "rejected spawn leaks nothing"
    );
}

// /proc/<pid>/io/output is wired to the process output stream, not Unsupported:
// reading an empty live output blocks (stream semantics) rather than erroring
// (PR #574 review).
#[tokio::test]
async fn proc_output_serves_the_stream() {
    use std::time::Duration;
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    fs.walk(Fid::ROOT, Fid(11), &[pid, "io".into(), "output".into()])
        .await
        .unwrap();
    fs.open(Fid(11), OpenMode::Read).await.unwrap();
    // Empty output stream → the read blocks; it must NOT return Unsupported.
    let r = tokio::time::timeout(Duration::from_millis(30), fs.read(Fid(11), 0, 64)).await;
    assert!(
        r.is_err(),
        "reading io/output should block on the stream, not error"
    );
}

#[tokio::test]
async fn proc_io_lists_generic_streams() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    let listing = String::from_utf8(read_at(&fs, &[&pid, "io"], Fid(11)).await.unwrap()).unwrap();

    assert_eq!(
        listing.lines().collect::<Vec<_>>(),
        vec!["input", "output", "events"]
    );
}

#[tokio::test]
async fn proc_input_accepts_write_intent_and_records_io_event() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    fs.walk(
        Fid::ROOT,
        Fid(11),
        &[pid.clone(), "io".into(), "input".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(11), OpenMode::Write).await.unwrap();
    fs.write(Fid(11), 0, b"hello proc").await.unwrap();
    fs.clunk(Fid(11)).await.unwrap();

    assert_eq!(
        String::from_utf8(read_at(&fs, &[&pid, "io", "input"], Fid(12)).await.unwrap()).unwrap(),
        "10\nhello proc"
    );
    assert_eq!(
        String::from_utf8(
            read_at(&fs, &[&pid, "io", "events"], Fid(13))
                .await
                .unwrap()
        )
        .unwrap(),
        "input:10\n"
    );
}

#[tokio::test]
async fn proc_output_accepts_write_intent() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

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

    assert_eq!(
        String::from_utf8(
            read_at(&fs, &[&pid, "io", "output"], Fid(12))
                .await
                .unwrap()
        )
        .unwrap(),
        "hello proc"
    );
    assert_eq!(
        String::from_utf8(
            read_at(&fs, &[&pid, "io", "events"], Fid(13))
                .await
                .unwrap()
        )
        .unwrap(),
        "output:10\n"
    );
}
