use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, ProcessEventSource, Qid,
    Stat,
};
use alan_kernel::{
    Access, Credentials, MountFs, Namespace, Pid, ProcFs, ProcessInvocation, ProcessOutcome,
    ProcessRunner,
};
use alan_memfs::MemFs;
use alan_shell::Shell;
use async_trait::async_trait;
use tokio::sync::Notify;

fn namespace_shell_with_agent_root() -> (InProcessTransport, Shell, Arc<AgentRootFs>, Arc<ProcFs>) {
    let proc = Arc::new(ProcFs::new());
    namespace_shell_with_agent_root_for_proc(proc)
}

fn namespace_shell_with_agent_root_for_proc(
    proc: Arc<ProcFs>,
) -> (InProcessTransport, Shell, Arc<AgentRootFs>, Arc<ProcFs>) {
    let proc_server: Arc<dyn FileServer> = proc.clone();
    let proc_events: Arc<dyn ProcessEventSource> = proc.clone();
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        proc_server,
        proc_events,
    ));

    let mut namespace = Namespace::new();
    namespace.mount(
        "/proc",
        InProcessTransport::new(proc.clone()),
        Access::ReadWrite,
    );
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );

    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    (root.clone(), Shell::new(root), agent_root, proc)
}

struct ImmediateOutputRunner {
    output: Vec<u8>,
}

impl ImmediateOutputRunner {
    fn new(output: impl Into<Vec<u8>>) -> Self {
        Self {
            output: output.into(),
        }
    }
}

#[async_trait]
impl ProcessRunner for ImmediateOutputRunner {
    async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
        ProcessOutcome::exited(0, self.output.clone())
    }
}

async fn spawn_on_proc(proc: &ProcFs, fid: Fid) -> String {
    proc.walk(Fid::ROOT, fid, &["clone".into()])
        .await
        .expect("walk clone");
    proc.open(fid, OpenMode::ReadWrite)
        .await
        .expect("open clone");
    let pid = String::from_utf8(proc.read(fid, 0, 64).await.expect("read pid")).unwrap();
    proc.write(fid, 0, br#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .expect("write exec");
    proc.clunk(fid).await.expect("commit process");
    pid
}

async fn wait_for_file_contains(shell: &Shell, path: &str, needle: &str) {
    for _ in 0..50 {
        let text = String::from_utf8(shell.cat(path).await.unwrap()).unwrap();
        if text.contains(needle) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{path} did not contain {needle:?}");
}

#[tokio::test]
async fn agent_root_lists_only_proc_backed_agent_processes() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();

    agent_root
        .bind_process("999", Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process("999").await;
    assert_eq!(shell.ls("/agent").await.unwrap(), Vec::<String>::new());
    assert!(matches!(
        shell.ls("/agent/999").await,
        Err(ErrorCode::NotFound)
    ));

    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;

    let listing = shell.ls("/agent").await.unwrap();
    assert!(listing.iter().any(|entry| entry == &pid), "{listing:?}");
    assert!(listing.iter().any(|entry| entry == "root"), "{listing:?}");
    assert!(!listing.iter().any(|entry| entry == "999"), "{listing:?}");
}

#[tokio::test]
async fn agent_root_qid_version_changes_with_listing() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();

    let empty_qid = agent_root.stat(Fid::ROOT).await.unwrap().qid;
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let populated_qid = agent_root.stat(Fid::ROOT).await.unwrap().qid;
    assert_eq!(empty_qid.kind, FileKind::Dir);
    assert_eq!(empty_qid.path, populated_qid.path);
    assert_ne!(empty_qid.version, populated_qid.version);
}

#[tokio::test]
async fn agent_root_alias_forwards_to_the_root_agent_surface() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;

    shell
        .write("/agent/root/io/output", b"hello root")
        .await
        .unwrap();

    assert_eq!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/io/output")).await.unwrap()).unwrap(),
        "hello root"
    );
    assert_eq!(
        String::from_utf8(shell.cat(&format!("/proc/{pid}/io/output")).await.unwrap()).unwrap(),
        "hello root"
    );
    assert!(matches!(
        shell.ls(&format!("/proc/{pid}/machine")).await,
        Err(ErrorCode::NotFound)
    ));
}

