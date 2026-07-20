use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use alan_agentfs::{AgentConformanceChecker, AgentFs, AgentRootFs};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, ProcessEventSource, Qid,
    Stat,
};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use async_trait::async_trait;

fn namespace_with_agent_root() -> (InProcessTransport, Shell, Arc<AgentRootFs>) {
    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = proc.clone();
    let proc_events: Arc<dyn ProcessEventSource> = proc.clone();
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        proc_server,
        proc_events,
    ));

    let mut namespace = Namespace::new();
    namespace.mount("/proc", InProcessTransport::new(proc), Access::ReadWrite);
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );

    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    (root.clone(), Shell::new(root), agent_root)
}

#[tokio::test]
async fn conformance_checker_accepts_agent_overlay_process_layout() {
    let (root, shell, agent_root) = namespace_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;

    let checker = AgentConformanceChecker::new(root);
    checker
        .check_agent_process(&format!("/agent/{pid}"))
        .await
        .assert_ok();
}

#[tokio::test]
async fn conformance_checker_accepts_procfs_generic_process_layout() {
    let (root, shell, _) = namespace_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"mounts":[]}}"#)
        .await
        .unwrap();

    let checker = AgentConformanceChecker::new(root);
    checker
        .check_generic_process(&format!("/proc/{pid}"))
        .await
        .assert_ok();
}

#[tokio::test]
async fn conformance_checker_rejects_generic_process_missing_input_and_events() {
    let checker = AgentConformanceChecker::new(InProcessTransport::new(Arc::new(
        IncompleteGenericProcessFs::new(),
    )));

    let report = checker.check_generic_process("/proc/1").await;
    let paths = report
        .issues
        .iter()
        .map(|issue| issue.path.as_str())
        .collect::<Vec<_>>();

    assert!(
        paths.contains(&"/proc/1/io/input"),
        "generic process checks must require io/input: {report:?}"
    );
    assert!(
        paths.contains(&"/proc/1/io/events"),
        "generic process checks must require io/events: {report:?}"
    );
    assert!(
        !paths.contains(&"/proc/1/io/output"),
        "the incomplete fixture still provides io/output: {report:?}"
    );
}

#[tokio::test]
async fn conformance_checker_requires_current_checkpoint_file() {
    let checker = AgentConformanceChecker::new(InProcessTransport::new(Arc::new(
        MissingCheckpointAgentFs::new(),
    )));

    let report = checker.check_agent_process("/agent/1").await;
    let paths = report
        .issues
        .iter()
        .map(|issue| issue.path.as_str())
        .collect::<Vec<_>>();

    assert!(
        paths.contains(&"/agent/1/machine/checkpoints/current"),
        "agent conformance checks must require machine/checkpoints/current: {report:?}"
    );
}

#[tokio::test]
async fn conformance_checker_verifies_dynamic_container_event_streams() {
    let (root, shell, agent_root) = namespace_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let checker = AgentConformanceChecker::new(root);
    checker
        .check_dynamic_container_events(&format!("/agent/{pid}"))
        .await
        .assert_ok();
}

#[tokio::test]
async fn conformance_checker_verifies_root_alias_matches_current_pid() {
    let (root, shell, agent_root) = namespace_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;

    let checker = AgentConformanceChecker::new(root);
    checker.check_root_alias("/agent", &pid).await.assert_ok();
}

#[tokio::test]
async fn conformance_checker_aborts_timed_out_event_readers() {
    let active_event_reads = Arc::new(AtomicUsize::new(0));
    let fs = Arc::new(HangingContainerEventFs::new(active_event_reads.clone()));
    let checker = AgentConformanceChecker::new(InProcessTransport::new(fs));

    let report = checker.check_dynamic_container_events("/agent/1").await;

    assert!(
        !report.is_ok(),
        "hanging event streams should report issues"
    );
    assert_eq!(
        active_event_reads.load(Ordering::SeqCst),
        0,
        "timed-out event read tasks should be aborted before returning"
    );
}

struct MissingCheckpointAgentFs {
    inner: AgentFs,
}

impl MissingCheckpointAgentFs {
    fn new() -> Self {
        Self {
            inner: AgentFs::new(),
        }
    }

    fn forwarded_names<'a>(
        &self,
        fid: Fid,
        names: &'a [String],
    ) -> Result<&'a [String], ErrorCode> {
        if fid != Fid::ROOT {
            return Ok(names);
        }
        match names {
            [agent, pid, rest @ ..] if agent == "agent" && pid == "1" => Ok(rest),
            _ => Err(ErrorCode::NotFound),
        }
    }

    fn hides_current_checkpoint(names: &[String]) -> bool {
        matches!(
            names,
            [machine, checkpoints, current]
                if machine == "machine" && checkpoints == "checkpoints" && current == "current"
        )
    }
}

#[async_trait]
impl FileServer for MissingCheckpointAgentFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let names = self.forwarded_names(fid, names)?;
        if Self::hides_current_checkpoint(names) {
            return Err(ErrorCode::NotFound);
        }
        self.inner.walk(fid, newfid, names).await
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }

    async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }

    async fn write(&self, fid: Fid, offset: u64, data: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data).await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.inner.stat(fid).await
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.clunk(fid).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HangingNode {
    EventStream,
    CloneFile,
}

