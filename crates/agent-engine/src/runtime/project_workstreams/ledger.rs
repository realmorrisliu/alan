//! System Store Project Ledger and per-stream `WorkstreamRecord` persistence.
//!
//! Layout under `<projects_root>/<opaque_project_key>/`:
//! - `ledger.json` — bounded index of streams and latest state
//! - `streams/<stream_name>.json` — individual `WorkstreamRecord` metadata

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::redact_secret_tokens;

use super::{
    atomic_write_json, ensure_projects_subdir, validate_identifier, validate_project_key,
    validate_projects_root, verify_existing_regular_file, verify_existing_subdir,
};

pub const PROJECT_LEDGER_VERSION: u16 = 1;
pub const WORKSTREAM_RECORD_VERSION: u16 = 1;

const LEDGER_FILE_NAME: &str = "ledger.json";
const STREAMS_DIR_NAME: &str = "streams";
const MAX_WORKSTREAM_SCOPE_CHARS: usize = 512;

const fn default_ledger_version() -> u16 {
    PROJECT_LEDGER_VERSION
}

const fn default_record_version() -> u16 {
    WORKSTREAM_RECORD_VERSION
}

/// Lifecycle durability class of a named workstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamPersistence {
    Persistent,
    OneOff,
}

/// Execution / retention state of a named workstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamState {
    Running,
    Idle,
    Archived,
    PausedError,
}

impl WorkstreamState {
    /// Returns true if the workstream has not collapsed into `Archived`.
    pub const fn is_non_archived(self) -> bool {
        !matches!(self, Self::Archived)
    }
}

/// Tiered Host Mount write isolation policy for a workstream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamWriteMode {
    #[default]
    ReadOnly,
    SharedQueue,
    Worktree,
}

/// Opt-in Stateful Cron Stream guardrails attached to a `Persistent` workstream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CronStreamPolicy {
    pub workflow_id: String,
    pub max_consecutive_failures: u32,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_threshold_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_run_budget_usd: Option<f64>,
}

/// Per-stream metadata & checkpoint pointer persisted at `streams/<name>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkstreamRecord {
    #[serde(default = "default_record_version")]
    pub version: u16,
    pub name: String,
    pub scope: String,
    pub persistence: WorkstreamPersistence,
    pub state: WorkstreamState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checkpoint_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_ref: Option<String>,
    #[serde(default)]
    pub write_mode: WorkstreamWriteMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron_binding: Option<CronStreamPolicy>,
    pub updated_at: String,
}

impl WorkstreamRecord {
    /// Construct a new `Running` workstream record with default metadata fields.
    pub fn new(
        name: impl Into<String>,
        scope: impl Into<String>,
        persistence: WorkstreamPersistence,
    ) -> Self {
        Self {
            version: WORKSTREAM_RECORD_VERSION,
            name: name.into(),
            scope: scope.into(),
            persistence,
            state: WorkstreamState::Running,
            last_checkpoint_ref: None,
            rollout_ref: None,
            write_mode: WorkstreamWriteMode::ReadOnly,
            worktree_ref: None,
            budget_profile: None,
            model_override: None,
            cron_binding: None,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

/// Aggregated workstream state counts for a project ledger.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkstreamCounts {
    pub running: usize,
    pub idle: usize,
    pub paused_error: usize,
    pub archived: usize,
}

impl WorkstreamCounts {
    /// Count of non-archived workstreams (`running + idle + paused_error`).
    pub const fn active_non_archived(&self) -> usize {
        self.running + self.idle + self.paused_error
    }

    /// Total workstreams in the ledger including `archived`.
    pub const fn total(&self) -> usize {
        self.running + self.idle + self.paused_error + self.archived
    }
}

/// Bounded summary index of streams + latest state persisted at `ledger.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectLedger {
    #[serde(default = "default_ledger_version")]
    pub version: u16,
    pub project_key: String,
    pub updated_at: String,
    #[serde(default)]
    pub streams: Vec<WorkstreamRecord>,
}

impl ProjectLedger {
    /// Construct an empty in-memory ledger for `project_key`.
    pub fn empty(project_key: impl Into<String>) -> Self {
        Self {
            version: PROJECT_LEDGER_VERSION,
            project_key: project_key.into(),
            updated_at: Utc::now().to_rfc3339(),
            streams: Vec::new(),
        }
    }

    /// Compute running, idle, paused_error, and archived counts.
    pub fn counts(&self) -> WorkstreamCounts {
        let mut counts = WorkstreamCounts::default();
        for stream in &self.streams {
            match stream.state {
                WorkstreamState::Running => counts.running += 1,
                WorkstreamState::Idle => counts.idle += 1,
                WorkstreamState::PausedError => counts.paused_error += 1,
                WorkstreamState::Archived => counts.archived += 1,
            }
        }
        counts
    }

    /// Return all non-archived workstreams in deterministic order.
    pub fn active_streams(&self) -> Vec<&WorkstreamRecord> {
        self.streams
            .iter()
            .filter(|record| record.state.is_non_archived())
            .collect()
    }