#[tokio::test]
async fn agent_io_output_writes_to_the_proc_output_stream() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let io_fid = Fid(9_100);
    let output_fid = Fid(9_101);
    agent_root
        .walk(Fid::ROOT, io_fid, &[pid.clone(), "io".to_string()])
        .await
        .unwrap();
    agent_root
        .walk(io_fid, output_fid, &["output".to_string()])
        .await
        .unwrap();
    agent_root.open(output_fid, OpenMode::Write).await.unwrap();
    agent_root
        .write(output_fid, 0, b"shared output")
        .await
        .unwrap();
    agent_root.clunk(output_fid).await.unwrap();
    agent_root.clunk(io_fid).await.unwrap();

    assert_eq!(
        String::from_utf8(shell.cat(&format!("/proc/{pid}/io/output")).await.unwrap()).unwrap(),
        "shared output"
    );
    assert_eq!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/io/output")).await.unwrap()).unwrap(),
        "shared output"
    );
    assert!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap())
            .unwrap()
            .contains("output:13")
    );
}

#[tokio::test]
async fn agent_io_input_writes_to_the_proc_input_stream() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let io_fid = Fid(9_150);
    let input_fid = Fid(9_151);
    agent_root
        .walk(Fid::ROOT, io_fid, &[pid.clone(), "io".to_string()])
        .await
        .unwrap();
    agent_root
        .walk(io_fid, input_fid, &["input".to_string()])
        .await
        .unwrap();
    agent_root.open(input_fid, OpenMode::Write).await.unwrap();
    agent_root
        .write(input_fid, 0, b"shared input")
        .await
        .unwrap();
    agent_root.clunk(input_fid).await.unwrap();
    agent_root.clunk(io_fid).await.unwrap();

    assert_eq!(
        String::from_utf8(shell.cat(&format!("/proc/{pid}/io/input")).await.unwrap()).unwrap(),
        "12\nshared input"
    );
    assert_eq!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/io/input")).await.unwrap()).unwrap(),
        "12\nshared input"
    );
    for path in [
        format!("/agent/{pid}/io/events"),
        format!("/agent/{pid}/events"),
    ] {
        assert!(
            String::from_utf8(shell.cat(&path).await.unwrap())
                .unwrap()
                .contains("input:12"),
            "{path} should publish a proc-owned input event"
        );
    }
}

#[tokio::test]
async fn direct_proc_output_writes_publish_agent_events() {
    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let output_fid = Fid(9_200);
    proc.walk(
        Fid::ROOT,
        output_fid,
        &[pid.clone(), "io".to_string(), "output".to_string()],
    )
    .await
    .unwrap();
    proc.open(output_fid, OpenMode::Write).await.unwrap();
    proc.write(output_fid, 0, b"direct proc").await.unwrap();
    proc.clunk(output_fid).await.unwrap();

    assert_eq!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/io/output")).await.unwrap()).unwrap(),
        "direct proc"
    );
    for path in [
        format!("/agent/{pid}/events"),
        format!("/agent/{pid}/io/events"),
    ] {
        assert!(
            String::from_utf8(shell.cat(&path).await.unwrap())
                .unwrap()
                .contains("output:11"),
            "{path} should publish a proc-owned output event"
        );
    }
}

#[tokio::test]
async fn direct_proc_input_writes_publish_agent_events() {
    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let input_fid = Fid(9_250);
    proc.walk(
        Fid::ROOT,
        input_fid,
        &[pid.clone(), "io".to_string(), "input".to_string()],
    )
    .await
    .unwrap();
    proc.open(input_fid, OpenMode::Write).await.unwrap();
    proc.write(input_fid, 0, b"direct input").await.unwrap();
    proc.clunk(input_fid).await.unwrap();

    assert_eq!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/io/input")).await.unwrap()).unwrap(),
        "12\ndirect input"
    );
    for path in [
        format!("/agent/{pid}/events"),
        format!("/agent/{pid}/io/events"),
    ] {
        assert!(
            String::from_utf8(shell.cat(&path).await.unwrap())
                .unwrap()
                .contains("input:12"),
            "{path} should publish a direct proc input event"
        );
    }
}

