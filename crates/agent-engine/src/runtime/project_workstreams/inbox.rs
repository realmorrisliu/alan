//! Project `inbox/` digest persistence for scheduled `alan workflow` runs.
//!
//! Digests live under `<projects_root>/<opaque_project_key>/inbox/<run_id>.json`,
//! have bounded secret-scrubbed summaries via [`crate::redact_secret_tokens`],
//! and track acknowledgement via `digested_at`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::redact_secret_tokens;

use super::{
    atomic_write_json, ensure_projects_subdir, validate_identifier, validate_project_key,
    validate_projects_root, verify_existing_regular_file, verify_existing_subdir,
};

pub const INBOX_DIGEST_VERSION: u16 = 1;
pub const MAX_INBOX_SUMMARY_CHARS: usize = 1_200;

const INBOX_DIR_NAME: &str = "inbox";

const fn default_inbox_version() -> u16 {
    INBOX_DIGEST_VERSION
}

/// Outcome severity of a scheduled workflow run recorded in `inbox/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxDigestStatus {
    Ok,
    Warning,
    PausedError,
}

/// Bounded, secret-scrubbed scheduled workflow digest persisted at `inbox/<run_id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxDigestEntry {
    #[serde(default = "default_inbox_version")]
    pub version: u16,
    pub run_id: String,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_name: Option<String>,
    pub status: InboxDigestStatus,
    pub summary: String,
    pub evidence_ref: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digested_at: Option<String>,
}

impl InboxDigestEntry {
    /// Construct a new unread `InboxDigestEntry` with the current timestamp and scrubbed summary.
    pub fn new(
        run_id: impl Into<String>,
        workflow_id: impl Into<String>,
        stream_name: Option<String>,
        status: InboxDigestStatus,
        summary: impl AsRef<str>,
        evidence_ref: impl Into<String>,
    ) -> Self {
        Self {
            version: INBOX_DIGEST_VERSION,
            run_id: run_id.into(),
            workflow_id: workflow_id.into(),
            stream_name,
            status,
            summary: scrub_inbox_summary(summary.as_ref()),
            evidence_ref: evidence_ref.into(),
            created_at: Utc::now().to_rfc3339(),
            digested_at: None,
        }
    }
}

/// Scrub secrets using [`crate::redact_secret_tokens`] and bound to [`MAX_INBOX_SUMMARY_CHARS`].
pub fn scrub_inbox_summary(raw: &str) -> String {
    let scrubbed = redact_secret_tokens(raw.trim());
    let cleaned = scrubbed.trim();
    if cleaned.chars().count() <= MAX_INBOX_SUMMARY_CHARS {
        return cleaned.to_string();
    }
    let mut out: String = cleaned
        .chars()
        .take(MAX_INBOX_SUMMARY_CHARS.saturating_sub(3))
        .collect();
    out.push_str("...");
    out
}

/// Return `<projects_root>/<project_key>/inbox`.
pub fn inbox_dir(projects_root: &Path, project_key: &str) -> Result<PathBuf> {
    validate_projects_root(projects_root)?;
    validate_project_key(project_key)?;
    Ok(projects_root.join(project_key).join(INBOX_DIR_NAME))
}

/// Return `<projects_root>/<project_key>/inbox/<run_id>.json`.
pub fn inbox_entry_path(projects_root: &Path, project_key: &str, run_id: &str) -> Result<PathBuf> {
    validate_identifier("run_id", run_id)?;
    Ok(inbox_dir(projects_root, project_key)?.join(format!("{run_id}.json")))
}

/// Atomically write an `InboxDigestEntry` under `inbox/<run_id>.json` after scrubbing secrets.
pub fn write_inbox_digest(
    projects_root: &Path,
    project_key: &str,
    mut entry: InboxDigestEntry,
) -> Result<InboxDigestEntry> {
    validate_project_key(project_key)?;
    validate_identifier("run_id", &entry.run_id)?;
    ensure!(
        !entry.workflow_id.trim().is_empty(),
        "inbox digest workflow_id must not be empty"
    );
    if let Some(stream) = entry.stream_name.as_deref() {
        validate_identifier("stream_name", stream)?;
    }

    entry.version = INBOX_DIGEST_VERSION;
    entry.summary = scrub_inbox_summary(&entry.summary);
    entry.evidence_ref = entry.evidence_ref.trim().to_string();
    if entry.created_at.trim().is_empty() {
        entry.created_at = Utc::now().to_rfc3339();
    }

    let dir_path = ensure_projects_subdir(projects_root, &[project_key, INBOX_DIR_NAME])?;
    let path = dir_path.join(format!("{}.json", entry.run_id));
    atomic_write_json(&dir_path, &path, &entry)?;
    Ok(entry)
}

