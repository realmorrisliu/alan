//! `/agent` root view.
//!
//! `AgentFs` serves one agent process's state tree. `AgentRootFs` is the
//! Plan-9-style view mounted at `/agent`: it lists only agent surfaces whose pid
//! still exists in `/proc`, resolves `/agent/root` to the configured root agent
//! pid, and forwards everything below `/agent/<pid>` to that pid's `AgentFs`.
//! It observes `/proc` through aP, keeping kernel internals and agent files
//! separate.

use std::{
    any::Any,
    collections::HashMap,
    collections::HashSet,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, ProcessEvent, ProcessEventSink,
    ProcessEventSource, ProcessInputEventSink, ProcessInputEventSource, ProcessIoEventKind,
    ProcessIoEventSink, ProcessIoEventSource, ProcessOutputEventSink, ProcessOutputEventSource,
    Qid, Stat,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::AgentFs;

static NEXT_BACKING_FID: AtomicU64 = AtomicU64::new(1_000_000);
static NEXT_PROC_FID: AtomicU64 = AtomicU64::new(2_000_000);

#[derive(Clone)]
struct AgentRegistration {
    backing: Arc<dyn FileServer>,
    event_sink: Option<Arc<AgentFs>>,
}

#[derive(Clone)]
enum Node {
    Root,
    AgentRoot {
        pid: String,
        backing: Arc<dyn FileServer>,
    },
    AgentChildren {
        pid: String,
    },
    AgentFile {
        pid: String,
        backing: Arc<dyn FileServer>,
        backing_fid: Fid,
    },
    ProcFile {
        proc: Arc<dyn FileServer>,
        proc_fid: Fid,
        pid: String,
        names: Vec<String>,
    },
}

struct Entry {
    node: Node,
}

struct ProcCreateDir {
    proc: Arc<dyn FileServer>,
    dir_fid: Fid,
    pid: String,
    names: Vec<String>,
}

struct State {
    agents: HashMap<String, AgentRegistration>,
    process_event_pids: HashSet<String>,
    io_event_pids: HashSet<String>,
    input_event_pids: HashSet<String>,
    output_event_pids: HashSet<String>,
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
    process_events: Option<Arc<dyn ProcessEventSource>>,
    io_events: Option<Arc<dyn ProcessIoEventSource>>,
    input_events: Option<Arc<dyn ProcessInputEventSource>>,
    output_events: Option<Arc<dyn ProcessOutputEventSource>>,
    state: Arc<Mutex<State>>,
}

impl AgentRootFs {
    pub fn new(proc: Arc<dyn FileServer>) -> Self {
        Self {
            proc,
            process_events: None,
            io_events: None,
            input_events: None,
            output_events: None,
            state: Arc::new(Mutex::new(State {
                agents: HashMap::new(),
                process_event_pids: HashSet::new(),
                io_event_pids: HashSet::new(),
                input_event_pids: HashSet::new(),
                output_event_pids: HashSet::new(),
                root_pid: None,
                fids: HashMap::new(),
            })),
        }
    }

    pub fn new_with_process_output_events(
        proc: Arc<dyn FileServer>,
        output_events: Arc<dyn ProcessOutputEventSource>,
    ) -> Self {
        Self {
            proc,
            process_events: None,
            io_events: None,
            input_events: None,
            output_events: Some(output_events),
            state: Arc::new(Mutex::new(State {
                agents: HashMap::new(),
                process_event_pids: HashSet::new(),
                io_event_pids: HashSet::new(),
                input_event_pids: HashSet::new(),
                output_event_pids: HashSet::new(),
                root_pid: None,
                fids: HashMap::new(),
            })),
        }
    }

    pub fn new_with_process_io_events(
        proc: Arc<dyn FileServer>,
        input_events: Arc<dyn ProcessInputEventSource>,
        output_events: Arc<dyn ProcessOutputEventSource>,
    ) -> Self {
        Self {
            proc,
            process_events: None,
            io_events: None,
            input_events: Some(input_events),
            output_events: Some(output_events),
            state: Arc::new(Mutex::new(State {
                agents: HashMap::new(),
                process_event_pids: HashSet::new(),
                io_event_pids: HashSet::new(),
                input_event_pids: HashSet::new(),
                output_event_pids: HashSet::new(),
                root_pid: None,
                fids: HashMap::new(),
            })),
        }
    }

    pub fn new_with_ordered_process_io_events(
        proc: Arc<dyn FileServer>,
        io_events: Arc<dyn ProcessIoEventSource>,
    ) -> Self {
        Self {
            proc,
            process_events: None,
            io_events: Some(io_events),
            input_events: None,
            output_events: None,
            state: Arc::new(Mutex::new(State {
                agents: HashMap::new(),
                process_event_pids: HashSet::new(),
                io_event_pids: HashSet::new(),
                input_event_pids: HashSet::new(),
                output_event_pids: HashSet::new(),
                root_pid: None,
                fids: HashMap::new(),
            })),
        }
    }

    pub fn new_with_process_events(
        proc: Arc<dyn FileServer>,
        process_events: Arc<dyn ProcessEventSource>,
    ) -> Self {
        Self {
            proc,
            process_events: Some(process_events),
            io_events: None,
            input_events: None,
            output_events: None,
            state: Arc::new(Mutex::new(State {
                agents: HashMap::new(),
                process_event_pids: HashSet::new(),
                io_event_pids: HashSet::new(),
                input_event_pids: HashSet::new(),
                output_event_pids: HashSet::new(),
                root_pid: None,
                fids: HashMap::new(),
            })),
        }
    }

    /// Register the agent-state backing tree for a committed process pid.
    pub async fn bind_process<T>(&self, pid: impl Into<String>, agent: Arc<T>)
    where
        T: FileServer + Any + 'static,
    {
        let pid = pid.into();
        let event_sink = agent_event_sink(&agent);
        let has_event_sink = event_sink.is_some();
        let backing: Arc<dyn FileServer> = agent;
        let parent_pid = self.proc_parent_of(&pid).await.ok().flatten();
        let (
            parent_events,
            subscribe_process_events,
            subscribe_io_events,
            subscribe_input_events,
            subscribe_output_events,
        ) = {
            let mut state = self.state.lock().await;
            state.agents.insert(
                pid.clone(),
                AgentRegistration {
                    backing,
                    event_sink,
                },
            );
            let parent_events = parent_pid.and_then(|parent_pid| {
                state
                    .agents
                    .get(&parent_pid)
                    .and_then(|agent| agent.event_sink.clone())
            });
            let subscribe_process_events = has_event_sink
                && self.process_events.is_some()
                && state.process_event_pids.insert(pid.clone());
            let subscribe_io_events = has_event_sink
                && self.process_events.is_none()
                && self.io_events.is_some()
                && state.io_event_pids.insert(pid.clone());
            let subscribe_input_events = has_event_sink
                && self.process_events.is_none()
                && self.io_events.is_none()
                && self.input_events.is_some()
                && state.input_event_pids.insert(pid.clone());
            let subscribe_output_events = has_event_sink
                && self.process_events.is_none()
                && self.io_events.is_none()
                && self.output_events.is_some()
                && state.output_event_pids.insert(pid.clone());
            (
                parent_events,
                subscribe_process_events,
                subscribe_io_events,
                subscribe_input_events,
                subscribe_output_events,
            )
        };
        if let Some(parent) = parent_events {
            parent.append_child_event(&pid).await;
        }
        if subscribe_process_events && let Some(process_events) = self.process_events.clone() {
            let sink = Arc::new(AgentProcessEventSink {
                state: self.state.clone(),
            });
            if process_events
                .subscribe_process_events(&pid, sink)
                .await
                .is_err()
            {
                self.state.lock().await.process_event_pids.remove(&pid);
            }
        }
        if subscribe_io_events && let Some(io_events) = self.io_events.clone() {
            let sink = Arc::new(AgentIoEventSink {
                state: self.state.clone(),
            });
            if io_events.subscribe_process_io(&pid, sink).await.is_err() {
                self.state.lock().await.io_event_pids.remove(&pid);
            }
        }
        if subscribe_input_events && let Some(input_events) = self.input_events.clone() {
            let sink = Arc::new(AgentInputEventSink {
                state: self.state.clone(),
            });
            if input_events
                .subscribe_process_input(&pid, sink)
                .await
                .is_err()
            {
                self.state.lock().await.input_event_pids.remove(&pid);
            }
        }
        if subscribe_output_events && let Some(output_events) = self.output_events.clone() {
            let sink = Arc::new(AgentOutputEventSink {
                state: self.state.clone(),
            });
            if output_events
                .subscribe_process_output(&pid, sink)
                .await
                .is_err()
            {
                self.state.lock().await.output_event_pids.remove(&pid);
            }
        }
    }

    /// Return a fresh attachment handle for the AgentFS tree bound to `pid`.
    pub async fn process_tree(&self, pid: &str) -> Option<Arc<dyn FileServer>> {
        self.state
            .lock()
            .await
            .agents
            .get(pid)
            .map(|registration| registration.backing.clone())
    }

    /// Remove the agent-state backing tree for a process that failed to launch.
    pub async fn unbind_process(&self, pid: &str) -> bool {
        let mut state = self.state.lock().await;
        state.process_event_pids.remove(pid);
        state.io_event_pids.remove(pid);
        state.input_event_pids.remove(pid);
        state.output_event_pids.remove(pid);
        if state.root_pid.as_deref() == Some(pid) {
            state.root_pid = None;
        }
        state.agents.remove(pid).is_some()
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
            let backing = state
                .agents
                .get(&pid)
                .map(|agent| agent.backing.clone())
                .ok_or(ErrorCode::NotFound)?;
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
        if base_fid == Fid::ROOT {
            if names.first().is_some_and(|name| name == "children") {
                return self.bind_agent_child_walk(newfid, &pid, &names[1..]).await;
            }
            if names.first().is_some_and(|name| is_proc_overlay_name(name)) {
                return self.bind_proc_walk(newfid, &pid, names).await;
            }
        }
        let backing_fid = Fid(NEXT_BACKING_FID.fetch_add(1, Ordering::Relaxed));
        match backing.walk(base_fid, backing_fid, names).await {
            Ok(_) => Ok(Node::AgentFile {
                pid,
                backing,
                backing_fid,
            }),
            Err(e) => Err(e),
        }
    }

    async fn bind_agent_child_walk(
        &self,
        newfid: Fid,
        parent_pid: &str,
        names: &[String],
    ) -> Result<Node, ErrorCode> {
        if names.is_empty() {
            return Ok(Node::AgentChildren {
                pid: parent_pid.to_string(),
            });
        }
        let child_pid = &names[0];
        if child_pid == "root" {
            return Err(ErrorCode::NotFound);
        }
        let (pid, backing) = self.entry_for_name(child_pid).await?;
        if !self.proc_parent_matches(&pid, parent_pid).await? {
            return Err(ErrorCode::NotFound);
        }
        Box::pin(self.bind_agent_walk(newfid, pid, backing, Fid::ROOT, &names[1..])).await
    }

    async fn bind_proc_walk(
        &self,
        _newfid: Fid,
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
                pid: pid.to_string(),
                names: names.to_vec(),
            }),
            Err(e) => Err(e),
        }
    }

    async fn bind_proc_relative_walk(
        &self,
        proc: Arc<dyn FileServer>,
        base_fid: Fid,
        pid: String,
        base_names: Vec<String>,
        names: &[String],
    ) -> Result<Node, ErrorCode> {
        let proc_fid = Fid(NEXT_PROC_FID.fetch_add(1, Ordering::Relaxed));
        match proc.walk(base_fid, proc_fid, names).await {
            Ok(_) => {
                let mut child_names = base_names;
                child_names.extend_from_slice(names);
                Ok(Node::ProcFile {
                    proc,
                    proc_fid,
                    pid,
                    names: child_names,
                })
            }
            Err(e) => Err(e),
        }
    }

    async fn proc_parent_matches(&self, pid: &str, parent_pid: &str) -> Result<bool, ErrorCode> {
        Ok(self
            .proc_parent_of(pid)
            .await?
            .is_some_and(|parent| parent == parent_pid))
    }

    async fn proc_parent_of(&self, pid: &str) -> Result<Option<String>, ErrorCode> {
        let names = [pid.to_string(), "parent".to_string()];
        match read_file_text(
            self.proc.clone(),
            &names,
            NEXT_PROC_FID.fetch_add(1, Ordering::Relaxed),
        )
        .await
        {
            Ok(parent) if parent.is_empty() => Ok(None),
            Ok(parent) => Ok(Some(parent)),
            Err(ErrorCode::NotFound) => Ok(None),
            Err(e) => Err(e),
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

    async fn agent_child_listing(&self, pid: &str) -> Result<Vec<String>, ErrorCode> {
        let registered = {
            let state = self.state.lock().await;
            state.agents.keys().cloned().collect::<Vec<_>>()
        };
        let proc_pids = self.proc_pids().await?;
        let mut names = Vec::new();
        for child_pid in proc_pids {
            if !registered.iter().any(|registered| registered == &child_pid) {
                continue;
            }
            if self.proc_parent_matches(&child_pid, pid).await? {
                names.push(child_pid);
            }
        }
        Ok(names)
    }

    async fn create_agent_file(
        &self,
        newfid: Fid,
        pid: String,
        backing: Arc<dyn FileServer>,
        dir_fid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if dir_fid == Fid::ROOT && is_agent_overlay_reserved_name(name) {
            return Err(ErrorCode::BadRequest);
        }
        let backing_fid = Fid(NEXT_BACKING_FID.fetch_add(1, Ordering::Relaxed));
        let qid = match backing.create(dir_fid, backing_fid, name, kind).await {
            Ok(qid) => qid,
            Err(e) => {
                let _ = backing.clunk(backing_fid).await;
                return Err(e);
            }
        };
        if let Err(e) = self
            .insert_fid(
                newfid,
                Node::AgentFile {
                    pid: pid.clone(),
                    backing: backing.clone(),
                    backing_fid,
                },
            )
            .await
        {
            rollback_created_fid(&backing, backing_fid).await;
            return Err(e);
        }
        Ok(namespace_agent_qid(&pid, qid))
    }

    async fn create_proc_file(
        &self,
        newfid: Fid,
        dir: ProcCreateDir,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let proc_fid = Fid(NEXT_PROC_FID.fetch_add(1, Ordering::Relaxed));
        let qid = match dir.proc.create(dir.dir_fid, proc_fid, name, kind).await {
            Ok(qid) => qid,
            Err(e) => return Err(e),
        };
        if let Err(e) = self
            .insert_fid(
                newfid,
                Node::ProcFile {
                    proc: dir.proc.clone(),
                    proc_fid,
                    pid: dir.pid,
                    names: proc_child_names(dir.names, name),
                },
            )
            .await
        {
            rollback_created_fid(&dir.proc, proc_fid).await;
            return Err(e);
        }
        Ok(qid)
    }

    async fn insert_fid(&self, fid: Fid, node: Node) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        if fid == Fid::ROOT || state.fids.contains_key(&fid) {
            return Err(ErrorCode::BadRequest);
        }
        state.fids.insert(fid, Entry { node });
        Ok(())
    }
}

