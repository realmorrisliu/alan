//! `MountFs` — the per-process [`Namespace`] presented as one aP [`FileServer`].
//!
//! A namespace is a path-addressed mount table; aP is a fid-addressed file
//! protocol. `MountFs` bridges the two so a single client (the shell, the agent
//! engine) reaches a whole assembled namespace — `/proc`, `/agent`, `/mnt/llm` —
//! through one transport, instead of holding a separate handle per mounted
//! server and re-implementing longest-prefix resolution itself.
//!
//! Each fid is one of two things:
//! - a **synthetic directory**: a path that is strictly above every mount (e.g.
//!   `/`, `/mnt`). No backing server owns it; `MountFs` renders it and lists its
//!   child mount points.
//! - a **backing node**: a path at or below a mount. The operation is forwarded
//!   to the backing tree through [`Resolved::call`], so the mount's access rights
//!   are enforced (a read-only mount cannot be written) — the namespace boundary
//!   holds exactly as it does for a directly-mounted server.

use std::collections::HashMap;

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Request, Response, Stat,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::namespace::{Namespace, Resolved};

/// A node reached inside a backing tree: the resolved mount (tree + access) and
/// the fid bound in that tree, which every forwarded op addresses.
struct Backing {
    resolved: Resolved,
    backing_fid: Fid,
}

/// What a `MountFs` fid points at.
struct Entry {
    /// The absolute path components this fid names (so a further walk can extend
    /// it).
    path: Vec<String>,
    /// `Some` when the path is at/below a mount; `None` for a synthetic directory.
    backing: Option<Backing>,
}

struct State {
    fids: HashMap<Fid, Entry>,
    /// Monotonic allocator for fids bound in backing trees. A global counter keeps
    /// them unique even across trees (each tree has its own space, but uniqueness
    /// is harmless and simpler).
    next_backing: u64,
}

/// The namespace-as-`FileServer`.
pub struct MountFs {
    ns: Namespace,
    state: Mutex<State>,
}

impl MountFs {
    /// Wrap an assembled [`Namespace`] as a single aP file server.
    pub fn new(ns: Namespace) -> Self {
        let mut fids = HashMap::new();
        // The root fid is the namespace root: the synthetic directory at `/`.
        fids.insert(
            Fid::ROOT,
            Entry {
                path: Vec::new(),
                backing: None,
            },
        );
        Self {
            ns,
            state: Mutex::new(State {
                fids,
                next_backing: 1,
            }),
        }
    }

    /// The mount prefixes of the namespace, as component vectors.
    fn mount_prefixes(&self) -> Vec<Vec<String>> {
        self.ns
            .describe()
            .into_iter()
            .map(|(path, _)| split_path(&path))
            .collect()
    }

    /// The child mount-point names directly under the synthetic directory `path`,
    /// deduplicated. `path` must be a strict prefix of each contributing mount.
    fn synthetic_children(&self, path: &[String]) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for prefix in self.mount_prefixes() {
            if prefix.len() > path.len() && prefix[..path.len()] == *path {
                let child = prefix[path.len()].clone();
                if !names.contains(&child) {
                    names.push(child);
                }
            }
        }
        names
    }

    /// Whether `path` is a synthetic directory: strictly above at least one mount
    /// (and so not itself at/below a mount).
    fn is_synthetic_dir(&self, path: &[String]) -> bool {
        self.mount_prefixes()
            .iter()
            .any(|prefix| prefix.len() > path.len() && prefix[..path.len()] == *path)
    }
}

