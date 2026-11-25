//! Executable `alan-arena` runtime orchestration, fail-closed OS sandbox gate,
//! host-agnostic CoW workspace port, and exit `.patch` flush lifecycle.
//!
//! Default `alan-arena` remains strictly [`ArenaMode::ReadOnly`]. Opting into
//! [`ArenaMode::Executable`] requires an OS-enforced sandbox backend
//! (`Seatbelt`, `LinuxReifiedNamespace`, or `Landlock`), provisions one
//! isolated CoW workspace per contender, confines contenders to a single
//! narrowed `ReadWrite` mount with default-denied network and zero secrets,
//! adjudicates via a `read_only = true` Verifier, immediately purges losing
//! clones, and flushes unapplied winning `.patch` evidence on exit.

mod branch_client;
mod orchestrator;
mod specs;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use alan_agent_protocol::SpawnHostMount;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::tools::{SandboxBackendKind, detect_backend};

pub use branch_client::{
    BranchForkCommand, BranchFsApClient, BranchRecordView, BranchScoreCommand,
};
pub use orchestrator::{
    ArenaBakeoffResult, ArenaCandidateOutcome, ArenaScoreEntry, ArenaSpawnFn, ArenaSubagentOutput,
    ArenaVerifierScorecard, ExecutableArenaCoordinator, ExecutedContenderOutput, ParsedScorecard,
    parse_verifier_scorecard,
};
pub use specs::{
    ArenaContenderRequest, ArenaVerifierRequest, ExecutableArenaRequest, VerifierCandidateSummary,
    build_executable_contender_spawn_spec, build_read_only_contender_spawn_spec,
    build_verifier_spawn_spec,
};

/// Hard cap on parallel executable arena contenders (matching `max_active_cow_workspaces = 4`).
pub const DEFAULT_MAX_EXECUTABLE_ARENA_CONTENDERS: usize = 4;
/// Canonical namespace mount target projected for a single contender CoW workspace.
pub const DEFAULT_ARENA_CONTENDER_MOUNT_TARGET: &str = "/mnt/work";
/// Conventional namespace mount path for `alan-branchfs`.
pub const BRANCH_MOUNT_PATH: &str = "/mnt/branch";
/// Default bounded timeout for contender and verifier subagents in seconds.
pub const DEFAULT_ARENA_SUBAGENT_TIMEOUT_SECS: u64 = 900;

/// Execution posture for `alan-arena` (`read_only` remains the default).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaMode {
    /// Default strictly read-only architectural bakeoff.
    #[default]
    ReadOnly,
    /// Opt-in CoW workspace bakeoff behind a fail-closed OS sandbox gate.
    Executable,
}

impl ArenaMode {
    /// Stable wire string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Executable => "executable",
        }
    }

    /// Parse an optional `mode` argument (`None` defaults to `ReadOnly`).
    pub fn from_optional_str(value: Option<&str>) -> Result<Self, ExecutableArenaError> {
        match value.map(str::trim) {
            None | Some("") | Some("read_only") => Ok(Self::ReadOnly),
            Some("executable") => Ok(Self::Executable),
            Some(other) => Err(ExecutableArenaError::InvalidConfiguration(format!(
                "unsupported alan-arena mode `{other}`; expected `read_only` (default) or `executable`"
            ))),
        }
    }
}

/// Physical backing mechanism reported by the host CoW workspace substrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArenaWorkspaceBackingKind {
    /// Native OS copy-on-write clone (`clonefile` on macOS APFS, `FICLONE` on Linux).
    NativeCow,
    /// Deterministic `git worktree` fallback when native CoW is unavailable.
    GitWorktree,
}

impl ArenaWorkspaceBackingKind {
    /// Wire label matching `alan-branchfs` `WorkspaceBackingKind`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NativeCow => "native_cow",
            Self::GitWorktree => "git_worktree",
        }
    }
}

/// Typed failure modes for executable `alan-arena` orchestration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecutableArenaError {
    #[error(
        "executable alan-arena refused under `{backend}`: an OS-enforced sandbox \
         (seatbelt, linux_reified_namespace, or landlock) is required; fall back to \
         standard `read_only` alan-arena comparison"
    )]
    UnsafeSandboxBackend { backend: &'static str },

    #[error("invalid executable alan-arena configuration: {0}")]
    InvalidConfiguration(String),

    #[error("raw Host filesystem path rejected at arena boundary: {0}")]
    RawHostPathRejected(String),

    #[error("arena verifier must remain read_only = true without read_write mounts: {0}")]
    VerifierNotReadOnly(String),

    #[error("contender workspace mount isolation violated: {0}")]
    ContenderMountViolation(String),

    #[error("CoW workspace port failure: {0}")]
    WorkspacePort(String),

    #[error("branchfs coordination failure: {0}")]
    BranchFsProtocol(String),

    #[error("arena execution failed: {0}")]
    ExecutionFailed(String),
}

