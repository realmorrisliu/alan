//! A reference in-memory [`FileServer`] used to exercise the aP conventions and
//! as a worked template for real servers (and alan-shell's M1 echo milestone).
//!
//! It is intentionally small but conformant on the points that are easy to get
//! wrong: the fid lifecycle (§5.2), clone-via-open allocating independent
//! resources (§5.4), and commit-on-clunk document writes whose malformed
//! commit is a commit-time [`ErrorCode::BadRequest`] (§5.5). It is not the
//! kernel's `/proc`/`/srv` and stores no durable state.

use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, VersionTable};

/// Upper bound on a buffered commit-on-clunk document, so a huge/sparse write
/// offset cannot make the reference server allocate unbounded memory.
const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

type NodeId = usize;

enum Node {
    Dir(BTreeMap<String, NodeId>),
    Bytes(Vec<u8>),
    WritableBytes(Vec<u8>),
    /// A clone file: each `open` allocates a fresh connection directory.
    Clone {
        next: u64,
    },
    /// A commit-on-clunk document file that must hold valid JSON at commit.
    Doc,
    /// Reclaimed slot available for reuse by future `create` calls.
    Tombstone,
}

struct FidState {
    node: NodeId,
    mode: Option<OpenMode>,
    /// Buffered document for a commit-on-clunk write, committed at `clunk`.
    write_buf: Vec<u8>,
    /// Whether any write on this fid failed; failed writes must never commit on clunk.
    write_failed: bool,
    /// For an opened clone fid: the resource name allocated at `open`, returned
    /// when the caller `read`s the clone fid.
    clone_name: Option<String>,
}

impl FidState {
    fn at(node: NodeId) -> Self {
        Self {
            node,
            mode: None,
            write_buf: Vec::new(),
            write_failed: false,
            clone_name: None,
        }
    }
}

struct State {
    nodes: Vec<Node>,
    fids: HashMap<Fid, FidState>,
    /// Per-node qid versions, keyed by `NodeId`; bumped when a node's content
    /// changes (here, the root listing when a clone allocates a connection).
    versions: VersionTable,
    writable: bool,
    unlinked: HashSet<NodeId>,
    free_nodes: Vec<NodeId>,
}

impl State {
    fn push(&mut self, node: Node) -> NodeId {
        if let Some(slot) = self.free_nodes.pop() {
            self.nodes[slot] = node;
            slot
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        }
    }

    fn reclaim_if_unlinked(&mut self, node: NodeId) {
        if self.unlinked.contains(&node) && !self.fids.values().any(|f| f.node == node) {
            self.unlinked.remove(&node);
            self.nodes[node] = Node::Tombstone;
            self.free_nodes.push(node);
        }
    }

    fn qid(&self, node: NodeId) -> Qid {
        let kind = match self.nodes[node] {
            Node::Dir(_) => FileKind::Dir,
            Node::Bytes(_) | Node::WritableBytes(_) | Node::Doc | Node::Tombstone => FileKind::File,
            Node::Clone { .. } => FileKind::Clone,
        };
        Qid {
            kind,
            version: self.versions.get(node as u64),
            path: node as u64,
        }
    }

