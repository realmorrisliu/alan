//! `/proc` — the synthetic device that renders the process table as files
//! (§7.1) and creates processes via clone-via-open (§7.1a).
//!
//! `/proc` is the single source of truth for processes; any `/agent`-style view
//! is derived from it. Its tree is:
//!
//! ```text
//! /proc
//!   clone                 # open → pending pid; write exec spec; clunk → start
//!   <pid>/
//!     status              # "running" | "exited"
//!     parent              # parent pid, or "" for none
//!     credentials         # uname
//!     exit                # exit code once exited, else ""
//!     ctl                 # write "interrupt"/"cancel" (generic process control)
//!     io/input            # the process's input stream
//!     io/output           # the process's output stream
//!     io/events           # IO-scoped input/output event stream
//! ```
//!
//! Process creation is pure aP: opening `clone` allocates a fid-private pending
//! pid (not yet in the public listing), the caller writes the exec-spec document
//! and `clunk`s to commit (commit-on-clunk; a malformed spec is rejected at
//! clunk and the pending slot is discarded, leaking nothing).
//!
//! v1 limitation (ADR-0024 R1): the kernel runs in one address space. A `/proc`
//! view can now carry its spawner's parent, credentials, and namespace into
//! `clone`, but the R1 capability boundary is still convention-enforced by
//! namespace resolution in-process rather than hard OS isolation.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::task::JoinHandle;

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, ProcessEvent,
    ProcessEventSink, ProcessEventSource, ProcessInputEventSink, ProcessInputEventSource,
    ProcessIoEventKind, ProcessIoEventSink, ProcessIoEventSource, ProcessOutputEventSink,
    ProcessOutputEventSource, Qid, Stat, Stream,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{Access, Credentials, ExecSpec, LiveNamespace, Namespace, Pid, ProcessTable, Status};

/// One committed process invocation handed to user-space execution.
#[derive(Clone)]
pub struct ProcessInvocation {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub credentials: Credentials,
    pub namespace: Namespace,
    pub exec: ExecSpec,
}

/// The terminal result produced by a user-space process runner.
pub struct ProcessOutcome {
    pub output: Vec<u8>,
    pub exit_code: i32,
}

impl ProcessOutcome {
    pub fn exited(exit_code: i32, output: impl Into<Vec<u8>>) -> Self {
        Self {
            output: output.into(),
            exit_code,
        }
    }
}

/// User-space execution hook for a committed process.
///
/// The kernel still only owns `/proc`, namespace state, process status, and the
/// process output stream. The runner supplies the executable semantics layered
/// above the substrate.
#[async_trait]
pub trait ProcessRunner: Send + Sync + 'static {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome;
}

/// What a fid in `/proc` points at.
#[derive(Clone)]
enum Node {
    Root,
    Clone,
    Proc(Pid),
    Status(Pid),
    Parent(Pid),
    Credentials(Pid),
    Exit(Pid),
    Ctl(Pid),
    IoDir(Pid),
    Input(Pid),
    Output(Pid),
    IoEvents(Pid),
    NamespaceInfo(Pid),
    Descriptors(Pid),
}

struct ProcFid {
    node: Node,
    /// The mode this fid was opened with (None until opened). Write surfaces
    /// require write intent, so a read-only mount/open cannot spawn or cancel.
    mode: Option<OpenMode>,
    /// For a fid that opened `clone`: the pending pid awaiting its exec spec.
    clone_pid: Option<Pid>,
    /// Buffered exec-spec document for a `clone` fid (commit-on-clunk).
    write_buf: Vec<u8>,
    /// Whether this fid received a write, including an intentional empty input.
    wrote: bool,
    /// Whether a buffered write failed; failed documents must never commit on clunk.
    write_failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ProcFidKey {
    view_id: u64,
    fid: Fid,
}

impl ProcFid {
    fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            clone_pid: None,
            write_buf: Vec::new(),
            wrote: false,
            write_failed: false,
        }
    }

    fn buffer_write(
        &mut self,
        offset: Offset,
        data: &[u8],
        limit: usize,
    ) -> Result<u32, ErrorCode> {
        let Some(start) = usize::try_from(offset).ok() else {
            self.write_failed = true;
            return Err(ErrorCode::BadRequest);
        };
        let Some(end) = start.checked_add(data.len()) else {
            self.write_failed = true;
            return Err(ErrorCode::BadRequest);
        };
        if end > limit {
            self.write_failed = true;
            return Err(ErrorCode::BadRequest);
        }
        if self.write_buf.len() < end {
            self.write_buf.resize(end, 0);
        }
        self.write_buf[start..end].copy_from_slice(data);
        self.wrote = true;
        Ok(data.len() as u32)
    }
}

/// Upper bound on a buffered exec-spec document, so a huge/sparse write offset
/// cannot make `/proc` allocate unbounded memory.
const MAX_EXEC_SPEC_BYTES: usize = 1 << 16; // 64 KiB
const MAX_PROCESS_INPUT_BYTES: usize = 1 << 20; // 1 MiB
const CHILD_PID_PLACEHOLDER: &str = "<child-pid>";
static NEXT_PROCFS_VIEW_ID: AtomicU64 = AtomicU64::new(1);

