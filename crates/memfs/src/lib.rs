//! alan-memfs — Memory Store file server backed by content-addressed knowledge.
//!
//! Callers see ordinary files under the mounted tree (for example
//! `/mnt/mem/facts`) alongside a read-only `recall/<query>` (alias
//! `search/<query>`) query surface powered by `alan-knowledge`'s pure-Rust
//! BM25 + CJK + English recall engine. File bytes are stored as immutable
//! knowledge blocks behind namespace-bound checkpoint roots, tracking per-file
//! checkpoint history for versioned rollbacks.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, VersionTable};
use alan_knowledge::recall::{IndexedDoc, Ranker, SearchHit, format_hits_json};
use alan_knowledge::{BoundRoot, ContentHash, KnowledgeError, KnowledgeStore, RootAccess};
use async_trait::async_trait;
use tokio::sync::Mutex;

const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB
const MAX_CHECKPOINT_HISTORY: usize = 64;
const DEFAULT_SURFACE_SEARCH_LIMIT: usize = 16;
const RECALL_HELP: &str = "# memfs recall/search — read-only BM25 + CJK query surface\n\
Walk `recall/<query>` or `search/<query>` and open ReadOnly to retrieve ranked JSON hits.\n";

/// One recorded content-addressed checkpoint revision for a [`MemFs`] file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryCheckpointRevision {
    /// Monotonic revision number assigned on commit or restore.
    pub revision: u64,
    /// Content-addressed checkpoint root hash for this revision.
    pub root: ContentHash,
}

/// A memory/context file server backed by content-addressed checkpoint roots.
pub struct MemFs {
    state: Mutex<State>,
}

struct State {
    store: KnowledgeStore,
    files: BTreeMap<String, MemoryFile>,
    history: HashMap<String, Vec<MemoryCheckpointRevision>>,
    versions: VersionTable,
    fids: HashMap<Fid, MemFid>,
    next_file_generation: u64,
    next_revision_seq: u64,
}

struct MemoryFile {
    root: BoundRoot,
    generation: u64,
    recency: u64,
}

struct MemFid {
    node: Node,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
    wrote: bool,
    write_failed: bool,
    file_generation: Option<u64>,
}

