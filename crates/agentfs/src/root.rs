//! `/agent` root view.
//!
//! `AgentFs` serves one agent process's state tree. `AgentRootFs` is the
//! Plan-9-style view mounted at `/agent`: it lists only agent surfaces whose pid
//! still exists in `/proc`, resolves `/agent/root` to the configured root agent
//! pid, and forwards everything below `/agent/<pid>` to that pid's `AgentFs`.
//! It observes `/proc` through aP, keeping kernel internals and agent files
//! separate.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use async_trait::async_trait;
use tokio::sync::Mutex;

static NEXT_BACKING_FID: AtomicU64 = AtomicU64::new(1_000_000);
static NEXT_PROC_FID: AtomicU64 = AtomicU64::new(2_000_000);

#[derive(Clone)]
enum Node {
    Root,
    AgentRoot {
        pid: String,
        backing: Arc<dyn FileServer>,
    },
    AgentFile {
        backing: Arc<dyn FileServer>,
        backing_fid: Fid,
    },
    ProcFile {
        proc: Arc<dyn FileServer>,
        proc_fid: Fid,
    },
}

struct Entry {
    node: Node,
}

struct State {
    agents: HashMap<String, Arc<dyn FileServer>>,
    root_pid: Option<String>,
    fids: HashMap<Fid, Entry>,
}

/// The `/agent` root view.
///
/// The view is intentionally thin: it owns no process table and no agent state.
/// Process existence is checked by walking the configured `/proc` file server;
/// per-agent state is served by the registered backing `AgentFs` handles.
pub struct AgentRootFs {
    proc: Arc<dyn FileServer>,
    state: Mutex<State>,
}

impl AgentRootFs {
    pub fn new(proc: Arc<dyn FileServer>) -> Self {
        Self {
            proc,
            state: Mutex::new(State {
                agents: HashMap::new(),
                root_pid: None,
                fids: HashMap::new(),
            }),
        }
    }

    /// Register the agent-state backing tree for a committed process pid.
    pub async fn bind_process(&self, pid: impl Into<String>, agent: Arc<dyn FileServer>) {
        self.state.lock().await.agents.insert(pid.into(), agent);
    }

    /// Point `/agent/root` at the pid that embodies the Root Agent Process.
    pub async fn set_root_process(&self, pid: impl Into<String>) {
        self.state.lock().await.root_pid = Some(pid.into());
    }

