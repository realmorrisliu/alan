//! alan-branchfs — headless branching execution as an aP file server.
//!
//! The server exposes speculative checkpoint branches as files. It does not run
//! a scheduler or model calls; it proves the file boundary that a later
//! scheduler can drive: create cheap forks, score branches, select a branch,
//! discard branches, and tail lifecycle events.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream, VersionTable,
};
use alan_knowledge::{ContentHash, KnowledgeStore};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

/// Canonical `/srv` handle name for the branching execution file server.
pub const SRV_HANDLE: &str = "branch";
/// Conventional mount path for branching execution surfaces.
pub const MOUNT_PATH: &str = "/mnt/branch";

/// Branch lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    /// Branch is visible and can be scored, selected, or discarded.
    Active,
    /// Branch is the explicitly selected candidate.
    Selected,
}

/// Inspectable branch metadata served at `branches/<id>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRecord {
    pub version: u16,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub root: ContentHash,
    pub status: BranchStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl BranchRecord {
    fn base(id: String, root: ContentHash) -> Self {
        Self {
            version: 1,
            id,
            base: None,
            root,
            status: BranchStatus::Active,
            score: None,
            summary: None,
        }
    }

    fn fork(id: String, base: String, root: ContentHash) -> Self {
        Self {
            version: 1,
            id,
            base: Some(base),
            root,
            status: BranchStatus::Active,
            score: None,
            summary: None,
        }
    }
}

/// JSON command document accepted by `ctl`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BranchCommand {
    /// Fork a new candidate from an existing visible branch.
    Fork {
        id: String,
        from: String,
        delta: String,
    },
    /// Record an explicit branch score and optional summary.
    Score {
        id: String,
        score: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    /// Explicitly select a visible branch.
    Select { id: String },
    /// Hide a visible branch while retaining lifecycle evidence in `events`.
    Discard { id: String },
}

#[derive(Serialize)]
struct SelectedBranch<'a> {
    id: &'a str,
    root: &'a ContentHash,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
}

struct State {
    store: KnowledgeStore,
    branches: BTreeMap<String, BranchRecord>,
    selected: Option<String>,
    events: Stream,
    versions: VersionTable,
    fids: HashMap<Fid, BranchFid>,
}

struct BranchFid {
    node: Node,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
    wrote: bool,
}

#[derive(Debug, Clone)]
enum Node {
    Root,
    Ctl,
    BranchesDir,
    Branch(String),
    Selected,
    Events,
}

/// Branching execution file server.
pub struct BranchFs {
    state: Mutex<State>,
}

impl Default for BranchFs {
    fn default() -> Self {
        Self::new()
    }
}

