//! Mount-time actor-scoped write projections over `/agent` (`AgentActorView`).
//!
//! Write authority over `/agent` carries an actor dimension enforced at mount
//! time rather than adding actor identity fields to `aP` wire frames (ADR-0063).

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use async_trait::async_trait;

use crate::{AgentFs, AgentRootFs};

// Distinct fid namespace above direct `MountFs` (`1..`) and `AgentRootFs`
// internal backing (`1_000_000..`) / proc (`2_000_000..`) ranges so multiple
// actor views sharing one `AgentRootFs` or `AgentFs` never collide on fids.
static NEXT_VIEW_FID: AtomicU64 = AtomicU64::new(10_000_000);

/// Mount-time actor identity projection controlling write authority over `/agent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentActorView {
    /// Agent Engine executing process `self_pid`: engine-owned writes are
    /// permitted only under `<self_pid>` (and `root` when resolved to
    /// `<self_pid>`); client-surface writes remain permitted under other
    /// mounted agent pids for child orchestration.
    Engine { self_pid: String },
    /// External/local client (TUI, `namespace.ap.sock`, `EmbeddedAlan`):
    /// write intent is granted only on `io/input`, `machine/ctl`, and
    /// `requests/<id>/response` (plus `/agent/history` continuity controls).
    Client,
    /// Child Agent Process executing `self_pid`: mirrors `Engine { self_pid }`
    /// for own `<self_pid>` while keeping parent and sibling `/agent/<other_pid>`
    /// trees read-only.
    Child { self_pid: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedTarget {
    Root,
    History { names: Vec<String> },
    AgentRoot { pid: String },
    AgentChildren { pid: String },
    AgentSurface { pid: String, names: Vec<String> },
    ProcSurface { pid: String, names: Vec<String> },
}

impl AgentActorView {
    pub(crate) fn allows_write(&self, target: &ResolvedTarget) -> bool {
        match (self, target) {
            (
                Self::Client,
                ResolvedTarget::AgentSurface { names, .. }
                | ResolvedTarget::ProcSurface { names, .. },
            ) => is_client_writable_agent_path(names),
            (Self::Client, ResolvedTarget::History { names }) => {
                is_client_writable_history_path(names)
            }
            (Self::Client, _) => false,
            (
                Self::Engine { self_pid },
                ResolvedTarget::AgentSurface { pid, names }
                | ResolvedTarget::ProcSurface { pid, names },
            ) => pid == self_pid || is_client_writable_agent_path(names),
            (Self::Engine { .. }, _) => false,
            (
                Self::Child { self_pid },
                ResolvedTarget::AgentSurface { pid, .. } | ResolvedTarget::ProcSurface { pid, .. },
            ) => pid == self_pid,
            (Self::Child { .. }, _) => false,
        }
    }

    pub(crate) fn allows_create(&self, target: &ResolvedTarget) -> bool {
        match (self, target) {
            (
                Self::Engine { self_pid } | Self::Child { self_pid },
                ResolvedTarget::AgentRoot { pid }
                | ResolvedTarget::AgentSurface { pid, .. }
                | ResolvedTarget::ProcSurface { pid, .. },
            ) => pid == self_pid,
            _ => false,
        }
    }
}

pub(crate) fn is_write_intent(mode: OpenMode) -> bool {
    matches!(
        mode,
        OpenMode::Write | OpenMode::WriteExclusive | OpenMode::ReadWrite
    )
}

fn is_client_writable_agent_path(names: &[String]) -> bool {
    match names {
        [dir, file] if dir == "io" && file == "input" => true,
        [dir, file] if dir == "machine" && file == "ctl" => true,
        [dir, id, field]
            if dir == "requests"
                && !id.is_empty()
                && id != "clone"
                && id != "events"
                && field == "response" =>
        {
            true
        }
        _ => false,
    }
}

fn is_client_writable_history_path(names: &[String]) -> bool {
    matches!(names, [file] if file == "ctl" || file == "summary_ctl")
}

#[derive(Clone)]
struct ViewFid {
    inner_fid: Fid,
    target: ResolvedTarget,
}

/// Mount-time actor-scoped `/agent` projection wrapping an [`AgentRootFs`].
pub struct AgentRootView {
    root: Arc<AgentRootFs>,
    view: AgentActorView,
    fids: Mutex<HashMap<Fid, ViewFid>>,
}

impl AgentRootView {
    pub fn new(root: Arc<AgentRootFs>, view: AgentActorView) -> Self {
        Self {
            root,
            view,
            fids: Mutex::new(HashMap::new()),
        }
    }

    pub fn view(&self) -> &AgentActorView {
        &self.view
    }

    fn resolve_fid(&self, fid: Fid) -> Result<ViewFid, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(ViewFid {
                inner_fid: Fid::ROOT,
                target: ResolvedTarget::Root,
            });
        }
        self.fids
            .lock()
            .expect("agent root view fids mutex poisoned")
            .get(&fid)
            .cloned()
            .ok_or(ErrorCode::NotFound)
    }
}

