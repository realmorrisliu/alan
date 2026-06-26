//! `/srv` — the bootstrap rendezvous device (§7.2).
//!
//! `/srv` exists before any user-space file server so servers have a place to
//! publish mountable handles and clients have a place to mount from. It is **not
//! an ambient backdoor**: each posted handle carries access rights, and a
//! process sees and mounts only the handles its namespace/access permit. A
//! service withheld from a process (filtered out of its `/srv` view) is not
//! remountable — denial-by-absent-mount (D6).
//!
//! In v1 (in-process) a handle is an [`InProcessTransport`]; the aP surface
//! lists handle *names*, and the in-process kernel mounts a named handle via
//! [`SrvFs::lookup`] (an Arc cannot ride a byte transport, so the actual channel
//! passes on the fast path). A future wire transport carries a dialable address
//! instead, with the same access-filtered listing.

use std::collections::HashSet;

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
}

/// The `/srv` rendezvous registry.
pub struct SrvFs {
    handles: Mutex<Vec<Handle>>,
}

impl Default for SrvFs {
    fn default() -> Self {
        Self::new()
    }
}

impl SrvFs {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
        }
    }

    /// Post a mountable handle under `name` with the given access rights.
    pub async fn post(&self, name: &str, tree: InProcessTransport, access: Access) {
        self.handles.lock().await.push(Handle {
            name: name.to_string(),
            tree,
            access,
        });
    }

    /// Every posted handle name, in post order (the unfiltered view).
    pub async fn list(&self) -> Vec<String> {
        self.handles
            .lock()
            .await
            .iter()
            .map(|h| h.name.clone())
            .collect()
    }

    /// Resolve a handle to its mountable tree and access. Returns `None` if no
    /// such handle is posted.
    pub async fn lookup(&self, name: &str) -> Option<(InProcessTransport, Access)> {
        self.handles
            .lock()
            .await
            .iter()
            .find(|h| h.name == name)
            .map(|h| (h.tree.clone(), h.access))
    }

    /// An access-filtered view of `/srv` for a restricted process: handles in
    /// `denied` are absent from listing and unresolvable, so a withheld service
    /// cannot be regained via `/srv`.
    pub async fn view(&self, denied: &HashSet<String>) -> SrvView {
        let visible: Vec<Handle> = self
            .handles
            .lock()
            .await
            .iter()
            .filter(|h| !denied.contains(&h.name))
            .cloned()
            .collect();
        SrvView { visible }
    }
}

/// A filtered snapshot of `/srv` as seen by one process.
pub struct SrvView {
    visible: Vec<Handle>,
}

impl SrvView {
    pub fn list(&self) -> Vec<String> {
        self.visible.iter().map(|h| h.name.clone()).collect()
    }

    pub fn lookup(&self, name: &str) -> Option<(InProcessTransport, Access)> {
        self.visible
            .iter()
            .find(|h| h.name == name)
            .map(|h| (h.tree.clone(), h.access))
    }
}

#[async_trait]
impl FileServer for SrvFs {
    async fn walk(&self, _fid: Fid, _newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        // `/srv` is flat: the root plus one level of handle names.
        match names {
            [] => Ok(Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 0,
            }),
            [name] => {
                if self.handles.lock().await.iter().any(|h| &h.name == name) {
                    Ok(Qid {
                        kind: FileKind::File,
                        version: 0,
                        path: 1,
                    })
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Dir,
            version: 0,
            path: 0,
        })
    }

    async fn read(&self, _fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let listing = self.list().await.join("\n").into_bytes();
        let start = (offset as usize).min(listing.len());
        let end = listing.len().min(start + count as usize);
        Ok(listing[start..end].to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: "srv".to_string(),
            qid: Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 0,
            },
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

    async fn clunk(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}