impl BranchFs {
    /// Create an empty branchfs.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                store: KnowledgeStore::new(),
                branches: BTreeMap::new(),
                selected: None,
                events: Stream::new(),
                versions: VersionTable::new(),
                fids: HashMap::new(),
            }),
        }
    }

    /// Install a visible base branch backed by a new checkpoint.
    pub async fn install_base_branch<I, B>(
        &self,
        id: impl Into<String>,
        blocks: I,
    ) -> Result<ContentHash, ErrorCode>
    where
        I: IntoIterator<Item = B> + Send,
        I::IntoIter: Send,
        B: AsRef<[u8]> + Send,
    {
        let mut state = self.state.lock().await;
        let id = id.into();
        validate_branch_id(&id)?;
        if state.branches.contains_key(&id) {
            return Err(ErrorCode::BadRequest);
        }
        let root = state
            .store
            .checkpoint_from_bytes(blocks)
            .map_err(knowledge_error)?;
        state
            .branches
            .insert(id.clone(), BranchRecord::base(id.clone(), root.clone()));
        state.versions.bump(node_identity(&Node::BranchesDir).1);
        state
            .versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        state
            .append_event(EventRecord::Base {
                id: &id,
                root: &root,
            })
            .await?;
        Ok(root)
    }

    /// Return a visible branch record for tests and bootstrap code.
    pub async fn branch(&self, id: &str) -> Option<BranchRecord> {
        self.state.lock().await.branches.get(id).cloned()
    }

    /// Number of stored raw knowledge blocks.
    pub async fn block_count(&self) -> usize {
        self.state.lock().await.store.block_count()
    }

    /// Number of stored knowledge DAG nodes.
    pub async fn node_count(&self) -> usize {
        self.state.lock().await.store.node_count()
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
                "ctl" => Ok(Node::Ctl),
                "branches" => Ok(Node::BranchesDir),
                "selected" => Ok(Node::Selected),
                "events" => Ok(Node::Events),
                _ => Err(ErrorCode::NotFound),
            },
            Node::BranchesDir => {
                if self.branches.contains_key(name) {
                    Ok(Node::Branch(name.to_string()))
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => b"ctl\nbranches\nselected\nevents".to_vec(),
            Node::Ctl => {
                b"# branchfs ctl: write one branch command JSON document, then clunk\n".to_vec()
            }
            Node::BranchesDir => listing(self.branches.keys()),
            Node::Branch(id) => json_line(self.branches.get(id).ok_or(ErrorCode::NotFound)?)?,
            Node::Selected => self.selected_bytes()?,
            Node::Events => return Err(ErrorCode::Unsupported),
        };
        Ok(bytes)
    }

    fn stream_for(&self, node: &Node) -> Option<Stream> {
        match node {
            Node::Events => Some(self.events.clone()),
            _ => None,
        }
    }

    fn selected_bytes(&self) -> Result<Vec<u8>, ErrorCode> {
        let Some(id) = self.selected.as_deref() else {
            return Ok(b"null\n".to_vec());
        };
        let branch = self.branches.get(id).ok_or(ErrorCode::NotFound)?;
        json_line(&SelectedBranch {
            id,
            root: &branch.root,
            score: branch.score,
        })
    }

    async fn commit_ctl(&mut self, bytes: Vec<u8>) -> Result<(), ErrorCode> {
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        let command: BranchCommand =
            serde_json::from_str(&text).map_err(|_| ErrorCode::BadRequest)?;
        match command {
            BranchCommand::Fork { id, from, delta } => self.fork_branch(id, from, delta).await,
            BranchCommand::Score { id, score, summary } => {
                self.score_branch(id, score, summary).await
            }
            BranchCommand::Select { id } => self.select_branch(id).await,
            BranchCommand::Discard { id } => self.discard_branch(id).await,
        }
    }

    async fn fork_branch(
        &mut self,
        id: String,
        from: String,
        delta: String,
    ) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        validate_branch_id(&from)?;
        if self.branches.contains_key(&id) {
            return Err(ErrorCode::BadRequest);
        }
        let base = self.branches.get(&from).ok_or(ErrorCode::NotFound)?;
        let root = self
            .store
            .fork_append_bytes(&base.root, [delta.into_bytes()])
            .map_err(knowledge_error)?;
        self.branches.insert(
            id.clone(),
            BranchRecord::fork(id.clone(), from.clone(), root.clone()),
        );
        self.versions.bump(node_identity(&Node::BranchesDir).1);
        self.versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        self.append_event(EventRecord::Fork {
            id: &id,
            from: &from,
            root: &root,
        })
        .await
    }

    async fn score_branch(
        &mut self,
        id: String,
        score: f64,
        summary: Option<String>,
    ) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        if !score.is_finite() {
            return Err(ErrorCode::BadRequest);
        }
        let branch = self.branches.get_mut(&id).ok_or(ErrorCode::NotFound)?;
        branch.score = Some(score);
        branch.summary = summary;
        let summary = branch.summary.clone();
        self.versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        if self.selected.as_deref() == Some(&id) {
            self.versions.bump(node_identity(&Node::Selected).1);
        }
        self.append_event(EventRecord::Score {
            id: &id,
            score,
            summary: summary.as_deref(),
        })
        .await
    }

    async fn select_branch(&mut self, id: String) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        if !self.branches.contains_key(&id) {
            return Err(ErrorCode::NotFound);
        }
        if let Some(previous) = self.selected.take()
            && let Some(branch) = self.branches.get_mut(&previous)
        {
            branch.status = BranchStatus::Active;
            self.versions.bump(node_identity(&Node::Branch(previous)).1);
        }
        let branch = self.branches.get_mut(&id).ok_or(ErrorCode::NotFound)?;
        branch.status = BranchStatus::Selected;
        let root = branch.root.clone();
        self.selected = Some(id.clone());
        self.versions.bump(node_identity(&Node::Selected).1);
        self.versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        self.append_event(EventRecord::Select {
            id: &id,
            root: &root,
        })
        .await
    }

    async fn discard_branch(&mut self, id: String) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        let branch = self.branches.remove(&id).ok_or(ErrorCode::NotFound)?;
        if self.selected.as_deref() == Some(&id) {
            self.selected = None;
            self.versions.bump(node_identity(&Node::Selected).1);
        }
        self.versions.bump(node_identity(&Node::BranchesDir).1);
        self.versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        self.append_event(EventRecord::Discard {
            id: &id,
            root: &branch.root,
        })
        .await
    }

    async fn append_event(&self, event: EventRecord<'_>) -> Result<(), ErrorCode> {
        let mut bytes = serde_json::to_vec(&event).map_err(|_| ErrorCode::BadRequest)?;
        bytes.push(b'\n');
        self.events.append(&bytes).await;
        Ok(())
    }
}

