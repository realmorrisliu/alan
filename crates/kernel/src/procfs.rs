//! `/proc` — the synthetic device that renders the process table as files
//! (§7.1) and creates processes via clone-via-open (§7.1a).
//!
//! `/proc` is the single source of truth for processes; any `/agent`-style view
//! is derived from it. Its tree is:
//!
//! ```text
//! /proc
//!   clone                 # open → pending pid; write exec spec; clunk → start
//!   self/                 # current Process view; namespace is the live spawn authority
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

mod file_server;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::task::JoinHandle;

use alan_ap::{
    ErrorCode, Fid, InProcessTransport, Offset, OpenMode, ProcessEvent, ProcessEventSink,
    ProcessEventSource, ProcessInputEventSink, ProcessInputEventSource, ProcessIoEventKind,
    ProcessIoEventSink, ProcessIoEventSource, ProcessOutputEventSink, ProcessOutputEventSource,
    Stream,
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
    SelfProc(Pid),
    SelfNamespace,
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

    fn generation(&self) -> u32 {
        match self {
            Self::Snapshot(_) => 0,
            Self::Live(namespace) => namespace.generation(),
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

fn mount_access_at(namespace: &Namespace, path: &str) -> Option<Access> {
    namespace
        .union_at(path)
        .last()
        .map(|resolved| resolved.access)
}

fn next_view_id() -> u64 {
    NEXT_PROCFS_VIEW_ID.fetch_add(1, Ordering::Relaxed)
}

fn parse_pid(name: &str) -> Option<Pid> {
    name.parse::<u64>().ok().map(Pid)
}
