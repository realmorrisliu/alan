//! Host-owned Native OS Copy-on-Write Workspace Substrate (`alan-os-host`).
//!
//! Materializes isolated full-tree workspaces under `System Store/workspaces/<instance_id>/<workspace_id>/`
//! using macOS APFS `clonefile(2)` or Linux `FICLONE` reflinks (with deterministic
//! `git worktree` / optional sibling-volume fallback), enforces per-instance clone
//! caps and startup dead-PID orphan GC, and provides explicit 3-way conflict-checked
//! sync-back, unified `.patch` export, Git branch export, and `/mnt` Host Mount
//! projection helpers without leaking raw Host paths above `alan-os-host` (`ADR-0050`).

pub mod adapters;
mod clone;
mod gc;
mod git_export;
mod hash;
mod manifest;
mod patch;
mod sync;
pub mod types;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use alan_agent_engine::GitPushDelegationScope;
use alan_kernel::Pid;
use alan_service_manager::{
    HostMountAccess, HostMountExport, HostMountGrantRecord, HostMountService,
};
use anyhow::{Context, Result};
use serde::Serialize;

use clone::{materialize_clone, remove_cloned_tree};
use gc::{
    WORKSPACE_META_FILE, WorkspaceMetadata, gc_orphaned_instances, now_ms, write_instance_metadata,
    write_workspace_metadata,
};
use git_export::export_workspace_to_git_branch;
use manifest::{TreeManifest, compute_diff_summary};
use patch::generate_patch;
use sync::perform_sync_back;

pub use adapters::{CowArenaWorkspaceAdapter, CowBranchWorkspaceAdapter, CowHostRuntime};
pub use types::{
    CloneFallbackPolicy, ConflictReason, CowWorkspaceConfig, CowWorkspaceError,
    CowWorkspaceSummary, CreateWorkspaceOptions, DEFAULT_COW_EXCLUDES,
    DEFAULT_MAX_ACTIVE_COW_WORKSPACES, DiffPathEntry, DiffSummary, ExitFlushReport, FileChangeKind,
    FlushedWorkspaceEvidence, GcReport, GitBranchExportReport, IntentProvenance, PatchExportReport,
    SyncBackReport, SyncConflict, WorkspaceBacking, WorkspaceRef,
};

const BASE_BLOBS_FILE: &str = "base.blobs";
const BASE_MANIFEST_FILE: &str = "base_manifest.json";

struct WorkspaceRecord {
    workspace_ref: WorkspaceRef,
    backing_kind: WorkspaceBacking,
    host_root: PathBuf,
    workspace_dir: PathBuf,
    tree_path: PathBuf,
    sibling_root: Option<PathBuf>,
    source_is_git: bool,
    created_at_ms: u128,
    applied_to_host: bool,
    excludes: Vec<String>,
    base_manifest: TreeManifest,
    base_blob_path: PathBuf,
    projected_grant_ids: Vec<String>,
    latest_namespace_path: Option<String>,
}

impl WorkspaceRecord {
    fn summary(&self) -> CowWorkspaceSummary {
        CowWorkspaceSummary {
            workspace_ref: self.workspace_ref.clone(),
            backing_kind: self.backing_kind,
            source_is_git: self.source_is_git,
            created_at_ms: self.created_at_ms,
            applied_to_host: self.applied_to_host,
            projected_grant_id: self.projected_grant_ids.last().cloned(),
            projected_namespace_path: self.latest_namespace_path.clone(),
        }
    }

    fn disk_metadata(&self) -> WorkspaceMetadata {
        WorkspaceMetadata {
            workspace_ref: self.workspace_ref.as_str().to_string(),
            backing_kind: self.backing_kind,
            host_root: self.host_root.clone(),
            tree_path: self.tree_path.clone(),
            sibling_root: self.sibling_root.clone(),
            source_is_git: self.source_is_git,
            created_at_ms: self.created_at_ms,
            applied_to_host: self.applied_to_host,
            projected_grant_id: self.projected_grant_ids.last().cloned(),
            projected_namespace_path: self.latest_namespace_path.clone(),
            excludes: self.excludes.clone(),
        }
    }
}

struct ManagerState {
    config: CowWorkspaceConfig,
    instance_dir: PathBuf,
    workspaces: BTreeMap<WorkspaceRef, WorkspaceRecord>,
}