#[async_trait]
impl FileServer for MountFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let base = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        let mut path = base.path.clone();
        path.extend(names.iter().cloned());

        // At or below a mount: forward the walk to the backing tree(s), trying each
        // union contributor (longest-prefix, most-recent-first) until one resolves.
        let candidates = self.ns.resolve_candidates(&join_path(&path));
        if !candidates.is_empty() {
            for resolved in candidates {
                let backing_fid = Fid(state.next_backing);
                let walked = resolved
                    .call(Request::Walk {
                        fid: Fid::ROOT,
                        newfid: backing_fid,
                        names: resolved.rel.clone(),
                    })
                    .await;
                if let Ok(Response::Walk { qid }) = walked {
                    state.next_backing += 1;
                    state.fids.insert(
                        newfid,
                        Entry {
                            path,
                            backing: Some(Backing {
                                resolved,
                                backing_fid,
                            }),
                        },
                    );
                    return Ok(qid);
                }
            }
            return Err(ErrorCode::NotFound);
        }

        // Above the mounts: a synthetic directory (its children are mount points).
        if path.is_empty() || self.is_synthetic_dir(&path) {
            let qid = synthetic_qid(&path);
            state.fids.insert(
                newfid,
                Entry {
                    path,
                    backing: None,
                },
            );
            return Ok(qid);
        }

        Err(ErrorCode::NotFound)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let state = self.state.lock().await;
        let entry = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        match &entry.backing {
            Some(b) => match b
                .resolved
                .call(Request::Open {
                    fid: b.backing_fid,
                    mode,
                })
                .await?
            {
                Response::Open { qid } => Ok(qid),
                _ => Err(ErrorCode::Io),
            },
            // A synthetic directory is read-only; a write intent is refused.
            None => {
                if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
                    return Err(ErrorCode::NoAccess);
                }
                Ok(synthetic_qid(&entry.path))
            }
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        let entry = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        match &entry.backing {
            Some(b) => match b
                .resolved
                .call(Request::Read {
                    fid: b.backing_fid,
                    offset,
                    count,
                })
                .await?
            {
                Response::Read { data } => Ok(data),
                _ => Err(ErrorCode::Io),
            },
            None => {
                let listing = self.synthetic_children(&entry.path).join("\n").into_bytes();
                Ok(slice(listing, offset, count))
            }
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let state = self.state.lock().await;
        let entry = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        match &entry.backing {
            Some(b) => match b
                .resolved
                .call(Request::Write {
                    fid: b.backing_fid,
                    offset,
                    data: data.to_vec(),
                })
                .await?
            {
                Response::Write { count } => Ok(count),
                _ => Err(ErrorCode::Io),
            },
            // A synthetic directory is not writable.
            None => Err(ErrorCode::IsDirectory),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let entry = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        match &entry.backing {
            Some(b) => match b
                .resolved
                .call(Request::Stat { fid: b.backing_fid })
                .await?
            {
                Response::Stat { stat } => Ok(stat),
                _ => Err(ErrorCode::Io),
            },
            None => {
                let length = self.synthetic_children(&entry.path).join("\n").len() as u64;
                Ok(Stat {
                    name: String::new(),
                    qid: synthetic_qid(&entry.path),
                    length,
                    writable: false,
                })
            }
        }
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        // v1 does not multiplex create across a mount (the newfid mapping is a
        // later slice); the backing servers do not support it either.
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        let entry = state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        match entry.backing {
            Some(b) => {
                b.resolved
                    .call(Request::Remove { fid: b.backing_fid })
                    .await?;
                Ok(())
            }
            None => Err(ErrorCode::Unsupported),
        }
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            // The root is the pre-bound namespace anchor: never released.
            return Ok(());
        }
        let mut state = self.state.lock().await;
        let Some(entry) = state.fids.remove(&fid) else {
            return Err(ErrorCode::NotFound);
        };
        if let Some(b) = entry.backing {
            // Release the fid bound in the backing tree too, so it does not leak.
            let _ = b.resolved.call(Request::Clunk { fid: b.backing_fid }).await;
        }
        Ok(())
    }
}

/// Split an absolute path into its non-empty components. `/` → `[]`.
fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Join components back into an absolute path (`[]` → `/`).
fn join_path(components: &[String]) -> String {
    if components.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", components.join("/"))
    }
}

/// A stable directory qid for a synthetic namespace path, keyed by the path so
/// distinct synthetic directories never share a qid.
fn synthetic_qid(path: &[String]) -> Qid {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    "synthetic".hash(&mut h);
    path.hash(&mut h);
    Qid {
        kind: FileKind::Dir,
        version: 0,
        path: h.finish(),
    }
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
