//! Inspectable, user-correctable, and deletable per-project continuity summaries.
//!
//! Continuity summaries live as derived files under `<memory_dir>/topics/` so they stay scoped
//! to the Process Memory Store, scrub secrets via [`crate::redact_secret_tokens`], never mutate
//! Host grants or policy state, honor user corrections over stale automation, and suppress recall
//! and stale regeneration once explicitly deleted.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{agent_machine::AgentMachine, redact_secret_tokens};

const SUMMARY_FILE_PREFIX: &str = "project-summary-";
const MAX_SUMMARY_SECTION_CHARS: usize = 1_200;
const MAX_SOURCE_ROLLOUTS: usize = 32;
const MAX_REVISION_HISTORY: usize = 32;

const ACTIVE_RETENTION_NOTE: &str = "Derived project continuity summary only; editing or deleting it never alters retained rollouts or Host grants.";
const DELETED_RETENTION_NOTE: &str = "Derived project continuity summary deleted (recall and unchanged regeneration suppressed); retained rollout history remains separate.";
const MISSING_RETENTION_NOTE: &str = "No derived project continuity summary recorded yet; retained rollout history remains separate.";

/// Authoritative provenance of the stored project continuity summary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSummaryOrigin {
    Generated,
    UserEdited,
}

/// Snapshot of one historical continuity summary revision for `/summary rollback`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSummaryRevisionSnapshot {
    pub revision: u64,
    pub goal: String,
    pub progress: String,
    pub verification: String,
    #[serde(default)]
    pub source_rollouts: Vec<String>,
    pub origin: ProjectSummaryOrigin,
    #[serde(default)]
    pub user_edited_sections: Vec<String>,
    #[serde(default)]
    pub deleted: bool,
    pub updated_at: String,
}

/// Persisted per-project continuity summary state stored in `topics/project-summary-<key>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectContinuitySummary {
    pub project_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_cwd: Option<String>,
    pub goal: String,
    pub progress: String,
    pub verification: String,
    #[serde(default)]
    pub source_rollouts: Vec<String>,
    pub revision: u64,
    pub updated_at: String,
    pub origin: ProjectSummaryOrigin,
    #[serde(default)]
    pub user_edited_sections: Vec<String>,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_source_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revision_history: Vec<ProjectSummaryRevisionSnapshot>,
}

/// Read projection returned by CLI/TUI/runtime summary inspection helpers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectContinuitySummaryView {
    pub exists: bool,
    pub deleted: bool,
    pub project_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<ProjectContinuitySummary>,
    pub markdown_recall_active: bool,
    pub retention_note: String,
}

/// Requested user correction against a project continuity summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSummaryEdit {
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub progress: Option<String>,
    #[serde(default)]
    pub verification: Option<String>,
    #[serde(default)]
    pub raw_text: Option<String>,
}

/// Result returned when deleting a project continuity summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSummaryDeleteOutcome {
    pub project_key: String,
    pub deleted: bool,
    pub revision: u64,
    pub retention_note: String,
}

/// Derive a safe deterministic filename key for one project boundary.
pub fn derive_project_key(project_cwd: Option<&str>) -> String {
    let Some(normalized) = project_cwd.and_then(normalize_project_cwd) else {
        return "root".to_string();
    };
    if normalized == "/" {
        return "root".to_string();
    }
    let label = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_slug)
        .filter(|slug| !slug.is_empty())
        .unwrap_or_else(|| "project".to_string());
    let digest = Sha256::digest(normalized.as_bytes());
    let mut hex = String::with_capacity(12);
    for byte in digest.iter().take(6) {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    format!("{label}-{hex}")
}

fn normalize_project_cwd(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "/" {
        return Some("/".to_string());
    }
    Some(trimmed.trim_end_matches('/').to_string())
}

fn sanitize_slug(raw: &str) -> String {
    let mut slug = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_') && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 24 {
            break;
        }
    }
    slug.trim_matches('-').to_string()
}

fn json_path(memory_dir: &Path, project_key: &str) -> PathBuf {
    memory_dir
        .join("topics")
        .join(format!("{SUMMARY_FILE_PREFIX}{project_key}.json"))
}

fn markdown_path(memory_dir: &Path, project_key: &str) -> PathBuf {
    memory_dir
        .join("topics")
        .join(format!("{SUMMARY_FILE_PREFIX}{project_key}.md"))
}

