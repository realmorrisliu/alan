//! alan-agentfs — the agent file server (the `alan-agent-adapter-contract`).
//!
//! In the namespace-native model (`refactor-engine-namespace-native`) the agent
//! process *writes its own state as files* and agentfs is the read-write file
//! backing of that state — not a projector of the legacy `EventEnvelope`
//! alphabet. It serves the `/agent/<pid>` surfaces over aP:
//!
//! ```text
//! io/input     # the shell/parent writes a message; the agent reads it
//! io/output    # the agent appends assistant text; consumers tail it
//! io/events    # aggregate record stream (every surface write appends here)
//! machine/tape # the agent appends the tape (append-only source of truth)
//! machine/status
//! requests/    # clone-via-open: the agent opens a yield; a consumer writes the response
//! actions/     # clone-via-open: the agent records a tool call and its result
//! ```
//!
//! It depends on `alan-ap` only — no `alan-agent-protocol`/`EventEnvelope` on the
//! live path (that alphabet remains only as legacy compatibility transport, ADR-
//! 0025 D4). The engine wiring that drives these writes from a running session is
//! a follow-on slice; here the surfaces are exercised directly over aP.

use std::collections::{BTreeMap, HashMap};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream};
use async_trait::async_trait;
use tokio::sync::Mutex;

/// Cap on a buffered document write (request/action field), so a hostile offset
/// cannot allocate unbounded memory.
const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

#[derive(Default)]
struct Request {
    kind: String,
    prompt: String,
    status: String,
    response: String,
}

#[derive(Default)]
struct Action {
    name: String,
    status: String,
    output: String,
}

struct State {
    input: Stream,
    output: Stream,
    events: Stream,
    tape: Stream,
    requests: BTreeMap<String, Request>,
    actions: BTreeMap<String, Action>,
    next_request: u64,
    next_action: u64,
    fids: HashMap<Fid, AgentFid>,
}

struct AgentFid {
    node: Node,
    mode: Option<OpenMode>,
    /// For a clone fid: the id allocated at open.
    clone_id: Option<String>,
    /// Buffered document for a field write (committed on clunk).
    write_buf: Vec<u8>,
}

impl AgentFid {
    fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            clone_id: None,
            write_buf: Vec::new(),
        }
    }
}