impl AgentRootFs {
    /// Wrap this `/agent` root in an actor-scoped mount projection.
    pub fn actor_view(self: &Arc<Self>, view: AgentActorView) -> Arc<AgentRootView> {
        Arc::new(AgentRootView::new(Arc::clone(self), view))
    }
}

#[async_trait]
impl FileServer for AgentRootView {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let base_inner_fid = {
            let fids = self
                .fids
                .lock()
                .expect("agent root view fids mutex poisoned");
            if newfid == Fid::ROOT || fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            if fid == Fid::ROOT {
                Fid::ROOT
            } else {
                fids.get(&fid).ok_or(ErrorCode::NotFound)?.inner_fid
            }
        };
        let inner_newfid = Fid(NEXT_VIEW_FID.fetch_add(1, Ordering::Relaxed));
        let qid = self.root.walk(base_inner_fid, inner_newfid, names).await?;
        let target = match self.root.target_of(inner_newfid).await {
            Ok(target) => target,
            Err(err) => {
                let _ = self.root.clunk(inner_newfid).await;
                return Err(err);
            }
        };
        let inserted = {
            let mut fids = self
                .fids
                .lock()
                .expect("agent root view fids mutex poisoned");
            if let std::collections::hash_map::Entry::Vacant(entry) = fids.entry(newfid) {
                entry.insert(ViewFid {
                    inner_fid: inner_newfid,
                    target,
                });
                true
            } else {
                false
            }
        };
        if !inserted {
            let _ = self.root.clunk(inner_newfid).await;
            return Err(ErrorCode::BadRequest);
        }
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        if is_write_intent(mode) && !self.view.allows_write(&entry.target) {
            return Err(ErrorCode::NoAccess);
        }
        self.root.open(entry.inner_fid, mode).await
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        self.root.read(entry.inner_fid, offset, count).await
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        if !self.view.allows_write(&entry.target) {
            return Err(ErrorCode::NoAccess);
        }
        self.root.write(entry.inner_fid, offset, data).await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        let mut stat = self.root.stat(entry.inner_fid).await?;
        if !self.view.allows_write(&entry.target) {
            stat.writable = false;
        }
        Ok(stat)
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let base = {
            let fids = self
                .fids
                .lock()
                .expect("agent root view fids mutex poisoned");
            if newfid == Fid::ROOT || fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            if fid == Fid::ROOT {
                ViewFid {
                    inner_fid: Fid::ROOT,
                    target: ResolvedTarget::Root,
                }
            } else {
                fids.get(&fid).cloned().ok_or(ErrorCode::NotFound)?
            }
        };
        if !self.view.allows_create(&base.target) {
            return Err(ErrorCode::NoAccess);
        }
        let inner_newfid = Fid(NEXT_VIEW_FID.fetch_add(1, Ordering::Relaxed));
        let qid = self
            .root
            .create(base.inner_fid, inner_newfid, name, kind)
            .await?;
        let target = match self.root.target_of(inner_newfid).await {
            Ok(target) => target,
            Err(err) => {
                if self.root.remove(inner_newfid).await.is_err() {
                    let _ = self.root.clunk(inner_newfid).await;
                }
                return Err(err);
            }
        };
        let inserted = {
            let mut fids = self
                .fids
                .lock()
                .expect("agent root view fids mutex poisoned");
            if let std::collections::hash_map::Entry::Vacant(entry) = fids.entry(newfid) {
                entry.insert(ViewFid {
                    inner_fid: inner_newfid,
                    target,
                });
                true
            } else {
                false
            }
        };
        if !inserted {
            if self.root.remove(inner_newfid).await.is_err() {
                let _ = self.root.clunk(inner_newfid).await;
            }
            return Err(ErrorCode::BadRequest);
        }
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        if !self.view.allows_write(&entry.target) {
            return Err(ErrorCode::NoAccess);
        }
        self.root.remove(entry.inner_fid).await?;
        self.fids
            .lock()
            .expect("agent root view fids mutex poisoned")
            .remove(&fid);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let entry = self
            .fids
            .lock()
            .expect("agent root view fids mutex poisoned")
            .remove(&fid)
            .ok_or(ErrorCode::NotFound)?;
        self.root.clunk(entry.inner_fid).await
    }
}

