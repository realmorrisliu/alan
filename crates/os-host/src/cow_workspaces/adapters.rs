//! Host adapters binding [`CowWorkspaceManager`] to Ring 3/4 ports
//! ([`alan_branchfs::BranchWorkspacePort`] and [`alan_agent_engine::ArenaWorkspacePort`])
//! while keeping raw Host filesystem paths strictly inside `alan-os-host` (`ADR-0050`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use alan_agent_engine::{
    ArenaContenderWorkspace, ArenaExitFlushRegistry, ArenaWinnerFlushOutcome,
    ArenaWinnerFlushRequest, ArenaWorkspaceBackingKind, ArenaWorkspaceInspection,
    ArenaWorkspacePort, BranchFsApClient, DEFAULT_ARENA_CONTENDER_MOUNT_TARGET,
    ExecutableArenaCoordinator, IsolatedWorkspaceAllocation, IsolatedWorkspaceBacking,
    IsolatedWorkspaceRequest, SpawnHostMount, SpawnMountAccess, WorkspaceIsolationPort,
    WorkspaceMergeHandoff, WorkspaceSyncConflict, WorkspaceSyncStatus,
};
use alan_ap::InProcessTransport;
use alan_branchfs::{
    BranchFs, BranchWorkspaceCreateRequest, BranchWorkspaceDiscardRequest,
    BranchWorkspaceExitFlushOutcome, BranchWorkspaceExitFlushRequest, BranchWorkspacePort,
    BranchWorkspacePortError, BranchWorkspaceProvisioned, WorkspaceBackingKind,
    validate_no_raw_host_path,
};
use alan_kernel::Pid;
use alan_service_manager::{HostMountAccess, HostMountService};
use anyhow::Result;
use async_trait::async_trait;

use crate::host_mounts::native_export_host_path;

use super::{
    CloneFallbackPolicy, CowWorkspaceError, CowWorkspaceManager, CreateWorkspaceOptions,
    DiffSummary, ExitFlushReport, FileChangeKind, IntentProvenance, WorkspaceBacking, WorkspaceRef,
};

/// Host-owned adapter implementing [`BranchWorkspacePort`] atop [`CowWorkspaceManager`].
///
/// Raw Host filesystem paths never cross into `alan-branchfs`.
#[derive(Clone, Debug)]
pub struct CowBranchWorkspaceAdapter {
    manager: CowWorkspaceManager,
    host_mount: Arc<OnceLock<Arc<HostMountService>>>,
    default_source_grant: Arc<Mutex<Option<String>>>,
    default_host_root: Arc<Mutex<Option<PathBuf>>>,
    fallback_policy: CloneFallbackPolicy,
}

impl CowBranchWorkspaceAdapter {
    /// Construct a `/mnt/branch` workspace adapter backed by `manager`.
    pub fn new(manager: CowWorkspaceManager) -> Self {
        Self {
            manager,
            host_mount: Arc::new(OnceLock::new()),
            default_source_grant: Arc::new(Mutex::new(None)),
            default_host_root: Arc::new(Mutex::new(None)),
            fallback_policy: CloneFallbackPolicy::Auto,
        }
    }

    /// Attach an initialized [`HostMountService`] for grant lookup and revocation on discard.
    pub fn with_host_mount(self, host_mount: Arc<HostMountService>) -> Self {
        let _ = self.host_mount.set(host_mount);
        self
    }

    /// Attach a default Host project root used when `ctl` forks without an explicit parent workspace.
    pub fn with_default_host_root(self, host_root: impl Into<PathBuf>) -> Self {
        *self.default_host_root.lock().unwrap() = Some(host_root.into());
        self
    }

    /// Configure a default `HostMountService` grant ID to resolve `host_root` on `fork`.
    pub fn with_default_source_grant(self, grant_id: impl Into<String>) -> Self {
        *self.default_source_grant.lock().unwrap() = Some(grant_id.into());
        self
    }

    /// Override the clone fallback policy (defaults to [`CloneFallbackPolicy::Auto`]).
    pub fn with_fallback_policy(mut self, policy: CloneFallbackPolicy) -> Self {
        self.fallback_policy = policy;
        self
    }

    /// Bind the live [`HostMountService`] after `ServiceManager::boot` completes.
    pub fn attach_host_mount(&self, host_mount: Arc<HostMountService>) {
        let _ = self.host_mount.set(host_mount);
    }

