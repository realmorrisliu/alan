//! Quartermaster's `/bin/q` Process image.
//!
//! The image has no native Package Service or Host filesystem handle. It snapshots an explicitly
//! named tree through the invoking Process namespace and submits commands through `/mnt/package`.

mod render;
mod snapshot;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{ErrorCode, InProcessTransport};
use alan_hostfs::{HostDirAccess, HostDirFs};
use alan_kernel::{
    Access, Credentials, ExecSpec, MountFs, Namespace, Pid, ProcessInvocation, ProcessOutcome,
    ProcessRunner,
};
use alan_shell::Shell;
use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::{PackageCommand, PackageCommandResult, PackageService, PackageSyncEntry};

pub(crate) const QUARTERMASTER_EXECUTABLE: &str = "/bin/q";
const PACKAGE_CONTROL_PATH: &str = "/mnt/package/ctl";
const PACKAGE_RESULT_PATH: &str = "/mnt/package/result";
/// Mount path used by `alan q` when importing an explicit host directory through `HostDirFs`.
pub const CLI_IMPORT_MOUNT_PATH: &str = "/mnt/import/source";

/// Result returned when running a Quartermaster `/bin/q` CLI command via an explicit namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuartermasterCommandResult {
    /// Process exit code (`0` success, `2` usage error, non-zero operational failure).
    pub exit_code: i32,
    /// Terminal UTF-8 or raw byte output produced on stdout/stderr.
    pub output: Vec<u8>,
}

/// Runs `/bin/q` with `args` against `package_service`, optionally mounting `host_source_dir`
/// read-only at [`CLI_IMPORT_MOUNT_PATH`] so host CLI commands share the exact same snapshot and
/// `/mnt/package/ctl` boundary without bypassing Service Manager.
pub async fn run_q_command_with_package_service(
    package_service: &Arc<PackageService>,
    host_source_dir: Option<&Path>,
    args: &[String],
) -> Result<QuartermasterCommandResult> {
    let mut namespace = Namespace::new();
    namespace.mount(
        QUARTERMASTER_EXECUTABLE,
        InProcessTransport::new(Arc::new(MemFs::empty())),
        Access::ReadOnly,
    );
    namespace.mount(
        "/mnt/package",
        InProcessTransport::new(package_service.file_server()),
        Access::ReadWrite,
    );
    if let Some(source_dir) = host_source_dir {
        namespace.mount(
            CLI_IMPORT_MOUNT_PATH,
            InProcessTransport::new(Arc::new(HostDirFs::new(
                source_dir,
                HostDirAccess::ReadOnly,
            )?)),
            Access::ReadOnly,
        );
    }

    let outcome = QuartermasterProcessRunner
        .run(ProcessInvocation {
            pid: Pid(1),
            parent: None,
            credentials: Credentials::user("alan-cli"),
            namespace,
            exec: ExecSpec {
                executable: QUARTERMASTER_EXECUTABLE.to_string(),
                args: args.to_vec(),
                namespace: alan_kernel::ExecNamespaceManifest::default(),
                descriptors: BTreeMap::new(),
                link_parent: false,
            },
        })
        .await;
    Ok(QuartermasterCommandResult {
        exit_code: outcome.exit_code,
        output: outcome.output,
    })
}

/// Process runner backing the `/bin/q` Quartermaster CLI executable.
#[derive(Clone, Default)]
pub(crate) struct QuartermasterProcessRunner;

#[async_trait]
impl ProcessRunner for QuartermasterProcessRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        if invocation.exec.executable != QUARTERMASTER_EXECUTABLE {
            return ProcessOutcome::exited(127, b"q: executable mismatch\n");
        }
        let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(
            invocation.namespace,
        ))));
        match run_q(&shell, &invocation.exec.args).await {
            Ok(output) => ProcessOutcome::exited(0, output),
            Err(QError::Usage(message)) => {
                ProcessOutcome::exited(2, format!("q: {message}\n{}", render::usage()).into_bytes())
            }
            Err(QError::Protocol { action, code }) => {
                ProcessOutcome::exited(1, format!("q: {action} failed: {code:?}\n").into_bytes())
            }
            Err(QError::Operation(message)) => {
                ProcessOutcome::exited(1, format!("q: {message}\n").into_bytes())
            }
        }
    }
}

/// Errors produced while parsing arguments, reading snapshots, or submitting `/bin/q` commands.
pub(super) enum QError {
    /// Invalid CLI arguments or source path syntax.
    Usage(String),
    /// aP protocol failure while talking to source mounts or `/mnt/package`.
    Protocol {
        /// Human-readable operation description.
        action: &'static str,
        /// aP error code returned by the mount.
        code: ErrorCode,
    },
    /// Package Service operation failure or resource budget violation.
    Operation(String),
}

impl From<anyhow::Error> for QError {
    fn from(error: anyhow::Error) -> Self {
        Self::Operation(format!("{error:#}"))
    }
}

