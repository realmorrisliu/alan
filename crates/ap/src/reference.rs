//! A reference in-memory [`FileServer`] used to exercise the aP conventions and
//! as a worked template for real servers (and alan-shell's M1 echo milestone).
//!
//! It is intentionally small but conformant on the points that are easy to get
//! wrong: the fid lifecycle (§5.2), clone-via-open allocating independent
//! resources (§5.4), and commit-on-clunk document writes whose malformed
//! commit is a commit-time [`ErrorCode::BadRequest`] (§5.5). It is not the
//! kernel's `/proc`/`/srv` and stores no durable state.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};

/// Upper bound on a buffered commit-on-clunk document, so a huge/sparse write
/// offset cannot make the reference server allocate unbounded memory.
const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

type NodeId = usize;

enum Node {
    Dir(BTreeMap<String, NodeId>),
    Bytes(Vec<u8>),
    /// A clone file: each `open` allocates a fresh connection directory.
    Clone {
        next: u64,
    },
    /// A commit-on-clunk document file that must hold valid JSON at commit.
    Doc,
}

struct FidState {
    node: NodeId,
    mode: Option<OpenMode>,
    /// Buffered document for a commit-on-clunk write, committed at `clunk`.
    write_buf: Vec<u8>,
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
            clone_name: None,
        }
    }
}

struct State {
    nodes: Vec<Node>,
    fids: HashMap<Fid, FidState>,
}

impl State {
    fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn qid(&self, node: NodeId) -> Qid {
        let kind = match self.nodes[node] {
            Node::Dir(_) => FileKind::Dir,
            Node::Bytes(_) => FileKind::File,
            Node::Clone { .. } => FileKind::Clone,
            Node::Doc => FileKind::File,
        };
        Qid {
            kind,
            version: 0,
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
    pub fn new() -> Self {
        // Root is node 0 so `Fid::ROOT` can be pre-bound to it.
        let nodes = vec![Node::Dir(BTreeMap::new())];
        let mut state = State {
            nodes,
            fids: HashMap::new(),
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
                Node::Bytes(b) => b.clone(),
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
        let node = state.fid(fid)?.node;
        match state.nodes[node] {
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
                // Honor the byte offset: the aP contract addresses writes by
                // offset, so place bytes at `offset` (out-of-order, retried, or
                // overwriting chunks build the document the caller addressed),
                // rather than blindly appending. Use checked arithmetic so a
                // hostile/huge offset returns an aP error instead of panicking.
                let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
                let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
                // Bound the buffered document: a representable-but-huge offset (a
                // sparse 1 TiB write) would otherwise resize/zero-fill gigabytes
                // and OOM the in-process server. Cap the addressable size so it
                // returns an aP error instead.
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
        let f = state.fid(fid)?;
        let qid = state.qid(f.node);
        let length = match &state.nodes[f.node] {
            Node::Bytes(b) => b.len() as u64,
            _ => 0,
        };
        Ok(Stat {
            name: String::new(),
            qid,
            length,
            writable: matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)),
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
            // Root is the reusable pre-bound anchor: never remove it, but clear
            // its per-open state so it can be opened again (otherwise the
            // reopen guard would lock root out for the server's lifetime).
            let mut state = self.state.lock().await;
            if let Some(root) = state.fids.get_mut(&Fid::ROOT) {
                root.mode = None;
                root.clone_name = None;
                root.write_buf.clear();
            }
            return Ok(());
        }
        let mut state = self.state.lock().await;
        let f = state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        // Commit-on-clunk validation: a Doc opened for write must hold a valid
        // document at commit, else a commit-time error (§5.5).
        if matches!(state.nodes[f.node], Node::Doc)
            && matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite))
        {
            serde_json::from_slice::<serde_json::Value>(&f.write_buf)
                .map_err(|_| ErrorCode::BadRequest)?;
        }
        Ok(())
    }
}

/// Whether `node` can service an open in `mode`. Read-only nodes (bytes,
/// directories, the clone file) serve only `Read`; the write-only document
/// serves only `Write`. Anything else is a dial-time access error, not a
/// success that fails later.
fn serviceable(node: &Node, mode: OpenMode) -> bool {
    match node {
        Node::Doc => matches!(mode, OpenMode::Write),
        Node::Bytes(_) | Node::Dir(_) | Node::Clone { .. } => matches!(mode, OpenMode::Read),
    }
}