#[tokio::test]
async fn bind_process_replays_existing_proc_io_events() {
    let proc = Arc::new(
        ProcFs::new().with_runner(Arc::new(ImmediateOutputRunner::new("early runner output"))),
    );
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root_for_proc(proc);
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    wait_for_file_contains(&shell, &format!("/proc/{pid}/io/events"), "output:19").await;

    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let events =
        String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap()).unwrap();
    assert!(
        events.contains("output:19"),
        "late-bound agent aggregate should replay existing proc IO events: {events:?}"
    );
}

#[tokio::test]
async fn bind_process_replays_existing_proc_lifecycle_events() {
    let proc =
        Arc::new(ProcFs::new().with_runner(Arc::new(ImmediateOutputRunner::new(Vec::new()))));
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root_for_proc(proc);
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    wait_for_file_contains(&shell, &format!("/proc/{pid}/status"), "exited").await;

    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let events =
        String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap()).unwrap();
    assert!(
        events.contains("status:exited"),
        "late-bound agent aggregate should replay existing proc lifecycle events: {events:?}"
    );
}

#[tokio::test]
async fn direct_proc_cancel_publishes_agent_lifecycle_events() {
    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let ctl_fid = Fid(9_274);
    proc.walk(Fid::ROOT, ctl_fid, &[pid.clone(), "ctl".to_string()])
        .await
        .unwrap();
    proc.open(ctl_fid, OpenMode::Write).await.unwrap();
    proc.write(ctl_fid, 0, b"cancel").await.unwrap();
    proc.clunk(ctl_fid).await.unwrap();
    wait_for_file_contains(&shell, &format!("/proc/{pid}/status"), "exited").await;

    let events =
        String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap()).unwrap();
    assert!(
        events.contains("status:exited"),
        "agent aggregate should publish proc cancel lifecycle events: {events:?}"
    );
}

#[tokio::test]
async fn bind_process_replays_existing_proc_io_events_in_order() {
    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();

    let output_fid = Fid(9_275);
    proc.walk(
        Fid::ROOT,
        output_fid,
        &[pid.clone(), "io".to_string(), "output".to_string()],
    )
    .await
    .unwrap();
    proc.open(output_fid, OpenMode::Write).await.unwrap();
    proc.write(output_fid, 0, b"early output").await.unwrap();
    proc.clunk(output_fid).await.unwrap();

    let input_fid = Fid(9_276);
    proc.walk(
        Fid::ROOT,
        input_fid,
        &[pid.clone(), "io".to_string(), "input".to_string()],
    )
    .await
    .unwrap();
    proc.open(input_fid, OpenMode::Write).await.unwrap();
    proc.write(input_fid, 0, b"early input").await.unwrap();
    proc.clunk(input_fid).await.unwrap();

    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let events =
        String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap()).unwrap();
    let io_records = events
        .lines()
        .filter(|line| line.starts_with("input:") || line.starts_with("output:"))
        .collect::<Vec<_>>();
    assert_eq!(
        io_records,
        vec!["output:12", "input:11"],
        "late-bound aggregate should preserve proc io/events order: {events:?}"
    );
}

#[tokio::test]
async fn failed_proc_overlay_walk_does_not_clunk_unbound_proc_fid() {
    let proc = Arc::new(ProcWalkCollisionFs::new());
    let proc_server: Arc<dyn FileServer> = proc.clone();
    let agent_root = AgentRootFs::new(proc_server);

    agent_root.bind_process("1", Arc::new(AgentFs::new())).await;

    assert_eq!(
        agent_root
            .walk(Fid::ROOT, Fid(9_300), &["1".into(), "status".into()])
            .await,
        Err(ErrorCode::BadRequest)
    );
    assert!(
        !proc.clunked_failed_fid(),
        "failed proc walks do not bind their newfid, so cleanup must not clunk it"
    );
}

#[tokio::test]
async fn failed_backing_walk_does_not_clunk_unbound_backing_fid() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let backing = Arc::new(FailedBackingWalkFs::new());
    agent_root.bind_process(pid.clone(), backing.clone()).await;

    assert_eq!(
        agent_root
            .walk(Fid::ROOT, Fid(9_350), &[pid, "file".to_string()])
            .await,
        Err(ErrorCode::BadRequest)
    );
    assert!(
        !backing.clunked_failed_fid(),
        "failed backing walks do not bind their newfid, so cleanup must not clunk it"
    );
}

