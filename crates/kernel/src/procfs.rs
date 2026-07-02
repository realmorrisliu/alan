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
//!     io/output           # the process's output stream
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

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat, Stream,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{Credentials, ExecSpec, Namespace, Pid, ProcessTable, Status};

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
    Output(Pid),
    NamespaceInfo(Pid),
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
}

impl ProcFid {
    fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            clone_pid: None,
            write_buf: Vec::new(),
        }
    }
}

/// Upper bound on a buffered exec-spec document, so a huge/sparse write offset
/// cannot make `/proc` allocate unbounded memory.
const MAX_EXEC_SPEC_BYTES: usize = 1 << 16; // 64 KiB
const CHILD_PID_PLACEHOLDER: &str = "<child-pid>";

struct State {
    table: ProcessTable,
    fids: HashMap<Fid, ProcFid>,
    /// One output stream per committed process (the kernel owns the file; the
    /// process's execution, in user space, writes to it).
    outputs: HashMap<Pid, Stream>,
}

#[derive(Clone)]
struct SpawnContext {
    parent: Option<Pid>,
    namespace: Namespace,
    credentials: Credentials,
}

/// The `/proc` file server.
#[derive(Clone)]
pub struct ProcFs {
    state: Arc<Mutex<State>>,
    spawn_context: SpawnContext,
    runner: Option<Arc<dyn ProcessRunner>>,
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
                outputs: HashMap::new(),
            })),
            spawn_context: SpawnContext {
                parent: None,
                namespace: Namespace::new(),
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
            spawn_context: SpawnContext {
                parent,
                namespace,
                credentials,
            },
            runner: self.runner.clone(),
        }
    }
}

impl State {
    fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .get(&fid)
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
            Node::Proc(p)
            | Node::IoDir(p)
            | Node::Status(p)
            | Node::Parent(p)
            | Node::Credentials(p)
            | Node::Exit(p)
            | Node::Ctl(p)
            | Node::NamespaceInfo(p) => self.table.generation(*p),
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
                _ => Err(ErrorCode::NotFound),
            },
            Node::IoDir(pid) if name == "output" => Ok(Node::Output(*pid)),
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
            Node::Proc(_) => "status\nparent\ncredentials\nexit\nctl\nio\nnamespace"
                .to_string()
                .into_bytes(),
            Node::IoDir(_) => b"output".to_vec(),
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
                p.namespace
                    .describe()
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
            // `clone`/`ctl` are write surfaces; `output` is a stream served
            // directly in `read`, not here.
            Node::Clone | Node::Ctl(_) | Node::Output(_) => return Err(ErrorCode::Unsupported),
        };
        Ok(bytes)
    }
}