    /// Update the default `source_grant` ID used for `/mnt/branch` forks from `base`.
    pub fn set_default_source_grant(&self, grant_id: Option<String>) {
        *self.default_source_grant.lock().unwrap() = grant_id;
    }

    /// Update the default Host project root used for `/mnt/branch` forks from `base`.
    pub fn set_default_host_root(&self, host_root: Option<PathBuf>) {
        *self.default_host_root.lock().unwrap() = host_root;
    }

    fn resolve_source_host_root(
        &self,
        from_workspace_ref: Option<&str>,
    ) -> std::result::Result<PathBuf, BranchWorkspacePortError> {
        if let Some(raw_ref) = from_workspace_ref {
            let ws_ref = WorkspaceRef::parse(raw_ref)
                .map_err(|err| BranchWorkspacePortError::rejected(err.to_string()))?;
            return self.manager.workspace_host_root(&ws_ref).ok_or_else(|| {
                BranchWorkspacePortError::not_found(format!(
                    "parent CoW workspace `{raw_ref}` is not active"
                ))
            });
        }

        if let Some(grant_id) = self.default_source_grant.lock().unwrap().clone() {
            let host_mount = self.host_mount.get().ok_or_else(|| {
                BranchWorkspacePortError::rejected(
                    "HostMountService is not attached for resolving default source_grant",
                )
            })?;
            return resolve_grant_host_root(host_mount, &grant_id)
                .map_err(BranchWorkspacePortError::rejected);
        }

        self.default_host_root.lock().unwrap().clone().ok_or_else(|| {
            BranchWorkspacePortError::rejected(
                "no source Host root or source_grant configured for /mnt/branch workspace creation",
            )
        })
    }
}

#[async_trait]
impl BranchWorkspacePort for CowBranchWorkspaceAdapter {
    async fn create_workspace(
        &self,
        request: &BranchWorkspaceCreateRequest,
    ) -> std::result::Result<BranchWorkspaceProvisioned, BranchWorkspacePortError> {
        let host_root = self.resolve_source_host_root(request.from_workspace_ref.as_deref())?;
        let manager = self.manager.clone();
        let fallback_policy = self.fallback_policy;
        let summary = tokio::task::spawn_blocking(move || {
            manager.create_workspace(
                &host_root,
                CreateWorkspaceOptions {
                    fallback_policy,
                    ..Default::default()
                },
            )
        })
        .await
        .map_err(|err| BranchWorkspacePortError::io(format!("join create_workspace: {err}")))?
        .map_err(map_cow_err_to_branch)?;

        Ok(BranchWorkspaceProvisioned {
            workspace_ref: summary.workspace_ref.as_str().to_string(),
            backing_kind: to_branch_backing_kind(summary.backing_kind),
            diff_summary: None,
            checks_summary: None,
        })
    }

    async fn discard_workspace(
        &self,
        request: &BranchWorkspaceDiscardRequest,
    ) -> std::result::Result<(), BranchWorkspacePortError> {
        let ws_ref = WorkspaceRef::parse(&request.workspace_ref)
            .map_err(|err| BranchWorkspacePortError::rejected(err.to_string()))?;
        let manager = self.manager.clone();
        let host_mount = self.host_mount.get().cloned();
        tokio::task::spawn_blocking(move || {
            manager.discard_workspace(&ws_ref, host_mount.as_deref(), "branchfs-discard")
        })
        .await
        .map_err(|err| BranchWorkspacePortError::io(format!("join discard_workspace: {err}")))?
        .map_err(map_cow_err_to_branch)
    }

