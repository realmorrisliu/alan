//! Quartermaster `alan q` CLI commands routed cleanly through Package Service and `/bin/q`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use alan_os_host::SystemStorePaths;
use alan_service_manager::{
    CLI_IMPORT_MOUNT_PATH, FileserverMountAccess, FileserverTransportSpec, PackageExportKind,
    PackageKind, PackageRecord, PackageService, PackageState, RootBindingMode,
    RootFileserverActivation, RootPackageBinding, run_q_command_with_package_service,
    validate_package_id,
};
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand, ValueEnum};

/// Top-level arguments for `alan q`.
#[derive(Debug, Args)]
pub struct QArgs {
    #[command(subcommand)]
    pub action: QAction,
}

/// Access mode exposed when enabling a Quartermaster `fileserver` package.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum FileserverAccessArg {
    #[value(alias = "ro")]
    ReadOnly,
    #[value(alias = "rw")]
    ReadWrite,
}

impl From<FileserverAccessArg> for FileserverMountAccess {
    fn from(value: FileserverAccessArg) -> Self {
        match value {
            FileserverAccessArg::ReadOnly => Self::ReadOnly,
            FileserverAccessArg::ReadWrite => Self::ReadWrite,
        }
    }
}

/// Supported Quartermaster hub subcommands.
#[derive(Debug, Subcommand)]
pub enum QAction {
    /// Install a package (`skill`, `mcp`, `tool`, `provider`, or `fileserver`) from a local directory.
    Install {
        /// Optional explicit canonical package identifier (`[a-z0-9]+(?:-[a-z0-9]+)*`).
        #[arg(long)]
        name: Option<String>,
        /// Catalog the immutable revision only without attaching Companion Skills to Root.
        #[arg(long = "catalog-only")]
        catalog_only: bool,
        /// Local directory containing `SKILL.md` or `alan-package.toml` / `package.toml`.
        path: PathBuf,
    },
    /// Bind an installed package for the Root Agent (`Active` runtime or `--skills-only`).
    Enable {
        /// Installed canonical package identifier.
        package_id: String,
        /// Fileserver mount access (`read-only`/`ro` or `read-write`/`rw`; defaults to `read-only`).
        #[arg(long, value_enum)]
        access: Option<FileserverAccessArg>,
        /// Authorize outbound network inside OS confinement (requires manifest `allow_network = true`).
        #[arg(long = "allow-network")]
        allow_network: bool,
        /// Attach read-only Skills / Companion Skills only without enabling runtime execution.
        #[arg(long = "skills-only")]
        skills_only: bool,
    },
    /// Unbind an installed package from Root (optionally retaining `SkillsOnly` instructions).
    Disable {
        /// Installed canonical package identifier.
        package_id: String,
        /// Keep read-only Skills / Companion Skills attached while disabling runtime capabilities.
        #[arg(long = "keep-skills")]
        keep_skills: bool,
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
    match args.action {
        QAction::Inspect { package_id } => {
            validate_package_id(&package_id)?;
            let record = service.resolve(&package_id)?;
            let binding = service.root_binding(&package_id)?;
            return Ok(render_package_inspection(&record, binding.as_ref()));
        }
        QAction::Enable {
            package_id,
            access,
            allow_network,
            skills_only,
        } => {
            validate_package_id(&package_id)?;
            let record = service.resolve(&package_id)?;
            if skills_only {
                ensure!(
                    access.is_none() && !allow_network,
                    "`--skills-only` cannot be combined with `--access` or `--allow-network`"
                );
                service.bind_for_root(&package_id, RootBindingMode::SkillsOnly, None)?;
                let skills = comma_skill_ids(&record);
                return Ok(format!(
                    "enabled `{package_id}` in `skills_only` mode (skills: {}; takes effect on next `alan` invocation)\n",
                    if skills.is_empty() { "none" } else { &skills }
                ));
            }
            if record.export_kind == PackageExportKind::Fileserver {
                let fs_access = access
                    .map(Into::into)
                    .unwrap_or(FileserverMountAccess::ReadOnly);
                let activation = RootFileserverActivation {
                    access: fs_access,
                    allow_network,
                    host_grant_ids: Vec::new(),
                };
                service.bind_for_root(&package_id, RootBindingMode::Active, Some(activation))?;
                let service_id = record
                    .fileserver_export
                    .as_ref()
                    .map(|fs| fs.service_id.as_str())
                    .unwrap_or(package_id.as_str());
                return Ok(format!(
                    "enabled `{package_id}` (`active` fileserver `/mnt/{service_id}` access={} allow_network={allow_network}; takes effect on next `alan` invocation)\n",
                    fs_access.as_str()
                ));
            }
            ensure!(
                access.is_none() && !allow_network,
                "`--access` and `--allow-network` only apply to `fileserver` packages"
            );
            service.bind_for_root(&package_id, RootBindingMode::Active, None)?;
            return Ok(format!(
                "enabled `{package_id}` (`active` {}; takes effect on next `alan` invocation)\n",
                record.export_kind.as_str()
            ));
        }
        QAction::Disable {
            package_id,
            keep_skills,
        } => {
            validate_package_id(&package_id)?;
            if keep_skills {
                let record = service.resolve(&package_id)?;
                service.bind_for_root(&package_id, RootBindingMode::SkillsOnly, None)?;
                let skills = comma_skill_ids(&record);
                return Ok(format!(
                    "downgraded `{package_id}` to `skills_only` (skills kept: {}; runtime disabled on next `alan` invocation)\n",
                    if skills.is_empty() { "none" } else { &skills }
                ));
            }
            let removed = service.unbind_for_root(&package_id)?;
            return Ok(if removed {
                format!("disabled `{package_id}` from Root bindings\n")
            } else {
                format!("`{package_id}` was not bound to Root\n")
            });
        }
        _ => {}
    }

    let is_list = matches!(args.action, QAction::List);
    let mut post_install: Option<(String, bool)> = None;
    let (host_source_dir, q_args) = match args.action {
        QAction::Install {
            name,
            catalog_only,
            path,
        } => {
            let canonical = canonical_package_source_dir(&path)?;
            let package_id = resolve_effective_package_id(name, &canonical)?;
            post_install = Some((package_id.clone(), catalog_only));
            let q_args = vec![
                "install".to_string(),
                "--name".to_string(),
                package_id,
                CLI_IMPORT_MOUNT_PATH.to_string(),
            ];
            (Some(canonical), q_args)
        }
        QAction::List => (None, vec!["list".to_string()]),
        QAction::Inspect { .. } | QAction::Enable { .. } | QAction::Disable { .. } => {
            unreachable!("handled above")
        }
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
    let mut text = String::from_utf8_lossy(&result.output).to_string();
    if result.exit_code != 0 {
        bail!("{}", text.trim_end());
    }
    if let Some((package_id, catalog_only)) = post_install {
        text.push_str(&apply_post_install_binding(
            service,
            &package_id,
            catalog_only,
        )?);
    } else if is_list {
        text = annotate_list_with_root_bindings(service, &text)?;
    }
    Ok(text)
}

fn comma_skill_ids(record: &PackageRecord) -> String {
    record
        .exports
        .iter()
        .map(|export| export.skill_id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_root_binding_label(binding: Option<&RootPackageBinding>) -> String {
    match binding {
        None => "catalog".to_string(),
        Some(b) if b.mode == RootBindingMode::SkillsOnly => "skills".to_string(),
        Some(b) => match b.fileserver.as_ref() {
            Some(fs) => {
                let access = match fs.access {
                    FileserverMountAccess::ReadOnly => "ro",
                    FileserverMountAccess::ReadWrite => "rw",
                };
                if fs.allow_network {
                    format!("active({access},net)")
                } else {
                    format!("active({access})")
                }
            }
            None => "active".to_string(),
        },
    }
}

fn emit_inactive_runtime_hint(note: &mut String, package_id: &str, record: &PackageRecord) {
    use std::fmt::Write;
    if record.export_kind == PackageExportKind::Skill {
        return;
    }
    let hint = if let Some(fs) = record.fileserver_export.as_ref()
        && fs.allow_network
    {
        format!("alan q enable {package_id} --access read-write --allow-network")
    } else {
        format!("alan q enable {package_id}")
    };
    let _ = writeln!(note, "runtime: inactive (enable with `{hint}`)");
}

fn apply_post_install_binding(
    service: &Arc<PackageService>,
    package_id: &str,
    catalog_only: bool,
) -> Result<String> {
    use std::fmt::Write;
    let record = service.resolve(package_id)?;
    let mut note = String::new();

    if let Some(existing) = service.root_binding(package_id)? {
        let label = format_root_binding_label(Some(&existing));
        let _ = writeln!(note, "root_binding: preserved ({label})");
        if existing.mode == RootBindingMode::SkillsOnly {
            emit_inactive_runtime_hint(&mut note, package_id, &record);
        }
        return Ok(note);
    }

    if catalog_only {
        let _ = writeln!(
            note,
            "root_binding: catalog-only (run `alan q enable {package_id}` when ready)"
        );
        return Ok(note);
    }

    if !record.exports.is_empty() {
        match service.bind_for_root(package_id, RootBindingMode::SkillsOnly, None) {
            Ok(_) => {
                let _ = writeln!(note, "root_skills: attached ({})", comma_skill_ids(&record));
            }
            Err(err) => {
                let _ = writeln!(
                    note,
                    "warning: installed to catalog only; Companion Skills were not auto-attached: {err:#}"
                );
                return Ok(note);
            }
        }
    }

    emit_inactive_runtime_hint(&mut note, package_id, &record);
    Ok(note)
}

fn annotate_list_with_root_bindings(service: &Arc<PackageService>, raw: &str) -> Result<String> {
    if raw.trim() == "no packages installed" {
        return Ok(raw.to_string());
    }
    let bindings = service
        .list_root_bindings()?
        .into_iter()
        .map(|binding| (binding.package_id.clone(), binding))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut out = String::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(package_id) = trimmed.split_whitespace().next() else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let Ok(record) = service.resolve(package_id) else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let root_label = match record.kind {
            PackageKind::Preinstalled => "preinstalled".to_string(),
            PackageKind::Installed => format_root_binding_label(bindings.get(package_id)),
        };
        out.push_str(line);
        out.push_str(&format!(" root={root_label}"));
        if record.export_kind != PackageExportKind::Skill && !record.exports.is_empty() {
            let skills = record
                .exports
                .iter()
                .map(|export| export.skill_id.as_str())
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&format!(" companion_skills={skills}"));
        }
        out.push('\n');
    }
    Ok(out)
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

fn render_package_inspection(
    record: &PackageRecord,
    binding: Option<&RootPackageBinding>,
) -> String {
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
    let root_binding = match record.kind {
        PackageKind::Preinstalled => "preinstalled",
        PackageKind::Installed => match binding {
            None => "catalog-only",
            Some(b) => b.mode.as_str(),
        },
    };
    let _ = writeln!(out, "root_binding: {root_binding}");
    if let Some(fs) = binding.and_then(|b| b.fileserver.as_ref()) {
        let _ = writeln!(out, "root_fileserver_access: {}", fs.access.as_str());
        let _ = writeln!(out, "root_fileserver_allow_network: {}", fs.allow_network);
    }
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
    if record.export_kind != PackageExportKind::Skill && !record.exports.is_empty() {
        let _ = writeln!(out, "companion_skills: {}", comma_skill_ids(record));
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
                    catalog_only: false,
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
                    catalog_only: false,
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

    #[tokio::test]
    async fn cli_q_install_auto_attaches_companion_skills_and_enable_disable_controls_runtime() {
        let service = PackageService::ephemeral().unwrap();
        let pkg_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(pkg_dir.path().join("bin")).unwrap();
        std::fs::create_dir_all(pkg_dir.path().join("skills/guide")).unwrap();
        std::fs::write(
            pkg_dir.path().join("alan-package.toml"),
            r#"
schema_version = 1
kind = "fileserver"
version = "0.1.0"
[fileserver]
id = "guidefs"
entrypoint = "bin/guidefs"
allow_network = true
[fileserver.transport]
type = "stdio"
"#,
        )
        .unwrap();
        std::fs::write(pkg_dir.path().join("bin/guidefs"), "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(
            pkg_dir.path().join("skills/guide/SKILL.md"),
            "---\nname: guide\ndescription: Guide companion skill.\n---\nSetup guide.\n",
        )
        .unwrap();

        // 1. Default `alan q install` attaches Companion Skills (`SkillsOnly`) while keeping runtime inactive.
        let installed = run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: Some("guide-pack".to_string()),
                    catalog_only: false,
                    path: pkg_dir.path().to_path_buf(),
                },
            },
        )
        .await
        .unwrap();
        assert!(
            installed.contains("root_skills: attached (guide)"),
            "{installed}"
        );
        assert!(installed.contains("runtime: inactive"), "{installed}");
        assert_eq!(
            service.root_binding("guide-pack").unwrap().unwrap().mode,
            RootBindingMode::SkillsOnly
        );

        let inspected = run_with_service(
            &service,
            QArgs {
                action: QAction::Inspect {
                    package_id: "guide-pack".to_string(),
                },
            },
        )
        .await
        .unwrap();
        assert!(
            inspected.contains("root_binding: skills_only"),
            "{inspected}"
        );
        assert!(inspected.contains("companion_skills: guide"), "{inspected}");

        // 2. `enable` without escalation defaults to `ReadOnly` and `allow_network = false`.
        let enabled_ro = run_with_service(
            &service,
            QArgs {
                action: QAction::Enable {
                    package_id: "guide-pack".to_string(),
                    access: None,
                    allow_network: false,
                    skills_only: false,
                },
            },
        )
        .await
        .unwrap();
        assert!(enabled_ro.contains("access=read_only"), "{enabled_ro}");
        assert!(enabled_ro.contains("allow_network=false"), "{enabled_ro}");

        // 3. Explicit `enable --access rw --allow-network` updates binding cleanly.
        let enabled_rw = run_with_service(
            &service,
            QArgs {
                action: QAction::Enable {
                    package_id: "guide-pack".to_string(),
                    access: Some(FileserverAccessArg::ReadWrite),
                    allow_network: true,
                    skills_only: false,
                },
            },
        )
        .await
        .unwrap();
        assert!(enabled_rw.contains("access=read_write"), "{enabled_rw}");
        assert!(enabled_rw.contains("allow_network=true"), "{enabled_rw}");
        let list_rw = run_with_service(
            &service,
            QArgs {
                action: QAction::List,
            },
        )
        .await
        .unwrap();
        assert!(list_rw.contains("root=active(rw,net)"), "{list_rw}");
        assert!(list_rw.contains("companion_skills=guide"), "{list_rw}");

        // 4. `disable --keep-skills` downgrades back to `skills_only`; `disable` removes binding.
        let kept = run_with_service(
            &service,
            QArgs {
                action: QAction::Disable {
                    package_id: "guide-pack".to_string(),
                    keep_skills: true,
                },
            },
        )
        .await
        .unwrap();
        assert!(kept.contains("skills_only"), "{kept}");
        assert_eq!(
            service.root_binding("guide-pack").unwrap().unwrap().mode,
            RootBindingMode::SkillsOnly
        );

        run_with_service(
            &service,
            QArgs {
                action: QAction::Disable {
                    package_id: "guide-pack".to_string(),
                    keep_skills: false,
                },
            },
        )
        .await
        .unwrap();
        assert!(service.root_binding("guide-pack").unwrap().is_none());

        // 5. `--catalog-only` skips auto-attaching Root binding.
        let cat_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(cat_dir.path().join("bin")).unwrap();
        std::fs::create_dir_all(cat_dir.path().join("skills/helper")).unwrap();
        std::fs::write(
            cat_dir.path().join("alan-package.toml"),
            r#"
kind = "tool"
[tool]
name = "cat_tool"
description = "Cat"
entrypoint = "bin/t.sh"
parameters = { type = "object" }
"#,
        )
        .unwrap();
        std::fs::write(cat_dir.path().join("bin/t.sh"), "#!/bin/sh\necho '{}'\n").unwrap();
        std::fs::write(
            cat_dir.path().join("skills/helper/SKILL.md"),
            "---\nname: helper\ndescription: Helper.\n---\nBody.\n",
        )
        .unwrap();
        let cat_out = run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: Some("cat-pack".to_string()),
                    catalog_only: true,
                    path: cat_dir.path().to_path_buf(),
                },
            },
        )
        .await
        .unwrap();
        assert!(cat_out.contains("catalog-only"), "{cat_out}");
        assert!(service.root_binding("cat-pack").unwrap().is_none());

        // Reinstalling the same or `--catalog-only` package preserves existing Active binding.
        let repo_packages =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../service-manager/packages");
        let gh_pkg = repo_packages.join("github-fileserver");
        run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: None,
                    catalog_only: false,
                    path: gh_pkg.clone(),
                },
            },
        )
        .await
        .unwrap();
        run_with_service(
            &service,
            QArgs {
                action: QAction::Enable {
                    package_id: "github-fileserver".to_string(),
                    access: Some(FileserverAccessArg::ReadWrite),
                    allow_network: true,
                    skills_only: false,
                },
            },
        )
        .await
        .unwrap();
        for catalog_only in [false, true] {
            let reinstall = run_with_service(
                &service,
                QArgs {
                    action: QAction::Install {
                        name: None,
                        catalog_only,
                        path: gh_pkg.clone(),
                    },
                },
            )
            .await
            .unwrap();
            assert!(
                reinstall.contains("preserved (active(rw,net))"),
                "{reinstall}"
            );
            let preserved = service.root_binding("github-fileserver").unwrap().unwrap();
            assert_eq!(preserved.mode, RootBindingMode::Active);
            let fs = preserved.fileserver.unwrap();
            assert_eq!(fs.access, FileserverMountAccess::ReadWrite);
            assert!(fs.allow_network);
        }

        // Non-fileserver `--allow-network` fails cleanly.
        assert!(
            run_with_service(
                &service,
                QArgs {
                    action: QAction::Enable {
                        package_id: "cat-pack".to_string(),
                        access: None,
                        allow_network: true,
                        skills_only: false,
                    },
                },
            )
            .await
            .is_err()
        );
    }
}
