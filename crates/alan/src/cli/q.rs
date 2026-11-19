//! Quartermaster `alan q` CLI commands routed cleanly through Package Service and `/bin/q`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use alan_os_host::SystemStorePaths;
use alan_service_manager::{
    CLI_IMPORT_MOUNT_PATH, PackageService, run_q_command_with_package_service, validate_package_id,
};
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};

/// Top-level arguments for `alan q`.
#[derive(Debug, Args)]
pub struct QArgs {
    #[command(subcommand)]
    pub action: QAction,
}

/// Supported Quartermaster hub subcommands.
#[derive(Debug, Subcommand)]
pub enum QAction {
    /// Install a package (`skill`, `mcp`, `tool`, or `provider`) from a local directory.
    Install {
        /// Optional explicit canonical package identifier (`[a-z0-9]+(?:-[a-z0-9]+)*`).
        #[arg(long)]
        name: Option<String>,
        /// Local directory containing `SKILL.md` or `alan-package.toml` / `package.toml`.
        path: PathBuf,
    },
    /// List installed and preinstalled Quartermaster packages and their export kinds.
    List,
    /// Upgrade an operator-installed package from an updated local directory.
    Upgrade {
        /// Installed canonical package identifier.
        package_id: String,
        /// Local package directory.
        path: PathBuf,
    },
    /// Uninstall an operator-installed package cleanly.
    Uninstall {
        /// Installed canonical package identifier.
        package_id: String,
    },
    /// Verify installed packages or atomically install/upgrade from a local directory.
    Sync {
        /// Optional explicit canonical package identifier when syncing a local directory.
        #[arg(long)]
        name: Option<String>,
        /// Optional local package directory to install or upgrade atomically.
        path: Option<PathBuf>,
    },
}

/// Executes `alan q` against the current installation's Package Store via Service Manager boundaries.
pub async fn run(args: QArgs) -> Result<()> {
    alan_os_host::installation::validate_current_invocation()?;
    let _installation_access = alan_os_host::installation::InstallationPaths::detect()?.access()?;
    let system = SystemStorePaths::detect()?;
    let service = PackageService::open(system.packages()?)?;
    let output = run_with_service(&service, args).await?;
    print!("{output}");
    Ok(())
}

/// Executes `QArgs` against an explicit `PackageService` using read-only `/mnt/import/source` mounts
/// and `/bin/q` -> `/mnt/package/ctl` (never bypassing Service Manager).
pub async fn run_with_service(service: &Arc<PackageService>, args: QArgs) -> Result<String> {
    let (host_source_dir, q_args) = match args.action {
        QAction::Install { name, path } => {
            let canonical = canonical_package_source_dir(&path)?;
            let package_id = resolve_effective_package_id(name, &canonical)?;
            let q_args = vec![
                "install".to_string(),
                "--name".to_string(),
                package_id,
                CLI_IMPORT_MOUNT_PATH.to_string(),
            ];
            (Some(canonical), q_args)
        }
        QAction::List => (None, vec!["list".to_string()]),
        QAction::Upgrade { package_id, path } => {
            validate_package_id(&package_id)?;
            let canonical = canonical_package_source_dir(&path)?;
            let q_args = vec![
                "upgrade".to_string(),
                package_id,
                CLI_IMPORT_MOUNT_PATH.to_string(),
            ];
            (Some(canonical), q_args)
        }
        QAction::Uninstall { package_id } => {
            validate_package_id(&package_id)?;
            (None, vec!["uninstall".to_string(), package_id])
        }
        QAction::Sync { name, path } => match path {
            Some(path) => {
                let canonical = canonical_package_source_dir(&path)?;
                let package_id = resolve_effective_package_id(name, &canonical)?;
                let q_args = vec![
                    "sync".to_string(),
                    "--name".to_string(),
                    package_id,
                    CLI_IMPORT_MOUNT_PATH.to_string(),
                ];
                (Some(canonical), q_args)
            }
            None => {
                ensure!(
                    name.is_none(),
                    "`--name` requires passing a local `<path>` to `alan q sync`"
                );
                (None, vec!["sync".to_string()])
            }
        },
    };

    let result =
        run_q_command_with_package_service(service, host_source_dir.as_deref(), &q_args).await?;
    let text = String::from_utf8_lossy(&result.output).to_string();
    if result.exit_code != 0 {
        bail!("{}", text.trim_end());
    }
    Ok(text)
}

fn canonical_package_source_dir(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("inspect package source `{}`", path.display()))?;
    ensure!(
        canonical.is_dir(),
        "package source `{}` must be a directory",
        canonical.display()
    );
    Ok(canonical)
}

fn resolve_effective_package_id(explicit: Option<String>, source_dir: &Path) -> Result<String> {
    if let Some(name) = explicit {
        validate_package_id(&name)?;
        return Ok(name);
    }
    let leaf = source_dir
        .file_name()
        .and_then(|value| value.to_str())
        .with_context(|| {
            format!(
                "package directory `{}` has no valid UTF-8 leaf; pass `--name <package-id>`",
                source_dir.display()
            )
        })?;
    validate_package_id(leaf).with_context(|| {
        format!("directory leaf `{leaf}` is not a canonical package id; choose one with `--name`")
    })?;
    Ok(leaf.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cli_q_install_list_sync_upgrade_and_uninstall_round_trip() {
        let service = PackageService::ephemeral().unwrap();
        let pkg_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(pkg_dir.path().join("bin")).unwrap();
        std::fs::write(
            pkg_dir.path().join("alan-package.toml"),
            r#"
kind = "tool"
description = "CLI q test tool"

[tool]
name = "cli_probe"
description = "Probe tool"
entrypoint = "bin/probe.sh"
parameters = { type = "object" }
"#,
        )
        .unwrap();
        std::fs::write(
            pkg_dir.path().join("bin/probe.sh"),
            "#!/bin/sh\necho '{\"ok\":true}'\n",
        )
        .unwrap();

        let installed = run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: Some("cli-probe-pack".to_string()),
                    path: pkg_dir.path().to_path_buf(),
                },
            },
        )
        .await
        .unwrap();
        assert!(
            installed.starts_with("installed cli-probe-pack"),
            "{installed}"
        );

        let listing = run_with_service(
            &service,
            QArgs {
                action: QAction::List,
            },
        )
        .await
        .unwrap();
        assert!(listing.contains("cli-probe-pack"), "{listing}");
        assert!(listing.contains("export=tool"), "{listing}");

        std::fs::write(
            pkg_dir.path().join("bin/probe.sh"),
            "#!/bin/sh\necho '{\"v\":2}'\n",
        )
        .unwrap();
        let synced = run_with_service(
            &service,
            QArgs {
                action: QAction::Sync {
                    name: Some("cli-probe-pack".to_string()),
                    path: Some(pkg_dir.path().to_path_buf()),
                },
            },
        )
        .await
        .unwrap();
        assert!(synced.starts_with("upgraded cli-probe-pack"), "{synced}");

        let uninstalled = run_with_service(
            &service,
            QArgs {
                action: QAction::Uninstall {
                    package_id: "cli-probe-pack".to_string(),
                },
            },
        )
        .await
        .unwrap();
        assert!(uninstalled.starts_with("uninstalled"), "{uninstalled}");
    }
}
