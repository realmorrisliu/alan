//! Startup dead-PID orphan garbage collection and instance metadata locks.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::clone::remove_cloned_tree;
use super::types::{GcReport, WorkspaceBacking};

const INSTANCE_META_FILE: &str = "instance.json";
pub(crate) const WORKSPACE_META_FILE: &str = "metadata.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct InstanceMetadata {
    pub(crate) instance_id: String,
    pub(crate) pid: u32,
    pub(crate) created_at_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WorkspaceMetadata {
    pub(crate) workspace_ref: String,
    pub(crate) backing_kind: WorkspaceBacking,
    pub(crate) host_root: PathBuf,
    pub(crate) tree_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sibling_root: Option<PathBuf>,
    pub(crate) source_is_git: bool,
    pub(crate) created_at_ms: u128,
    pub(crate) applied_to_host: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) projected_grant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) projected_namespace_path: Option<String>,
    #[serde(default)]
    pub(crate) excludes: Vec<String>,
}

pub(crate) fn write_instance_metadata(instance_dir: &Path, instance_id: &str) -> Result<()> {
    fs::create_dir_all(instance_dir)
        .with_context(|| format!("create instance workspace dir {}", instance_dir.display()))?;
    let meta_path = instance_dir.join(INSTANCE_META_FILE);
    let meta = InstanceMetadata {
        instance_id: instance_id.to_string(),
        pid: std::process::id(),
        created_at_ms: now_ms(),
    };
    let encoded = serde_json::to_vec_pretty(&meta).context("serialize instance metadata")?;
    let tmp_path = instance_dir.join(format!(".{INSTANCE_META_FILE}.tmp-{}", std::process::id()));
    fs::write(&tmp_path, encoded)?;
    fs::rename(&tmp_path, &meta_path)?;
    Ok(())
}

pub(crate) fn write_workspace_metadata(
    workspace_dir: &Path,
    meta: &WorkspaceMetadata,
) -> Result<()> {
    let meta_path = workspace_dir.join(WORKSPACE_META_FILE);
    let encoded = serde_json::to_vec_pretty(meta).context("serialize workspace metadata")?;
    let tmp_path = workspace_dir.join(format!(".{WORKSPACE_META_FILE}.tmp-{}", std::process::id()));
    fs::write(&tmp_path, encoded)?;
    fs::rename(&tmp_path, &meta_path)?;
    Ok(())
}

/// Sweep crashed-instance directories under `workspaces_root` whose PIDs are no longer alive.
pub(crate) fn gc_orphaned_instances(
    workspaces_root: &Path,
    current_instance_id: &str,
) -> Result<GcReport> {
    let mut report = GcReport::default();
    if !workspaces_root.exists() {
        return Ok(report);
    }

    for entry in fs::read_dir(workspaces_root)
        .with_context(|| format!("read workspaces root {}", workspaces_root.display()))?
    {
        let entry = entry?;
        let instance_dir = entry.path();
        if !instance_dir.is_dir() {
            continue;
        }
        let Some(name) = instance_dir.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == current_instance_id {
            continue;
        }

        let meta_path = instance_dir.join(INSTANCE_META_FILE);
        let Ok(raw) = fs::read(&meta_path) else {
            // Do not blindly wipe unknown directories that lack an instance lock file.
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<InstanceMetadata>(&raw) else {
            continue;
        };

        if is_pid_alive(meta.pid) {
            report.skipped_live_instances.push(meta.instance_id);
            continue;
        }

        let mut swept_count = 0usize;
        if let Ok(ws_entries) = fs::read_dir(&instance_dir) {
            for ws_entry in ws_entries.flatten() {
                let ws_dir = ws_entry.path();
                if !ws_dir.is_dir() {
                    continue;
                }
                let ws_meta_path = ws_dir.join(WORKSPACE_META_FILE);
                if let Ok(ws_raw) = fs::read(&ws_meta_path)
                    && let Ok(ws_meta) = serde_json::from_slice::<WorkspaceMetadata>(&ws_raw)
                {
                    let _ = remove_cloned_tree(
                        &ws_meta.host_root,
                        &ws_meta.tree_path,
                        ws_meta.backing_kind,
                        ws_meta.sibling_root.as_deref(),
                    );
                }
                swept_count += 1;
            }
        }

        fs::remove_dir_all(&instance_dir).with_context(|| {
            format!(
                "remove orphaned workspace instance dir {}",
                instance_dir.display()
            )
        })?;
        report.swept_instances.push(meta.instance_id);
        report.swept_workspaces += swept_count;
    }

    report.swept_instances.sort();
    report.skipped_live_instances.sort();
    Ok(report)
}

pub(crate) fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(pid_t) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: `kill(pid, 0)` performs existence/permission check without sending a signal.
    let rc = unsafe { libc::kill(pid_t, 0) };
    if rc == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    )
}

pub(crate) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
