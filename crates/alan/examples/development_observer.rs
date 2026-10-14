//! Qualification-only attachment to an existing product instance; never boots an Agent.

use alan_os_host::{HostCommandPlane, HostEndpointPaths, LocalAttachment};
use alan_service_manager::HostMountAccess;
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

#[derive(Parser)]
struct Args {
    runtime: PathBuf,
    boot: Uuid,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Read {
        path: String,
        #[arg(long)]
        list: bool,
    },
    Mount {
        operation: Uuid,
        #[arg(value_enum)]
        access: Access,
        path: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Access {
    ReadOnly,
    ReadWrite,
}

fn validate_observation_path(path: &str) -> Result<()> {
    let parts = path.split('/').collect::<Vec<_>>();
    ensure!(
        parts.len() >= 4
            && parts[0].is_empty()
            && matches!(parts[1], "agent" | "proc")
            && parts[2].parse::<u64>().is_ok_and(|pid| pid != 0)
            && parts[3..]
                .iter()
                .all(|part| !matches!(*part, "" | "." | "..")),
        "observation requires a pinned numeric Process path"
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let paths = HostEndpointPaths::from_runtime_dir(&args.runtime)?;
    ensure!(
        paths.read_status()?.boot_id == args.boot,
        "Host boot changed"
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        match args.command {
            Command::Read { path, list } => {
                validate_observation_path(&path)?;
                let attached = LocalAttachment::new(paths).connect().await?;
                ensure!(attached.boot_id == args.boot, "attachment boot changed");
                let shell = alan_shell::Shell::new(attached.root);
                if list {
                    let entries =
                        shell
                            .ls_bounded(&path, 1024, 1 << 20)
                            .await
                            .map_err(|error| {
                                anyhow::anyhow!("bounded observation failed: {error:?}")
                            })?;
                    println!("{}", serde_json::to_string(&entries)?);
                } else {
                    let bytes = shell.cat(&path).await?;
                    print!("{}", String::from_utf8(bytes)?);
                }
            }
            Command::Mount {
                operation,
                access,
                path,
            } => {
                ensure!(
                    path.is_absolute(),
                    "qualification mount path must be absolute"
                );
                // Retain the operation before effects; uncertain transport permits same-ID reconciliation.
                println!(
                    "{}",
                    serde_json::json!({"phase":"intent","operation":operation,"boot":args.boot})
                );
                let access = match access {
                    Access::ReadOnly => HostMountAccess::ReadOnly,
                    Access::ReadWrite => HostMountAccess::ReadWrite,
                };
                let mounted = HostCommandPlane::new(paths)
                    .mount_project(operation, args.boot, path, access)
                    .await?;
                println!(
                    "{}",
                    serde_json::json!({"phase":"acknowledged","operation":operation,
                    "boot":args.boot,"grant":mounted.grant})
                );
            }
        }
        Ok(())
    })
    .await
    .context("qualification attachment timed out; retain any mount intent")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_rejects_aliases_traversal_and_unowned_store_paths() {
        for path in ["/agent/8/actions/a1/output", "/proc/9/status"] {
            assert!(validate_observation_path(path).is_ok());
        }
        for path in [
            "/agent/root/status",
            "/agent/0/status",
            "/agent/8/../9/status",
            "/agent/8//status",
            "/memory/MEMORY.md",
            "agent/8/status",
        ] {
            assert!(validate_observation_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn mount_requires_explicit_operation_boot_and_access() {
        let boot = "a925a8d1-3b47-4725-ac90-875dd08e6afb";
        let operation = "996fa726-1fe5-4c2c-af64-d598ff64df70";
        assert!(
            Args::try_parse_from([
                "observer",
                "/owned/runtime",
                boot,
                "mount",
                operation,
                "read-only",
                "/owned/project"
            ])
            .is_ok()
        );
        assert!(
            Args::try_parse_from([
                "observer",
                "/owned/runtime",
                "bad",
                "read",
                "/agent/8/status"
            ])
            .is_err()
        );
        assert!(
            Args::try_parse_from([
                "observer",
                "/owned/runtime",
                boot,
                "mount",
                operation,
                "unknown",
                "/owned/project"
            ])
            .is_err()
        );
    }

    #[test]
    fn directory_observation_requires_explicit_listing_and_pinned_owner() {
        let args = Args::try_parse_from([
            "observer",
            "/owned/runtime",
            "a925a8d1-3b47-4725-ac90-875dd08e6afb",
            "read",
            "/agent/8/actions",
            "--list",
        ])
        .unwrap();
        assert!(matches!(args.command, Command::Read { list: true, .. }));
        assert!(validate_observation_path("/agent/8/actions").is_ok());
        assert!(validate_observation_path("/agent/root/actions").is_err());
    }
}
