//! Contender and Verifier `SpawnSpec` builders and isolation validation for `alan-arena`.

use std::collections::HashSet;
use std::path::{Component, PathBuf};

use alan_agent_protocol::{
    DelegatedCapabilityRequirement, DelegatedSpawnContext, SpawnHandle, SpawnHostMount,
    SpawnLaunchInputs, SpawnMountAccess, SpawnRuntimeOverrides, SpawnSpec, SpawnTarget,
};
use serde::{Deserialize, Serialize};

use super::{
    ArenaMode, ArenaWorkspaceBackingKind, DEFAULT_ARENA_SUBAGENT_TIMEOUT_SECS,
    DEFAULT_MAX_EXECUTABLE_ARENA_CONTENDERS, ExecutableArenaError, validate_no_raw_host_path,
    validate_opaque_workspace_ref,
};

/// Single contender specification inside an `alan-arena` bakeoff request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaContenderRequest {
    pub candidate_id: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

/// Independent Verifier specification inside an `alan-arena` bakeoff request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaVerifierRequest {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

impl Default for ArenaVerifierRequest {
    fn default() -> Self {
        Self {
            role: "arena-verifier".to_string(),
            model: None,
            timeout_secs: None,
        }
    }
}

/// Full bounded request for running a `read_only` (default) or `executable` `alan-arena`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableArenaRequest {
    #[serde(default)]
    pub mode: ArenaMode,
    pub problem_statement: String,
    pub acceptance_criteria: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_grant: Option<String>,
    #[serde(default = "default_base_branch_id")]
    pub base_branch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default = "default_true")]
    pub network_denied: bool,
    #[serde(default = "default_max_contenders")]
    pub max_contenders: usize,
    pub contenders: Vec<ArenaContenderRequest>,
    #[serde(default)]
    pub verifier: ArenaVerifierRequest,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_only_mounts: Vec<SpawnHostMount>,
}

fn default_base_branch_id() -> String {
    "base".to_string()
}

const fn default_true() -> bool {
    true
}

const fn default_max_contenders() -> usize {
    DEFAULT_MAX_EXECUTABLE_ARENA_CONTENDERS
}

impl ExecutableArenaRequest {
    /// Validate request-level bounds before provisioning CoW workspaces or launching contenders.
    pub fn validate(&self) -> Result<(), ExecutableArenaError> {
        if self.problem_statement.trim().is_empty() {
            return Err(ExecutableArenaError::InvalidConfiguration(
                "problem_statement must be non-empty".to_string(),
            ));
        }
        if self.acceptance_criteria.trim().is_empty() {
            return Err(ExecutableArenaError::InvalidConfiguration(
                "acceptance_criteria must be non-empty".to_string(),
            ));
        }
        validate_no_raw_host_path(&self.problem_statement)?;
        validate_no_raw_host_path(&self.acceptance_criteria)?;
        validate_branch_slug("base_branch_id", &self.base_branch_id)?;

        if let Some(workstream) = self.workstream.as_deref() {
            validate_no_raw_host_path(workstream)?;
        }
        if let Some(rollout_ref) = self.rollout_ref.as_deref() {
            validate_no_raw_host_path(rollout_ref)?;
        }
        if let Some(why) = self.why.as_deref() {
            validate_no_raw_host_path(why)?;
        }

        if self.max_contenders == 0 || self.max_contenders > DEFAULT_MAX_EXECUTABLE_ARENA_CONTENDERS
        {
            return Err(ExecutableArenaError::InvalidConfiguration(format!(
                "max_contenders must be between 1 and {DEFAULT_MAX_EXECUTABLE_ARENA_CONTENDERS}"
            )));
        }
        if self.contenders.len() < 2 {
            return Err(ExecutableArenaError::InvalidConfiguration(
                "alan-arena requires at least 2 contenders".to_string(),
            ));
        }
        if self.contenders.len() > self.max_contenders {
            return Err(ExecutableArenaError::InvalidConfiguration(format!(
                "contender count {} exceeds max_contenders {}",
                self.contenders.len(),
                self.max_contenders
            )));
        }

        let mut seen_ids = HashSet::new();
        for contender in &self.contenders {
            validate_branch_slug("candidate_id", &contender.candidate_id)?;
            if !seen_ids.insert(contender.candidate_id.as_str()) {
                return Err(ExecutableArenaError::InvalidConfiguration(format!(
                    "duplicate candidate_id `{}`",
                    contender.candidate_id
                )));
            }
            if contender.role.trim().is_empty() || contender.task.trim().is_empty() {
                return Err(ExecutableArenaError::InvalidConfiguration(format!(
                    "contender `{}` requires non-empty role and task",
                    contender.candidate_id
                )));
            }
            validate_no_raw_host_path(&contender.task)?;
        }

        if self.verifier.role.trim().is_empty() {
            return Err(ExecutableArenaError::InvalidConfiguration(
                "verifier.role must be non-empty".to_string(),
            ));
        }

        match self.mode {
            ArenaMode::ReadOnly => {
                if self
                    .read_only_mounts
                    .iter()
                    .any(|mount| mount.access != SpawnMountAccess::ReadOnly)
                {
                    return Err(ExecutableArenaError::ContenderMountViolation(
                        "read_only alan-arena forbids read_write host_mounts".to_string(),
                    ));
                }
            }
            ArenaMode::Executable => {
                if !self.network_denied {
                    return Err(ExecutableArenaError::InvalidConfiguration(
                        "executable alan-arena requires network_denied = true by default"
                            .to_string(),
                    ));
                }
                let grant = self.source_grant.as_deref().map(str::trim).unwrap_or("");
                if grant.is_empty() {
                    return Err(ExecutableArenaError::InvalidConfiguration(
                        "executable alan-arena requires an explicit authorized source_grant"
                            .to_string(),
                    ));
                }
                validate_no_raw_host_path(grant)?;
            }
        }

        Ok(())
    }
}

