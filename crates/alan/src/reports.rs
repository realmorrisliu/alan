use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::legacy_state;

pub(crate) async fn wait_for_host_stop(paths: &alan_os_host::HostEndpointPaths) -> Result<()> {
    // Runtime shutdown can use ten seconds for graceful drain plus five seconds to abort.
    for _ in 0..400 {
        if !paths.status.exists() && !paths.socket.exists() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    anyhow::bail!("Alan OS Host did not stop within twenty seconds")
}

pub(crate) fn print_host_status(status: &alan_os_host::HostStatus, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(status)?);
    } else {
        println!("state: {:?}", status.readiness);
        println!("boot: {}", status.boot_id);
        println!("host pid: {}", status.pid);
        println!("attachment: {}", status.socket.display());
    }
    Ok(())
}

pub(crate) async fn print_host_mount_state(shell: &alan_shell::Shell) -> Result<()> {
    let request_ids = shell
        .ls("/mnt/host-mount/requests")
        .await?
        .into_iter()
        .filter(|entry| !matches!(entry.as_str(), "clone" | "events"))
        .collect::<Vec<_>>();
    println!("requests:");
    if request_ids.is_empty() {
        println!("  (none)");
    }
    for request_id in request_ids {
        let base = format!("/mnt/host-mount/requests/{request_id}");
        let status = String::from_utf8(shell.cat(&format!("{base}/status")).await?)?;
        let request = String::from_utf8(shell.cat(&format!("{base}/request")).await?)?;
        println!("  {request_id} [{}] {request}", status.trim());
    }

    let grant_ids = shell.ls("/mnt/host-mount/grants").await?;
    println!("grants:");
    if grant_ids.is_empty() {
        println!("  (none)");
    }
    for grant_id in grant_ids {
        let record = String::from_utf8(
            shell
                .cat(&format!("/mnt/host-mount/grants/{grant_id}/record"))
                .await?,
        )?;
        println!("  {grant_id} {record}");
    }
    Ok(())
}

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