#[derive(Clone)]
enum Node {
    Root,
    IoDir,
    Input,
    Output,
    IoEvents,
    MachineDir,
    Tape,
    Status,
    Events,
    RequestsDir,
    RequestsClone,
    Request(String),
    RequestField(String, &'static str),
    ActionsDir,
    ActionsClone,
    Action(String),
    ActionField(String, &'static str),
    ContextDir,
    ChildrenDir,
}

/// The agent file server.
pub struct AgentFs {
    state: Mutex<State>,
}

impl Default for AgentFs {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentFs {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                input: Stream::new(),
                output: Stream::new(),
                events: Stream::new(),
                tape: Stream::new(),
                requests: BTreeMap::new(),
                actions: BTreeMap::new(),
                next_request: 0,
                next_action: 0,
                fids: HashMap::new(),
            }),
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

    fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match name {
                "io" => Ok(Node::IoDir),
                "machine" => Ok(Node::MachineDir),
                "events" => Ok(Node::Events),
                "requests" => Ok(Node::RequestsDir),
                "actions" => Ok(Node::ActionsDir),
                "context" => Ok(Node::ContextDir),
                "children" => Ok(Node::ChildrenDir),
                _ => Err(ErrorCode::NotFound),
            },
            Node::IoDir => match name {
                "input" => Ok(Node::Input),
                "output" => Ok(Node::Output),
                "events" => Ok(Node::IoEvents),
                _ => Err(ErrorCode::NotFound),
            },
            Node::MachineDir => match name {
                "tape" => Ok(Node::Tape),
                "status" => Ok(Node::Status),
                _ => Err(ErrorCode::NotFound),
            },
            Node::RequestsDir => {
                if name == "clone" {
                    Ok(Node::RequestsClone)
                } else if self.requests.contains_key(name) {
                    Ok(Node::Request(name.to_string()))
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            Node::Request(id) => match name {
                "kind" => Ok(Node::RequestField(id.clone(), "kind")),
                "prompt" => Ok(Node::RequestField(id.clone(), "prompt")),
                "status" => Ok(Node::RequestField(id.clone(), "status")),
                "response" => Ok(Node::RequestField(id.clone(), "response")),
                _ => Err(ErrorCode::NotFound),
            },
            Node::ActionsDir => {
                if name == "clone" {
                    Ok(Node::ActionsClone)
                } else if self.actions.contains_key(name) {
                    Ok(Node::Action(name.to_string()))
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            Node::Action(id) => match name {
                "name" => Ok(Node::ActionField(id.clone(), "name")),
                "status" => Ok(Node::ActionField(id.clone(), "status")),
                "output" => Ok(Node::ActionField(id.clone(), "output")),
                _ => Err(ErrorCode::NotFound),
            },
            // context/ and children/ are agent-layout dirs, empty until the engine
            // projects into them — any child is simply absent for now.
            Node::ContextDir | Node::ChildrenDir => Err(ErrorCode::NotFound),
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => b"io\nmachine\nevents\nrequests\nactions\ncontext\nchildren".to_vec(),
            Node::ContextDir | Node::ChildrenDir => Vec::new(),
            Node::IoDir => b"input\noutput\nevents".to_vec(),
            Node::MachineDir => b"tape\nstatus".to_vec(),
            Node::Status => b"running\n".to_vec(),
            Node::RequestsDir => listing("clone", self.requests.keys()),
            Node::ActionsDir => listing("clone", self.actions.keys()),
            Node::Request(_) => b"kind\nprompt\nstatus\nresponse".to_vec(),
            Node::Action(_) => b"name\nstatus\noutput".to_vec(),
            Node::RequestField(id, field) => {
                let r = self.requests.get(id).ok_or(ErrorCode::NotFound)?;
                match *field {
                    "kind" => &r.kind,
                    "prompt" => &r.prompt,
                    "status" => &r.status,
                    _ => &r.response,
                }
                .clone()
                .into_bytes()
            }
            Node::ActionField(id, field) => {
                let a = self.actions.get(id).ok_or(ErrorCode::NotFound)?;
                match *field {
                    "name" => &a.name,
                    "status" => &a.status,
                    _ => &a.output,
                }
                .clone()
                .into_bytes()
            }
            // Streams are served via stream_for; clone files via the fid's clone_id.
            Node::Input | Node::Output | Node::IoEvents | Node::Tape | Node::Events => {
                return Err(ErrorCode::Unsupported);
            }
            Node::RequestsClone | Node::ActionsClone => return Err(ErrorCode::Unsupported),
        };
        Ok(bytes)
    }

    fn stream_for(&self, node: &Node) -> Option<Stream> {
        match node {
            Node::Output => Some(self.output.clone()),
            Node::Input => Some(self.input.clone()),
            Node::Tape => Some(self.tape.clone()),
            Node::Events | Node::IoEvents => Some(self.events.clone()),
            _ => None,
        }
    }
}

/// A directory listing joining a fixed entry with dynamic ids.
fn listing<'a>(fixed: &str, ids: impl Iterator<Item = &'a String>) -> Vec<u8> {
    let mut names = vec![fixed.to_string()];
    names.extend(ids.cloned());
    names.join("\n").into_bytes()
}

#[async_trait]
impl FileServer for AgentFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = state.node_of(fid)?;
        for name in names {
            node = state.child(&node, name)?;
        }
        let qid = qid_of(&node);
        state.fids.insert(newfid, AgentFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(qid_of(&Node::Root));
        }
        let mut state = self.state.lock().await;
        if state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        let node = state.node_of(fid)?;
        // Dial-time access check: a write-intent open on a read-only node fails at
        // open, not later as Unsupported on write.
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) && !is_writable(&node) {
            return Err(ErrorCode::NoAccess);
        }
        // Clone-via-open: allocate a fresh request/action and remember its id.
        let clone_id = match node {
            Node::RequestsClone => {
                let id = format!("r{}", state.next_request);
                state.next_request += 1;
                state.requests.insert(
                    id.clone(),
                    Request {
                        status: "pending".into(),
                        ..Default::default()
                    },
                );
                Some(id)
            }
            Node::ActionsClone => {
                let id = format!("a{}", state.next_action);
                state.next_action += 1;
                state.actions.insert(
                    id.clone(),
                    Action {
                        status: "running".into(),
                        ..Default::default()
                    },
                );
                Some(id)
            }
            _ => None,
        };
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        f.mode = Some(mode);
        f.clone_id = clone_id;
        Ok(qid_of(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (node, clone_id, stream) = {
            let state = self.state.lock().await;
            // Reading needs read authority from a successful read-open (ROOT, the
            // pre-bound anchor, is always readable).
            if fid != Fid::ROOT {
                let mode = state.fids.get(&fid).and_then(|f| f.mode);
                if !matches!(mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
            }
            let clone_id = state.fids.get(&fid).and_then(|f| f.clone_id.clone());
            let node = state.node_of(fid)?;
            let stream = state.stream_for(&node);
            (node, clone_id, stream)
        };
        // A clone fid reads back the allocated id.
        if let Some(id) = clone_id {
            return Ok(slice(id.into_bytes(), offset, count));
        }
        if let Some(stream) = stream {
            return Ok(stream.read(offset, count).await);
        }
        let state = self.state.lock().await;
        Ok(slice(state.computed_bytes(&node)?, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let has_write = state
            .fids
            .get(&fid)
            .is_some_and(|f| matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)));
        if !has_write {
            return Err(ErrorCode::NoAccess);
        }
        match node {
            // The agent appends assistant output and tape records directly.
            Node::Output | Node::Tape => {
                let stream = state.stream_for(&node).expect("stream node");
                let record = match node {
                    Node::Output => format!("output:{}", data.len()),
                    _ => format!("tape:{}", data.len()),
                };
                drop(state);
                stream.append(data).await;
                self.state
                    .lock()
                    .await
                    .events
                    .append(format!("{record}\n").as_bytes())
                    .await;
                Ok(data.len() as u32)
            }
            // io/input and request/action fields are framed documents: buffer at
            // offset and commit the whole unit on clunk, so a turn never starts on
            // a truncated message (commit-on-clunk).
            Node::Input | Node::RequestField(..) | Node::ActionField(..) => {
                let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
                let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
                let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
                if end > MAX_DOC_BYTES {
                    return Err(ErrorCode::BadRequest);
                }
                if f.write_buf.len() < end {
                    f.write_buf.resize(end, 0);
                }
                f.write_buf[start..end].copy_from_slice(data);
                Ok(data.len() as u32)
            }
            _ => Err(ErrorCode::Unsupported),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let length = match &node {
            Node::Output | Node::Input | Node::Tape | Node::Events | Node::IoEvents => {
                state.stream_for(&node).expect("stream").len().await
            }
            other => state
                .computed_bytes(other)
                .map(|b| b.len() as u64)
                .unwrap_or(0),
        };
        Ok(Stat {
            name: String::new(),
            qid: qid_of(&node),
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
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let mut state = self.state.lock().await;
        let Some(f) = state.fids.remove(&fid) else {
            return Err(ErrorCode::NotFound);
        };
        // Commit a buffered write on clunk.
        if matches!(f.node, Node::Input) && !f.write_buf.is_empty() {
            // The whole message is committed as one framed unit to io/input.
            let input = state.input.clone();
            input.append(&f.write_buf).await;
            state
                .events
                .append(format!("input:{}\n", f.write_buf.len()).as_bytes())
                .await;
        } else if let Node::RequestField(id, field) = &f.node
            && !f.write_buf.is_empty()
        {
            let value = String::from_utf8(f.write_buf).map_err(|_| ErrorCode::BadRequest)?;
            if let Some(r) = state.requests.get_mut(id) {
                match *field {
                    "kind" => r.kind = value,
                    "prompt" => r.prompt = value,
                    "status" => r.status = value,
                    "response" => {
                        r.response = value;
                        // A written response answers the request.
                        r.status = "answered".to_string();
                    }
                    _ => {}
                }
            }
            state
                .events
                .append(format!("request:{id}\n").as_bytes())
                .await;
        } else if let Node::ActionField(id, field) = &f.node
            && !f.write_buf.is_empty()
        {
            let value = String::from_utf8(f.write_buf).map_err(|_| ErrorCode::BadRequest)?;
            if let Some(a) = state.actions.get_mut(id) {
                match *field {
                    "name" => a.name = value,
                    "status" => a.status = value,
                    "output" => a.output = value,
                    _ => {}
                }
            }
            state
                .events
                .append(format!("action:{id}\n").as_bytes())
                .await;
        }
        Ok(())
    }
}

fn qid_of(node: &Node) -> Qid {
    use std::hash::{Hash, Hasher};
    // A stable, server-unique path per node, keyed by its full file identity, so
    // distinct files (and distinct request/action ids) never share a qid.
    fn path_of(key: &str) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        h.finish()
    }
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::IoDir => (FileKind::Dir, "io".into()),
        Node::MachineDir => (FileKind::Dir, "machine".into()),
        Node::RequestsDir => (FileKind::Dir, "requests".into()),
        Node::ActionsDir => (FileKind::Dir, "actions".into()),
        Node::ContextDir => (FileKind::Dir, "context".into()),
        Node::ChildrenDir => (FileKind::Dir, "children".into()),
        Node::Request(id) => (FileKind::Dir, format!("requests/{id}")),
        Node::Action(id) => (FileKind::Dir, format!("actions/{id}")),
        Node::RequestsClone => (FileKind::Clone, "requests/clone".into()),
        Node::ActionsClone => (FileKind::Clone, "actions/clone".into()),
        Node::Input => (FileKind::Stream, "io/input".into()),
        Node::Output => (FileKind::Stream, "io/output".into()),
        Node::IoEvents => (FileKind::Stream, "io/events".into()),
        Node::Tape => (FileKind::Stream, "machine/tape".into()),
        Node::Events => (FileKind::Stream, "events".into()),
        Node::Status => (FileKind::File, "machine/status".into()),
        Node::RequestField(id, field) => (FileKind::File, format!("requests/{id}/{field}")),
        Node::ActionField(id, field) => (FileKind::File, format!("actions/{id}/{field}")),
    };
    Qid {
        kind,
        version: 0,
        path: path_of(&key),
    }
}

fn is_writable(node: &Node) -> bool {
    matches!(
        node,
        Node::Input
            | Node::Output
            | Node::Tape
            | Node::RequestsClone
            | Node::ActionsClone
            | Node::RequestField(..)
            | Node::ActionField(..)
    )
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