    async fn flush_selected_on_exit(
        &self,
        request: &BranchWorkspaceExitFlushRequest,
    ) -> std::result::Result<BranchWorkspaceExitFlushOutcome, BranchWorkspacePortError> {
        let ws_ref = WorkspaceRef::parse(&request.workspace_ref)
            .map_err(|err| BranchWorkspacePortError::rejected(err.to_string()))?;
        let provenance = build_single_line_provenance(
            Some(format!("branch:{}", request.branch_id).as_str()),
            Some(format!("branchfs:{}", request.branch_id).as_str()),
            request.summary.as_deref(),
            "Selected /mnt/branch candidate flushed on exit",
        )
        .map_err(BranchWorkspacePortError::rejected)?;
        let scorecard = format_scorecard_summary(
            request.score,
            request.summary.as_deref(),
            request.diff_summary.as_deref(),
            request.checks_summary.as_deref(),
        );
        let manager = self.manager.clone();
        let host_mount = self.host_mount.get().cloned();
        let evidence = tokio::task::spawn_blocking(move || {
            manager.flush_unapplied_to_evidence(
                &ws_ref,
                host_mount.as_deref(),
                &provenance,
                scorecard.as_deref(),
                "branchfs-exit",
            )
        })
        .await
        .map_err(|err| BranchWorkspacePortError::io(format!("join exit flush: {err}")))?;

        match evidence {
            Ok(item) => Ok(BranchWorkspaceExitFlushOutcome {
                patch_artifact_ref: item.map(|entry| entry.patch_file_name),
                clone_purged: true,
            }),
            Err(CowWorkspaceError::UnknownWorkspace(_)) => Ok(BranchWorkspaceExitFlushOutcome {
                patch_artifact_ref: self
                    .manager
                    .existing_flushed_patch_name(&request.workspace_ref),
                clone_purged: true,
            }),
            Err(err) => Err(map_cow_err_to_branch(err)),
        }
    }
}

/// Host-owned adapter implementing [`ArenaWorkspacePort`] atop [`CowWorkspaceManager`]
/// and [`HostMountService`].
#[derive(Clone, Debug)]
pub struct CowArenaWorkspaceAdapter {
    manager: CowWorkspaceManager,
    host_mount: Arc<HostMountService>,
    parent_pid: Arc<AtomicU64>,
    fallback_policy: CloneFallbackPolicy,
}

impl CowArenaWorkspaceAdapter {
    /// Construct an executable-arena workspace adapter bound to a fixed `parent_pid`.
    pub fn new(
        manager: CowWorkspaceManager,
        host_mount: Arc<HostMountService>,
        parent_pid: Pid,
    ) -> Self {
        Self::with_dynamic_parent_pid(manager, host_mount, Arc::new(AtomicU64::new(parent_pid.0)))
    }

    /// Construct an executable-arena workspace adapter tracking a dynamic Root `parent_pid`.
    pub fn with_dynamic_parent_pid(
        manager: CowWorkspaceManager,
        host_mount: Arc<HostMountService>,
        parent_pid: Arc<AtomicU64>,
    ) -> Self {
        Self {
            manager,
            host_mount,
            parent_pid,
            fallback_policy: CloneFallbackPolicy::Auto,
        }
    }

    /// Override the clone fallback policy (defaults to [`CloneFallbackPolicy::Auto`]).
    pub fn with_fallback_policy(mut self, policy: CloneFallbackPolicy) -> Self {
        self.fallback_policy = policy;
        self
    }

    fn current_parent_pid(&self) -> Pid {
        Pid(self.parent_pid.load(Ordering::Acquire))
    }
}

