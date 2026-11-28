//! End-to-end multi-writer regression suite exercising real `AgentRootFs` +
//! `ProcFs` + `MountFs` together (ADR-0063 / Task 5.1).
//!
//! Covers:
//! 1. Concurrent `io/input` clunks + `io/output` writes preserving exact
//!    cross-stream ordering across `io/input`, `io/output`, `io/events`, and
//!    bridged `/agent/<pid>/events` (plus dial-time `NoAccess` on `io/events`).
//! 2. Mount-time `AgentActorView::Client` and `AgentActorView::Child { self_pid }`
//!    enforcement rejecting engine-owned writes with `ErrorCode::NoAccess` and
//!    `stat.writable == false` while permitting authorized client and own-pid
//!    child writes.
//! 3. Interposition-proof `OpenMode::WriteExclusive` leases across
//!    `machine/tape`, `io/output`, and `machine/ui/events`, with deterministic
//!    lease release on `clunk`, wire transport drop (`clunk_all`), and
//!    in-process `MountFs`/transport drop.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use alan_agentfs::{AgentActorView, AgentFs, AgentRootFs};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, ImportedFileServer, InProcessTransport, Offset, OpenMode,
    ProcessEventSource, ProcessIoEventKind, Qid, Request, Response, Stat, Stream,
    export_file_server,
};
use alan_kernel::{Access, Credentials, LiveNamespace, MountFs, Namespace, Pid, ProcFs};
use alan_shell::Shell;
use async_trait::async_trait;
use tokio::sync::{Barrier, Mutex};

static NEXT_TEST_FID: AtomicU64 = AtomicU64::new(10_000);

fn next_fid() -> Fid {
    Fid(NEXT_TEST_FID.fetch_add(1, Ordering::Relaxed))
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

async fn walk_path(transport: &InProcessTransport, path: &str) -> Result<Fid, ErrorCode> {
    let fid = next_fid();
    transport
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: split_path(path),
        })
        .await?;
    Ok(fid)
}

async fn open_fid(
    transport: &InProcessTransport,
    fid: Fid,
    mode: OpenMode,
) -> Result<Response, ErrorCode> {
    transport.call(Request::Open { fid, mode }).await
}

async fn write_fid(
    transport: &InProcessTransport,
    fid: Fid,
    offset: Offset,
    data: &[u8],
) -> Result<Response, ErrorCode> {
    transport
        .call(Request::Write {
            fid,
            offset,
            data: data.to_vec(),
        })
        .await
}

async fn clunk_fid(transport: &InProcessTransport, fid: Fid) -> Result<Response, ErrorCode> {
    transport.call(Request::Clunk { fid }).await
}

async fn stat_path(transport: &InProcessTransport, path: &str) -> Stat {
    let fid = walk_path(transport, path)
        .await
        .unwrap_or_else(|e| panic!("walk {path} failed: {e:?}"));
    let Response::Stat { stat } = transport
        .call(Request::Stat { fid })
        .await
        .unwrap_or_else(|e| panic!("stat {path} failed: {e:?}"))
    else {
        panic!("expected Stat response for {path}");
    };
    let _ = clunk_fid(transport, fid).await;
    stat
}

async fn assert_write_denied_and_not_writable(transport: &InProcessTransport, path: &str) {
    let stat = stat_path(transport, path).await;
    assert!(
        !stat.writable,
        "{path} must report stat.writable == false, got true"
    );
    for mode in [
        OpenMode::Write,
        OpenMode::WriteExclusive,
        OpenMode::ReadWrite,
    ] {
        let fid = walk_path(transport, path)
            .await
            .unwrap_or_else(|e| panic!("walk {path} for {mode:?} failed: {e:?}"));
        let err = open_fid(transport, fid, mode)
            .await
            .expect_err(&format!("{path} open({mode:?}) must fail with NoAccess"));
        assert_eq!(
            err,
            ErrorCode::NoAccess,
            "{path} open({mode:?}) returned {err:?} instead of NoAccess"
        );
        let _ = clunk_fid(transport, fid).await;
    }
}

