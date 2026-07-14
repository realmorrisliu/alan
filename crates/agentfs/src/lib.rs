//! alan-agentfs — the agent file server (the `alan-agent-adapter-contract`).
//!
//! In the namespace-native model (`refactor-engine-namespace-native`) the agent
//! process *writes its own state as files* and agentfs is the read-write file
//! backing of that state — not a projector of the legacy `EventEnvelope`
//! alphabet. It serves the `/agent/<pid>` surfaces over aP:
//!
//! ```text
//! io/input     # the shell/parent writes a message; the agent reads it. Each
//!              # committed message is length-framed (`<len>\n<payload>`) so
//!              # consecutive messages keep distinct boundaries in the stream
//! io/output    # the agent appends assistant text; consumers tail it
//! io/events    # aggregate record stream (every surface write appends here)
//! machine/tape # the agent appends the tape (append-only source of truth)
//! machine/status # read-only run-state
//! machine/ctl  # agent-runtime control: compact/rollback (engine-owned semantics)
//! machine/ui   # renderer-visible runtime UI snapshots + watchable UI event stream
//! requests/    # clone-via-open: the agent opens a yield; a consumer answers by
//!              # writing `response` (committed on clunk), which settles it
//! actions/     # clone-via-open: the agent records a tool call and its result
//! ```
//!
//! Surfaces follow `define-agent-file-layout-contract`: generic process control
//! (interrupt/cancel) is the kernel's `/proc/<pid>/ctl`, while `machine/ctl`
//! carries agent-runtime tape/checkpoint commands. A response written to a request
//! that is already terminal is rejected (request-status integrity).
//!
//! It depends on `alan-ap` only — no `alan-agent-protocol`/`EventEnvelope` on the
//! live path (that alphabet remains only as legacy compatibility transport, ADR-
//! 0025 D4). [`AgentRootFs`] is the thin `/agent` view over these per-agent file
//! servers: it derives visible pids from `/proc` and forwards `/agent/<pid>` or
//! `/agent/root` to the corresponding [`AgentFs`] backing tree.

mod conformance;
mod root;

use std::collections::{BTreeMap, HashMap};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream, VersionTable,
};
use alan_knowledge::{ContentHash, KnowledgeError, KnowledgeStore, RetentionPolicy, RootAccess};
use async_trait::async_trait;
use tokio::sync::Mutex;

pub use conformance::{AgentConformanceChecker, ConformanceIssue, ConformanceReport};
pub use root::AgentRootFs;

/// Cap on a buffered document write (request/action field), so a hostile offset
/// cannot allocate unbounded memory.
const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB
// Action output is the durable evidence body and can be materially larger than
// control-plane documents. Keep a separate bound so model/tool output above the
// generic document cap remains referenceable without making every AgentFS
// document equally large.
const MAX_ACTION_OUTPUT_BYTES: usize = 16 << 20; // 16 MiB

/// In-band help for `machine/ctl`: reading the control file lists its accepted
/// commands so a namespace-native client need not consult external docs
/// (self-describing namespace). Semantics belong to the engine; this is the
/// documented vocabulary, not an exhaustive parser.
const MACHINE_CTL_HELP: &str = "\
# machine/ctl — agent-runtime control. Write one command per write.
compact   compact the tape into a checkpoint
rollback  roll back to the previous checkpoint
interrupt stop the current turn; the agent process stays alive
";
const ACTIONS_HELP: &str = "\
# actions — Tool effect records and durable evidence.
clone                         open read-write to allocate an action id
events                        watch action creation and field changes
<id>/output                   full durable output; append-only for a retained record
<id>/result                   structured completion metadata
evidence_projection           tape record: preview + namespace reference + truncation
reference                     {path, offset?, length?}; resolve by namespace walk
evidence_retention_expired    structured output record after storing-server retention expiry
[REDACTED reason=<class>]      durable redaction marker; distinct from truncation
";

const TAPE_ROOT_NAME: &str = "machine/tape";
const DEFAULT_UI_ACTIVITY: &str = r#"{"version":1,"state":"idle"}"#;
const DEFAULT_UI_PLAN: &str = r#"{"version":1,"items":[]}"#;
const DEFAULT_UI_THINKING: &str = r#"{"version":1,"state":"idle","text":""}"#;
const DEFAULT_UI_NOTICE: &str = r#"{"version":1,"kind":"none","message":""}"#;