/// Load a single inbox digest entry by `run_id` if present.
pub fn load_inbox_digest(
    projects_root: &Path,
    project_key: &str,
    run_id: &str,
) -> Result<Option<InboxDigestEntry>> {
    validate_project_key(project_key)?;
    validate_identifier("run_id", run_id)?;
    let Some(dir_path) = verify_existing_subdir(projects_root, &[project_key, INBOX_DIR_NAME])?
    else {
        return Ok(None);
    };
    let path = dir_path.join(format!("{run_id}.json"));
    if !verify_existing_regular_file(&dir_path, &path)? {
        return Ok(None);
    }
    read_digest_file(&path, run_id).map(Some)
}

/// List all inbox digest entries for `project_key`, sorted deterministically by `(created_at, run_id)`.
pub fn list_inbox_digests(
    projects_root: &Path,
    project_key: &str,
) -> Result<Vec<InboxDigestEntry>> {
    validate_project_key(project_key)?;
    let Some(dir_path) = verify_existing_subdir(projects_root, &[project_key, INBOX_DIR_NAME])?
    else {
        return Ok(Vec::new());
    };

    let mut entries = Vec::new();
    for item in
        fs::read_dir(&dir_path).with_context(|| format!("read inbox dir {}", dir_path.display()))?
    {
        let item = item?;
        let file_type = item.file_type()?;
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        let Some(file_name) = item.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(run_id) = file_name.strip_suffix(".json") else {
            continue;
        };
        if validate_identifier("run_id", run_id).is_err() {
            continue;
        }
        let path = dir_path.join(&file_name);
        if verify_existing_regular_file(&dir_path, &path)? {
            entries.push(read_digest_file(&path, run_id)?);
        }
    }
    entries.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.run_id.cmp(&b.run_id))
    });
    Ok(entries)
}

/// List only unread (`digested_at.is_none()`) inbox digest entries in chronological order.
pub fn list_unread_inbox_digests(
    projects_root: &Path,
    project_key: &str,
) -> Result<Vec<InboxDigestEntry>> {
    Ok(list_inbox_digests(projects_root, project_key)?
        .into_iter()
        .filter(|entry| entry.digested_at.is_none())
        .collect())
}

/// Mark a single inbox digest entry as digested by recording `digested_at`.
/// Preserves an existing `digested_at` timestamp if already acknowledged.
pub fn mark_inbox_digest_digested(
    projects_root: &Path,
    project_key: &str,
    run_id: &str,
) -> Result<InboxDigestEntry> {
    let mut entry = load_inbox_digest(projects_root, project_key, run_id)?
        .with_context(|| format!("inbox digest `{run_id}` not found in project `{project_key}`"))?;
    if entry.digested_at.is_none() {
        entry.digested_at = Some(Utc::now().to_rfc3339());
        return write_inbox_digest(projects_root, project_key, entry);
    }
    Ok(entry)
}

/// Mark every unread inbox digest entry for `project_key` as digested, returning the updated entries.
pub fn mark_all_inbox_digests_digested(
    projects_root: &Path,
    project_key: &str,
) -> Result<Vec<InboxDigestEntry>> {
    let unread = list_unread_inbox_digests(projects_root, project_key)?;
    if unread.is_empty() {
        return Ok(Vec::new());
    }
    let timestamp = Utc::now().to_rfc3339();
    let mut updated = Vec::with_capacity(unread.len());
    for mut entry in unread {
        entry.digested_at = Some(timestamp.clone());
        updated.push(write_inbox_digest(projects_root, project_key, entry)?);
    }
    Ok(updated)
}

fn read_digest_file(path: &Path, expected_run_id: &str) -> Result<InboxDigestEntry> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let entry = serde_json::from_slice::<InboxDigestEntry>(&bytes)
        .with_context(|| format!("parse {}", path.display()))?;
    ensure!(
        entry.run_id == expected_run_id,
        "inbox digest run_id mismatch in {}: expected `{expected_run_id}`, found `{}`",
        path.display(),
        entry.run_id
    );
    Ok(entry)
}