async fn write_with_mode(
    transport: &InProcessTransport,
    path: &str,
    mode: OpenMode,
    payload: &[u8],
) -> Result<(), ErrorCode> {
    let stat = stat_path(transport, path).await;
    assert!(
        stat.writable,
        "{path} must report stat.writable == true, got false"
    );
    let fid = walk_path(transport, path).await?;
    open_fid(transport, fid, mode).await?;
    write_fid(transport, fid, 0, payload).await?;
    clunk_fid(transport, fid).await?;
    Ok(())
}

async fn assert_exclusive_write_only(
    transport: &InProcessTransport,
    path: &str,
    payload: &[u8],
) -> Result<(), ErrorCode> {
    for rejected in [OpenMode::Write, OpenMode::ReadWrite] {
        let fid = walk_path(transport, path).await?;
        assert_eq!(
            open_fid(transport, fid, rejected).await,
            Err(ErrorCode::NoAccess),
            "{path} open({rejected:?}) must fail with NoAccess"
        );
        let _ = clunk_fid(transport, fid).await;
    }
    write_with_mode(transport, path, OpenMode::WriteExclusive, payload).await
}

async fn clone_entry(transport: &InProcessTransport, clone_path: &str) -> String {
    let fid = walk_path(transport, clone_path).await.unwrap();
    open_fid(transport, fid, OpenMode::ReadWrite).await.unwrap();
    let Response::Read { data } = transport
        .call(Request::Read {
            fid,
            offset: 0,
            count: 64,
        })
        .await
        .unwrap()
    else {
        panic!("expected Read response from {clone_path}");
    };
    clunk_fid(transport, fid).await.unwrap();
    String::from_utf8(data).unwrap()
}

fn parse_framed_input_lengths(bytes: &[u8]) -> Vec<u32> {
    let mut lengths = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let nl = bytes[cursor..]
            .iter()
            .position(|&b| b == b'\n')
            .expect("framed input header newline");
        let len: usize = std::str::from_utf8(&bytes[cursor..cursor + nl])
            .unwrap()
            .parse()
            .unwrap();
        cursor += nl + 1;
        assert!(cursor + len <= bytes.len(), "truncated framed input");
        lengths.push(len as u32);
        cursor += len;
    }
    lengths
}

fn parse_output_run_lengths(bytes: &[u8]) -> Vec<u32> {
    let mut lengths = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let marker = bytes[cursor];
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor] == marker {
            cursor += 1;
        }
        lengths.push((cursor - start) as u32);
    }
    lengths
}

fn parse_io_event_lines(bytes: &[u8]) -> Vec<(ProcessIoEventKind, u32)> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .map(|line| {
            let (kind, count) = line
                .split_once(':')
                .unwrap_or_else(|| panic!("expected kind:count line, got {line:?}"));
            let kind = match kind {
                "input" => ProcessIoEventKind::Input,
                "output" => ProcessIoEventKind::Output,
                other => panic!("unexpected IO event kind {other:?} in {line:?}"),
            };
            (kind, count.parse::<u32>().unwrap())
        })
        .collect()
}

struct E2eEnv {
    procfs: Arc<ProcFs>,
    agent_root: Arc<AgentRootFs>,
}

impl E2eEnv {
    fn new() -> Self {
        let procfs = Arc::new(ProcFs::new());
        let proc_server: Arc<dyn FileServer> = procfs.clone();
        let proc_events: Arc<dyn ProcessEventSource> = procfs.clone();
        let agent_root = Arc::new(AgentRootFs::new_with_process_events(
            proc_server,
            proc_events,
        ));
        Self { procfs, agent_root }
    }