struct State {
    table: ProcessTable,
    fids: HashMap<ProcFidKey, ProcFid>,
    live_namespaces: HashMap<Pid, LiveNamespace>,
    /// Generic IO streams per committed process. The kernel owns the files; user
    /// space supplies the execution semantics layered above them.
    inputs: HashMap<Pid, Stream>,
    outputs: HashMap<Pid, Stream>,
    io_events: HashMap<Pid, Stream>,
    event_history: HashMap<Pid, Vec<ProcessEvent>>,
    event_observers: HashMap<Pid, Vec<Arc<OrderedProcessEventObserver>>>,
    io_observers: HashMap<Pid, Vec<Arc<OrderedProcessIoEventObserver>>>,
    input_observers: HashMap<Pid, Vec<Arc<dyn ProcessInputEventSink>>>,
    output_observers: HashMap<Pid, Vec<Arc<dyn ProcessOutputEventSink>>>,
    runner_tasks: HashMap<Pid, JoinHandle<()>>,
}

struct OrderedProcessEventObserver {
    sink: Arc<dyn ProcessEventSink>,
    replay: Mutex<()>,
}

impl OrderedProcessEventObserver {
    fn new(sink: Arc<dyn ProcessEventSink>) -> Self {
        Self {
            sink,
            replay: Mutex::new(()),
        }
    }

    async fn deliver(&self, pid: &str, event: ProcessEvent) {
        let _replay = self.replay.lock().await;
        self.sink.process_event(pid, event).await;
    }
}

struct OrderedProcessIoEventObserver {
    sink: Arc<dyn ProcessIoEventSink>,
    replay: Mutex<()>,
}

impl OrderedProcessIoEventObserver {
    fn new(sink: Arc<dyn ProcessIoEventSink>) -> Self {
        Self {
            sink,
            replay: Mutex::new(()),
        }
    }

    async fn deliver(&self, pid: &str, kind: ProcessIoEventKind, count: u32) {
        let _replay = self.replay.lock().await;
        self.sink.io_appended(pid, kind, count).await;
    }
}

#[derive(Clone)]
struct SpawnContext {
    parent: Option<Pid>,
    namespace: NamespaceSource,
    credentials: Credentials,
}

#[derive(Clone)]
enum NamespaceSource {
    Snapshot(Namespace),
    Live(LiveNamespace),
}

impl NamespaceSource {
    fn snapshot(&self) -> Namespace {
        match self {
            Self::Snapshot(namespace) => namespace.clone(),
            Self::Live(namespace) => namespace.snapshot(),
        }
    }

    fn child_with_path_substitution(&self, placeholder: &str, pid: &str) -> Namespace {
        self.snapshot()
            .child_with_path_substitution(placeholder, pid)
    }
}

/// The `/proc` file server.
#[derive(Clone)]
pub struct ProcFs {
    state: Arc<Mutex<State>>,
    view_id: u64,
    root_node: Node,
    spawn_context: SpawnContext,
    runner: Option<Arc<dyn ProcessRunner>>,
}

/// Point-in-time observation of the files owned by one Process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessFileSnapshot {
    pub status: Status,
    pub exit_code: Option<i32>,
    pub output: Vec<u8>,
    pub output_offset: u64,
    pub io_events_offset: u64,
}