#[async_trait]
impl ArenaWorkspacePort for CowArenaWorkspaceAdapter {
    async fn create_contender_workspace(
        &self,
        request: &alan_agent_engine::ArenaWorkspaceCreateRequest,
    ) -> std::result::Result<ArenaContenderWorkspace, String> {
        let parent_pid = self.current_parent_pid();
        if !self
            .host_mount
            .process_holds_grant(parent_pid, &request.source_grant)
        {
            return Err(format!(
                "parent Process {} does not hold active source_grant `{}`",
                parent_pid.0, request.source_grant
            ));
        }
        let host_root = resolve_grant_host_root(&self.host_mount, &request.source_grant)?;

        let manager = self.manager.clone();
        let fallback_policy = self.fallback_policy;
        let summary = tokio::task::spawn_blocking(move || {
            manager.create_workspace(
                &host_root,
                CreateWorkspaceOptions {
                    fallback_policy,
                    ..Default::default()
                },
            )
        })
        .await
        .map_err(|err| format!("join create_workspace: {err}"))?
        .map_err(|err| err.to_string())?;

        let parent_namespace_path = format!("/mnt/cow-{}", summary.workspace_ref.as_str());
        let grant = match self.manager.project_into_process(
            &summary.workspace_ref,
            &self.host_mount,
            parent_pid,
            &parent_namespace_path,
            HostMountAccess::ReadWrite,
        ) {
            Ok(grant) => grant,
            Err(err) => {
                let manager = self.manager.clone();
                let host_mount = self.host_mount.clone();
                let ws_ref = summary.workspace_ref.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    manager.discard_workspace(&ws_ref, Some(&host_mount), "arena-rollback")
                })
                .await;
                return Err(format!(
                    "failed to project CoW workspace into parent Process: {err}"
                ));
            }
        };

        let verifier_target =
            PathBuf::from(format!("/mnt/arena-candidates/{}", request.candidate_id));
        Ok(ArenaContenderWorkspace {
            workspace_ref: summary.workspace_ref.as_str().to_string(),
            backing_kind: to_arena_backing_kind(summary.backing_kind),
            contender_mount: SpawnHostMount {
                grant: grant.id.clone(),
                target: PathBuf::from(DEFAULT_ARENA_CONTENDER_MOUNT_TARGET),
                access: SpawnMountAccess::ReadWrite,
            },
            verifier_read_only_mount: Some(SpawnHostMount {
                grant: grant.id,
                target: verifier_target,
                access: SpawnMountAccess::ReadOnly,
            }),
        })
    }

    async fn inspect_workspace(
        &self,
        workspace_ref: &str,
    ) -> std::result::Result<ArenaWorkspaceInspection, String> {
        let ws_ref = WorkspaceRef::parse(workspace_ref).map_err(|err| err.to_string())?;
        let manager = self.manager.clone();
        let diff = tokio::task::spawn_blocking(move || manager.diff_summary(&ws_ref))
            .await
            .map_err(|err| format!("join diff_summary: {err}"))?
            .map_err(|err| err.to_string())?;
        let diff_summary = format_opaque_diff_summary(&diff);
        validate_no_raw_host_path(&diff_summary)
            .map_err(|_| "diff summary unexpectedly contained a raw Host path".to_string())?;
        Ok(ArenaWorkspaceInspection {
            diff_summary,
            checks_summary: None,
            has_changes: !diff.is_empty(),
        })
    }

    async fn discard_workspace(&self, workspace_ref: &str) -> std::result::Result<(), String> {
        let ws_ref = WorkspaceRef::parse(workspace_ref).map_err(|err| err.to_string())?;
        let manager = self.manager.clone();
        let host_mount = self.host_mount.clone();
        match tokio::task::spawn_blocking(move || {
            manager.discard_workspace(&ws_ref, Some(&host_mount), "arena-discard")
        })
        .await
        .map_err(|err| format!("join discard_workspace: {err}"))?
        {
            Ok(()) | Err(CowWorkspaceError::UnknownWorkspace(_)) => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    }

    async fn flush_winner_patch_on_exit(
        &self,
        request: &ArenaWinnerFlushRequest,
    ) -> std::result::Result<ArenaWinnerFlushOutcome, String> {
        let ws_ref = WorkspaceRef::parse(&request.workspace_ref).map_err(|err| err.to_string())?;
        let fallback_rollout = format!("arena-{}", request.candidate_id);
        let provenance = build_single_line_provenance(
            request.workstream.as_deref().or(Some("alan-arena")),
            request
                .rollout_ref
                .as_deref()
                .or(Some(fallback_rollout.as_str())),
            request.why.as_deref(),
            "Unapplied executable alan-arena winner",
        )?;
        let scorecard = format_scorecard_summary(
            request.score,
            request.verifier_summary.as_deref(),
            request.diff_summary.as_deref(),
            request.checks_summary.as_deref(),
        );
        let manager = self.manager.clone();
        let host_mount = self.host_mount.clone();
        let evidence = tokio::task::spawn_blocking(move || {
            manager.flush_unapplied_to_evidence(
                &ws_ref,
                Some(&host_mount),
                &provenance,
                scorecard.as_deref(),
                "arena-exit",
            )
        })
        .await
        .map_err(|err| format!("join flush_winner_patch_on_exit: {err}"))?
        .map_err(|err| err.to_string())?;

        Ok(ArenaWinnerFlushOutcome {
            patch_artifact_ref: evidence.as_ref().map(|item| item.patch_file_name.clone()),
            scorecard_artifact_ref: evidence.as_ref().map(|item| item.summary_file_name.clone()),
            clone_purged: true,
        })
    }
}

/// Host-owned adapter implementing M8.3 [`WorkspaceIsolationPort`] atop
/// [`CowWorkspaceManager`] and [`HostMountService`].
#[derive(Clone, Debug)]
pub struct CowWorkspaceIsolationAdapter {
    manager: CowWorkspaceManager,
    host_mount: Arc<OnceLock<Arc<HostMountService>>>,
    parent_pid: Arc<OnceLock<Arc<AtomicU64>>>,
    source_grants: Arc<Mutex<HashMap<String, String>>>,
    fallback_policy: CloneFallbackPolicy,
}