#[derive(Clone)]
enum Node {
    Root,
    File(String),
    RecallDir(&'static str),
    RecallQuery { dir: &'static str, query: String },
}

impl Default for MemFs {
    fn default() -> Self {
        Self::new()
    }
}

/// A read-only projection of a [`MemFs`] store for child workstreams.
///
/// Allows `walk`, `open(Read)`, `read`, `stat`, `clunk`, and `recall/<query>` /
/// `search/<query>` BM25 + CJK queries while denying all mutations (`create`,
/// `open(Write | ReadWrite)`, `write`, `remove`) with [`ErrorCode::NoAccess`].
pub struct ReadOnlyMemFs {
    inner: Arc<MemFs>,
    fids: Mutex<HashMap<Fid, MemFid>>,
}

impl MemFs {
    /// Create an empty memory file server.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                store: KnowledgeStore::new(),
                files: BTreeMap::new(),
                history: HashMap::new(),
                versions: VersionTable::new(),
                fids: HashMap::new(),
                next_file_generation: 1,
                next_revision_seq: 1,
            }),
        }
    }

    /// Create an independent read-only [`FileServer`] view backed by this [`MemFs`].
    pub fn read_only(self: &Arc<Self>) -> ReadOnlyMemFs {
        ReadOnlyMemFs {
            inner: Arc::clone(self),
            fids: Mutex::new(HashMap::new()),
        }
    }

    /// List all committed file names in deterministic sorted order.
    pub async fn list_files(&self) -> Vec<String> {
        let state = self.state.lock().await;
        state.files.keys().cloned().collect()
    }

    /// Atomically commit one named file and return its new checkpoint revision.
    pub async fn commit_file(
        &self,
        name: impl Into<String>,
        bytes: &[u8],
    ) -> Result<MemoryCheckpointRevision, ErrorCode> {
        let name = name.into();
        let mut results = self
            .commit_files_atomic(vec![(name, bytes.to_vec())])
            .await?;
        results.pop().map(|(_, rev)| rev).ok_or(ErrorCode::Io)
    }

    /// Atomically validate and commit a batch of named files under a single lock.
    ///
    /// Either every file in `updates` validates and commits, or no state is mutated.
    pub async fn commit_files_atomic(
        &self,
        updates: Vec<(String, Vec<u8>)>,
    ) -> Result<Vec<(String, MemoryCheckpointRevision)>, ErrorCode> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }
        let mut seen = std::collections::BTreeSet::new();
        for (name, bytes) in &updates {
            if !valid_name(name) || bytes.len() > MAX_DOC_BYTES || !seen.insert(name.as_str()) {
                return Err(ErrorCode::BadRequest);
            }
        }
        let mut state = self.state.lock().await;
        let mut staged = Vec::with_capacity(updates.len());
        for (name, bytes) in &updates {
            let root = state
                .store
                .checkpoint_from_bytes([bytes.as_slice()])
                .map_err(map_knowledge_error)?;
            staged.push((name.clone(), root));
        }
        let mut committed = Vec::with_capacity(staged.len());
        for (name, root) in staged {
            let generation = match state.files.get(&name) {
                Some(file) => file.generation,
                None => state.next_generation(),
            };
            state.bind_file_root(name.clone(), root, generation)?;
            let rev = state
                .history
                .get(&name)
                .and_then(|revs| revs.last())
                .cloned()
                .ok_or(ErrorCode::Io)?;
            committed.push((name, rev));
        }
        Ok(committed)
    }

    /// Delete a named memory file and unbind its root.
    pub async fn delete_file(&self, name: &str) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        state.remove_named_file(name)
    }

    /// Revert a named file to a specific recorded [`MemoryCheckpointRevision::revision`].
    pub async fn revert_to_revision(
        &self,
        name: &str,
        revision: u64,
    ) -> Result<ContentHash, ErrorCode> {
        let mut state = self.state.lock().await;
        if !state.files.contains_key(name) {
            return Err(ErrorCode::NotFound);
        }
        let target_root = state
            .history
            .get(name)
            .and_then(|revs| revs.iter().find(|rev| rev.revision == revision))
            .map(|rev| rev.root.clone())
            .ok_or(ErrorCode::NotFound)?;
        state
            .store
            .verify_root_hash(&target_root)
            .map_err(map_knowledge_error)?;
        let generation = state.next_generation();
        state.bind_file_root(name.to_string(), target_root.clone(), generation)?;
        Ok(target_root)
    }

    /// Return the current checkpoint root for a named file.
    pub async fn checkpoint_root(&self, name: &str) -> Result<ContentHash, ErrorCode> {
        let state = self.state.lock().await;
        let file = state.files.get(name).ok_or(ErrorCode::NotFound)?;
        Ok(file.root.root_hash().clone())
    }

    /// Return the recorded checkpoint revision history for a named file (oldest to newest).
    pub async fn checkpoint_history(
        &self,
        name: &str,
    ) -> Result<Vec<MemoryCheckpointRevision>, ErrorCode> {
        let state = self.state.lock().await;
        if !state.files.contains_key(name) {
            return Err(ErrorCode::NotFound);
        }
        Ok(state.history.get(name).cloned().unwrap_or_default())
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
        let generation = state.next_generation();
        state.bind_file_root(name, root, generation)?;
        Ok(())
    }

    /// Revert a named file to an explicit historical checkpoint root.
    pub async fn revert_to_checkpoint(
        &self,
        name: &str,
        target_root: &ContentHash,
    ) -> Result<ContentHash, ErrorCode> {
        let mut state = self.state.lock().await;
        if !state.files.contains_key(name) {
            return Err(ErrorCode::NotFound);
        }
        let in_history = state
            .history
            .get(name)
            .is_some_and(|revs| revs.iter().any(|rev| &rev.root == target_root));
        if !in_history && !state.store.contains_node(target_root) {
            return Err(ErrorCode::NotFound);
        }
        state
            .store
            .verify_root_hash(target_root)
            .map_err(map_knowledge_error)?;
        let generation = state.next_generation();
        state.bind_file_root(name.to_string(), target_root.clone(), generation)?;
        Ok(target_root.clone())
    }

    /// Revert a named file to its immediately preceding distinct checkpoint root.
    pub async fn revert_previous(&self, name: &str) -> Result<ContentHash, ErrorCode> {
        let mut state = self.state.lock().await;
        let current_root = state
            .files
            .get(name)
            .ok_or(ErrorCode::NotFound)?
            .root
            .root_hash()
            .clone();
        let target_root = state
            .history
            .get(name)
            .and_then(|revs| {
                revs.iter()
                    .rev()
                    .find(|rev| rev.root != current_root)
                    .map(|rev| rev.root.clone())
            })
            .ok_or(ErrorCode::NotFound)?;
        state
            .store
            .verify_root_hash(&target_root)
            .map_err(map_knowledge_error)?;
        let generation = state.next_generation();
        state.bind_file_root(name.to_string(), target_root.clone(), generation)?;
        Ok(target_root)
    }

    /// Search live UTF-8 memory files using `alan-knowledge`'s BM25 + CJK ranker.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, ErrorCode> {
        let state = self.state.lock().await;
        state.search_files(query, limit)
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

    fn search_files(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, ErrorCode> {
        let mut docs = Vec::with_capacity(self.files.len());
        for (name, file) in &self.files {
            let bytes = self
                .store
                .read_bound_root(&file.root)
                .map_err(map_knowledge_error)?;
            if let Ok(text) = String::from_utf8(bytes) {
                docs.push(
                    IndexedDoc::new(name.clone(), format!("{name}\n{text}"))
                        .with_content_hash(file.root.root_hash().clone())
                        .with_recency(file.recency),
                );
            }
        }
        let ranker = Ranker::new(docs);
        Ok(ranker.search(query, limit))
    }

    fn render_query_json(&self, query: &str) -> Result<Vec<u8>, ErrorCode> {
        let hits = self.search_files(query, DEFAULT_SURFACE_SEARCH_LIMIT)?;
        let json = format_hits_json(&hits).map_err(|_| ErrorCode::Io)?;
        Ok(format!("{json}\n").into_bytes())
    }

    fn next_generation(&mut self) -> u64 {
        let generation = self.next_file_generation;
        self.next_file_generation = self.next_file_generation.wrapping_add(1);
        generation
    }

    fn next_revision(&mut self) -> u64 {
        let revision = self.next_revision_seq;
        self.next_revision_seq = self.next_revision_seq.wrapping_add(1);
        revision
    }

    fn put_file(&mut self, name: String, bytes: &[u8]) -> Result<u64, ErrorCode> {
        let generation = match self.files.get(&name) {
            Some(file) => file.generation,
            None => self.next_generation(),
        };
        self.put_file_at_generation(name, bytes, generation)?;
        Ok(generation)
    }

    fn put_existing_file(
        &mut self,
        name: String,
        expected_generation: u64,
        bytes: &[u8],
    ) -> Result<(), ErrorCode> {
        let file = self.files.get(&name).ok_or(ErrorCode::NotFound)?;
        if file.generation != expected_generation {
            return Err(ErrorCode::NotFound);
        }
        self.put_file_at_generation(name, bytes, expected_generation)
    }

    fn put_file_at_generation(
        &mut self,
        name: String,
        bytes: &[u8],
        generation: u64,
    ) -> Result<(), ErrorCode> {
        let root = self
            .store
            .checkpoint_from_bytes([bytes])
            .map_err(map_knowledge_error)?;
        self.bind_file_root(name, root, generation)
    }

    fn bind_file_root(
        &mut self,
        name: String,
        root: ContentHash,
        generation: u64,
    ) -> Result<(), ErrorCode> {
        let binding = self
            .store
            .bind_root(root_name(&name), root.clone(), RootAccess::ReadWrite)
            .map_err(map_knowledge_error)?;
        let _ = self.store.pin_root(&root);
        let revision = self.next_revision();
        let history = self.history.entry(name.clone()).or_default();
        if history.last().is_none_or(|last| last.root != root) {
            history.push(MemoryCheckpointRevision { revision, root });
            while history.len() > MAX_CHECKPOINT_HISTORY {
                let removed = history.remove(0);
                let still_referenced = history.iter().any(|rev| rev.root == removed.root);
                if !still_referenced {
                    self.store.unpin_root(&removed.root);
                }
            }
        }
        self.files.insert(
            name.clone(),
            MemoryFile {
                root: binding,
                generation,
                recency: revision,
            },
        );
        self.versions.bump(node_identity(&Node::Root).1);
        self.versions.bump(node_identity(&Node::File(name)).1);
        Ok(())
    }

    fn step_node(&self, node: &Node, segment: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match segment {
                "recall" => Ok(Node::RecallDir("recall")),
                "search" => Ok(Node::RecallDir("search")),
                _ if self.files.contains_key(segment) => Ok(Node::File(segment.to_string())),
                _ => Err(ErrorCode::NotFound),
            },
            Node::RecallDir(dir) => {
                if segment.is_empty() || segment.contains('\n') {
                    return Err(ErrorCode::BadRequest);
                }
                Ok(Node::RecallQuery {
                    dir,
                    query: segment.to_string(),
                })
            }
            Node::File(_) | Node::RecallQuery { .. } => Err(ErrorCode::NotDirectory),
        }
    }

    fn remove_named_file(&mut self, name: &str) -> Result<(), ErrorCode> {
        self.files.remove(name).ok_or(ErrorCode::NotFound)?;
        if let Some(revisions) = self.history.remove(name) {
            for rev in revisions {
                self.store.unpin_root(&rev.root);
            }
        }
        self.store
            .unbind_root(&root_name(name))
            .map_err(map_knowledge_error)?;
        self.versions.bump(node_identity(&Node::Root).1);
        self.versions
            .bump(node_identity(&Node::File(name.to_string())).1);
        Ok(())
    }

    fn read_node_bytes(&self, node: Node) -> Result<Vec<u8>, ErrorCode> {
        match node {
            Node::Root => Ok(self
                .files
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes()),
            Node::File(name) => self.materialize(&name),
            Node::RecallDir(_) => Ok(RECALL_HELP.as_bytes().to_vec()),
            Node::RecallQuery { query, .. } => self.render_query_json(&query),
        }
    }

    fn stat_node(&self, node: &Node, read_only: bool) -> Result<Stat, ErrorCode> {
        let length = match node {
            Node::Root => self
                .files
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
                .len() as u64,
            Node::File(name) => self.materialize(name)?.len() as u64,
            Node::RecallDir(_) => RECALL_HELP.len() as u64,
            Node::RecallQuery { query, .. } => self.render_query_json(query)?.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid: self.qid(node),
            length,
            executable: false,
            writable: !read_only && is_writable_node(node),
        })
    }
}

