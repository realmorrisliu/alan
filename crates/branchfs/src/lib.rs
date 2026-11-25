//! alan-branchfs — headless branching execution as an aP file server.
//!
//! Ring 3/4 aP file server retained in the workspace: exposes speculative
//! `alan-knowledge` checkpoint branches and opaque CoW workspace metadata as
//! plain files (`ctl`, `branches/`, `selected`, `events`). It never stores or
//! exposes raw Host filesystem paths (`ADR-0050`), and coordinates physical
//! clone creation, immediate losing-clone discard, and exit `.patch` flush
//! through the host-agnostic [`BranchWorkspacePort`].

mod state;
pub mod workspace;

use std::sync::Arc;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use alan_knowledge::ContentHash;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use state::{
    BranchFid, EventRecord, InsertForkedBranch, Node, State, is_writable, knowledge_error,
    node_identity, slice, validate_branch_id, validate_workspace_fields,
};
pub use workspace::{
    BranchWorkspaceCreateRequest, BranchWorkspaceDiscardRequest, BranchWorkspaceExitFlushOutcome,
    BranchWorkspaceExitFlushRequest, BranchWorkspacePort, BranchWorkspacePortError,
    BranchWorkspacePortErrorKind, BranchWorkspaceProvisioned, WorkspaceBackingKind,
    contains_raw_host_path, validate_no_raw_host_path, validate_opaque_workspace_ref,
};

const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

/// Canonical `/srv` handle name for the branching execution file server.
pub const SRV_HANDLE: &str = "branch";
/// Conventional mount path for branching execution surfaces.
pub const MOUNT_PATH: &str = "/mnt/branch";

/// Branch lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    /// Branch is visible and can be scored, selected, or discarded.
    Active,
    /// Branch is the explicitly selected candidate.
    Selected,
}

/// Inspectable branch metadata served at `branches/<id>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchRecord {
    pub version: u16,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub root: ContentHash,
    pub status: BranchStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_kind: Option<WorkspaceBackingKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<String>,
}

impl BranchRecord {
    fn base(id: String, root: ContentHash) -> Self {
        Self {
            version: 1,
            id,
            base: None,
            root,
            status: BranchStatus::Active,
            score: None,
            summary: None,
            workspace_ref: None,
            backing_kind: None,
            diff_summary: None,
            checks_summary: None,
        }
    }

    fn fork(
        id: String,
        base: String,
        root: ContentHash,
        workspace_ref: Option<String>,
        backing_kind: Option<WorkspaceBackingKind>,
        diff_summary: Option<String>,
        checks_summary: Option<String>,
    ) -> Self {
        Self {
            version: 1,
            id,
            base: Some(base),
            root,
            status: BranchStatus::Active,
            score: None,
            summary: None,
            workspace_ref,
            backing_kind,
            diff_summary,
            checks_summary,
        }
    }
}

/// JSON command document accepted by `ctl`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BranchCommand {
    /// Fork a new candidate from an existing visible branch.
    Fork {
        id: String,
        from: String,
        delta: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        backing_kind: Option<WorkspaceBackingKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checks_summary: Option<String>,
    },
    /// Record an explicit branch score and optional summary/evidence summaries.
    Score {
        id: String,
        score: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checks_summary: Option<String>,
    },
    /// Attach or update opaque workspace metadata on an existing visible branch.
    UpdateWorkspace {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        backing_kind: Option<WorkspaceBackingKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checks_summary: Option<String>,
    },
    /// Explicitly select a visible branch.
    Select { id: String },
    /// Hide a visible branch while immediately discarding any physical workspace
    /// and retaining lifecycle evidence in `events`.
    Discard { id: String },
}

/// Summary of one selected branch processed during [`BranchFs::flush_on_exit`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchExitFlushRecord {
    pub branch_id: String,
    pub workspace_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_artifact_ref: Option<String>,
    pub clone_purged: bool,
}

/// Branching execution file server.
pub struct BranchFs {
    state: Mutex<State>,
    workspace_port: Option<Arc<dyn BranchWorkspacePort>>,
}

impl Default for BranchFs {
    fn default() -> Self {
        Self::new()
    }
}

