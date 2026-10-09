//! Explicit installation adoption; all inspection is offline and source preserving.

mod quiescence;
mod snapshot;
mod transaction;
pub use transaction::{MigrationMode, MigrationReport, migrate_installation};

use std::fs;
use std::path::{Component, Path};

use alan_agent_engine::RolloutRecorder;
use alan_os_host::SecretStore;
use alan_service_manager::{
    ConnectionsFile, CredentialKind, default_credential_backend, sanitize_identifier,
};
use anyhow::{Context, Result, ensure};

/// Validate the supported source payload through its owning readers.
/// The transaction caller must retain writer exclusion and compare source digests.
pub async fn validate_store_pair(system: &Path, host: &Path) -> Result<()> {
    validate_pair(system, host, &[]).await
}

async fn validate_pair(system: &Path, host: &Path, ignored: &[&str]) -> Result<()> {
    let system_entries = ["services"]
        .into_iter()
        .chain(ignored.iter().copied())
        .collect::<Vec<_>>();
    let host_entries = ["credentials", "auth.json", "auth.refresh.lock"]
        .into_iter()
        .chain(ignored.iter().copied())
        .collect::<Vec<_>>();
    check_entries(system, &system_entries)?;
    check_entries(host, &host_entries)?;
    check_entries(
        &host.join("credentials"),
        &["secrets.toml", "secrets.toml.lock"],
    )?;
    let services = system.join("services");
    check_entries(
        &services,
        &[
            "connections",
            "packages",
            "agent-runtime",
            "memory",
            "shell-ui",
        ],
    )?;
    check_entries(
        &services.join("connections"),
        &["connections.toml", "connections.toml.lock"],
    )?;
    let connections =
        ConnectionsFile::load_from_path(&services.join("connections/connections.toml"))?.0;
    validate_connections(&connections, host)?;
    if host.join("auth.json").exists() {
        alan_auth::AuthStorage::new(host.join("auth.json"))?.validate_for_migration()?;
    }
    if services.join("packages").exists() {
        alan_service_manager::validate_package_store_for_migration(&services.join("packages"))?;
    }
    validate_runtime(&services.join("agent-runtime")).await?;
    check_entries(&services.join("memory"), &["stores"])?;
    validate_authored_tree(&services.join("memory/stores"))?;
    validate_authored_tree(&services.join("agent-runtime/definitions"))?;
    let history = services.join("shell-ui");
    check_entries(
        &history,
        &[
            "composer-history",
            "composer-history.v1.jsonl",
            "composer-history.v2.jsonl",
        ],
    )?;
    for (name, version) in [
        ("composer-history", 0),
        ("composer-history.v1.jsonl", 1),
        ("composer-history.v2.jsonl", 2),
    ] {
        let path = history.join(name);
        if !path.exists() {
            continue;
        }
        for line in fs::read_to_string(path)?
            .lines()
            .filter(|line| !line.trim().is_empty())
        {
            match version {
                1 => {
                    serde_json::from_str::<String>(line).context("invalid shell history record")?;
                }
                2 => {
                    serde_json::from_str::<alan_tui::composer::HistoryEntry>(line)
                        .context("invalid shell history record")?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_connections(connections: &ConnectionsFile, host: &Path) -> Result<()> {
    let secrets = SecretStore::from_directory(&host.join("credentials"))?;
    secrets.validate_for_migration()?;
    if connections.default_profile.is_some() {
        connections.resolve_profile(None)?;
    }
    for id in connections.profiles.keys() {
        ensure!(
            sanitize_identifier(id).as_deref() == Some(id.as_str()),
            "invalid migration profile id"
        );
        connections.resolve_profile(Some(id))?;
    }
    for (id, credential) in &connections.credentials {
        ensure!(
            sanitize_identifier(id).as_deref() == Some(id.as_str()),
            "invalid migration credential id"
        );
        ensure!(
            credential.backend == default_credential_backend(credential.kind),
            "unsupported credential backing reference"
        );
        match credential.kind {
            CredentialKind::SecretString => secrets.validate_credential_for_migration(id)?,
            CredentialKind::ManagedOauth => {
                ensure!(
                    host.join("auth.json").is_file(),
                    "managed credential backing is missing"
                );
                alan_auth::AuthStorage::new(host.join("auth.json"))?.validate_for_migration()?;
            }
            CredentialKind::AmbientCloudAuth => {}
        }
    }
    Ok(())
}

async fn validate_runtime(root: &Path) -> Result<()> {
    check_entries(
        root,
        &[
            "rollouts",
            "checkpoints",
            "metadata",
            "definitions",
            "cache",
            "tmp",
        ],
    )?;
    check_entries(&root.join("metadata"), &["root-rollout"])?;
    // The shipped engine stores checkpoint records in rollouts, not this reserved directory.
    check_entries(&root.join("checkpoints"), &[])?;
    let rollouts = root.join("rollouts");
    if rollouts.exists() {
        for entry in fs::read_dir(&rollouts)? {
            let path = entry?.path();
            ensure!(
                fs::symlink_metadata(&path)?.is_file() && !path.is_symlink(),
                "rollout is not a regular file"
            );
            ensure!(
                path.extension()
                    .is_some_and(|extension| extension == "jsonl"),
                "unsupported rollout filename"
            );
            RolloutRecorder::load_history(&path)
                .await
                .context("validate durable rollout without replay")?;
        }
    }
    let selection = root.join("metadata/root-rollout");
    if selection.exists() {
        let name = fs::read_to_string(selection)?;
        let mut components = Path::new(&name).components();
        ensure!(
            matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none(),
            "unsupported Root rollout backing reference"
        );
        ensure!(
            rollouts.join(name).is_file(),
            "selected Root rollout is missing"
        );
    }
    Ok(())
}

fn check_entries(root: &Path, allowed: &[&str]) -> Result<()> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "migration subtree is not a real directory: {}",
        root.display()
    );
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            kind.is_dir() || kind.is_file(),
            "migration subtree contains a symlink or special file"
        );
        ensure!(
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| allowed.contains(&name)),
            "unsupported migration entry: {}",
            entry.path().display()
        );
    }
    Ok(())
}

fn validate_authored_tree(root: &Path) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "authored store contains a symlink"
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else {
            ensure!(metadata.is_file(), "authored store contains a special file");
            fs::File::open(path).context("authored store file is unreadable")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
