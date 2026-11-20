//! Safe CLI reset command (`alan reset [--all] [--yes]`).

use alan_os_host::{HostStorePaths, ResetReport, ResetScope, SystemStorePaths, reset_alan_stores};
use anyhow::Result;
use clap::Args;
use std::io::{self, IsTerminal, Write};

#[derive(Args, Debug, Clone)]
pub struct ResetArgs {
    /// Factory reset: clear all Alan services, Memory Stores, and rollouts in addition to settings
    #[arg(long)]
    pub all: bool,
    /// Skip interactive confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

/// Execute `alan reset`.
pub fn run(args: ResetArgs) -> Result<()> {
    let scope = if args.all {
        ResetScope::All
    } else {
        ResetScope::Settings
    };
    let system = SystemStorePaths::detect()?;
    let host = HostStorePaths::detect()?;

    if !args.yes {
        anyhow::ensure!(
            io::stdin().is_terminal() && io::stdout().is_terminal(),
            "Refusing to reset without confirmation in non-interactive mode; re-run with `--yes`."
        );
        print_scope_preview(scope, &system, &host);
        print!("Continue with reset? [y/N]: ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !parse_confirmation(&answer) {
            println!("Reset cancelled.");
            return Ok(());
        }
    }

    let report = reset_alan_stores(&system, &host, scope)?;
    println!("{}", format_reset_summary(&report));
    Ok(())
}

fn print_scope_preview(scope: ResetScope, system: &SystemStorePaths, host: &HostStorePaths) {
    match scope {
        ResetScope::Settings => {
            println!(
                "This will clear Alan connections, secrets/OAuth ({}), project mounts, shell history, updater cache, and the active rollout pointer under {}.",
                host.credentials.display(),
                system.root.display()
            );
            println!(
                "Preserved: recorded rollouts, Memory Stores, user projects, executables, and ~/.config/gcloud."
            );
        }
        ResetScope::All => {
            println!(
                "FACTORY RESET: this will remove all Alan System Store services under {} and Host credentials/OAuth under {}.",
                system.root.display(),
                host.credentials.display()
            );
            println!("Preserved: user project directories, executables, and ~/.config/gcloud.");
        }
    }
}

fn parse_confirmation(raw: &str) -> bool {
    matches!(raw.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn format_reset_summary(report: &ResetReport) -> String {
    let label = match report.scope {
        ResetScope::Settings => "Reset Alan connections and settings",
        ResetScope::All => "Factory-reset all Alan data",
    };
    if report.removed_paths.is_empty() {
        format!("{label} (already clean; nothing to remove).")
    } else {
        format!(
            "{label} (removed {} store item{}). Start `alan` to run first-time setup.",
            report.removed_paths.len(),
            if report.removed_paths.len() == 1 {
                ""
            } else {
                "s"
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_parser_requires_explicit_yes() {
        assert!(parse_confirmation("y"));
        assert!(parse_confirmation(" YES \n"));
        assert!(!parse_confirmation(""));
        assert!(!parse_confirmation("n"));
        assert!(!parse_confirmation("nope"));
    }

    #[test]
    fn reset_summary_formats_temp_store_reset() {
        let temp = tempfile::tempdir().unwrap();
        let system = SystemStorePaths::from_data_dir(temp.path()).unwrap();
        let host = HostStorePaths::from_data_dir(temp.path()).unwrap();
        std::fs::create_dir_all(system.service("connections").unwrap()).unwrap();
        std::fs::write(system.connections_metadata().unwrap(), "version = 1\n").unwrap();

        let report = reset_alan_stores(&system, &host, ResetScope::Settings).unwrap();
        let summary = format_reset_summary(&report);
        assert!(summary.contains("Reset Alan connections and settings"));
        assert!(summary.contains("removed 1 store item"));

        let second = reset_alan_stores(&system, &host, ResetScope::Settings).unwrap();
        assert!(format_reset_summary(&second).contains("already clean"));
    }
}