impl BranchFs {
    /// Create an empty branchfs without a physical workspace host port.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::new()),
            workspace_port: None,
        }
    }

    /// Create a branchfs wired to a host-agnostic [`BranchWorkspacePort`].
    pub fn with_workspace_port(port: Arc<dyn BranchWorkspacePort>) -> Self {
        Self {
            state: Mutex::new(State::new()),
            workspace_port: Some(port),
        }
    }

    /// Install a visible base branch backed by a new checkpoint.
    pub async fn install_base_branch<I, B>(
        &self,
        id: impl Into<String>,
        blocks: I,
    ) -> Result<ContentHash, ErrorCode>
    where
        I: IntoIterator<Item = B> + Send,
        I::IntoIter: Send,
        B: AsRef<[u8]> + Send,
    {
        let mut state = self.state.lock().await;
        let id = id.into();
        validate_branch_id(&id)?;
        if state.branches.contains_key(&id) {
            return Err(ErrorCode::BadRequest);
        }
        let root = state
            .store
            .checkpoint_from_bytes(blocks)
            .map_err(knowledge_error)?;
        state
            .branches
            .insert(id.clone(), BranchRecord::base(id.clone(), root.clone()));
        state.versions.bump(node_identity(&Node::BranchesDir).1);
        state
            .versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        state
            .append_event(EventRecord::Base {
                id: &id,
                root: &root,
            })
            .await?;
        Ok(root)
    }

    /// Return a visible branch record for tests and bootstrap code.
    pub async fn branch(&self, id: &str) -> Option<BranchRecord> {
        self.state.lock().await.branches.get(id).cloned()
    }

    /// Discard all active non-selected candidate branches, immediately purging any
    /// physical CoW clones through the attached [`BranchWorkspacePort`].
    pub async fn discard_unselected_candidates(&self) -> Result<Vec<String>, ErrorCode> {
        let candidates = {
            let state = self.state.lock().await;
            state
                .branches
                .iter()
                .filter(|(_, record)| {
                    record.status != BranchStatus::Selected
                        && (record.base.is_some() || record.workspace_ref.is_some())
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        };

        let mut discarded = Vec::with_capacity(candidates.len());
        for id in candidates {
            self.discard_branch_inner(id.clone()).await?;
            discarded.push(id);
        }
        Ok(discarded)
    }

    /// Exit-time cleanup: immediately purge any remaining non-selected candidate
    /// clones and persist unified `.patch` + scorecard evidence for an unapplied
    /// selected winner before removing its physical clone directory.
    pub async fn flush_on_exit(&self) -> Result<Vec<BranchExitFlushRecord>, ErrorCode> {
        self.discard_unselected_candidates().await?;

        let selected_record = {
            let state = self.state.lock().await;
            let Some(selected_id) = state.selected.as_deref() else {
                return Ok(Vec::new());
            };
            let Some(record) = state.branches.get(selected_id) else {
                return Ok(Vec::new());
            };
            record.workspace_ref.as_ref().map(|_| record.clone())
        };

        let Some(record) = selected_record else {
            return Ok(Vec::new());
        };
        let workspace_ref = record
            .workspace_ref
            .clone()
            .expect("checked workspace_ref presence");

        let outcome = if let Some(port) = &self.workspace_port {
            let outcome = port
                .flush_selected_on_exit(&BranchWorkspaceExitFlushRequest {
                    branch_id: record.id.clone(),
                    workspace_ref: workspace_ref.clone(),
                    backing_kind: record.backing_kind,
                    score: record.score,
                    summary: record.summary.clone(),
                    diff_summary: record.diff_summary.clone(),
                    checks_summary: record.checks_summary.clone(),
                })
                .await
                .map_err(|err| err.to_error_code())?;
            if let Some(artifact_ref) = outcome.patch_artifact_ref.as_deref() {
                validate_no_raw_host_path(artifact_ref)?;
            }
            outcome
        } else {
            BranchWorkspaceExitFlushOutcome {
                patch_artifact_ref: None,
                clone_purged: false,
            }
        };

        let mut state = self.state.lock().await;
        if let Some(current) = state.branches.get_mut(&record.id)
            && current.workspace_ref.as_deref() == Some(&workspace_ref)
            && outcome.clone_purged
        {
            current.workspace_ref = None;
            state
                .versions
                .bump(node_identity(&Node::Branch(record.id.clone())).1);
            if state.selected.as_deref() == Some(&record.id) {
                state.versions.bump(node_identity(&Node::Selected).1);
            }
        }
        state
            .append_event(EventRecord::ExitFlush {
                id: &record.id,
                workspace_ref: &workspace_ref,
                backing_kind: record.backing_kind,
                patch_artifact_ref: outcome.patch_artifact_ref.as_deref(),
                clone_purged: outcome.clone_purged,
            })
            .await?;

        Ok(vec![BranchExitFlushRecord {
            branch_id: record.id,
            workspace_ref,
            patch_artifact_ref: outcome.patch_artifact_ref,
            clone_purged: outcome.clone_purged,
        }])
    }

    /// Number of stored raw knowledge blocks.
    pub async fn block_count(&self) -> usize {
        self.state.lock().await.store.block_count()
    }

    /// Number of stored knowledge DAG nodes.
    pub async fn node_count(&self) -> usize {
        self.state.lock().await.store.node_count()
    }

    async fn commit_ctl(&self, bytes: Vec<u8>) -> Result<(), ErrorCode> {
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        validate_no_raw_host_path(&text)?;
        let command: BranchCommand =
            serde_json::from_str(&text).map_err(|_| ErrorCode::BadRequest)?;
        match command {
            BranchCommand::Fork {
                id,
                from,
                delta,
                workspace_ref,
                backing_kind,
                diff_summary,
                checks_summary,
            } => {
                self.fork_branch_inner(ForkBranchArgs {
                    id,
                    from,
                    delta,
                    workspace_ref,
                    backing_kind,
                    diff_summary,
                    checks_summary,
                })
                .await
            }
            BranchCommand::Score {
                id,
                score,
                summary,
                diff_summary,
                checks_summary,
            } => {
                self.state
                    .lock()
                    .await
                    .score_branch(id, score, summary, diff_summary, checks_summary)
                    .await
            }
            BranchCommand::UpdateWorkspace {
                id,
                workspace_ref,
                backing_kind,
                diff_summary,
                checks_summary,
            } => {
                self.state
                    .lock()
                    .await
                    .update_workspace(
                        id,
                        workspace_ref,
                        backing_kind,
                        diff_summary,
                        checks_summary,
                    )
                    .await
            }
            BranchCommand::Select { id } => self.state.lock().await.select_branch(id).await,
            BranchCommand::Discard { id } => self.discard_branch_inner(id).await,
        }
    }

    async fn fork_branch_inner(&self, args: ForkBranchArgs) -> Result<(), ErrorCode> {
        validate_branch_id(&args.id)?;
        validate_branch_id(&args.from)?;
        validate_no_raw_host_path(&args.delta)?;
        validate_workspace_fields(
            args.workspace_ref.as_deref(),
            args.diff_summary.as_deref(),
            args.checks_summary.as_deref(),
        )?;

        let from_workspace_ref = {
            let state = self.state.lock().await;
            if state.branches.contains_key(&args.id) {
                return Err(ErrorCode::BadRequest);
            }
            let base = state.branches.get(&args.from).ok_or(ErrorCode::NotFound)?;
            base.workspace_ref.clone()
        };

        let mut created_workspace_ref = None;
        let (workspace_ref, backing_kind, diff_summary, checks_summary) =
            if args.workspace_ref.is_none()
                && let Some(port) = &self.workspace_port
            {
                let provisioned = port
                    .create_workspace(&BranchWorkspaceCreateRequest {
                        branch_id: args.id.clone(),
                        from_branch_id: args.from.clone(),
                        from_workspace_ref,
                    })
                    .await
                    .map_err(|err| err.to_error_code())?;
                if validate_workspace_fields(
                    Some(&provisioned.workspace_ref),
                    provisioned.diff_summary.as_deref(),
                    provisioned.checks_summary.as_deref(),
                )
                .is_err()
                {
                    let _ = port
                        .discard_workspace(&BranchWorkspaceDiscardRequest {
                            branch_id: args.id.clone(),
                            workspace_ref: provisioned.workspace_ref,
                            backing_kind: Some(provisioned.backing_kind),
                        })
                        .await;
                    return Err(ErrorCode::BadRequest);
                }
                created_workspace_ref =
                    Some((provisioned.workspace_ref.clone(), provisioned.backing_kind));
                (
                    Some(provisioned.workspace_ref),
                    Some(args.backing_kind.unwrap_or(provisioned.backing_kind)),
                    args.diff_summary.or(provisioned.diff_summary),
                    args.checks_summary.or(provisioned.checks_summary),
                )
            } else {
                (
                    args.workspace_ref,
                    args.backing_kind,
                    args.diff_summary,
                    args.checks_summary,
                )
            };

        let insert_result = {
            let mut state = self.state.lock().await;
            state
                .insert_forked_branch(InsertForkedBranch {
                    id: args.id.clone(),
                    from: args.from,
                    delta: args.delta,
                    workspace_ref,
                    backing_kind,
                    diff_summary,
                    checks_summary,
                })
                .await
        };

        if let Err(err) = insert_result {
            if let (Some(port), Some((ws_ref, kind))) =
                (&self.workspace_port, created_workspace_ref)
            {
                let _ = port
                    .discard_workspace(&BranchWorkspaceDiscardRequest {
                        branch_id: args.id,
                        workspace_ref: ws_ref,
                        backing_kind: Some(kind),
                    })
                    .await;
            }
            return Err(err);
        }
        Ok(())
    }

    async fn discard_branch_inner(&self, id: String) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        let existing = {
            let state = self.state.lock().await;
            state
                .branches
                .get(&id)
                .cloned()
                .ok_or(ErrorCode::NotFound)?
        };

        if let (Some(port), Some(ws_ref)) = (&self.workspace_port, existing.workspace_ref.as_ref())
        {
            port.discard_workspace(&BranchWorkspaceDiscardRequest {
                branch_id: id.clone(),
                workspace_ref: ws_ref.clone(),
                backing_kind: existing.backing_kind,
            })
            .await
            .map_err(|err| err.to_error_code())?;
        }

        let mut state = self.state.lock().await;
        state.discard_branch(id).await
    }
}

