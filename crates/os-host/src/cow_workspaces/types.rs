//! Public types and errors for Host-owned Copy-on-Write workspaces.
//!
//! All types exposed outside `alan-os-host` remain strictly free of raw Host
//! filesystem paths (ADR-0050).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::paths::{SystemStorePaths, validate_store_segment};

/// Default per-instance active CoW workspace cap.
pub const DEFAULT_MAX_ACTIVE_COW_WORKSPACES: usize = 4;

/// Default excluded directory/entry names when capturing manifests, syncing back,
/// or exporting unified patches and Git branches.
pub const DEFAULT_COW_EXCLUDES: &[&str] = &[".git", "target", "node_modules", ".venv"];

/// Opaque reference to a Host-owned CoW workspace clone.
///
/// Never encodes or reveals raw Host staging paths.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceRef(String);

impl WorkspaceRef {
    /// Construct and validate an opaque workspace reference.
    pub fn parse(raw: impl Into<String>) -> std::result::Result<Self, CowWorkspaceError> {
        let value = raw.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(CowWorkspaceError::InvalidInput(format!(
                "invalid workspace_ref `{value}`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkspaceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Physical backing strategy used for a CoW workspace clone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBacking {
    /// Native OS CoW clone inside `System Store/workspaces/` (`clonefile` on APFS, `FICLONE` on Linux).
    NativeCow,
    /// Deterministic `git worktree` fallback when cross-volume (`EXDEV`) or non-CoW filesystem.
    GitWorktree,
    /// Native OS CoW clone staged on the project's own volume when explicitly permitted.
    SiblingVolumeCow,
}

impl WorkspaceBacking {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NativeCow => "native_cow",
            Self::GitWorktree => "git_worktree",
            Self::SiblingVolumeCow => "sibling_volume_cow",
        }
    }
}

impl fmt::Display for WorkspaceBacking {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Policy controlling CoW clone creation and fallback behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloneFallbackPolicy {
    /// Prefer native OS CoW; fall back to `git worktree` (or sibling CoW if enabled) on `EXDEV`/unsupported FS.
    #[default]
    Auto,
    /// Require native OS CoW inside `System Store/workspaces/` without fallback.
    RequireNativeCow,
    /// Force cross-volume fallback behavior (for deterministic verification of `EXDEV`/non-CoW paths).
    ForceCrossVolumeFallback,
}

/// Configuration for [`crate::cow_workspaces::CowWorkspaceManager`].
#[derive(Clone, Debug)]
pub struct CowWorkspaceConfig {
    pub system_store: SystemStorePaths,
    pub instance_id: String,
    pub max_active_cow_workspaces: usize,
    pub default_excludes: Vec<String>,
    pub allow_sibling_project_volume_staging: bool,
    pub max_base_blob_bytes_per_file: u64,
    pub max_total_base_blobs_bytes: u64,
}

impl CowWorkspaceConfig {
    /// Create a default configuration for the given `SystemStorePaths` and instance ID.
    pub fn new(
        system_store: SystemStorePaths,
        instance_id: impl Into<String>,
    ) -> anyhow::Result<Self> {
        let instance_id = instance_id.into();
        validate_store_segment("workspace instance id", &instance_id)?;
        Ok(Self {
            system_store,
            instance_id,
            max_active_cow_workspaces: DEFAULT_MAX_ACTIVE_COW_WORKSPACES,
            default_excludes: DEFAULT_COW_EXCLUDES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            allow_sibling_project_volume_staging: false,
            max_base_blob_bytes_per_file: 4 * 1024 * 1024,
            max_total_base_blobs_bytes: 128 * 1024 * 1024,
        })
    }
}

/// Options when creating a new isolated CoW workspace.
#[derive(Clone, Debug, Default)]
pub struct CreateWorkspaceOptions {
    pub extra_excludes: Vec<String>,
    pub fallback_policy: CloneFallbackPolicy,
}

/// Opaque summary of a CoW workspace safe to expose to Ring 3/4 callers (`ADR-0050`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CowWorkspaceSummary {
    pub workspace_ref: WorkspaceRef,
    pub backing_kind: WorkspaceBacking,
    pub source_is_git: bool,
    pub created_at_ms: u128,
    pub applied_to_host: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_grant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_namespace_path: Option<String>,
}

/// Lightweight local intent provenance attached to sync-back records, patches, and Git commits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentProvenance {
    pub workstream: String,
    pub rollout_ref: String,
    pub why: String,
}

impl IntentProvenance {
    /// Validate and construct an [`IntentProvenance`].
    pub fn new(
        workstream: impl Into<String>,
        rollout_ref: impl Into<String>,
        why: impl Into<String>,
    ) -> std::result::Result<Self, CowWorkspaceError> {
        let workstream = workstream.into().trim().to_string();
        let rollout_ref = rollout_ref.into().trim().to_string();
        let why = why.into().trim().to_string();
        if workstream.is_empty()
            || workstream.contains('\n')
            || workstream.contains('\r')
            || rollout_ref.is_empty()
            || rollout_ref.contains('\n')
            || rollout_ref.contains('\r')
            || why.is_empty()
            || why.contains('\n')
            || why.contains('\r')
        {
            return Err(CowWorkspaceError::InvalidInput(
                "IntentProvenance requires non-empty single-line workstream, rollout_ref, and why"
                    .to_string(),
            ));
        }
        Ok(Self {
            workstream,
            rollout_ref,
            why,
        })
    }
}

/// Kind of change for a project-relative path inside a CoW workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
}

