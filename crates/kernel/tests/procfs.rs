//! `/proc` synthetic device (substrate §7.1) and spawn via clone-via-open
//! (§7.1a). `/proc` renders the process table as files: a `clone` file plus a
//! directory per pid (`status`, `parent`, `credentials`, `exit`, `ctl`, `io/`).
//! Process creation is pure aP — open `/proc/clone` (a pending pid, not yet
//! public), write the exec spec, and `clunk` to commit — so an aP-only client
//! needs no side API to launch a process.

use alan_ap::{
    ErrorCode, Fid, FileServer, InProcessTransport, OpenMode, ProcessEvent, ProcessEventSink,
    ProcessEventSource, ProcessInputEventSink, ProcessInputEventSource, ProcessOutputEventSink,
    ProcessOutputEventSource, Request, Response,
};
use alan_kernel::{
    Access, Credentials, Namespace, Pid, ProcFs, ProcessInvocation, ProcessOutcome, ProcessRunner,
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
    spawn_exec(fs, clone_fid, "/bin/agent", Vec::<String>::new()).await
}

async fn spawn_exec(fs: &ProcFs, clone_fid: Fid, executable: &str, args: Vec<String>) -> String {
    fs.walk(Fid::ROOT, clone_fid, &["clone".to_string()])
        .await
        .unwrap();
    fs.open(clone_fid, OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(fs.read(clone_fid, 0, 64).await.unwrap()).unwrap();
    let exec = serde_json::json!({
        "executable": executable,
        "args": args,
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
    fs.write(Fid(10), 0, br#"{"executable":"/bin/agent","args":[]}"#)
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
    let child = spawn(&spawner, Fid(20)).await;
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
            data: br#"{"executable":"/bin/grandchild","args":[]}"#.to_vec(),
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
    let child = spawn(&spawner, Fid(20)).await;
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
            data: br#"{"executable":"/bin/grandchild","args":[]}"#.to_vec(),
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
        .write(Fid::ROOT, 0, br#"{"executable":"/bin/child","args":[]}"#)
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
        "hello proc"
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
    fs.write(Fid(10), 14, br#""/bin/agent","args":[]}"#)
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
async fn clone_uses_the_spawner_context_for_child_identity() {
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

    let child = spawn(&spawner, Fid(20)).await;

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
        "child namespace inherits the spawner namespace: {namespace:?}"
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
            data: br#"{"executable":"/bin/grandchild","args":[]}"#.to_vec(),
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