/// Neutral candidate dossier passed to the independent `read_only = true` Verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierCandidateSummary<'a> {
    pub neutral_label: &'a str,
    pub branch_id: &'a str,
    pub workspace_ref: Option<&'a str>,
    pub backing_kind: Option<ArenaWorkspaceBackingKind>,
    pub diff_summary: Option<&'a str>,
    pub checks_summary: Option<&'a str>,
    pub contender_output: &'a str,
}

/// Build an isolated executable contender `SpawnSpec` bound strictly to a single
/// narrowed `ReadWrite` CoW clone mount with network denied and zero secrets.
pub fn build_executable_contender_spawn_spec(
    request: &ExecutableArenaRequest,
    contender: &ArenaContenderRequest,
    source_grant: &str,
    workspace_ref: &str,
    contender_mount: &SpawnHostMount,
) -> Result<SpawnSpec, ExecutableArenaError> {
    if !request.network_denied {
        return Err(ExecutableArenaError::InvalidConfiguration(
            "executable contenders must run with network_denied = true".to_string(),
        ));
    }
    validate_opaque_workspace_ref(workspace_ref)?;
    validate_narrowed_contender_mount(source_grant, contender_mount)?;

    let task = format!(
        "Executable alan-arena contender `{}` (workspace_ref: `{workspace_ref}`).\n\
         Isolation posture: narrowed ReadWrite mount at `{}` ONLY; network denied; zero Host Store secrets.\n\n\
         Problem Statement:\n{}\n\n\
         Acceptance Criteria:\n{}\n\n\
         Contender Task:\n{}",
        contender.candidate_id,
        contender_mount.target.display(),
        request.problem_statement,
        request.acceptance_criteria,
        contender.task
    );

    let spec = SpawnSpec {
        target: SpawnTarget::AdHocSubagent {
            role: contender.role.trim().to_string(),
        },
        launch: SpawnLaunchInputs {
            task,
            cwd: Some(contender_mount.target.clone()),
            timeout_secs: Some(
                contender
                    .timeout_secs
                    .unwrap_or(DEFAULT_ARENA_SUBAGENT_TIMEOUT_SECS),
            ),
            output_dir: None,
        },
        // ApprovalScope only: zero Memory descriptor, zero Artifacts, zero secret handles.
        handles: vec![SpawnHandle::ApprovalScope],
        host_mounts: vec![contender_mount.clone()],
        runtime_overrides: SpawnRuntimeOverrides {
            model: contender.model.clone(),
            read_only: false,
            ..SpawnRuntimeOverrides::default()
        },
        delegated: Some(DelegatedSpawnContext {
            requirements: vec![
                DelegatedCapabilityRequirement::MountRead {
                    path: Some(contender_mount.target.clone()),
                },
                DelegatedCapabilityRequirement::MountWrite {
                    path: Some(contender_mount.target.clone()),
                },
                DelegatedCapabilityRequirement::Shell,
                DelegatedCapabilityRequirement::LlmConnection,
            ],
        }),
    };

    spec.validate_agent_process_launch()
        .map_err(|err| ExecutableArenaError::InvalidConfiguration(err.to_string()))?;
    Ok(spec)
}