/// One changed relative path in a workspace diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffPathEntry {
    pub path: String,
    pub kind: FileChangeKind,
}

/// Summary of workspace changes relative to `BaseManifest` (excluding build/VCS paths).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub files_added: usize,
    pub files_modified: usize,
    pub files_deleted: usize,
    pub changed_paths: Vec<DiffPathEntry>,
}

impl DiffSummary {
    pub fn is_empty(&self) -> bool {
        self.files_added == 0 && self.files_modified == 0 && self.files_deleted == 0
    }

    pub fn short_summary(&self) -> String {
        format!(
            "+{} ~{} -{}",
            self.files_added, self.files_modified, self.files_deleted
        )
    }
}

/// Reason a 3-way sync-back failed closed on a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    ModifiedInBoth,
    CreatedInBoth,
    DeletedInWorkspaceModifiedOnHost,
    ModifiedInWorkspaceDeletedOnHost,
}

/// Typed conflict entry returned when Host and workspace diverged concurrently.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflict {
    pub path: String,
    pub reason: ConflictReason,
}

/// Result of a conflict-checked sync-back to the Host directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncBackReport {
    pub workspace_ref: WorkspaceRef,
    pub applied_paths: Vec<String>,
    pub deleted_paths: Vec<String>,
    pub audit_record_id: String,
    pub provenance: IntentProvenance,
}

/// Result of exporting a clean unified `.patch` with Intent Provenance headers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchExportReport {
    pub workspace_ref: WorkspaceRef,
    pub backing_kind: WorkspaceBacking,
    pub patch_text: String,
    pub diff_summary: DiffSummary,
    pub provenance: IntentProvenance,
}

/// Result of materializing workspace changes onto a fresh Git branch in the Host repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBranchExportReport {
    pub workspace_ref: WorkspaceRef,
    pub branch_name: String,
    pub commit_oid: String,
    pub diff_summary: DiffSummary,
    pub provenance: IntentProvenance,
}

/// Evidence record written when flushing an unapplied winning workspace on normal exit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlushedWorkspaceEvidence {
    pub workspace_ref: WorkspaceRef,
    pub backing_kind: WorkspaceBacking,
    pub patch_file_name: String,
    pub summary_file_name: String,
    pub diff_summary: DiffSummary,
    pub provenance: IntentProvenance,
}

/// Summary of exit cleanup across active CoW workspaces.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitFlushReport {
    pub flushed_workspaces: Vec<FlushedWorkspaceEvidence>,
    pub discarded_workspaces: Vec<WorkspaceRef>,
}

/// Summary of startup dead-PID orphan garbage collection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcReport {
    pub swept_instances: Vec<String>,
    pub swept_workspaces: usize,
    pub skipped_live_instances: Vec<String>,
}

/// Typed error surface for CoW workspace lifecycle and sync-back operations.
#[derive(Debug)]
pub enum CowWorkspaceError {
    ActiveCapacityReached { active: usize, max: usize },
    UnknownWorkspace(String),
    CrossVolumeNonGitUnsupported,
    NativeCowRequiredButUnavailable(String),
    UnbornGitHead,
    NotAGitRepository,
    BranchAlreadyExists(String),
    NoChangesToExport,
    SyncConflict { conflicts: Vec<SyncConflict> },
    InvalidInput(String),
    Io(std::io::Error),
    Other(anyhow::Error),
}

impl fmt::Display for CowWorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ActiveCapacityReached { active, max } => {
                write!(f, "active CoW workspace limit reached ({active}/{max})")
            }
            Self::UnknownWorkspace(id) => write!(f, "unknown CoW workspace `{id}`"),
            Self::CrossVolumeNonGitUnsupported => write!(
                f,
                "native CoW clone is unavailable across volumes or on a non-CoW filesystem for non-Git directory"
            ),
            Self::NativeCowRequiredButUnavailable(reason) => {
                write!(f, "native CoW workspace required but unavailable: {reason}")
            }
            Self::UnbornGitHead => {
                write!(
                    f,
                    "cannot fall back to `git worktree` on a repository with an unborn HEAD"
                )
            }
            Self::NotAGitRepository => write!(f, "target directory is not a Git repository"),
            Self::BranchAlreadyExists(branch) => write!(f, "Git branch `{branch}` already exists"),
            Self::NoChangesToExport => write!(f, "workspace has no source changes to export"),
            Self::SyncConflict { conflicts } => write!(
                f,
                "sync-back aborted due to {} concurrent Host conflict(s)",
                conflicts.len()
            ),
            Self::InvalidInput(msg) => write!(f, "invalid CoW workspace input: {msg}"),
            Self::Io(err) => err.fmt(f),
            Self::Other(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for CowWorkspaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CowWorkspaceError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<anyhow::Error> for CowWorkspaceError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err)
    }
}
