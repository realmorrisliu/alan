//! alan-agentfs — the projection file server (the `alan-agent-adapter-contract`).
//!
//! It maps the Agent Execution Engine's session/event stream onto the agent file
//! layout served at `/agent/<pid>`: assistant text → `io/output`, every event →
//! the aggregate `events` stream and `io/events`, tool calls → `actions/<id>/`,
//! yields → `requests/<id>/`, and the tape → `machine/tape`. It is a user-space
//! file server above the kernel (ADR-0025 D3): `alan-kernel` never learns about
//! agents; this crate carries the agent-runtime knowledge.
//!
//! This slice projects the engine's [`EventEnvelope`] alphabet, the source of
//! truth for the agent surfaces, so it is driven by [`AgentFs::ingest`] and is
//! exercised with synthetic envelopes — no live LLM. Wiring a live session loop
//! (feeding `ingest` from a running engine), the `io/input` resume path, the
//! `/agent` overlay union with `/proc`, and the TUI migration are follow-on
//! slices of `introduce-alan-kernel-runtime`.

use std::collections::{BTreeMap, HashMap};

use alan_agent_protocol::{Event, EventEnvelope, YieldKind};
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream};
use async_trait::async_trait;
use tokio::sync::Mutex;

/// The projected state of one agent process's surfaces.
struct Projection {
    /// Assistant text output (`io/output`).
    output: Stream,
    /// The aggregate, watchable event stream (`events`) and `io/events`. One
    /// retained log feeds both names.
    events: Stream,
    /// Append-only tape source of truth (`machine/tape`).
    tape: Stream,
    /// `requests/<id>/` — projected yields.
    requests: BTreeMap<String, RequestProj>,
    /// `actions/<id>/` — projected tool calls.
    actions: BTreeMap<String, ActionProj>,
}

struct RequestProj {
    kind: String,
    status: String,
    payload: String,
}

struct ActionProj {
    name: String,
    status: String,
}

impl Projection {
    fn new() -> Self {
        Self {
            output: Stream::new(),
            events: Stream::new(),
            tape: Stream::new(),
            requests: BTreeMap::new(),
            actions: BTreeMap::new(),
        }
    }
}

/// What a fid points at within an agent's `/agent/<pid>` surface tree.
#[derive(Clone)]
enum Node {
    Root,
    IoDir,
    Output,
    IoEvents,
    MachineDir,
    Tape,
    Status,
    Events,
    RequestsDir,
    Request(String),
    RequestField(String, &'static str),
    ActionsDir,
    Action(String),
    ActionField(String, &'static str),
}

/// The agent-runtime projection file server.
pub struct AgentFs {
    state: Mutex<Projection>,
    fids: Mutex<HashMap<Fid, Node>>,
}

impl Default for AgentFs {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentFs {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(Projection::new()),
            fids: Mutex::new(HashMap::new()),
        }
    }

    /// Project one engine event into the agent file layout. This is the seam a
    /// live session loop feeds; the envelope is the source of truth.
    pub async fn ingest(&self, env: EventEnvelope) {
        let record = serde_json::to_string(&env).unwrap_or_default();
        let mut state = self.state.lock().await;
        // Every event joins the aggregate stream and the tape.
        state.events.append(format!("{record}\n").as_bytes()).await;
        state.tape.append(format!("{record}\n").as_bytes()).await;

        match env.event {
            Event::TextDelta { chunk, .. } => {
                state.output.append(chunk.as_bytes()).await;
            }
            Event::ToolCallStarted { id, name, .. } => {
                state.actions.insert(
                    id,
                    ActionProj {
                        name,
                        status: "running".to_string(),
                    },
                );
            }
            Event::ToolCallCompleted {
                id, name, success, ..
            } => {
                let status = match success {
                    Some(true) | None => "completed",
                    Some(false) => "failed",
                };
                let entry = state.actions.entry(id).or_insert_with(|| ActionProj {
                    name: String::new(),
                    status: String::new(),
                });
                entry.status = status.to_string();
                if let Some(name) = name {
                    entry.name = name;
                }
            }
            Event::Yield {
                request_id,
                kind,
                payload,
            } => {
                state.requests.insert(
                    request_id,
                    RequestProj {
                        kind: yield_kind_name(&kind),
                        status: "pending".to_string(),
                        payload: payload.to_string(),
                    },
                );
            }
            _ => {}
        }
    }

    async fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .lock()
            .await
            .get(&fid)
            .cloned()
            .ok_or(ErrorCode::NotFound)
    }

