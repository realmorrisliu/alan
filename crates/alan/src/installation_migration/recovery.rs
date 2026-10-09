//! Source-independent inventories, checked against the component hashes in the journal.

use alan_os_host::installation::{InstallationPaths, MigrationJournal, MigrationPayload};
use anyhow::{Context, Result, ensure};

use super::{snapshot::Snapshot, transaction::Component};

pub(super) fn encode(components: &[Component]) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(
        &components
            .iter()
            .map(|component| (component.kind, &component.snapshot))
            .collect::<Vec<_>>(),
    )?)
}

pub(super) fn load(
    paths: &InstallationPaths,
    journal: &MigrationJournal,
) -> Result<Option<Vec<Component>>> {
    let Some(bytes) = paths.read_recovery_inventory(journal.id)? else {
        return Ok(None);
    };
    let inventories: Vec<(MigrationPayload, Snapshot)> =
        serde_json::from_slice(&bytes).context("invalid installation recovery inventory")?;
    ensure!(
        inventories.len() == journal.components.len(),
        "recovery component count differs from journal"
    );
    let mut components = Vec::new();
    for ((kind, snapshot), expected) in inventories.into_iter().zip(&journal.components) {
        snapshot.validate_inventory()?;
        ensure!(
            kind == expected.payload && snapshot.fingerprint()? == expected.fingerprint,
            "recovery inventory differs from committed fingerprint"
        );
        let (parent, name) = match kind {
            MigrationPayload::Services => (paths.system_root(), "services"),
            MigrationPayload::Credentials => (paths.host_root(), "credentials"),
            MigrationPayload::ManagedAuth => (paths.host_root(), "auth.json"),
        };
        components.push(Component {
            kind,
            source: parent.join(journal.source.id()).join(name),
            destination: parent.join(name),
            snapshot,
        });
    }
    Ok(Some(components))
}
