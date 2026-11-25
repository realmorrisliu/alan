//! Tiered Host Mount write isolation: `read_only` concurrency, per-grant FIFO
//! `shared_queue` serialization, and explicit `cow_workspace` / `worktree` isolation port.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use alan_agent_protocol::{SpawnHostMount, SpawnMountAccess};
use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::project_workstreams::WorkstreamWriteMode;

/// Backing mechanism used for an isolated write workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolatedWorkspaceBacking {
    /// Native OS copy-on-write clone (APFS `clonefile` / Linux `FICLONE`).
    OsCowClone,
    /// Isolated `git worktree` fallback when native CoW cloning is unavailable.
    GitWorktree,
}

/// Request sent by Root to allocate an isolated writable workspace for a child workstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolatedWorkspaceRequest {
    pub workstream_name: String,
    pub source_grant_id: String,
    pub source_namespace_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_ref: Option<String>,
    pub why: String,
}

/// Opaque isolated workspace allocation returned by [`WorkspaceIsolationPort`].
///
/// Never exposes raw Host filesystem staging paths to Ring 3/4 callers (`ADR-0050`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolatedWorkspaceAllocation {
    pub workspace_ref: String,
    pub backing: IsolatedWorkspaceBacking,
    /// Narrowed `ReadWrite` Host Mount grant covering only the isolated workspace.
    pub narrowed_grant: SpawnHostMount,
    /// Optional Git branch name when backed by `git worktree`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_ref: Option<String>,
}

/// Sync-back outcome for an isolated write workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSyncStatus {
    /// Diff collected and awaiting explicit Root/user merge or sync-back confirmation.
    PendingMerge,
    /// Synced back cleanly into the primary Host working tree after 3-way check.
    SyncedBack,
    /// Aborted fail-closed due to concurrent Host edits detected in 3-way check.
    ConflictAborted,
}

/// Typed conflict entry reported when 3-way sync-back detects concurrent Host edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSyncConflict {
    pub path: String,
    pub reason: String,
}

/// Explicit diff and merge/sync-back handoff contract returned on isolated workstream completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMergeHandoff {
    pub workspace_ref: String,
    pub backing: IsolatedWorkspaceBacking,
    pub source_grant_id: String,
    pub workstream_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_ref: Option<String>,
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_files: Vec<String>,
    pub diff_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_evidence_ref: Option<String>,
    pub sync_status: WorkspaceSyncStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<WorkspaceSyncConflict>,
}

/// Host-agnostic port for allocating, inspecting, syncing back, and discarding isolated
/// `cow_workspace` / `worktree` clones (`Task 3.1` / `@cow-substrate` integration seam).
#[async_trait]
pub trait WorkspaceIsolationPort: Send + Sync + std::fmt::Debug {
    /// Allocate an isolated writable workspace and return its narrowed Host Mount grant.
    async fn allocate_workspace(
        &self,
        request: IsolatedWorkspaceRequest,
    ) -> Result<IsolatedWorkspaceAllocation>;

    /// Collect the non-destructive diff / `.patch` merge handoff after child execution finishes.
    async fn collect_handoff(
        &self,
        allocation: &IsolatedWorkspaceAllocation,
        workstream_name: &str,
        rollout_ref: Option<&str>,
        why: &str,
    ) -> Result<WorkspaceMergeHandoff>;

    /// Apply changes back to the primary Host directory with explicit 3-way conflict checking.
    async fn sync_back(
        &self,
        allocation: &IsolatedWorkspaceAllocation,
        workstream_name: &str,
        rollout_ref: Option<&str>,
        why: &str,
    ) -> Result<WorkspaceMergeHandoff>;

    /// Discard an isolated workspace after merge or cancellation.
    async fn discard_workspace(&self, workspace_ref: &str) -> Result<()>;
}

#[derive(Debug)]
struct FifoWaiter {
    owner: String,
    tx: oneshot::Sender<()>,
}

#[derive(Default, Debug)]
struct GrantQueueState {
    active_owner: Option<String>,
    waiters: VecDeque<FifoWaiter>,
}

/// In-instance per-grant strict FIFO queue serializing routine `shared_queue` writers.
#[derive(Clone, Default, Debug)]
pub struct SharedWriteQueueRegistry {
    queues: Arc<Mutex<HashMap<String, GrantQueueState>>>,
}

/// RAII permit holding exclusive `shared_queue` access for one grant.
#[derive(Debug)]
struct SingleGrantWritePermit {
    queues: Arc<Mutex<HashMap<String, GrantQueueState>>>,
    grant_id: String,
    released: bool,
}