#[async_trait]
impl FileServer for ProcFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        // A fid is a handle to one interaction: never rebind the reserved root or
        // an already-live fid, or a retry/collision would clobber another fid's
        // state (e.g. drop a pending clone pid, leaking the slot).
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = state.node_of(fid)?;
        for name in names {
            node = state.child(&node, name)?;
        }
        let qid = state.qid(&node);
        state.fids.insert(newfid, ProcFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        // The pre-bound root fid is openable directly (to read the listing)
        // without a redundant empty walk, matching SrvFs and the reference server.
        if fid == Fid::ROOT {
            let state = self.state.lock().await;
            return Ok(state.qid(&Node::Root));
        }
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        // Reopening a live fid before clunk is rejected, so a retried open cannot
        // overwrite a pending clone slot (leaking it) or downgrade write intent.
        if state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
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
            let spawn_namespace = self.spawn_context.namespace.clone();
            let proc_template = self.clone();
            let slot = state
                .table
                .clone_begin_with_namespace(parent, credentials, |pid| {
                    let mut child_namespace = spawn_namespace
                        .child_with_path_substitution(CHILD_PID_PLACEHOLDER, &pid.0.to_string());
                    if let Ok(proc_mount) = child_namespace.resolve("/proc") {
                        let child_proc = proc_template.for_spawner(
                            Some(pid),
                            child_namespace.clone(),
                            proc_template.spawn_context.credentials.clone(),
                        );
                        child_namespace.unmount("/proc");
                        child_namespace.mount(
                            "/proc",
                            InProcessTransport::new(Arc::new(child_proc)),
                            proc_mount.access,
                        );
                    }
                    child_namespace
                })
                .ok_or(ErrorCode::Io)?;
            let f = state
                .fids
                .entry(fid)
                .or_insert_with(|| ProcFid::at(Node::Clone));
            f.clone_pid = Some(slot);
            f.mode = Some(mode);
            return Ok(state.qid(&node));
        }
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        f.mode = Some(mode);
        Ok(state.qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        // A process's output is a stream: clone it out and serve a blocking read
        // without holding the state lock (so tailing one process never stalls
        // the whole `/proc`).
        let output = {
            let state = self.state.lock().await;
            // An opened clone fid reads back its pending pid (the allocated name).
            if let Some(f) = state.fids.get(&fid)
                && let Some(pid) = f.clone_pid
            {
                return Ok(slice(pid.0.to_string().into_bytes(), offset, count));
            }
            let node = state.node_of(fid)?;
            match node {
                Node::Output(pid) => state
                    .outputs
                    .get(&pid)
                    .cloned()
                    .ok_or(ErrorCode::NotFound)?,
                other => return Ok(slice(state.file_bytes(&other)?, offset, count)),
            }
        };
        Ok(output.read(offset, count).await)
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        // Write surfaces require write authority established at open.
        let has_write_intent = state
            .fids
            .get(&fid)
            .is_some_and(|f| matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)));
        match node {
            // Exec-spec document for a clone fid: buffer at the given offset until
            // clunk (commit-on-clunk; honor offsets, bound size, reject overflow).
            Node::Clone => {
                if !has_write_intent {
                    return Err(ErrorCode::NoAccess);
                }
                let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
                if f.clone_pid.is_none() {
                    return Err(ErrorCode::BadRequest);
                }
                let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
                let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
                if end > MAX_EXEC_SPEC_BYTES {
                    return Err(ErrorCode::BadRequest);
                }
                if f.write_buf.len() < end {
                    f.write_buf.resize(end, 0);
                }
                f.write_buf[start..end].copy_from_slice(data);
                Ok(data.len() as u32)
            }
            // Generic process control (interrupt/cancel) routes through ctl.
            Node::Ctl(pid) => {
                if !has_write_intent {
                    return Err(ErrorCode::NoAccess);
                }
                match data {
                    b"cancel" | b"interrupt" => state.table.exit(pid, 130),
                    _ => return Err(ErrorCode::BadRequest),
                }
                Ok(data.len() as u32)
            }
            _ => Err(ErrorCode::Unsupported),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = state.node_of(fid)?;
        // Report the readable byte length so clients can size reads; write-only
        // surfaces (clone/ctl) are 0, and a process output is its retained length.
        let length = match &node {
            Node::Output(pid) => match state.outputs.get(pid) {
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
        if fid == Fid::ROOT {
            return Ok(());
        }
        let runner_launch = {
            let mut state = self.state.lock().await;
            let Some(f) = state.fids.remove(&fid) else {
                return Err(ErrorCode::NotFound);
            };
            // Commit-on-clunk: a clone fid commits its pending process here.
            if let Some(pid) = f.clone_pid {
                match serde_json::from_slice::<ExecSpec>(&f.write_buf) {
                    Ok(exec) => {
                        if let Some(namespace_manifest) = exec.namespace.as_ref() {
                            let Some(pending_namespace) = state.table.pending_namespace(pid) else {
                                state.table.discard(pid);
                                return Err(ErrorCode::BadRequest);
                            };
                            if !namespace_manifest.matches_namespace(pending_namespace) {
                                state.table.discard(pid);
                                return Err(ErrorCode::BadRequest);
                            }
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
                        let output = Stream::new();
                        state.outputs.insert(committed, output.clone());
                        self.runner
                            .clone()
                            .map(|runner| (runner, output, invocation))
                    }
                    Err(_) => {
                        // Reject at commit and discard the fid-private slot — it was
                        // never public, so nothing leaks.
                        state.table.discard(pid);
                        return Err(ErrorCode::BadRequest);
                    }
                }
            } else {
                None
            }
        };
        if let Some((runner, output, invocation)) = runner_launch {
            let state = self.state.clone();
            tokio::spawn(async move {
                let outcome = runner.run(invocation.clone()).await;
                if !outcome.output.is_empty() {
                    output.append(&outcome.output).await;
                }
                let mut state = state.lock().await;
                state.table.exit(invocation.pid, outcome.exit_code);
            });
        }
        Ok(())
    }
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
    }
}

fn is_writable(node: &Node) -> bool {
    matches!(node, Node::Clone | Node::Ctl(_))
}

fn parse_pid(name: &str) -> Option<Pid> {
    name.parse::<u64>().ok().map(Pid)
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
