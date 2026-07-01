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
use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Request, Response, Stat,
};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::namespace::{Namespace, Resolved};

/// A process-global allocator for fids bound in backing trees. It must be unique
/// across *every* `MountFs` — two wrappers over namespaces that share a backing
/// server (child namespaces cloning the same `/proc` transport) would otherwise
/// both start at 1 and send colliding `newfid`s into that shared server, so one
/// wrapper's walk would be rejected while the other holds a live fid.
static NEXT_BACKING: AtomicU64 = AtomicU64::new(1);

/// A node reached inside a backing tree: the resolved mount (tree + access), the
/// fid bound in that tree that every forwarded op addresses, and whether the node
/// is a directory (so a listing can be overlaid with mount-point children).
struct Backing {
    resolved: Resolved,
    backing_fid: Fid,
    is_dir: bool,
}

/// What a fid resolves to for a leaf operation, extracted from the fid table so
/// the state lock is released *before* the (possibly blocking) backing call.
enum Target {
    /// A node in a backing tree: its resolved mount, bound fid, whether it is a
    /// directory, and its absolute path (to overlay mount-point children).
    Backing {
        resolved: Resolved,
        backing_fid: Fid,
        is_dir: bool,
        path: Vec<String>,
    },
    /// A synthetic namespace directory at this path.
    Synthetic(Vec<String>),
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
            state: Mutex::new(State { fids }),
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