/// Host-owned manager for Native OS Copy-on-Write workspaces.
#[derive(Clone)]
pub struct CowWorkspaceManager {
    state: Arc<Mutex<ManagerState>>,
}

impl fmt::Debug for CowWorkspaceManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let guard = self.state.lock().ok();
        let active = guard.as_ref().map_or(0, |s| s.workspaces.len());
        let instance_id = guard
            .as_ref()
            .map_or("<locked>", |s| s.config.instance_id.as_str());
        f.debug_struct("CowWorkspaceManager")
            .field("instance_id", &instance_id)
            .field("active_workspaces", &active)
            .finish_non_exhaustive()
    }
}

impl CowWorkspaceManager {
    /// Initialize a CoW workspace manager for an Alan instance and sweep any dead-PID orphans.
    pub fn new(config: CowWorkspaceConfig) -> Result<(Self, GcReport)> {
        let workspaces_root = config.system_store.workspaces()?;
        fs::create_dir_all(&workspaces_root).with_context(|| {
            format!(
                "create System Store workspaces root {}",
                workspaces_root.display()
            )
        })?;
        let gc_report = gc_orphaned_instances(&workspaces_root, &config.instance_id)?;
        let instance_dir = config
            .system_store
            .instance_workspaces(&config.instance_id)?;
        write_instance_metadata(&instance_dir, &config.instance_id)?;

        Ok((
            Self {
                state: Arc::new(Mutex::new(ManagerState {
                    config,
                    instance_dir,
                    workspaces: BTreeMap::new(),
                })),
            },
            gc_report,
        ))
    }

    /// Re-run dead-PID orphan garbage collection under `System Store/workspaces/`.
    pub fn gc_orphaned_instances(&self) -> Result<GcReport> {
        let (workspaces_root, instance_id) = {
            let guard = self.state.lock().unwrap();
            (
                guard.config.system_store.workspaces()?,
                guard.config.instance_id.clone(),
            )
        };
        gc_orphaned_instances(&workspaces_root, &instance_id)
    }

    /// Current number of active workspaces managed by this instance.
    pub fn active_count(&self) -> usize {
        self.state.lock().unwrap().workspaces.len()
    }

    /// Configured active clone cap for this instance.
    pub fn max_active(&self) -> usize {
        self.state.lock().unwrap().config.max_active_cow_workspaces
    }

    /// Return opaque summaries for all active workspaces in this instance.
    pub fn list_workspaces(&self) -> Vec<CowWorkspaceSummary> {
        self.state
            .lock()
            .unwrap()
            .workspaces
            .values()
            .map(WorkspaceRecord::summary)
            .collect()
    }

    /// Look up one workspace's opaque summary.
    pub fn get_workspace(&self, workspace_ref: &WorkspaceRef) -> Option<CowWorkspaceSummary> {
        self.state
            .lock()
            .unwrap()
            .workspaces
            .get(workspace_ref)
            .map(WorkspaceRecord::summary)
    }

    pub(crate) fn workspace_host_root(&self, workspace_ref: &WorkspaceRef) -> Option<PathBuf> {
        self.state
            .lock()
            .unwrap()
            .workspaces
            .get(workspace_ref)
            .map(|record| record.host_root.clone())
    }

    pub(crate) fn existing_flushed_patch_name(&self, workspace_ref: &str) -> Option<String> {
        let system_store = {
            let guard = self.state.lock().unwrap();
            guard.config.system_store.clone()
        };
        let evidence_dir = system_store.cow_workspace_evidence().ok()?;
        let patch_name = format!("{workspace_ref}.patch");
        evidence_dir
            .join(&patch_name)
            .is_file()
            .then_some(patch_name)
    }

