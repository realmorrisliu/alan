//! `/srv` — the bootstrap rendezvous device (§7.2).
//!
//! `/srv` exists before any user-space file server so servers have a place to
//! publish mountable handles and clients have a place to mount from. It is **not
//! an ambient backdoor**: a service withheld from a process is filtered out of
//! its `/srv` and is not remountable — denial-by-absent-mount (D6). The filtered
//! view is itself a [`SrvFs`] (a real `FileServer`), so the denial holds on the
//! aP surface a process actually reads, not just in a Rust-side snapshot.
//!
//! In v1 (in-process) a handle is an [`InProcessTransport`]; the aP surface lists
//! handle *names* and the in-process kernel mounts a named handle via
//! [`SrvFs::lookup`] (an Arc cannot ride a byte transport, so the actual channel
//! passes on the fast path). A future wire transport carries a dialable address
//! instead, with the same access-filtered listing.

use std::collections::{HashMap, HashSet};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::Access;

#[derive(Clone)]
struct Handle {
    name: String,
    tree: InProcessTransport,
    access: Access,
    /// Server-unique qid path for this handle instance.
    qid_path: u64,
}

/// What a fid in `/srv` points at.
#[derive(Clone)]
enum Node {
    Root,
    Handle(String),
}

struct SrvState {
    handles: Vec<Handle>,
    fids: HashMap<Fid, Node>,
    next_qid: u64,
}

/// The `/srv` rendezvous registry (or a filtered view of one).
pub struct SrvFs {
    state: Mutex<SrvState>,
}

impl Default for SrvFs {
    fn default() -> Self {
        Self::new()
    }
}

impl SrvFs {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SrvState {
                handles: Vec::new(),
                fids: HashMap::new(),
                next_qid: 1,
            }),
        }
    }

    /// Post a mountable handle under `name`. A repeat post of the same name
    /// **replaces** the previous handle (a restarted service supersedes its stale
    /// transport), so a name identifies exactly one rendezvous entry.
    pub async fn post(&self, name: &str, tree: InProcessTransport, access: Access) {
        let mut state = self.state.lock().await;
        let qid_path = state.next_qid;
        state.next_qid += 1;
        state.handles.retain(|h| h.name != name);
        state.handles.push(Handle {
            name: name.to_string(),
            tree,
            access,
            qid_path,
        });
    }

    /// Every posted handle name, in post order.
    pub async fn list(&self) -> Vec<String> {
        self.state
            .lock()
            .await
            .handles
            .iter()
            .map(|h| h.name.clone())
            .collect()
    }

    /// Resolve a handle to its mountable tree and access, or `None`.
    pub async fn lookup(&self, name: &str) -> Option<(InProcessTransport, Access)> {
        self.state
            .lock()
            .await
            .handles
            .iter()
            .find(|h| h.name == name)
            .map(|h| (h.tree.clone(), h.access))
    }

    /// A real, access-filtered `/srv` for a restricted process: handles in
    /// `denied` are absent from the returned server's listing and unresolvable —
    /// and because it is a [`FileServer`], the denial holds on the aP surface the
    /// process reads, not only in a snapshot.
    pub async fn view(&self, denied: &HashSet<String>) -> SrvFs {
        let state = self.state.lock().await;
        let visible: Vec<Handle> = state
            .handles
            .iter()
            .filter(|h| !denied.contains(&h.name))
            .cloned()
            .collect();
        SrvFs {
            state: Mutex::new(SrvState {
                handles: visible,
                fids: HashMap::new(),
                next_qid: state.next_qid,
            }),
        }
    }
}

impl SrvState {
    fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids.get(&fid).cloned().ok_or(ErrorCode::NotFound)
    }

    fn qid_of(&self, node: &Node) -> Qid {
        match node {
            Node::Root => Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 0,
            },
            Node::Handle(name) => {
                let path = self
                    .handles
                    .iter()
                    .find(|h| &h.name == name)
                    .map(|h| h.qid_path)
                    .unwrap_or(0);
                Qid {
                    kind: FileKind::File,
                    version: 0,
                    path,
                }
            }
        }
    }
}

#[async_trait]
impl FileServer for SrvFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let start = state.node_of(fid)?;
        let node = match (&start, names) {
            (_, []) => start.clone(),
            (Node::Root, [name]) if state.handles.iter().any(|h| &h.name == name) => {
                Node::Handle(name.clone())
            }
            (Node::Root, [_]) => return Err(ErrorCode::NotFound),
            _ => return Err(ErrorCode::NotDirectory),
        };
        let qid = state.qid_of(&node);
        state.fids.insert(newfid, node);
        Ok(qid)
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        let state = self.state.lock().await;
        Ok(state.qid_of(&state.node_of(fid)?))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        let bytes = match state.node_of(fid)? {
            // The root lists the (filtered) handle names this server exposes.
            Node::Root => state
                .handles
                .iter()
                .map(|h| h.name.clone())
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes(),
            // A handle file reads back its name (the rendezvous identity).
            Node::Handle(name) => name.into_bytes(),
        };
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = state.node_of(fid)?;
        Ok(Stat {
            name: String::new(),
            qid: state.qid_of(&node),
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
        self.state.lock().await.fids.remove(&fid);
        Ok(())
    }
}
