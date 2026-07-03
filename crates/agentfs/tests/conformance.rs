use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use alan_agentfs::{AgentConformanceChecker, AgentFs, AgentRootFs};
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Qid, Stat};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use async_trait::async_trait;

fn namespace_with_agent_root() -> (InProcessTransport, Shell, Arc<AgentRootFs>) {
    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = proc.clone();
    let agent_root = Arc::new(AgentRootFs::new(proc_server));

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
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
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
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();

    let checker = AgentConformanceChecker::new(root);
    checker
        .check_generic_process(&format!("/proc/{pid}"))
        .await
        .assert_ok();
}

#[tokio::test]
async fn conformance_checker_verifies_dynamic_container_event_streams() {
    let (root, shell, agent_root) = namespace_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
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
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
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