struct HangingContainerEventFs {
    fids: Mutex<HashMap<Fid, HangingNode>>,
    active_event_reads: Arc<AtomicUsize>,
}

impl HangingContainerEventFs {
    fn new(active_event_reads: Arc<AtomicUsize>) -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
            active_event_reads,
        }
    }

    fn node_for(&self, fid: Fid) -> Result<HangingNode, ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .get(&fid)
            .copied()
            .ok_or(ErrorCode::NotFound)
    }
}

#[async_trait]
impl FileServer for HangingContainerEventFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names.len() != 4 || names[0] != "agent" || names[1] != "1" {
            return Err(ErrorCode::NotFound);
        }
        let container = names[2].as_str();
        if container != "requests" && container != "actions" {
            return Err(ErrorCode::NotFound);
        }
        let node = match names[3].as_str() {
            "events" => HangingNode::EventStream,
            "clone" => HangingNode::CloneFile,
            _ => return Err(ErrorCode::NotFound),
        };
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .insert(newfid, node);
        Ok(qid_for_hanging_node(node))
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.node_for(fid).map(qid_for_hanging_node)
    }

    async fn read(&self, fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        match self.node_for(fid)? {
            HangingNode::CloneFile => Ok(b"1\n".to_vec()),
            HangingNode::EventStream => {
                let _guard = ActiveEventReadGuard::new(self.active_event_reads.clone());
                std::future::pending::<()>().await;
                unreachable!("pending event read should be cancelled by the checker")
            }
        }
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_for(fid)?;
        let name = match node {
            HangingNode::EventStream => "events",
            HangingNode::CloneFile => "clone",
        };
        Ok(Stat {
            name: name.to_string(),
            qid: qid_for_hanging_node(node),
            length: 0,
            executable: false,
            writable: true,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IncompleteGenericNode {
    Proc,
    Io,
    Output,
    Status,
    Ctl,
}

struct IncompleteGenericProcessFs {
    fids: Mutex<HashMap<Fid, IncompleteGenericNode>>,
}

impl IncompleteGenericProcessFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
        }
    }

    fn node_for(&self, fid: Fid) -> Result<IncompleteGenericNode, ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .get(&fid)
            .copied()
            .ok_or(ErrorCode::NotFound)
    }
}

#[async_trait]
impl FileServer for IncompleteGenericProcessFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT {
            return Err(ErrorCode::NotFound);
        }
        let node = match names {
            [proc, pid] if proc == "proc" && pid == "1" => IncompleteGenericNode::Proc,
            [proc, pid, io] if proc == "proc" && pid == "1" && io == "io" => {
                IncompleteGenericNode::Io
            }
            [proc, pid, io, output]
                if proc == "proc" && pid == "1" && io == "io" && output == "output" =>
            {
                IncompleteGenericNode::Output
            }
            [proc, pid, status] if proc == "proc" && pid == "1" && status == "status" => {
                IncompleteGenericNode::Status
            }
            [proc, pid, ctl] if proc == "proc" && pid == "1" && ctl == "ctl" => {
                IncompleteGenericNode::Ctl
            }
            _ => return Err(ErrorCode::NotFound),
        };
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .insert(newfid, node);
        Ok(qid_for_incomplete_generic_node(node))
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.node_for(fid).map(qid_for_incomplete_generic_node)
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_for(fid)?;
        Ok(Stat {
            name: String::new(),
            qid: qid_for_incomplete_generic_node(node),
            length: 0,
            executable: false,
            writable: matches!(node, IncompleteGenericNode::Ctl),
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

struct ActiveEventReadGuard {
    active_event_reads: Arc<AtomicUsize>,
}

impl Drop for ActiveEventReadGuard {
    fn drop(&mut self) {
        self.active_event_reads.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ActiveEventReadGuard {
    fn new(active_event_reads: Arc<AtomicUsize>) -> Self {
        active_event_reads.fetch_add(1, Ordering::SeqCst);
        Self { active_event_reads }
    }
}

fn qid_for_hanging_node(node: HangingNode) -> Qid {
    let kind = match node {
        HangingNode::EventStream => FileKind::Stream,
        HangingNode::CloneFile => FileKind::Clone,
    };
    let path = match node {
        HangingNode::EventStream => 1,
        HangingNode::CloneFile => 2,
    };
    Qid {
        kind,
        version: 0,
        path,
    }
}

fn qid_for_incomplete_generic_node(node: IncompleteGenericNode) -> Qid {
    let (kind, path) = match node {
        IncompleteGenericNode::Proc => (FileKind::Dir, 100),
        IncompleteGenericNode::Io => (FileKind::Dir, 101),
        IncompleteGenericNode::Output => (FileKind::Stream, 102),
        IncompleteGenericNode::Status => (FileKind::File, 103),
        IncompleteGenericNode::Ctl => (FileKind::File, 104),
    };
    Qid {
        kind,
        version: 0,
        path,
    }
}