#[tokio::test]
async fn failed_stat_after_delegated_walk_releases_backing_fid() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let backing = Arc::new(StatFailWalkFs::new());
    agent_root.bind_process(pid.clone(), backing.clone()).await;

    assert_eq!(
        agent_root
            .walk(Fid::ROOT, Fid(9_360), &[pid, "file".to_string()])
            .await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        backing.bound_fid_count(),
        0,
        "a failed outer walk must release the delegated backing fid"
    );
}

#[tokio::test]
async fn agent_children_are_derived_from_proc_parentage() {
    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let parent = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let spawner = proc.for_spawner(
        Some(Pid(parent.parse::<u64>().unwrap())),
        Namespace::new(),
        Credentials::user("alan"),
    );
    let child = spawn_on_proc(&spawner, Fid(10_000)).await;
    let unbound_child = spawn_on_proc(&spawner, Fid(10_001)).await;

    agent_root
        .bind_process(parent.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root
        .bind_process(child.clone(), Arc::new(AgentFs::new()))
        .await;

    let children = shell
        .ls(&format!("/agent/{parent}/children"))
        .await
        .unwrap();
    assert!(children.iter().any(|entry| entry == &child), "{children:?}");
    assert!(
        !children.iter().any(|entry| entry == &unbound_child),
        "{children:?}"
    );

    shell
        .write(
            &format!("/agent/{parent}/children/{child}/io/output"),
            b"hello child",
        )
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/{child}/io/output"))
                .await
                .unwrap()
        )
        .unwrap(),
        "hello child"
    );
}

#[tokio::test]
async fn child_registration_wakes_parent_events_stream() {
    use std::time::Duration;

    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let parent = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let spawner = proc.for_spawner(
        Some(Pid(parent.parse::<u64>().unwrap())),
        Namespace::new(),
        Credentials::user("alan"),
    );
    let parent_agent = Arc::new(AgentFs::new());
    agent_root
        .bind_process(parent.clone(), parent_agent.clone())
        .await;

    let events_fid = Fid(10_100);
    agent_root
        .walk(
            Fid::ROOT,
            events_fid,
            &[parent.clone(), "events".to_string()],
        )
        .await
        .unwrap();
    agent_root.open(events_fid, OpenMode::Read).await.unwrap();
    let events_offset = agent_root.stat(events_fid).await.unwrap().length;
    let reader_root = agent_root.clone();
    let reader =
        tokio::spawn(async move { reader_root.read(events_fid, events_offset, 4096).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        !reader.is_finished(),
        "events reader should block before the child is registered"
    );

    let child = spawn_on_proc(&spawner, Fid(10_101)).await;
    agent_root
        .bind_process(child.clone(), Arc::new(AgentFs::new()))
        .await;

    let record = tokio::time::timeout(Duration::from_millis(500), reader)
        .await
        .expect("child event should wake the parent events reader")
        .unwrap()
        .unwrap();
    let record = String::from_utf8(record).unwrap();
    assert!(
        record.contains(&format!("child:{child}")),
        "parent events should publish child registration: {record:?}"
    );
    agent_root.clunk(events_fid).await.unwrap();
}

#[tokio::test]
async fn agent_children_qid_versions_change_with_listing() {
    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let parent = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(parent.clone(), Arc::new(AgentFs::new()))
        .await;

    let empty_fid = Fid(11_000);
    agent_root
        .walk(
            Fid::ROOT,
            empty_fid,
            &[parent.clone(), "children".to_string()],
        )
        .await
        .unwrap();
    let empty_qid = agent_root.stat(empty_fid).await.unwrap().qid;
    agent_root.clunk(empty_fid).await.unwrap();

    let spawner = proc.for_spawner(
        Some(Pid(parent.parse::<u64>().unwrap())),
        Namespace::new(),
        Credentials::user("alan"),
    );
    let child = spawn_on_proc(&spawner, Fid(11_001)).await;
    agent_root
        .bind_process(child.clone(), Arc::new(AgentFs::new()))
        .await;

    let child_fid = Fid(11_002);
    agent_root
        .walk(Fid::ROOT, child_fid, &[parent, "children".to_string()])
        .await
        .unwrap();
    let child_qid = agent_root.stat(child_fid).await.unwrap().qid;
    agent_root.clunk(child_fid).await.unwrap();

    assert_eq!(empty_qid.kind, FileKind::Dir);
    assert_eq!(empty_qid.path, child_qid.path);
    assert_ne!(empty_qid.version, child_qid.version);
}

#[tokio::test]
async fn agent_root_namespaces_backing_qids_by_pid() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let first = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let second = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(first.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root
        .bind_process(second.clone(), Arc::new(AgentFs::new()))
        .await;

    let first_fid = Fid(12_000);
    let first_qid = agent_root
        .walk(
            Fid::ROOT,
            first_fid,
            &[first, "io".to_string(), "output".to_string()],
        )
        .await
        .unwrap();
    let first_stat_qid = agent_root.stat(first_fid).await.unwrap().qid;

    let second_fid = Fid(12_001);
    let second_qid = agent_root
        .walk(
            Fid::ROOT,
            second_fid,
            &[second, "io".to_string(), "output".to_string()],
        )
        .await
        .unwrap();
    let second_stat_qid = agent_root.stat(second_fid).await.unwrap().qid;
    agent_root.clunk(first_fid).await.unwrap();
    agent_root.clunk(second_fid).await.unwrap();

    assert_eq!(first_qid, first_stat_qid);
    assert_eq!(second_qid, second_stat_qid);
    assert_eq!(first_qid.kind, second_qid.kind);
    assert_ne!(first_qid.path, second_qid.path);
}

#[tokio::test]
async fn agent_root_rejects_creates_for_overlay_reserved_names() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(MemFs::new()))
        .await;

    let dir_fid = Fid(13_000);
    agent_root
        .walk(Fid::ROOT, dir_fid, std::slice::from_ref(&pid))
        .await
        .unwrap();
    for (idx, name) in ["children", "status", "ctl", "io"].into_iter().enumerate() {
        assert_eq!(
            agent_root
                .create(dir_fid, Fid(13_001 + idx as u64), name, FileKind::File)
                .await,
            Err(ErrorCode::BadRequest),
            "{name} should stay reserved for the overlay"
        );
    }
    agent_root.clunk(dir_fid).await.unwrap();
}