impl CowWorkspaceIsolationAdapter {
    /// Construct a deferred-binding [`WorkspaceIsolationPort`] adapter for Host boot.
    pub fn new(manager: CowWorkspaceManager) -> Self {
        Self {
            manager,
            host_mount: Arc::new(OnceLock::new()),
            parent_pid: Arc::new(OnceLock::new()),
            source_grants: Arc::new(Mutex::new(HashMap::new())),
            fallback_policy: CloneFallbackPolicy::Auto,
        }
    }

    /// Construct an immediately bound adapter for tests with explicit `host_mount` and `parent_pid`.
    pub fn with_host_mount_and_pid(
        manager: CowWorkspaceManager,
        host_mount: Arc<HostMountService>,
        parent_pid: Pid,
    ) -> Self {
        let adapter = Self::new(manager);
        adapter.attach(host_mount, Arc::new(AtomicU64::new(parent_pid.0)));
        adapter
    }

    /// Attach the live [`HostMountService`] and Root PID atomic once `ServiceManager::boot` finishes.
    pub fn attach(&self, host_mount: Arc<HostMountService>, root_pid: Arc<AtomicU64>) {
        let _ = self.host_mount.set(host_mount);
        let _ = self.parent_pid.set(root_pid);
    }

    /// Override the clone fallback policy (defaults to [`CloneFallbackPolicy::Auto`]).
    pub fn with_fallback_policy(mut self, policy: CloneFallbackPolicy) -> Self {
        self.fallback_policy = policy;
        self
    }

    fn require_host_and_pid(&self) -> Result<(Arc<HostMountService>, Pid)> {
        let host_mount = self.host_mount.get().cloned().ok_or_else(|| {
            anyhow::anyhow!("HostMountService is not attached to CoW isolation port")
        })?;
        let pid_val = self
            .parent_pid
            .get()
            .map(|pid| pid.load(Ordering::Acquire))
            .filter(|&pid| pid > 0)
            .ok_or_else(|| anyhow::anyhow!("Root PID is not bound to CoW isolation port"))?;
        Ok((host_mount, Pid(pid_val)))
    }
}

