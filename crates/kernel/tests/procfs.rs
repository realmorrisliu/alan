//! `/proc` synthetic device (substrate §7.1) and spawn via clone-via-open
//! (§7.1a). `/proc` renders the process table as files: a `clone` file plus a
//! directory per pid (`status`, `parent`, `credentials`, `exit`, `ctl`, `io/`).
//! Process creation is pure aP — open `/proc/clone` (a pending pid, not yet
//! public), write the exec spec, and `clunk` to commit — so an aP-only client
//! needs no side API to launch a process.

use alan_ap::{ErrorCode, Fid, FileServer, InProcessTransport, OpenMode, Request, Response};
use alan_kernel::{
    Access, Credentials, Namespace, Pid, ProcFs, ProcessInvocation, ProcessOutcome, ProcessRunner,
};
use std::sync::{Arc, Mutex};

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
            let exit =
                String::from_utf8(read_at(&fs, &[&pid, "exit"], Fid(201)).await.unwrap()).unwrap();
            assert_eq!(exit, "0");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("runner did not exit process {pid}");
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