fn ensure_safe_topic_path(memory_dir: &Path, path: &Path) -> Result<()> {
    crate::prompts::ensure_memory_store_layout_at(memory_dir)
        .with_context(|| format!("failed to ensure memory layout at {}", memory_dir.display()))?;
    let canonical_root = fs::canonicalize(memory_dir)
        .with_context(|| format!("canonicalize memory root {}", memory_dir.display()))?;
    let topics_dir = memory_dir.join("topics");
    let topics_meta = fs::symlink_metadata(&topics_dir)
        .with_context(|| format!("stat topics dir {}", topics_dir.display()))?;
    if topics_meta.file_type().is_symlink() || !topics_meta.is_dir() {
        bail!("topics directory is not an ordinary directory");
    }
    let canonical_topics = fs::canonicalize(&topics_dir)?;
    if !canonical_topics.starts_with(&canonical_root) {
        bail!("topics directory escapes memory store root");
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() || !meta.file_type().is_file() {
            bail!(
                "refusing symlink or non-regular summary file {}",
                path.display()
            );
        }
        let canonical_target = fs::canonicalize(path)?;
        if !canonical_target.starts_with(&canonical_root) {
            bail!("summary file escapes memory store root");
        }
    }
    Ok(())
}

fn read_summary_raw(
    memory_dir: &Path,
    project_key: &str,
) -> Result<Option<ProjectContinuitySummary>> {
    let path = json_path(memory_dir, project_key);
    if !path.exists() && fs::symlink_metadata(&path).is_err() {
        return Ok(None);
    }
    ensure_safe_topic_path(memory_dir, &path)?;
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let parsed = serde_json::from_slice::<ProjectContinuitySummary>(&bytes)
        .with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(scrub_summary(parsed)))
}

fn write_summary_files(memory_dir: &Path, summary: &ProjectContinuitySummary) -> Result<()> {
    let json_target = json_path(memory_dir, &summary.project_key);
    let md_target = markdown_path(memory_dir, &summary.project_key);
    ensure_safe_topic_path(memory_dir, &json_target)?;
    ensure_safe_topic_path(memory_dir, &md_target)?;

    let scrubbed = scrub_summary(summary.clone());
    let json_bytes = serde_json::to_vec_pretty(&scrubbed)?;
    write_atomic(&json_target, &json_bytes)?;

    if scrubbed.deleted {
        if md_target.exists() {
            fs::remove_file(&md_target).with_context(|| {
                format!("remove deleted summary markdown {}", md_target.display())
            })?;
        }
    } else {
        let markdown = render_summary_markdown(&scrubbed);
        write_atomic(&md_target, markdown.as_bytes())?;
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("summary path missing parent directory")?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("summary path missing filename")?;
    let temp = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path)?;
    Ok(())
}

/// Inspect the project continuity summary for `(memory_dir, project_cwd)`.
pub fn inspect_project_summary(
    memory_dir: &Path,
    project_cwd: Option<&str>,
) -> Result<ProjectContinuitySummaryView> {
    let project_key = derive_project_key(project_cwd);
    let normalized_cwd = project_cwd.and_then(normalize_project_cwd);
    let loaded = read_summary_raw(memory_dir, &project_key)?;
    let md_active = loaded.as_ref().is_some_and(|s| !s.deleted)
        && markdown_path(memory_dir, &project_key).is_file();
    match loaded {
        None => Ok(ProjectContinuitySummaryView {
            exists: false,
            deleted: false,
            project_key,
            project_cwd: normalized_cwd,
            summary: None,
            markdown_recall_active: false,
            retention_note: MISSING_RETENTION_NOTE.to_string(),
        }),
        Some(summary) if summary.deleted => Ok(ProjectContinuitySummaryView {
            exists: false,
            deleted: true,
            project_key,
            project_cwd: summary.project_cwd.clone().or(normalized_cwd),
            summary: Some(summary),
            markdown_recall_active: false,
            retention_note: DELETED_RETENTION_NOTE.to_string(),
        }),
        Some(summary) => Ok(ProjectContinuitySummaryView {
            exists: true,
            deleted: false,
            project_key,
            project_cwd: summary.project_cwd.clone().or(normalized_cwd),
            summary: Some(summary),
            markdown_recall_active: md_active,
            retention_note: ACTIVE_RETENTION_NOTE.to_string(),
        }),
    }
}

