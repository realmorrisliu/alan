//! alan-memfs — Memory Store file server backed by content-addressed knowledge.
//!
//! The server is intentionally small: callers see ordinary files under the
//! mounted tree (for example `/mnt/mem/facts`). File bytes are stored as immutable
//! knowledge blocks behind a namespace-bound checkpoint root, so memory/context
//! reads stay file-shaped while persistence can deduplicate, verify, and garbage
//! collect by reachability.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, VersionTable};
use alan_knowledge::{BoundRoot, ContentHash, KnowledgeError, KnowledgeStore, RootAccess};
use async_trait::async_trait;
use tokio::sync::Mutex;

const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

/// A memory/context file server backed by content-addressed checkpoint roots.
pub struct MemFs {
    state: Mutex<State>,
}

struct State {
    store: KnowledgeStore,
    files: BTreeMap<String, MemoryFile>,
    versions: VersionTable,
    fids: HashMap<Fid, MemFid>,
}

struct MemoryFile {
    root: BoundRoot,
}

struct MemFid {
    node: Node,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
    wrote: bool,
}

#[derive(Clone)]
enum Node {
    Root,
    File(String),
}

impl Default for MemFs {
    fn default() -> Self {
        Self::new()
    }
}

impl MemFs {
    /// Create an empty memory file server.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                store: KnowledgeStore::new(),
                files: BTreeMap::new(),
                versions: VersionTable::new(),
                fids: HashMap::new(),
            }),
        }
    }

    /// Return the current checkpoint root for a named file.
    pub async fn checkpoint_root(&self, name: &str) -> Result<ContentHash, ErrorCode> {
        let state = self.state.lock().await;
        let file = state.files.get(name).ok_or(ErrorCode::NotFound)?;
        Ok(file.root.root_hash().clone())
    }

    /// Bind an existing checkpoint root back into this memory tree.
    ///
    /// This is the durable-home resume primitive for the in-process v1 store: a
    /// storage-backed home keeps blocks/nodes after a file is unbound, so the
    /// checkpoint root can be rebound later. A fresh ephemeral store lacks those
    /// nodes and rejects the same root.
    pub async fn restore_checkpoint(
        &self,
        name: impl Into<String>,
        root: ContentHash,
    ) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        let name = name.into();
        if !valid_name(&name) {
            return Err(ErrorCode::BadRequest);
        }
        let binding = state
            .store
            .bind_root(root_name(&name), root, RootAccess::ReadWrite)
            .map_err(map_knowledge_error)?;
        state
            .files
            .insert(name.clone(), MemoryFile { root: binding });
        state.versions.bump(node_identity(&Node::Root).1);
        state.versions.bump(node_identity(&Node::File(name)).1);
        Ok(())
    }

    /// Verify and materialize a named file through its namespace-bound root.
    pub async fn materialize(&self, name: &str) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        state.materialize(name)
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

    fn materialize(&self, name: &str) -> Result<Vec<u8>, ErrorCode> {
        let file = self.files.get(name).ok_or(ErrorCode::NotFound)?;
        self.store
            .read_bound_root(&file.root)
            .map_err(map_knowledge_error)
    }

    fn put_file(&mut self, name: String, bytes: &[u8]) -> Result<(), ErrorCode> {
        let root = self
            .store
            .checkpoint_from_bytes([bytes])
            .map_err(map_knowledge_error)?;
        let binding = self
            .store
            .bind_root(root_name(&name), root, RootAccess::ReadWrite)
            .map_err(map_knowledge_error)?;
        self.files
            .insert(name.clone(), MemoryFile { root: binding });
        self.versions.bump(node_identity(&Node::Root).1);
        self.versions.bump(node_identity(&Node::File(name)).1);
        Ok(())
    }
}

#[async_trait]
impl FileServer for MemFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let node = match state.node_of(fid)? {
            Node::Root if names.is_empty() => Node::Root,
            Node::Root if names.len() == 1 && state.files.contains_key(&names[0]) => {
                Node::File(names[0].clone())
            }
            Node::Root if names.len() == 1 => return Err(ErrorCode::NotFound),
            Node::Root => return Err(ErrorCode::NotDirectory),
            Node::File(_) => return Err(ErrorCode::NotDirectory),
        };
        let qid = state.qid(&node);
        state.fids.insert(
            newfid,
            MemFid {
                node,
                mode: None,
                write_buf: Vec::new(),
                wrote: false,
            },
        );
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        if fid != Fid::ROOT && state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        if matches!(node, Node::Root) && matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            return Err(ErrorCode::NoAccess);
        }
        if let Node::File(name) = &node
            && !state.files.contains_key(name)
        {
            return Err(ErrorCode::NotFound);
        }
        if fid != Fid::ROOT {
            let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            f.mode = Some(mode);
        }
        Ok(state.qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        if fid != Fid::ROOT {
            let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                return Err(ErrorCode::NoAccess);
            }
        }
        let bytes = match state.node_of(fid)? {
            Node::Root => state
                .files
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes(),
            Node::File(name) => state.materialize(&name)?,
        };
        Ok(slice(bytes, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        if !matches!(f.node, Node::File(_)) {
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
            Node::Root => state
                .files
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .len() as u64,
            Node::File(name) => state.materialize(name)?.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid: state.qid(&node),
            length,
            writable: matches!(node, Node::File(_)),
        })
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if kind != FileKind::File || !valid_name(name) {
            return Err(ErrorCode::BadRequest);
        }
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        if !matches!(state.node_of(fid)?, Node::Root) {
            return Err(ErrorCode::NotDirectory);
        }
        if state.files.contains_key(name) {
            return Err(ErrorCode::BadRequest);
        }
        let node = Node::File(name.to_string());
        state.put_file(name.to_string(), b"")?;
        let qid = state.qid(&node);
        state.fids.insert(
            newfid,
            MemFid {
                node,
                mode: None,
                write_buf: Vec::new(),
                wrote: false,
            },
        );
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let Node::File(name) = node else {
            return Err(ErrorCode::Unsupported);
        };
        state.files.remove(&name).ok_or(ErrorCode::NotFound)?;
        state
            .store
            .unbind_root(&root_name(&name))
            .map_err(map_knowledge_error)?;
        state.versions.bump(node_identity(&Node::Root).1);
        state.fids.remove(&fid);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let mut state = self.state.lock().await;
        let f = state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        if let Node::File(name) = f.node
            && f.wrote
        {
            state.put_file(name, &f.write_buf)?;
        }
        Ok(())
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\n')
}

fn root_name(name: &str) -> String {
    format!("mem/{name}")
}

fn node_identity(node: &Node) -> (FileKind, u64) {
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::File(name) => (FileKind::File, name.clone()),
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    (kind, h.finish())
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