    /// Resolve a fid to its leaf [`Target`] and release the state lock, so a leaf
    /// op forwards to the backing tree **without** holding the namespace lock — a
    /// tail parked on a stream's live edge must not freeze every other operation.
    async fn target(&self, fid: Fid) -> Result<Target, ErrorCode> {
        let state = self.state.lock().await;
        let entry = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        Ok(match &entry.backing {
            Some(b) => Target::Backing {
                resolved: b.resolved.clone(),
                backing_fid: b.backing_fid,
                is_dir: b.is_dir,
                path: entry.path.clone(),
            },
            None => Target::Synthetic(entry.path.clone()),
        })
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
        for resolved in candidates {
            let backing_fid = Fid(NEXT_BACKING.fetch_add(1, Ordering::Relaxed));
            let walked = resolved
                .call(Request::Walk {
                    fid: Fid::ROOT,
                    newfid: backing_fid,
                    names: resolved.rel.clone(),
                })
                .await;
            if let Ok(Response::Walk { qid }) = walked {
                state.fids.insert(
                    newfid,
                    Entry {
                        path,
                        backing: Some(Backing {
                            resolved,
                            backing_fid,
                            is_dir: qid.kind == FileKind::Dir,
                        }),
                    },
                );
                return Ok(qid);
            }
        }

        // No backing tree resolved this path. Fall through to the synthetic check:
        // an intermediate parent of a deeper mount (e.g. `/mnt` for `/mnt/llm`) is a
        // synthetic directory even when a broader mount (like `/`) exists but does
        // not contain that component — so a component-at-a-time walk still reaches
        // the deeper mount.
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
        match self.target(fid).await? {
            Target::Backing {
                resolved,
                backing_fid,
                ..
            } => {
                match resolved
                    .call(Request::Open {
                        fid: backing_fid,
                        mode,
                    })
                    .await?
                {
                    Response::Open { qid } => Ok(qid),
                    _ => Err(ErrorCode::Io),
                }
            }
            // A synthetic directory is read-only; a write intent is refused.
            Target::Synthetic(path) => {
                if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
                    return Err(ErrorCode::NoAccess);
                }
                Ok(synthetic_qid(&path))
            }
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        match self.target(fid).await? {
            // Forward with the state lock released: a stream read may block at the
            // live edge, and holding the namespace lock across it would freeze every
            // other operation (a tail would deadlock concurrent input).
            Target::Backing {
                resolved,
                backing_fid,
                is_dir,
                path,
            } => {
                // A backed directory that also has mounted descendants must list
                // those mount points too, or a broad mount (e.g. `/`) would hide the
                // deeper mounts (`/proc`, `/mnt/llm`) from its listing. Overlay the
                // synthetic mount-point children onto the backing listing; a file is
                // forwarded byte-for-byte.
                if is_dir {
                    let extra = self.synthetic_children(&path);
                    if !extra.is_empty() {
                        let backing = read_all(&resolved, backing_fid).await?;
                        return Ok(slice(overlay_listing(backing, extra), offset, count));
                    }
                }
                match resolved
                    .call(Request::Read {
                        fid: backing_fid,
                        offset,
                        count,
                    })
                    .await?
                {
                    Response::Read { data } => Ok(data),
                    _ => Err(ErrorCode::Io),
                }
            }
            Target::Synthetic(path) => {
                let listing = self.synthetic_children(&path).join("\n").into_bytes();
                Ok(slice(listing, offset, count))
            }
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        match self.target(fid).await? {
            Target::Backing {
                resolved,
                backing_fid,
                ..
            } => {
                match resolved
                    .call(Request::Write {
                        fid: backing_fid,
                        offset,
                        data: data.to_vec(),
                    })
                    .await?
                {
                    Response::Write { count } => Ok(count),
                    _ => Err(ErrorCode::Io),
                }
            }
            // A synthetic directory is not writable.
            Target::Synthetic(_) => Err(ErrorCode::IsDirectory),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        match self.target(fid).await? {
            Target::Backing {
                resolved,
                backing_fid,
                ..
            } => match resolved.call(Request::Stat { fid: backing_fid }).await? {
                Response::Stat { stat } => Ok(stat),
                _ => Err(ErrorCode::Io),
            },
            Target::Synthetic(path) => {
                let length = self.synthetic_children(&path).join("\n").len() as u64;
                Ok(Stat {
                    name: String::new(),
                    qid: synthetic_qid(&path),
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
        if fid == Fid::ROOT {
            // The root is the pre-bound namespace anchor: never unbind it, or every
            // later walk/open on this handle would fail with NotFound.
            return Err(ErrorCode::Unsupported);
        }
        // Drop the entry under the lock, then forward without it held.
        let backing = {
            let mut state = self.state.lock().await;
            state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?.backing
        };
        match backing {
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
        // Drop the entry under the lock, then forward the backing clunk without the
        // lock held (a commit-on-clunk commit must not serialize the namespace).
        let backing = {
            let mut state = self.state.lock().await;
            let Some(entry) = state.fids.remove(&fid) else {
                return Err(ErrorCode::NotFound);
            };
            entry.backing
        };
        if let Some(b) = backing {
            // The MountFs fid is already dropped (no leak even on error). Propagate
            // the backing clunk result: on a commit-on-clunk endpoint the clunk *is*
            // the commit, so a rejected document must surface, not be swallowed.
            return match b
                .resolved
                .call(Request::Clunk { fid: b.backing_fid })
                .await?
            {
                Response::Clunk => Ok(()),
                _ => Err(ErrorCode::Io),
            };
        }
        Ok(())
    }
}

/// Read a backing node's full contents by looping until a read returns empty
/// (used to get a whole directory listing before overlaying mount points).
async fn read_all(resolved: &Resolved, fid: Fid) -> Result<Vec<u8>, ErrorCode> {
    let mut out = Vec::new();
    loop {
        let chunk = match resolved
            .call(Request::Read {
                fid,
                offset: out.len() as u64,
                count: 4096,
            })
            .await?
        {
            Response::Read { data } => data,
            _ => return Err(ErrorCode::Io),
        };
        if chunk.is_empty() {
            break;
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// Overlay synthetic mount-point child names onto a backing directory listing,
/// appending only names the backing tree does not already list (newline-joined).
fn overlay_listing(backing: Vec<u8>, extra: Vec<String>) -> Vec<u8> {
    let text = String::from_utf8_lossy(&backing);
    let mut names: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    for name in extra {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names.join("\n").into_bytes()
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