impl Default for ProcFs {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcFs {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                table: ProcessTable::new(),
                fids: HashMap::new(),
                live_namespaces: HashMap::new(),
                inputs: HashMap::new(),
                outputs: HashMap::new(),
                io_events: HashMap::new(),
                event_history: HashMap::new(),
                event_observers: HashMap::new(),
                io_observers: HashMap::new(),
                input_observers: HashMap::new(),
                output_observers: HashMap::new(),
                runner_tasks: HashMap::new(),
            })),
            view_id: next_view_id(),
            root_node: Node::Root,
            spawn_context: SpawnContext {
                parent: None,
                namespace: NamespaceSource::Snapshot(Namespace::new()),
                credentials: Credentials::system(),
            },
            runner: None,
        }
    }

    /// Attach the user-space runner that will execute committed processes for
    /// this `/proc` view and any spawner views cloned from it.
    pub fn with_runner(mut self, runner: Arc<dyn ProcessRunner>) -> Self {
        self.runner = Some(runner);
        self
    }

    /// Create a `/proc` view over the same process table with the spawn context
    /// of a particular process. Opening `clone` through this view starts child
    /// processes with the given parent, credentials, and a child copy of the
    /// given namespace.
    pub fn for_spawner(
        &self,
        parent: Option<Pid>,
        namespace: Namespace,
        credentials: Credentials,
    ) -> Self {
        Self {
            state: self.state.clone(),
            view_id: next_view_id(),
            root_node: Node::Root,
            spawn_context: SpawnContext {
                parent,
                namespace: NamespaceSource::Snapshot(namespace),
                credentials,
            },
            runner: self.runner.clone(),
        }
    }

    /// Create a `/proc` view whose spawner inherits from a live namespace handle.
    /// Children snapshot the handle at clone time, so grants approved before spawn
    /// are visible without making every child share later mutations by default.
    pub fn for_live_spawner(
        &self,
        parent: Option<Pid>,
        namespace: LiveNamespace,
        credentials: Credentials,
    ) -> Self {
        Self {
            state: self.state.clone(),
            view_id: next_view_id(),
            root_node: Node::Root,
            spawn_context: SpawnContext {
                parent,
                namespace: NamespaceSource::Live(namespace),
                credentials,
            },
            runner: self.runner.clone(),
        }
    }

    /// Create a view whose root is the `clone` file for a delegated
    /// `/proc/clone` mount.
    pub fn clone_file_for_spawner(
        &self,
        parent: Option<Pid>,
        namespace: Namespace,
        credentials: Credentials,
    ) -> Self {
        let mut view = self.for_spawner(parent, namespace, credentials);
        view.root_node = Node::Clone;
        view
    }

    /// Bind a committed process's namespace description to a live namespace
    /// handle. This is used for long-lived root processes whose namespace can gain
    /// approved mounts after the process has started.
    pub async fn bind_live_namespace(&self, pid: Pid, namespace: LiveNamespace) {
        let mut state = self.state.lock().await;
        if state.table.get(pid).is_some() {
            state.live_namespaces.insert(pid, namespace);
            state.table.bump_generation(pid);
        }
    }

    /// Record the terminal result reported by the user-space host for a
    /// committed process.
    ///
    /// This is distinct from writing `cancel` or `interrupt` to `ctl`: those
    /// are control requests and therefore terminate a live process with the
    /// conventional cancellation exit code. A host that observes its process
    /// complete normally must preserve the actual result instead.
    pub async fn record_exit(&self, pid: Pid, exit_code: i32) {
        let (transitioned, runner_task) = {
            let mut state = self.state.lock().await;
            let transitioned = state
                .table
                .get(pid)
                .is_some_and(|process| process.status != Status::Exited);
            let runner_task = state.runner_tasks.remove(&pid);
            state.table.exit(pid, exit_code);
            (transitioned, runner_task)
        };
        if let Some(task) = runner_task {
            task.abort();
        }
        if transitioned {
            self.publish_process_event(
                pid,
                ProcessEvent::Status {
                    status: "exited".to_string(),
                },
            )
            .await;
        }
    }

    /// Observe Process lifecycle and retained IO files without opening a live-edge reader.
    pub async fn observe_process_files(&self, pid: Pid) -> Option<ProcessFileSnapshot> {
        let (status, exit_code, output, io_events) = {
            let state = self.state.lock().await;
            let process = state.table.get(pid)?;
            (
                process.status,
                process.exit_code,
                state.outputs.get(&pid).cloned(),
                state.io_events.get(&pid).cloned(),
            )
        };
        let output_offset = match output.as_ref() {
            Some(stream) => stream.len().await,
            None => 0,
        };
        let output = match output {
            Some(stream) => {
                let mut bytes = Vec::new();
                let mut offset = 0;
                while offset < output_offset {
                    let chunk = stream
                        .read(offset, (output_offset - offset).min(u32::MAX as u64) as u32)
                        .await;
                    if chunk.is_empty() {
                        break;
                    }
                    offset += chunk.len() as u64;
                    bytes.extend(chunk);
                }
                bytes
            }
            None => Vec::new(),
        };
        let io_events_offset = match io_events {
            Some(stream) => stream.len().await,
            None => 0,
        };
        Some(ProcessFileSnapshot {
            status,
            exit_code,
            output,
            output_offset,
            io_events_offset,
        })
    }

    /// Observe Process lifecycle without waiting on retained IO work.
    pub fn try_observe_process_lifecycle(&self, pid: Pid) -> Option<(Status, Option<i32>)> {
        let state = self.state.try_lock().ok()?;
        let process = state.table.get(pid)?;
        Some((process.status, process.exit_code))
    }

    fn child_namespace_for_spawn(&self, pid: Pid) -> Namespace {
        let mut child_namespace = self
            .spawn_context
            .namespace
            .child_with_path_substitution(CHILD_PID_PLACEHOLDER, &pid.0.to_string());
        self.rebind_proc_spawners(&mut child_namespace, pid);
        child_namespace
    }

    fn rebind_proc_spawners(&self, namespace: &mut Namespace, pid: Pid) {
        if let Some(proc_access) = mount_access_at(namespace, "/proc") {
            let child_proc = self.for_spawner(
                Some(pid),
                namespace.clone(),
                self.spawn_context.credentials.clone(),
            );
            namespace.unmount("/proc");
            namespace.mount(
                "/proc",
                InProcessTransport::new(Arc::new(child_proc)),
                proc_access,
            );
        }
        if let Some(clone_access) = mount_access_at(namespace, "/proc/clone") {
            let child_clone = self.clone_file_for_spawner(
                Some(pid),
                namespace.clone(),
                self.spawn_context.credentials.clone(),
            );
            namespace.unmount("/proc/clone");
            namespace.mount(
                "/proc/clone",
                InProcessTransport::new(Arc::new(child_clone)),
                clone_access,
            );
        }
    }

    fn fid_key(&self, fid: Fid) -> ProcFidKey {
        ProcFidKey {
            view_id: self.view_id,
            fid,
        }
    }

    async fn publish_process_event(&self, pid: Pid, event: ProcessEvent) {
        let (event_observers, io_observers, input_observers, output_observers) = {
            let mut state = self.state.lock().await;
            state
                .event_history
                .entry(pid)
                .or_default()
                .push(event.clone());
            (
                state.event_observers.get(&pid).cloned().unwrap_or_default(),
                state.io_observers.get(&pid).cloned().unwrap_or_default(),
                state.input_observers.get(&pid).cloned().unwrap_or_default(),
                state
                    .output_observers
                    .get(&pid)
                    .cloned()
                    .unwrap_or_default(),
            )
        };
        let pid = pid.0.to_string();
        match &event {
            ProcessEvent::Input { count } => {
                for observer in input_observers {
                    observer.input_appended(&pid, *count).await;
                }
                for observer in io_observers {
                    observer
                        .deliver(&pid, ProcessIoEventKind::Input, *count)
                        .await;
                }
            }
            ProcessEvent::Output { count } => {
                for observer in output_observers {
                    observer.output_appended(&pid, *count).await;
                }
                for observer in io_observers {
                    observer
                        .deliver(&pid, ProcessIoEventKind::Output, *count)
                        .await;
                }
            }
            ProcessEvent::Status { .. } => {}
        }
        for observer in event_observers {
            observer.deliver(&pid, event.clone()).await;
        }
    }
}

