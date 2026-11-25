//! Explicit 3-way conflict-checked sync-back (`base` vs `workspace` vs `host_now`).

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use super::gc::now_ms;
use super::manifest::{ManifestEntry, TreeManifest, same_optional_entry};
use super::types::{
    ConflictReason, CowWorkspaceError, IntentProvenance, SyncBackReport, SyncConflict,
    WorkspaceBacking, WorkspaceRef,
};
use crate::paths::SystemStorePaths;

enum SyncAction {
    Put { rel: String, entry: ManifestEntry },
    Delete { rel: String },
}

#[derive(Serialize)]
struct SyncAuditRecord<'a> {
    audit_record_id: &'a str,
    workspace_ref: &'a WorkspaceRef,
    backing_kind: WorkspaceBacking,
    timestamp_ms: u128,
    workstream: &'a str,
    rollout_ref: &'a str,
    why: &'a str,
    applied_paths: &'a [String],
    deleted_paths: &'a [String],
}

#[allow(
    clippy::too_many_arguments,
    reason = "3-way sync-back takes explicit base, workspace, host, audit store, and provenance inputs"
)]
pub(crate) fn perform_sync_back(
    workspace_ref: &WorkspaceRef,
    backing_kind: WorkspaceBacking,
    base: &TreeManifest,
    workspace_tree: &Path,
    host_root: &Path,
    excludes: &[String],
    system_store: &SystemStorePaths,
    provenance: &IntentProvenance,
) -> std::result::Result<SyncBackReport, CowWorkspaceError> {
    let workspace_now = TreeManifest::scan(workspace_tree, excludes)?;
    let host_now = TreeManifest::scan(host_root, excludes)?;

    let mut all_paths: BTreeSet<&String> = BTreeSet::new();
    all_paths.extend(base.entries.keys());
    all_paths.extend(workspace_now.entries.keys());
    all_paths.extend(host_now.entries.keys());

    let mut actions = Vec::new();
    let mut conflicts = Vec::new();

    for rel in all_paths {
        let b = base.entries.get(rel);
        let w = workspace_now.entries.get(rel);
        let h = host_now.entries.get(rel);

        if same_optional_entry(w, b) || same_optional_entry(w, h) {
            continue;
        }
        if same_optional_entry(h, b) {
            match w {
                Some(entry) => actions.push(SyncAction::Put {
                    rel: rel.clone(),
                    entry: entry.clone(),
                }),
                None => actions.push(SyncAction::Delete { rel: rel.clone() }),
            }
            continue;
        }

        let reason = match (b, w, h) {
            (None, Some(_), Some(_)) => ConflictReason::CreatedInBoth,
            (Some(_), None, Some(_)) => ConflictReason::DeletedInWorkspaceModifiedOnHost,
            (Some(_), Some(_), None) => ConflictReason::ModifiedInWorkspaceDeletedOnHost,
            _ => ConflictReason::ModifiedInBoth,
        };
        conflicts.push(SyncConflict {
            path: rel.clone(),
            reason,
        });
    }

    if !conflicts.is_empty() {
        return Err(CowWorkspaceError::SyncConflict { conflicts });
    }

    // Pre-validate every destination path on Host before mutating any file.
    for action in &actions {
        let rel = match action {
            SyncAction::Put { rel, .. } | SyncAction::Delete { rel } => rel,
        };
        validate_host_target_safe(host_root, rel)?;
    }

    let mut applied_paths = Vec::new();
    let mut deleted_paths = Vec::new();

    for action in actions {
        match action {
            SyncAction::Put { rel, entry } => {
                apply_put(workspace_tree, host_root, &rel, &entry)?;
                applied_paths.push(rel);
            }
            SyncAction::Delete { rel } => {
                let target = host_root.join(&rel);
                if target.symlink_metadata().is_ok() {
                    fs::remove_file(&target)?;
                }
                deleted_paths.push(rel);
            }
        }
    }

    let audit_record_id = format!("sync-{}", uuid::Uuid::new_v4());
    append_sync_audit(
        system_store,
        &SyncAuditRecord {
            audit_record_id: &audit_record_id,
            workspace_ref,
            backing_kind,
            timestamp_ms: now_ms(),
            workstream: &provenance.workstream,
            rollout_ref: &provenance.rollout_ref,
            why: &provenance.why,
            applied_paths: &applied_paths,
            deleted_paths: &deleted_paths,
        },
    )?;

    Ok(SyncBackReport {
        workspace_ref: workspace_ref.clone(),
        applied_paths,
        deleted_paths,
        audit_record_id,
        provenance: provenance.clone(),
    })
}

fn validate_host_target_safe(host_root: &Path, rel: &str) -> Result<()> {
    let rel_path = Path::new(rel);
    ensure!(
        !rel_path.is_absolute(),
        "sync-back relative path must not be absolute: {rel}"
    );
    let mut current = host_root.to_path_buf();
    let mut components = rel_path.components().peekable();
    while let Some(component) = components.next() {
        let std::path::Component::Normal(part) = component else {
            anyhow::bail!("sync-back path contains invalid component: {rel}");
        };
        current.push(part);
        let is_leaf = components.peek().is_none();
        if !is_leaf && let Ok(meta) = fs::symlink_metadata(&current) {
            ensure!(
                !meta.file_type().is_symlink(),
                "refusing to traverse symlinked directory during sync-back: {}",
                current.display()
            );
            ensure!(
                meta.is_dir(),
                "non-directory parent encountered during sync-back: {}",
                current.display()
            );
        }
    }
    Ok(())
}

fn apply_put(
    workspace_tree: &Path,
    host_root: &Path,
    rel: &str,
    entry: &ManifestEntry,
) -> Result<()> {
    let dst = host_root.join(rel);
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent directory {}", parent.display()))?;
    }

    match entry {
        ManifestEntry::File { mode, .. } => {
            let src = workspace_tree.join(rel);
            let bytes =
                fs::read(&src).with_context(|| format!("read workspace file {}", src.display()))?;
            let tmp_name = format!(
                ".alan-cow-sync-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            );
            let tmp = dst.with_file_name(tmp_name);
            {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&tmp)
                    .with_context(|| format!("create temp sync file {}", tmp.display()))?;
                file.write_all(&bytes)?;
                file.flush()?;
            }
            fs::set_permissions(&tmp, fs::Permissions::from_mode(*mode))?;
            if let Ok(meta) = fs::symlink_metadata(&dst)
                && meta.file_type().is_symlink()
            {
                fs::remove_file(&dst)?;
            }
            fs::rename(&tmp, &dst)
                .with_context(|| format!("replace host file {}", dst.display()))?;
        }
        ManifestEntry::Symlink { target } => {
            if let Ok(meta) = fs::symlink_metadata(&dst)
                && (meta.file_type().is_symlink() || meta.is_file())
            {
                fs::remove_file(&dst)?;
            }
            std::os::unix::fs::symlink(target, &dst)
                .with_context(|| format!("create symlink {}", dst.display()))?;
        }
    }
    Ok(())
}

fn append_sync_audit(system_store: &SystemStorePaths, record: &SyncAuditRecord<'_>) -> Result<()> {
    let dir = system_store.service("host-mount")?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("cow-sync-audit.jsonl");
    let mut line = serde_json::to_string(record).context("serialize sync audit record")?;
    line.push('\n');
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(line.as_bytes())?;
    file.flush()?;
    Ok(())
}
