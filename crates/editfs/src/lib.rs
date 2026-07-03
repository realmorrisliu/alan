//! alan-editfs — headless editable-buffer interaction as an aP file server.
//!
//! The server exposes one editable buffer with `body`, `tag`, `addr`, `ctl`, and
//! `event` files. It proves the Ring 4 editable-buffer contract without native UI
//! or real shell execution: text edits are committed on clunk, address ranges are
//! revision-bound, explicit `ctl exec` is policy-gated, and all activity is
//! observable through a retained blocking-read event stream.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream, VersionTable,
};
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::Mutex;

const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

/// Canonical `/srv` handle name for the editable-buffer file server.
pub const SRV_HANDLE: &str = "edit";
/// Conventional mount path for the first editable-buffer surface.
pub const MOUNT_PATH: &str = "/mnt/edit";

/// Minimal execution policy for the headless v1 server.
///
/// This does not execute shell commands. It records whether an explicit `ctl
/// exec` would be accepted by the mounted policy boundary, keeping the file
/// contract testable without granting hidden authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPolicy {
    /// Reject all explicit executions.
    DenyAll,
    /// Accept explicit executions for harness/testing.
    AcceptAll,
}

impl ExecutionPolicy {
    fn decide(self, _command: &str) -> ExecutionStatus {
        match self {
            Self::DenyAll => ExecutionStatus::Denied,
            Self::AcceptAll => ExecutionStatus::Accepted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ExecutionStatus {
    Accepted,
    Denied,
}

/// A revision-bound byte range in `body`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddressRange {
    pub revision: u64,
    pub start: usize,
    pub end: usize,
}

impl AddressRange {
    fn collapsed(revision: u64) -> Self {
        Self {
            revision,
            start: 0,
            end: 0,
        }
    }

    fn parse(source: &str) -> Result<Self, ErrorCode> {
        let trimmed = source.trim();
        let (revision, range) = trimmed
            .strip_prefix("rev:")
            .and_then(|rest| rest.split_once(' '))
            .ok_or(ErrorCode::BadRequest)?;
        let revision = revision.parse::<u64>().map_err(|_| ErrorCode::BadRequest)?;
        let (start, end) = range.split_once("..").ok_or(ErrorCode::BadRequest)?;
        let start = start.parse::<usize>().map_err(|_| ErrorCode::BadRequest)?;
        let end = end.parse::<usize>().map_err(|_| ErrorCode::BadRequest)?;
        if start > end {
            return Err(ErrorCode::BadRequest);
        }
        Ok(Self {
            revision,
            start,
            end,
        })
    }
}

impl std::fmt::Display for AddressRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rev:{} {}..{}", self.revision, self.start, self.end)
    }
}

struct State {
    body: String,
    tag: String,
    body_revision: u64,
    addr: AddressRange,
    event: Stream,
    versions: VersionTable,
    fids: HashMap<Fid, EditFid>,
    execution_policy: ExecutionPolicy,
}

struct EditFid {
    node: Node,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
    wrote: bool,
}

#[derive(Debug, Clone)]
enum Node {
    Root,
    Body,
    Tag,
    Addr,
    Ctl,
    Event,
}

/// Editable-buffer file server.
pub struct EditFs {
    state: Mutex<State>,
}

impl Default for EditFs {
    fn default() -> Self {
        Self::new()
    }
}

impl EditFs {
    /// Create an editable buffer with default-denied execution.
    pub fn new() -> Self {
        Self::with_execution_policy(ExecutionPolicy::DenyAll)
    }

    /// Create an editable buffer with an explicit execution policy.
    pub fn with_execution_policy(execution_policy: ExecutionPolicy) -> Self {
        Self {
            state: Mutex::new(State {
                body: String::new(),
                tag: String::new(),
                body_revision: 0,
                addr: AddressRange::collapsed(0),
                event: Stream::new(),
                versions: VersionTable::new(),
                fids: HashMap::new(),
                execution_policy,
            }),
        }
    }