#[async_trait]
impl ProcessOutputEventSource for ProcFs {
    async fn subscribe_process_output(
        &self,
        pid: &str,
        sink: Arc<dyn ProcessOutputEventSink>,
    ) -> Result<(), ErrorCode> {
        let pid = parse_pid(pid).ok_or(ErrorCode::BadRequest)?;
        let (pid_text, replay) = {
            let mut state = self.state.lock().await;
            if state.table.get(pid).is_none() {
                return Err(ErrorCode::NotFound);
            }
            let replay = state
                .event_history
                .get(&pid)
                .into_iter()
                .flatten()
                .filter_map(|event| match event {
                    ProcessEvent::Input { .. } | ProcessEvent::Status { .. } => None,
                    ProcessEvent::Output { count } => Some(*count),
                })
                .collect::<Vec<_>>();
            state
                .output_observers
                .entry(pid)
                .or_default()
                .push(sink.clone());
            (pid.0.to_string(), replay)
        };
        for count in replay {
            sink.output_appended(&pid_text, count).await;
        }
        Ok(())
    }
}

#[async_trait]
impl ProcessInputEventSource for ProcFs {
    async fn subscribe_process_input(
        &self,
        pid: &str,
        sink: Arc<dyn ProcessInputEventSink>,
    ) -> Result<(), ErrorCode> {
        let pid = parse_pid(pid).ok_or(ErrorCode::BadRequest)?;
        let (pid_text, replay) = {
            let mut state = self.state.lock().await;
            if state.table.get(pid).is_none() {
                return Err(ErrorCode::NotFound);
            }
            let replay = state
                .event_history
                .get(&pid)
                .into_iter()
                .flatten()
                .filter_map(|event| match event {
                    ProcessEvent::Input { count } => Some(*count),
                    ProcessEvent::Output { .. } | ProcessEvent::Status { .. } => None,
                })
                .collect::<Vec<_>>();
            state
                .input_observers
                .entry(pid)
                .or_default()
                .push(sink.clone());
            (pid.0.to_string(), replay)
        };
        for count in replay {
            sink.input_appended(&pid_text, count).await;
        }
        Ok(())
    }
}

#[async_trait]
impl ProcessIoEventSource for ProcFs {
    async fn subscribe_process_io(
        &self,
        pid: &str,
        sink: Arc<dyn ProcessIoEventSink>,
    ) -> Result<(), ErrorCode> {
        let pid = parse_pid(pid).ok_or(ErrorCode::BadRequest)?;
        let observer = Arc::new(OrderedProcessIoEventObserver::new(sink));
        let observer_for_state = observer.clone();
        let replay_sink = observer.sink.clone();
        let replay_guard = observer.replay.lock().await;
        let (pid_text, replay) = {
            let mut state = self.state.lock().await;
            if state.table.get(pid).is_none() {
                return Err(ErrorCode::NotFound);
            }
            let replay = state
                .event_history
                .get(&pid)
                .into_iter()
                .flatten()
                .filter_map(|event| match event {
                    ProcessEvent::Input { count } => Some((ProcessIoEventKind::Input, *count)),
                    ProcessEvent::Output { count } => Some((ProcessIoEventKind::Output, *count)),
                    ProcessEvent::Status { .. } => None,
                })
                .collect::<Vec<_>>();
            state
                .io_observers
                .entry(pid)
                .or_default()
                .push(observer_for_state);
            (pid.0.to_string(), replay)
        };
        for (kind, count) in replay {
            replay_sink.io_appended(&pid_text, kind, count).await;
        }
        drop(replay_guard);
        Ok(())
    }
}