/// Apply a user edit to the project continuity summary with optimistic revision checking.
pub fn edit_project_summary(
    memory_dir: &Path,
    project_cwd: Option<&str>,
    expected_revision: Option<u64>,
    edit: ProjectSummaryEdit,
) -> Result<ProjectContinuitySummary> {
    let project_key = derive_project_key(project_cwd);
    let normalized_cwd = project_cwd.and_then(normalize_project_cwd);
    let existing = read_summary_raw(memory_dir, &project_key)?;

    match (&existing, expected_revision) {
        (Some(current), Some(expected)) if current.revision != expected => {
            bail!(
                "project summary revision conflict: expected {}, current {}",
                expected,
                current.revision
            );
        }
        (None, Some(expected)) if expected > 0 => {
            bail!(
                "project summary revision conflict: expected {}, current 0 (no summary yet)",
                expected
            );
        }
        _ => {}
    }

    let parsed_raw = edit.raw_text.as_deref().map(parse_raw_summary_edit);
    let goal_update = edit
        .goal
        .or_else(|| parsed_raw.as_ref().and_then(|p| p.goal.clone()));
    let progress_update = edit
        .progress
        .or_else(|| parsed_raw.as_ref().and_then(|p| p.progress.clone()));
    let verification_update = edit
        .verification
        .or_else(|| parsed_raw.as_ref().and_then(|p| p.verification.clone()));

    if goal_update.is_none() && progress_update.is_none() && verification_update.is_none() {
        bail!("project summary edit requires non-empty content");
    }

    let mut summary = match existing {
        Some(mut current) => {
            push_revision_snapshot(&mut current);
            current
        }
        None => ProjectContinuitySummary {
            project_key,
            project_cwd: normalized_cwd.clone(),
            goal: "No current goal recorded.".to_string(),
            progress: "No progress recorded.".to_string(),
            verification: "No verification status recorded.".to_string(),
            source_rollouts: Vec::new(),
            revision: 0,
            updated_at: Utc::now().to_rfc3339(),
            origin: ProjectSummaryOrigin::UserEdited,
            user_edited_sections: Vec::new(),
            deleted: false,
            deleted_fingerprint: None,
            last_source_fingerprint: None,
            revision_history: Vec::new(),
        },
    };

    if let Some(cwd) = normalized_cwd {
        summary.project_cwd = Some(cwd);
    }
    let mut edited_sections: BTreeSet<String> = summary.user_edited_sections.into_iter().collect();
    if let Some(goal) = goal_update {
        summary.goal = sanitize_summary_text(&goal);
        edited_sections.insert("goal".to_string());
    }
    if let Some(progress) = progress_update {
        summary.progress = sanitize_summary_text(&progress);
        edited_sections.insert("progress".to_string());
    }
    if let Some(verification) = verification_update {
        summary.verification = sanitize_summary_text(&verification);
        edited_sections.insert("verification".to_string());
    }

    summary.revision = summary.revision.saturating_add(1);
    summary.updated_at = Utc::now().to_rfc3339();
    summary.origin = ProjectSummaryOrigin::UserEdited;
    summary.user_edited_sections = edited_sections.into_iter().collect();
    summary.deleted = false;
    summary.deleted_fingerprint = None;

    write_summary_files(memory_dir, &summary)?;
    Ok(scrub_summary(summary))
}