fn agent_event_sink<T>(agent: &Arc<T>) -> Option<Arc<AgentFs>>
where
    T: FileServer + Any + 'static,
{
    let erased: Arc<dyn Any + Send + Sync> = agent.clone();
    Arc::downcast::<AgentFs>(erased).ok()
}

struct AgentOutputEventSink {
    state: Arc<Mutex<State>>,
}

struct AgentInputEventSink {
    state: Arc<Mutex<State>>,
}

struct AgentIoEventSink {
    state: Arc<Mutex<State>>,
}

struct AgentProcessEventSink {
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl ProcessEventSink for AgentProcessEventSink {
    async fn process_event(&self, pid: &str, event: ProcessEvent) {
        let event_sink = {
            let state = self.state.lock().await;
            state
                .agents
                .get(pid)
                .and_then(|agent| agent.event_sink.clone())
        };
        if let Some(agent) = event_sink {
            match event {
                ProcessEvent::Input { count } => agent.append_input_event(count).await,
                ProcessEvent::Output { count } => agent.append_output_event(count).await,
                ProcessEvent::Status { status } => agent.append_status_event(&status).await,
            }
        }
    }
}

#[async_trait]
impl ProcessIoEventSink for AgentIoEventSink {
    async fn io_appended(&self, pid: &str, kind: ProcessIoEventKind, count: u32) {
        let event_sink = {
            let state = self.state.lock().await;
            state
                .agents
                .get(pid)
                .and_then(|agent| agent.event_sink.clone())
        };
        if let Some(agent) = event_sink {
            match kind {
                ProcessIoEventKind::Input => agent.append_input_event(count).await,
                ProcessIoEventKind::Output => agent.append_output_event(count).await,
            }
        }
    }
}

#[async_trait]
impl ProcessInputEventSink for AgentInputEventSink {
    async fn input_appended(&self, pid: &str, count: u32) {
        let event_sink = {
            let state = self.state.lock().await;
            state
                .agents
                .get(pid)
                .and_then(|agent| agent.event_sink.clone())
        };
        if let Some(agent) = event_sink {
            agent.append_input_event(count).await;
        }
    }
}

#[async_trait]
impl ProcessOutputEventSink for AgentOutputEventSink {
    async fn output_appended(&self, pid: &str, count: u32) {
        let event_sink = {
            let state = self.state.lock().await;
            state
                .agents
                .get(pid)
                .and_then(|agent| agent.event_sink.clone())
        };
        if let Some(agent) = event_sink {
            agent.append_output_event(count).await;
        }
    }
}

async fn rollback_created_fid(server: &Arc<dyn FileServer>, fid: Fid) {
    if server.remove(fid).await.is_err() {
        let _ = server.clunk(fid).await;
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
            Node::AgentChildren { pid } => self.bind_agent_child_walk(newfid, &pid, names).await?,
            Node::AgentFile {
                pid,
                backing,
                backing_fid,
            } => {
                self.bind_agent_walk(newfid, pid, backing, backing_fid, names)
                    .await?
            }
            Node::ProcFile {
                proc,
                proc_fid,
                pid,
                names: proc_names,
            } => {
                self.bind_proc_relative_walk(proc, proc_fid, pid, proc_names, names)
                    .await?
            }
        };
        let qid = match self.qid_for_node(&new_node).await {
            Ok(qid) => qid,
            Err(error) => {
                release_node(new_node).await;
                return Err(error);
            }
        };
        if let Err(error) = self.insert_fid(newfid, new_node.clone()).await {
            release_node(new_node).await;
            return Err(error);
        }
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        match self.node(fid).await? {
            Node::Root => {
                if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
                    return Err(ErrorCode::NoAccess);
                }
                let listing = self.root_listing().await?;
                Ok(root_qid(&listing))
            }
            Node::AgentChildren { pid } => {
                if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
                    return Err(ErrorCode::NoAccess);
                }
                let listing = self.agent_child_listing(&pid).await?;
                Ok(agent_children_qid(&pid, &listing))
            }
            Node::AgentRoot { pid, backing } => backing
                .open(Fid::ROOT, mode)
                .await
                .map(|qid| namespace_agent_qid(&pid, qid)),
            Node::AgentFile {
                pid,
                backing,
                backing_fid,
                ..
            } => backing
                .open(backing_fid, mode)
                .await
                .map(|qid| namespace_agent_qid(&pid, qid)),
            Node::ProcFile { proc, proc_fid, .. } => proc.open(proc_fid, mode).await,
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
            Node::AgentChildren { pid } => Ok(slice(
                self.agent_child_listing(&pid)
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
            Node::ProcFile { proc, proc_fid, .. } => proc.read(proc_fid, offset, count).await,
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        match self.node(fid).await? {
            Node::Root => Err(ErrorCode::NoAccess),
            Node::AgentChildren { .. } => Err(ErrorCode::NoAccess),
            Node::AgentRoot { backing, .. } => backing.write(Fid::ROOT, offset, data).await,
            Node::AgentFile {
                backing,
                backing_fid,
                ..
            } => backing.write(backing_fid, offset, data).await,
            Node::ProcFile { proc, proc_fid, .. } => proc.write(proc_fid, offset, data).await,
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        match self.node(fid).await? {
            Node::Root => {
                let listing = self.root_listing().await?;
                Ok(Stat {
                    name: String::new(),
                    qid: root_qid(&listing),
                    length: listing.join("\n").len() as u64,
                    writable: false,
                })
            }
            Node::AgentRoot { pid, backing } => {
                let mut stat = backing.stat(Fid::ROOT).await?;
                stat.qid = namespace_agent_qid(&pid, stat.qid);
                stat.length = self.agent_listing(&pid, backing).await?.join("\n").len() as u64;
                Ok(stat)
            }
            Node::AgentChildren { pid } => {
                let listing = self.agent_child_listing(&pid).await?;
                Ok(Stat {
                    name: "children".to_string(),
                    qid: agent_children_qid(&pid, &listing),
                    length: listing.join("\n").len() as u64,
                    writable: false,
                })
            }
            Node::AgentFile {
                pid,
                backing,
                backing_fid,
                ..
            } => {
                let mut stat = backing.stat(backing_fid).await?;
                stat.qid = namespace_agent_qid(&pid, stat.qid);
                Ok(stat)
            }
            Node::ProcFile { proc, proc_fid, .. } => proc.stat(proc_fid).await,
        }
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        {
            let state = self.state.lock().await;
            if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
        }
        match self.node(fid).await? {
            Node::Root => Err(ErrorCode::Unsupported),
            Node::AgentChildren { .. } => Err(ErrorCode::Unsupported),
            Node::AgentRoot { pid, backing } => {
                self.create_agent_file(newfid, pid, backing, Fid::ROOT, name, kind)
                    .await
            }
            Node::AgentFile {
                pid,
                backing,
                backing_fid,
                ..
            } => {
                self.create_agent_file(newfid, pid, backing, backing_fid, name, kind)
                    .await
            }
            Node::ProcFile {
                proc,
                proc_fid,
                pid,
                names,
            } => {
                let dir = ProcCreateDir {
                    proc,
                    dir_fid: proc_fid,
                    pid,
                    names,
                };
                self.create_proc_file(newfid, dir, name, kind).await
            }
        }
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Err(ErrorCode::Unsupported);
        }
        match self.node(fid).await? {
            Node::Root => Err(ErrorCode::Unsupported),
            Node::AgentChildren { .. } => Err(ErrorCode::Unsupported),
            Node::AgentRoot { backing, .. } => backing.remove(Fid::ROOT).await,
            Node::AgentFile {
                backing,
                backing_fid,
                ..
            } => backing.remove(backing_fid).await,
            Node::ProcFile { proc, proc_fid, .. } => proc.remove(proc_fid).await,
        }?;
        self.state.lock().await.fids.remove(&fid);
        Ok(())
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
            Some(Node::ProcFile { proc, proc_fid, .. }) => proc.clunk(proc_fid).await,
            Some(Node::Root | Node::AgentRoot { .. } | Node::AgentChildren { .. }) => Ok(()),
            None => Err(ErrorCode::NotFound),
        }
    }
}