#[derive(Default)]
struct Request {
    kind: String,
    prompt: String,
    options: String,
    status: String,
    response: String,
}

#[derive(Default)]
struct Action {
    name: String,
    status: String,
    output: String,
    output_root: Option<ContentHash>,
    output_retention_expired: bool,
    /// The tool's structured result (agent-file-layout-contract).
    result: String,
    /// The action's approval state.
    approval: String,
    /// A reference to the tool process in `/proc` (not a copy of its state).
    process: String,
}

struct State {
    input: Stream,
    output: Stream,
    /// The aggregate, watchable record stream (`events`): every surface write.
    events: Stream,
    /// IO-scoped lifecycle stream (`io/events`): only io/input and io/output.
    io_events: Stream,
    /// Per-container notification streams: a new request/action or field change.
    request_events: Stream,
    action_events: Stream,
    ui_events: Stream,
    tape: Stream,
    knowledge: KnowledgeStore,
    tape_root: ContentHash,
    requests: BTreeMap<String, Request>,
    actions: BTreeMap<String, Action>,
    /// Agent run-state (machine/status): read-only over aP, transitioned only by
    /// lifecycle verbs on machine/ctl (D7).
    status: String,
    ui_activity: String,
    ui_plan: String,
    ui_thinking: String,
    ui_notice: String,
    next_request: u64,
    next_action: u64,
    /// The fid currently holding the exclusive-write lease on `machine/tape`, if
    /// any; a second write-open of the tape is refused while this is set.
    tape_writer: Option<Fid>,
    /// Per-node qid versions, keyed by the node's qid path; bumped when a
    /// directory listing or flat file's content changes (streams are versioned
    /// by read offset, not qid version, so they are not tracked here).
    versions: VersionTable,
    fids: HashMap<Fid, AgentFid>,
}

struct AgentFid {
    node: Node,
    mode: Option<OpenMode>,
    /// For a clone fid: the id allocated at open.
    clone_id: Option<String>,
    /// Buffered document for a field write (committed on clunk).
    write_buf: Vec<u8>,
    /// Whether a write was issued on this fid (even a zero-byte one). The commit
    /// trigger is write intent, not a non-empty buffer, so an intentionally empty
    /// answer (`requests/<id>/response`) still settles the request.
    wrote: bool,
}

impl AgentFid {
    fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            clone_id: None,
            write_buf: Vec::new(),
            wrote: false,
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
    /// The agent-runtime control surface (`machine/ctl`): text commands such as
    /// `compact` / `rollback` whose tape/checkpoint semantics belong to the engine
    /// (agent-file-layout-contract). Generic process control (interrupt/cancel)
    /// is the kernel's `/proc/<pid>/ctl`, not here.
    MachineCtl,
    UiDir,
    UiActivity,
    UiPlan,
    UiThinking,
    UiNotice,
    UiEvents,
    CheckpointsDir,
    CurrentCheckpoint,
    Events,
    RequestsDir,
    RequestsClone,
    RequestsEvents,
    Request(String),
    RequestField(String, &'static str),
    ActionsDir,
    ActionsHelp,
    ActionsClone,
    ActionsEvents,
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
        let (knowledge, tape_root) = initial_tape_knowledge();
        Self {
            state: Mutex::new(State {
                input: Stream::new(),
                output: Stream::new(),
                events: Stream::new(),
                io_events: Stream::new(),
                request_events: Stream::new(),
                action_events: Stream::new(),
                ui_events: Stream::new(),
                tape: Stream::new(),
                knowledge,
                tape_root,
                requests: BTreeMap::new(),
                actions: BTreeMap::new(),
                status: "running".to_string(),
                ui_activity: DEFAULT_UI_ACTIVITY.to_string(),
                ui_plan: DEFAULT_UI_PLAN.to_string(),
                ui_thinking: DEFAULT_UI_THINKING.to_string(),
                ui_notice: DEFAULT_UI_NOTICE.to_string(),
                next_request: 0,
                next_action: 0,
                tape_writer: None,
                versions: VersionTable::new(),
                fids: HashMap::new(),
            }),
        }
    }

    /// Current content-addressed checkpoint root for `machine/tape`.
    pub async fn current_tape_checkpoint(&self) -> ContentHash {
        self.state.lock().await.tape_root.clone()
    }

    /// Verify and materialize the current `machine/tape` checkpoint root.
    pub async fn materialize_tape_checkpoint(&self) -> Result<Vec<u8>, ErrorCode> {
        self.state.lock().await.materialized_tape()
    }