    /// Fork an isolated full-tree CoW workspace from `host_root`.
    pub fn create_workspace(
        &self,
        host_root: &Path,
        options: CreateWorkspaceOptions,
    ) -> std::result::Result<CowWorkspaceSummary, CowWorkspaceError> {
        let (config, instance_dir) = {
            let guard = self.state.lock().unwrap();
            let active = guard.workspaces.len();
            let max = guard.config.max_active_cow_workspaces;
            if active >= max {
                return Err(CowWorkspaceError::ActiveCapacityReached { active, max });
            }
            (guard.config.clone(), guard.instance_dir.clone())
        };

        let symlink_meta = fs::symlink_metadata(host_root)?;
        if symlink_meta.file_type().is_symlink() {
            return Err(CowWorkspaceError::InvalidInput(
                "CoW workspace source directory must not be a symlink".to_string(),
            ));
        }
        if !symlink_meta.is_dir() {
            return Err(CowWorkspaceError::InvalidInput(
                "CoW workspace source path must be a directory".to_string(),
            ));
        }
        let canonical_host_root = dunce::canonicalize(host_root)?;

        let workspace_id = format!("cow-ws-{}", uuid::Uuid::new_v4());
        let workspace_ref = WorkspaceRef::parse(&workspace_id)?;
        let workspace_dir = instance_dir.join(&workspace_id);
        fs::create_dir_all(&workspace_dir)?;

        let materialized = match materialize_clone(
            &config,
            &canonical_host_root,
            &workspace_dir,
            &workspace_id,
            options.fallback_policy,
        ) {
            Ok(m) => m,
            Err(err) => {
                let _ = fs::remove_dir_all(&workspace_dir);
                return Err(err);
            }
        };

        let mut excludes = config.default_excludes.clone();
        for extra in options.extra_excludes {
            let trimmed = extra.trim().to_string();
            if !trimmed.is_empty() && !excludes.contains(&trimmed) {
                excludes.push(trimmed);
            }
        }

        let base_blob_path = workspace_dir.join(BASE_BLOBS_FILE);
        let base_manifest = match TreeManifest::capture_base(
            &materialized.tree_path,
            &excludes,
            &base_blob_path,
            config.max_base_blob_bytes_per_file,
            config.max_total_base_blobs_bytes,
        ) {
            Ok(manifest) => manifest,
            Err(err) => {
                let _ = remove_cloned_tree(
                    &canonical_host_root,
                    &materialized.tree_path,
                    materialized.backing_kind,
                    materialized.sibling_root.as_deref(),
                );
                let _ = fs::remove_dir_all(&workspace_dir);
                return Err(CowWorkspaceError::Other(err));
            }
        };

        if let Err(err) = write_json_atomic(
            &workspace_dir.join(BASE_MANIFEST_FILE),
            &base_manifest,
            "base manifest",
        ) {
            let _ = remove_cloned_tree(
                &canonical_host_root,
                &materialized.tree_path,
                materialized.backing_kind,
                materialized.sibling_root.as_deref(),
            );
            let _ = fs::remove_dir_all(&workspace_dir);
            return Err(CowWorkspaceError::Other(err));
        }

        let created_at_ms = now_ms();
        let record = WorkspaceRecord {
            workspace_ref: workspace_ref.clone(),
            backing_kind: materialized.backing_kind,
            host_root: canonical_host_root,
            workspace_dir: workspace_dir.clone(),
            tree_path: materialized.tree_path,
            sibling_root: materialized.sibling_root,
            source_is_git: materialized.source_is_git,
            created_at_ms,
            applied_to_host: false,
            excludes,
            base_manifest,
            base_blob_path,
            projected_grant_ids: Vec::new(),
            latest_namespace_path: None,
        };

        if let Err(err) = write_workspace_metadata(&workspace_dir, &record.disk_metadata()) {
            let _ = remove_cloned_tree(
                &record.host_root,
                &record.tree_path,
                record.backing_kind,
                record.sibling_root.as_deref(),
            );
            let _ = fs::remove_dir_all(&workspace_dir);
            return Err(CowWorkspaceError::Other(err));
        }

        let summary = record.summary();
        let mut guard = self.state.lock().unwrap();
        if guard.workspaces.len() >= guard.config.max_active_cow_workspaces {
            drop(guard);
            let _ = remove_cloned_tree(
                &record.host_root,
                &record.tree_path,
                record.backing_kind,
                record.sibling_root.as_deref(),
            );
            let _ = fs::remove_dir_all(&workspace_dir);
            return Err(CowWorkspaceError::ActiveCapacityReached {
                active: config.max_active_cow_workspaces,
                max: config.max_active_cow_workspaces,
            });
        }
        guard.workspaces.insert(workspace_ref, record);
        Ok(summary)
    }

