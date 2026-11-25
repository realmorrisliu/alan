//! System Store project binding, workstream ledger, and workflow inbox persistence.
//!
//! Every project's durable state lives under `<projects_root>/<opaque_project_key>/`:
//! - `binding.json` — explicit canonical directory binding (`ProjectBinding`)
//! - `ledger.json` — bounded index of named workstreams (`ProjectLedger`)
//! - `streams/<name>.json` — individual workstream metadata (`WorkstreamRecord`)
//! - `inbox/<run_id>.json` — bounded, secret-scrubbed scheduled workflow digests (`InboxDigestEntry`)

pub mod binding;
pub mod inbox;
pub mod lease;
pub mod ledger;
#[cfg(test)]
mod tests;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

pub use binding::{
    PROJECT_BINDING_VERSION, ProjectBinding, bind_project_dir, binding_path, list_project_bindings,
    load_project_binding, project_dir, resolve_bound_project, validate_project_key,
};
pub use inbox::{
    INBOX_DIGEST_VERSION, InboxDigestEntry, InboxDigestStatus, MAX_INBOX_SUMMARY_CHARS, inbox_dir,
    inbox_entry_path, list_inbox_digests, list_unread_inbox_digests, load_inbox_digest,
    mark_all_inbox_digests_digested, mark_inbox_digest_digested, scrub_inbox_summary,
    write_inbox_digest,
};
pub use lease::{
    StreamLeaseAcquireResult, StreamLeaseGuard, StreamLeaseHolderKind, StreamLeaseMetadata,
};
pub use ledger::{
    CronStreamPolicy, PROJECT_LEDGER_VERSION, ProjectLedger, WORKSTREAM_RECORD_VERSION,
    WorkstreamCounts, WorkstreamPersistence, WorkstreamRecord, WorkstreamState,
    WorkstreamWriteMode, archive_workstream, complete_workstream, idle_workstream, ledger_path,
    load_project_ledger, load_workstream_record, pause_workstream_error, resume_workstream,
    stream_path, streams_dir, upsert_workstream, workstream_counts,
};

const MAX_IDENTIFIER_LEN: usize = 128;
const STREAMS_DIR_NAME: &str = "streams";

pub(super) fn validate_identifier(kind: &str, value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= MAX_IDENTIFIER_LEN,
        "{kind} `{value}` must be 1..={MAX_IDENTIFIER_LEN} characters"
    );
    ensure!(
        value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric()),
        "{kind} `{value}` must start with an ASCII alphanumeric character"
    );
    ensure!(
        value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "{kind} `{value}` contains invalid characters (allowed: [a-zA-Z0-9_-])"
    );
    Ok(())
}

pub(super) fn validate_projects_root(projects_root: &Path) -> Result<()> {
    ensure!(
        !projects_root
            .components()
            .any(|component| matches!(component, Component::ParentDir)),
        "projects root must not contain `..`: {}",
        projects_root.display()
    );
    Ok(())
}

pub(super) fn ensure_real_dir(path: &Path, label: &str) -> Result<()> {
    let meta = fs::symlink_metadata(path)
        .with_context(|| format!("inspect {label} {}", path.display()))?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "{label} must be a real directory without symlinks: {}",
        path.display()
    );
    Ok(())
}

pub(super) fn ensure_projects_subdir(projects_root: &Path, segments: &[&str]) -> Result<PathBuf> {
    validate_projects_root(projects_root)?;
    if fs::symlink_metadata(projects_root).is_err() {
        fs::create_dir_all(projects_root)
            .with_context(|| format!("create projects root {}", projects_root.display()))?;
    }
    ensure_real_dir(projects_root, "projects root")?;
    let canonical_root = fs::canonicalize(projects_root)
        .with_context(|| format!("canonicalize projects root {}", projects_root.display()))?;

    let mut cursor = projects_root.to_path_buf();
    for segment in segments {
        cursor.push(segment);
        match fs::symlink_metadata(&cursor) {
            Ok(meta) => {
                ensure!(
                    meta.is_dir() && !meta.file_type().is_symlink(),
                    "project store path must be a real directory without symlinks: {}",
                    cursor.display()
                );
                let canonical_cursor = fs::canonicalize(&cursor)
                    .with_context(|| format!("canonicalize {}", cursor.display()))?;
                ensure!(
                    canonical_cursor.starts_with(&canonical_root),
                    "project store path escapes projects root: {}",
                    cursor.display()
                );
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&cursor)
                    .with_context(|| format!("create directory {}", cursor.display()))?;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("inspect {}", cursor.display()));
            }
        }
    }
    Ok(cursor)
}

pub(super) fn verify_existing_subdir(
    projects_root: &Path,
    segments: &[&str],
) -> Result<Option<PathBuf>> {
    validate_projects_root(projects_root)?;
    match fs::symlink_metadata(projects_root) {
        Ok(_) => ensure_real_dir(projects_root, "projects root")?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("inspect projects root {}", projects_root.display()));
        }
    }
    let canonical_root = fs::canonicalize(projects_root)
        .with_context(|| format!("canonicalize projects root {}", projects_root.display()))?;

    let mut cursor = projects_root.to_path_buf();
    for segment in segments {
        cursor.push(segment);
        match fs::symlink_metadata(&cursor) {
            Ok(meta) => {
                ensure!(
                    meta.is_dir() && !meta.file_type().is_symlink(),
                    "project store path must be a real directory without symlinks: {}",
                    cursor.display()
                );
                let canonical_cursor = fs::canonicalize(&cursor)
                    .with_context(|| format!("canonicalize {}", cursor.display()))?;
                ensure!(
                    canonical_cursor.starts_with(&canonical_root),
                    "project store path escapes projects root: {}",
                    cursor.display()
                );
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(err).with_context(|| format!("inspect {}", cursor.display()));
            }
        }
    }
    Ok(Some(cursor))
}

pub(super) fn verify_existing_regular_file(parent_dir: &Path, file_path: &Path) -> Result<bool> {
    match fs::symlink_metadata(file_path) {
        Ok(meta) => {
            ensure!(
                meta.file_type().is_file() && !meta.file_type().is_symlink(),
                "refusing symlink or non-regular file {}",
                file_path.display()
            );
            let canonical_parent = fs::canonicalize(parent_dir)
                .with_context(|| format!("canonicalize {}", parent_dir.display()))?;
            let canonical_file = fs::canonicalize(file_path)
                .with_context(|| format!("canonicalize {}", file_path.display()))?;
            ensure!(
                canonical_file.starts_with(&canonical_parent),
                "file escapes project store directory: {}",
                file_path.display()
            );
            Ok(true)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| format!("inspect {}", file_path.display())),
    }
}

pub(super) fn atomic_write_json<T: Serialize>(
    parent_dir: &Path,
    target_path: &Path,
    value: &T,
) -> Result<()> {
    let _ = verify_existing_regular_file(parent_dir, target_path)?;
    let mut bytes = serde_json::to_vec_pretty(value)
        .with_context(|| format!("serialize JSON for {}", target_path.display()))?;
    bytes.push(b'\n');

    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("target path missing filename")?;
    let temp_path = parent_dir.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));

    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .with_context(|| format!("create temp file {}", temp_path.display()))?;
        file.write_all(&bytes)
            .with_context(|| format!("write temp file {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("sync temp file {}", temp_path.display()))?;
        drop(file);
        fs::rename(&temp_path, target_path).with_context(|| {
            format!(
                "atomically rename {} -> {}",
                temp_path.display(),
                target_path.display()
            )
        })?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&temp_path);
        bail!(err);
    }
    Ok(())
}