    /// Verify the current `machine/tape` root without materializing bytes.
    pub async fn verify_tape_checkpoint(&self) -> Result<(), ErrorCode> {
        let state = self.state.lock().await;
        state
            .knowledge
            .verify_root_hash(&state.tape_root)
            .map_err(map_knowledge_error)
    }

    /// Apply the storing server's retention policy to one action output.
    ///
    /// Readers keep the namespace path and receive an in-band structured expiry
    /// record instead of silently shifted or missing bytes.
    pub async fn expire_action_output_for_retention(
        &self,
        action_id: &str,
        cause: &str,
    ) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        let root_name = action_output_root_name(action_id);
        state
            .knowledge
            .unbind_root(&root_name)
            .map_err(map_knowledge_error)?;
        state
            .knowledge
            .collect_garbage(RetentionPolicy::CollectUnreachable);
        let action = state
            .actions
            .get_mut(action_id)
            .ok_or(ErrorCode::NotFound)?;
        action.output_root = None;
        action.output_retention_expired = true;
        action.output = retention_expired_record(&format!("/actions/{action_id}/output"), cause);
        state.bump(&Node::ActionField(action_id.to_string(), "output"));
        Ok(())
    }

    pub(crate) async fn append_child_event(&self, child_pid: &str) {
        let state = self.state.lock().await;
        state
            .events
            .append(format!("child:{child_pid}\n").as_bytes())
            .await;
    }

    pub(crate) async fn append_output_event(&self, count: u32) {
        let state = self.state.lock().await;
        let record = format!("output:{count}\n");
        state.io_events.append(record.as_bytes()).await;
        state.events.append(record.as_bytes()).await;
    }

    pub(crate) async fn append_input_event(&self, count: u32) {
        let state = self.state.lock().await;
        let record = format!("input:{count}\n");
        state.io_events.append(record.as_bytes()).await;
        state.events.append(record.as_bytes()).await;
    }

    pub(crate) async fn append_status_event(&self, status: &str) {
        let state = self.state.lock().await;
        state
            .events
            .append(format!("status:{status}\n").as_bytes())
            .await;
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

    /// The qid for `node`, with its current version from the table.
    fn qid(&self, node: &Node) -> Qid {
        let (kind, path) = node_identity(node);
        Qid {
            kind,
            version: self.versions.get(path),
            path,
        }
    }

    /// Record that `node`'s content changed: bump its qid version.
    fn bump(&mut self, node: &Node) {
        let (_, path) = node_identity(node);
        self.versions.bump(path);
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
                "ctl" => Ok(Node::MachineCtl),
                "ui" => Ok(Node::UiDir),
                "checkpoints" => Ok(Node::CheckpointsDir),
                _ => Err(ErrorCode::NotFound),
            },
            Node::UiDir => match name {
                "activity" => Ok(Node::UiActivity),
                "plan" => Ok(Node::UiPlan),
                "thinking" => Ok(Node::UiThinking),
                "notice" => Ok(Node::UiNotice),
                "events" => Ok(Node::UiEvents),
                _ => Err(ErrorCode::NotFound),
            },
            Node::CheckpointsDir => match name {
                "current" => Ok(Node::CurrentCheckpoint),
                _ => Err(ErrorCode::NotFound),
            },
            Node::RequestsDir => match name {
                "clone" => Ok(Node::RequestsClone),
                "events" => Ok(Node::RequestsEvents),
                id if self.requests.contains_key(id) => Ok(Node::Request(id.to_string())),
                _ => Err(ErrorCode::NotFound),
            },
            Node::Request(id) => match name {
                "kind" => Ok(Node::RequestField(id.clone(), "kind")),
                "prompt" => Ok(Node::RequestField(id.clone(), "prompt")),
                "options" => Ok(Node::RequestField(id.clone(), "options")),
                "status" => Ok(Node::RequestField(id.clone(), "status")),
                "response" => Ok(Node::RequestField(id.clone(), "response")),
                _ => Err(ErrorCode::NotFound),
            },
            Node::ActionsDir => match name {
                "help" => Ok(Node::ActionsHelp),
                "clone" => Ok(Node::ActionsClone),
                "events" => Ok(Node::ActionsEvents),
                id if self.actions.contains_key(id) => Ok(Node::Action(id.to_string())),
                _ => Err(ErrorCode::NotFound),
            },
            Node::Action(id) => match name {
                "name" => Ok(Node::ActionField(id.clone(), "name")),
                "status" => Ok(Node::ActionField(id.clone(), "status")),
                "output" => Ok(Node::ActionField(id.clone(), "output")),
                "result" => Ok(Node::ActionField(id.clone(), "result")),
                "approval" => Ok(Node::ActionField(id.clone(), "approval")),
                "process" => Ok(Node::ActionField(id.clone(), "process")),
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
            Node::MachineDir => b"tape\nstatus\nctl\nui\ncheckpoints".to_vec(),
            Node::UiDir => b"activity\nplan\nthinking\nnotice\nevents".to_vec(),
            Node::CheckpointsDir => b"current".to_vec(),
            Node::CurrentCheckpoint => format!("{}\n", self.tape_root).into_bytes(),
            Node::Status => self.status.clone().into_bytes(),
            Node::UiActivity => self.ui_activity.clone().into_bytes(),
            Node::UiPlan => self.ui_plan.clone().into_bytes(),
            Node::UiThinking => self.ui_thinking.clone().into_bytes(),
            Node::UiNotice => self.ui_notice.clone().into_bytes(),
            // machine/ctl exposes its accepted commands in-band, so a
            // namespace-native client discovers them by reading the file rather
            // than from external docs (self-describing namespace).
            Node::MachineCtl => MACHINE_CTL_HELP.as_bytes().to_vec(),
            Node::RequestsDir => listing(&["clone", "events"], self.requests.keys()),
            Node::ActionsDir => listing(&["clone", "events", "help"], self.actions.keys()),
            Node::ActionsHelp => ACTIONS_HELP.as_bytes().to_vec(),
            Node::Request(_) => b"kind\nprompt\noptions\nstatus\nresponse".to_vec(),
            Node::Action(_) => b"name\nstatus\noutput\nresult\napproval\nprocess".to_vec(),
            Node::RequestField(id, field) => {
                let r = self.requests.get(id).ok_or(ErrorCode::NotFound)?;
                match *field {
                    "kind" => &r.kind,
                    "prompt" => &r.prompt,
                    "options" => &r.options,
                    "status" => &r.status,
                    _ => &r.response,
                }
                .clone()
                .into_bytes()
            }
            Node::ActionField(id, field) => {
                let a = self.actions.get(id).ok_or(ErrorCode::NotFound)?;
                match *field {
                    "name" => a.name.clone().into_bytes(),
                    "status" => a.status.clone().into_bytes(),
                    "result" => a.result.clone().into_bytes(),
                    "approval" => a.approval.clone().into_bytes(),
                    "process" => a.process.clone().into_bytes(),
                    "output" if a.output_retention_expired => a.output.clone().into_bytes(),
                    "output" if a.output_root.is_some() => self
                        .knowledge
                        .root(&action_output_root_name(id))
                        .map_err(map_knowledge_error)
                        .and_then(|root| {
                            self.knowledge
                                .read_bound_root(&root)
                                .map_err(map_knowledge_error)
                        })?,
                    "output" => a.output.clone().into_bytes(),
                    _ => return Err(ErrorCode::NotFound),
                }
            }
            // Streams are served via stream_for; clone files via the fid's clone_id.
            Node::Input
            | Node::Output
            | Node::IoEvents
            | Node::Tape
            | Node::Events
            | Node::UiEvents
            | Node::RequestsEvents
            | Node::ActionsEvents => {
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
            Node::Events => Some(self.events.clone()),
            // io/events is IO-scoped; the per-container streams are their own.
            Node::IoEvents => Some(self.io_events.clone()),
            Node::UiEvents => Some(self.ui_events.clone()),
            Node::RequestsEvents => Some(self.request_events.clone()),
            Node::ActionsEvents => Some(self.action_events.clone()),
            _ => None,
        }
    }

    fn append_tape_block(&mut self, data: &[u8]) -> Result<(), ErrorCode> {
        let root = self
            .knowledge
            .fork_append_bytes(&self.tape_root, [data])
            .map_err(map_knowledge_error)?;
        self.knowledge
            .bind_root(TAPE_ROOT_NAME, root.clone(), RootAccess::ReadWrite)
            .map_err(map_knowledge_error)?;
        self.tape_root = root;
        self.bump(&Node::CurrentCheckpoint);
        Ok(())
    }

    fn materialized_tape(&self) -> Result<Vec<u8>, ErrorCode> {
        let root = self
            .knowledge
            .root(TAPE_ROOT_NAME)
            .map_err(map_knowledge_error)?;
        self.knowledge
            .read_bound_root(&root)
            .map_err(map_knowledge_error)
    }

    fn store_action_output(
        &mut self,
        action_id: &str,
        bytes: &[u8],
    ) -> Result<ContentHash, ErrorCode> {
        let root = self
            .knowledge
            .checkpoint_from_bytes([bytes])
            .map_err(map_knowledge_error)?;
        self.knowledge
            .bind_root(
                action_output_root_name(action_id),
                root.clone(),
                RootAccess::ReadWrite,
            )
            .map_err(map_knowledge_error)?;
        Ok(root)
    }
}