/// Fail-closed OS sandbox gate for `mode = "executable"`.
///
/// Only `Seatbelt`, `LinuxReifiedNamespace`, and `Landlock` are permitted.
/// `HostMountPathGuard` is unconditionally rejected before any CoW workspace is created.
pub fn ensure_executable_arena_sandbox(
    backend: SandboxBackendKind,
) -> Result<(), ExecutableArenaError> {
    match backend {
        SandboxBackendKind::Seatbelt
        | SandboxBackendKind::LinuxReifiedNamespace
        | SandboxBackendKind::Landlock => Ok(()),
        SandboxBackendKind::HostMountPathGuard => Err(ExecutableArenaError::UnsafeSandboxBackend {
            backend: backend.name(),
        }),
    }
}

/// Probe the active host sandbox backend and enforce the fail-closed OS confinement gate.
pub fn ensure_active_executable_arena_sandbox() -> Result<SandboxBackendKind, ExecutableArenaError>
{
    let backend = detect_backend();
    ensure_executable_arena_sandbox(backend)?;
    Ok(backend)
}

/// Request sent across [`ArenaWorkspacePort`] to create one candidate's CoW clone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaWorkspaceCreateRequest {
    pub candidate_id: String,
    pub source_grant: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// Opaque candidate workspace handle and narrowed mount descriptors returned by the host port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaContenderWorkspace {
    /// Opaque workspace reference (never a raw Host path).
    pub workspace_ref: String,
    /// Physical CoW or worktree backing kind.
    pub backing_kind: ArenaWorkspaceBackingKind,
    /// Narrowed `ReadWrite` mount covering strictly this contender's clone.
    pub contender_mount: SpawnHostMount,
    /// Optional `ReadOnly` mount allowing the independent Verifier to inspect this clone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_read_only_mount: Option<SpawnHostMount>,
}

/// Diff and check summary collected from a candidate workspace without raw Host paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaWorkspaceInspection {
    pub diff_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<String>,
    pub has_changes: bool,
}

/// Request to persist unified `.patch` + scorecard evidence for an unapplied winning
/// workspace into System Store rollout evidence before purging the clone directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArenaWinnerFlushRequest {
    pub candidate_id: String,
    pub workspace_ref: String,
    pub backing_kind: ArenaWorkspaceBackingKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<String>,
}

/// Outcome returned by [`ArenaWorkspacePort::flush_winner_patch_on_exit`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaWinnerFlushOutcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_artifact_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scorecard_artifact_ref: Option<String>,
    pub clone_purged: bool,
}

/// Host-agnostic port connecting `executable_arena` to `alan-os-host`'s CoW substrate.
#[async_trait]
pub trait ArenaWorkspacePort: Send + Sync {
    /// Provision an isolated CoW clone for a contender and return its opaque ref +
    /// narrowed single-clone `ReadWrite` mount (plus optional `ReadOnly` verifier mount).
    async fn create_contender_workspace(
        &self,
        request: &ArenaWorkspaceCreateRequest,
    ) -> Result<ArenaContenderWorkspace, String>;

    /// Inspect candidate workspace mutations (`diff_summary`, optional `checks_summary`)
    /// without exposing raw Host filesystem paths.
    async fn inspect_workspace(
        &self,
        workspace_ref: &str,
    ) -> Result<ArenaWorkspaceInspection, String>;

    /// Immediately delete a losing, cancelled, or already-applied physical CoW clone.
    async fn discard_workspace(&self, workspace_ref: &str) -> Result<(), String>;

    /// Persist unified `.patch` + scorecard evidence for an unapplied winning CoW workspace
    /// into System Store rollout evidence, then delete the physical clone directory.
    async fn flush_winner_patch_on_exit(
        &self,
        request: &ArenaWinnerFlushRequest,
    ) -> Result<ArenaWinnerFlushOutcome, String>;
}

/// Report emitted for each pending winner processed by [`ArenaExitFlushRegistry::flush_all_on_exit`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaExitFlushReport {
    pub workspace_ref: String,
    pub candidate_id: String,
    pub was_applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_artifact_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scorecard_artifact_ref: Option<String>,
    pub clone_purged: bool,
}

#[derive(Clone)]
struct PendingWinnerEntry {
    port: Arc<dyn ArenaWorkspacePort>,
    request: ArenaWinnerFlushRequest,
    applied_to_host: bool,
}

/// Exit-time cleanup registry ensuring unapplied winning `.patch` + scorecard
/// evidence is flushed to System Store and physical CoW clones are purged when
/// `alan` exits normally.
#[derive(Clone, Default)]
pub struct ArenaExitFlushRegistry {
    pending: Arc<Mutex<HashMap<String, PendingWinnerEntry>>>,
}

