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
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Request,
    Response, Stat,
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
    /// Temporarily reserves a caller-visible fid while a backing create is in
    /// flight. Normal operations cannot use it until the backing tree succeeds and
    /// this entry is replaced with a real backing fid.
    reserved: bool,
}

struct State {
    fids: HashMap<Fid, Entry>,
}

/// The namespace-as-`FileServer`.
pub struct MountFs {
    ns: LiveNamespace,
    state: Mutex<State>,
}

/// Shared, live mount table for a running namespace.
///
/// The handle is host-agnostic: callers provide an already-constructed aP file
/// server transport plus mount access. Existing `MountFs` fids keep their
/// resolved backing server; future walks observe mount-table changes.
#[derive(Clone)]
pub struct LiveNamespace {
    ns: Arc<RwLock<Namespace>>,
    generation: Arc<AtomicU32>,
}

impl std::fmt::Debug for LiveNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveNamespace")
            .field("mounts", &self.describe())
            .finish()
    }
}

impl LiveNamespace {
    /// Create a live handle around an assembled namespace.
    pub fn new(ns: Namespace) -> Self {
        Self {
            ns: Arc::new(RwLock::new(ns)),
            generation: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Mount a file server at `at`, preserving normal namespace stacking rules.
    pub fn mount(&self, at: &str, tree: InProcessTransport, access: crate::Access) {
        self.write().mount(at, tree, access);
        self.bump_generation();
    }

    /// Replace every exact mount at `at`, then mount the supplied file server.
    pub fn replace_mount(&self, at: &str, tree: InProcessTransport, access: crate::Access) {
        let mut ns = self.write();
        ns.unmount(at);
        ns.mount(at, tree, access);
        drop(ns);
        self.bump_generation();
    }

    /// Remove every exact mount at `at`; future walks lose that authority.
    pub fn unmount(&self, at: &str) {
        self.write().unmount(at);
        self.bump_generation();
    }

    /// Return an inspectable summary of the current mount table.
    pub fn describe(&self) -> Vec<(String, crate::Access)> {
        self.read().describe()
    }

    /// Return a point-in-time namespace snapshot for process spawn inheritance.
    pub fn snapshot(&self) -> Namespace {
        self.read().clone()
    }

    /// Monotonic mount-table version used by qid/cache invalidation.
    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::Relaxed)
    }

    fn resolve_candidates(&self, path: &str) -> Vec<Resolved> {
        self.read().resolve_candidates(path)
    }

    fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Namespace> {
        self.ns.read().expect("live namespace lock poisoned")
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Namespace> {
        self.ns.write().expect("live namespace lock poisoned")
    }
}

impl MountFs {
    /// Wrap an assembled [`Namespace`] as a single aP file server.
    pub fn new(ns: Namespace) -> Self {
        Self::from_live_namespace(LiveNamespace::new(ns))
    }

    /// Wrap a live namespace mount handle as a single aP file server.
    pub fn from_live_namespace(ns: LiveNamespace) -> Self {
        let mut fids = HashMap::new();
        // The root fid is the namespace root: the synthetic directory at `/`.
        fids.insert(
            Fid::ROOT,
            Entry {
                path: Vec::new(),
                backing: None,
                reserved: false,
            },
        );
        Self {
            ns,
            state: Mutex::new(State { fids }),
        }
    }

    /// Return the live namespace handle backing this file server.
    pub fn live_namespace(&self) -> LiveNamespace {
        self.ns.clone()
    }