/// Delete the derived project continuity summary, recording a tombstone to prevent recall and stale regeneration.
pub fn delete_project_summary(
    memory_dir: &Path,
    project_cwd: Option<&str>,
    expected_revision: Option<u64>,
) -> Result<ProjectSummaryDeleteOutcome> {
    let project_key = derive_project_key(project_cwd);
    let existing = read_summary_raw(memory_dir, &project_key)?;
    match (&existing, expected_revision) {
        (Some(current), Some(expected)) if current.revision != expected => {
            bail!(
                "project summary revision conflict: expected {}, current {}",
                expected,
                current.revision
            );
        }
        (None, Some(expected)) if expected > 0 => {
            bail!(
                "project summary revision conflict: expected {}, current 0 (no summary yet)",
                expected
            );
        }
        _ => {}
    }

    let mut tombstoned = match existing {
        Some(mut current) => {
            push_revision_snapshot(&mut current);
            current
        }
        None => ProjectContinuitySummary {
            project_key: project_key.clone(),
            project_cwd: project_cwd.and_then(normalize_project_cwd),
            goal: String::new(),
            progress: String::new(),
            verification: String::new(),
            source_rollouts: Vec::new(),
            revision: 0,
            updated_at: Utc::now().to_rfc3339(),
            origin: ProjectSummaryOrigin::Generated,
            user_edited_sections: Vec::new(),
            deleted: true,
            deleted_fingerprint: None,
            last_source_fingerprint: None,
            revision_history: Vec::new(),
        },
    };
    let fingerprint = tombstoned
        .last_source_fingerprint
        .clone()
        .unwrap_or_else(|| {
            fingerprint_summary_inputs(
                &tombstoned.goal,
                &tombstoned.progress,
                &tombstoned.verification,
                &tombstoned.source_rollouts,
            )
        });
    tombstoned.goal.clear();
    tombstoned.progress.clear();
    tombstoned.verification.clear();
    tombstoned.revision = tombstoned.revision.saturating_add(1);
    tombstoned.updated_at = Utc::now().to_rfc3339();
    tombstoned.deleted = true;
    tombstoned.deleted_fingerprint = Some(fingerprint);
    tombstoned.user_edited_sections.clear();

    write_summary_files(memory_dir, &tombstoned)?;
    Ok(ProjectSummaryDeleteOutcome {
        project_key,
        deleted: true,
        revision: tombstoned.revision,
        retention_note: DELETED_RETENTION_NOTE.to_string(),
    })
}

/// Update the derived project continuity summary from a finished turn while respecting tombstones and user edits.
pub(crate) fn refresh_project_summary_from_turn(
    memory_dir: &Path,
    machine: &AgentMachine,
    project_cwd: Option<&str>,
) -> Result<()> {
    let goal = sanitize_summary_text(&super::derive_current_goal(machine));
    let assistant_state = super::derive_latest_assistant_state(machine);
    let completed_items = super::render_plan_items(machine, &["completed"]);
    let pending_items = super::render_plan_items(machine, &["in_progress", "pending"]);
    let progress =
        sanitize_summary_text(&format!("Latest state: {assistant_state}\n{pending_items}"));
    let verification = sanitize_summary_text(if completed_items.contains("No ") {
        "No completed verification items recorded yet."
    } else {
        completed_items.trim()
    });
    let rollout_ref = sanitize_summary_text(machine.memory_record_id());

    if goal == "No current goal recorded." && assistant_state == "No assistant update recorded yet."
    {
        return Ok(());
    }

    let project_key = derive_project_key(project_cwd);
    let normalized_cwd = project_cwd.and_then(normalize_project_cwd);
    let incoming_fingerprint = fingerprint_summary_inputs(
        &goal,
        &progress,
        &verification,
        std::slice::from_ref(&rollout_ref),
    );

    let existing = read_summary_raw(memory_dir, &project_key)?;
    if let Some(current) = existing {
        // Tombstone gate: never silently resurrect after deletion from unchanged or previously seen sources.
        if current.deleted {
            let same_fingerprint =
                current.deleted_fingerprint.as_deref() == Some(&incoming_fingerprint);
            let same_source = current.source_rollouts.iter().any(|r| r == &rollout_ref);
            if same_fingerprint
                || same_source
                || current.last_source_fingerprint.as_deref() == Some(&incoming_fingerprint)
            {
                return Ok(());
            }
        }

        let mut merged = current.clone();
        push_revision_snapshot(&mut merged);
        if let Some(cwd) = normalized_cwd {
            merged.project_cwd = Some(cwd);
        }
        let user_sections: BTreeSet<&str> = current
            .user_edited_sections
            .iter()
            .map(String::as_str)
            .collect();
        let preserve_all =
            current.origin == ProjectSummaryOrigin::UserEdited && user_sections.contains("all");

        if !(current.origin == ProjectSummaryOrigin::UserEdited
            && (preserve_all || user_sections.contains("goal")))
        {
            merged.goal = goal;
        }
        if !(current.origin == ProjectSummaryOrigin::UserEdited
            && (preserve_all || user_sections.contains("progress")))
        {
            merged.progress = progress;
        }
        if !(current.origin == ProjectSummaryOrigin::UserEdited
            && (preserve_all || user_sections.contains("verification")))
        {
            merged.verification = verification;
        }

        if !rollout_ref.is_empty() && !merged.source_rollouts.contains(&rollout_ref) {
            merged.source_rollouts.push(rollout_ref);
            if merged.source_rollouts.len() > MAX_SOURCE_ROLLOUTS {
                let excess = merged.source_rollouts.len() - MAX_SOURCE_ROLLOUTS;
                merged.source_rollouts.drain(0..excess);
            }
        }
        merged.revision = merged.revision.saturating_add(1);
        merged.updated_at = Utc::now().to_rfc3339();
        merged.deleted = false;
        merged.deleted_fingerprint = None;
        merged.last_source_fingerprint = Some(incoming_fingerprint);
        return write_summary_files(memory_dir, &merged);
    }

    let created = ProjectContinuitySummary {
        project_key,
        project_cwd: normalized_cwd,
        goal,
        progress,
        verification,
        source_rollouts: if !rollout_ref.is_empty() {
            vec![rollout_ref]
        } else {
            Vec::new()
        },
        revision: 1,
        updated_at: Utc::now().to_rfc3339(),
        origin: ProjectSummaryOrigin::Generated,
        user_edited_sections: Vec::new(),
        deleted: false,
        deleted_fingerprint: None,
        last_source_fingerprint: Some(incoming_fingerprint),
        revision_history: Vec::new(),
    };
    write_summary_files(memory_dir, &created)
}

