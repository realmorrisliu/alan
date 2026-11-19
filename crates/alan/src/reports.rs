use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::legacy_state;

pub(crate) fn canonical_existing_roots(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    paths
        .into_iter()
        .map(|path| {
            std::fs::canonicalize(&path)
                .with_context(|| format!("failed to resolve source root {}", path.display()))
        })
        .collect()
}

pub(crate) fn print_legacy_inspection(
    report: &legacy_state::LegacyInspection,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    if let Some(installations) = &report.installations {
        println!(
            "canonical stores: system={}, host={}",
            installations.canonical.system_present, installations.canonical.host_present
        );
        for source in &installations.sources {
            println!(
                "legacy {}: system={} ({}), host={} ({})",
                source.source.id(),
                source.stores.system_present,
                source.stores.system_root.display(),
                source.stores.host_present,
                source.stores.host_root.display()
            );
        }
        if installations.migration_journal_present {
            println!(
                "installation migration journal present; inspect migration status before opening stores"
            );
        }
    }
    for path in &report.generated_paths {
        println!("generated: {}", path.display());
    }
    for path in &report.migratable_paths {
        println!("migratable: {}", path.display());
    }
    for root in &report.authored_roots {
        println!("authored {:?}: {}", root.kind, root.path.display());
    }
    if report.generated_paths.is_empty()
        && report.migratable_paths.is_empty()
        && report.authored_roots.is_empty()
    {
        println!("no recognized legacy state");
    }
    Ok(())
}

pub(crate) fn print_legacy_cleanup(
    report: &legacy_state::LegacyCleanupReport,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    if report.connection_migration.metadata_migrated {
        println!("migrated and verified: connection metadata");
    }
    if report.connection_migration.credential_file_migrated {
        println!("migrated and verified: Host credential file");
    }
    if report.connection_migration.managed_auth_migrated {
        println!("migrated and verified: managed auth file");
    }
    for path in &report.removed_generated_paths {
        println!("removed generated: {}", path.display());
    }
    for root in &report.authored_roots {
        if root.kind == legacy_state::AuthoredRootKind::Skills {
            println!(
                "preserved authored Skills: {} (install through `q` in Alan Shell)",
                root.path.display()
            );
        } else {
            println!(
                "preserved authored {:?}: {} (use `alan host legacy-state import` explicitly)",
                root.kind,
                root.path.display()
            );
        }
    }
    if report == &legacy_state::LegacyCleanupReport::default() {
        println!("no recognized legacy state");
    }
    Ok(())
}