    /// Return all archived workstreams in deterministic order.
    pub fn archived_streams(&self) -> Vec<&WorkstreamRecord> {
        self.streams
            .iter()
            .filter(|record| record.state == WorkstreamState::Archived)
            .collect()
    }

    /// Look up a workstream by exact `name`.
    pub fn find_stream(&self, name: &str) -> Option<&WorkstreamRecord> {
        self.streams.iter().find(|record| record.name == name)
    }
}

/// Return `<projects_root>/<project_key>/ledger.json`.
pub fn ledger_path(projects_root: &Path, project_key: &str) -> Result<PathBuf> {
    validate_projects_root(projects_root)?;
    validate_project_key(project_key)?;
    Ok(projects_root.join(project_key).join(LEDGER_FILE_NAME))
}

/// Return `<projects_root>/<project_key>/streams`.
pub fn streams_dir(projects_root: &Path, project_key: &str) -> Result<PathBuf> {
    validate_projects_root(projects_root)?;
    validate_project_key(project_key)?;
    Ok(projects_root.join(project_key).join(STREAMS_DIR_NAME))
}

/// Return `<projects_root>/<project_key>/streams/<name>.json`.
pub fn stream_path(projects_root: &Path, project_key: &str, name: &str) -> Result<PathBuf> {
    validate_identifier("stream_name", name)?;
    Ok(streams_dir(projects_root, project_key)?.join(format!("{name}.json")))
}

/// Read-only load of `ledger.json`. Returns an empty `ProjectLedger` if none has been written yet.
pub fn load_project_ledger(projects_root: &Path, project_key: &str) -> Result<ProjectLedger> {
    validate_project_key(project_key)?;
    let Some(proj_dir) = verify_existing_subdir(projects_root, &[project_key])? else {
        return Ok(ProjectLedger::empty(project_key));
    };
    let path = proj_dir.join(LEDGER_FILE_NAME);
    if !verify_existing_regular_file(&proj_dir, &path)? {
        return Ok(ProjectLedger::empty(project_key));
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let mut ledger = serde_json::from_slice::<ProjectLedger>(&bytes)
        .with_context(|| format!("parse {}", path.display()))?;
    ensure!(
        ledger.project_key == project_key,
        "ledger project_key mismatch in {}: expected `{project_key}`, found `{}`",
        path.display(),
        ledger.project_key
    );
    ledger.streams.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ledger)
}

/// Read-only load of a single `streams/<name>.json` record if present.
pub fn load_workstream_record(
    projects_root: &Path,
    project_key: &str,
    name: &str,
) -> Result<Option<WorkstreamRecord>> {
    validate_project_key(project_key)?;
    validate_identifier("stream_name", name)?;
    let Some(dir_path) = verify_existing_subdir(projects_root, &[project_key, STREAMS_DIR_NAME])?
    else {
        return Ok(None);
    };
    let path = dir_path.join(format!("{name}.json"));
    if !verify_existing_regular_file(&dir_path, &path)? {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let record = serde_json::from_slice::<WorkstreamRecord>(&bytes)
        .with_context(|| format!("parse {}", path.display()))?;
    ensure!(
        record.name == name,
        "workstream name mismatch in {}: expected `{name}`, found `{}`",
        path.display(),
        record.name
    );
    Ok(Some(record))
}

/// Atomically persist `record` to `streams/<name>.json` and update `ledger.json`.
pub fn upsert_workstream(
    projects_root: &Path,
    project_key: &str,
    mut record: WorkstreamRecord,
) -> Result<WorkstreamRecord> {
    validate_project_key(project_key)?;
    validate_identifier("stream_name", &record.name)?;
    record.version = WORKSTREAM_RECORD_VERSION;
    record.scope = sanitize_scope(&record.scope);
    if record.updated_at.trim().is_empty() {
        record.updated_at = Utc::now().to_rfc3339();
    }

    let proj_dir = ensure_projects_subdir(projects_root, &[project_key])?;
    let s_dir = ensure_projects_subdir(projects_root, &[project_key, STREAMS_DIR_NAME])?;
    let s_path = s_dir.join(format!("{}.json", record.name));
    atomic_write_json(&s_dir, &s_path, &record)?;

    let mut ledger = load_project_ledger(projects_root, project_key)?;
    if let Some(slot) = ledger
        .streams
        .iter_mut()
        .find(|existing| existing.name == record.name)
    {
        *slot = record.clone();
    } else {
        ledger.streams.push(record.clone());
    }
    ledger.streams.sort_by(|a, b| a.name.cmp(&b.name));
    ledger.updated_at = Utc::now().to_rfc3339();

    let l_path = proj_dir.join(LEDGER_FILE_NAME);
    atomic_write_json(&proj_dir, &l_path, &ledger)?;
    Ok(record)
}

/// Transition an `Idle` or `PausedError` `Persistent` workstream to `Running`.
pub fn resume_workstream(
    projects_root: &Path,
    project_key: &str,
    name: &str,
) -> Result<WorkstreamRecord> {
    let mut record = load_workstream_record(projects_root, project_key, name)?
        .with_context(|| format!("workstream `{name}` not found in project `{project_key}`"))?;
    ensure!(
        record.persistence == WorkstreamPersistence::Persistent,
        "cannot resume one_off workstream `{name}`"
    );
    match record.state {
        WorkstreamState::Idle | WorkstreamState::PausedError => {
            record.state = WorkstreamState::Running;
            record.updated_at = Utc::now().to_rfc3339();
            upsert_workstream(projects_root, project_key, record)
        }
        WorkstreamState::Running => bail!("workstream `{name}` is already running"),
        WorkstreamState::Archived => bail!("cannot resume archived workstream `{name}`"),
    }
}

/// Transition a workstream to `Idle`, updating checkpoint/rollout refs when supplied.
pub fn idle_workstream(
    projects_root: &Path,
    project_key: &str,
    name: &str,
    checkpoint_ref: Option<String>,
    rollout_ref: Option<String>,
) -> Result<WorkstreamRecord> {
    let mut record = load_workstream_record(projects_root, project_key, name)?
        .with_context(|| format!("workstream `{name}` not found in project `{project_key}`"))?;
    ensure!(
        record.state != WorkstreamState::Archived,
        "cannot idle archived workstream `{name}`"
    );
    if let Some(ckpt) = clean_optional_ref(checkpoint_ref) {
        record.last_checkpoint_ref = Some(ckpt);
    }
    if let Some(rollout) = clean_optional_ref(rollout_ref) {
        record.rollout_ref = Some(rollout);
    }
    record.state = WorkstreamState::Idle;
    record.updated_at = Utc::now().to_rfc3339();
    upsert_workstream(projects_root, project_key, record)
}

/// Transition a workstream to `Archived`, collapsing it from the active roster while
/// preserving its `rollout_ref` for durable inspection.
pub fn archive_workstream(
    projects_root: &Path,
    project_key: &str,
    name: &str,
    rollout_ref: Option<String>,
) -> Result<WorkstreamRecord> {
    let mut record = load_workstream_record(projects_root, project_key, name)?
        .with_context(|| format!("workstream `{name}` not found in project `{project_key}`"))?;
    if let Some(rollout) = clean_optional_ref(rollout_ref) {
        record.rollout_ref = Some(rollout);
    }
    record.state = WorkstreamState::Archived;
    record.updated_at = Utc::now().to_rfc3339();
    upsert_workstream(projects_root, project_key, record)
}

/// Transition a workstream to `PausedError`, preserving/updating durable refs.
pub fn pause_workstream_error(
    projects_root: &Path,
    project_key: &str,
    name: &str,
    checkpoint_ref: Option<String>,
    rollout_ref: Option<String>,
) -> Result<WorkstreamRecord> {
    let mut record = load_workstream_record(projects_root, project_key, name)?
        .with_context(|| format!("workstream `{name}` not found in project `{project_key}`"))?;
    if let Some(ckpt) = clean_optional_ref(checkpoint_ref) {
        record.last_checkpoint_ref = Some(ckpt);
    }
    if let Some(rollout) = clean_optional_ref(rollout_ref) {
        record.rollout_ref = Some(rollout);
    }
    record.state = WorkstreamState::PausedError;
    record.updated_at = Utc::now().to_rfc3339();
    upsert_workstream(projects_root, project_key, record)
}

/// Complete a workstream turn according to its persistence policy:
/// - `Persistent` -> `Idle` with updated checkpoint/rollout refs
/// - `OneOff` -> `Archived` with updated rollout ref
pub fn complete_workstream(
    projects_root: &Path,
    project_key: &str,
    name: &str,
    checkpoint_ref: Option<String>,
    rollout_ref: Option<String>,
) -> Result<WorkstreamRecord> {
    let record = load_workstream_record(projects_root, project_key, name)?
        .with_context(|| format!("workstream `{name}` not found in project `{project_key}`"))?;
    match record.persistence {
        WorkstreamPersistence::Persistent => idle_workstream(
            projects_root,
            project_key,
            name,
            checkpoint_ref,
            rollout_ref,
        ),
        WorkstreamPersistence::OneOff => {
            archive_workstream(projects_root, project_key, name, rollout_ref)
        }
    }
}

/// Return active vs archived counts for `project_key`.
pub fn workstream_counts(projects_root: &Path, project_key: &str) -> Result<WorkstreamCounts> {
    Ok(load_project_ledger(projects_root, project_key)?.counts())
}

fn clean_optional_ref(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn sanitize_scope(raw: &str) -> String {
    let scrubbed = redact_secret_tokens(raw.trim());
    let cleaned = scrubbed.trim();
    if cleaned.chars().count() <= MAX_WORKSTREAM_SCOPE_CHARS {
        return cleaned.to_string();
    }
    let mut out: String = cleaned
        .chars()
        .take(MAX_WORKSTREAM_SCOPE_CHARS.saturating_sub(3))
        .collect();
    out.push_str("...");
    out
}
