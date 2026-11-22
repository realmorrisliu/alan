//! Quartermaster `alan q` CLI commands routed cleanly through Package Service and `/bin/q`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use alan_os_host::SystemStorePaths;
use alan_service_manager::{
    CLI_IMPORT_MOUNT_PATH, FileserverTransportSpec, PackageExportKind, PackageKind, PackageRecord,
    PackageService, PackageState, run_q_command_with_package_service, validate_package_id,
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
    /// Install a package (`skill`, `mcp`, `tool`, `provider`, or `fileserver`) from a local directory.
    Install {
        /// Optional explicit canonical package identifier (`[a-z0-9]+(?:-[a-z0-9]+)*`).
        #[arg(long)]
        name: Option<String>,
        /// Local directory containing `SKILL.md` or `alan-package.toml` / `package.toml`.
        path: PathBuf,
    },
    /// List installed and preinstalled Quartermaster packages and their export kinds.
    List,
    /// Inspect one installed Quartermaster package and its export metadata.
    Inspect {
        /// Installed canonical package identifier.
        package_id: String,
    },
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
    if let QAction::Inspect { package_id } = args.action {
        validate_package_id(&package_id)?;
        let record = service.resolve(&package_id)?;
        return Ok(render_package_inspection(&record));
    }

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
        QAction::Inspect { .. } => unreachable!("inspect handled above"),
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

fn render_package_inspection(record: &PackageRecord) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let kind = match record.kind {
        PackageKind::Preinstalled => "preinstalled",
        PackageKind::Installed => "installed",
    };
    let state = match record.state {
        PackageState::Installed => "installed",
        PackageState::Retiring => "retiring",
    };
    let _ = writeln!(out, "package: {}", record.id);
    let _ = writeln!(out, "revision: {}", record.revision);
    let _ = writeln!(out, "ownership: {kind}");
    let _ = writeln!(out, "state: {state}");
    let _ = writeln!(out, "export_kind: {}", record.export_kind.as_str());
    let _ = writeln!(out, "refs: {}", record.reference_count);
    if let Some(version) = record.manifest_version.as_deref() {
        let _ = writeln!(out, "version: {version}");
    }
    if let Some(description) = record.description.as_deref() {
        let _ = writeln!(out, "description: {description}");
    }

    match record.export_kind {
        PackageExportKind::Skill => {
            let skills = record
                .exports
                .iter()
                .map(|export| export.skill_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "skills: {skills}");
        }
        PackageExportKind::Tool => {
            let tools = record
                .tool_exports
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "tools: {tools}");
        }
        PackageExportKind::Mcp => {
            let servers = record
                .mcp_exports
                .iter()
                .map(|mcp| mcp.server_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "mcps: {servers}");
        }
        PackageExportKind::Provider => {
            if let Some(provider) = record.provider_export.as_ref() {
                let _ = writeln!(
                    out,
                    "provider: {} ({})",
                    provider.provider_id, provider.protocol
                );
            }
        }
        PackageExportKind::Fileserver => {
            if let Some(fs) = record.fileserver_export.as_ref() {
                let _ = writeln!(out, "service_id: {}", fs.service_id);
                let _ = writeln!(out, "srv_handle: /srv/{}", fs.service_id);
                let _ = writeln!(out, "mount_path: /mnt/{}", fs.service_id);
                let _ = writeln!(out, "entrypoint: {}", fs.entrypoint);
                match &fs.transport {
                    FileserverTransportSpec::Stdio { command, args, .. } => {
                        let _ = writeln!(
                            out,
                            "transport: stdio (command={}, args={:?})",
                            command, args
                        );
                    }
                    FileserverTransportSpec::UnixSocket { socket_name } => {
                        let _ = writeln!(out, "transport: unix_socket (socket={socket_name})");
                    }
                }
                let _ = writeln!(out, "allow_network: {}", fs.allow_network);
                if !fs.network_domains.is_empty() {
                    let _ = writeln!(out, "network_domains: {}", fs.network_domains.join(", "));
                }
                let _ = writeln!(out, "timeout_secs: {}", fs.timeout_secs);
                let _ = writeln!(out, "max_message_bytes: {}", fs.max_message_bytes);
            }
        }
    }

    out
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

    #[tokio::test]
    async fn cli_q_install_list_and_inspect_displays_fileserver_package() {
        let service = PackageService::ephemeral().unwrap();
        let pkg_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(pkg_dir.path().join("bin")).unwrap();
        std::fs::write(
            pkg_dir.path().join("alan-package.toml"),
            r#"
schema_version = 1
kind = "fileserver"
version = "0.1.0"
description = "Demo fileserver plugin"

[fileserver]
id = "demo-fs"
entrypoint = "bin/demo-fs"
allow_network = true
network_domains = ["api.github.com"]
timeout_secs = 20
max_message_bytes = 65536

[fileserver.transport]
type = "stdio"
args = ["--serve"]
"#,
        )
        .unwrap();
        std::fs::write(pkg_dir.path().join("bin/demo-fs"), "#!/bin/sh\nexit 0\n").unwrap();

        let installed = run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: Some("demo-fileserver".to_string()),
                    path: pkg_dir.path().to_path_buf(),
                },
            },
        )
        .await
        .unwrap();
        assert!(
            installed.starts_with("installed demo-fileserver"),
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
        assert!(listing.contains("demo-fileserver"), "{listing}");
        assert!(listing.contains("export=fileserver"), "{listing}");
        assert!(
            listing.contains("fileserver=demo-fs(stdio)"),
            "expected fileserver summary in listing: {listing}"
        );

        let inspected = run_with_service(
            &service,
            QArgs {
                action: QAction::Inspect {
                    package_id: "demo-fileserver".to_string(),
                },
            },
        )
        .await
        .unwrap();
        assert!(inspected.contains("export_kind: fileserver"), "{inspected}");
        assert!(inspected.contains("service_id: demo-fs"), "{inspected}");
        assert!(
            inspected.contains("srv_handle: /srv/demo-fs"),
            "{inspected}"
        );
        assert!(
            inspected.contains("mount_path: /mnt/demo-fs"),
            "{inspected}"
        );
        assert!(inspected.contains("transport: stdio"), "{inspected}");
        assert!(
            inspected.contains("network_domains: api.github.com"),
            "{inspected}"
        );
    }
}