#[derive(Clone)]
struct AgentFsViewFid {
    inner_fid: Fid,
    names: Vec<String>,
}

/// Mount-time actor-scoped projection wrapping a single [`AgentFs`] tree bound
/// to `pid`.
pub struct AgentFsView {
    fs: Arc<AgentFs>,
    pid: String,
    view: AgentActorView,
    fids: Mutex<HashMap<Fid, AgentFsViewFid>>,
}

impl AgentFsView {
    pub fn new(fs: Arc<AgentFs>, pid: impl Into<String>, view: AgentActorView) -> Self {
        Self {
            fs,
            pid: pid.into(),
            view,
            fids: Mutex::new(HashMap::new()),
        }
    }

    pub fn view(&self) -> &AgentActorView {
        &self.view
    }

    fn resolve_fid(&self, fid: Fid) -> Result<AgentFsViewFid, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(AgentFsViewFid {
                inner_fid: Fid::ROOT,
                names: Vec::new(),
            });
        }
        self.fids
            .lock()
            .expect("agent fs view fids mutex poisoned")
            .get(&fid)
            .cloned()
            .ok_or(ErrorCode::NotFound)
    }

    fn target_for(&self, names: &[String]) -> ResolvedTarget {
        if names.is_empty() {
            ResolvedTarget::AgentRoot {
                pid: self.pid.clone(),
            }
        } else {
            ResolvedTarget::AgentSurface {
                pid: self.pid.clone(),
                names: names.to_vec(),
            }
        }
    }
}

impl AgentFs {
    /// Wrap this per-process [`AgentFs`] tree in an actor-scoped mount projection.
    pub fn actor_view(
        self: &Arc<Self>,
        pid: impl Into<String>,
        view: AgentActorView,
    ) -> Arc<AgentFsView> {
        Arc::new(AgentFsView::new(Arc::clone(self), pid, view))
    }
}

#[async_trait]
impl FileServer for AgentFsView {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let base = {
            let fids = self.fids.lock().expect("agent fs view fids mutex poisoned");
            if newfid == Fid::ROOT || fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            if fid == Fid::ROOT {
                AgentFsViewFid {
                    inner_fid: Fid::ROOT,
                    names: Vec::new(),
                }
            } else {
                fids.get(&fid).cloned().ok_or(ErrorCode::NotFound)?
            }
        };
        let inner_newfid = Fid(NEXT_VIEW_FID.fetch_add(1, Ordering::Relaxed));
        let qid = self.fs.walk(base.inner_fid, inner_newfid, names).await?;
        let mut full_names = base.names;
        full_names.extend_from_slice(names);
        let inserted = {
            let mut fids = self.fids.lock().expect("agent fs view fids mutex poisoned");
            if let std::collections::hash_map::Entry::Vacant(entry) = fids.entry(newfid) {
                entry.insert(AgentFsViewFid {
                    inner_fid: inner_newfid,
                    names: full_names,
                });
                true
            } else {
                false
            }
        };
        if !inserted {
            let _ = self.fs.clunk(inner_newfid).await;
            return Err(ErrorCode::BadRequest);
        }
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        let target = self.target_for(&entry.names);
        if is_write_intent(mode) && !self.view.allows_write(&target) {
            return Err(ErrorCode::NoAccess);
        }
        self.fs.open(entry.inner_fid, mode).await
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        self.fs.read(entry.inner_fid, offset, count).await
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        let target = self.target_for(&entry.names);
        if !self.view.allows_write(&target) {
            return Err(ErrorCode::NoAccess);
        }
        self.fs.write(entry.inner_fid, offset, data).await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        let target = self.target_for(&entry.names);
        let mut stat = self.fs.stat(entry.inner_fid).await?;
        if !self.view.allows_write(&target) {
            stat.writable = false;
        }
        Ok(stat)
    }

    async fn create(
        &self,
        fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        let target = self.target_for(&entry.names);
        if !self.view.allows_create(&target) {
            return Err(ErrorCode::NoAccess);
        }
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let entry = self.resolve_fid(fid)?;
        let target = self.target_for(&entry.names);
        if !self.view.allows_write(&target) {
            return Err(ErrorCode::NoAccess);
        }
        self.fs.remove(entry.inner_fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let entry = self
            .fids
            .lock()
            .expect("agent fs view fids mutex poisoned")
            .remove(&fid)
            .ok_or(ErrorCode::NotFound)?;
        self.fs.clunk(entry.inner_fid).await
    }
}
