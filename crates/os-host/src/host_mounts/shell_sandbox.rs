use std::path::{Path, PathBuf};

use alan_agent_engine::tools::{ReifiedMountAccess, Sandbox, SandboxHostMount, SandboxSpec};
use alan_service_manager::HostMountAccess;
use anyhow::{Context, Result};

use super::{NativeToolMount, longest_namespace_mount};

pub(super) fn sandbox_host_mounts(mounts: &[NativeToolMount]) -> Vec<SandboxHostMount> {
    mounts
        .iter()
        .map(|mount| SandboxHostMount {
            namespace_path: mount.namespace_path.clone(),
            host_path: mount.host_path.clone(),
            access: match mount.access {
                HostMountAccess::ReadOnly => ReifiedMountAccess::ReadOnly,
                HostMountAccess::ReadWrite => ReifiedMountAccess::ReadWrite,
            },
        })
        .collect()
}

/// Build a shell sandbox confined to `selected` (plus read-only sibling grants) while excluding
/// every other `ReadWrite` Host Mount backing root.
pub(super) fn build_shell_sandbox_for_mount(
    mounts: &[NativeToolMount],
    selected: &NativeToolMount,
) -> Sandbox {
    let sandbox_mounts = sandbox_host_mounts(mounts);
    let shell_mount = sandbox_mounts
        .iter()
        .find(|mount| mount.namespace_path == selected.namespace_path)
        .expect("selected grant is in the native projection");
    // Keep the cwd grant first for command/runtime inspection.
    let mut shell_mounts = vec![shell_mount.clone()];
    shell_mounts.extend(
        sandbox_mounts
            .iter()
            .filter(|mount| {
                mount.access == ReifiedMountAccess::ReadOnly
                    && mount.namespace_path != selected.namespace_path
            })
            .cloned(),
    );
    let excluded_roots = mounts
        .iter()
        .filter(|mount| {
            mount.access == HostMountAccess::ReadWrite
                && !mount.host_path.starts_with(&selected.host_path)
        })
        .map(|mount| mount.host_path.clone())
        .collect();
    Sandbox::from_spec(SandboxSpec::from_host_mounts(&shell_mounts))
        .with_excluded_host_roots(excluded_roots)
}

pub(super) fn resolve_shell_execution_for_directory(
    mounts: &[NativeToolMount],
    resolved_namespace_dir: &Path,
) -> Result<(PathBuf, Sandbox)> {
    anyhow::ensure!(
        resolved_namespace_dir != Path::new("/"),
        "Process cwd `/` is outside delegated Host Mounts (available mounts: {})",
        format_available_mounts(mounts)
    );
    let selected = longest_namespace_mount(mounts, resolved_namespace_dir).with_context(|| {
        format!(
            "directory {} is outside delegated Host Mounts (available mounts: {})",
            resolved_namespace_dir.display(),
            format_available_mounts(mounts)
        )
    })?;
    let suffix = resolved_namespace_dir
        .strip_prefix(&selected.namespace_path)
        .expect("selected Host Mount is a namespace prefix");
    let host_dir = selected.host_path.join(suffix);
    anyhow::ensure!(
        host_dir.is_dir(),
        "{} is not a directory",
        resolved_namespace_dir.display()
    );
    Ok((host_dir, build_shell_sandbox_for_mount(mounts, selected)))
}

pub(super) fn format_available_mounts(mounts: &[NativeToolMount]) -> String {
    if mounts.is_empty() {
        return "none".to_string();
    }
    mounts
        .iter()
        .map(|mount| mount.namespace_path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