/// A directory listing joining fixed entries with dynamic ids.
fn listing<'a>(fixed: &[&str], ids: impl Iterator<Item = &'a String>) -> Vec<u8> {
    let mut names: Vec<String> = fixed.iter().map(|s| s.to_string()).collect();
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
        let qid = state.qid(&node);
        state.fids.insert(newfid, AgentFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT {
            // The root is a read-only directory: a write-intent open is denied
            // here rather than slipping through the fast-path.
            if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
                return Err(ErrorCode::NoAccess);
            }
            return Ok(qid_v0(&Node::Root));
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
        // Exclusive-write lease on machine/tape (agent-file-layout-contract): while
        // one fid holds the tape open for write, a second write-open is refused so
        // no second writer can interleave records into the source-of-truth tape.
        // Readers are not excluded; the lease releases when the holder clunks. This
        // is the M2 in-server form of the GENERATING lease (the generator is the
        // single tape writer); promotion to an aP-layer mode is owned by the future
        // external-writers work, for writers that bypass this server.
        let is_tape_write =
            matches!(node, Node::Tape) && matches!(mode, OpenMode::Write | OpenMode::ReadWrite);
        if is_tape_write && state.tape_writer.is_some() {
            return Err(ErrorCode::NoAccess);
        }
        // Clone-via-open allocates state *and* the caller must read the fid back
        // to learn the allocated id, so it requires ReadWrite: a read-only
        // observer can't allocate, and a write-only open can't strand an entry
        // whose id it could never read.
        if matches!(node, Node::RequestsClone | Node::ActionsClone)
            && !matches!(mode, OpenMode::ReadWrite)
        {
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
                // Announce the new request on its container stream + the aggregate.
                state
                    .request_events
                    .append(format!("created:{id}\n").as_bytes())
                    .await;
                state
                    .events
                    .append(format!("request:{id}\n").as_bytes())
                    .await;
                // The requests/ directory listing gained an entry.
                state.bump(&Node::RequestsDir);
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
                state
                    .action_events
                    .append(format!("created:{id}\n").as_bytes())
                    .await;
                state
                    .events
                    .append(format!("action:{id}\n").as_bytes())
                    .await;
                // The actions/ directory listing gained an entry.
                state.bump(&Node::ActionsDir);
                Some(id)
            }
            _ => None,
        };
        let read_write_base = if matches!(mode, OpenMode::ReadWrite) {
            match &node {
                Node::UiActivity
                | Node::UiPlan
                | Node::UiThinking
                | Node::UiNotice
                | Node::RequestField(..)
                | Node::ActionField(..) => Some(state.computed_bytes(&node)?),
                _ => None,
            }
        } else {
            None
        };
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        f.mode = Some(mode);
        f.clone_id = clone_id;
        if let Some(bytes) = read_write_base {
            f.write_buf = bytes;
        }
        if is_tape_write {
            state.tape_writer = Some(fid);
        }
        Ok(state.qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (node, clone_id, stream, tape_snapshot) = {
            let state = self.state.lock().await;
            // Reading needs read authority from a successful read-open (ROOT, the
            // pre-bound anchor, is always readable). A released/unknown fid is
            // NotFound — distinct from a live fid lacking read intent (NoAccess).
            if fid != Fid::ROOT {
                let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
                if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
            }
            let clone_id = state.fids.get(&fid).and_then(|f| f.clone_id.clone());
            let node = state.node_of(fid)?;
            let stream = state.stream_for(&node);
            let tape_snapshot = if matches!(node, Node::Tape) {
                Some(state.materialized_tape()?)
            } else {
                None
            };
            (node, clone_id, stream, tape_snapshot)
        };
        // A clone fid reads back the allocated id.
        if let Some(id) = clone_id {
            return Ok(slice(id.into_bytes(), offset, count));
        }
        if let Some(bytes) = tape_snapshot {
            if (offset as usize) < bytes.len() {
                return Ok(slice(bytes, offset, count));
            }
            if let Some(stream) = stream {
                return Ok(stream.read(offset, count).await);
            }
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
            // The agent appends assistant output and tape records directly. Output
            // also goes to the IO-scoped io/events stream; both go to the aggregate.
            Node::Output | Node::Tape | Node::UiEvents => {
                let stream = state.stream_for(&node).expect("stream node");
                let is_output = matches!(node, Node::Output);
                let record = if is_output {
                    format!("output:{}\n", data.len())
                } else if matches!(node, Node::UiEvents) {
                    format!("ui:events:{}\n", data.len())
                } else {
                    format!("tape:{}\n", data.len())
                };
                if matches!(node, Node::Tape) {
                    state.append_tape_block(data)?;
                }
                stream.append(data).await;
                if is_output {
                    state.io_events.append(record.as_bytes()).await;
                }
                state.events.append(record.as_bytes()).await;
                Ok(data.len() as u32)
            }
            // machine/ctl is the agent-runtime control surface: a text command
            // (e.g. `compact`/`rollback`) whose semantics belong to the engine, so
            // the file server only records it for the engine to consume — it does
            // not interpret runtime semantics (agent-file-layout-contract). An
            // empty command is malformed.
            Node::MachineCtl => {
                if data.is_empty() {
                    return Err(ErrorCode::BadRequest);
                }
                let cmd = String::from_utf8(data.to_vec()).map_err(|_| ErrorCode::BadRequest)?;
                state.events.append(format!("ctl:{cmd}\n").as_bytes()).await;
                Ok(data.len() as u32)
            }
            // io/input and request/action data fields are framed documents: buffer
            // at offset and commit the whole unit on clunk, so a turn never starts
            // on a truncated message (commit-on-clunk).
            Node::Input
            | Node::UiActivity
            | Node::UiPlan
            | Node::UiThinking
            | Node::UiNotice
            | Node::RequestField(..)
            | Node::ActionField(..) => {
                let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
                let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
                let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
                let max_bytes = if matches!(node, Node::ActionField(_, "output")) {
                    MAX_ACTION_OUTPUT_BYTES
                } else {
                    MAX_DOC_BYTES
                };
                if end > max_bytes {
                    return Err(ErrorCode::BadRequest);
                }
                if f.write_buf.len() < end {
                    f.write_buf.resize(end, 0);
                }
                f.write_buf[start..end].copy_from_slice(data);
                // A write was issued (even zero-byte): mark intent so an empty
                // answer still commits on clunk.
                f.wrote = true;
                Ok(data.len() as u32)
            }
            _ => Err(ErrorCode::Unsupported),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let length = match &node {
            Node::Output
            | Node::Input
            | Node::Events
            | Node::IoEvents
            | Node::UiEvents
            | Node::RequestsEvents
            | Node::ActionsEvents => state.stream_for(&node).expect("stream").len().await,
            Node::Tape => state.materialized_tape()?.len() as u64,
            other => state
                .computed_bytes(other)
                .map(|b| b.len() as u64)
                .unwrap_or(0),
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
        // Releasing the tape's write fid releases its exclusive-write lease.
        if state.tape_writer == Some(fid) {
            state.tape_writer = None;
        }
        // Commit a buffered write on clunk.
        if matches!(f.node, Node::Input) && f.wrote {
            // Each committed message is length-framed in io/input so consecutive
            // messages keep distinct boundaries in the stream itself (an agent
            // draining io/input reconstructs turns without a side channel): a
            // decimal byte-length, a newline, then the raw payload. The IO-scoped
            // io/events plus the aggregate also announce it.
            let input = state.input.clone();
            let mut framed = format!("{}\n", f.write_buf.len()).into_bytes();
            framed.extend_from_slice(&f.write_buf);
            input.append(&framed).await;
            let record = format!("input:{}\n", f.write_buf.len());
            state.io_events.append(record.as_bytes()).await;
            state.events.append(record.as_bytes()).await;
        } else if let Node::RequestField(id, field) = &f.node
            && f.wrote
        {
            let value = String::from_utf8(f.write_buf).map_err(|_| ErrorCode::BadRequest)?;
            if let Some(r) = state.requests.get_mut(id) {
                // Request status integrity (agent-file-layout-contract): a write to
                // a request that is already terminal (answered/closed/cancelled) is
                // rejected, so a decided yield is never overwritten.
                if is_terminal(&r.status) {
                    return Err(ErrorCode::NoAccess);
                }
                match *field {
                    "kind" => r.kind = value,
                    "prompt" => r.prompt = value,
                    "options" => r.options = value,
                    "response" => {
                        // Answering is writing the response (committed on clunk):
                        // delivering the answer settles the request.
                        r.response = value;
                        r.status = "answered".to_string();
                    }
                    _ => {}
                }
            }
            // The field's content changed; answering also changes status.
            state.bump(&Node::RequestField(id.clone(), field));
            if *field == "response" {
                state.bump(&Node::RequestField(id.clone(), "status"));
            }
            let record = format!("{id}:{field}\n");
            state.request_events.append(record.as_bytes()).await;
            state
                .events
                .append(format!("request:{id}\n").as_bytes())
                .await;
        } else if let Node::ActionField(id, field) = &f.node
            && f.wrote
        {
            let value = String::from_utf8(f.write_buf).map_err(|_| ErrorCode::BadRequest)?;
            let output_root = if *field == "output" {
                if state.actions.get(id).is_some_and(|action| {
                    action.output_root.is_some() || action.output_retention_expired
                }) {
                    return Err(ErrorCode::NoAccess);
                }
                Some(state.store_action_output(id, value.as_bytes())?)
            } else {
                None
            };
            if let Some(a) = state.actions.get_mut(id) {
                match *field {
                    "name" => a.name = value,
                    "status" => a.status = value,
                    "output" => {
                        a.output = value;
                        a.output_root = output_root;
                        a.output_retention_expired = false;
                    }
                    "result" => a.result = value,
                    "approval" => a.approval = value,
                    "process" => a.process = value,
                    _ => {}
                }
            }
            state.bump(&Node::ActionField(id.clone(), field));
            let record = format!("{id}:{field}\n");
            state.action_events.append(record.as_bytes()).await;
            state
                .events
                .append(format!("action:{id}\n").as_bytes())
                .await;
        } else if matches!(
            f.node,
            Node::UiActivity | Node::UiPlan | Node::UiThinking | Node::UiNotice
        ) && f.wrote
        {
            let value = String::from_utf8(f.write_buf).map_err(|_| ErrorCode::BadRequest)?;
            let (node, record) = match f.node {
                Node::UiActivity => {
                    state.ui_activity = value;
                    (Node::UiActivity, "ui:activity\n")
                }
                Node::UiPlan => {
                    state.ui_plan = value;
                    (Node::UiPlan, "ui:plan\n")
                }
                Node::UiThinking => {
                    state.ui_thinking = value;
                    (Node::UiThinking, "ui:thinking\n")
                }
                Node::UiNotice => {
                    state.ui_notice = value;
                    (Node::UiNotice, "ui:notice\n")
                }
                _ => unreachable!("matched above"),
            };
            state.bump(&node);
            state.events.append(record.as_bytes()).await;
        }
        Ok(())
    }
}

/// A node's stable identity: its file kind and a server-unique qid path, keyed by
/// its full file identity so distinct files (and distinct request/action ids)
/// never share a qid. The qid *version* is layered on top from the state's
/// [`VersionTable`] (see [`State::qid`]); this part never changes for a node.
fn node_identity(node: &Node) -> (FileKind, u64) {
    use std::hash::{Hash, Hasher};
    fn path_of(key: &str) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        h.finish()
    }
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::IoDir => (FileKind::Dir, "io".into()),
        Node::MachineDir => (FileKind::Dir, "machine".into()),
        Node::UiDir => (FileKind::Dir, "machine/ui".into()),
        Node::CheckpointsDir => (FileKind::Dir, "machine/checkpoints".into()),
        Node::RequestsDir => (FileKind::Dir, "requests".into()),
        Node::ActionsDir => (FileKind::Dir, "actions".into()),
        Node::ActionsHelp => (FileKind::File, "actions/help".into()),
        Node::ContextDir => (FileKind::Dir, "context".into()),
        Node::ChildrenDir => (FileKind::Dir, "children".into()),
        Node::Request(id) => (FileKind::Dir, format!("requests/{id}")),
        Node::Action(id) => (FileKind::Dir, format!("actions/{id}")),
        Node::RequestsClone => (FileKind::Clone, "requests/clone".into()),
        Node::ActionsClone => (FileKind::Clone, "actions/clone".into()),
        Node::RequestsEvents => (FileKind::Stream, "requests/events".into()),
        Node::ActionsEvents => (FileKind::Stream, "actions/events".into()),
        Node::Input => (FileKind::Stream, "io/input".into()),
        Node::Output => (FileKind::Stream, "io/output".into()),
        Node::IoEvents => (FileKind::Stream, "io/events".into()),
        Node::Tape => (FileKind::Stream, "machine/tape".into()),
        Node::Events => (FileKind::Stream, "events".into()),
        Node::UiEvents => (FileKind::Stream, "machine/ui/events".into()),
        Node::Status => (FileKind::File, "machine/status".into()),
        Node::MachineCtl => (FileKind::File, "machine/ctl".into()),
        Node::UiActivity => (FileKind::File, "machine/ui/activity".into()),
        Node::UiPlan => (FileKind::File, "machine/ui/plan".into()),
        Node::UiThinking => (FileKind::File, "machine/ui/thinking".into()),
        Node::UiNotice => (FileKind::File, "machine/ui/notice".into()),
        Node::CurrentCheckpoint => (FileKind::File, "machine/checkpoints/current".into()),
        Node::RequestField(id, field) => (FileKind::File, format!("requests/{id}/{field}")),
        Node::ActionField(id, field) => (FileKind::File, format!("actions/{id}/{field}")),
    };
    (kind, path_of(&key))
}

