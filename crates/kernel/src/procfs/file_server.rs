//! aP file-server adapter for the synthetic /proc tree.

use std::sync::Arc;

use super::{Node, ProcFid, ProcFidKey, ProcFs, ProcessInvocation, State, parse_pid};
use crate::{
    CANCELLED_EXIT_CODE, ExecSpec, ExitReason, LiveNamespace, Pid, ProcessExit, ProcessWaitRecord,
    Status,
};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, ProcessEvent, Qid, Stat, Stream,
};
use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

const MAX_EXEC_SPEC_BYTES: usize = 1 << 16; // 64 KiB
const MAX_PROCESS_INPUT_BYTES: usize = 1 << 20; // 1 MiB

impl ProcFs {
    fn fid_key(&self, fid: Fid) -> ProcFidKey {
        ProcFidKey {
            view_id: self.view_id,
            fid,
        }
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
    fn qid(&self, node: &Node, view_id: u64, namespace_source: &super::NamespaceSource) -> Qid {
        let (kind, path) = node_identity(node);
        let version = match node {
            Node::Root => self.table.listing_generation(),
            Node::Clone | Node::Output(_) => 0,
            Node::Input(_) | Node::IoEvents(_) | Node::Wait(_) => 0,
            Node::SelfProc(p)
            | Node::Proc(p)
            | Node::IoDir(p)
            | Node::Status(p)
            | Node::Parent(p)
            | Node::Credentials(p)
            | Node::Exit(p)
            | Node::Ctl(p) => self.table.generation(*p),
            Node::NamespaceInfo(p) => self
                .live_namespaces
                .get(p)
                .map_or(0, LiveNamespace::generation),
            Node::SelfNamespace => namespace_source.generation(),
            Node::Descriptors(p) => self.table.generation(*p),
        };
        let path = match node {
            Node::SelfProc(_) => tagged_identity(13, view_id),
            Node::SelfNamespace => tagged_identity(14, view_id),
            _ => path,
        };
        Qid {
            kind,
            version,
            path,
        }
    }

    /// Resolve one path component from a node to its child node.
    fn child(&self, node: &Node, name: &str, current_pid: Option<Pid>) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => {
                if name == "clone" {
                    Ok(Node::Clone)
                } else if name == "self" {
                    current_pid
                        .filter(|pid| self.table.get(*pid).is_some())
                        .map(Node::SelfProc)
                        .ok_or(ErrorCode::NotFound)
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
                "wait" => Ok(Node::Wait(*pid)),
                "ctl" => Ok(Node::Ctl(*pid)),
                "io" => Ok(Node::IoDir(*pid)),
                "namespace" => Ok(Node::NamespaceInfo(*pid)),
                "descriptors" => Ok(Node::Descriptors(*pid)),
                _ => Err(ErrorCode::NotFound),
            },
            Node::SelfProc(pid) => match name {
                "status" => Ok(Node::Status(*pid)),
                "parent" => Ok(Node::Parent(*pid)),
                "credentials" => Ok(Node::Credentials(*pid)),
                "exit" => Ok(Node::Exit(*pid)),
                "wait" => Ok(Node::Wait(*pid)),
                "ctl" => Ok(Node::Ctl(*pid)),
                "io" => Ok(Node::IoDir(*pid)),
                "namespace" => Ok(Node::SelfNamespace),
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
    fn file_bytes(
        &self,
        node: &Node,
        current_pid: Option<Pid>,
        namespace_source: &super::NamespaceSource,
    ) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => {
                let mut names = vec!["clone".to_string()];
                if current_pid.is_some_and(|pid| self.table.get(pid).is_some()) {
                    names.push("self".to_string());
                }
                names.extend(self.table.list().iter().map(|p| p.0.to_string()));
                names.join("\n").into_bytes()
            }
            Node::SelfProc(_) | Node::Proc(_) => {
                "status\nparent\ncredentials\nexit\nwait\nctl\nio\nnamespace\ndescriptors"
                    .to_string()
                    .into_bytes()
            }
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
            Node::SelfNamespace => namespace_bytes(&namespace_source.snapshot()),
            Node::Descriptors(pid) => {
                let process = self.table.get(*pid).ok_or(ErrorCode::NotFound)?;
                serde_json::to_vec(&process.exec.descriptors).map_err(|_| ErrorCode::Io)?
            }
            // `clone`/`ctl` are write surfaces; IO and wait streams are served
            // directly in `read`, not here.
            Node::Clone
            | Node::Ctl(_)
            | Node::Input(_)
            | Node::Output(_)
            | Node::IoEvents(_)
            | Node::Wait(_) => {
                return Err(ErrorCode::Unsupported);
            }
        };
        Ok(bytes)
    }
}

enum ProcStreamWrite {
    Output {
        pid: Pid,
        io_lock: Arc<Mutex<()>>,
        stream: Stream,
        events: Stream,
    },
    Exits {
        exits: Vec<ProcessExit>,
        tasks_to_abort: Vec<JoinHandle<()>>,
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
            node = state.child(&node, name, self.spawn_context.parent)?;
        }
        let qid = state.qid(&node, self.view_id, &self.spawn_context.namespace);
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
            if is_write_intent(mode) {
                return Err(ErrorCode::NoAccess);
            }
            return Ok(state.qid(&node, self.view_id, &self.spawn_context.namespace));
        }
        // Reopening a live fid before clunk is rejected, so a retried open cannot
        // overwrite a pending clone slot (leaking it) or downgrade write intent.
        if state.fids.get(&fid_key).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        // Dial-time access check: reject any write-intent open on read-only
        // `/proc` nodes (including `wait`, `io/events`, `status`, `exit`,
        // `parent`, `credentials`, `namespace`, `descriptors`, and directories).
        if is_write_intent(mode) && !is_writable(&node) {
            return Err(ErrorCode::NoAccess);
        }
        // Clone-via-open: spawning requires write intent (you write the exec
        // spec), so a read-only open cannot allocate an uncommittable — and thus
        // leaked — pending slot.
        if matches!(node, Node::Clone) {
            if !is_write_intent(mode) {
                return Err(ErrorCode::NoAccess);
            }
            let parent = self.spawn_context.parent;
            let credentials = self.spawn_context.credentials.clone();
            let proc_template = self.clone();
            let slot = state
                .table
                .clone_begin_with_namespace_generation(parent, credentials, |pid| {
                    proc_template.child_namespace_for_spawn(pid)
                })
                .ok_or(ErrorCode::Io)?;
            let f = state
                .fids
                .entry(fid_key)
                .or_insert_with(|| ProcFid::at(Node::Clone));
            f.clone_pid = Some(slot);
            f.mode = Some(mode);
            return Ok(state.qid(&node, self.view_id, &self.spawn_context.namespace));
        }
        let f = state.fids.get_mut(&fid_key).ok_or(ErrorCode::NotFound)?;
        f.mode = Some(mode);
        Ok(state.qid(&node, self.view_id, &self.spawn_context.namespace))
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
                Node::Wait(pid) => state
                    .wait_streams
                    .get(&pid)
                    .cloned()
                    .ok_or(ErrorCode::NotFound)?,
                other => {
                    return Ok(slice(
                        state.file_bytes(
                            &other,
                            self.spawn_context.parent,
                            &self.spawn_context.namespace,
                        )?,
                        offset,
                        count,
                    ));
                }
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
                .and_then(|f| f.mode)
                .is_some_and(is_write_intent);
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
                            let (exits, tasks_to_abort) = state.apply_exit_locked(
                                pid,
                                CANCELLED_EXIT_CODE,
                                ExitReason::Cancelled,
                                true,
                            );
                            ProcStreamWrite::Exits {
                                exits,
                                tasks_to_abort,
                            }
                        }
                        _ => return Err(ErrorCode::BadRequest),
                    }
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
                    let stream = state
                        .outputs
                        .get(&pid)
                        .cloned()
                        .ok_or(ErrorCode::NotFound)?;
                    let events = state
                        .io_events
                        .get(&pid)
                        .cloned()
                        .ok_or(ErrorCode::NotFound)?;
                    let io_lock = state
                        .io_locks
                        .entry(pid)
                        .or_insert_with(|| Arc::new(Mutex::new(())))
                        .clone();
                    ProcStreamWrite::Output {
                        pid,
                        io_lock,
                        stream,
                        events,
                    }
                }
                _ => return Err(ErrorCode::Unsupported),
            }
        };
        let count = data.len() as u32;
        match stream_write {
            ProcStreamWrite::Output {
                pid,
                io_lock,
                stream,
                events,
            } => {
                self.commit_io_output(pid, io_lock, stream, events, data, count)
                    .await;
            }
            ProcStreamWrite::Exits {
                exits,
                tasks_to_abort,
            } => {
                self.finish_exits(exits, tasks_to_abort).await;
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
            Node::Input(pid) => state.inputs.get(pid).map_or(0, Stream::len_sync),
            Node::Output(pid) => state.outputs.get(pid).map_or(0, Stream::len_sync),
            Node::IoEvents(pid) => state.io_events.get(pid).map_or(0, Stream::len_sync),
            Node::Wait(pid) => state.wait_streams.get(pid).map_or(0, Stream::len_sync),
            Node::Clone | Node::Ctl(_) => 0,
            other => state
                .file_bytes(
                    other,
                    self.spawn_context.parent,
                    &self.spawn_context.namespace,
                )
                .map(|b| b.len() as u64)
                .unwrap_or(0),
        };
        Ok(Stat {
            name: String::new(),
            qid: state.qid(&node, self.view_id, &self.spawn_context.namespace),
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
                        if state.table.pending_namespace_generation(pid)
                            != Some(exec.namespace.generation)
                        {
                            state.table.discard(pid);
                            return Err(ErrorCode::BadRequest);
                        }
                        let Some(mut narrowed_namespace) = ({
                            let Some(pending_namespace) = state.table.pending_namespace(pid) else {
                                state.table.discard(pid);
                                return Err(ErrorCode::BadRequest);
                            };
                            exec.namespace.namespace_subset_from(pending_namespace)
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
                        let immediate_exit =
                            (process.status == Status::Exited).then(|| ProcessExit {
                                pid: committed,
                                exit_code: process.exit_code.unwrap_or(CANCELLED_EXIT_CODE),
                                reason: process.exit_reason.unwrap_or(ExitReason::Cancelled),
                            });
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
                        let wait_stream = Stream::new();
                        if let Some(exit) = immediate_exit {
                            wait_stream
                                .append_sync(&ProcessWaitRecord::from_exit(exit).to_json_line());
                        }
                        let io_lock = Arc::new(Mutex::new(()));
                        state.inputs.insert(committed, input);
                        state.outputs.insert(committed, output.clone());
                        state.io_events.insert(committed, io_events.clone());
                        state.wait_streams.insert(committed, wait_stream);
                        state.io_locks.insert(committed, io_lock.clone());
                        state.event_history.insert(committed, Vec::new());
                        let runner_launch = if immediate_exit.is_some() {
                            None
                        } else {
                            self.runner.clone().map(|runner| {
                                (runner, io_lock, output, io_events, invocation, self.clone())
                            })
                        };
                        (
                            Some((committed, immediate_exit.is_some(), runner_launch)),
                            None,
                        )
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
                let io_lock = state
                    .io_locks
                    .entry(pid)
                    .or_insert_with(|| Arc::new(Mutex::new(())))
                    .clone();
                let count = f.write_buf.len() as u32;
                let mut framed = format!("{}\n", f.write_buf.len()).into_bytes();
                framed.extend_from_slice(&f.write_buf);
                (None, Some((pid, io_lock, stream, events, framed, count)))
            } else {
                (None, None)
            }
        };
        if let Some((pid, io_lock, stream, events, framed, count)) = committed_input {
            self.commit_io_input(pid, io_lock, stream, events, &framed, count)
                .await;
        }
        if let Some((committed, immediate_exit, runner_launch)) = committed_process {
            if immediate_exit {
                self.publish_process_event(
                    committed,
                    ProcessEvent::Status {
                        status: "exited".to_string(),
                    },
                )
                .await;
            } else {
                self.publish_process_event(
                    committed,
                    ProcessEvent::Status {
                        status: "running".to_string(),
                    },
                )
                .await;
                if let Some((runner, io_lock, output, io_events, invocation, events)) =
                    runner_launch
                {
                    let (start_tx, start_rx) = tokio::sync::oneshot::channel::<()>();
                    let task = tokio::spawn(async move {
                        let _ = start_rx.await;
                        let outcome = runner.run(invocation.clone()).await;
                        if !outcome.output.is_empty() {
                            let count = outcome.output.len() as u32;
                            events
                                .commit_io_output(
                                    invocation.pid,
                                    io_lock,
                                    output,
                                    io_events,
                                    &outcome.output,
                                    count,
                                )
                                .await;
                        }
                        events
                            .finalize_exit(
                                invocation.pid,
                                outcome.exit_code,
                                ExitReason::from_exit_code(outcome.exit_code),
                                false,
                            )
                            .await;
                    });
                    let should_start = {
                        let mut state = self.state.lock().await;
                        if state
                            .table
                            .get(committed)
                            .is_some_and(|process| process.status != Status::Exited)
                        {
                            state.runner_tasks.insert(committed, task);
                            true
                        } else {
                            task.abort();
                            false
                        }
                    };
                    if should_start {
                        let _ = start_tx.send(());
                    }
                }
            }
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
    match node {
        Node::Root => (FileKind::Dir, 0),
        Node::Clone => (FileKind::Clone, 1),
        Node::SelfProc(_) => (FileKind::Dir, 0),
        Node::SelfNamespace => (FileKind::File, 0),
        Node::Proc(p) => (FileKind::Dir, tagged_identity(1, p.0)),
        Node::IoDir(p) => (FileKind::Dir, tagged_identity(2, p.0)),
        Node::Output(p) => (FileKind::Stream, tagged_identity(3, p.0)),
        Node::Status(p) => (FileKind::File, tagged_identity(4, p.0)),
        Node::Parent(p) => (FileKind::File, tagged_identity(5, p.0)),
        Node::Credentials(p) => (FileKind::File, tagged_identity(6, p.0)),
        Node::Exit(p) => (FileKind::File, tagged_identity(7, p.0)),
        Node::Ctl(p) => (FileKind::File, tagged_identity(8, p.0)),
        Node::NamespaceInfo(p) => (FileKind::File, tagged_identity(9, p.0)),
        Node::Input(p) => (FileKind::Stream, tagged_identity(10, p.0)),
        Node::IoEvents(p) => (FileKind::Stream, tagged_identity(11, p.0)),
        Node::Descriptors(p) => (FileKind::File, tagged_identity(12, p.0)),
        Node::Wait(p) => (FileKind::Stream, tagged_identity(15, p.0)),
    }
}

fn tagged_identity(tag: u64, id: u64) -> u64 {
    (tag << 48) | id
}

fn namespace_bytes(namespace: &crate::Namespace) -> Vec<u8> {
    namespace
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

fn is_writable(node: &Node) -> bool {
    matches!(
        node,
        Node::Clone | Node::Ctl(_) | Node::Input(_) | Node::Output(_)
    )
}

fn is_write_intent(mode: OpenMode) -> bool {
    matches!(
        mode,
        OpenMode::Write | OpenMode::WriteExclusive | OpenMode::ReadWrite
    )
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