    async fn entry_for_name(&self, name: &str) -> Result<(String, Arc<dyn FileServer>), ErrorCode> {
        let (pid, backing) = {
            let state = self.state.lock().await;
            let pid = if name == "root" {
                state.root_pid.clone().ok_or(ErrorCode::NotFound)?
            } else {
                name.to_string()
            };
            let backing = state.agents.get(&pid).cloned().ok_or(ErrorCode::NotFound)?;
            (pid, backing)
        };
        if self.proc_has_pid(&pid).await? {
            Ok((pid, backing))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn proc_has_pid(&self, pid: &str) -> Result<bool, ErrorCode> {
        let fid = Fid(NEXT_PROC_FID.fetch_add(1, Ordering::Relaxed));
        match self.proc.walk(Fid::ROOT, fid, &[pid.to_string()]).await {
            Ok(_) => {
                let _ = self.proc.clunk(fid).await;
                Ok(true)
            }
            Err(ErrorCode::NotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn proc_pids(&self) -> Result<Vec<String>, ErrorCode> {
        let length = self.proc.stat(Fid::ROOT).await?.length;
        let count = u32::try_from(length.saturating_add(1)).unwrap_or(u32::MAX);
        let bytes = self.proc.read(Fid::ROOT, 0, count).await?;
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::Io)?;
        Ok(text
            .lines()
            .filter(|line| !line.is_empty() && *line != "clone")
            .map(str::to_string)
            .collect())
    }

    async fn root_listing(&self) -> Result<Vec<String>, ErrorCode> {
        let (registered, root_pid) = {
            let state = self.state.lock().await;
            (
                state.agents.keys().cloned().collect::<Vec<_>>(),
                state.root_pid.clone(),
            )
        };
        let proc_pids = self.proc_pids().await?;
        let mut names = Vec::new();
        for pid in proc_pids {
            if registered.iter().any(|registered| registered == &pid) {
                names.push(pid);
            }
        }
        if let Some(root_pid) = root_pid
            && names.iter().any(|name| name == &root_pid)
        {
            names.push("root".to_string());
        }
        Ok(names)
    }

    fn node_of(state: &State, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        state
            .fids
            .get(&fid)
            .map(|entry| entry.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    async fn bind_agent_walk(
        &self,
        newfid: Fid,
        pid: String,
        backing: Arc<dyn FileServer>,
        base_fid: Fid,
        names: &[String],
    ) -> Result<Node, ErrorCode> {
        if names.is_empty() && base_fid == Fid::ROOT {
            return Ok(Node::AgentRoot { pid, backing });
        }
        if base_fid == Fid::ROOT && names.first().is_some_and(|name| is_proc_overlay_name(name)) {
            return self.bind_proc_walk(newfid, &pid, names).await;
        }
        let backing_fid = Fid(NEXT_BACKING_FID.fetch_add(1, Ordering::Relaxed));
        match backing.walk(base_fid, backing_fid, names).await {
            Ok(_) => Ok(Node::AgentFile {
                backing,
                backing_fid,
            }),
            Err(e) => {
                let _ = backing.clunk(backing_fid).await;
                let mut state = self.state.lock().await;
                state.fids.remove(&newfid);
                Err(e)
            }
        }
    }

    async fn bind_proc_walk(
        &self,
        newfid: Fid,
        pid: &str,
        names: &[String],
    ) -> Result<Node, ErrorCode> {
        let proc_fid = Fid(NEXT_PROC_FID.fetch_add(1, Ordering::Relaxed));
        let mut proc_names = Vec::with_capacity(names.len() + 1);
        proc_names.push(pid.to_string());
        proc_names.extend_from_slice(names);
        match self.proc.walk(Fid::ROOT, proc_fid, &proc_names).await {
            Ok(_) => Ok(Node::ProcFile {
                proc: self.proc.clone(),
                proc_fid,
            }),
            Err(e) => {
                let _ = self.proc.clunk(proc_fid).await;
                let mut state = self.state.lock().await;
                state.fids.remove(&newfid);
                Err(e)
            }
        }
    }

    async fn agent_listing(
        &self,
        pid: &str,
        backing: Arc<dyn FileServer>,
    ) -> Result<Vec<String>, ErrorCode> {
        let mut names = read_listing(backing, [].as_slice()).await?;
        let proc_names = read_listing(self.proc.clone(), &[pid.to_string()]).await?;
        for name in proc_names {
            if !names.iter().any(|existing| existing == &name) {
                names.push(name);
            }
        }
        Ok(names)
    }
}

#[async_trait]
impl FileServer for AgentRootFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        {
            let state = self.state.lock().await;
            if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
        }

        let node = {
            let state = self.state.lock().await;
            Self::node_of(&state, fid)?
        };
        let new_node = match node {
            Node::Root => {
                if names.is_empty() {
                    Node::Root
                } else {
                    let (pid, backing) = self.entry_for_name(&names[0]).await?;
                    self.bind_agent_walk(newfid, pid, backing, Fid::ROOT, &names[1..])
                        .await?
                }
            }
            Node::AgentRoot { pid, backing } => {
                self.bind_agent_walk(newfid, pid, backing, Fid::ROOT, names)
                    .await?
            }
            Node::AgentFile {
                backing,
                backing_fid,
            } => {
                self.bind_agent_walk(newfid, String::new(), backing, backing_fid, names)
                    .await?
            }
            Node::ProcFile { .. } => return Err(ErrorCode::NotDirectory),
        };
        let qid = qid_for_node(&new_node).await?;
        self.state
            .lock()
            .await
            .fids
            .insert(newfid, Entry { node: new_node });
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        match self.node(fid).await? {
            Node::Root => {
                if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
                    return Err(ErrorCode::NoAccess);
                }
                Ok(root_qid())
            }
            Node::AgentRoot { backing, .. } => backing.open(Fid::ROOT, mode).await,
            Node::AgentFile {
                backing,
                backing_fid,
                ..
            } => backing.open(backing_fid, mode).await,
            Node::ProcFile { proc, proc_fid } => proc.open(proc_fid, mode).await,
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        match self.node(fid).await? {
            Node::Root => Ok(slice(
                self.root_listing().await?.join("\n").into_bytes(),
                offset,
                count,
            )),
            Node::AgentRoot { pid, backing } => Ok(slice(
                self.agent_listing(&pid, backing)
                    .await?
                    .join("\n")
                    .into_bytes(),
                offset,
                count,
            )),
            Node::AgentFile {
                backing,
                backing_fid,
                ..
            } => backing.read(backing_fid, offset, count).await,
            Node::ProcFile { proc, proc_fid } => proc.read(proc_fid, offset, count).await,
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        match self.node(fid).await? {
            Node::Root => Err(ErrorCode::NoAccess),
            Node::AgentRoot { backing, .. } => backing.write(Fid::ROOT, offset, data).await,
            Node::AgentFile {
                backing,
                backing_fid,
                ..
            } => backing.write(backing_fid, offset, data).await,
            Node::ProcFile { proc, proc_fid } => proc.write(proc_fid, offset, data).await,
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        match self.node(fid).await? {
            Node::Root => Ok(Stat {
                name: String::new(),
                qid: root_qid(),
                length: self.root_listing().await?.join("\n").len() as u64,
                writable: false,
            }),
            Node::AgentRoot { pid, backing } => {
                let mut stat = backing.stat(Fid::ROOT).await?;
                stat.length = self.agent_listing(&pid, backing).await?.join("\n").len() as u64;
                Ok(stat)
            }
            Node::AgentFile {
                backing,
                backing_fid,
                ..
            } => backing.stat(backing_fid).await,
            Node::ProcFile { proc, proc_fid } => proc.stat(proc_fid).await,
        }
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        match self.node(fid).await? {
            Node::Root => Err(ErrorCode::Unsupported),
            Node::AgentRoot { backing, .. } => backing.create(Fid::ROOT, newfid, name, kind).await,
            Node::AgentFile {
                backing,
                backing_fid,
                ..
            } => backing.create(backing_fid, newfid, name, kind).await,
            Node::ProcFile { proc, proc_fid } => proc.create(proc_fid, newfid, name, kind).await,
        }
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        match self.node(fid).await? {
            Node::Root => Err(ErrorCode::Unsupported),
            Node::AgentRoot { backing, .. } => backing.remove(Fid::ROOT).await,
            Node::AgentFile {
                backing,
                backing_fid,
                ..
            } => backing.remove(backing_fid).await,
            Node::ProcFile { proc, proc_fid } => proc.remove(proc_fid).await,
        }
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let entry = self.state.lock().await.fids.remove(&fid);
        match entry.map(|entry| entry.node) {
            Some(Node::AgentFile {
                backing,
                backing_fid,
                ..
            }) => backing.clunk(backing_fid).await,
            Some(Node::ProcFile { proc, proc_fid }) => proc.clunk(proc_fid).await,
            Some(Node::Root | Node::AgentRoot { .. }) => Ok(()),
            None => Err(ErrorCode::NotFound),
        }
    }
}

impl AgentRootFs {
    async fn node(&self, fid: Fid) -> Result<Node, ErrorCode> {
        let state = self.state.lock().await;
        Self::node_of(&state, fid)
    }
}

async fn qid_for_node(node: &Node) -> Result<Qid, ErrorCode> {
    match node {
        Node::Root => Ok(root_qid()),
        Node::AgentRoot { backing, .. } => backing.stat(Fid::ROOT).await.map(|stat| stat.qid),
        Node::AgentFile {
            backing,
            backing_fid,
            ..
        } => backing.stat(*backing_fid).await.map(|stat| stat.qid),
        Node::ProcFile { proc, proc_fid } => proc.stat(*proc_fid).await.map(|stat| stat.qid),
    }
}

async fn read_listing(
    server: Arc<dyn FileServer>,
    names: &[String],
) -> Result<Vec<String>, ErrorCode> {
    let fid = Fid(NEXT_BACKING_FID.fetch_add(1, Ordering::Relaxed));
    server.walk(Fid::ROOT, fid, names).await?;
    server.open(fid, OpenMode::Read).await?;
    let length = server.stat(fid).await?.length;
    let bytes = server
        .read(
            fid,
            0,
            u32::try_from(length.saturating_add(1)).unwrap_or(u32::MAX),
        )
        .await;
    let clunk = server.clunk(fid).await;
    let bytes = bytes?;
    clunk?;
    let text = String::from_utf8(bytes).map_err(|_| ErrorCode::Io)?;
    Ok(text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn is_proc_overlay_name(name: &str) -> bool {
    matches!(
        name,
        "status" | "parent" | "credentials" | "exit" | "ctl" | "namespace"
    )
}

fn root_qid() -> Qid {
    Qid {
        kind: FileKind::Dir,
        version: 0,
        path: 0xA6E7,
    }
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