/// The qid for a node at version 0, for the stateless contexts (the pre-bound
/// root, whose listing never changes). Mutable nodes get their version through
/// [`State::qid`].
fn qid_v0(node: &Node) -> Qid {
    let (kind, path) = node_identity(node);
    Qid {
        kind,
        version: 0,
        path,
    }
}

/// A request whose decision is final: its fields are frozen against late writes.
fn is_terminal(status: &str) -> bool {
    matches!(status, "answered" | "closed" | "cancelled")
}

fn is_writable(node: &Node) -> bool {
    match node {
        Node::Input
        | Node::Output
        | Node::Tape
        | Node::MachineCtl
        | Node::UiActivity
        | Node::UiPlan
        | Node::UiThinking
        | Node::UiNotice
        | Node::UiEvents
        | Node::RequestsClone
        | Node::ActionsClone
        | Node::ActionField(..) => true,
        // A request's status is read-only state (set by answering, i.e. writing
        // `response`); its other fields are writable data. `machine/status` is
        // read-only too (agent-file-layout-contract).
        Node::RequestField(_, field) => *field != "status",
        _ => false,
    }
}

fn initial_tape_knowledge() -> (KnowledgeStore, ContentHash) {
    let mut knowledge = KnowledgeStore::new();
    let root = knowledge
        .checkpoint_from_blocks(Vec::<ContentHash>::new())
        .expect("empty tape checkpoint can be created");
    knowledge
        .bind_root(TAPE_ROOT_NAME, root.clone(), RootAccess::ReadWrite)
        .expect("empty tape checkpoint can be bound");
    (knowledge, root)
}