/// Roll back the derived project continuity summary to an earlier recorded revision
/// (or the immediately preceding revision when `target_revision` is `None`).
pub fn rollback_project_summary(
    memory_dir: &Path,
    project_cwd: Option<&str>,
    target_revision: Option<u64>,
    expected_revision: Option<u64>,
) -> Result<ProjectContinuitySummary> {
    let project_key = derive_project_key(project_cwd);
    let mut current = read_summary_raw(memory_dir, &project_key)?
        .with_context(|| format!("no continuity summary exists for project `{project_key}`"))?;
    if let Some(expected) = expected_revision
        && current.revision != expected
    {
        bail!(
            "project summary revision conflict: expected {}, current {}",
            expected,
            current.revision
        );
    }
    let snapshot = match target_revision {
        Some(rev) => current
            .revision_history
            .iter()
            .rev()
            .find(|entry| entry.revision == rev)
            .cloned()
            .with_context(|| format!("project summary revision {rev} not found in history"))?,
        None => current
            .revision_history
            .last()
            .cloned()
            .context("project summary has no prior revision to roll back to")?,
    };
    push_revision_snapshot(&mut current);
    current.goal = snapshot.goal;
    current.progress = snapshot.progress;
    current.verification = snapshot.verification;
    current.source_rollouts = snapshot.source_rollouts;
    current.origin = snapshot.origin;
    current.user_edited_sections = snapshot.user_edited_sections;
    current.deleted = snapshot.deleted;
    if !current.deleted {
        current.deleted_fingerprint = None;
    }
    current.revision = current.revision.saturating_add(1);
    current.updated_at = Utc::now().to_rfc3339();
    write_summary_files(memory_dir, &current)?;
    Ok(scrub_summary(current))
}