/// Build a standard `read_only = true` contender `SpawnSpec` (default `alan-arena` mode).
pub fn build_read_only_contender_spawn_spec(
    request: &ExecutableArenaRequest,
    contender: &ArenaContenderRequest,
    read_only_mounts: &[SpawnHostMount],
) -> Result<SpawnSpec, ExecutableArenaError> {
    if read_only_mounts
        .iter()
        .any(|mount| mount.access != SpawnMountAccess::ReadOnly)
    {
        return Err(ExecutableArenaError::ContenderMountViolation(
            "read_only arena contenders cannot receive read_write host_mounts".to_string(),
        ));
    }
    for mount in read_only_mounts {
        validate_namespace_mount_target(&mount.target)?;
    }

    let cwd = read_only_mounts
        .first()
        .map(|mount| mount.target.clone())
        .unwrap_or_else(|| PathBuf::from("/"));

    let task = format!(
        "Read-only alan-arena contender `{}`.\n\n\
         Problem Statement:\n{}\n\n\
         Acceptance Criteria:\n{}\n\n\
         Contender Task:\n{}",
        contender.candidate_id,
        request.problem_statement,
        request.acceptance_criteria,
        contender.task
    );

    let spec = SpawnSpec {
        target: SpawnTarget::AdHocSubagent {
            role: contender.role.trim().to_string(),
        },
        launch: SpawnLaunchInputs {
            task,
            cwd: Some(cwd),
            timeout_secs: Some(
                contender
                    .timeout_secs
                    .unwrap_or(DEFAULT_ARENA_SUBAGENT_TIMEOUT_SECS),
            ),
            output_dir: None,
        },
        handles: vec![SpawnHandle::ApprovalScope],
        host_mounts: read_only_mounts.to_vec(),
        runtime_overrides: SpawnRuntimeOverrides {
            model: contender.model.clone(),
            read_only: true,
            ..SpawnRuntimeOverrides::default()
        },
        delegated: None,
    };

    spec.validate_agent_process_launch()
        .map_err(|err| ExecutableArenaError::InvalidConfiguration(err.to_string()))?;
    Ok(spec)
}