    /// Produce an opaque [`HostMountExport`] pinned strictly to the CoW clone directory.
    pub fn export_workspace(
        &self,
        workspace_ref: &WorkspaceRef,
        namespace_path: &str,
        access: HostMountAccess,
    ) -> Result<Arc<dyn HostMountExport>> {
        let tree_path = {
            let guard = self.state.lock().unwrap();
            let record = guard
                .workspaces
                .get(workspace_ref)
                .ok_or_else(|| CowWorkspaceError::UnknownWorkspace(workspace_ref.to_string()))?;
            record.tree_path.clone()
        };
        crate::host_mounts::native_export(namespace_path, &tree_path, access)
    }

    /// Project a CoW workspace directly into `pid`'s namespace on `HostMountService` at `/mnt/...`.
    pub fn project_into_process(
        &self,
        workspace_ref: &WorkspaceRef,
        service: &HostMountService,
        pid: Pid,
        namespace_path: &str,
        access: HostMountAccess,
    ) -> Result<HostMountGrantRecord> {
        self.project_into_process_with_scopes(
            workspace_ref,
            service,
            pid,
            namespace_path,
            access,
            Vec::new(),
        )
    }

    /// Project a CoW workspace into `pid`'s namespace with optional narrow `git push` scopes.
    pub fn project_into_process_with_scopes(
        &self,
        workspace_ref: &WorkspaceRef,
        service: &HostMountService,
        pid: Pid,
        namespace_path: &str,
        access: HostMountAccess,
        git_push_scopes: Vec<GitPushDelegationScope>,
    ) -> Result<HostMountGrantRecord> {
        let export = self.export_workspace(workspace_ref, namespace_path, access)?;
        let grant_id = format!("cow-grant-{}", uuid::Uuid::new_v4());
        let grant = service.grant_direct_with_scopes(
            pid,
            grant_id.clone(),
            format!("CoW Workspace ({workspace_ref})"),
            namespace_path.to_string(),
            access,
            export,
            format!("cow-workspace:{workspace_ref}"),
            git_push_scopes,
        )?;

        let mut guard = self.state.lock().unwrap();
        if let Some(record) = guard.workspaces.get_mut(workspace_ref) {
            record.projected_grant_ids.push(grant_id);
            record.latest_namespace_path = Some(namespace_path.to_string());
            let _ = write_workspace_metadata(&record.workspace_dir, &record.disk_metadata());
        }
        Ok(grant)
    }

    /// Compute the workspace's diff summary relative to `BaseManifest` (excluding `.git/` and build caches).
    pub fn diff_summary(
        &self,
        workspace_ref: &WorkspaceRef,
    ) -> std::result::Result<DiffSummary, CowWorkspaceError> {
        let (tree_path, excludes, base_manifest) = {
            let guard = self.state.lock().unwrap();
            let record = guard
                .workspaces
                .get(workspace_ref)
                .ok_or_else(|| CowWorkspaceError::UnknownWorkspace(workspace_ref.to_string()))?;
            (
                record.tree_path.clone(),
                record.excludes.clone(),
                record.base_manifest.clone(),
            )
        };
        let workspace_now = TreeManifest::scan(&tree_path, &excludes)?;
        Ok(compute_diff_summary(&base_manifest, &workspace_now))
    }

    /// Perform explicit 3-way conflict-checked sync-back (`base` vs `workspace` vs `host_now`).
    ///
    /// Fails closed with [`CowWorkspaceError::SyncConflict`] on any concurrent Host conflict
    /// without mutating any Host file.
    pub fn sync_back(
        &self,
        workspace_ref: &WorkspaceRef,
        provenance: &IntentProvenance,
    ) -> std::result::Result<SyncBackReport, CowWorkspaceError> {
        let (backing_kind, base_manifest, tree_path, host_root, excludes, system_store) = {
            let guard = self.state.lock().unwrap();
            let record = guard
                .workspaces
                .get(workspace_ref)
                .ok_or_else(|| CowWorkspaceError::UnknownWorkspace(workspace_ref.to_string()))?;
            (
                record.backing_kind,
                record.base_manifest.clone(),
                record.tree_path.clone(),
                record.host_root.clone(),
                record.excludes.clone(),
                guard.config.system_store.clone(),
            )
        };

        let report = perform_sync_back(
            workspace_ref,
            backing_kind,
            &base_manifest,
            &tree_path,
            &host_root,
            &excludes,
            &system_store,
            provenance,
        )?;

        let mut guard = self.state.lock().unwrap();
        if let Some(record) = guard.workspaces.get_mut(workspace_ref) {
            record.applied_to_host = true;
            let _ = write_workspace_metadata(&record.workspace_dir, &record.disk_metadata());
        }
        Ok(report)
    }

