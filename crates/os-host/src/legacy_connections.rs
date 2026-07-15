//! One-shot connection migration from the retired Host-directory model.

use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use alan_agent_engine::InstallChannel;
use alan_service_manager::{ConnectionsFile, default_credential_backend};
use anyhow::{Context, Result, ensure};
use serde::Serialize;

use crate::{HostStorePaths, SystemStorePaths};

const SECRET_STORE_FILE: &str = "secrets.toml";

/// Fixed legacy connection paths for one install channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyConnectionPaths {
    pub channel: InstallChannel,
    pub alan_root: PathBuf,
}

impl LegacyConnectionPaths {
    pub fn detect(channel: InstallChannel) -> Result<Option<Self>> {
        dirs::home_dir()
            .map(|home| Self::from_home_dir(&home, channel))
            .transpose()
    }

    pub fn from_home_dir(home_dir: &Path, channel: InstallChannel) -> Result<Self> {
        validate_absolute_path("Host home directory", home_dir)?;
        Ok(Self {
            channel,
            alan_root: home_dir.join(match channel {
                InstallChannel::Stable => ".alan",
                InstallChannel::Dev => ".alan-dev",
            }),
        })
    }

    fn connections_metadata(&self) -> PathBuf {
        self.alan_root.join("connections.toml")
    }

    fn credential_file(&self) -> PathBuf {
        self.alan_root.join("credentials").join(SECRET_STORE_FILE)
    }