    fn dir_entry(&self, dir: NodeId, name: &str) -> Result<NodeId, ErrorCode> {
        match &self.nodes[dir] {
            Node::Dir(entries) => entries.get(name).copied().ok_or(ErrorCode::NotFound),
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    fn fid(&self, fid: Fid) -> Result<&FidState, ErrorCode> {
        self.fids.get(&fid).ok_or(ErrorCode::NotFound)
    }
}

/// A small in-memory aP file server: a root directory containing `greeting`
/// (bytes), `clone` (clone-via-open), and `submit` (commit-on-clunk JSON doc).
pub struct MemFs {
    state: Mutex<State>,
}

impl Default for MemFs {
    fn default() -> Self {
        Self::new()
    }
}

impl MemFs {
    /// Create an empty read-only directory tree.
    pub fn empty() -> Self {
        Self::with_read_only_files(std::iter::empty())
    }

    pub fn new() -> Self {
        // Root is node 0 so `Fid::ROOT` can be pre-bound to it.
        let nodes = vec![Node::Dir(BTreeMap::new())];
        let mut state = State {
            nodes,
            fids: HashMap::new(),
            versions: VersionTable::new(),
            writable: false,
            unlinked: HashSet::new(),
            free_nodes: Vec::new(),
        };

        let greeting = state.push(Node::Bytes(b"hi".to_vec()));
        let clone = state.push(Node::Clone { next: 0 });
        let submit = state.push(Node::Doc);
        if let Node::Dir(entries) = &mut state.nodes[0] {
            entries.insert("greeting".into(), greeting);
            entries.insert("clone".into(), clone);
            entries.insert("submit".into(), submit);
        }
        state.fids.insert(Fid::ROOT, FidState::at(0));

        Self {
            state: Mutex::new(state),
        }
    }

    /// Create an empty writable directory tree for ephemeral in-memory storage.
    pub fn writable() -> Self {
        let mut state = State {
            nodes: vec![Node::Dir(BTreeMap::new())],
            fids: HashMap::new(),
            versions: VersionTable::new(),
            writable: true,
            unlinked: HashSet::new(),
            free_nodes: Vec::new(),
        };
        state.fids.insert(Fid::ROOT, FidState::at(0));
        Self {
            state: Mutex::new(state),
        }
    }

    /// Create a root containing exactly one read-only byte file.
    pub fn with_read_only_file(name: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self::with_read_only_files([(name.into(), bytes.into())])
    }

    /// Create a root containing exactly the supplied read-only byte files.
    pub fn with_read_only_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        let mut state = State {
            nodes: vec![Node::Dir(BTreeMap::new())],
            fids: HashMap::new(),
            versions: VersionTable::new(),
            writable: false,
            unlinked: HashSet::new(),
            free_nodes: Vec::new(),
        };
        for (name, bytes) in files {
            assert!(!name.is_empty() && !name.contains('/'));
            let file = state.push(Node::Bytes(bytes));
            if let Node::Dir(entries) = &mut state.nodes[0] {
                entries.insert(name, file);
            }
        }
        state.fids.insert(Fid::ROOT, FidState::at(0));
        Self {
            state: Mutex::new(state),
        }
    }

    /// Create a read-only directory tree from `/`-separated relative paths.
    pub fn with_read_only_tree(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self::try_with_read_only_tree(files).expect("invalid read-only tree")
    }

    /// Fallibly create a read-only directory tree from `/`-separated relative paths.
    pub fn try_with_read_only_tree(
        files: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> Result<Self, ErrorCode> {
        let mut state = State {
            nodes: vec![Node::Dir(BTreeMap::new())],
            fids: HashMap::new(),
            versions: VersionTable::new(),
            writable: false,
            unlinked: HashSet::new(),
            free_nodes: Vec::new(),
        };
        for (path, bytes) in files {
            let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
            if parts.is_empty() {
                return Err(ErrorCode::BadRequest);
            }
            let mut dir_idx = 0usize;
            for &dir_name in &parts[..parts.len() - 1] {
                let existing = match &state.nodes[dir_idx] {
                    Node::Dir(entries) => entries.get(dir_name).copied(),
                    _ => return Err(ErrorCode::BadRequest),
                };
                dir_idx = match existing {
                    Some(idx) => {
                        if !matches!(state.nodes[idx], Node::Dir(_)) {
                            return Err(ErrorCode::BadRequest);
                        }
                        idx
                    }
                    None => {
                        let next_idx = state.push(Node::Dir(BTreeMap::new()));
                        if let Node::Dir(entries) = &mut state.nodes[dir_idx] {
                            entries.insert(dir_name.to_string(), next_idx);
                        }
                        next_idx
                    }
                };
            }
            let file_name = parts[parts.len() - 1].to_string();
            let already_dir = match &state.nodes[dir_idx] {
                Node::Dir(entries) => entries.contains_key(&file_name),
                _ => return Err(ErrorCode::BadRequest),
            };
            if already_dir {
                return Err(ErrorCode::BadRequest);
            }
            let file_idx = state.push(Node::Bytes(bytes));
            if let Node::Dir(entries) = &mut state.nodes[dir_idx] {
                entries.insert(file_name, file_idx);
            }
        }
        state.fids.insert(Fid::ROOT, FidState::at(0));
        Ok(Self {
            state: Mutex::new(state),
        })
    }
}

#[async_trait]
impl FileServer for MemFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        // A fid is a handle to one interaction (§5.2): never rebind the reserved
        // root or an already-live fid, or two callers reusing a number would
        // clobber each other's state.
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = state.fid(fid)?.node;
        for name in names {
            node = state.dir_entry(node, name)?;
        }
        state.fids.insert(newfid, FidState::at(node));
        Ok(state.qid(node))
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        let existing = state.fid(fid)?;
        // A fid is a handle to one interaction (§5.2): reopening an already-open
        // fid before clunk is rejected, so a second `open` cannot downgrade the
        // write intent and let a buffered malformed document bypass commit-time
        // validation.
        if existing.mode.is_some() {
            return Err(ErrorCode::BadRequest);
        }
        let node = existing.node;

        // Dial-time access check (§5.5): reject a mode the node cannot service —
        // a write to a read-only node or a read of the write-only document fails
        // at open, not later as a misclassified "successful" interaction.
        if !serviceable(&state.nodes[node], mode) {
            return Err(ErrorCode::NoAccess);
        }

        // Clone-via-open: allocate a fresh connection directory under root and
        // remember its name for this fid's subsequent read.
        let clone_name = if let Node::Clone { next } = &mut state.nodes[node] {
            let n = *next;
            *next += 1;
            let name = format!("conn-{n}");
            let id_file = state.push(Node::Bytes(name.clone().into_bytes()));
            let mut entries = BTreeMap::new();
            entries.insert("id".to_string(), id_file);
            let conn_dir = state.push(Node::Dir(entries));
            if let Node::Dir(root) = &mut state.nodes[0] {
                root.insert(name.clone(), conn_dir);
            }
            // The root directory's listing changed: bump its qid version.
            state.versions.bump(0);
            Some(name)
        } else {
            None
        };

        let qid = state.qid(node);
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        f.mode = Some(mode);
        f.clone_name = clone_name;
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        let f = state.fid(fid)?;
        // Reading needs read authority established by a successful read-open;
        // mirror the write path so bytes are never served before the per-fid
        // access intent is set (§5.2 / three-phase model).
        if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        // An opened clone fid reads back the allocated resource name.
        let bytes = if let Some(name) = &f.clone_name {
            name.clone().into_bytes()
        } else {
            match &state.nodes[f.node] {
                Node::Bytes(b) | Node::WritableBytes(b) => b.clone(),
                Node::Dir(entries) => entries
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into_bytes(),
                _ => return Err(ErrorCode::Unsupported),
            }
        };
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let f = state.fid(fid)?;
        let node = f.node;
        let mode = f.mode;
        match state.nodes[node] {
            Node::WritableBytes(_) => {
                if !matches!(mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
                if matches!(mode, Some(OpenMode::Write)) {
                    let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
                    let res = write_to_buf(&mut f.write_buf, offset, data);
                    if res.is_err() {
                        f.write_failed = true;
                    }
                    return res;
                }
                let written = match &mut state.nodes[node] {
                    Node::WritableBytes(bytes) => write_to_buf(bytes, offset, data)?,
                    _ => unreachable!(),
                };
                state.versions.bump(node as u64);
                Ok(written)
            }
            // Commit-on-clunk: buffer the document; it is acted on only at clunk.
            Node::Doc => {
                let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
                // Writing needs write authority: a fid opened read-only (or not
                // opened for write) must not buffer, or its malformed payload
                // would be silently skipped at commit and defeat the
                // commit-on-clunk error model this server demonstrates.
                if !matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
                let res = write_to_buf(&mut f.write_buf, offset, data);
                if res.is_err() {
                    f.write_failed = true;
                }
                res
            }
            _ => Err(ErrorCode::Unsupported),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let f = state.fid(fid)?;
        let qid = state.qid(f.node);
        let length = match &state.nodes[f.node] {
            Node::Bytes(b) | Node::WritableBytes(b) => b.len() as u64,
            _ => 0,
        };
        let writable = matches!(state.nodes[f.node], Node::WritableBytes(_))
            || matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite));
        Ok(Stat {
            name: String::new(),
            qid,
            length,
            executable: false,
            writable,
        })
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if !state.writable {
            return Err(ErrorCode::Unsupported);
        }
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            return Err(ErrorCode::BadRequest);
        }
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let parent_node = state.fid(fid)?.node;
        let already_exists = match &state.nodes[parent_node] {
            Node::Dir(entries) => entries.contains_key(name),
            _ => return Err(ErrorCode::NotDirectory),
        };
        if already_exists {
            return Err(ErrorCode::BadRequest);
        }
        let child_node = match kind {
            FileKind::File => state.push(Node::WritableBytes(Vec::new())),
            FileKind::Dir => state.push(Node::Dir(BTreeMap::new())),
            FileKind::Stream | FileKind::Clone => return Err(ErrorCode::Unsupported),
        };
        if let Node::Dir(entries) = &mut state.nodes[parent_node] {
            entries.insert(name.to_string(), child_node);
        }
        state.versions.bump(parent_node as u64);
        let qid = state.qid(child_node);
        state.fids.insert(newfid, FidState::at(child_node));
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        if !state.writable {
            return Err(ErrorCode::Unsupported);
        }
        if fid == Fid::ROOT {
            return Err(ErrorCode::BadRequest);
        }
        let target = state.fid(fid)?.node;
        if let Node::Dir(entries) = &state.nodes[target]
            && !entries.is_empty()
        {
            return Err(ErrorCode::BadRequest);
        }
        state.fids.remove(&fid);
        for idx in 0..state.nodes.len() {
            if let Node::Dir(entries) = &mut state.nodes[idx]
                && entries.values().any(|&child| child == target)
            {
                entries.retain(|_, &mut child| child != target);
                state.versions.bump(idx as u64);
            }
        }
        state.unlinked.insert(target);
        state.reclaim_if_unlinked(target);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            // Root is the reusable pre-bound anchor: never remove it, but clear
            // its per-open state so it can be opened again (otherwise the
            // reopen guard would lock root out for the server's lifetime).
            let mut state = self.state.lock().await;
            if let Some(root) = state.fids.get_mut(&Fid::ROOT) {
                root.mode = None;
                root.clone_name = None;
                root.write_buf.clear();
                root.write_failed = false;
            }
            return Ok(());
        }
        let mut state = self.state.lock().await;
        let f = state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        // Commit-on-clunk validation: a Doc opened for write must hold a valid
        // document at commit, else a commit-time error (§5.5).
        if matches!(state.nodes[f.node], Node::Doc)
            && matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite))
            && !f.write_failed
        {
            serde_json::from_slice::<serde_json::Value>(&f.write_buf)
                .map_err(|_| ErrorCode::BadRequest)?;
        } else if matches!(f.mode, Some(OpenMode::Write))
            && !f.write_failed
            && let Node::WritableBytes(bytes) = &mut state.nodes[f.node]
        {
            *bytes = f.write_buf;
            state.versions.bump(f.node as u64);
        }
        state.reclaim_if_unlinked(f.node);
        Ok(())
    }
}

fn write_to_buf(buf: &mut Vec<u8>, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
    let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
    let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
    if end > MAX_DOC_BYTES {
        return Err(ErrorCode::BadRequest);
    }
    if buf.len() < end {
        buf.resize(end, 0);
    }
    buf[start..end].copy_from_slice(data);
    Ok(data.len() as u32)
}

/// Whether `node` can service an open in `mode`. Read-only nodes (bytes,
/// directories, the clone file) serve only `Read`; the write-only document
/// serves only `Write`. Anything else is a dial-time access error, not a
/// success that fails later.
fn serviceable(node: &Node, mode: OpenMode) -> bool {
    match node {
        Node::Doc => matches!(mode, OpenMode::Write),
        Node::WritableBytes(_) => true,
        Node::Bytes(_) | Node::Dir(_) | Node::Clone { .. } => matches!(mode, OpenMode::Read),
        Node::Tombstone => false,
    }
}