    /// Export a clean unified `.patch` excluding `.git/` and build/cache directories,
    /// attaching `workstream` + opaque `rollout_ref` + 1-line `why` Intent Provenance.
    pub fn export_patch(
        &self,
        workspace_ref: &WorkspaceRef,
        provenance: &IntentProvenance,
    ) -> std::result::Result<PatchExportReport, CowWorkspaceError> {
        let (backing_kind, base_manifest, base_blob_path, tree_path, host_root, excludes) = {
            let guard = self.state.lock().unwrap();
            let record = guard
                .workspaces
                .get(workspace_ref)
                .ok_or_else(|| CowWorkspaceError::UnknownWorkspace(workspace_ref.to_string()))?;
            (
                record.backing_kind,
                record.base_manifest.clone(),
                record.base_blob_path.clone(),
                record.tree_path.clone(),
                record.host_root.clone(),
                record.excludes.clone(),
            )
        };
        generate_patch(
            workspace_ref,
            backing_kind,
            &base_manifest,
            &base_blob_path,
            &tree_path,
            &host_root,
            &excludes,
            provenance,
        )
    }

    /// Materialize clean workspace changes onto a fresh Git branch in the Host repository
    /// with `Alan-Workstream:` and `Alan-Rollout:` commit trailers.
    pub fn export_to_git_branch(
        &self,
        workspace_ref: &WorkspaceRef,
        branch_name: &str,
        provenance: &IntentProvenance,
    ) -> std::result::Result<GitBranchExportReport, CowWorkspaceError> {
        let (base_manifest, tree_path, host_root, excludes) = {
            let guard = self.state.lock().unwrap();
            let record = guard
                .workspaces
                .get(workspace_ref)
                .ok_or_else(|| CowWorkspaceError::UnknownWorkspace(workspace_ref.to_string()))?;
            (
                record.base_manifest.clone(),
                record.tree_path.clone(),
                record.host_root.clone(),
                record.excludes.clone(),
            )
        };
        export_workspace_to_git_branch(
            workspace_ref,
            &base_manifest,
            &tree_path,
            &host_root,
            &excludes,
            branch_name,
            provenance,
        )
    }

    /// Revoke any active `HostMountService` projections for `workspace_ref` and delete the clone.
    pub fn discard_workspace(
        &self,
        workspace_ref: &WorkspaceRef,
        service: Option<&HostMountService>,
        actor: &str,
    ) -> std::result::Result<(), CowWorkspaceError> {
        let record = {
            let mut guard = self.state.lock().unwrap();
            guard
                .workspaces
                .remove(workspace_ref)
                .ok_or_else(|| CowWorkspaceError::UnknownWorkspace(workspace_ref.to_string()))?
        };

        if let Some(svc) = service {
            for grant_id in &record.projected_grant_ids {
                let _ = svc.revoke(grant_id, actor);
            }
        }

        remove_cloned_tree(
            &record.host_root,
            &record.tree_path,
            record.backing_kind,
            record.sibling_root.as_deref(),
        )?;
        if record.workspace_dir.exists() {
            fs::remove_dir_all(&record.workspace_dir)?;
        }
        let _ = fs::remove_file(record.workspace_dir.join(WORKSPACE_META_FILE));
        Ok(())
    }

