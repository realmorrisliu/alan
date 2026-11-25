//! Host-agnostic CoW workspace coordination port and ADR-0050 path-isolation guards.
//!
//! `alan-branchfs` sits at Ring 3/4 and MUST NEVER store or emit raw Host
//! filesystem paths (`ADR-0050`). Callers above `alan-os-host` interact only
//! through opaque `workspace_ref` handles, [`WorkspaceBackingKind`], and
//! bounded summary strings.

use std::fmt;

use alan_ap::ErrorCode;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

const MAX_WORKSPACE_REF_LEN: usize = 128;
const MAX_SUMMARY_BYTES: usize = 64 * 1024;

const ALLOWED_NAMESPACE_PREFIXES: &[&str] = &[
    "/mnt/",
    "/agent/",
    "/proc/",
    "/srv/",
    "/bin/",
    "/lib/",
    "/man/",
    "/dev/null",
];

/// Physical backing mechanism for a CoW workspace candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBackingKind {
    /// Native OS copy-on-write clone (macOS APFS `clonefile` or Linux `FICLONE`).
    NativeCow,
    /// Deterministic `git worktree` fallback when native CoW is unavailable.
    GitWorktree,
}

/// Request sent to the host workspace port when `ctl` forks a branch without an
/// already-provisioned `workspace_ref`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchWorkspaceCreateRequest {
    pub branch_id: String,
    pub from_branch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_workspace_ref: Option<String>,
}

/// Opaque metadata returned by the host workspace port after provisioning a CoW clone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchWorkspaceProvisioned {
    pub workspace_ref: String,
    pub backing_kind: WorkspaceBackingKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<String>,
}

/// Request to immediately delete a losing or discarded candidate's physical clone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchWorkspaceDiscardRequest {
    pub branch_id: String,
    pub workspace_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_kind: Option<WorkspaceBackingKind>,
}

/// Request to persist unified `.patch` + scorecard evidence for an unapplied winner
/// on exit before removing its physical CoW clone directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchWorkspaceExitFlushRequest {
    pub branch_id: String,
    pub workspace_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_kind: Option<WorkspaceBackingKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<String>,
}

/// Opaque outcome returned after persisting exit `.patch` evidence and purging the clone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchWorkspaceExitFlushOutcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_artifact_ref: Option<String>,
    pub clone_purged: bool,
}

/// Category of failure returned by a [`BranchWorkspacePort`] implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchWorkspacePortErrorKind {
    Rejected,
    NotFound,
    Io,
}

/// Error returned by [`BranchWorkspacePort`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchWorkspacePortError {
    pub kind: BranchWorkspacePortErrorKind,
    pub message: String,
}

impl BranchWorkspacePortError {
    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            kind: BranchWorkspacePortErrorKind::Rejected,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: BranchWorkspacePortErrorKind::NotFound,
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self {
            kind: BranchWorkspacePortErrorKind::Io,
            message: message.into(),
        }
    }

    pub(crate) fn to_error_code(&self) -> ErrorCode {
        match self.kind {
            BranchWorkspacePortErrorKind::Rejected => ErrorCode::BadRequest,
            BranchWorkspacePortErrorKind::NotFound => ErrorCode::NotFound,
            BranchWorkspacePortErrorKind::Io => ErrorCode::Io,
        }
    }
}

impl fmt::Display for BranchWorkspacePortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BranchWorkspacePortError {}

/// Host-agnostic port connecting `/mnt/branch` lifecycle commands to physical CoW
/// workspace creation, immediate losing-clone purge, and exit `.patch` persistence.
///
/// Implementations (such as the `alan-os-host` adapter) MUST keep all real Host
/// filesystem paths private and return strictly opaque `workspace_ref` handles.
#[async_trait]
pub trait BranchWorkspacePort: Send + Sync {
    /// Provision an isolated CoW workspace for a forked candidate branch.
    async fn create_workspace(
        &self,
        request: &BranchWorkspaceCreateRequest,
    ) -> Result<BranchWorkspaceProvisioned, BranchWorkspacePortError>;

    /// Immediately purge the physical CoW clone directory for a discarded branch.
    async fn discard_workspace(
        &self,
        request: &BranchWorkspaceDiscardRequest,
    ) -> Result<(), BranchWorkspacePortError>;

    /// Persist unified `.patch` + scorecard evidence for an unapplied selected winner
    /// and delete its physical CoW clone directory on exit.
    async fn flush_selected_on_exit(
        &self,
        request: &BranchWorkspaceExitFlushRequest,
    ) -> Result<BranchWorkspaceExitFlushOutcome, BranchWorkspacePortError>;
}

/// Validate an opaque `workspace_ref` handle (never a filesystem path).
pub fn validate_opaque_workspace_ref(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty()
        || value.len() > MAX_WORKSPACE_REF_LEN
        || value.contains("..")
        || !value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(ErrorCode::BadRequest);
    }
    Ok(())
}

/// Validate that a Ring 3/4 `branchfs` text field contains no raw Host filesystem paths.
pub fn validate_no_raw_host_path(value: &str) -> Result<(), ErrorCode> {
    if value.len() > MAX_SUMMARY_BYTES || contains_raw_host_path(value) {
        return Err(ErrorCode::BadRequest);
    }
    Ok(())
}

/// Returns `true` when `value` appears to embed a raw Host filesystem path, home-relative
/// path, `file://` URI, or Host Store / System Store staging directory reference.
pub fn contains_raw_host_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("file://")
        || lower.contains("system store/workspaces")
        || lower.contains("system store\\workspaces")
        || lower.contains("host store/")
        || lower.contains("host store\\")
        || value.contains("\\\\")
    {
        return true;
    }

    let bytes = value.as_bytes();
    for (idx, &byte) in bytes.iter().enumerate() {
        let at_token_boundary = idx == 0
            || matches!(
                bytes[idx - 1],
                b' ' | b'\t'
                    | b'\n'
                    | b'\r'
                    | b'"'
                    | b'\''
                    | b'`'
                    | b'('
                    | b'['
                    | b'{'
                    | b'='
                    | b':'
                    | b','
            );

        // Windows drive letter path: `C:\...` or `C:/...`
        if at_token_boundary
            && byte.is_ascii_alphabetic()
            && idx + 2 < bytes.len()
            && bytes[idx + 1] == b':'
            && matches!(bytes[idx + 2], b'\\' | b'/')
        {
            return true;
        }

        // Home-relative path: `~/...` or `~\...`
        if at_token_boundary
            && byte == b'~'
            && idx + 1 < bytes.len()
            && matches!(bytes[idx + 1], b'/' | b'\\')
        {
            return true;
        }

        // Absolute Unix filesystem path outside Alan's virtual namespace roots.
        if at_token_boundary
            && byte == b'/'
            && idx + 1 < bytes.len()
            && (bytes[idx + 1].is_ascii_alphabetic() || matches!(bytes[idx + 1], b'_' | b'.'))
        {
            let rest = &value[idx..];
            let allowed = ALLOWED_NAMESPACE_PREFIXES
                .iter()
                .any(|prefix| rest.starts_with(prefix))
                || matches!(
                    rest.split([' ', '\t', '\n', '\r', '"', '\'', '`', ')', ']', '}', ','])
                        .next(),
                    Some("/mnt" | "/agent" | "/proc" | "/srv" | "/bin" | "/lib" | "/man")
                );
            if !allowed {
                return true;
            }
        }
    }

    false
}
