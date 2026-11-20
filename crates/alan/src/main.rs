//! Alan — a programmable personal computing environment.

mod cli;
mod foreground;
mod legacy_cli;
mod legacy_state;
mod reports;

use alan_os_host::{AlanOsHost, HostBootConfig, HostEndpointPaths, LocalAttachment};
use anyhow::{Context, Result};
use cli::dispatch::{
    BareRunMode, LegacyImportKind, LegacyStateAction, bare_run_mode, dispatch_subcommand,
    parse_cli, uses_product_stores, validate_bare_flags,
};
use reports::{canonical_existing_roots, print_legacy_cleanup, print_legacy_inspection};
use std::{
    io::{IsTerminal, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

#[tokio::main]
async fn main() -> Result<()> {
    alan_tui::terminal::install_panic_hook();
    let cli = parse_cli();
    alan_os_host::installation::validate_current_invocation()?;
    let (resume, resume_rollout_id) =
        cli::trust::parse_resume_arg(cli.resume.as_deref(), cli.command.is_some())?;
    let cwd_override = validate_bare_flags(
        resume,
        cli.ephemeral,
        cli.cwd.as_deref(),
        cli.command.is_some(),
    )?;
    let trust_options = cli::trust::resolve_bare_trust_options(&cli.trust, cli.command.is_some())?;
    anyhow::ensure!(
        cli.shadow_evaluator.is_none() || cli.command.is_none(),
        "`--shadow-evaluator` only applies to bare `alan`"
    );
    anyhow::ensure!(
        !cli.no_auto_mount || cli.command.is_none(),
        "`--no-auto-mount` only applies to bare `alan`"
    );
    cli::attachments::validate_cli_scripting_flags(
        cli.print.as_deref(),
        cli.json,
        &cli.images,
        cli.command.is_some(),
    )?;

    let uses_stores = uses_product_stores(cli.command.as_ref());
    let _installation_access = uses_stores
        .then(|| alan_os_host::installation::InstallationPaths::detect()?.access())
        .transpose()?;

    match cli.command {
        Some(command) => {
            if let Some(legacy_action) = dispatch_subcommand(command).await? {
                legacy_cli::run(legacy_action).await?;
            }
        }
        None => {
            let mode = bare_run_mode(
                std::io::stdin().is_terminal(),
                std::io::stdout().is_terminal(),
                cli.print,
                cli.json,
                cli.images,
            )?;

            let (runtime_dir, remove_runtime_dir) = foreground::foreground_runtime_dir()?;
            let paths = HostEndpointPaths::from_runtime_dir(&runtime_dir)?;
            let result = foreground::run_bare_in_foreground_instance(
                paths,
                mode,
                resume,
                resume_rollout_id,
                cli.ephemeral,
                cwd_override,
                cli.no_auto_mount,
                cli.shadow_evaluator,
                trust_options,
            )
            .await;
            if remove_runtime_dir
                && let Err(error) = std::fs::remove_dir_all(&runtime_dir)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%error, path = %runtime_dir.display(), "failed to remove temporary Alan instance runtime directory");
            }
            let exit_code = result?;
            if exit_code != 0 {
                std::process::exit(if (1..=255).contains(&exit_code) {
                    exit_code
                } else {
                    1
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