    /// Release every backing fid owned by this namespace view.
    ///
    /// Local wire connections call this when they disconnect so connection
    /// lifetime and fid lifetime remain identical even though the mounted file
    /// servers outlive the connection.
    pub async fn clunk_all(&self) {
        let backings = {
            let mut state = self.state.lock().await;
            std::mem::take(&mut state.fids)
                .into_values()
                .filter_map(|entry| {
                    entry
                        .backing
                        .map(|backing| (backing.resolved, backing.backing_fid))
                })
                .collect::<Vec<_>>()
        };
        for (resolved, fid) in backings {
            let _ = resolved.call(Request::Clunk { fid }).await;
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

    /// Merge a directory listing across every union contributor at `path` plus the
    /// synthetic mount-point children, deduplicated by name. Each contributor is
    /// read through its own fresh backing fid (walk → open → read → clunk); a
    /// contributor that does not resolve the path is skipped. Runs without the
    /// state lock held (it only reads the immutable namespace).
    async fn merged_dir_listing(&self, path: &[String]) -> Result<Vec<u8>, ErrorCode> {
        let mut names: Vec<String> = Vec::new();
        let push = |name: String, names: &mut Vec<String>| {
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        };
        for cand in self.ns.resolve_candidates(&join_path(path)) {
            let backing_fid = Fid(NEXT_BACKING.fetch_add(1, Ordering::Relaxed));
            // A contributor that simply lacks this path (NotFound) is skipped; any
            // other error (Io/RateLimited/NoAccess) is an operational failure and
            // must surface, not be masked as a partial/empty listing.
            match cand
                .call(Request::Walk {
                    fid: Fid::ROOT,
                    newfid: backing_fid,
                    names: cand.rel.clone(),
                })
                .await
            {
                Ok(Response::Walk { .. }) => {}
                Err(ErrorCode::NotFound) => continue,
                Err(e) => return Err(e),
                Ok(_) => return Err(ErrorCode::Io),
            }
            // Read this contributor's listing, clunking on every path.
            let listing = read_contributor_listing(&cand, backing_fid).await;
            let _ = cand.call(Request::Clunk { fid: backing_fid }).await;
            let bytes = match listing {
                Ok(bytes) => bytes,
                Err(ErrorCode::NotFound) => continue,
                Err(e) => return Err(e),
            };
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines() {
                push(line.to_string(), &mut names);
            }
        }
        for child in self.synthetic_children(path) {
            push(child, &mut names);
        }
        Ok(names.join("\n").into_bytes())
    }

    /// Resolve a fid to its leaf [`Target`] and release the state lock, so a leaf
    /// op forwards to the backing tree **without** holding the namespace lock — a
    /// tail parked on a stream's live edge must not freeze every other operation.
    async fn target(&self, fid: Fid) -> Result<Target, ErrorCode> {
        let state = self.state.lock().await;
        let entry = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        if entry.reserved {
            return Err(ErrorCode::BadRequest);
        }
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
        // Resolve the target path under the lock, then RELEASE it before
        // forwarding the walk to the backing tree(s): a backing walk can block
        // (it may itself resolve through another server that is mid-operation),
        // and holding the namespace lock across it would freeze every other
        // operation — the same discipline `target()`/`read`/`open`/`write` use.
        let path = {
            let state = self.state.lock().await;
            if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            let base = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if base.reserved {
                return Err(ErrorCode::BadRequest);
            }
            // A non-empty walk descends into a child, so the base must be a
            // directory. A backing *file* base is rejected here rather than
            // re-resolving the absolute path (which could otherwise traverse a
            // non-directory into a mounted descendant, e.g. a `/` file `mnt`
            // walking into a `/mnt/llm` mount).
            if !names.is_empty()
                && let Some(b) = &base.backing
                && !b.is_dir
            {
                return Err(ErrorCode::NotDirectory);
            }
            let mut path = base.path.clone();
            path.extend(names.iter().cloned());
            path
        };

        // At or below a mount: forward the walk to the backing tree(s), trying each
        // union contributor (longest-prefix, most-recent-first) until one resolves.
        // The namespace lock is not held across these forwarded calls.
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
                let mut state = self.state.lock().await;
                // A concurrent walk may have claimed `newfid` while the lock was
                // released; discard our backing fid and reject rather than leak.
                if state.fids.contains_key(&newfid) {
                    drop(state);
                    let _ = resolved.call(Request::Clunk { fid: backing_fid }).await;
                    return Err(ErrorCode::BadRequest);
                }
                state.fids.insert(
                    newfid,
                    Entry {
                        path,
                        backing: Some(Backing {
                            resolved,
                            backing_fid,
                            is_dir: qid.kind == FileKind::Dir,
                        }),
                        reserved: false,
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
            let qid = synthetic_qid(&path, self.ns.generation());
            let mut state = self.state.lock().await;
            if state.fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            state.fids.insert(
                newfid,
                Entry {
                    path,
                    backing: None,
                    reserved: false,
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
                Ok(synthetic_qid(&path, self.ns.generation()))
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
                // A directory listing must merge every contributor at this prefix
                // (a union mount such as `/bin`) plus the synthetic mount-point
                // children (deeper mounts under a broad mount), so neither a union
                // sibling nor a nested mount is hidden. A single non-union directory
                // with no mounted descendants, and every file, is forwarded
                // byte-for-byte.
                if is_dir {
                    let is_union = self.ns.resolve_candidates(&join_path(&path)).len() > 1;
                    if is_union || !self.synthetic_children(&path).is_empty() {
                        return Ok(slice(self.merged_dir_listing(&path).await?, offset, count));
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
                    qid: synthetic_qid(&path, self.ns.generation()),
                    length,
                    executable: false,
                    writable: false,
                })
            }
        }
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let (resolved, backing_fid, path) = {
            let mut state = self.state.lock().await;
            if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            let entry = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if entry.reserved {
                return Err(ErrorCode::BadRequest);
            }
            let Some(backing) = &entry.backing else {
                return Err(ErrorCode::Unsupported);
            };
            if !backing.is_dir {
                return Err(ErrorCode::NotDirectory);
            }
            let resolved = backing.resolved.clone();
            let backing_fid = backing.backing_fid;
            let mut path = entry.path.clone();
            path.push(name.to_string());
            state.fids.insert(
                newfid,
                Entry {
                    path: path.clone(),
                    backing: None,
                    reserved: true,
                },
            );
            (resolved, backing_fid, path)
        };

        let backing_newfid = Fid(NEXT_BACKING.fetch_add(1, Ordering::Relaxed));
        let create = resolved
            .call(Request::Create {
                fid: backing_fid,
                newfid: backing_newfid,
                name: name.to_string(),
                kind,
            })
            .await;
        let qid = match create {
            Ok(Response::Create { qid }) => qid,
            Ok(_) => {
                self.state.lock().await.fids.remove(&newfid);
                return Err(ErrorCode::Io);
            }
            Err(err) => {
                self.state.lock().await.fids.remove(&newfid);
                return Err(err);
            }
        };

        let inserted = {
            let mut state = self.state.lock().await;
            if matches!(state.fids.get(&newfid), Some(entry) if entry.reserved) {
                state.fids.insert(
                    newfid,
                    Entry {
                        path,
                        backing: Some(Backing {
                            resolved: resolved.clone(),
                            backing_fid: backing_newfid,
                            is_dir: qid.kind == FileKind::Dir,
                        }),
                        reserved: false,
                    },
                );
                true
            } else {
                false
            }
        };
        if !inserted {
            let _ = resolved
                .call(Request::Clunk {
                    fid: backing_newfid,
                })
                .await;
            return Err(ErrorCode::BadRequest);
        }
        Ok(qid)
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
            let entry = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if entry.reserved {
                return Err(ErrorCode::BadRequest);
            }
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
            let Some(entry) = state.fids.get(&fid) else {
                return Err(ErrorCode::NotFound);
            };
            if entry.reserved {
                return Err(ErrorCode::BadRequest);
            }
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

/// Open an already-walked contributor directory fid for read and return its full
/// listing, propagating the open/read error (the caller clunks the fid).
async fn read_contributor_listing(resolved: &Resolved, fid: Fid) -> Result<Vec<u8>, ErrorCode> {
    match resolved
        .call(Request::Open {
            fid,
            mode: OpenMode::Read,
        })
        .await?
    {
        Response::Open { .. } => {}
        _ => return Err(ErrorCode::Io),
    }
    read_all(resolved, fid).await
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
fn synthetic_qid(path: &[String], version: u32) -> Qid {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    "synthetic".hash(&mut h);
    path.hash(&mut h);
    Qid {
        kind: FileKind::Dir,
        version,
        path: h.finish(),
    }
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