#[tokio::test]
async fn concurrent_walk_rechecks_newfid_before_insert() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let backing = Arc::new(RacingWalkFs::new());
    agent_root.bind_process(pid.clone(), backing.clone()).await;

    let shared_fid = Fid(14_000);
    let first = {
        let agent_root = agent_root.clone();
        let pid = pid.clone();
        tokio::spawn(async move {
            agent_root
                .walk(Fid::ROOT, shared_fid, &[pid, "file".to_string()])
                .await
        })
    };
    let second = {
        let agent_root = agent_root.clone();
        tokio::spawn(async move {
            agent_root
                .walk(Fid::ROOT, shared_fid, &[pid, "file".to_string()])
                .await
        })
    };
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    let ok_count = [first, second].into_iter().filter(Result::is_ok).count();
    let collision_count = [first, second]
        .into_iter()
        .filter(|result| matches!(result, Err(ErrorCode::BadRequest)))
        .count();

    assert_eq!(ok_count, 1);
    assert_eq!(collision_count, 1);
    assert_eq!(
        backing.bound_fid_count(),
        1,
        "the backing fid allocated by the losing walk should be clunked"
    );
    agent_root.clunk(shared_fid).await.unwrap();
    assert_eq!(backing.bound_fid_count(), 0);
}

#[tokio::test]
async fn failed_concurrent_walk_does_not_delete_winning_fid() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let backing = Arc::new(DelayedFailWalkFs::new());
    agent_root.bind_process(pid.clone(), backing.clone()).await;

    let shared_fid = Fid(15_000);
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    for _ in 0..2 {
        let agent_root = agent_root.clone();
        let pid = pid.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = agent_root
                .walk(Fid::ROOT, shared_fid, &[pid, "file".to_string()])
                .await;
            tx.send(result).await.unwrap();
        });
    }
    drop(tx);

    let winner = rx.recv().await.unwrap();
    assert!(winner.is_ok(), "one concurrent walk should bind the fid");
    backing.release_failure();
    let loser = rx.recv().await.unwrap();
    assert_eq!(loser, Err(ErrorCode::NotFound));

    agent_root
        .open(shared_fid, OpenMode::Read)
        .await
        .expect("failed concurrent walk must not remove the winning binding");
    agent_root.clunk(shared_fid).await.unwrap();
}