#[async_trait]
impl WorkspaceIsolationPort for CowWorkspaceIsolationAdapter {
    async fn allocate_workspace(
        &self,
        request: IsolatedWorkspaceRequest,
    ) -> Result<IsolatedWorkspaceAllocation> {
        let (host_mount, parent_pid) = self.require_host_and_pid()?;
        anyhow::ensure!(
            host_mount.process_holds_grant(parent_pid, &request.source_grant_id),
            "parent Process {} does not hold active source_grant `{}`",
            parent_pid.0,
            request.source_grant_id
        );
        let host_root = resolve_grant_host_root(&host_mount, &request.source_grant_id)
            .map_err(anyhow::Error::msg)?;

        let manager = self.manager.clone();
        let fallback_policy = self.fallback_policy;
        let summary = tokio::task::spawn_blocking(move || {
            manager.create_workspace(
                &host_root,
                CreateWorkspaceOptions {
                    fallback_policy,
                    ..Default::default()
                },
            )
        })
        .await??;

        let parent_namespace_path = format!("/mnt/cow-{}", summary.workspace_ref.as_str());
        let grant = match self.manager.project_into_process(
            &summary.workspace_ref,
            &host_mount,
            parent_pid,
            &parent_namespace_path,
            HostMountAccess::ReadWrite,
        ) {
            Ok(grant) => grant,
            Err(err) => {
                let manager = self.manager.clone();
                let ws_ref = summary.workspace_ref.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    manager.discard_workspace(&ws_ref, Some(&host_mount), "workstream-rollback")
                })
                .await;
                return Err(err);
            }
        };

        let ws_ref_str = summary.workspace_ref.as_str().to_string();
        self.source_grants
            .lock()
            .unwrap()
            .insert(ws_ref_str.clone(), request.source_grant_id);

        Ok(IsolatedWorkspaceAllocation {
            workspace_ref: ws_ref_str,
            backing: to_isolated_backing(summary.backing_kind),
            narrowed_grant: SpawnHostMount {
                grant: grant.id,
                target: request.source_namespace_path,
                access: SpawnMountAccess::ReadWrite,
            },
            branch_ref: None,
        })
    }

    async fn collect_handoff(
        &self,
        allocation: &IsolatedWorkspaceAllocation,
        workstream_name: &str,
        rollout_ref: Option<&str>,
        why: &str,
    ) -> Result<WorkspaceMergeHandoff> {
        let ws_ref = WorkspaceRef::parse(&allocation.workspace_ref)?;
        let provenance = build_single_line_provenance(
            Some(workstream_name),
            rollout_ref,
            Some(why),
            "Isolated M8.3 CoW workstream delta",
        )
        .map_err(anyhow::Error::msg)?;

        let manager = self.manager.clone();
        let ws_ref_clone = ws_ref.clone();
        let provenance_clone = provenance.clone();
        let (diff, patch_evidence_ref) = tokio::task::spawn_blocking(move || {
            let diff = manager.diff_summary(&ws_ref_clone)?;
            let patch_ref = if diff.is_empty() {
                None
            } else {
                let file_name = manager.write_evidence_patch(&ws_ref_clone, &provenance_clone)?;
                Some(format!("evidence://patches/{file_name}"))
            };
            Ok::<_, CowWorkspaceError>((diff, patch_ref))
        })
        .await??;

        let changed_files = diff
            .changed_paths
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let diff_summary = format_opaque_diff_summary(&diff);
        validate_no_raw_host_path(&diff_summary)
            .map_err(|_| anyhow::anyhow!("diff summary unexpectedly contained a raw Host path"))?;

        let source_grant_id = self
            .source_grants
            .lock()
            .unwrap()
            .get(&allocation.workspace_ref)
            .cloned()
            .unwrap_or_default();

        Ok(WorkspaceMergeHandoff {
            workspace_ref: allocation.workspace_ref.clone(),
            backing: allocation.backing,
            source_grant_id,
            workstream_name: workstream_name.to_string(),
            rollout_ref: rollout_ref.map(str::to_string),
            why: why.to_string(),
            branch_ref: allocation.branch_ref.clone(),
            changed_files,
            diff_summary,
            patch_evidence_ref,
            sync_status: WorkspaceSyncStatus::PendingMerge,
            conflicts: Vec::new(),
        })
    }

    async fn sync_back(
        &self,
        allocation: &IsolatedWorkspaceAllocation,
        workstream_name: &str,
        rollout_ref: Option<&str>,
        why: &str,
    ) -> Result<WorkspaceMergeHandoff> {
        let mut handoff = self
            .collect_handoff(allocation, workstream_name, rollout_ref, why)
            .await?;
        if handoff.changed_files.is_empty() {
            handoff.sync_status = WorkspaceSyncStatus::SyncedBack;
            return Ok(handoff);
        }

        let ws_ref = WorkspaceRef::parse(&allocation.workspace_ref)?;
        let provenance = build_single_line_provenance(
            Some(workstream_name),
            rollout_ref,
            Some(why),
            "Isolated M8.3 CoW workstream sync-back",
        )
        .map_err(anyhow::Error::msg)?;

        let manager = self.manager.clone();
        let sync_res =
            tokio::task::spawn_blocking(move || manager.sync_back(&ws_ref, &provenance)).await?;

        match sync_res {
            Ok(_) => {
                handoff.sync_status = WorkspaceSyncStatus::SyncedBack;
                Ok(handoff)
            }
            Err(CowWorkspaceError::SyncConflict { conflicts }) => {
                handoff.sync_status = WorkspaceSyncStatus::ConflictAborted;
                handoff.conflicts = conflicts
                    .into_iter()
                    .map(|c| WorkspaceSyncConflict {
                        path: c.path,
                        reason: format!("{:?}", c.reason),
                    })
                    .collect();
                handoff.diff_summary = format!(
                    "Sync-back aborted fail-closed due to {} concurrent Host conflict(s); patch preserved.",
                    handoff.conflicts.len()
                );
                Ok(handoff)
            }
            Err(err) => Err(err.into()),
        }
    }

    async fn discard_workspace(&self, workspace_ref: &str) -> Result<()> {
        let ws_ref = WorkspaceRef::parse(workspace_ref)?;
        self.source_grants.lock().unwrap().remove(workspace_ref);
        let manager = self.manager.clone();
        let host_mount = self.host_mount.get().cloned();
        match tokio::task::spawn_blocking(move || {
            manager.discard_workspace(&ws_ref, host_mount.as_deref(), "workstream-discard")
        })
        .await?
        {
            Ok(()) | Err(CowWorkspaceError::UnknownWorkspace(_)) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

/// Host-lifetime bundle holding [`CowWorkspaceManager`], `/mnt/branch` [`BranchFs`],
/// its [`CowBranchWorkspaceAdapter`], [`CowWorkspaceIsolationAdapter`], and [`ArenaExitFlushRegistry`].
#[derive(Clone)]
pub struct CowHostRuntime {
    manager: CowWorkspaceManager,
    branch_fs: Arc<BranchFs>,
    branch_adapter: Arc<CowBranchWorkspaceAdapter>,
    isolation_adapter: Arc<CowWorkspaceIsolationAdapter>,
    arena_exit_registry: ArenaExitFlushRegistry,
}

impl CowHostRuntime {
    /// Construct a CoW host runtime, binding `/mnt/branch` and `WorkspaceIsolationPort`.
    pub fn new(manager: CowWorkspaceManager) -> Self {
        let branch_adapter = Arc::new(CowBranchWorkspaceAdapter::new(manager.clone()));
        let branch_fs = Arc::new(BranchFs::with_workspace_port(branch_adapter.clone()));
        let isolation_adapter = Arc::new(CowWorkspaceIsolationAdapter::new(manager.clone()));
        Self {
            manager,
            branch_fs,
            branch_adapter,
            isolation_adapter,
            arena_exit_registry: ArenaExitFlushRegistry::new(),
        }
    }

    pub fn manager(&self) -> &CowWorkspaceManager {
        &self.manager
    }

    pub fn branch_fs(&self) -> &Arc<BranchFs> {
        &self.branch_fs
    }

    pub fn branch_adapter(&self) -> &Arc<CowBranchWorkspaceAdapter> {
        &self.branch_adapter
    }

    pub fn isolation_adapter(&self) -> &Arc<CowWorkspaceIsolationAdapter> {
        &self.isolation_adapter
    }

    pub fn arena_exit_registry(&self) -> &ArenaExitFlushRegistry {
        &self.arena_exit_registry
    }

    pub fn attach_host_mount(&self, host_mount: Arc<HostMountService>) {
        self.branch_adapter.attach_host_mount(host_mount);
    }

    pub fn attach_host_mount_and_root_pid(
        &self,
        host_mount: Arc<HostMountService>,
        root_pid: Arc<AtomicU64>,
    ) {
        self.branch_adapter.attach_host_mount(host_mount.clone());
        self.isolation_adapter.attach(host_mount, root_pid);
    }

    /// Ensure `/mnt/branch/branches/base` exists so `fork` commands targeting `"base"` succeed.
    pub async fn ensure_base_branch(&self) -> Result<()> {
        if self.branch_fs.branch("base").await.is_none() {
            self.branch_fs
                .install_base_branch("base", [b"base".as_slice()])
                .await
                .map_err(|code| anyhow::anyhow!("install /mnt/branch/branches/base: {code:?}"))?;
        }
        Ok(())
    }

    /// Build an [`ExecutableArenaCoordinator`] bound to `CowArenaWorkspaceAdapter`,
    /// `/mnt/branch` (`BranchFs`), and this runtime's [`ArenaExitFlushRegistry`].
    pub fn arena_coordinator(
        &self,
        host_mount: Arc<HostMountService>,
        parent_pid: Pid,
    ) -> ExecutableArenaCoordinator {
        let adapter = Arc::new(CowArenaWorkspaceAdapter::new(
            self.manager.clone(),
            host_mount,
            parent_pid,
        ));
        let branch_client =
            BranchFsApClient::direct(InProcessTransport::new(self.branch_fs.clone()));
        ExecutableArenaCoordinator::new(self.arena_exit_registry.clone())
            .with_workspace_port(adapter)
            .with_branch_client(branch_client)
    }

    /// Flush all unapplied arena winners and `/mnt/branch` selections to System Store
    /// evidence and delete every remaining active CoW clone for this Host instance.
    pub async fn flush_on_exit(
        &self,
        host_mount: Option<&HostMountService>,
    ) -> Result<ExitFlushReport> {
        let _ = self.arena_exit_registry.flush_all_on_exit().await;
        let _ = self.branch_fs.flush_on_exit().await;
        let manager = self.manager.clone();
        // Revoke any remaining active projections before sweeping clones synchronously.
        if let Some(svc) = host_mount {
            for ws in manager.list_workspaces() {
                if let Some(grant_id) = ws.projected_grant_id {
                    let _ = svc.revoke(&grant_id, "host-exit");
                }
            }
        }
        let report =
            tokio::task::spawn_blocking(move || manager.flush_all_on_exit(None, None, "host-exit"))
                .await??;
        Ok(report)
    }
}

fn resolve_grant_host_root(
    host_mount: &HostMountService,
    grant_id: &str,
) -> std::result::Result<PathBuf, String> {
    let (_record, export) = host_mount
        .active_grant_export(grant_id)
        .ok_or_else(|| format!("unknown or revoked Host Mount grant `{grant_id}`"))?;
    native_export_host_path(&*export).ok_or_else(|| {
        format!("Host Mount grant `{grant_id}` is not backed by a native Host directory")
    })
}

fn to_branch_backing_kind(backing: WorkspaceBacking) -> WorkspaceBackingKind {
    match backing {
        WorkspaceBacking::NativeCow | WorkspaceBacking::SiblingVolumeCow => {
            WorkspaceBackingKind::NativeCow
        }
        WorkspaceBacking::GitWorktree => WorkspaceBackingKind::GitWorktree,
    }
}

fn to_arena_backing_kind(backing: WorkspaceBacking) -> ArenaWorkspaceBackingKind {
    match backing {
        WorkspaceBacking::NativeCow | WorkspaceBacking::SiblingVolumeCow => {
            ArenaWorkspaceBackingKind::NativeCow
        }
        WorkspaceBacking::GitWorktree => ArenaWorkspaceBackingKind::GitWorktree,
    }
}

fn to_isolated_backing(backing: WorkspaceBacking) -> IsolatedWorkspaceBacking {
    match backing {
        WorkspaceBacking::NativeCow | WorkspaceBacking::SiblingVolumeCow => {
            IsolatedWorkspaceBacking::OsCowClone
        }
        WorkspaceBacking::GitWorktree => IsolatedWorkspaceBacking::GitWorktree,
    }
}

fn map_cow_err_to_branch(err: CowWorkspaceError) -> BranchWorkspacePortError {
    match err {
        CowWorkspaceError::UnknownWorkspace(msg) => BranchWorkspacePortError::not_found(msg),
        CowWorkspaceError::InvalidInput(msg)
        | CowWorkspaceError::NativeCowRequiredButUnavailable(msg) => {
            BranchWorkspacePortError::rejected(msg)
        }
        CowWorkspaceError::ActiveCapacityReached { .. }
        | CowWorkspaceError::CrossVolumeNonGitUnsupported => {
            BranchWorkspacePortError::rejected(err.to_string())
        }
        other => BranchWorkspacePortError::io(other.to_string()),
    }
}

fn format_opaque_diff_summary(diff: &DiffSummary) -> String {
    if diff.is_empty() {
        return "0 files changed (+0 ~0 -0)".to_string();
    }
    let count = diff.changed_paths.len();
    let preview = diff
        .changed_paths
        .iter()
        .take(12)
        .map(|entry| {
            let tag = match entry.kind {
                FileChangeKind::Added => "A",
                FileChangeKind::Modified => "M",
                FileChangeKind::Deleted => "D",
            };
            format!("{tag} {}", entry.path)
        })
        .collect::<Vec<_>>()
        .join(", ");
    if count > 12 {
        format!(
            "{count} file(s) changed ({}): {preview}, (+{} more)",
            diff.short_summary(),
            count - 12
        )
    } else {
        format!(
            "{count} file(s) changed ({}): {preview}",
            diff.short_summary()
        )
    }
}

fn sanitize_single_line(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_single_line_provenance(
    workstream: Option<&str>,
    rollout_ref: Option<&str>,
    why: Option<&str>,
    default_why: &str,
) -> std::result::Result<IntentProvenance, String> {
    let ws = workstream
        .map(sanitize_single_line)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "alan-arena".to_string());
    let rollout = rollout_ref
        .map(sanitize_single_line)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "rollout-unknown".to_string());
    let why_clean = why
        .map(sanitize_single_line)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_why.to_string());
    IntentProvenance::new(ws, rollout, why_clean).map_err(|err| err.to_string())
}

fn format_scorecard_summary(
    score: Option<f64>,
    summary: Option<&str>,
    diff_summary: Option<&str>,
    checks_summary: Option<&str>,
) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(score) = score {
        parts.push(format!("score={score:.3}"));
    }
    if let Some(summary) = summary.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("summary={}", sanitize_single_line(summary)));
    }
    if let Some(diff) = diff_summary.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("diff={}", sanitize_single_line(diff)));
    }
    if let Some(checks) = checks_summary.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("checks={}", sanitize_single_line(checks)));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}