#[async_trait]
impl FileServer for MemFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = state.node_of(fid)?;
        for name in names {
            node = state.step_node(&node, name)?;
        }
        let file_generation = match &node {
            Node::File(name) => Some(state.files.get(name).ok_or(ErrorCode::NotFound)?.generation),
            Node::Root | Node::RecallDir(_) | Node::RecallQuery { .. } => None,
        };
        let qid = state.qid(&node);
        state.fids.insert(
            newfid,
            MemFid {
                node,
                mode: None,
                write_buf: Vec::new(),
                wrote: false,
                write_failed: false,
                file_generation,
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
        if !is_writable_node(&node) && matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            return Err(ErrorCode::NoAccess);
        }
        if let Node::File(name) = &node
            && !state.files.contains_key(name)
        {
            return Err(ErrorCode::NotFound);
        }
        let read_write_base = if matches!(mode, OpenMode::ReadWrite) {
            match &node {
                Node::File(name) => Some(state.materialize(name)?),
                Node::Root | Node::RecallDir(_) | Node::RecallQuery { .. } => None,
            }
        } else {
            None
        };
        if fid != Fid::ROOT {
            let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            f.mode = Some(mode);
            if let Some(bytes) = read_write_base {
                f.write_buf = bytes;
            }
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
            Node::RecallDir(_) => RECALL_HELP.as_bytes().to_vec(),
            Node::RecallQuery { query, .. } => state.render_query_json(&query)?,
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
        let Some(start) = usize::try_from(offset).ok() else {
            f.write_failed = true;
            return Err(ErrorCode::BadRequest);
        };
        let Some(end) = start.checked_add(data.len()) else {
            f.write_failed = true;
            return Err(ErrorCode::BadRequest);
        };
        if end > MAX_DOC_BYTES {
            f.write_failed = true;
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
            Node::RecallDir(_) => RECALL_HELP.len() as u64,
            Node::RecallQuery { query, .. } => state.render_query_json(query)?.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid: state.qid(&node),
            length,
            executable: false,
            writable: is_writable_node(&node),
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
        let file_generation = state.put_file(name.to_string(), b"")?;
        let qid = state.qid(&node);
        state.fids.insert(
            newfid,
            MemFid {
                node,
                mode: None,
                write_buf: Vec::new(),
                wrote: false,
                write_failed: false,
                file_generation: Some(file_generation),
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
        state.remove_named_file(&name)?;
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
            && !f.write_failed
        {
            let generation = f.file_generation.ok_or(ErrorCode::NotFound)?;
            state.put_existing_file(name, generation, &f.write_buf)?;
        }
        Ok(())
    }
}

#[async_trait]
impl FileServer for ReadOnlyMemFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let state = self.inner.state.lock().await;
        let mut ro_fids = self.fids.lock().await;
        if newfid == Fid::ROOT || ro_fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = if fid == Fid::ROOT {
            Node::Root
        } else {
            ro_fids
                .get(&fid)
                .map(|f| f.node.clone())
                .ok_or(ErrorCode::NotFound)?
        };
        for name in names {
            node = state.step_node(&node, name)?;
        }
        let qid = state.qid(&node);
        ro_fids.insert(
            newfid,
            MemFid {
                node,
                mode: None,
                write_buf: Vec::new(),
                wrote: false,
                write_failed: false,
                file_generation: None,
            },
        );
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            return Err(ErrorCode::NoAccess);
        }
        let state = self.inner.state.lock().await;
        let mut ro_fids = self.fids.lock().await;
        let node = if fid == Fid::ROOT {
            Node::Root
        } else {
            let f = ro_fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if f.mode.is_some() {
                return Err(ErrorCode::BadRequest);
            }
            f.node.clone()
        };
        if let Node::File(name) = &node
            && !state.files.contains_key(name)
        {
            return Err(ErrorCode::NotFound);
        }
        if fid != Fid::ROOT {
            let f = ro_fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            f.mode = Some(mode);
        }
        Ok(state.qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let state = self.inner.state.lock().await;
        let ro_fids = self.fids.lock().await;
        let node = if fid == Fid::ROOT {
            Node::Root
        } else {
            let f = ro_fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if !matches!(f.mode, Some(OpenMode::Read)) {
                return Err(ErrorCode::NoAccess);
            }
            f.node.clone()
        };
        let bytes = state.read_node_bytes(node)?;
        Ok(slice(bytes, offset, count))
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.inner.state.lock().await;
        let ro_fids = self.fids.lock().await;
        let node = if fid == Fid::ROOT {
            Node::Root
        } else {
            ro_fids
                .get(&fid)
                .map(|f| f.node.clone())
                .ok_or(ErrorCode::NotFound)?
        };
        state.stat_node(&node, true)
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let mut ro_fids = self.fids.lock().await;
        ro_fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        Ok(())
    }
}

fn is_writable_node(node: &Node) -> bool {
    matches!(node, Node::File(_))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\n')
        && !matches!(name, "recall" | "search")
}

fn root_name(name: &str) -> String {
    format!("mem/{name}")
}

fn node_identity(node: &Node) -> (FileKind, u64) {
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::File(name) => (FileKind::File, name.clone()),
        Node::RecallDir(dir) => (FileKind::Dir, format!("/{dir}")),
        Node::RecallQuery { dir, query } => (FileKind::File, format!("/{dir}/{query}")),
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