    /// Return the current body revision for tests and bootstrap code.
    pub async fn body_revision(&self) -> u64 {
        self.state.lock().await.body_revision
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

    fn qid(&self, node: &Node) -> Qid {
        let (kind, path) = node_identity(node);
        Qid {
            kind,
            version: self.versions.get(path),
            path,
        }
    }

    fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match name {
                "body" => Ok(Node::Body),
                "tag" => Ok(Node::Tag),
                "addr" => Ok(Node::Addr),
                "ctl" => Ok(Node::Ctl),
                "event" => Ok(Node::Event),
                _ => Err(ErrorCode::NotFound),
            },
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => b"body\ntag\naddr\nctl\nevent".to_vec(),
            Node::Body => self.body.as_bytes().to_vec(),
            Node::Tag => self.tag.as_bytes().to_vec(),
            Node::Addr => self.addr.to_string().into_bytes(),
            Node::Ctl => b"# editfs ctl: write 'exec' and clunk to execute addr\n".to_vec(),
            Node::Event => return Err(ErrorCode::Unsupported),
        };
        Ok(bytes)
    }

    fn stream_for(&self, node: &Node) -> Option<Stream> {
        match node {
            Node::Event => Some(self.event.clone()),
            _ => None,
        }
    }

    async fn commit_document(&mut self, node: Node, bytes: Vec<u8>) -> Result<(), ErrorCode> {
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        match node {
            Node::Body => {
                self.body = text;
                self.body_revision += 1;
                self.versions.bump(node_identity(&Node::Body).1);
                self.versions.bump(node_identity(&Node::Addr).1);
                self.append_event(EventRecord::Edit {
                    file: "body",
                    revision: Some(self.body_revision),
                    length: self.body.len(),
                })
                .await
            }
            Node::Tag => {
                self.tag = text;
                self.versions.bump(node_identity(&Node::Tag).1);
                self.append_event(EventRecord::Edit {
                    file: "tag",
                    revision: None,
                    length: self.tag.len(),
                })
                .await
            }
            _ => Err(ErrorCode::Unsupported),
        }
    }

    async fn commit_addr(&mut self, bytes: Vec<u8>) -> Result<(), ErrorCode> {
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        let addr = AddressRange::parse(&text)?;
        if addr.revision != self.body_revision {
            return Err(ErrorCode::BadRequest);
        }
        self.validate_range_shape(&addr)?;
        self.addr = addr.clone();
        self.versions.bump(node_identity(&Node::Addr).1);
        self.append_event(EventRecord::Address { range: addr })
            .await
    }

    async fn commit_ctl(&mut self, bytes: Vec<u8>) -> Result<(), ErrorCode> {
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        match text.trim() {
            "exec" => self.exec_addr().await,
            _ => Err(ErrorCode::BadRequest),
        }
    }

    fn validate_range_shape(&self, addr: &AddressRange) -> Result<(), ErrorCode> {
        if addr.end > self.body.len()
            || !self.body.is_char_boundary(addr.start)
            || !self.body.is_char_boundary(addr.end)
        {
            return Err(ErrorCode::BadRequest);
        }
        Ok(())
    }

    async fn exec_addr(&mut self) -> Result<(), ErrorCode> {
        let addr = self.addr.clone();
        if addr.revision != self.body_revision {
            return Err(ErrorCode::BadRequest);
        }
        self.validate_range_shape(&addr)?;
        let command = self
            .body
            .get(addr.start..addr.end)
            .ok_or(ErrorCode::BadRequest)?
            .to_string();
        let status = self.execution_policy.decide(&command);
        self.append_event(EventRecord::Exec {
            range: addr,
            command,
            status,
        })
        .await?;
        match status {
            ExecutionStatus::Accepted => Ok(()),
            ExecutionStatus::Denied => Err(ErrorCode::NoAccess),
        }
    }

    async fn append_event(&self, event: EventRecord<'_>) -> Result<(), ErrorCode> {
        let mut bytes = serde_json::to_vec(&event).map_err(|_| ErrorCode::BadRequest)?;
        bytes.push(b'\n');
        self.event.append(&bytes).await;
        Ok(())
    }
}

#[async_trait]
impl FileServer for EditFs {
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
        state.fids.insert(newfid, EditFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        if fid != Fid::ROOT && state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) && !is_writable(&node) {
            return Err(ErrorCode::NoAccess);
        }
        if fid != Fid::ROOT {
            let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            f.mode = Some(mode);
        }
        Ok(state.qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (node, stream) = {
            let state = self.state.lock().await;
            if fid != Fid::ROOT {
                let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
                if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
            }
            let node = state.node_of(fid)?;
            let stream = state.stream_for(&node);
            (node, stream)
        };
        if let Some(stream) = stream {
            return Ok(stream.read(offset, count).await);
        }
        let state = self.state.lock().await;
        Ok(slice(state.computed_bytes(&node)?, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        if !is_writable(&node) {
            return Err(ErrorCode::Unsupported);
        }
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        if end > MAX_DOC_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        if f.write_buf.len() < end {
            f.write_buf.resize(end, 0);
        }
        f.write_buf[start..end].copy_from_slice(data);
        f.wrote = true;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let length = match &node {
            Node::Event => {
                state
                    .stream_for(&node)
                    .ok_or(ErrorCode::NotFound)?
                    .len()
                    .await
            }
            other => state.computed_bytes(other)?.len() as u64,
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
        let f = state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        if !f.wrote {
            return Ok(());
        }
        match f.node {
            Node::Body | Node::Tag => state.commit_document(f.node, f.write_buf).await,
            Node::Addr => state.commit_addr(f.write_buf).await,
            Node::Ctl => state.commit_ctl(f.write_buf).await,
            _ => Ok(()),
        }
    }
}

impl EditFid {
    fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            write_buf: Vec::new(),
            wrote: false,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum EventRecord<'a> {
    Edit {
        file: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        revision: Option<u64>,
        length: usize,
    },
    Address {
        range: AddressRange,
    },
    Exec {
        range: AddressRange,
        command: String,
        status: ExecutionStatus,
    },
}

fn is_writable(node: &Node) -> bool {
    matches!(node, Node::Body | Node::Tag | Node::Addr | Node::Ctl)
}

fn node_identity(node: &Node) -> (FileKind, u64) {
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/"),
        Node::Body => (FileKind::File, "body"),
        Node::Tag => (FileKind::File, "tag"),
        Node::Addr => (FileKind::File, "addr"),
        Node::Ctl => (FileKind::File, "ctl"),
        Node::Event => (FileKind::Stream, "event"),
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    (kind, h.finish())
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