impl ArenaExitFlushRegistry {
    /// Create an empty exit-flush registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Track a selected winning workspace until it is either applied or flushed on exit.
    pub fn register_winner(
        &self,
        port: Arc<dyn ArenaWorkspacePort>,
        request: ArenaWinnerFlushRequest,
    ) {
        self.pending
            .lock()
            .expect("arena exit registry poisoned")
            .insert(
                request.workspace_ref.clone(),
                PendingWinnerEntry {
                    port,
                    request,
                    applied_to_host: false,
                },
            );
    }

    /// Mark a selected winning workspace as already synced back to the Host directory.
    ///
    /// On exit its physical clone is still purged, without emitting a duplicate unapplied patch.
    pub fn mark_winner_applied(&self, workspace_ref: &str) -> bool {
        let mut guard = self.pending.lock().expect("arena exit registry poisoned");
        if let Some(entry) = guard.get_mut(workspace_ref) {
            entry.applied_to_host = true;
            true
        } else {
            false
        }
    }

    /// Number of tracked winning workspaces awaiting exit cleanup.
    pub fn pending_count(&self) -> usize {
        self.pending
            .lock()
            .expect("arena exit registry poisoned")
            .len()
    }

    /// Flush all unapplied winning `.patch` + scorecard artifacts and purge all tracked clones.
    pub async fn flush_all_on_exit(
        &self,
    ) -> Result<Vec<ArenaExitFlushReport>, ExecutableArenaError> {
        let entries = {
            let mut guard = self.pending.lock().expect("arena exit registry poisoned");
            guard.drain().map(|(_, entry)| entry).collect::<Vec<_>>()
        };

        let mut reports = Vec::with_capacity(entries.len());
        let mut first_error = None;
        for entry in entries {
            if entry.applied_to_host {
                match entry
                    .port
                    .discard_workspace(&entry.request.workspace_ref)
                    .await
                {
                    Ok(()) => reports.push(ArenaExitFlushReport {
                        workspace_ref: entry.request.workspace_ref,
                        candidate_id: entry.request.candidate_id,
                        was_applied: true,
                        patch_artifact_ref: None,
                        scorecard_artifact_ref: None,
                        clone_purged: true,
                    }),
                    Err(err) if first_error.is_none() => {
                        first_error = Some(ExecutableArenaError::WorkspacePort(err));
                    }
                    Err(_) => {}
                }
            } else {
                match entry.port.flush_winner_patch_on_exit(&entry.request).await {
                    Ok(outcome) => {
                        if let Some(ref artifact_ref) = outcome.patch_artifact_ref
                            && let Err(err) = validate_no_raw_host_path(artifact_ref)
                            && first_error.is_none()
                        {
                            first_error = Some(err);
                            continue;
                        }
                        if let Some(ref scorecard_ref) = outcome.scorecard_artifact_ref
                            && let Err(err) = validate_no_raw_host_path(scorecard_ref)
                            && first_error.is_none()
                        {
                            first_error = Some(err);
                            continue;
                        }
                        reports.push(ArenaExitFlushReport {
                            workspace_ref: entry.request.workspace_ref,
                            candidate_id: entry.request.candidate_id,
                            was_applied: false,
                            patch_artifact_ref: outcome.patch_artifact_ref,
                            scorecard_artifact_ref: outcome.scorecard_artifact_ref,
                            clone_purged: outcome.clone_purged,
                        });
                    }
                    Err(err) if first_error.is_none() => {
                        first_error = Some(ExecutableArenaError::WorkspacePort(err));
                    }
                    Err(_) => {}
                }
            }
        }

        if let Some(err) = first_error {
            return Err(err);
        }
        Ok(reports)
    }
}

/// Validate an opaque `workspace_ref` handle (must never be a filesystem path).
pub fn validate_opaque_workspace_ref(value: &str) -> Result<(), ExecutableArenaError> {
    if value.is_empty()
        || value.len() > 128
        || value.contains("..")
        || !value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(ExecutableArenaError::RawHostPathRejected(format!(
            "invalid opaque workspace_ref `{value}`"
        )));
    }
    Ok(())
}

/// Reject strings embedding raw Host filesystem paths (`ADR-0050`).
pub fn validate_no_raw_host_path(value: &str) -> Result<(), ExecutableArenaError> {
    if contains_raw_host_path(value) {
        return Err(ExecutableArenaError::RawHostPathRejected(
            "payload contains raw Host filesystem path".to_string(),
        ));
    }
    Ok(())
}

fn contains_raw_host_path(value: &str) -> bool {
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

        if at_token_boundary
            && byte.is_ascii_alphabetic()
            && idx + 2 < bytes.len()
            && bytes[idx + 1] == b':'
            && matches!(bytes[idx + 2], b'\\' | b'/')
        {
            return true;
        }

        if at_token_boundary
            && byte == b'~'
            && idx + 1 < bytes.len()
            && matches!(bytes[idx + 1], b'/' | b'\\')
        {
            return true;
        }

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