#[async_trait]
impl ProcessEventSource for ProcFs {
    async fn subscribe_process_events(
        &self,
        pid: &str,
        sink: Arc<dyn ProcessEventSink>,
    ) -> Result<(), ErrorCode> {
        let pid = parse_pid(pid).ok_or(ErrorCode::BadRequest)?;
        let observer = Arc::new(OrderedProcessEventObserver::new(sink));
        let observer_for_state = observer.clone();
        let replay_sink = observer.sink.clone();
        let replay_guard = observer.replay.lock().await;
        let (pid_text, replay) = {
            let mut state = self.state.lock().await;
            if state.table.get(pid).is_none() {
                return Err(ErrorCode::NotFound);
            }
            let replay = state.event_history.get(&pid).cloned().unwrap_or_default();
            state
                .event_observers
                .entry(pid)
                .or_default()
                .push(observer_for_state);
            (pid.0.to_string(), replay)
        };
        for event in replay {
            replay_sink.process_event(&pid_text, event).await;
        }
        drop(replay_guard);
        Ok(())
    }
}

impl State {
    fn node_of(&self, key: ProcFidKey, root_node: &Node) -> Result<Node, ErrorCode> {
        let fid = key.fid;
        if fid == Fid::ROOT {
            return Ok(root_node.clone());
        }
        self.fids
            .get(&key)
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    /// The qid for `node`, with its current version from the process table's
    /// generations: the public listing for the root, a per-process generation for
    /// per-pid files, and a stable 0 for the clone file and the output stream
    /// (a stream's freshness is its read offset, not the qid version).
    fn qid(&self, node: &Node) -> Qid {
        let (kind, path) = node_identity(node);
        let version = match node {
            Node::Root => self.table.listing_generation(),
            Node::Clone | Node::Output(_) => 0,
            Node::Input(_) | Node::IoEvents(_) => 0,
            Node::Proc(p)
            | Node::IoDir(p)
            | Node::Status(p)
            | Node::Parent(p)
            | Node::Credentials(p)
            | Node::Exit(p)
            | Node::Ctl(p) => self.table.generation(*p),
            Node::NamespaceInfo(p) => self.table.generation(*p).wrapping_add(
                self.live_namespaces
                    .get(p)
                    .map_or(0, LiveNamespace::generation),
            ),
            Node::Descriptors(p) => self.table.generation(*p),
        };
        Qid {
            kind,
            version,
            path,
        }
    }

    /// Resolve one path component from a node to its child node.
    fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => {
                if name == "clone" {
                    Ok(Node::Clone)
                } else if let Some(pid) = parse_pid(name).filter(|p| self.table.get(*p).is_some()) {
                    Ok(Node::Proc(pid))
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            Node::Proc(pid) => match name {
                "status" => Ok(Node::Status(*pid)),
                "parent" => Ok(Node::Parent(*pid)),
                "credentials" => Ok(Node::Credentials(*pid)),
                "exit" => Ok(Node::Exit(*pid)),
                "ctl" => Ok(Node::Ctl(*pid)),
                "io" => Ok(Node::IoDir(*pid)),
                "namespace" => Ok(Node::NamespaceInfo(*pid)),
                "descriptors" => Ok(Node::Descriptors(*pid)),
                _ => Err(ErrorCode::NotFound),
            },
            Node::IoDir(pid) => match name {
                "input" => Ok(Node::Input(*pid)),
                "output" => Ok(Node::Output(*pid)),
                "events" => Ok(Node::IoEvents(*pid)),
                _ => Err(ErrorCode::NotFound),
            },
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    /// The readable bytes of a non-directory node.
    fn file_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => {
                let mut names = vec!["clone".to_string()];
                names.extend(self.table.list().iter().map(|p| p.0.to_string()));
                names.join("\n").into_bytes()
            }
            Node::Proc(_) => "status\nparent\ncredentials\nexit\nctl\nio\nnamespace\ndescriptors"
                .to_string()
                .into_bytes(),
            Node::IoDir(_) => b"input\noutput\nevents".to_vec(),
            Node::Status(pid) => match self.table.get(*pid).map(|p| p.status) {
                Some(Status::Running) => b"running\n".to_vec(),
                Some(Status::Exited) => b"exited\n".to_vec(),
                None => return Err(ErrorCode::NotFound),
            },
            Node::Parent(pid) => {
                let p = self.table.get(*pid).ok_or(ErrorCode::NotFound)?;
                p.parent
                    .map(|pp| pp.0.to_string())
                    .unwrap_or_default()
                    .into_bytes()
            }
            Node::Credentials(pid) => {
                let p = self.table.get(*pid).ok_or(ErrorCode::NotFound)?;
                p.credentials.uname.clone().into_bytes()
            }
            Node::Exit(pid) => {
                let p = self.table.get(*pid).ok_or(ErrorCode::NotFound)?;
                p.exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_default()
                    .into_bytes()
            }
            Node::NamespaceInfo(pid) => {
                let p = self.table.get(*pid).ok_or(ErrorCode::NotFound)?;
                self.live_namespaces
                    .get(pid)
                    .map(LiveNamespace::describe)
                    .unwrap_or_else(|| p.namespace.describe())
                    .iter()
                    .map(|(path, access)| {
                        let rights = match access {
                            crate::Access::ReadOnly => "ro",
                            crate::Access::ReadWrite => "rw",
                        };
                        format!("{path} {rights}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into_bytes()
            }
            Node::Descriptors(pid) => {
                let process = self.table.get(*pid).ok_or(ErrorCode::NotFound)?;
                serde_json::to_vec(&process.exec.descriptors).map_err(|_| ErrorCode::Io)?
            }
            // `clone`/`ctl` are write surfaces; IO streams are served directly in
            // `read`, not here.
            Node::Clone | Node::Ctl(_) | Node::Input(_) | Node::Output(_) | Node::IoEvents(_) => {
                return Err(ErrorCode::Unsupported);
            }
        };
        Ok(bytes)
    }
}

enum ProcStreamWrite {
    Output {
        pid: Pid,
        stream: Stream,
        events: Stream,
    },
    Status {
        pid: Pid,
    },
}

#[async_trait]
impl FileServer for ProcFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        let fid_key = self.fid_key(fid);
        let newfid_key = self.fid_key(newfid);
        // A fid is a handle to one interaction: never rebind the reserved root or
        // an already-live fid, or a retry/collision would clobber another fid's
        // state (e.g. drop a pending clone pid, leaking the slot).
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid_key) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = state.node_of(fid_key, &self.root_node)?;
        for name in names {
            node = state.child(&node, name)?;
        }
        let qid = state.qid(&node);
        state.fids.insert(newfid_key, ProcFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        // The pre-bound root fid is openable directly (to read the listing)
        // without a redundant empty walk, matching SrvFs and the reference server.
        let mut state = self.state.lock().await;
        let fid_key = self.fid_key(fid);
        let node = state.node_of(fid_key, &self.root_node)?;
        if fid == Fid::ROOT && !matches!(node, Node::Clone) {
            return Ok(state.qid(&node));
        }
        // Reopening a live fid before clunk is rejected, so a retried open cannot
        // overwrite a pending clone slot (leaking it) or downgrade write intent.
        if state.fids.get(&fid_key).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        // Clone-via-open: spawning requires write intent (you write the exec
        // spec), so a read-only open cannot allocate an uncommittable — and thus
        // leaked — pending slot.
        if matches!(node, Node::Clone) {
            if !matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
                return Err(ErrorCode::NoAccess);
            }
            let parent = self.spawn_context.parent;
            let credentials = self.spawn_context.credentials.clone();
            let proc_template = self.clone();
            let slot = state
                .table
                .clone_begin_with_namespace(parent, credentials, |pid| {
                    proc_template.child_namespace_for_spawn(pid)
                })
                .ok_or(ErrorCode::Io)?;
            let f = state
                .fids
                .entry(fid_key)
                .or_insert_with(|| ProcFid::at(Node::Clone));
            f.clone_pid = Some(slot);
            f.mode = Some(mode);
            return Ok(state.qid(&node));
        }
        let f = state.fids.get_mut(&fid_key).ok_or(ErrorCode::NotFound)?;
        f.mode = Some(mode);
        Ok(state.qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        // A process's IO files are streams: clone them out and serve blocking
        // reads without holding the state lock (so tailing one process never
        // stalls the whole `/proc`).
        let stream = {
            let state = self.state.lock().await;
            let fid_key = self.fid_key(fid);
            // An opened clone fid reads back its pending pid (the allocated name).
            if let Some(f) = state.fids.get(&fid_key)
                && let Some(pid) = f.clone_pid
            {
                return Ok(slice(pid.0.to_string().into_bytes(), offset, count));
            }
            let node = state.node_of(fid_key, &self.root_node)?;
            match node {
                Node::Input(pid) => state.inputs.get(&pid).cloned().ok_or(ErrorCode::NotFound)?,
                Node::Output(pid) => state
                    .outputs
                    .get(&pid)
                    .cloned()
                    .ok_or(ErrorCode::NotFound)?,
                Node::IoEvents(pid) => state
                    .io_events
                    .get(&pid)
                    .cloned()
                    .ok_or(ErrorCode::NotFound)?,
                other => return Ok(slice(state.file_bytes(&other)?, offset, count)),
            }
        };
        Ok(stream.read(offset, count).await)
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let stream_write = {
            let mut state = self.state.lock().await;
            let fid_key = self.fid_key(fid);
            let node = state.node_of(fid_key, &self.root_node)?;
            // Write surfaces require write authority established at open.
            let has_write_intent = state
                .fids
                .get(&fid_key)
                .is_some_and(|f| matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)));
            match node {
                // Exec-spec document for a clone fid: buffer at the given offset until
                // clunk (commit-on-clunk; honor offsets, bound size, reject overflow).
                Node::Clone => {
                    if !has_write_intent {
                        return Err(ErrorCode::NoAccess);
                    }
                    let f = state.fids.get_mut(&fid_key).ok_or(ErrorCode::NotFound)?;
                    if f.clone_pid.is_none() {
                        return Err(ErrorCode::BadRequest);
                    }
                    return f.buffer_write(offset, data, MAX_EXEC_SPEC_BYTES);
                }
                // Generic process control (interrupt/cancel) routes through ctl.
                Node::Ctl(pid) => {
                    if !has_write_intent {
                        return Err(ErrorCode::NoAccess);
                    }
                    match data {
                        b"cancel" | b"interrupt" => {
                            if let Some(task) = state.runner_tasks.remove(&pid) {
                                task.abort();
                            }
                            state.table.exit(pid, 130);
                        }
                        _ => return Err(ErrorCode::BadRequest),
                    }
                    ProcStreamWrite::Status { pid }
                }
                // Process input is a stream owned by `/proc`; parents, shells,
                // and hosts write to it, and process descriptors read from it.
                Node::Input(_) => {
                    if !has_write_intent {
                        return Err(ErrorCode::NoAccess);
                    }
                    let f = state.fids.get_mut(&fid_key).ok_or(ErrorCode::NotFound)?;
                    return f.buffer_write(offset, data, MAX_PROCESS_INPUT_BYTES);
                }
                // Process output is a stream owned by `/proc`; process descriptors
                // write to it, and readers tail it through `io/output`.
                Node::Output(pid) => {
                    if !has_write_intent {
                        return Err(ErrorCode::NoAccess);
                    }
                    ProcStreamWrite::Output {
                        pid,
                        stream: state
                            .outputs
                            .get(&pid)
                            .cloned()
                            .ok_or(ErrorCode::NotFound)?,
                        events: state
                            .io_events
                            .get(&pid)
                            .cloned()
                            .ok_or(ErrorCode::NotFound)?,
                    }
                }
                _ => return Err(ErrorCode::Unsupported),
            }
        };
        let count = data.len() as u32;
        match stream_write {
            ProcStreamWrite::Output {
                pid,
                stream,
                events,
            } => {
                stream.append(data).await;
                append_io_event(&events, "output", count).await;
                self.publish_process_event(pid, ProcessEvent::Output { count })
                    .await;
            }
            ProcStreamWrite::Status { pid } => {
                self.publish_process_event(
                    pid,
                    ProcessEvent::Status {
                        status: "exited".to_string(),
                    },
                )
                .await;
            }
        }
        Ok(count)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = state.node_of(self.fid_key(fid), &self.root_node)?;
        // Report the readable byte length so clients can size reads; write-only
        // surfaces (clone/ctl) are 0, and a process output is its retained length.
        let length = match &node {
            Node::Input(pid) => match state.inputs.get(pid) {
                Some(stream) => stream.len().await,
                None => 0,
            },
            Node::Output(pid) => match state.outputs.get(pid) {
                Some(stream) => stream.len().await,
                None => 0,
            },
            Node::IoEvents(pid) => match state.io_events.get(pid) {
                Some(stream) => stream.len().await,
                None => 0,
            },
            Node::Clone | Node::Ctl(_) => 0,
            other => state.file_bytes(other).map(|b| b.len() as u64).unwrap_or(0),
        };
        Ok(Stat {
            name: String::new(),
            qid: state.qid(&node),
            length,
            executable: false,
            writable: is_writable(&node),
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        // Processes are created through clone-via-open, not generic create.
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT && !matches!(self.root_node, Node::Clone) {
            return Ok(());
        }
        let (committed_process, committed_input) = {
            let mut state = self.state.lock().await;
            let Some(f) = state.fids.remove(&self.fid_key(fid)) else {
                return Err(ErrorCode::NotFound);
            };
            if f.write_failed {
                if let Some(pid) = f.clone_pid {
                    state.table.discard(pid);
                }
                return Err(ErrorCode::BadRequest);
            }
            // Commit-on-clunk: a clone fid commits its pending process here.
            if let Some(pid) = f.clone_pid {
                match serde_json::from_slice::<ExecSpec>(&f.write_buf) {
                    Ok(exec) => {
                        if let Some(namespace_manifest) = exec.namespace.as_ref() {
                            let Some(mut narrowed_namespace) = ({
                                let Some(pending_namespace) = state.table.pending_namespace(pid)
                                else {
                                    state.table.discard(pid);
                                    return Err(ErrorCode::BadRequest);
                                };
                                namespace_manifest.namespace_subset_from(pending_namespace)
                            }) else {
                                state.table.discard(pid);
                                return Err(ErrorCode::BadRequest);
                            };
                            self.rebind_proc_spawners(&mut narrowed_namespace, pid);
                            if state
                                .table
                                .replace_pending_namespace(pid, narrowed_namespace)
                                .is_none()
                            {
                                state.table.discard(pid);
                                return Err(ErrorCode::BadRequest);
                            }
                        }
                        let descriptors_valid = {
                            let Some(pending_namespace) = state.table.pending_namespace(pid) else {
                                state.table.discard(pid);
                                return Err(ErrorCode::BadRequest);
                            };
                            exec.descriptors.iter().all(|(number, path)| {
                                *number >= 3
                                    && valid_descriptor_path(path)
                                    && pending_namespace.resolve(path).is_ok()
                            })
                        };
                        if !descriptors_valid {
                            state.table.discard(pid);
                            return Err(ErrorCode::BadRequest);
                        }
                        let committed =
                            state.table.commit(pid, exec).ok_or(ErrorCode::BadRequest)?;
                        let process = state.table.get(committed).ok_or(ErrorCode::Io)?;
                        let invocation = ProcessInvocation {
                            pid: process.pid,
                            parent: process.parent,
                            credentials: process.credentials.clone(),
                            namespace: process.namespace.clone(),
                            exec: process.exec.clone(),
                        };
                        let input = Stream::new();
                        let output = Stream::new();
                        let io_events = Stream::new();
                        state.inputs.insert(committed, input);
                        state.outputs.insert(committed, output.clone());
                        state.io_events.insert(committed, io_events.clone());
                        state.event_history.insert(committed, Vec::new());
                        let runner_launch = self
                            .runner
                            .clone()
                            .map(|runner| (runner, output, io_events, invocation, self.clone()));
                        (Some((committed, runner_launch)), None)
                    }
                    Err(_) => {
                        // Reject at commit and discard the fid-private slot — it was
                        // never public, so nothing leaks.
                        state.table.discard(pid);
                        return Err(ErrorCode::BadRequest);
                    }
                }
            } else if let Node::Input(pid) = f.node
                && f.wrote
            {
                let stream = state.inputs.get(&pid).cloned().ok_or(ErrorCode::NotFound)?;
                let events = state
                    .io_events
                    .get(&pid)
                    .cloned()
                    .ok_or(ErrorCode::NotFound)?;
                let count = f.write_buf.len() as u32;
                let mut framed = format!("{}\n", f.write_buf.len()).into_bytes();
                framed.extend_from_slice(&f.write_buf);
                (None, Some((pid, stream, events, framed, count)))
            } else {
                (None, None)
            }
        };
        if let Some((pid, stream, events, framed, count)) = committed_input {
            stream.append(&framed).await;
            append_io_event(&events, "input", count).await;
            self.publish_process_event(pid, ProcessEvent::Input { count })
                .await;
        }
        if let Some((committed, runner_launch)) = committed_process {
            self.publish_process_event(
                committed,
                ProcessEvent::Status {
                    status: "running".to_string(),
                },
            )
            .await;
            if let Some((runner, output, io_events, invocation, events)) = runner_launch {
                let state = self.state.clone();
                let (start_tx, start_rx) = tokio::sync::oneshot::channel::<()>();
                let task = tokio::spawn(async move {
                    let _ = start_rx.await;
                    let outcome = runner.run(invocation.clone()).await;
                    if !outcome.output.is_empty() {
                        let count = outcome.output.len() as u32;
                        output.append(&outcome.output).await;
                        append_io_event(&io_events, "output", count).await;
                        events
                            .publish_process_event(invocation.pid, ProcessEvent::Output { count })
                            .await;
                    }
                    {
                        let mut state = state.lock().await;
                        state.runner_tasks.remove(&invocation.pid);
                        state.table.exit(invocation.pid, outcome.exit_code);
                    }
                    events
                        .publish_process_event(
                            invocation.pid,
                            ProcessEvent::Status {
                                status: "exited".to_string(),
                            },
                        )
                        .await;
                });
                {
                    let mut state = self.state.lock().await;
                    state.runner_tasks.insert(committed, task);
                }
                let _ = start_tx.send(());
            }
        }
        Ok(())
    }
}

fn mount_access_at(namespace: &Namespace, path: &str) -> Option<Access> {
    namespace
        .union_at(path)
        .last()
        .map(|resolved| resolved.access)
}

/// A node's stable identity: file kind and a server-unique qid path. The qid
/// *version* is layered on from the process table's generations (see
/// [`State::qid`]); this part never changes for a node.
fn node_identity(node: &Node) -> (FileKind, u64) {
    // Give each per-process file kind its own 2^48 path space keyed by a tag, so
    // qids stay server-unique even after millions of pids (the old 0x1000 stride
    // collided once pids passed 4096).
    fn tagged(tag: u64, pid: Pid) -> u64 {
        (tag << 48) | pid.0
    }
    match node {
        Node::Root => (FileKind::Dir, 0),
        Node::Clone => (FileKind::Clone, 1),
        Node::Proc(p) => (FileKind::Dir, tagged(1, *p)),
        Node::IoDir(p) => (FileKind::Dir, tagged(2, *p)),
        Node::Output(p) => (FileKind::Stream, tagged(3, *p)),
        Node::Status(p) => (FileKind::File, tagged(4, *p)),
        Node::Parent(p) => (FileKind::File, tagged(5, *p)),
        Node::Credentials(p) => (FileKind::File, tagged(6, *p)),
        Node::Exit(p) => (FileKind::File, tagged(7, *p)),
        Node::Ctl(p) => (FileKind::File, tagged(8, *p)),
        Node::NamespaceInfo(p) => (FileKind::File, tagged(9, *p)),
        Node::Input(p) => (FileKind::Stream, tagged(10, *p)),
        Node::IoEvents(p) => (FileKind::Stream, tagged(11, *p)),
        Node::Descriptors(p) => (FileKind::File, tagged(12, *p)),
    }
}

fn is_writable(node: &Node) -> bool {
    matches!(
        node,
        Node::Clone | Node::Ctl(_) | Node::Input(_) | Node::Output(_)
    )
}

fn next_view_id() -> u64 {
    NEXT_PROCFS_VIEW_ID.fetch_add(1, Ordering::Relaxed)
}

fn parse_pid(name: &str) -> Option<Pid> {
    name.parse::<u64>().ok().map(Pid)
}

fn valid_descriptor_path(path: &str) -> bool {
    path.starts_with('/')
        && !path
            .split('/')
            .any(|component| matches!(component, "." | ".."))
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}

async fn append_io_event(events: &Stream, kind: &str, count: u32) {
    let record = format!("{kind}:{count}\n");
    events.append(record.as_bytes()).await;
}