impl SingleGrantWritePermit {
    fn release_now(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let mut queues = self
            .queues
            .lock()
            .expect("shared write queue mutex poisoned");
        let Some(state) = queues.get_mut(&self.grant_id) else {
            return;
        };
        while let Some(waiter) = state.waiters.pop_front() {
            let next_owner = waiter.owner.clone();
            if waiter.tx.send(()).is_ok() {
                state.active_owner = Some(next_owner);
                return;
            }
        }
        state.active_owner = None;
    }
}

impl Drop for SingleGrantWritePermit {
    fn drop(&mut self) {
        self.release_now();
    }
}

/// Composite RAII guard holding per-grant FIFO permits across all target write grants.
#[derive(Debug, Default)]
pub struct SharedWriteGuard {
    permits: Vec<SingleGrantWritePermit>,
}

impl SharedWriteGuard {
    /// Returns the number of serialized grant permits held.
    pub fn len(&self) -> usize {
        self.permits.len()
    }

    /// Returns true when no grant permits are held.
    pub fn is_empty(&self) -> bool {
        self.permits.is_empty()
    }
}

impl SharedWriteQueueRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if `grant_id` currently has an active `shared_queue` writer.
    pub fn is_grant_busy(&self, grant_id: &str) -> bool {
        let queues = self
            .queues
            .lock()
            .expect("shared write queue mutex poisoned");
        queues
            .get(grant_id)
            .is_some_and(|state| state.active_owner.is_some())
    }

    /// Returns the current active writer label for `grant_id`, if any.
    pub fn active_writer(&self, grant_id: &str) -> Option<String> {
        let queues = self
            .queues
            .lock()
            .expect("shared write queue mutex poisoned");
        queues
            .get(grant_id)
            .and_then(|state| state.active_owner.clone())
    }

    /// Acquire exclusive FIFO `shared_queue` permits for all `grant_ids` in sorted order.
    pub async fn acquire_grants_fifo(
        &self,
        grant_ids: &[String],
        owner_label: &str,
        cancel: &CancellationToken,
    ) -> Result<SharedWriteGuard> {
        let unique_sorted: BTreeSet<String> = grant_ids.iter().cloned().collect();
        let mut permits = Vec::with_capacity(unique_sorted.len());
        for grant_id in unique_sorted {
            let permit = self.acquire_single(&grant_id, owner_label, cancel).await?;
            permits.push(permit);
        }
        Ok(SharedWriteGuard { permits })
    }

    async fn acquire_single(
        &self,
        grant_id: &str,
        owner_label: &str,
        cancel: &CancellationToken,
    ) -> Result<SingleGrantWritePermit> {
        if cancel.is_cancelled() {
            bail!("Shared write queue acquisition cancelled for grant `{grant_id}`");
        }
        let maybe_rx = {
            let mut queues = self
                .queues
                .lock()
                .expect("shared write queue mutex poisoned");
            let state = queues.entry(grant_id.to_string()).or_default();
            if state.active_owner.is_none() && state.waiters.is_empty() {
                state.active_owner = Some(owner_label.to_string());
                None
            } else {
                let (tx, rx) = oneshot::channel();
                state.waiters.push_back(FifoWaiter {
                    owner: owner_label.to_string(),
                    tx,
                });
                Some(rx)
            }
        };

        let Some(mut rx) = maybe_rx else {
            return Ok(SingleGrantWritePermit {
                queues: Arc::clone(&self.queues),
                grant_id: grant_id.to_string(),
                released: false,
            });
        };

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                // If ownership was handed to us concurrently with cancellation, release it so
                // subsequent FIFO waiters are never starved.
                if rx.try_recv().is_ok() {
                    let permit = SingleGrantWritePermit {
                        queues: Arc::clone(&self.queues),
                        grant_id: grant_id.to_string(),
                        released: false,
                    };
                    drop(permit);
                }
                bail!("Shared write queue acquisition cancelled for grant `{grant_id}`");
            }
            res = &mut rx => {
                res.context("shared write queue waiter dropped unexpectedly")?;
                Ok(SingleGrantWritePermit {
                    queues: Arc::clone(&self.queues),
                    grant_id: grant_id.to_string(),
                    released: false,
                })
            }
        }
    }
}