impl AgentRootFs {
    async fn node(&self, fid: Fid) -> Result<Node, ErrorCode> {
        let state = self.state.lock().await;
        Self::node_of(&state, fid)
    }

    async fn qid_for_node(&self, node: &Node) -> Result<Qid, ErrorCode> {
        match node {
            Node::Root => {
                let listing = self.root_listing().await?;
                Ok(root_qid(&listing))
            }
            Node::AgentRoot { pid, backing } => backing
                .stat(Fid::ROOT)
                .await
                .map(|stat| namespace_agent_qid(pid, stat.qid)),
            Node::AgentChildren { pid } => {
                let listing = self.agent_child_listing(pid).await?;
                Ok(agent_children_qid(pid, &listing))
            }
            Node::AgentFile {
                pid,
                backing,
                backing_fid,
                ..
            } => backing
                .stat(*backing_fid)
                .await
                .map(|stat| namespace_agent_qid(pid, stat.qid)),
            Node::ProcFile { proc, proc_fid, .. } => {
                proc.stat(*proc_fid).await.map(|stat| stat.qid)
            }
        }
    }
}

async fn read_file_text(
    server: Arc<dyn FileServer>,
    names: &[String],
    raw_fid: u64,
) -> Result<String, ErrorCode> {
    let fid = Fid(raw_fid);
    server.walk(Fid::ROOT, fid, names).await?;
    let result = match server.open(fid, OpenMode::Read).await {
        Ok(_) => {
            let length = match server.stat(fid).await {
                Ok(stat) => stat.length,
                Err(e) => {
                    let _ = server.clunk(fid).await;
                    return Err(e);
                }
            };
            match server
                .read(
                    fid,
                    0,
                    u32::try_from(length.saturating_add(1)).unwrap_or(u32::MAX),
                )
                .await
            {
                Ok(bytes) => String::from_utf8(bytes).map_err(|_| ErrorCode::Io),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    };
    let clunk = server.clunk(fid).await;
    let text = result?;
    clunk?;
    Ok(text)
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
        "status" | "parent" | "credentials" | "exit" | "ctl" | "namespace" | "io"
    )
}

fn proc_child_names(mut names: Vec<String>, child: &str) -> Vec<String> {
    names.push(child.to_string());
    names
}

fn is_agent_overlay_reserved_name(name: &str) -> bool {
    matches!(name, "children" | "io") || is_proc_overlay_name(name)
}

fn agent_children_qid(pid: &str, children: &[String]) -> Qid {
    Qid {
        kind: FileKind::Dir,
        version: hash_value(&("agent-children-version", pid, children)) as u32,
        path: hash_value(&("agent-children", pid)),
    }
}

fn root_qid(listing: &[String]) -> Qid {
    Qid {
        kind: FileKind::Dir,
        version: hash_value(&("agent-root-version", listing)) as u32,
        path: 0xA6E7,
    }
}

async fn release_node(node: Node) {
    match node {
        Node::AgentFile {
            backing,
            backing_fid,
            ..
        } => {
            let _ = backing.clunk(backing_fid).await;
        }
        Node::ProcFile { proc, proc_fid, .. } => {
            let _ = proc.clunk(proc_fid).await;
        }
        Node::Root | Node::AgentRoot { .. } | Node::AgentChildren { .. } => {}
    }
}

fn namespace_agent_qid(pid: &str, qid: Qid) -> Qid {
    Qid {
        kind: qid.kind,
        version: qid.version,
        path: hash_value(&("agent-qid", pid, qid.path, file_kind_tag(qid.kind))),
    }
}

fn file_kind_tag(kind: FileKind) -> u8 {
    match kind {
        FileKind::Dir => 1,
        FileKind::File => 2,
        FileKind::Stream => 3,
        FileKind::Clone => 4,
    }
}

fn hash_value<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