    fn managed_auth(&self) -> PathBuf {
        self.alan_root.join("auth.json")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ConnectionMigrationReport {
    pub metadata_migrated: bool,
    pub credential_file_migrated: bool,
    pub managed_auth_migrated: bool,
}

/// Migrate, verify, and delete legacy connection state before Host boot reads it.
pub fn migrate_legacy_connections(
    paths: &LegacyConnectionPaths,
    system_store: &SystemStorePaths,
    host_store: &HostStorePaths,
) -> Result<ConnectionMigrationReport> {
    ensure_real_legacy_root_or_missing(&paths.alan_root)?;
    ensure!(
        paths.channel.descriptor().id == system_store.channel_id,
        "legacy and System Store channels differ"
    );

    let credential_file_migrated = migrate_host_file(
        &paths.credential_file(),
        &host_store.credentials.join(SECRET_STORE_FILE),
        "Host credential file",
    )?;
    let managed_auth_migrated = migrate_host_file(
        &paths.managed_auth(),
        &host_store.managed_auth,
        "managed auth file",
    )?;
    let metadata_migrated = migrate_connection_metadata(
        &paths.connections_metadata(),
        &system_store.connections_metadata()?,
    )?;

    Ok(ConnectionMigrationReport {
        metadata_migrated,
        credential_file_migrated,
        managed_auth_migrated,
    })
}

fn migrate_connection_metadata(source: &Path, target: &Path) -> Result<bool> {
    let Some(source_metadata) = optional_symlink_metadata(source)? else {
        return Ok(false);
    };
    ensure!(
        source_metadata.file_type().is_file() && !source_metadata.file_type().is_symlink(),
        "legacy connection metadata must be a real file: {}",
        source.display()
    );
    let legacy = load_legacy_connections(source)?;
    let (current, _) = ConnectionsFile::load_from_path(target)?;
    let merged = merge_connections(current, legacy)?;

    if !target.is_file() || ConnectionsFile::load_from_path(target)?.0 != merged {
        save_connections_atomically(&merged, target)?;
    }
    ensure!(
        ConnectionsFile::load_from_path(target)?.0 == merged,
        "Connection Service verification failed at {}",
        target.display()
    );
    fs::remove_file(source).with_context(|| {
        format!(
            "connection metadata migrated but legacy deletion failed for {}",
            source.display()
        )
    })?;
    prune_empty_parents(source.parent(), source.parent().and_then(Path::parent));
    Ok(true)
}

fn load_legacy_connections(source: &Path) -> Result<ConnectionsFile> {
    let content = fs::read_to_string(source).with_context(|| {
        format!(
            "failed to read legacy connection metadata {}",
            source.display()
        )
    })?;
    let mut document: toml::Value = toml::from_str(&content).with_context(|| {
        format!(
            "failed to parse legacy connection metadata {}",
            source.display()
        )
    })?;
    let table = document.as_table_mut().with_context(|| {
        format!(
            "legacy connection metadata must be a TOML table: {}",
            source.display()
        )
    })?;
    table.remove("workspace_pins");
    let mut connections: ConnectionsFile = document.try_into().with_context(|| {
        format!(
            "failed to decode legacy connection metadata {}",
            source.display()
        )
    })?;
    ensure!(
        connections.version == ConnectionsFile::default().version,
        "unsupported legacy connections file version {} in {}",
        connections.version,
        source.display()
    );
    for credential in connections.credentials.values_mut() {
        credential.backend = default_credential_backend(credential.kind).to_string();
    }
    Ok(connections)
}

fn merge_connections(
    mut current: ConnectionsFile,
    legacy: ConnectionsFile,
) -> Result<ConnectionsFile> {
    match (&current.default_profile, &legacy.default_profile) {
        (None, Some(value)) => current.default_profile = Some(value.clone()),
        (Some(current_value), Some(legacy_value)) => ensure!(
            current_value == legacy_value,
            "legacy and current default connection profiles conflict (`{legacy_value}` vs `{current_value}`)"
        ),
        _ => {}
    }
    for (id, credential) in legacy.credentials {
        if let Some(existing) = current.credentials.get(&id) {
            ensure!(
                existing == &credential,
                "legacy and current credential metadata conflict for `{id}`"
            );
        } else {
            current.credentials.insert(id, credential);
        }
    }
    for (id, profile) in legacy.profiles {
        if let Some(existing) = current.profiles.get(&id) {
            ensure!(
                existing == &profile,
                "legacy and current connection profiles conflict for `{id}`"
            );
        } else {
            current.profiles.insert(id, profile);
        }
    }
    Ok(current)
}

fn save_connections_atomically(connections: &ConnectionsFile, target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .context("Connection Service metadata path has no parent")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create Connection Service directory {}",
            parent.display()
        )
    })?;
    let staging = parent.join(format!(
        ".connections-migration-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    connections.save_to_path(&staging)?;
    ensure!(
        ConnectionsFile::load_from_path(&staging)?.0 == *connections,
        "staged Connection Service metadata failed verification"
    );
    fs::rename(&staging, target).with_context(|| {
        format!(
            "failed to atomically install Connection Service metadata {}",
            target.display()
        )
    })?;
    Ok(())
}

fn migrate_host_file(source: &Path, target: &Path, label: &str) -> Result<bool> {
    let Some(source_metadata) = optional_symlink_metadata(source)? else {
        return Ok(false);
    };
    ensure!(
        source_metadata.file_type().is_file() && !source_metadata.file_type().is_symlink(),
        "legacy {label} must be a real file: {}",
        source.display()
    );
    let source_bytes = fs::read(source)
        .with_context(|| format!("failed to read legacy {label} {}", source.display()))?;
    if let Some(target_metadata) = optional_symlink_metadata(target)? {
        ensure!(
            target_metadata.file_type().is_file() && !target_metadata.file_type().is_symlink(),
            "current {label} must be a real file: {}",
            target.display()
        );
        ensure!(
            fs::read(target)? == source_bytes,
            "legacy and current {label} conflict; both files were preserved"
        );
    } else {
        write_sensitive_file_atomically(target, &source_bytes)?;
        ensure!(
            fs::read(target)? == source_bytes,
            "{label} verification failed at {}",
            target.display()
        );
    }
    fs::remove_file(source).with_context(|| {
        format!(
            "{label} migrated but legacy deletion failed for {}",
            source.display()
        )
    })?;
    Ok(true)
}

fn write_sensitive_file_atomically(target: &Path, bytes: &[u8]) -> Result<()> {
    let parent = target
        .parent()
        .context("Host-owned sensitive path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create Host Store directory {}", parent.display()))?;
    let staging = parent.join(format!(
        ".host-migration-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&staging)
        .with_context(|| format!("failed to stage Host Store file {}", staging.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to stage Host Store file {}", staging.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync Host Store file {}", staging.display()))?;
    drop(file);
    ensure!(
        fs::read(&staging)? == bytes,
        "staged Host Store file failed verification"
    );
    fs::rename(&staging, target)
        .with_context(|| format!("failed to install Host Store file {}", target.display()))?;
    Ok(())
}

fn ensure_real_legacy_root_or_missing(root: &Path) -> Result<()> {
    let Some(metadata) = optional_symlink_metadata(root)? else {
        return Ok(());
    };
    ensure!(
        metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
        "refusing to traverse non-directory or symlinked legacy root {}",
        root.display()
    );
    Ok(())
}

fn validate_absolute_path(label: &str, path: &Path) -> Result<()> {
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

fn optional_symlink_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

fn prune_empty_parents(mut path: Option<&Path>, stop_before: Option<&Path>) {
    while let Some(current) = path {
        if stop_before.is_some_and(|stop| current == stop) {
            break;
        }
        if fs::remove_dir(current).is_err() {
            break;
        }
        path = current.parent();
    }
}