/// Record Root-committed child `memory_proposals` into `/summary` continuity state while
/// honoring `/summary` delete tombstones and user-edited section precedence.
pub(crate) fn record_memory_proposals_into_summary(
    memory_dir: &Path,
    project_cwd: Option<&str>,
    entries: &[crate::runtime::memory_proposals::CommittedMemoryProposal],
) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let project_key = derive_project_key(project_cwd);
    let normalized_cwd = project_cwd.and_then(normalize_project_cwd);
    let rollouts: Vec<String> = entries
        .iter()
        .map(|entry| sanitize_summary_text(&entry.provenance_rollout))
        .filter(|r| !r.is_empty())
        .collect();
    let proposal_lines = entries
        .iter()
        .map(|entry| format!("- [{}] {}", entry.kind.as_str(), entry.summary))
        .collect::<Vec<_>>()
        .join("\n");
    let incoming_fingerprint = fingerprint_summary_inputs("", &proposal_lines, "", &rollouts);

    let existing = read_summary_raw(memory_dir, &project_key)?;
    if let Some(current) = existing {
        if current.deleted
            && (current.deleted_fingerprint.as_deref() == Some(&incoming_fingerprint)
                || rollouts.iter().all(|r| current.source_rollouts.contains(r)))
        {
            return Ok(());
        }
        let mut merged = current.clone();
        push_revision_snapshot(&mut merged);
        if let Some(cwd) = normalized_cwd {
            merged.project_cwd = Some(cwd);
        }
        let user_sections: BTreeSet<&str> = current
            .user_edited_sections
            .iter()
            .map(String::as_str)
            .collect();
        let preserve_all =
            current.origin == ProjectSummaryOrigin::UserEdited && user_sections.contains("all");
        if !(current.origin == ProjectSummaryOrigin::UserEdited
            && (preserve_all || user_sections.contains("progress")))
        {
            let combined = if merged.progress.trim().is_empty()
                || merged.progress == "No progress recorded."
            {
                proposal_lines
            } else {
                format!("{}\n{}", merged.progress.trim(), proposal_lines)
            };
            merged.progress = sanitize_summary_text(&combined);
        }
        for rollout in rollouts {
            if !merged.source_rollouts.contains(&rollout) {
                merged.source_rollouts.push(rollout);
                if merged.source_rollouts.len() > MAX_SOURCE_ROLLOUTS {
                    let excess = merged.source_rollouts.len() - MAX_SOURCE_ROLLOUTS;
                    merged.source_rollouts.drain(0..excess);
                }
            }
        }
        merged.revision = merged.revision.saturating_add(1);
        merged.updated_at = Utc::now().to_rfc3339();
        merged.deleted = false;
        merged.deleted_fingerprint = None;
        merged.last_source_fingerprint = Some(incoming_fingerprint);
        return write_summary_files(memory_dir, &merged);
    }

    let created = ProjectContinuitySummary {
        project_key,
        project_cwd: normalized_cwd,
        goal: "Shared project workstream memory updated.".to_string(),
        progress: sanitize_summary_text(&proposal_lines),
        verification: "No verification status recorded.".to_string(),
        source_rollouts: rollouts.into_iter().take(MAX_SOURCE_ROLLOUTS).collect(),
        revision: 1,
        updated_at: Utc::now().to_rfc3339(),
        origin: ProjectSummaryOrigin::Generated,
        user_edited_sections: Vec::new(),
        deleted: false,
        deleted_fingerprint: None,
        last_source_fingerprint: Some(incoming_fingerprint),
        revision_history: Vec::new(),
    };
    write_summary_files(memory_dir, &created)
}

fn push_revision_snapshot(summary: &mut ProjectContinuitySummary) {
    if summary.revision == 0 {
        return;
    }
    let snapshot = ProjectSummaryRevisionSnapshot {
        revision: summary.revision,
        goal: summary.goal.clone(),
        progress: summary.progress.clone(),
        verification: summary.verification.clone(),
        source_rollouts: summary.source_rollouts.clone(),
        origin: summary.origin,
        user_edited_sections: summary.user_edited_sections.clone(),
        deleted: summary.deleted,
        updated_at: summary.updated_at.clone(),
    };
    summary.revision_history.push(snapshot);
    if summary.revision_history.len() > MAX_REVISION_HISTORY {
        let excess = summary.revision_history.len() - MAX_REVISION_HISTORY;
        summary.revision_history.drain(0..excess);
    }
}

/// Determine whether a markdown file under `<memory_dir>/topics/` is a suppressed or cross-project summary.
pub(crate) fn should_include_topic_markdown(
    memory_dir: &Path,
    markdown_file: &Path,
    active_project_cwd: Option<&str>,
) -> bool {
    let Some(file_name) = markdown_file.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(stem) = file_name
        .strip_prefix(SUMMARY_FILE_PREFIX)
        .and_then(|s| s.strip_suffix(".md"))
    else {
        return true;
    };
    if let Some(active_cwd) = active_project_cwd {
        let expected_key = derive_project_key(Some(active_cwd));
        if stem != expected_key {
            return false;
        }
    }
    match read_summary_raw(memory_dir, stem) {
        Ok(Some(summary)) => !summary.deleted,
        Ok(None) => true,
        Err(_) => false,
    }
}

/// Resolve the active summary markdown path for recall when present and non-deleted.
pub(crate) fn active_summary_markdown_path(
    memory_dir: &Path,
    active_project_cwd: Option<&str>,
) -> Option<PathBuf> {
    let key = derive_project_key(active_project_cwd);
    let md = markdown_path(memory_dir, &key);
    if !md.is_file() {
        return None;
    }
    should_include_topic_markdown(memory_dir, &md, active_project_cwd).then_some(md)
}