struct ForkBranchArgs {
    id: String,
    from: String,
    delta: String,
    workspace_ref: Option<String>,
    backing_kind: Option<WorkspaceBackingKind>,
    diff_summary: Option<String>,
    checks_summary: Option<String>,
}

#[async_trait]
impl FileServer for BranchFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = state.node_of(fid)?;
        for name in names {
            node = state.child(&node, name)?;
        }
        let qid = state.qid(&node);
        state.fids.insert(newfid, BranchFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        if fid != Fid::ROOT && state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) && !is_writable(&node) {
            return Err(ErrorCode::NoAccess);
        }
        if fid != Fid::ROOT {
            let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            f.mode = Some(mode);
        }
        Ok(state.qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (node, stream) = {
            let state = self.state.lock().await;
            if fid != Fid::ROOT {
                let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
                if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
            }
            let node = state.node_of(fid)?;
            let stream = state.stream_for(&node);
            (node, stream)
        };
        if let Some(stream) = stream {
            return Ok(stream.read(offset, count).await);
        }
        let state = self.state.lock().await;
        Ok(slice(state.computed_bytes(&node)?, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        if !is_writable(&node) {
            return Err(ErrorCode::Unsupported);
        }
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        if end > MAX_DOC_BYTES {
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
            Node::Events => {
                state
                    .stream_for(&node)
                    .ok_or(ErrorCode::NotFound)?
                    .len()
                    .await
            }
            other => state.computed_bytes(other)?.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid: state.qid(&node),
            length,
            executable: false,
            writable: is_writable(&node),
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
            return Ok(());
        }
        let f = {
            let mut state = self.state.lock().await;
            state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?
        };
        if !f.wrote {
            return Ok(());
        }
        match f.node {
            Node::Ctl => self.commit_ctl(f.write_buf).await,
            _ => Ok(()),
        }
    }
}
