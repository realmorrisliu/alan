use std::{
    fs,
    path::{Path, PathBuf},
};

use alan_os_host::installation::{
    InstallationPaths, LegacyInstallation, MigrationComponent, MigrationJournal, MigrationPayload,
    MigrationState,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{quiescence::SourceLocks, snapshot::Snapshot, validate_store_pair};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MigrationMode {
    DryRun,
    Apply,
    Rollback,
}

#[derive(Debug, Serialize)]
pub struct MigrationReport {
    pub source: LegacyInstallation,
    pub state: &'static str,
    pub components: usize,
}

pub(super) struct Component {
    pub(super) kind: MigrationPayload,
    pub(super) source: PathBuf,
    pub(super) destination: PathBuf,
    pub(super) snapshot: Snapshot,
}

impl Component {
    fn stage_path(&self, id: uuid::Uuid) -> PathBuf {
        self.destination
            .parent()
            .unwrap()
            .join(format!(".installation-{id}"))
            .join(self.destination.file_name().unwrap())
    }
}

/// Explicit adoption only. The dry run opens no writable store and creates no journal.
pub async fn migrate_installation(
    paths: &InstallationPaths,
    source: LegacyInstallation,
    mode: MigrationMode,
) -> Result<MigrationReport> {
    migrate(paths, source, mode, |locks, roots| {
        locks.check_processes(roots)
    })
    .await
}

async fn migrate(
    paths: &InstallationPaths,
    source: LegacyInstallation,
    mode: MigrationMode,
    check: impl Fn(&SourceLocks, &[PathBuf]) -> Result<()>,
) -> Result<MigrationReport> {
    migrate_with_checkpoints(paths, source, mode, check, |_| Ok(())).await
}

async fn migrate_with_checkpoints(
    paths: &InstallationPaths,
    source: LegacyInstallation,
    mode: MigrationMode,
    check: impl Fn(&SourceLocks, &[PathBuf]) -> Result<()>,
    checkpoint: impl Fn(&str) -> Result<()>,
) -> Result<MigrationReport> {
    let canonical = paths.inspect_canonical()?;
    let previous = paths.read_migration_journal()?;
    if let Some(journal) = &previous {
        ensure!(
            journal.state != MigrationState::RollingBack || mode == MigrationMode::Rollback,
            "interrupted rollback must be completed with --rollback"
        );
        ensure!(
            journal.source == source,
            "migration receipt selects a different legacy source"
        );
        if journal.state == MigrationState::Committed && mode != MigrationMode::Rollback {
            let _access = (mode != MigrationMode::DryRun)
                .then(|| paths.access())
                .transpose()?;
            ensure!(
                paths.read_migration_journal()? == previous,
                "installation receipt changed during retry"
            );
            let components = super::recovery::load(paths, journal)?
                .context("committed migration recovery inventory is missing")?;
            for component in components {
                ensure!(
                    Snapshot::read(&component.destination, false)? == component.snapshot,
                    "committed migration component is missing or changed; canonical data was retained"
                );
            }
            return Ok(MigrationReport {
                source,
                state: "already-committed",
                components: journal.components.len(),
            });
        }
    } else {
        ensure!(
            mode != MigrationMode::Rollback,
            "there is no installation migration to roll back"
        );
        ensure!(
            !canonical.system_present && !canonical.host_present,
            "canonical stores already contain data; whole-store merging is not supported"
        );
    }
    if mode == MigrationMode::Rollback {
        return rollback(
            paths,
            previous.context("missing migration receipt")?,
            check,
            checkpoint,
        );
    }
    paths.inspect()?;
    let system = paths.system_root().join(source.id());
    let host = paths.host_root().join(source.id());
    ensure!(
        system.is_dir() || host.is_dir(),
        "selected legacy installation is absent"
    );
    validate_store_pair(&system, &host).await?;
    let roots = vec![system.clone(), host.clone()];
    let native = SourceLocks::acquire(&system, &host)?;
    check(&native, &roots)?;
    let mut components = Vec::new();
    for (kind, from, to) in [
        (
            MigrationPayload::Services,
            system.join("services"),
            paths.system_root().join("services"),
        ),
        (
            MigrationPayload::Credentials,
            host.join("credentials"),
            paths.host_root().join("credentials"),
        ),
        (
            MigrationPayload::ManagedAuth,
            host.join("auth.json"),
            paths.host_root().join("auth.json"),
        ),
    ] {
        let snapshot = Snapshot::read(&from, kind == MigrationPayload::Services)?;
        if snapshot.present() {
            components.push(Component {
                kind,
                source: from,
                destination: to,
                snapshot,
            });
        }
    }
    ensure!(
        !components.is_empty(),
        "selected legacy installation contains no durable payload"
    );
    let receipts = components
        .iter()
        .map(|component| {
            Ok(MigrationComponent {
                payload: component.kind,
                fingerprint: component.snapshot.fingerprint()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let fingerprint = super::snapshot::hex(Sha256::digest(serde_json::to_vec(&receipts)?));
    if let Some(journal) = &previous {
        ensure!(
            journal.source_fingerprint == fingerprint && journal.components == receipts,
            "legacy source changed since migration started; source and canonical data were retained"
        );
    }
    if mode == MigrationMode::DryRun {
        return Ok(MigrationReport {
            source,
            state: "validated",
            components: components.len(),
        });
    }
    let exclusive = paths.migration_access()?;
    ensure!(
        paths.read_migration_journal()? == previous,
        "installation receipt changed during preflight"
    );
    if previous.is_none() {
        let latest = paths.inspect()?;
        ensure!(
            !latest.canonical.system_present && !latest.canonical.host_present,
            "canonical stores changed during preflight"
        );
    }
    let mut journal = previous.unwrap_or(MigrationJournal {
        version: 1,
        id: uuid::Uuid::new_v4(),
        source,
        state: MigrationState::Preparing,
        source_fingerprint: fingerprint,
        components: receipts,
    });
    ensure_destination_shape(paths, &components, journal.id)?;
    exclusive.write_journal(&journal)?;
    checkpoint("prepared")?;
    if super::recovery::load(paths, &journal)?.is_none() {
        ensure!(
            journal.state == MigrationState::Preparing && stage_roots_absent(paths, journal.id)?,
            "recovery inventory is missing after staging began; retain data for inspection"
        );
        let bytes = super::recovery::encode(&components)?;
        exclusive.write_recovery_inventory(journal.id, &bytes)?;
    }
    checkpoint("inventory")?;
    if journal.state == MigrationState::Preparing {
        prepare_stage_roots(paths, journal.id)?;
        for component in &components {
            let staged = component.stage_path(journal.id);
            component.snapshot.discard_partial(&staged)?;
            component.snapshot.stage(&component.source, &staged)?;
            checkpoint("staged")?;
        }
        let staged_system = paths
            .system_root()
            .join(format!(".installation-{}", journal.id));
        let staged_host = paths
            .host_root()
            .join(format!(".installation-{}", journal.id));
        validate_store_pair(&staged_system, &staged_host).await?;
        journal.state = MigrationState::Publishing;
        exclusive.write_journal(&journal)?;
        checkpoint("publishing")?;
    }
    check(&native, &roots)?;
    for component in &components {
        ensure!(
            Snapshot::read(
                &component.source,
                component.kind == MigrationPayload::Services
            )? == component.snapshot,
            "source changed before publication; resume or roll back after stopping writers"
        );
    }
    for component in &components {
        let staged = component.stage_path(journal.id);
        let destination = Snapshot::read(&component.destination, false)?;
        if destination.present() {
            ensure!(
                destination == component.snapshot && !staged.exists(),
                "publication destination is occupied or inconsistent"
            );
        } else {
            ensure!(
                Snapshot::read(&staged, false)? == component.snapshot,
                "staged component is missing or changed"
            );
            fs::rename(&staged, &component.destination)
                .context("publish installation component")?;
            sync_parent(&component.destination)?;
            sync_parent(&staged)?;
        }
        checkpoint("published")?;
    }
    super::validate_pair(
        &paths.system_root(),
        &paths.host_root(),
        &["stable", "dev", &format!(".installation-{}", journal.id)],
    )
    .await?;
    remove_stage_roots(paths, journal.id)?;
    checkpoint("before-commit")?;
    journal.state = MigrationState::Committed;
    exclusive.write_journal(&journal)?;
    checkpoint("committed")?;
    Ok(MigrationReport {
        source,
        state: "committed",
        components: components.len(),
    })
}

fn rollback(
    paths: &InstallationPaths,
    mut journal: MigrationJournal,
    check: impl Fn(&SourceLocks, &[PathBuf]) -> Result<()>,
    checkpoint: impl Fn(&str) -> Result<()>,
) -> Result<MigrationReport> {
    let exclusive = paths.migration_access()?;
    ensure!(
        paths.read_migration_journal()?.as_ref() == Some(&journal),
        "installation receipt changed during rollback"
    );
    let native = SourceLocks::acquire(&paths.system_root(), &paths.host_root())?;
    check(&native, &[paths.system_root(), paths.host_root()])?;
    let Some(components) = super::recovery::load(paths, &journal)? else {
        let canonical = paths.inspect_canonical()?;
        ensure!(
            matches!(
                journal.state,
                MigrationState::Preparing | MigrationState::RollingBack
            ) && !canonical.system_present
                && !canonical.host_present
                && stage_roots_absent(paths, journal.id)?,
            "recovery inventory is missing while migration data remains; retain it for inspection"
        );
        exclusive.remove_journal()?;
        return Ok(MigrationReport {
            source: journal.source,
            state: "rolled-back",
            components: journal.components.len(),
        });
    };
    ensure_destination_shape(paths, &components, journal.id)?;
    for component in &components {
        let current = Snapshot::read(&component.destination, false)?;
        ensure!(
            !current.present() || current == component.snapshot,
            "canonical data changed after adoption; rollback would lose new work"
        );
    }
    journal.state = MigrationState::RollingBack;
    exclusive.write_journal(&journal)?;
    checkpoint("rolling-back")?;
    for component in &components {
        component.snapshot.discard_partial(&component.destination)?;
        component
            .snapshot
            .discard_partial(&component.stage_path(journal.id))?;
        sync_parent(&component.destination)?;
        checkpoint("removed")?;
    }
    remove_stage_roots(paths, journal.id)?;
    exclusive.remove_recovery_inventory(journal.id)?;
    checkpoint("inventory-removed")?;
    exclusive.remove_journal()?;
    Ok(MigrationReport {
        source: journal.source,
        state: "rolled-back",
        components: components.len(),
    })
}

fn stage_roots_absent(paths: &InstallationPaths, id: uuid::Uuid) -> Result<bool> {
    for parent in [paths.system_root(), paths.host_root()] {
        match fs::symlink_metadata(parent.join(format!(".installation-{id}"))) {
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}

fn ensure_destination_shape(
    paths: &InstallationPaths,
    components: &[Component],
    id: uuid::Uuid,
) -> Result<()> {
    for parent in [paths.system_root(), paths.host_root()] {
        let entries = match fs::read_dir(&parent) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let path = entry?.path();
            let name = path.file_name().unwrap();
            ensure!(
                name == "stable"
                    || name == "dev"
                    || name == format!(".installation-{id}").as_str()
                    || components
                        .iter()
                        .any(|component| component.destination == path),
                "canonical store contains data outside this migration; retain it for inspection"
            );
        }
    }
    Ok(())
}

fn prepare_stage_roots(paths: &InstallationPaths, id: uuid::Uuid) -> Result<()> {
    for parent in [paths.system_root(), paths.host_root()] {
        let mut builder = fs::DirBuilder::new();
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700).recursive(true).create(&parent)?;
        let stage = parent.join(format!(".installation-{id}"));
        if !stage.exists() {
            builder.recursive(false).create(&stage)?;
        }
        ensure!(
            fs::symlink_metadata(&stage)?.is_dir() && !stage.is_symlink(),
            "migration stage is not a real directory"
        );
        for entry in fs::read_dir(&stage)? {
            let name = entry?.file_name();
            ensure!(
                matches!(
                    name.to_str(),
                    Some("services" | "credentials" | "auth.json")
                ),
                "unrecognized staging data; retain for inspection"
            );
        }
        fs::File::open(&parent)?.sync_all()?;
    }
    Ok(())
}

fn remove_stage_roots(paths: &InstallationPaths, id: uuid::Uuid) -> Result<()> {
    for parent in [paths.system_root(), paths.host_root()] {
        let stage = parent.join(format!(".installation-{id}"));
        match fs::remove_dir(&stage) {
            Ok(()) => fs::File::open(&parent)?.sync_all()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("retain nonempty migration staging"),
        }
    }
    Ok(())
}

fn sync_parent(path: &Path) -> Result<()> {
    match fs::File::open(path.parent().context("migration component has no parent")?) {
        Ok(parent) => parent.sync_all()?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
