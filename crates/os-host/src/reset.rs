//! Reset Alan-owned System Store and Host Store data safely.

use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, ensure};

use crate::paths::{HostStorePaths, SystemStorePaths};

/// Scope of an Alan store reset operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetScope {
    /// Clear connection profiles, Host secrets/OAuth, saved project grants,
    /// shell UI history, updater cache, and the active Root rollout selector.
    /// Keeps recorded rollouts and Memory Stores intact.
    Settings,
    /// Factory reset: clear all Alan System Store services and Host Store
    /// credentials/OAuth, returning Alan to clean first-run state.
    All,
}

/// Summary of Alan-owned paths cleared by [`reset_alan_stores`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetReport {
    pub scope: ResetScope,
    pub removed_paths: Vec<PathBuf>,
}

/// Clear Alan-owned configuration and state under `system` and `host`.
///
/// Only touches paths owned inside `SystemStorePaths` and `HostStorePaths`.
/// Never touches executables, user project directories, or external tool
/// credentials such as `~/.config/gcloud`.
pub fn reset_alan_stores(
    system: &SystemStorePaths,
    host: &HostStorePaths,
    scope: ResetScope,
) -> Result<ResetReport> {
    validate_safe_dir("System Store root", &system.root)?;
    validate_safe_dir("Host Store credentials", &host.credentials)?;
    validate_safe_dir("Host Store managed auth", &host.managed_auth)?;

    let mut targets: Vec<PathBuf> = match scope {
        ResetScope::Settings => vec![
            system.service("connections")?,
            system.service("host-mount")?,
            system.service("shell-ui")?,
            system.service("updater")?,
            system.agent_runtime()?.metadata.join("root-rollout"),
            host.credentials.clone(),
            host.managed_auth.clone(),
        ],
        ResetScope::All => vec![
            system.root.join("services"),
            host.credentials.clone(),
            host.managed_auth.clone(),
        ],
    };

    if let Some(host_root) = host.managed_auth.parent() {
        for lock_name in ["auth.json.lock", "auth.refresh.lock"] {
            targets.push(host_root.join(lock_name));
        }
    }

    let mut removed_paths = Vec::new();
    for path in targets {
        if remove_target_if_exists(&path)? {
            removed_paths.push(path);
        }
    }

    Ok(ResetReport {
        scope,
        removed_paths,
    })
}

fn validate_safe_dir(label: &str, path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "{label} must be absolute: {}",
        path.display()
    );
    ensure!(
        !path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir)),
        "{label} must not contain relative components: {}",
        path.display()
    );
    Ok(())
}

fn remove_target_if_exists(path: &Path) -> Result<bool> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    if meta.is_dir() {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err.into()),
        }
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_stores(root: &Path) -> (SystemStorePaths, HostStorePaths) {
        let system = SystemStorePaths::from_data_dir(root).unwrap();
        let host = HostStorePaths::from_data_dir(root).unwrap();

        std::fs::create_dir_all(system.service("connections").unwrap()).unwrap();
        std::fs::write(system.connections_metadata().unwrap(), "version = 1\n").unwrap();

        std::fs::create_dir_all(system.service("host-mount").unwrap()).unwrap();
        std::fs::write(system.host_mount_grants().unwrap(), "{}").unwrap();

        std::fs::create_dir_all(system.service("shell-ui").unwrap()).unwrap();
        std::fs::write(
            system.service("shell-ui").unwrap().join("composer-history"),
            "hi\n",
        )
        .unwrap();

        std::fs::create_dir_all(system.service("updater").unwrap()).unwrap();
        std::fs::write(
            system.service("updater").unwrap().join("last-check.json"),
            "{}",
        )
        .unwrap();

        let runtime = system.agent_runtime().unwrap();
        std::fs::create_dir_all(&runtime.rollouts).unwrap();
        std::fs::create_dir_all(&runtime.metadata).unwrap();
        std::fs::write(runtime.rollouts.join("rollout-1.jsonl"), "{}").unwrap();
        std::fs::write(runtime.metadata.join("root-rollout"), "rollout-1").unwrap();

        let mem_default = system.memory_stores().unwrap().join("default");
        std::fs::create_dir_all(&mem_default).unwrap();
        std::fs::write(mem_default.join("USER.md"), "notes").unwrap();

        std::fs::create_dir_all(&host.credentials).unwrap();
        std::fs::write(host.credentials.join("secrets.toml"), "secret").unwrap();
        std::fs::write(&host.managed_auth, "{}").unwrap();
        std::fs::write(
            host.managed_auth.parent().unwrap().join("auth.json.lock"),
            "",
        )
        .unwrap();

        (system, host)
    }

    #[test]
    fn reset_settings_clears_connections_secrets_and_ui_while_preserving_rollouts_and_memory() {
        let temp = tempfile::tempdir().unwrap();
        let (system, host) = seed_stores(temp.path());

        let report = reset_alan_stores(&system, &host, ResetScope::Settings).unwrap();
        assert_eq!(report.scope, ResetScope::Settings);
        assert!(!system.connections_metadata().unwrap().exists());
        assert!(!system.host_mount_grants().unwrap().exists());
        assert!(!system.service("shell-ui").unwrap().exists());
        assert!(!system.service("updater").unwrap().exists());
        assert!(
            !system
                .agent_runtime()
                .unwrap()
                .metadata
                .join("root-rollout")
                .exists()
        );
        assert!(!host.credentials.exists());
        assert!(!host.managed_auth.exists());
        assert!(
            !host
                .managed_auth
                .parent()
                .unwrap()
                .join("auth.json.lock")
                .exists()
        );

        assert!(
            system
                .agent_runtime()
                .unwrap()
                .rollouts
                .join("rollout-1.jsonl")
                .exists()
        );
        assert!(
            system
                .memory_stores()
                .unwrap()
                .join("default/USER.md")
                .exists()
        );

        let second = reset_alan_stores(&system, &host, ResetScope::Settings).unwrap();
        assert!(second.removed_paths.is_empty());
    }

    #[test]
    fn reset_all_wipes_all_alan_services_and_host_store_idempotently() {
        let temp = tempfile::tempdir().unwrap();
        let (system, host) = seed_stores(temp.path());

        let report = reset_alan_stores(&system, &host, ResetScope::All).unwrap();
        assert_eq!(report.scope, ResetScope::All);
        assert!(!system.root.join("services").exists());
        assert!(!host.credentials.exists());
        assert!(!host.managed_auth.exists());

        let second = reset_alan_stores(&system, &host, ResetScope::All).unwrap();
        assert!(second.removed_paths.is_empty());
    }
}
