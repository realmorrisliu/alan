//! Kernel-enforced exclusive-write lease table shared across a namespace lineage.
//!
//! Leases bind strictly to fid `open`/`clunk` and `MountFs` transport lifetime
//! (no TTL timers or heartbeats). Keying by canonical backing target path ensures
//! an interposing `FileServer` mounted ahead of a target tree cannot grant a
//! conflicting write open while an exclusive lease is active.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use alan_ap::{ErrorCode, OpenMode};

#[derive(Default)]
struct PathLeases {
    exclusive: Option<u64>,
    shared_writers: HashSet<u64>,
}

/// Shared exclusive-write lease table for one namespace lineage.
///
/// Uses a synchronous mutex that is never held across `.await`, so RAII lease
/// release in [`LeaseGuard::drop`] is immediate and deadlock-free even on
/// transport drop or task cancellation.
pub(crate) struct ExclusiveLeaseTable {
    next_id: AtomicU64,
    entries: Mutex<HashMap<String, PathLeases>>,
}

/// RAII lease handle held by an open `MountFs` fid (or in-flight `open` call).
///
/// Dropping the guard deterministically releases the lease on `clunk`, `remove`,
/// `clunk_all`, failed/cancelled `open`, or `MountFs` drop.
pub(crate) struct LeaseGuard {
    table: Arc<ExclusiveLeaseTable>,
    path: String,
    id: u64,
    exclusive: bool,
}

impl ExclusiveLeaseTable {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            next_id: AtomicU64::new(1),
            entries: Mutex::new(HashMap::new()),
        })
    }

    /// Enforce exclusive-write semantics for `mode` at canonical `path`.
    ///
    /// - `OpenMode::Read` never conflicts with write leases and returns `Ok(None)`.
    /// - `OpenMode::WriteExclusive` requires no active exclusive or shared writer
    ///   on `path`, failing with [`ErrorCode::NoAccess`] otherwise.
    /// - `OpenMode::Write` and `OpenMode::ReadWrite` conflict with an active
    ///   exclusive lease on `path` ([`ErrorCode::NoAccess`]) while allowing
    ///   concurrent non-exclusive writers when no exclusive lease is held.
    pub(crate) fn acquire(
        self: &Arc<Self>,
        path: &str,
        mode: OpenMode,
    ) -> Result<Option<LeaseGuard>, ErrorCode> {
        if mode == OpenMode::Read {
            return Ok(None);
        }
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = entries.entry(path.to_string()).or_default();
        match mode {
            OpenMode::Read => unreachable!("handled above"),
            OpenMode::WriteExclusive => {
                if entry.exclusive.is_some() || !entry.shared_writers.is_empty() {
                    return Err(ErrorCode::NoAccess);
                }
                let id = self.next_id.fetch_add(1, Ordering::Relaxed);
                entry.exclusive = Some(id);
                Ok(Some(LeaseGuard {
                    table: Arc::clone(self),
                    path: path.to_string(),
                    id,
                    exclusive: true,
                }))
            }
            OpenMode::Write | OpenMode::ReadWrite => {
                if entry.exclusive.is_some() {
                    return Err(ErrorCode::NoAccess);
                }
                let id = self.next_id.fetch_add(1, Ordering::Relaxed);
                entry.shared_writers.insert(id);
                Ok(Some(LeaseGuard {
                    table: Arc::clone(self),
                    path: path.to_string(),
                    id,
                    exclusive: false,
                }))
            }
        }
    }

    fn release(&self, path: &str, id: u64, exclusive: bool) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let remove_entry = if let Some(entry) = entries.get_mut(path) {
            if exclusive {
                if entry.exclusive == Some(id) {
                    entry.exclusive = None;
                }
            } else {
                entry.shared_writers.remove(&id);
            }
            entry.exclusive.is_none() && entry.shared_writers.is_empty()
        } else {
            false
        };
        if remove_entry {
            entries.remove(path);
        }
    }
}

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        self.table.release(&self.path, self.id, self.exclusive);
    }
}
