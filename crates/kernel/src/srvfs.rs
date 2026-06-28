//! `/srv` — the bootstrap rendezvous device (§7.2).
//!
//! `/srv` exists before any user-space file server so servers have a place to
//! publish mountable handles and clients have a place to mount from. It is **not
//! an ambient backdoor**: a service withheld from a process is filtered out of
//! its `/srv` and is not remountable — denial-by-absent-mount (D6). A filtered
//! view shares the **live** registry and applies its deny set per operation, so
//! it is a real `FileServer` *and* stays current as services post/restart — not
//! a stale snapshot.
//!
//! In v1 (in-process) a handle is an [`InProcessTransport`]; the aP surface lists
//! handle *names* and the in-process kernel mounts a named handle via
//! [`SrvFs::lookup`] (an Arc cannot ride a byte transport, so the actual channel
//! passes on the fast path). A future wire transport carries a dialable address
//! instead, with the same access-filtered listing.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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

/// The shared, live registry of posted handles. Views hold an `Arc` to the same
/// registry so they observe later posts/restarts.
struct Registry {
    handles: Vec<Handle>,
    next_qid: u64,
}

/// What a fid in `/srv` points at.
#[derive(Clone)]
enum Node {
    Root,
    Handle(String),
}

/// The `/srv` rendezvous device, or an access-filtered view of one. A view shares
/// the same live registry and only adds names to its `denied` set; fids are
/// per-view.
pub struct SrvFs {
    registry: Arc<Mutex<Registry>>,
    denied: HashSet<String>,
    fids: Mutex<HashMap<Fid, Node>>,
}

impl Default for SrvFs {
    fn default() -> Self {
        Self::new()
    }
}

impl SrvFs {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(Mutex::new(Registry {
                handles: Vec::new(),
                next_qid: 1,
            })),
            denied: HashSet::new(),
            fids: Mutex::new(HashMap::new()),
        }
    }

    fn visible(&self, name: &str) -> bool {
        !self.denied.contains(name)
    }

    /// Post a mountable handle under `name`. A repeat post of the same name
    /// **replaces** the previous handle (a restarted service supersedes its stale
    /// transport), so a name identifies exactly one rendezvous entry.
    pub async fn post(&self, name: &str, tree: InProcessTransport, access: Access) {
        let mut reg = self.registry.lock().await;
        let qid_path = reg.next_qid;
        reg.next_qid += 1;
        reg.handles.retain(|h| h.name != name);
        reg.handles.push(Handle {
            name: name.to_string(),
            tree,
            access,
            qid_path,
        });
    }

    /// Every posted handle name visible through this view, in post order.
    pub async fn list(&self) -> Vec<String> {
        self.registry
            .lock()
            .await
            .handles
            .iter()
            .filter(|h| self.visible(&h.name))
            .map(|h| h.name.clone())
            .collect()
    }

    /// Resolve a visible handle to its mountable tree and access, or `None`.
    pub async fn lookup(&self, name: &str) -> Option<(InProcessTransport, Access)> {
        if !self.visible(name) {
            return None;
        }
        self.registry
            .lock()
            .await
            .handles
            .iter()
            .find(|h| h.name == name)
            .map(|h| (h.tree.clone(), h.access))
    }

    /// An access-filtered `/srv` for a restricted process: handles in `denied`
    /// are absent and unresolvable. The view shares the **live** registry, so a
    /// later permitted post/restart on the parent is immediately visible to it.
    pub async fn view(&self, denied: &HashSet<String>) -> SrvFs {
        let mut combined = self.denied.clone();
        combined.extend(denied.iter().cloned());
        SrvFs {
            registry: Arc::clone(&self.registry),
            denied: combined,
            fids: Mutex::new(HashMap::new()),
        }
    }

    /// The qid for a node, looked up against the live registry.
    async fn qid_of(&self, node: &Node) -> Qid {
        let reg = self.registry.lock().await;
        match node {
            // The root version is derived from THIS view's *visible* handles
            // (their names + per-post qid_paths), not a global counter — so a
            // hidden handle's post/replace never changes a restricted view's root
            // version. Otherwise denial-by-absent-mount would leak hidden-service
            // activity through a qid-version side channel (D6).
            Node::Root => {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                for handle in reg.handles.iter().filter(|h| self.visible(&h.name)) {
                    handle.name.hash(&mut h);
                    handle.qid_path.hash(&mut h);
                }
                Qid {
                    kind: FileKind::Dir,
                    version: h.finish() as u32,
                    path: 0,
                }
            }
            Node::Handle(name) => {
                let path = reg
                    .handles
                    .iter()
                    .find(|h| &h.name == name)
                    .map(|h| h.qid_path)
                    .unwrap_or(0);
                Qid {
                    kind: FileKind::File,
                    // A handle's identity changes via a fresh qid_path on each
                    // post; its content within one post does not change.
                    version: 0,
                    path,
                }
            }
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
}

#[async_trait]
impl FileServer for SrvFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        // Hold the fid table across check-and-insert so a concurrent walk reusing
        // the same `newfid` cannot observe it as free and rebind it. The start
        // node is read from the held guard (not `node_of`, which would re-lock and
        // deadlock); the registry lock is always taken *after* the fid lock.
        let mut fids = self.fids.lock().await;
        if newfid == Fid::ROOT || fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let start = if fid == Fid::ROOT {
            Node::Root
        } else {
            fids.get(&fid).cloned().ok_or(ErrorCode::NotFound)?
        };
        let node = match (&start, names) {
            (_, []) => start.clone(),
            (Node::Root, [name]) if self.visible(name) => {
                let present = self
                    .registry
                    .lock()
                    .await
                    .handles
                    .iter()
                    .any(|h| &h.name == name);
                if present {
                    Node::Handle(name.clone())
                } else {
                    return Err(ErrorCode::NotFound);
                }
            }
            (Node::Root, [_]) => return Err(ErrorCode::NotFound),
            _ => return Err(ErrorCode::NotDirectory),
        };
        let qid = self.qid_of(&node).await;
        fids.insert(newfid, node);
        Ok(qid)
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        let node = self.node_of(fid).await?;
        Ok(self.qid_of(&node).await)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match self.node_of(fid).await? {
            // The root lists the handle names visible through this view (live).
            Node::Root => self
                .registry
                .lock()
                .await
                .handles
                .iter()
                .filter(|h| self.visible(&h.name))
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
        let node = self.node_of(fid).await?;
        // Report the readable byte length so clients can size reads.
        let length = match &node {
            Node::Root => {
                let reg = self.registry.lock().await;
                reg.handles
                    .iter()
                    .filter(|h| self.visible(&h.name))
                    .map(|h| h.name.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
                    .len() as u64
            }
            Node::Handle(name) => name.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid: self.qid_of(&node).await,
            length,
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