fn sanitize_summary_text(raw: &str) -> String {
    let scrubbed = redact_secret_tokens(raw.trim());
    let cleaned = scrubbed.trim();
    if cleaned.chars().count() <= MAX_SUMMARY_SECTION_CHARS {
        return cleaned.to_string();
    }
    let mut out: String = cleaned
        .chars()
        .take(MAX_SUMMARY_SECTION_CHARS.saturating_sub(3))
        .collect();
    out.push_str("...");
    out
}

fn scrub_summary(mut summary: ProjectContinuitySummary) -> ProjectContinuitySummary {
    summary.goal = sanitize_summary_text(&summary.goal);
    summary.progress = sanitize_summary_text(&summary.progress);
    summary.verification = sanitize_summary_text(&summary.verification);
    summary.source_rollouts = summary
        .source_rollouts
        .into_iter()
        .map(|s| sanitize_summary_text(&s))
        .filter(|s| !s.is_empty())
        .collect();
    summary
}

fn render_summary_markdown(summary: &ProjectContinuitySummary) -> String {
    let sources = if summary.source_rollouts.is_empty() {
        "- none recorded".to_string()
    } else {
        summary
            .source_rollouts
            .iter()
            .map(|id| format!("- {id}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let origin = match summary.origin {
        ProjectSummaryOrigin::Generated => "generated",
        ProjectSummaryOrigin::UserEdited => "user_edited",
    };
    format!(
        "# Project Continuity Summary\n\nproject_key: {}\nrevision: {}\norigin: {}\nupdated_at: {}\n\n> Derived continuity memory only. Non-authoritative; never alters Host mounts, grants, or policies.\n\n## Goal\n{}\n\n## Progress\n{}\n\n## Verification\n{}\n\n## Source Rollouts\n{}\n",
        summary.project_key,
        summary.revision,
        origin,
        summary.updated_at,
        summary.goal,
        summary.progress,
        summary.verification,
        sources,
    )
}

fn parse_raw_summary_edit(raw: &str) -> ProjectSummaryEdit {
    let mut edit = ProjectSummaryEdit::default();
    let mut current_field: Option<&str> = None;
    let mut buf: BTreeSet<&str> = BTreeSet::new();
    let _ = &mut buf;
    let mut goal_lines = Vec::new();
    let mut progress_lines = Vec::new();
    let mut verification_lines = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = trimmed
            .strip_prefix("goal:")
            .or_else(|| trimmed.strip_prefix("Goal:"))
        {
            current_field = Some("goal");
            if !rest.trim().is_empty() {
                goal_lines.push(rest.trim().to_string());
            }
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("progress:")
            .or_else(|| trimmed.strip_prefix("Progress:"))
        {
            current_field = Some("progress");
            if !rest.trim().is_empty() {
                progress_lines.push(rest.trim().to_string());
            }
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("verification:")
            .or_else(|| trimmed.strip_prefix("Verification:"))
        {
            current_field = Some("verification");
            if !rest.trim().is_empty() {
                verification_lines.push(rest.trim().to_string());
            }
            continue;
        }
        if lower == "## goal" {
            current_field = Some("goal");
            continue;
        }
        if lower == "## progress" {
            current_field = Some("progress");
            continue;
        }
        if lower == "## verification" {
            current_field = Some("verification");
            continue;
        }
        match current_field {
            Some("goal") => goal_lines.push(line.to_string()),
            Some("verification") => verification_lines.push(line.to_string()),
            _ => progress_lines.push(line.to_string()),
        }
    }

    let g = goal_lines.join("\n").trim().to_string();
    let p = progress_lines.join("\n").trim().to_string();
    let v = verification_lines.join("\n").trim().to_string();
    if !g.is_empty() {
        edit.goal = Some(g);
    }
    if !p.is_empty() {
        edit.progress = Some(p);
    }
    if !v.is_empty() {
        edit.verification = Some(v);
    }
    edit
}

fn fingerprint_summary_inputs(
    goal: &str,
    progress: &str,
    verification: &str,
    source_rollouts: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(goal.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(progress.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(verification.as_bytes());
    hasher.update(b"\x1f");
    for source in source_rollouts {
        hasher.update(source.as_bytes());
        hasher.update(b"\x1e");
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(24);
    for byte in digest.iter().take(12) {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
#[path = "project_summary_tests.rs"]
mod tests;