async fn run_q(shell: &Shell, args: &[String]) -> Result<Vec<u8>, QError> {
    let Some(verb) = args.first().map(String::as_str) else {
        return Err(QError::Usage("a command is required".to_string()));
    };
    match verb {
        "install" => {
            let (source, explicit_name) = parse_name_and_source(
                &args[1..],
                "install requires [--name <package-id>] <namespace-path>",
            )?;
            let (package_id, source_name) = resolve_package_id(source, explicit_name)?;
            let pkg_snapshot =
                snapshot::snapshot_namespace_tree(shell, source, source_name).await?;
            let request_id = request_id("install");
            let result = submit(
                shell,
                PackageCommand::Install {
                    request_id: request_id.clone(),
                    package_id,
                    snapshot: pkg_snapshot,
                },
                &request_id,
            )
            .await?;
            render::render_mutation(result)
        }
        "list" => {
            if args.len() != 1 {
                return Err(QError::Usage("list takes no arguments".to_string()));
            }
            let request_id = request_id("list");
            let result = submit(
                shell,
                PackageCommand::List {
                    request_id: request_id.clone(),
                },
                &request_id,
            )
            .await?;
            render::render_catalog(result)
        }
        "upgrade" => {
            if args.len() != 3 {
                return Err(QError::Usage(
                    "upgrade requires <package-id> <namespace-path>".to_string(),
                ));
            }
            crate::package::validate_package_id(&args[1])
                .map_err(|error| QError::Usage(error.to_string()))?;
            let source_name = snapshot::source_leaf(&args[2])?;
            let pkg_snapshot =
                snapshot::snapshot_namespace_tree(shell, &args[2], source_name).await?;
            let request_id = request_id("upgrade");
            let result = submit(
                shell,
                PackageCommand::Upgrade {
                    request_id: request_id.clone(),
                    package_id: args[1].clone(),
                    snapshot: pkg_snapshot,
                },
                &request_id,
            )
            .await?;
            render::render_mutation(result)
        }
        "uninstall" => {
            if args.len() != 2 {
                return Err(QError::Usage("uninstall requires <package-id>".to_string()));
            }
            crate::package::validate_package_id(&args[1])
                .map_err(|error| QError::Usage(error.to_string()))?;
            let request_id = request_id("uninstall");
            let result = submit(
                shell,
                PackageCommand::Uninstall {
                    request_id: request_id.clone(),
                    package_id: args[1].clone(),
                },
                &request_id,
            )
            .await?;
            render::render_mutation(result)
        }
        "sync" => {
            if args.len() == 1 {
                let request_id = request_id("sync");
                let result = submit(
                    shell,
                    PackageCommand::Sync {
                        request_id: request_id.clone(),
                        packages: Vec::new(),
                        prune: false,
                    },
                    &request_id,
                )
                .await?;
                render::render_catalog(result)
            } else {
                let (source, explicit_name) = parse_name_and_source(
                    &args[1..],
                    "sync requires optional [--name <package-id>] <namespace-path>",
                )?;
                let (package_id, source_name) = resolve_package_id(source, explicit_name)?;
                let pkg_snapshot =
                    snapshot::snapshot_namespace_tree(shell, source, source_name).await?;
                let request_id = request_id("sync");
                let result = submit(
                    shell,
                    PackageCommand::Sync {
                        request_id: request_id.clone(),
                        packages: vec![PackageSyncEntry {
                            package_id,
                            snapshot: pkg_snapshot,
                        }],
                        prune: false,
                    },
                    &request_id,
                )
                .await?;
                render::render_mutation(result)
            }
        }
        _ => Err(QError::Usage(format!("unknown command `{verb}`"))),
    }
}

fn resolve_package_id(
    source: &str,
    explicit_name: Option<String>,
) -> Result<(String, String), QError> {
    let source_name = snapshot::source_leaf(source)?;
    let derived = explicit_name.is_none();
    let package_id = explicit_name.unwrap_or_else(|| source_name.clone());
    crate::package::validate_package_id(&package_id).map_err(|error| {
        QError::Usage(if derived {
            format!("{error}; choose a canonical package id with --name")
        } else {
            error.to_string()
        })
    })?;
    Ok((package_id, source_name))
}

pub(super) fn parse_name_and_source<'a>(
    args: &'a [String],
    usage_error: &str,
) -> Result<(&'a str, Option<String>), QError> {
    match args {
        [source] => Ok((source, None)),
        [flag, package_id, source] if flag == "--name" => Ok((source, Some(package_id.clone()))),
        [source, flag, package_id] if flag == "--name" => Ok((source, Some(package_id.clone()))),
        _ => Err(QError::Usage(usage_error.to_string())),
    }
}

async fn submit(
    shell: &Shell,
    command: PackageCommand,
    request_id: &str,
) -> Result<PackageCommandResult, QError> {
    let command = serde_json::to_vec(&command).map_err(anyhow::Error::from)?;
    shell
        .write(PACKAGE_CONTROL_PATH, &command)
        .await
        .map_err(|code| QError::Protocol {
            action: "submit command",
            code,
        })?;
    let bytes = shell
        .cat(PACKAGE_RESULT_PATH)
        .await
        .map_err(|code| QError::Protocol {
            action: "read result",
            code,
        })?;
    let results: BTreeMap<String, PackageCommandResult> = serde_json::from_slice(&bytes)
        .context("decode Package Service result")
        .map_err(QError::from)?;
    results
        .get(request_id)
        .cloned()
        .ok_or_else(|| QError::Operation("Package Service returned no matching result".to_string()))
}

fn request_id(action: &str) -> String {
    format!("q-{action}-{}", uuid::Uuid::new_v4().simple())
}