/// Resolve the effective [`WorkstreamWriteMode`] from request flags and active grant contention.
pub(crate) fn resolve_effective_write_mode(
    read_only: bool,
    requested_mode: Option<WorkstreamWriteMode>,
    escalate_on_contention: bool,
    host_mounts: &[SpawnHostMount],
    queue: &SharedWriteQueueRegistry,
) -> std::result::Result<WorkstreamWriteMode, String> {
    let rw_grants: Vec<&str> = host_mounts
        .iter()
        .filter(|m| m.access == SpawnMountAccess::ReadWrite)
        .map(|m| m.grant.as_str())
        .collect();

    if read_only {
        if matches!(
            requested_mode,
            Some(WorkstreamWriteMode::SharedQueue | WorkstreamWriteMode::Worktree)
        ) {
            return Err(
                "Read-only subagent cannot request `shared_queue` or `worktree` write_mode."
                    .to_string(),
            );
        }
        return Ok(WorkstreamWriteMode::ReadOnly);
    }

    match requested_mode {
        Some(WorkstreamWriteMode::ReadOnly) => {
            if !rw_grants.is_empty() {
                return Err(
                    "Workstream with `write_mode = \"read_only\"` cannot request `read_write` host_mounts."
                        .to_string(),
                );
            }
            Ok(WorkstreamWriteMode::ReadOnly)
        }
        Some(WorkstreamWriteMode::Worktree) => {
            if rw_grants.is_empty() {
                return Err(
                    "Workstream with `write_mode = \"worktree\"` requires at least one `read_write` host_mount."
                        .to_string(),
                );
            }
            Ok(WorkstreamWriteMode::Worktree)
        }
        Some(WorkstreamWriteMode::SharedQueue) => {
            if rw_grants.is_empty() {
                Ok(WorkstreamWriteMode::ReadOnly)
            } else if escalate_on_contention
                && rw_grants.iter().any(|grant| queue.is_grant_busy(grant))
            {
                Ok(WorkstreamWriteMode::Worktree)
            } else {
                Ok(WorkstreamWriteMode::SharedQueue)
            }
        }
        None => {
            if rw_grants.is_empty() {
                Ok(WorkstreamWriteMode::ReadOnly)
            } else if escalate_on_contention
                && rw_grants.iter().any(|grant| queue.is_grant_busy(grant))
            {
                Ok(WorkstreamWriteMode::Worktree)
            } else {
                Ok(WorkstreamWriteMode::SharedQueue)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct RegisteredGitGrant {
    host_repo_path: PathBuf,
    namespace_path: PathBuf,
}

#[derive(Debug, Clone)]
struct GitWorktreeRecord {
    source_grant_id: String,
    host_repo_path: PathBuf,
    worktree_path: PathBuf,
    branch_ref: String,
    base_commit: String,
}

/// Real `git worktree` fallback implementation of [`WorkspaceIsolationPort`] when native OS CoW
/// cloning is unavailable or in host-agnostic engine testing.
#[derive(Debug)]
pub struct GitWorktreeIsolationPort {
    staging_root: PathBuf,
    evidence_root: PathBuf,
    grants: Mutex<HashMap<String, RegisteredGitGrant>>,
    active: Mutex<HashMap<String, GitWorktreeRecord>>,
}

impl GitWorktreeIsolationPort {
    /// Create a `git worktree` isolation port storing clones under `staging_root` and `.patch`
    /// handoffs under `evidence_root`.
    pub fn new(staging_root: impl Into<PathBuf>, evidence_root: impl Into<PathBuf>) -> Self {
        Self {
            staging_root: staging_root.into(),
            evidence_root: evidence_root.into(),
            grants: Mutex::new(HashMap::new()),
            active: Mutex::new(HashMap::new()),
        }
    }

    /// Register an authorized Host Git repository backing a logical `grant_id`.
    pub fn register_grant(
        &self,
        grant_id: impl Into<String>,
        host_repo_path: impl Into<PathBuf>,
        namespace_path: impl Into<PathBuf>,
    ) {
        self.grants
            .lock()
            .expect("git worktree grants mutex poisoned")
            .insert(
                grant_id.into(),
                RegisteredGitGrant {
                    host_repo_path: host_repo_path.into(),
                    namespace_path: namespace_path.into(),
                },
            );
    }

    /// Resolve the Host path for an opaque `workspace_ref` (Host-side inspection/test helper only).
    pub fn host_worktree_path(&self, workspace_ref: &str) -> Option<PathBuf> {
        self.active
            .lock()
            .expect("git worktree active mutex poisoned")
            .get(workspace_ref)
            .map(|rec| rec.worktree_path.clone())
    }
}

#[async_trait]
impl WorkspaceIsolationPort for GitWorktreeIsolationPort {
    async fn allocate_workspace(
        &self,
        request: IsolatedWorkspaceRequest,
    ) -> Result<IsolatedWorkspaceAllocation> {
        let grant = {
            let grants = self
                .grants
                .lock()
                .expect("git worktree grants mutex poisoned");
            grants.get(&request.source_grant_id).cloned()
        }
        .with_context(|| {
            format!(
                "source grant `{}` is not registered for git worktree isolation",
                request.source_grant_id
            )
        });
        let grant = grant?;
        ensure!(
            request
                .source_namespace_path
                .starts_with(&grant.namespace_path),
            "requested namespace `{}` escapes registered grant `{}` scope `{}`",
            request.source_namespace_path.display(),
            request.source_grant_id,
            grant.namespace_path.display()
        );

        let inside = run_git(
            &grant.host_repo_path,
            &["rev-parse", "--is-inside-work-tree"],
        )?;
        ensure!(
            inside.trim() == "true",
            "host grant `{}` target is not a Git repository",
            request.source_grant_id
        );
        let base_commit = run_git(&grant.host_repo_path, &["rev-parse", "HEAD"])?
            .trim()
            .to_string();

        fs::create_dir_all(&self.staging_root)?;
        let short_id = &uuid::Uuid::new_v4().simple().to_string()[..12];
        let workspace_ref = format!("wt-{short_id}");
        let slug = sanitize_branch_slug(&request.workstream_name);
        let branch_ref = format!("alan-ws/{slug}-{short_id}");
        let worktree_path = self.staging_root.join(&workspace_ref);

        let wt_str = worktree_path
            .to_str()
            .context("worktree path is not valid UTF-8")?;
        run_git(
            &grant.host_repo_path,
            &["worktree", "add", "-b", &branch_ref, wt_str, &base_commit],
        )?;

        self.active
            .lock()
            .expect("git worktree active mutex poisoned")
            .insert(
                workspace_ref.clone(),
                GitWorktreeRecord {
                    source_grant_id: request.source_grant_id,
                    host_repo_path: grant.host_repo_path,
                    worktree_path,
                    branch_ref: branch_ref.clone(),
                    base_commit,
                },
            );

        Ok(IsolatedWorkspaceAllocation {
            workspace_ref: workspace_ref.clone(),
            backing: IsolatedWorkspaceBacking::GitWorktree,
            narrowed_grant: SpawnHostMount {
                grant: workspace_ref.clone(),
                target: PathBuf::from(format!("/mnt/worktrees/{workspace_ref}")),
                access: SpawnMountAccess::ReadWrite,
            },
            branch_ref: Some(branch_ref),
        })
    }

    async fn collect_handoff(
        &self,
        allocation: &IsolatedWorkspaceAllocation,
        workstream_name: &str,
        rollout_ref: Option<&str>,
        why: &str,
    ) -> Result<WorkspaceMergeHandoff> {
        let rec = self
            .active
            .lock()
            .expect("git worktree active mutex poisoned")
            .get(&allocation.workspace_ref)
            .cloned()
            .with_context(|| format!("unknown workspace_ref `{}`", allocation.workspace_ref))?;

        let (changed_files, patch_body) =
            collect_git_worktree_changes(&rec.worktree_path, &rec.base_commit)?;
        let patch_evidence_ref = if changed_files.is_empty() {
            None
        } else {
            fs::create_dir_all(&self.evidence_root)?;
            let patch_path = self
                .evidence_root
                .join(format!("{}.patch", allocation.workspace_ref));
            let header = format!(
                "# Alan-Workstream: {workstream_name}\n# Alan-Rollout: {}\n# Alan-Why: {why}\n\n",
                rollout_ref.unwrap_or("none")
            );
            fs::write(&patch_path, format!("{header}{patch_body}"))?;
            Some(format!(
                "evidence://patches/{}.patch",
                allocation.workspace_ref
            ))
        };
        let diff_summary = if changed_files.is_empty() {
            "No workspace changes produced.".to_string()
        } else {
            format!(
                "{} file(s) changed in isolated {:?} workspace ({})",
                changed_files.len(),
                allocation.backing,
                changed_files.join(", ")
            )
        };

        Ok(WorkspaceMergeHandoff {
            workspace_ref: allocation.workspace_ref.clone(),
            backing: allocation.backing,
            source_grant_id: rec.source_grant_id,
            workstream_name: workstream_name.to_string(),
            rollout_ref: rollout_ref.map(str::to_string),
            why: why.to_string(),
            branch_ref: Some(rec.branch_ref),
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
        let rec = self
            .active
            .lock()
            .expect("git worktree active mutex poisoned")
            .get(&allocation.workspace_ref)
            .cloned()
            .with_context(|| format!("unknown workspace_ref `{}`", allocation.workspace_ref))?;

        if handoff.changed_files.is_empty() {
            handoff.sync_status = WorkspaceSyncStatus::SyncedBack;
            return Ok(handoff);
        }

        // Explicit 3-way conflict check: fail closed if any changed file was edited on Host
        // since `base_commit`.
        let mut conflicts = Vec::new();
        for rel in &handoff.changed_files {
            if is_host_file_modified_since(&rec.host_repo_path, &rec.base_commit, rel)? {
                conflicts.push(WorkspaceSyncConflict {
                    path: rel.clone(),
                    reason: "Host file changed concurrently since workspace clone".to_string(),
                });
            }
        }
        if !conflicts.is_empty() {
            handoff.sync_status = WorkspaceSyncStatus::ConflictAborted;
            handoff.conflicts = conflicts;
            handoff.diff_summary = format!(
                "Sync-back aborted fail-closed due to {} concurrent Host conflict(s); patch preserved.",
                handoff.conflicts.len()
            );
            return Ok(handoff);
        }

        for rel in &handoff.changed_files {
            let src = rec.worktree_path.join(rel);
            let dst = rec.host_repo_path.join(rel);
            if src.exists() {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&src, &dst).with_context(|| format!("sync back `{rel}` to Host"))?;
            } else if dst.exists() {
                fs::remove_file(&dst).with_context(|| format!("remove deleted `{rel}` on Host"))?;
            }
        }
        handoff.sync_status = WorkspaceSyncStatus::SyncedBack;
        Ok(handoff)
    }

    async fn discard_workspace(&self, workspace_ref: &str) -> Result<()> {
        let rec = self
            .active
            .lock()
            .expect("git worktree active mutex poisoned")
            .remove(workspace_ref);
        let Some(rec) = rec else {
            return Ok(());
        };
        if let Some(wt_str) = rec.worktree_path.to_str() {
            let _ = run_git(
                &rec.host_repo_path,
                &["worktree", "remove", "--force", wt_str],
            );
        }
        let _ = run_git(&rec.host_repo_path, &["branch", "-D", &rec.branch_ref]);
        if rec.worktree_path.exists() {
            let _ = fs::remove_dir_all(&rec.worktree_path);
        }
        Ok(())
    }
}

fn sanitize_branch_slug(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 32 {
            break;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "workstream".to_string()
    } else {
        trimmed.to_string()
    }
}

fn run_git(repo_dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(args)
        .output()
        .with_context(|| format!("run git {:?} in {}", args, repo_dir.display()))?;
    ensure!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn collect_git_worktree_changes(
    worktree_path: &Path,
    base_commit: &str,
) -> Result<(Vec<String>, String)> {
    let mut changed = BTreeSet::new();
    let tracked = run_git(worktree_path, &["diff", "--name-only", base_commit])?;
    for line in tracked.lines().map(str::trim).filter(|l| !l.is_empty()) {
        changed.insert(line.to_string());
    }
    let untracked = run_git(
        worktree_path,
        &["ls-files", "--others", "--exclude-standard"],
    )?;
    for line in untracked.lines().map(str::trim).filter(|l| !l.is_empty()) {
        changed.insert(line.to_string());
    }
    let mut patch = run_git(worktree_path, &["diff", base_commit])?;
    for rel in untracked.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Ok(content) = fs::read_to_string(worktree_path.join(rel)) {
            patch.push_str(&format!(
                "diff --git a/{rel} b/{rel}\nnew file mode 100644\n--- /dev/null\n+++ b/{rel}\n"
            ));
            for line in content.lines() {
                patch.push('+');
                patch.push_str(line);
                patch.push('\n');
            }
        }
    }
    Ok((changed.into_iter().collect(), patch))
}

fn is_host_file_modified_since(
    host_repo_path: &Path,
    base_commit: &str,
    rel_path: &str,
) -> Result<bool> {
    let head_diff = run_git(
        host_repo_path,
        &["diff", "--name-only", base_commit, "--", rel_path],
    )?;
    if !head_diff.trim().is_empty() {
        return Ok(true);
    }
    let status = run_git(host_repo_path, &["status", "--porcelain", "--", rel_path])?;
    Ok(!status.trim().is_empty())
}

#[cfg(test)]
#[path = "write_isolation_tests.rs"]
mod tests;
