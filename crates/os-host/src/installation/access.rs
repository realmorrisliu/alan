//! Process-lifetime exclusion between ordinary product access and store adoption.

use std::fs::{self, File, OpenOptions};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{InstallationPaths, LegacyInstallation, directory_or_absent};

/// Fixed publication destinations; journal data never supplies arbitrary paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationPayload {
    Services,
    Credentials,
    ManagedAuth,
}

/// A published component's staged content fingerprint, including permissions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationComponent {
    pub payload: MigrationPayload,
    pub fingerprint: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationState {
    Preparing,
    Publishing,
    RollingBack,
    Committed,
}

/// Durable publication receipt outside both stores. Pending states block all readers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationJournal {
    pub version: u32,
    pub id: uuid::Uuid,
    pub source: LegacyInstallation,
    pub state: MigrationState,
    pub source_fingerprint: String,
    pub components: Vec<MigrationComponent>,
}

impl MigrationJournal {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "unsupported installation migration journal version"
        );
        ensure!(
            valid_fingerprint(&self.source_fingerprint),
            "invalid migration source fingerprint"
        );
        let mut payloads = Vec::new();
        for component in &self.components {
            ensure!(
                !payloads.contains(&component.payload),
                "duplicate migration component"
            );
            payloads.push(component.payload);
            ensure!(
                valid_fingerprint(&component.fingerprint),
                "invalid migration component fingerprint"
            );
        }
        Ok(())
    }
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Keep this guard alive until every store consumer in the invocation has stopped.
#[derive(Debug)]
pub struct InstallationAccess {
    _lock: File,
}

/// Exclusive access is retained through source checks, staging and publication.
#[derive(Debug)]
pub struct InstallationMigrationAccess {
    paths: InstallationPaths,
    _lock: File,
}

impl InstallationPaths {
    /// Acquire shared product access before resolving metadata, credentials or services.
    pub fn access(&self) -> Result<InstallationAccess> {
        let lock = self.open_access_lock()?;
        lock.try_lock_shared()
            .context("installation migration is running; retry after it finishes")?;
        if let Some(journal) = self.read_migration_journal()? {
            ensure!(
                journal.state == MigrationState::Committed,
                "installation migration is incomplete; resume or roll back the explicit migration before opening stores"
            );
        }
        let inventory = self.inspect()?;
        ensure!(
            inventory.canonical.system_present
                || inventory.canonical.host_present
                || !inventory
                    .sources
                    .iter()
                    .any(|source| source.stores.system_present || source.stores.host_present),
            "legacy Alan stores exist; explicitly select a source with alan legacy-state migrate-installation --from stable|dev before starting Alan"
        );
        Ok(InstallationAccess { _lock: lock })
    }

    pub fn migration_access(&self) -> Result<InstallationMigrationAccess> {
        let lock = self.open_access_lock()?;
        lock.try_lock()
            .context("Alan still has active store consumers; stop them before migration")?;
        self.read_migration_journal()?;
        Ok(InstallationMigrationAccess {
            paths: self.clone(),
            _lock: lock,
        })
    }

    /// Read-only; malformed receipts fail closed rather than exposing partially published data.
    pub fn read_migration_journal(&self) -> Result<Option<MigrationJournal>> {
        let path = self.journal();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "migration journal is not a regular file"
        );
        ensure!(
            metadata.len() <= 16 * 1024,
            "migration journal exceeds supported size"
        );
        let journal: MigrationJournal = serde_json::from_slice(&fs::read(path)?)
            .context("invalid installation migration journal")?;
        journal.validate()?;
        Ok(Some(journal))
    }

    /// Private recovery metadata, containing inventories and hashes rather than credential bytes.
    pub fn read_recovery_inventory(&self, id: uuid::Uuid) -> Result<Option<Vec<u8>>> {
        use std::io::Read;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = match options.open(self.recovery_inventory(id)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("open installation recovery inventory"),
        };
        ensure!(
            file.metadata()?.is_file(),
            "recovery inventory is not a regular file"
        );
        let mut bytes = Vec::new();
        file.take(RECOVERY_INVENTORY_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= RECOVERY_INVENTORY_LIMIT,
            "recovery inventory exceeds supported size"
        );
        Ok(Some(bytes))
    }

    fn recovery_inventory(&self, id: uuid::Uuid) -> std::path::PathBuf {
        self.product
            .join(format!("installation-recovery-{id}.json"))
    }

    fn open_access_lock(&self) -> Result<File> {
        for ancestor in self.product.ancestors() {
            directory_or_absent(ancestor)?;
        }
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&self.product)?;
        let path = self.product.join("installation.lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let lock = options.open(path)?;
        ensure!(
            lock.metadata()?.is_file(),
            "installation lock is not a regular file"
        );
        Ok(lock)
    }
}

impl InstallationMigrationAccess {
    /// Atomic, durable journal replacement while readers are excluded.
    pub fn write_journal(&self, journal: &MigrationJournal) -> Result<()> {
        use std::io::Write;
        journal.validate()?;
        let mut staged = tempfile::NamedTempFile::new_in(&self.paths.product)?;
        serde_json::to_writer(&mut staged, journal)?;
        staged.flush()?;
        staged.as_file().sync_all()?;
        staged
            .persist(self.paths.journal())
            .context("publish installation journal")?;
        sync_directory(&self.paths.product)
    }

    /// Persist once, before staging, so recovery never needs to trust a changed source.
    pub fn write_recovery_inventory(&self, id: uuid::Uuid, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        ensure!(
            bytes.len() as u64 <= RECOVERY_INVENTORY_LIMIT,
            "recovery inventory exceeds supported size"
        );
        let mut staged = tempfile::NamedTempFile::new_in(&self.paths.product)?;
        staged.write_all(bytes)?;
        staged.as_file().sync_all()?;
        staged
            .persist_noclobber(self.paths.recovery_inventory(id))
            .context("persist installation recovery inventory without replacement")?;
        sync_directory(&self.paths.product)
    }

    /// Remove recovery metadata only after all rollback work is durably complete.
    pub fn remove_recovery_inventory(&self, id: uuid::Uuid) -> Result<()> {
        fs::remove_file(self.paths.recovery_inventory(id))?;
        sync_directory(&self.paths.product)
    }

    /// Remove the pending receipt only after rollback has removed every published component.
    pub fn remove_journal(&self) -> Result<()> {
        fs::remove_file(self.paths.journal())?;
        sync_directory(&self.paths.product)
    }
}

const RECOVERY_INVENTORY_LIMIT: u64 = 64 * 1024 * 1024;

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests;