    /// Flush an unapplied winning workspace's `.patch` and scorecard summary into
    /// System Store rollout evidence, then delete the physical CoW clone directory.
    pub fn flush_unapplied_to_evidence(
        &self,
        workspace_ref: &WorkspaceRef,
        service: Option<&HostMountService>,
        provenance: &IntentProvenance,
        scorecard_summary: Option<&str>,
        actor: &str,
    ) -> std::result::Result<Option<FlushedWorkspaceEvidence>, CowWorkspaceError> {
        let (applied, backing_kind, system_store) = {
            let guard = self.state.lock().unwrap();
            let record = guard
                .workspaces
                .get(workspace_ref)
                .ok_or_else(|| CowWorkspaceError::UnknownWorkspace(workspace_ref.to_string()))?;
            (
                record.applied_to_host,
                record.backing_kind,
                guard.config.system_store.clone(),
            )
        };

        if applied {
            self.discard_workspace(workspace_ref, service, actor)?;
            return Ok(None);
        }

        let patch_report = match self.export_patch(workspace_ref, provenance) {
            Ok(report) => Some(report),
            Err(CowWorkspaceError::NoChangesToExport) if scorecard_summary.is_none() => None,
            Err(CowWorkspaceError::NoChangesToExport) => Some(PatchExportReport {
                workspace_ref: workspace_ref.clone(),
                backing_kind,
                patch_text: format!(
                    "From: Alan CoW Workspace <alan@local>\nSubject: [PATCH] {}\nAlan-Workstream: {}\nAlan-Rollout: {}\nAlan-Why: {}\nAlan-Workspace: {}\nAlan-Backing: {}\n\n",
                    provenance.why,
                    provenance.workstream,
                    provenance.rollout_ref,
                    provenance.why,
                    workspace_ref,
                    backing_kind
                ),
                diff_summary: DiffSummary::default(),
                provenance: provenance.clone(),
            }),
            Err(err) => return Err(err),
        };

        let flushed = if let Some(report) = patch_report {
            let evidence_dir = system_store.cow_workspace_evidence()?;
            fs::create_dir_all(&evidence_dir)?;
            let patch_file_name = format!("{}.patch", workspace_ref.as_str());
            let summary_file_name = format!("{}.summary.json", workspace_ref.as_str());
            let patch_path = evidence_dir.join(&patch_file_name);
            let summary_path = evidence_dir.join(&summary_file_name);

            write_bytes_atomic(&patch_path, report.patch_text.as_bytes(), "evidence patch")?;

            let summary_doc = EvidenceSummaryDoc {
                workspace_ref: workspace_ref.clone(),
                backing_kind: report.backing_kind,
                patch_file_name: patch_file_name.clone(),
                diff_summary: report.diff_summary.clone(),
                provenance: report.provenance.clone(),
                scorecard_summary: scorecard_summary.map(ToString::to_string),
                flushed_at_ms: now_ms(),
            };
            write_json_atomic(&summary_path, &summary_doc, "evidence summary")?;

            Some(FlushedWorkspaceEvidence {
                workspace_ref: workspace_ref.clone(),
                backing_kind: report.backing_kind,
                patch_file_name,
                summary_file_name,
                diff_summary: report.diff_summary,
                provenance: report.provenance,
            })
        } else {
            None
        };

        self.discard_workspace(workspace_ref, service, actor)?;
        Ok(flushed)
    }

    /// Exit cleanup helper: flush an optional unapplied winning workspace into System Store
    /// evidence and delete all remaining active clone directories for this instance.
    pub fn flush_all_on_exit(
        &self,
        service: Option<&HostMountService>,
        selected_winner: Option<(&WorkspaceRef, &IntentProvenance, Option<&str>)>,
        actor: &str,
    ) -> std::result::Result<ExitFlushReport, CowWorkspaceError> {
        let mut report = ExitFlushReport::default();

        if let Some((winner_ref, provenance, scorecard)) = selected_winner
            && self.get_workspace(winner_ref).is_some()
        {
            if let Some(evidence) =
                self.flush_unapplied_to_evidence(winner_ref, service, provenance, scorecard, actor)?
            {
                report.flushed_workspaces.push(evidence);
            } else {
                report.discarded_workspaces.push(winner_ref.clone());
            }
        }

        let remaining: Vec<WorkspaceRef> = {
            let guard = self.state.lock().unwrap();
            guard.workspaces.keys().cloned().collect()
        };
        for ws_ref in remaining {
            self.discard_workspace(&ws_ref, service, actor)?;
            report.discarded_workspaces.push(ws_ref);
        }

        Ok(report)
    }
}

#[derive(Serialize)]
struct EvidenceSummaryDoc {
    workspace_ref: WorkspaceRef,
    backing_kind: WorkspaceBacking,
    patch_file_name: String,
    diff_summary: DiffSummary,
    provenance: IntentProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scorecard_summary: Option<String>,
    flushed_at_ms: u128,
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T, label: &str) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(value).with_context(|| format!("serialize {label}"))?;
    write_bytes_atomic(path, &encoded, label)
}

fn write_bytes_atomic(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let tmp = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("cow-file"),
        std::process::id()
    ));
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)
            .with_context(|| format!("write temp {label} {}", tmp.display()))?;
        file.write_all(bytes)?;
        file.flush()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("replace {label} {}", path.display()))?;
    Ok(())
}