#[tokio::test]
async fn create_collision_rolls_back_the_losing_backing_file() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let backing = Arc::new(RacingCreateFs::new());
    agent_root.bind_process(pid.clone(), backing.clone()).await;

    let dir_fid = Fid(16_000);
    let shared_fid = Fid(16_001);
    agent_root
        .walk(Fid::ROOT, dir_fid, std::slice::from_ref(&pid))
        .await
        .unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    for name in ["alpha", "beta"] {
        let agent_root = agent_root.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = agent_root
                .create(dir_fid, shared_fid, name, FileKind::File)
                .await;
            tx.send((name.to_string(), result)).await.unwrap();
        });
    }
    drop(tx);

    let first = rx.recv().await.unwrap();
    let second = rx.recv().await.unwrap();
    let results = [first, second];
    let ok_names: BTreeSet<String> = results
        .iter()
        .filter(|(_, result)| result.is_ok())
        .map(|(name, _)| name.clone())
        .collect();
    let collision_count = results
        .iter()
        .filter(|(_, result)| matches!(result, Err(ErrorCode::BadRequest)))
        .count();

    assert_eq!(ok_names.len(), 1);
    assert_eq!(collision_count, 1);
    assert_eq!(
        backing.file_names(),
        ok_names,
        "a create that returns BadRequest must not leave a hidden backing file"
    );
    agent_root.clunk(shared_fid).await.unwrap();
    agent_root.clunk(dir_fid).await.unwrap();
}

#[tokio::test]
async fn agent_root_tracks_created_fids_forwarded_to_backing() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(MemFs::new()))
        .await;

    let dir_fid = Fid(20_000);
    let file_fid = Fid(20_001);
    agent_root
        .walk(Fid::ROOT, dir_fid, std::slice::from_ref(&pid))
        .await
        .unwrap();
    let qid = agent_root
        .create(dir_fid, file_fid, "facts", FileKind::File)
        .await
        .unwrap();
    assert_eq!(qid.kind, FileKind::File);
    agent_root.open(file_fid, OpenMode::Write).await.unwrap();
    agent_root.write(file_fid, 0, b"alpha").await.unwrap();
    agent_root.clunk(file_fid).await.unwrap();
    agent_root.clunk(dir_fid).await.unwrap();

    assert_eq!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/facts")).await.unwrap()).unwrap(),
        "alpha"
    );
}

#[tokio::test]
async fn agent_root_releases_outer_fid_after_delegated_remove() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(MemFs::new()))
        .await;

    let dir_fid = Fid(21_000);
    let file_fid = Fid(21_001);
    agent_root
        .walk(Fid::ROOT, dir_fid, std::slice::from_ref(&pid))
        .await
        .unwrap();
    agent_root
        .create(dir_fid, file_fid, "scratch", FileKind::File)
        .await
        .unwrap();
    agent_root.clunk(file_fid).await.unwrap();
    agent_root.clunk(dir_fid).await.unwrap();

    let remove_fid = Fid(21_002);
    agent_root
        .walk(Fid::ROOT, remove_fid, &[pid.clone(), "scratch".into()])
        .await
        .unwrap();
    agent_root.remove(remove_fid).await.unwrap();
    assert!(matches!(
        shell.cat(&format!("/agent/{pid}/scratch")).await,
        Err(ErrorCode::NotFound)
    ));

    agent_root
        .walk(Fid::ROOT, remove_fid, std::slice::from_ref(&pid))
        .await
        .expect("remove releases the outer fid for reuse");
}

struct ProcWalkCollisionFs {
    failed_fids: Mutex<Vec<Fid>>,
    clunked_fids: Mutex<Vec<Fid>>,
}

impl ProcWalkCollisionFs {
    fn new() -> Self {
        Self {
            failed_fids: Mutex::new(Vec::new()),
            clunked_fids: Mutex::new(Vec::new()),
        }
    }

    fn clunked_failed_fid(&self) -> bool {
        let failed = self
            .failed_fids
            .lock()
            .expect("failed fids lock should not be poisoned");
        let clunked = self
            .clunked_fids
            .lock()
            .expect("clunked fids lock should not be poisoned");
        failed.iter().any(|fid| clunked.contains(fid))
    }

