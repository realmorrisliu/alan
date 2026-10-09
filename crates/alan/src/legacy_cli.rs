use super::*;

pub(super) async fn run(action: LegacyStateAction) -> Result<()> {
    let channel = alan_agent_engine::InstallChannel::detect_current();
    match action {
        LegacyStateAction::MigrateInstallation {
            from,
            dry_run,
            rollback,
            json,
        } => {
            use alan::installation_migration::{MigrationMode, migrate_installation};
            use alan_os_host::installation::{InstallationPaths, LegacyInstallation};
            let source = match from.as_str() {
                "stable" => LegacyInstallation::Stable,
                "dev" => LegacyInstallation::Dev,
                _ => anyhow::bail!("unsupported historical installation source"),
            };
            let mode = if dry_run {
                MigrationMode::DryRun
            } else if rollback {
                MigrationMode::Rollback
            } else {
                MigrationMode::Apply
            };
            let report = migrate_installation(&InstallationPaths::detect()?, source, mode).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "installation migration: {} (source: {}, components: {})",
                    report.state,
                    source.id(),
                    report.components
                );
            }
        }
        LegacyStateAction::Inspect { source_roots, json } => {
            let Some(paths) = legacy_state::LegacyStatePaths::detect(channel)? else {
                anyhow::bail!("cannot determine Host home directory");
            };
            let source_roots = canonical_existing_roots(source_roots)?;
            let mut report = legacy_state::inspect_legacy_state(&paths, &source_roots)?;
            report.installations =
                Some(alan_os_host::installation::InstallationPaths::detect()?.inspect()?);
            print_legacy_inspection(&report, json)?;
        }
        LegacyStateAction::Cleanup { source_roots, json } => {
            let Some(paths) = legacy_state::LegacyStatePaths::detect(channel)? else {
                anyhow::bail!("cannot determine Host home directory");
            };
            let source_roots = canonical_existing_roots(source_roots)?;
            let system = alan_os_host::SystemStorePaths::detect(channel.descriptor().id)?;
            let host = alan_os_host::HostStorePaths::detect(channel.descriptor().id)?;
            let report = legacy_state::cleanup_legacy_state(&paths, &system, &host, &source_roots)?;
            print_legacy_cleanup(&report, json)?;
        }
        LegacyStateAction::Import {
            kind,
            source,
            name,
            delete_source,
        } => {
            let source = std::path::absolute(&source).with_context(|| {
                format!(
                    "failed to make import source absolute: {}",
                    source.display()
                )
            })?;
            let system = alan_os_host::SystemStorePaths::detect(channel.descriptor().id)?;
            let kind = match kind {
                LegacyImportKind::AgentDefinition => {
                    legacy_state::AuthoredImportKind::AgentDefinition
                }
                LegacyImportKind::MemoryStore => legacy_state::AuthoredImportKind::MemoryStore,
            };
            let report = legacy_state::import_authored_content(
                kind,
                &source,
                &name,
                delete_source,
                &system,
            )?;
            println!("imported: {}", report.destination.display());
            if report.source_deleted {
                println!(
                    "source deleted after verification: {}",
                    report.source.display()
                );
            }
        }
    }

    Ok(())
}