    async fn spawn_agent(&self, parent: Option<Pid>) -> String {
        let spawner = self
            .procfs
            .for_spawner(parent, Namespace::new(), Credentials::user("alan"));
        let fid = next_fid();
        spawner
            .walk(Fid::ROOT, fid, &["clone".to_string()])
            .await
            .unwrap();
        spawner.open(fid, OpenMode::ReadWrite).await.unwrap();
        let pid = String::from_utf8(spawner.read(fid, 0, 64).await.unwrap()).unwrap();
        spawner
            .write(
                fid,
                0,
                br#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#,
            )
            .await
            .unwrap();
        spawner.clunk(fid).await.unwrap();
        self.agent_root
            .bind_process(pid.clone(), Arc::new(AgentFs::new()))
            .await;
        pid
    }

    fn mount_namespace(&self, view: AgentActorView) -> (LiveNamespace, InProcessTransport) {
        let mut ns = Namespace::new();
        ns.mount(
            "/proc",
            InProcessTransport::new(self.procfs.clone()),
            Access::ReadWrite,
        );
        ns.mount(
            "/agent",
            InProcessTransport::new(self.agent_root.actor_view(view)),
            Access::ReadWrite,
        );
        let live = LiveNamespace::new(ns);
        let transport =
            InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live.clone())));
        (live, transport)
    }
}

/// Naive interposing `FileServer` that counts `open` calls and would grant any
/// write mode if the kernel lease table failed to block it first.
struct PermissiveInterposerFs {
    stream: Stream,
    fids: Mutex<HashMap<Fid, bool>>,
    open_calls: AtomicUsize,
}

impl PermissiveInterposerFs {
    fn new() -> Self {
        Self {
            stream: Stream::new(),
            fids: Mutex::new(HashMap::new()),
            open_calls: AtomicUsize::new(0),
        }
    }

    fn open_calls(&self) -> usize {
        self.open_calls.load(Ordering::SeqCst)
    }

    fn qid(is_dir: bool) -> Qid {
        Qid {
            kind: if is_dir {
                FileKind::Dir
            } else {
                FileKind::Stream
            },
            version: 0,
            path: if is_dir { 10 } else { 20 },
        }
    }
}