    fn qid() -> Qid {
        Qid {
            kind: FileKind::Dir,
            version: 0,
            path: 0xC011_1510,
        }
    }
}

#[async_trait]
impl FileServer for ProcWalkCollisionFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT {
            return Err(ErrorCode::NotFound);
        }
        match names {
            [pid] if pid == "1" => Ok(Self::qid()),
            [pid, name] if pid == "1" && name == "status" => {
                self.failed_fids
                    .lock()
                    .expect("failed fids lock should not be poisoned")
                    .push(newfid);
                Err(ErrorCode::BadRequest)
            }
            [pid, name] if pid == "1" && name == "parent" => {
                self.failed_fids
                    .lock()
                    .expect("failed fids lock should not be poisoned")
                    .push(newfid);
                Err(ErrorCode::NotFound)
            }
            _ => Err(ErrorCode::NotFound),
        }
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Err(ErrorCode::Unsupported)
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

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.clunked_fids
            .lock()
            .expect("clunked fids lock should not be poisoned")
            .push(fid);
        Ok(())
    }
}

struct FailedBackingWalkFs {
    failed_fids: Mutex<Vec<Fid>>,
    clunked_fids: Mutex<Vec<Fid>>,
}

impl FailedBackingWalkFs {
    fn new() -> Self {
        Self {
            failed_fids: Mutex::new(Vec::new()),
            clunked_fids: Mutex::new(Vec::new()),
        }
    }

    fn clunked_failed_fid(&self) -> bool {
        let failed = self
            .failed_fids
            .lock()
            .expect("failed fids lock should not be poisoned");
        let clunked = self
            .clunked_fids
            .lock()
            .expect("clunked fids lock should not be poisoned");
        failed.iter().any(|fid| clunked.contains(fid))
    }
}

#[async_trait]
impl FileServer for FailedBackingWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT && names == ["file"] {
            self.failed_fids
                .lock()
                .expect("failed fids lock should not be poisoned")
                .push(newfid);
            Err(ErrorCode::BadRequest)
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Err(ErrorCode::Unsupported)
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

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.clunked_fids
            .lock()
            .expect("clunked fids lock should not be poisoned")
            .push(fid);
        Ok(())
    }
}

struct StatFailWalkFs {
    fids: Mutex<HashMap<Fid, ()>>,
}

impl StatFailWalkFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
        }
    }

    fn bound_fid_count(&self) -> usize {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .len()
    }

    fn qid() -> Qid {
        Qid {
            kind: FileKind::File,
            version: 0,
            path: 0x57A7_F411,
        }
    }
}

#[async_trait]
impl FileServer for StatFailWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names != ["file"] {
            return Err(ErrorCode::NotFound);
        }
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .insert(newfid, ());
        Ok(Self::qid())
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Err(ErrorCode::Io)
        } else {
            Err(ErrorCode::NotFound)
        }
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
        self.clunk(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid);
        Ok(())
    }
}

struct RacingWalkFs {
    fids: Mutex<HashMap<Fid, ()>>,
    started_walks: AtomicUsize,
    release_walks: Notify,
}

impl RacingWalkFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
            started_walks: AtomicUsize::new(0),
            release_walks: Notify::new(),
        }
    }

    fn bound_fid_count(&self) -> usize {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .len()
    }

    fn qid() -> Qid {
        Qid {
            kind: FileKind::File,
            version: 0,
            path: 0xF11E,
        }
    }
}

#[async_trait]
impl FileServer for RacingWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names.len() != 1 || names[0] != "file" {
            return Err(ErrorCode::NotFound);
        }
        let started = self.started_walks.fetch_add(1, Ordering::SeqCst) + 1;
        if started >= 2 {
            self.release_walks.notify_waiters();
        } else {
            self.release_walks.notified().await;
        }
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .insert(newfid, ());
        Ok(Self::qid())
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Ok(Self::qid())
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(Vec::new())
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Ok(Stat {
                name: "file".to_string(),
                qid: Self::qid(),
                length: 0,
                executable: false,
                writable: true,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
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
        self.clunk(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid);
        Ok(())
    }
}

struct DelayedFailWalkFs {
    fids: Mutex<HashMap<Fid, ()>>,
    started_walks: AtomicUsize,
    both_started: Notify,
    release_failed_walk: Notify,
}