/// Build the independent `read_only = true` Verifier `SpawnSpec`.
///
/// Rejects any `ReadWrite` mount and strips raw Host paths from candidate dossiers.
pub fn build_verifier_spawn_spec(
    request: &ExecutableArenaRequest,
    verifier_mounts: &[SpawnHostMount],
    candidates: &[VerifierCandidateSummary<'_>],
) -> Result<SpawnSpec, ExecutableArenaError> {
    for mount in verifier_mounts {
        if mount.access != SpawnMountAccess::ReadOnly {
            return Err(ExecutableArenaError::VerifierNotReadOnly(format!(
                "mount `{}` for target `{}` requested `{:?}`; verifier allows read_only only",
                mount.grant,
                mount.target.display(),
                mount.access
            )));
        }
        validate_namespace_mount_target(&mount.target)?;
    }

    let mut dossier = String::new();
    for candidate in candidates {
        if let Some(ws_ref) = candidate.workspace_ref {
            validate_opaque_workspace_ref(ws_ref)?;
        }
        if let Some(diff) = candidate.diff_summary {
            validate_no_raw_host_path(diff)?;
        }
        if let Some(checks) = candidate.checks_summary {
            validate_no_raw_host_path(checks)?;
        }
        validate_no_raw_host_path(candidate.contender_output)?;

        dossier.push_str(&format!(
            "## {} (branch_id: {})\n",
            candidate.neutral_label, candidate.branch_id
        ));
        if let Some(ws_ref) = candidate.workspace_ref {
            dossier.push_str(&format!("- workspace_ref: {ws_ref}\n"));
        }
        if let Some(kind) = candidate.backing_kind {
            dossier.push_str(&format!("- backing_kind: {}\n", kind.as_str()));
        }
        if let Some(diff) = candidate.diff_summary {
            dossier.push_str(&format!("- diff_summary: {diff}\n"));
        }
        if let Some(checks) = candidate.checks_summary {
            dossier.push_str(&format!("- checks_summary: {checks}\n"));
        }
        dossier.push_str(&format!(
            "- contender_output:\n{}\n\n",
            candidate.contender_output
        ));
    }

    let task = format!(
        "Independent read-only alan-arena verifier (`read_only: true`).\n\
         Inspect candidate diffs, check outcomes, and read-only views, score every candidate \
         against the acceptance criteria, and return a JSON scorecard with `scores` \
         (array of {{`candidate`, `score`, `summary`}}), `winner`, and `rationale`.\n\n\
         Problem Statement:\n{}\n\n\
         Acceptance Criteria:\n{}\n\n\
         Candidates:\n{dossier}",
        request.problem_statement, request.acceptance_criteria
    );

    let cwd = verifier_mounts
        .first()
        .map(|mount| mount.target.clone())
        .unwrap_or_else(|| PathBuf::from("/"));

    let spec = SpawnSpec {
        target: SpawnTarget::AdHocSubagent {
            role: request.verifier.role.trim().to_string(),
        },
        launch: SpawnLaunchInputs {
            task,
            cwd: Some(cwd),
            timeout_secs: Some(
                request
                    .verifier
                    .timeout_secs
                    .unwrap_or(DEFAULT_ARENA_SUBAGENT_TIMEOUT_SECS),
            ),
            output_dir: None,
        },
        handles: vec![SpawnHandle::ApprovalScope],
        host_mounts: verifier_mounts.to_vec(),
        runtime_overrides: SpawnRuntimeOverrides {
            model: request.verifier.model.clone(),
            read_only: true,
            ..SpawnRuntimeOverrides::default()
        },
        delegated: None,
    };

    spec.validate_agent_process_launch()
        .map_err(|err| ExecutableArenaError::VerifierNotReadOnly(err.to_string()))?;
    Ok(spec)
}

fn validate_narrowed_contender_mount(
    source_grant: &str,
    mount: &SpawnHostMount,
) -> Result<(), ExecutableArenaError> {
    if mount.grant.trim().is_empty() {
        return Err(ExecutableArenaError::ContenderMountViolation(
            "contender mount grant must be non-empty".to_string(),
        ));
    }
    validate_no_raw_host_path(&mount.grant)?;
    if mount.grant.trim() == source_grant.trim() {
        return Err(ExecutableArenaError::ContenderMountViolation(
            "contender must receive a narrowed clone grant, not the parent source_grant"
                .to_string(),
        ));
    }
    if mount.access != SpawnMountAccess::ReadWrite {
        return Err(ExecutableArenaError::ContenderMountViolation(format!(
            "executable contender requires a narrowed read_write clone mount, got {:?}",
            mount.access
        )));
    }
    validate_namespace_mount_target(&mount.target)
}

fn validate_namespace_mount_target(target: &std::path::Path) -> Result<(), ExecutableArenaError> {
    if !target.is_absolute()
        || target
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ExecutableArenaError::ContenderMountViolation(format!(
            "mount target `{}` must be an absolute normalized namespace path",
            target.display()
        )));
    }
    if !target.starts_with("/mnt/") {
        return Err(ExecutableArenaError::ContenderMountViolation(format!(
            "mount target `{}` must project under `/mnt/`",
            target.display()
        )));
    }
    Ok(())
}

fn validate_branch_slug(field: &str, value: &str) -> Result<(), ExecutableArenaError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ExecutableArenaError::InvalidConfiguration(format!(
            "{field} `{value}` must contain only ASCII alphanumeric, `-`, or `_`"
        )));
    }
    Ok(())
}