#[async_trait]
impl FileServer for PermissiveInterposerFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let is_dir = matches!(
            names.last().map(String::as_str),
            None | Some("machine") | Some("ui") | Some("io")
        );
        self.fids.lock().await.insert(newfid, is_dir);
        Ok(Self::qid(is_dir))
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.open_calls.fetch_add(1, Ordering::SeqCst);
        let is_dir = *self
            .fids
            .lock()
            .await
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?;
        Ok(Self::qid(is_dir))
    }

    async fn read(&self, _fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(self.stream.read(offset, count).await)
    }

    async fn write(&self, _fid: Fid, _offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.stream.append(data).await;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let is_dir = self.fids.lock().await.get(&fid).copied().unwrap_or(false);
        Ok(Stat {
            name: String::new(),
            qid: Self::qid(is_dir),
            length: self.stream.len().await,
            executable: false,
            writable: !is_dir,
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids.lock().await.remove(&fid);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids.lock().await.remove(&fid);
        Ok(())
    }
}

#[tokio::test]
async fn e2e_concurrent_io_input_clunks_and_io_output_writes_preserve_cross_stream_order() {
    let env = E2eEnv::new();
    let pid = env.spawn_agent(None).await;
    env.agent_root.set_root_process(pid.clone()).await;

    let (_, engine) = env.mount_namespace(AgentActorView::Engine {
        self_pid: pid.clone(),
    });
    let (_, client) = env.mount_namespace(AgentActorView::Client);
    let shell = Shell::new(engine.clone());

    // Dial-time write denial holds on both `/proc/<pid>/io/events` and `/agent/<pid>/io/events`.
    for events_path in [
        format!("/proc/{pid}/io/events"),
        format!("/agent/{pid}/io/events"),
        "/agent/root/io/events".to_string(),
    ] {
        assert_write_denied_and_not_writable(&engine, &events_path).await;
        assert_write_denied_and_not_writable(&client, &events_path).await;
    }

    // Race 12 concurrent client `io/input` clunks (distinct lengths 101..=112)
    // against an owning Engine streaming 12 distinct `io/output` chunks
    // (distinct marker bytes `A..=L` and lengths 201..=212) over the real
    // MountFs -> AgentActorView -> AgentRootFs -> ProcFs -> ProcessEventSink -> AgentFs stack.
    let per_direction = 12usize;
    let barrier = Arc::new(Barrier::new(per_direction + 1));
    let mut handles = Vec::with_capacity(per_direction + 1);

    for index in 0..per_direction {
        let client = client.clone();
        let barrier = barrier.clone();
        let target = if index % 2 == 0 {
            "/agent/root/io/input".to_string()
        } else {
            format!("/agent/{pid}/io/input")
        };
        let payload = vec![b'a' + (index % 26) as u8; 101 + index];
        handles.push(tokio::spawn(async move {
            let fid = walk_path(&client, &target).await.unwrap();
            open_fid(&client, fid, OpenMode::Write).await.unwrap();
            write_fid(&client, fid, 0, &payload).await.unwrap();
            barrier.wait().await;
            clunk_fid(&client, fid).await.unwrap();
        }));
    }

    {
        let engine = engine.clone();
        let barrier = barrier.clone();
        let output_path = format!("/agent/{pid}/io/output");
        handles.push(tokio::spawn(async move {
            let fid = walk_path(&engine, &output_path).await.unwrap();
            open_fid(&engine, fid, OpenMode::WriteExclusive)
                .await
                .unwrap();
            barrier.wait().await;
            let mut offset = 0u64;
            for index in 0..per_direction {
                let chunk = vec![b'A' + index as u8; 201 + index];
                write_fid(&engine, fid, offset, &chunk).await.unwrap();
                offset += chunk.len() as u64;
                tokio::task::yield_now().await;
            }
            clunk_fid(&engine, fid).await.unwrap();
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let agent_input = shell.cat(&format!("/agent/{pid}/io/input")).await.unwrap();
    let proc_input = shell.cat(&format!("/proc/{pid}/io/input")).await.unwrap();
    let agent_output = shell.cat(&format!("/agent/{pid}/io/output")).await.unwrap();
    let proc_output = shell.cat(&format!("/proc/{pid}/io/output")).await.unwrap();
    let agent_io_events = shell.cat(&format!("/agent/{pid}/io/events")).await.unwrap();
    let proc_io_events = shell.cat(&format!("/proc/{pid}/io/events")).await.unwrap();
    let bridged_events = shell.cat(&format!("/agent/{pid}/events")).await.unwrap();

    assert_eq!(agent_input, proc_input);
    assert_eq!(agent_output, proc_output);
    assert_eq!(agent_io_events, proc_io_events);

    // `bind_process` replays the process's initial `status:running` lifecycle
    // record into `/agent/<pid>/events`; every subsequent record must match
    // `/agent/<pid>/io/events` byte-for-byte in exact commit order.
    let bridged_io_tail = bridged_events
        .strip_prefix(b"status:running\n")
        .expect("bridged /agent/<pid>/events must start with replayed status:running");
    assert_eq!(
        bridged_io_tail,
        agent_io_events.as_slice(),
        "bridged /agent/<pid>/events IO records must match /agent/<pid>/io/events byte-for-byte"
    );

    let io_events = parse_io_event_lines(&agent_io_events);
    let bridged_parsed = parse_io_event_lines(bridged_io_tail);
    assert_eq!(io_events.len(), per_direction * 2);
    assert_eq!(bridged_parsed, io_events);

    let input_lengths = parse_framed_input_lengths(&agent_input);
    let io_event_input_lengths: Vec<u32> = io_events
        .iter()
        .filter_map(|(kind, count)| (*kind == ProcessIoEventKind::Input).then_some(*count))
        .collect();
    assert_eq!(
        io_event_input_lengths, input_lengths,
        "io/events & bridged events input order must match framed io/input bytes"
    );

    let output_run_lengths = parse_output_run_lengths(&agent_output);
    let io_event_output_lengths: Vec<u32> = io_events
        .iter()
        .filter_map(|(kind, count)| (*kind == ProcessIoEventKind::Output).then_some(*count))
        .collect();
    assert_eq!(
        io_event_output_lengths, output_run_lengths,
        "io/events & bridged events output order must match io/output byte chunks"
    );
}

#[tokio::test]
async fn e2e_client_and_child_actor_views_enforce_write_boundaries_over_mountfs() {
    let env = E2eEnv::new();
    let root_pid = env.spawn_agent(None).await;
    env.agent_root.set_root_process(root_pid.clone()).await;
    let root_pid_num = Pid(root_pid.parse().unwrap());
    let child_pid = env.spawn_agent(Some(root_pid_num)).await;
    let sibling_pid = env.spawn_agent(Some(root_pid_num)).await;

    let (_, root_engine) = env.mount_namespace(AgentActorView::Engine {
        self_pid: root_pid.clone(),
    });
    let (_, client) = env.mount_namespace(AgentActorView::Client);
    let (_, child_view) = env.mount_namespace(AgentActorView::Child {
        self_pid: child_pid.clone(),
    });

    // Seed one request and one action on root and child through their owning views.
    let root_req = clone_entry(&root_engine, &format!("/agent/{root_pid}/requests/clone")).await;
    write_with_mode(
        &root_engine,
        &format!("/agent/{root_pid}/requests/{root_req}/prompt"),
        OpenMode::Write,
        b"approve root action?",
    )
    .await
    .unwrap();
    let root_action = clone_entry(&root_engine, &format!("/agent/{root_pid}/actions/clone")).await;
    write_with_mode(
        &root_engine,
        &format!("/agent/{root_pid}/actions/{root_action}/name"),
        OpenMode::Write,
        b"read_file",
    )
    .await
    .unwrap();

    let child_req = clone_entry(&child_view, &format!("/agent/{child_pid}/requests/clone")).await;
    let child_action = clone_entry(&child_view, &format!("/agent/{child_pid}/actions/clone")).await;

    // (1) Client view rejects engine-owned surfaces on both `/agent/root` and `/agent/<root_pid>`.
    for prefix in ["/agent/root".to_string(), format!("/agent/{root_pid}")] {
        for forbidden in [
            format!("{prefix}/machine/tape"),
            format!("{prefix}/io/output"),
            format!("{prefix}/io/events"),
            format!("{prefix}/machine/ui/events"),
            format!("{prefix}/machine/ui/activity"),
            format!("{prefix}/actions/clone"),
            format!("{prefix}/actions/{root_action}/name"),
            format!("{prefix}/actions/{root_action}/status"),
            format!("{prefix}/actions/{root_action}/output"),
            format!("{prefix}/actions/{root_action}/result"),
            format!("{prefix}/requests/clone"),
            format!("{prefix}/requests/{root_req}/prompt"),
            format!("{prefix}/requests/{root_req}/ctl"),
            format!("{prefix}/ctl"),
        ] {
            assert_write_denied_and_not_writable(&client, &forbidden).await;
        }
    }

    // Client writes to authorized surfaces succeed and propagate through ProcFs/AgentFs.
    write_with_mode(
        &client,
        "/agent/root/io/input",
        OpenMode::Write,
        b"client prompt",
    )
    .await
    .unwrap();
    write_with_mode(
        &client,
        "/agent/root/machine/ctl",
        OpenMode::Write,
        b"interrupt",
    )
    .await
    .unwrap();
    write_with_mode(
        &client,
        &format!("/agent/root/requests/{root_req}/response"),
        OpenMode::Write,
        b"approved",
    )
    .await
    .unwrap();

    let client_shell = Shell::new(client.clone());
    assert_eq!(
        client_shell
            .cat(&format!("/proc/{root_pid}/io/input"))
            .await
            .unwrap(),
        b"13\nclient prompt"
    );
    assert_eq!(
        client_shell
            .cat(&format!("/agent/root/requests/{root_req}/status"))
            .await
            .unwrap(),
        b"answered"
    );
    let root_events =
        String::from_utf8(client_shell.cat("/agent/root/events").await.unwrap()).unwrap();
    assert!(root_events.contains("input:13\n"), "{root_events:?}");
    assert!(root_events.contains("ctl:interrupt\n"), "{root_events:?}");

    // Anti-DoS yield guarantee: after Engine releases its tape fid on yield,
    // Client still cannot acquire a lease on `machine/tape`, so Engine resumes cleanly.
    assert_exclusive_write_only(&root_engine, "/agent/root/machine/tape", b"turn-1\n")
        .await
        .unwrap();
    assert_write_denied_and_not_writable(&client, "/agent/root/machine/tape").await;
    assert_exclusive_write_only(&root_engine, "/agent/root/machine/tape", b"turn-2\n")
        .await
        .unwrap();

    // (2) Child view rejects all writes on parent, `/agent/root`, and sibling —
    // including both engine-owned and client surfaces — while allowing own-pid writes.
    for foreign in [
        format!("/agent/{root_pid}"),
        "/agent/root".to_string(),
        format!("/agent/{sibling_pid}"),
    ] {
        for forbidden in [
            format!("{foreign}/machine/tape"),
            format!("{foreign}/io/output"),
            format!("{foreign}/machine/ui/events"),
            format!("{foreign}/actions/clone"),
            format!("{foreign}/requests/clone"),
            format!("{foreign}/io/input"),
            format!("{foreign}/machine/ctl"),
        ] {
            assert_write_denied_and_not_writable(&child_view, &forbidden).await;
        }
    }
    assert_write_denied_and_not_writable(
        &child_view,
        &format!("/agent/{root_pid}/actions/{root_action}/name"),
    )
    .await;
    assert_write_denied_and_not_writable(
        &child_view,
        &format!("/agent/{root_pid}/requests/{root_req}/response"),
    )
    .await;

    // Own-pid child writes succeed across both direct `/agent/<child_pid>` and
    // `/agent/<root_pid>/children/<child_pid>`.
    for own in [
        format!("/agent/{child_pid}"),
        format!("/agent/{root_pid}/children/{child_pid}"),
    ] {
        assert_exclusive_write_only(&child_view, &format!("{own}/machine/tape"), b"child-tape\n")
            .await
            .unwrap();
        assert_exclusive_write_only(&child_view, &format!("{own}/io/output"), b"child-out\n")
            .await
            .unwrap();
        assert_exclusive_write_only(
            &child_view,
            &format!("{own}/machine/ui/events"),
            b"{\"kind\":\"tick\"}\n",
        )
        .await
        .unwrap();
        write_with_mode(
            &child_view,
            &format!("{own}/requests/{child_req}/prompt"),
            OpenMode::Write,
            b"child prompt",
        )
        .await
        .unwrap();
        write_with_mode(
            &child_view,
            &format!("{own}/actions/{child_action}/name"),
            OpenMode::Write,
            b"grep",
        )
        .await
        .unwrap();
    }

    // Backing read-only surfaces on own pid remain read-only for Child.
    for ro in [
        format!("/agent/{child_pid}/io/events"),
        format!("/agent/{child_pid}/events"),
        format!("/agent/{child_pid}/machine/status"),
    ] {
        assert_write_denied_and_not_writable(&child_view, &ro).await;
    }

    // Child can still observe parent surfaces read-only.
    let child_shell = Shell::new(child_view);
    assert_eq!(
        child_shell.cat("/agent/root/machine/tape").await.unwrap(),
        b"turn-1\nturn-2\n"
    );
}

#[tokio::test]
async fn e2e_interposer_cannot_bypass_exclusive_leases_and_releases_on_clunk_and_drop() {
    let env = E2eEnv::new();
    let pid = env.spawn_agent(None).await;
    env.agent_root.set_root_process(pid.clone()).await;

    let (live, engine) = env.mount_namespace(AgentActorView::Engine {
        self_pid: pid.clone(),
    });
    let (_, client) = env.mount_namespace(AgentActorView::Client);
    let client_shell = Shell::new(client);

    for (surface, deep_mount_prefix) in [
        ("machine/tape", format!("/agent/{pid}/machine")),
        ("io/output", format!("/agent/{pid}/io")),
        ("machine/ui/events", format!("/agent/{pid}/machine")),
    ] {
        let concrete_path = format!("/agent/{pid}/{surface}");
        let root_alias = format!("/agent/root/{surface}");

        // Restore `/agent` to the real Engine view at the start of each surface test.
        live.replace_mount(
            "/agent",
            InProcessTransport::new(env.agent_root.actor_view(AgentActorView::Engine {
                self_pid: pid.clone(),
            })),
            Access::ReadWrite,
        );
        let direct_engine =
            InProcessTransport::new(Arc::new(MountFs::new(live.snapshot().child())));

        // 1. Engine holds an active `OpenMode::WriteExclusive` lease on the real surface.
        let writer_fid = walk_path(&engine, &concrete_path).await.unwrap();
        open_fid(&engine, writer_fid, OpenMode::WriteExclusive)
            .await
            .unwrap();
        write_fid(&engine, writer_fid, 0, b"engine-1\n")
            .await
            .unwrap();

        // 2. Stack a permissive interposer over `/agent` on `live` AND a deeper
        //    interposer on a derived child namespace sharing the same lease table.
        let top_interposer = Arc::new(PermissiveInterposerFs::new());
        live.mount(
            "/agent",
            InProcessTransport::new(top_interposer.clone()),
            Access::ReadWrite,
        );
        let live_contender =
            InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live.clone())));

        let mut deep_ns = live.snapshot().child();
        let deep_interposer = Arc::new(PermissiveInterposerFs::new());
        deep_ns.mount(
            &deep_mount_prefix,
            InProcessTransport::new(deep_interposer.clone()),
            Access::ReadWrite,
        );
        let deep_contender = InProcessTransport::new(Arc::new(MountFs::new(deep_ns)));

        for (contender, interposer) in [
            (&live_contender, &top_interposer),
            (&deep_contender, &deep_interposer),
        ] {
            for mode in [
                OpenMode::Write,
                OpenMode::WriteExclusive,
                OpenMode::ReadWrite,
            ] {
                let fid = walk_path(contender, &concrete_path).await.unwrap();
                assert_eq!(
                    open_fid(contender, fid, mode).await,
                    Err(ErrorCode::NoAccess),
                    "stacked interposer must not grant {mode:?} on {concrete_path} while lease is held"
                );
                clunk_fid(contender, fid).await.unwrap();
            }
            assert_eq!(
                interposer.open_calls(),
                0,
                "kernel lease boundary must reject conflicting opens before reaching interposer"
            );
        }

        // Concurrent client `OpenMode::Read` tail continues unblocked while lease is active.
        let mut tail = client_shell.tail(&root_alias).await.unwrap();
        assert_eq!(tail.read(4096).await.unwrap(), b"engine-1\n");
        let tail_task = tokio::spawn(async move {
            let chunk = tail.read(4096).await.unwrap();
            tail.close().await.unwrap();
            chunk
        });
        tokio::task::yield_now().await;
        write_fid(&engine, writer_fid, 9, b"engine-2\n")
            .await
            .unwrap();
        let observed = tokio::time::timeout(Duration::from_secs(1), tail_task)
            .await
            .expect("client tail must not block under active WriteExclusive lease")
            .unwrap();
        assert_eq!(observed, b"engine-2\n");

        // 3. Explicit `clunk` deterministically releases the lease.
        clunk_fid(&engine, writer_fid).await.unwrap();
        let after_clunk_fid = walk_path(&direct_engine, &concrete_path).await.unwrap();
        open_fid(&direct_engine, after_clunk_fid, OpenMode::WriteExclusive)
            .await
            .expect("lease must be available immediately after clunk");
        clunk_fid(&direct_engine, after_clunk_fid).await.unwrap();

        // 4. Wire transport drop triggers `clunk_all` and releases both kernel and
        //    real AgentRootFs/AgentFs backing leases deterministically.
        live.replace_mount(
            "/agent",
            InProcessTransport::new(env.agent_root.actor_view(AgentActorView::Engine {
                self_pid: pid.clone(),
            })),
            Access::ReadWrite,
        );
        let server_fs = Arc::new(MountFs::from_live_namespace(live.clone()));
        let (client_io, server_io) = tokio::io::duplex(8192);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (server_read, server_write) = tokio::io::split(server_io);
        let server_task = tokio::spawn(async move {
            let _ = export_file_server(
                server_fs.clone(),
                tokio::io::BufReader::new(server_read),
                server_write,
            )
            .await;
            server_fs.clunk_all().await;
        });

        let wire_client =
            ImportedFileServer::new(tokio::io::BufReader::new(client_read), client_write);
        let wire_fid = next_fid();
        wire_client
            .walk(Fid::ROOT, wire_fid, &split_path(&concrete_path))
            .await
            .unwrap();
        wire_client
            .open(wire_fid, OpenMode::WriteExclusive)
            .await
            .unwrap();
        wire_client.write(wire_fid, 18, b"wire-1\n").await.unwrap();

        let wire_interposer = Arc::new(PermissiveInterposerFs::new());
        live.mount(
            "/agent",
            InProcessTransport::new(wire_interposer.clone()),
            Access::ReadWrite,
        );
        let wire_contender =
            InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live.clone())));
        let blocked_fid = walk_path(&wire_contender, &concrete_path).await.unwrap();
        assert_eq!(
            open_fid(&wire_contender, blocked_fid, OpenMode::WriteExclusive).await,
            Err(ErrorCode::NoAccess)
        );
        clunk_fid(&wire_contender, blocked_fid).await.unwrap();
        assert_eq!(wire_interposer.open_calls(), 0);

        // Drop wire client WITHOUT clunking `wire_fid`; server disconnects and runs `clunk_all`.
        drop(wire_client);
        server_task.await.unwrap();

        let post_wire_fid = walk_path(&direct_engine, &concrete_path).await.unwrap();
        open_fid(&direct_engine, post_wire_fid, OpenMode::WriteExclusive)
            .await
            .expect("wire disconnect must release both kernel and AgentRootFs/AgentFs leases");
        write_fid(&direct_engine, post_wire_fid, 25, b"after-wire-drop\n")
            .await
            .unwrap();
        clunk_fid(&direct_engine, post_wire_fid).await.unwrap();

        // 5. In-process `MountFs`/transport drop synchronously releases the kernel lease
        //    even without an explicit `clunk` or `clunk_all`.
        let holder_transport =
            InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live.clone())));
        let holder_fid = walk_path(&holder_transport, &concrete_path).await.unwrap();
        open_fid(&holder_transport, holder_fid, OpenMode::WriteExclusive)
            .await
            .unwrap();

        let contending_fid = walk_path(&direct_engine, &concrete_path).await.unwrap();
        assert_eq!(
            open_fid(&direct_engine, contending_fid, OpenMode::WriteExclusive).await,
            Err(ErrorCode::NoAccess),
            "kernel lease held via interposer must block direct Engine on {concrete_path}"
        );

        drop(holder_transport);

        open_fid(&direct_engine, contending_fid, OpenMode::WriteExclusive)
            .await
            .expect(
                "dropping in-process MountFs/transport must release kernel lease synchronously",
            );
        write_fid(&direct_engine, contending_fid, 41, b"after-inproc-drop\n")
            .await
            .unwrap();
        clunk_fid(&direct_engine, contending_fid).await.unwrap();

        assert_eq!(
            client_shell.cat(&root_alias).await.unwrap(),
            b"engine-1\nengine-2\nwire-1\nafter-wire-drop\nafter-inproc-drop\n",
            "{concrete_path} stream must contain intact, non-interleaved writes across all phases"
        );
    }
}