impl DelayedFailWalkFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
            started_walks: AtomicUsize::new(0),
            both_started: Notify::new(),
            release_failed_walk: Notify::new(),
        }
    }

    fn release_failure(&self) {
        self.release_failed_walk.notify_waiters();
    }

    fn qid() -> Qid {
        Qid {
            kind: FileKind::File,
            version: 0,
            path: 0xF11E_0002,
        }
    }
}

#[async_trait]
impl FileServer for DelayedFailWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names.len() != 1 || names[0] != "file" {
            return Err(ErrorCode::NotFound);
        }
        let started = self.started_walks.fetch_add(1, Ordering::SeqCst) + 1;
        if started >= 2 {
            self.both_started.notify_waiters();
        }
        while self.started_walks.load(Ordering::SeqCst) < 2 {
            self.both_started.notified().await;
        }
        if started == 1 {
            self.fids
                .lock()
                .expect("fid map lock should not be poisoned")
                .insert(newfid, ());
            Ok(Self::qid())
        } else {
            self.release_failed_walk.notified().await;
            Err(ErrorCode::NotFound)
        }
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Ok(Self::qid())
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(Vec::new())
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Ok(Stat {
                name: "file".to_string(),
                qid: Self::qid(),
                length: 0,
                executable: false,
                writable: false,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
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
        self.clunk(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid);
        Ok(())
    }
}

struct RacingCreateFs {
    fids: Mutex<HashMap<Fid, String>>,
    files: Mutex<BTreeSet<String>>,
    started_creates: AtomicUsize,
    release_creates: Notify,
}

impl RacingCreateFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
            files: Mutex::new(BTreeSet::new()),
            started_creates: AtomicUsize::new(0),
            release_creates: Notify::new(),
        }
    }

    fn file_names(&self) -> BTreeSet<String> {
        self.files
            .lock()
            .expect("file set lock should not be poisoned")
            .clone()
    }

    fn qid(kind: FileKind) -> Qid {
        Qid {
            kind,
            version: 0,
            path: match kind {
                FileKind::Dir => 0x0C0D_ED1A,
                FileKind::File => 0xC0DE_F11E,
                FileKind::Stream => 0xC0DE_57EA,
                FileKind::Clone => 0xC0DE_C10E,
            },
        }
    }
}

#[async_trait]
impl FileServer for RacingCreateFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names.len() != 1 {
            return Err(ErrorCode::NotFound);
        }
        if self
            .files
            .lock()
            .expect("file set lock should not be poisoned")
            .contains(&names[0])
        {
            self.fids
                .lock()
                .expect("fid map lock should not be poisoned")
                .insert(newfid, names[0].clone());
            Ok(Self::qid(FileKind::File))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT
            || self
                .fids
                .lock()
                .expect("fid map lock should not be poisoned")
                .contains_key(&fid)
        {
            Ok(Self::qid(FileKind::File))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn read(&self, fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        if fid == Fid::ROOT {
            Ok(self
                .file_names()
                .into_iter()
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes())
        } else {
            Ok(Vec::new())
        }
    }

    async fn write(&self, _fid: Fid, _offset: u64, data: &[u8]) -> Result<u32, ErrorCode> {
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if fid == Fid::ROOT
            || self
                .fids
                .lock()
                .expect("fid map lock should not be poisoned")
                .contains_key(&fid)
        {
            Ok(Stat {
                name: String::new(),
                qid: Self::qid(if fid == Fid::ROOT {
                    FileKind::Dir
                } else {
                    FileKind::File
                }),
                length: 0,
                executable: false,
                writable: true,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || kind != FileKind::File {
            return Err(ErrorCode::BadRequest);
        }
        let started = self.started_creates.fetch_add(1, Ordering::SeqCst) + 1;
        if started >= 2 {
            self.release_creates.notify_waiters();
        }
        while self.started_creates.load(Ordering::SeqCst) < 2 {
            self.release_creates.notified().await;
        }
        self.files
            .lock()
            .expect("file set lock should not be poisoned")
            .insert(name.to_string());
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .insert(newfid, name.to_string());
        Ok(Self::qid(FileKind::File))
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let name = self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid)
            .ok_or(ErrorCode::NotFound)?;
        self.files
            .lock()
            .expect("file set lock should not be poisoned")
            .remove(&name);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid);
        Ok(())
    }
}