fn action_output_root_name(action_id: &str) -> String {
    format!("actions/{action_id}/output")
}

fn retention_expired_record(reference: &str, cause: &str) -> String {
    format!(
        "{{\"type\":\"evidence_retention_expired\",\"reference\":{},\"cause\":{}}}",
        json_string(reference),
        json_string(cause)
    )
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}

fn map_knowledge_error(error: KnowledgeError) -> ErrorCode {
    match error {
        KnowledgeError::NoAccess => ErrorCode::NoAccess,
        KnowledgeError::MissingBlock(_)
        | KnowledgeError::MissingNode(_)
        | KnowledgeError::UnknownRoot(_) => ErrorCode::NotFound,
        KnowledgeError::HashMismatch(_) | KnowledgeError::Cycle(_) => ErrorCode::Io,
    }
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tape_checkpoint_appends_one_delta_node_per_write() {
        let fs = AgentFs::new();
        let mut state = fs.state.lock().await;
        let initial_nodes = state.knowledge.node_count();

        state.append_tape_block(b"turn-1\n").unwrap();
        let after_first = state.knowledge.node_count();
        state.append_tape_block(b"turn-2\n").unwrap();
        let after_second = state.knowledge.node_count();

        assert_eq!(after_first, initial_nodes + 1);
        assert_eq!(after_second, after_first + 1);
        assert_eq!(state.materialized_tape().unwrap(), b"turn-1\nturn-2\n");
    }
}