#[async_trait]
impl FileServer for BranchFs {
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
        state.fids.insert(newfid, BranchFid::at(node));
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
            Node::Events => {
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
        let f = state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        if !f.wrote {
            return Ok(());
        }
        match f.node {
            Node::Ctl => state.commit_ctl(f.write_buf).await,
            _ => Ok(()),
        }
    }
}

impl BranchFid {
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
    Base {
        id: &'a str,
        root: &'a ContentHash,
    },
    Fork {
        id: &'a str,
        from: &'a str,
        root: &'a ContentHash,
    },
    Score {
        id: &'a str,
        score: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<&'a str>,
    },
    Select {
        id: &'a str,
        root: &'a ContentHash,
    },
    Discard {
        id: &'a str,
        root: &'a ContentHash,
    },
}

fn is_writable(node: &Node) -> bool {
    matches!(node, Node::Ctl)
}

fn validate_branch_id(id: &str) -> Result<(), ErrorCode> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ErrorCode::BadRequest);
    }
    Ok(())
}

fn knowledge_error(error: alan_knowledge::KnowledgeError) -> ErrorCode {
    match error {
        alan_knowledge::KnowledgeError::MissingBlock(_)
        | alan_knowledge::KnowledgeError::MissingNode(_)
        | alan_knowledge::KnowledgeError::UnknownRoot(_) => ErrorCode::NotFound,
        alan_knowledge::KnowledgeError::NoAccess => ErrorCode::NoAccess,
        alan_knowledge::KnowledgeError::HashMismatch(_)
        | alan_knowledge::KnowledgeError::Cycle(_) => ErrorCode::BadRequest,
    }
}

fn json_line(value: &impl Serialize) -> Result<Vec<u8>, ErrorCode> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| ErrorCode::BadRequest)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn listing<'a>(names: impl Iterator<Item = &'a String>) -> Vec<u8> {
    let mut out = Vec::new();
    for name in names {
        out.extend_from_slice(name.as_bytes());
        out.push(b'\n');
    }
    out
}

fn node_identity(node: &Node) -> (FileKind, u64) {
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::Ctl => (FileKind::File, "ctl".to_string()),
        Node::BranchesDir => (FileKind::Dir, "branches".to_string()),
        Node::Branch(id) => (FileKind::File, format!("branches/{id}")),
        Node::Selected => (FileKind::File, "selected".to_string()),
        Node::Events => (FileKind::Stream, "events".to_string()),
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