    /// Resolve one path component from a node to its child.
    async fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        let state = self.state.lock().await;
        match node {
            Node::Root => match name {
                "io" => Ok(Node::IoDir),
                "machine" => Ok(Node::MachineDir),
                "status" => Ok(Node::Status),
                "events" => Ok(Node::Events),
                "requests" => Ok(Node::RequestsDir),
                "actions" => Ok(Node::ActionsDir),
                _ => Err(ErrorCode::NotFound),
            },
            Node::IoDir => match name {
                "output" => Ok(Node::Output),
                "events" => Ok(Node::IoEvents),
                _ => Err(ErrorCode::NotFound),
            },
            Node::MachineDir if name == "tape" => Ok(Node::Tape),
            Node::RequestsDir if state.requests.contains_key(name) => {
                Ok(Node::Request(name.to_string()))
            }
            Node::Request(id) => match name {
                "kind" => Ok(Node::RequestField(id.clone(), "kind")),
                "status" => Ok(Node::RequestField(id.clone(), "status")),
                "payload" => Ok(Node::RequestField(id.clone(), "payload")),
                _ => Err(ErrorCode::NotFound),
            },
            Node::ActionsDir if state.actions.contains_key(name) => {
                Ok(Node::Action(name.to_string()))
            }
            Node::Action(id) => match name {
                "name" => Ok(Node::ActionField(id.clone(), "name")),
                "status" => Ok(Node::ActionField(id.clone(), "status")),
                _ => Err(ErrorCode::NotFound),
            },
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    /// The stream backing a stream-kind node, cloned out so reads can block
    /// without holding the projection lock.
    async fn stream_for(&self, node: &Node) -> Option<Stream> {
        let state = self.state.lock().await;
        match node {
            Node::Output => Some(state.output.clone()),
            Node::Events | Node::IoEvents => Some(state.events.clone()),
            Node::Tape => Some(state.tape.clone()),
            _ => None,
        }
    }

    /// The computed bytes of a non-stream node (directory listings and metadata).
    async fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        let bytes = match node {
            Node::Root => b"io\nmachine\nstatus\nevents\nrequests\nactions".to_vec(),
            Node::IoDir => b"input\noutput\nevents".to_vec(),
            Node::MachineDir => b"tape\nstatus\nctl".to_vec(),
            Node::Status => b"running\n".to_vec(),
            Node::RequestsDir => state
                .requests
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes(),
            Node::ActionsDir => state
                .actions
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes(),
            Node::Request(_) => b"kind\nstatus\npayload".to_vec(),
            Node::Action(_) => b"name\nstatus".to_vec(),
            Node::RequestField(id, field) => {
                let r = state.requests.get(id).ok_or(ErrorCode::NotFound)?;
                match *field {
                    "kind" => r.kind.clone(),
                    "status" => r.status.clone(),
                    _ => r.payload.clone(),
                }
                .into_bytes()
            }
            Node::ActionField(id, field) => {
                let a = state.actions.get(id).ok_or(ErrorCode::NotFound)?;
                match *field {
                    "name" => a.name.clone(),
                    _ => a.status.clone(),
                }
                .into_bytes()
            }
            // Stream nodes are served by `stream_for`, not here.
            Node::Output | Node::IoEvents | Node::Tape | Node::Events => {
                return Err(ErrorCode::Unsupported);
            }
        };
        Ok(bytes)
    }
}

#[async_trait]
impl FileServer for AgentFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut node = self.node_of(fid).await?;
        for name in names {
            node = self.child(&node, name).await?;
        }
        let qid = qid_of(&node);
        self.fids.lock().await.insert(newfid, node);
        Ok(qid)
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(qid_of(&self.node_of(fid).await?))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let node = self.node_of(fid).await?;
        if let Some(stream) = self.stream_for(&node).await {
            return Ok(stream.read(offset, count).await);
        }
        let bytes = self.computed_bytes(&node).await?;
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        // io/input, requests/<id>/response, and machine/ctl writes are the
        // resume/control path, projected in a follow-on slice.
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_of(fid).await?;
        Ok(Stat {
            name: String::new(),
            qid: qid_of(&node),
            length: 0,
            writable: false,
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
        self.fids.lock().await.remove(&fid);
        Ok(())
    }
}

fn qid_of(node: &Node) -> Qid {
    let (kind, path) = match node {
        Node::Root | Node::IoDir | Node::MachineDir | Node::RequestsDir | Node::ActionsDir => {
            (FileKind::Dir, 0)
        }
        Node::Request(_) | Node::Action(_) => (FileKind::Dir, 1),
        Node::Output | Node::IoEvents | Node::Tape | Node::Events => (FileKind::Stream, 2),
        Node::Status | Node::RequestField(..) | Node::ActionField(..) => (FileKind::File, 3),
    };
    Qid {
        kind,
        version: 0,
        path,
    }
}

fn yield_kind_name(kind: &YieldKind) -> String {
    match kind {
        YieldKind::Confirmation => "confirmation".to_string(),
        YieldKind::StructuredInput => "structured_input".to_string(),
        YieldKind::DynamicTool => "dynamic_tool".to_string(),
        YieldKind::Custom(s) => s.clone(),
    }
}
