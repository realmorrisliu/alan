//! Quartermaster's `/bin/q` Process image.
//!
//! The image has no native Package Service or Host filesystem handle. It snapshots an explicitly
//! named tree through the invoking Process namespace and submits commands through `/mnt/package`.

use std::collections::BTreeMap;
use std::sync::Arc;

use alan_ap::{ErrorCode, FileKind, InProcessTransport};
use alan_kernel::{MountFs, ProcessInvocation, ProcessOutcome, ProcessRunner};
use alan_shell::{BoundedListError, Shell};
use anyhow::{Context, Result};
use async_trait::async_trait;

#[cfg(test)]
use crate::process_runner::SystemProcessRunner;

use crate::{
    PackageCatalog, PackageCommand, PackageCommandResult, PackageSnapshot, PackageSnapshotEntry,
};

pub(crate) const QUARTERMASTER_EXECUTABLE: &str = "/bin/q";
const PACKAGE_CONTROL_PATH: &str = "/mnt/package/ctl";
const PACKAGE_RESULT_PATH: &str = "/mnt/package/result";
const MAX_SOURCE_FILES: usize = 4_096;
const MAX_SOURCE_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_SOURCE_NODES: usize = 8_192;
const MAX_SOURCE_DIRECTORY_BYTES: usize = 8 * 1024 * 1024;

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
                ProcessOutcome::exited(2, format!("q: {message}\n{}", usage()).into_bytes())
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

enum QError {
    Usage(String),
    Protocol {
        action: &'static str,
        code: ErrorCode,
    },
    Operation(String),
}

enum BoundedReadError {
    Protocol(ErrorCode),
    LimitExceeded,
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
            let (source, explicit_name) = parse_install_args(&args[1..])?;
            let source_name = source_leaf(source)?;
            let derived = explicit_name.is_none();
            let package_id = explicit_name.unwrap_or_else(|| source_name.clone());
            crate::package::validate_package_id(&package_id).map_err(|error| {
                QError::Usage(if derived {
                    format!("{error}; choose a canonical package id with --name")
                } else {
                    error.to_string()
                })
            })?;
            let snapshot = snapshot_namespace_tree(shell, source, source_name).await?;
            let request_id = request_id("install");
            let result = submit(
                shell,
                PackageCommand::Install {
                    request_id: request_id.clone(),
                    package_id,
                    snapshot,
                },
                &request_id,
            )
            .await?;
            render_mutation(result)
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
            render_catalog(result)
        }
        "upgrade" => {
            if args.len() != 3 {
                return Err(QError::Usage(
                    "upgrade requires <package-id> <namespace-path>".to_string(),
                ));
            }
            crate::package::validate_package_id(&args[1])
                .map_err(|error| QError::Usage(error.to_string()))?;
            let source_name = source_leaf(&args[2])?;
            let snapshot = snapshot_namespace_tree(shell, &args[2], source_name).await?;
            let request_id = request_id("upgrade");
            let result = submit(
                shell,
                PackageCommand::Upgrade {
                    request_id: request_id.clone(),
                    package_id: args[1].clone(),
                    snapshot,
                },
                &request_id,
            )
            .await?;
            render_mutation(result)
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
            render_mutation(result)
        }
        _ => Err(QError::Usage(format!("unknown command `{verb}`"))),
    }
}

fn parse_install_args(args: &[String]) -> Result<(&str, Option<String>), QError> {
    match args {
        [source] => Ok((source, None)),
        [flag, package_id, source] if flag == "--name" => Ok((source, Some(package_id.clone()))),
        [source, flag, package_id] if flag == "--name" => Ok((source, Some(package_id.clone()))),
        _ => Err(QError::Usage(
            "install requires [--name <package-id>] <namespace-path>".to_string(),
        )),
    }
}

fn source_leaf(source: &str) -> Result<String, QError> {
    if !source.starts_with('/') || source == "/" {
        return Err(QError::Usage(
            "package source must be an absolute non-root namespace path".to_string(),
        ));
    }
    if !source
        .split('/')
        .filter(|component| !component.is_empty())
        .all(|component| component != "." && component != "..")
    {
        return Err(QError::Usage(
            "package source path must be normalized".to_string(),
        ));
    }
    source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|leaf| !leaf.is_empty())
        .map(str::to_string)
        .ok_or_else(|| QError::Usage("package source has no leaf name".to_string()))
}

async fn read_bounded_source_file(
    shell: &Shell,
    path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, BoundedReadError> {
    let mut reader = shell.tail(path).await.map_err(BoundedReadError::Protocol)?;
    let read_result = async {
        let mut bytes = Vec::new();
        loop {
            let remaining = max_bytes.saturating_sub(bytes.len() as u64);
            let count = remaining.saturating_add(1).min(4_096) as u32;
            let chunk = reader
                .read(count)
                .await
                .map_err(BoundedReadError::Protocol)?;
            if chunk.is_empty() {
                break;
            }
            if chunk.len() as u64 > remaining {
                return Err(BoundedReadError::LimitExceeded);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    .await;
    let close_result = reader.close().await;
    match (read_result, close_result) {
        (Err(error), _) => Err(error),
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Ok(_), Err(code)) => Err(BoundedReadError::Protocol(code)),
    }
}

async fn snapshot_namespace_tree(
    shell: &Shell,
    source: &str,
    source_name: String,
) -> Result<PackageSnapshot, QError> {
    let source = source.trim_end_matches('/');
    let root_entries = list_source_directory(shell, source, MAX_SOURCE_NODES)
        .await?
        .into_iter()
        .filter(|entry| entry != ".git")
        .collect::<Vec<_>>();
    let mut discovered_nodes = root_entries.len();
    let mut pending = vec![(source.to_string(), String::new(), root_entries)];
    let mut entries = Vec::new();
    let mut total_bytes = 0u64;
    while let Some((absolute, relative, children)) = pending.pop() {
        let mut children = children;
        children.sort();
        for child in children.into_iter().rev() {
            if child.is_empty() || child.contains('/') || child == "." || child == ".." {
                return Err(QError::Operation(
                    "source File-Server returned an invalid directory entry".to_string(),
                ));
            }
            if child == ".git" {
                continue;
            }
            let child_absolute = format!("{absolute}/{child}");
            let child_relative = if relative.is_empty() {
                child.clone()
            } else {
                format!("{relative}/{child}")
            };
            let remaining_nodes = MAX_SOURCE_NODES - discovered_nodes;
            match list_source_directory(shell, &child_absolute, remaining_nodes).await {
                Ok(grandchildren) => {
                    discovered_nodes += grandchildren.len();
                    pending.push((child_absolute, child_relative, grandchildren));
                }
                Err(QError::Protocol {
                    code: ErrorCode::NotDirectory,
                    ..
                }) => {
                    if entries.len() >= MAX_SOURCE_FILES {
                        return Err(QError::Operation(
                            "package source has too many files".to_string(),
                        ));
                    }
                    let stat =
                        shell
                            .stat(&child_absolute)
                            .await
                            .map_err(|code| QError::Protocol {
                                action: "inspect source",
                                code,
                            })?;
                    if stat.qid.kind != FileKind::File {
                        return Err(QError::Operation(
                            "package source contains a non-file leaf".to_string(),
                        ));
                    }
                    if stat.length > MAX_SOURCE_FILE_BYTES {
                        return Err(QError::Operation(
                            "package source file is too large".to_string(),
                        ));
                    }
                    let remaining_total = MAX_SOURCE_BYTES.saturating_sub(total_bytes);
                    if stat.length > remaining_total {
                        return Err(QError::Operation("package source is too large".to_string()));
                    }
                    let read_limit = MAX_SOURCE_FILE_BYTES.min(remaining_total);
                    let bytes = read_bounded_source_file(shell, &child_absolute, read_limit)
                        .await
                        .map_err(|error| match error {
                            BoundedReadError::Protocol(code) => QError::Protocol {
                                action: "read source",
                                code,
                            },
                            BoundedReadError::LimitExceeded
                                if remaining_total < MAX_SOURCE_FILE_BYTES =>
                            {
                                QError::Operation("package source is too large".to_string())
                            }
                            BoundedReadError::LimitExceeded => {
                                QError::Operation("package source file is too large".to_string())
                            }
                        })?;
                    if bytes.len() as u64 != stat.length {
                        return Err(QError::Operation(
                            "package source changed while it was being imported".to_string(),
                        ));
                    }
                    total_bytes = total_bytes.checked_add(bytes.len() as u64).ok_or_else(|| {
                        QError::Operation("package source size overflow".to_string())
                    })?;
                    entries.push(PackageSnapshotEntry {
                        path: child_relative,
                        bytes,
                        executable: stat.executable,
                    });
                }
                Err(error) => return Err(error),
            }
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(PackageSnapshot {
        source_name,
        entries,
    })
}

async fn list_source_directory(
    shell: &Shell,
    path: &str,
    remaining_nodes: usize,
) -> Result<Vec<String>, QError> {
    shell
        .ls_bounded(path, remaining_nodes, MAX_SOURCE_DIRECTORY_BYTES)
        .await
        .map_err(|error| match error {
            BoundedListError::Protocol(code) => QError::Protocol {
                action: "read source",
                code,
            },
            BoundedListError::LimitExceeded => {
                QError::Operation("package source has too many entries".to_string())
            }
        })
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

fn render_mutation(result: PackageCommandResult) -> Result<Vec<u8>, QError> {
    if !result.success {
        return Err(QError::Operation(format!(
            "{} failed: {}",
            result.action, result.message
        )));
    }
    let package = result.package;
    let detail = package
        .as_ref()
        .map(|package| format!(" {} {}", package.id, package.revision))
        .unwrap_or_default();
    Ok(format!("{}{}\n", result.message, detail).into_bytes())
}

fn render_catalog(result: PackageCommandResult) -> Result<Vec<u8>, QError> {
    if !result.success {
        return Err(QError::Operation(format!(
            "list failed: {}",
            result.message
        )));
    }
    let catalog: PackageCatalog = result
        .catalog
        .ok_or_else(|| QError::Operation("list result has no catalog".to_string()))?;
    if catalog.packages.is_empty() {
        return Ok(b"no packages installed\n".to_vec());
    }
    let mut output = String::new();
    for record in catalog.packages.values() {
        let kind = match record.kind {
            crate::PackageKind::Preinstalled => "preinstalled",
            crate::PackageKind::Installed => "installed",
        };
        let state = match record.state {
            crate::PackageState::Installed => "installed",
            crate::PackageState::Retiring => "retiring",
        };
        let skills = record
            .exports
            .iter()
            .map(|export| export.skill_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let issues = record
            .exports
            .iter()
            .flat_map(|export| export.dependencies.iter())
            .map(|dependency| dependency.identity_key())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        output.push_str(&format!(
            "{} {} {} {} refs={} skills={}",
            record.id, record.revision, kind, state, record.reference_count, skills
        ));
        if !issues.is_empty() {
            output.push_str(&format!(" unavailable={issues}"));
        }
        output.push('\n');
    }
    Ok(output.into_bytes())
}

fn request_id(action: &str) -> String {
    format!("q-{action}-{}", uuid::Uuid::new_v4().simple())
}

fn usage() -> &'static str {
    "usage: q install [--name <package-id>] <namespace-path>\n       q list\n       q upgrade <package-id> <namespace-path>\n       q uninstall <package-id>\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_ap::reference::MemFs;
    use alan_ap::{Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
    use alan_hostfs::{HostDirAccess, HostDirFs};
    use alan_kernel::{Access, Credentials, ExecSpec, Namespace, Pid};
    use alan_shell::StdioDriver;
    use async_trait::async_trait;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

    struct UnderreportedInfiniteFile {
        inner: MemFs,
    }

    impl UnderreportedInfiniteFile {
        fn new() -> Self {
            Self {
                inner: MemFs::with_read_only_file("SKILL.md", Vec::new()),
            }
        }
    }

    #[async_trait]
    impl FileServer for UnderreportedInfiniteFile {
        async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
            self.inner.walk(fid, newfid, names).await
        }

        async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
            self.inner.open(fid, mode).await
        }

        async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
            if self.inner.stat(fid).await?.qid.kind == FileKind::File {
                Ok(vec![b'x'; count as usize])
            } else {
                self.inner.read(fid, offset, count).await
            }
        }

        async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
            self.inner.write(fid, offset, data).await
        }

        async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
            let mut stat = self.inner.stat(fid).await?;
            if stat.qid.kind == FileKind::File {
                stat.length = 0;
            }
            Ok(stat)
        }

        async fn create(
            &self,
            fid: Fid,
            newfid: Fid,
            name: &str,
            kind: FileKind,
        ) -> Result<Qid, ErrorCode> {
            self.inner.create(fid, newfid, name, kind).await
        }

        async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
            self.inner.remove(fid).await
        }

        async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
            self.inner.clunk(fid).await
        }
    }

    fn invocation(namespace: Namespace, args: &[&str]) -> ProcessInvocation {
        ProcessInvocation {
            pid: Pid(2),
            parent: Some(Pid(1)),
            credentials: Credentials::user("test"),
            namespace,
            exec: ExecSpec {
                executable: QUARTERMASTER_EXECUTABLE.to_string(),
                args: args.iter().map(|value| (*value).to_string()).collect(),
                namespace: None,
                descriptors: BTreeMap::new(),
            },
        }
    }

    fn q_shell(
        service: &Arc<crate::PackageService>,
        source: Option<&std::path::Path>,
        package_access: Access,
    ) -> Shell {
        let procfs =
            alan_kernel::ProcFs::new().with_runner(Arc::new(SystemProcessRunner::new(None, None)));
        let mut namespace = Namespace::new();
        namespace.mount(
            QUARTERMASTER_EXECUTABLE,
            InProcessTransport::new(Arc::new(MemFs::empty())),
            Access::ReadOnly,
        );
        namespace.mount(
            "/mnt/package",
            InProcessTransport::new(service.file_server()),
            package_access,
        );
        if let Some(source) = source {
            namespace.mount(
                "/mnt/fixture",
                InProcessTransport::new(Arc::new(
                    HostDirFs::new(source, HostDirAccess::ReadOnly).unwrap(),
                )),
                Access::ReadOnly,
            );
        }
        namespace.mount(
            "/proc",
            InProcessTransport::new(Arc::new(procfs.for_spawner(
                None,
                namespace.clone(),
                Credentials::user("q-test"),
            ))),
            Access::ReadWrite,
        );
        Shell::new(InProcessTransport::new(Arc::new(MountFs::new(namespace))))
    }

    #[tokio::test]
    async fn unavailable_package_service_is_a_bounded_failure() {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/bin/q",
            InProcessTransport::new(Arc::new(MemFs::empty())),
            Access::ReadOnly,
        );
        let outcome = QuartermasterProcessRunner
            .run(invocation(namespace, &["list"]))
            .await;
        assert_eq!(outcome.exit_code, 1);
        assert!(
            String::from_utf8(outcome.output)
                .unwrap()
                .contains("submit command")
        );
    }

    #[tokio::test]
    async fn system_runner_rejects_q_without_an_executable_mount() {
        let outcome = SystemProcessRunner::new(None, None)
            .run(invocation(Namespace::new(), &["list"]))
            .await;

        assert_eq!(outcome.exit_code, 127);
        assert_eq!(outcome.output, b"executable is not mounted\n");
    }

    #[tokio::test]
    async fn malformed_command_returns_usage_exit() {
        let outcome = QuartermasterProcessRunner
            .run(invocation(Namespace::new(), &["install"]))
            .await;
        assert_eq!(outcome.exit_code, 2);
        assert!(
            String::from_utf8(outcome.output)
                .unwrap()
                .contains("usage: q")
        );

        let remote = QuartermasterProcessRunner
            .run(invocation(
                Namespace::new(),
                &["install", "https://example.invalid/package.git"],
            ))
            .await;
        assert_eq!(remote.exit_code, 2);
        assert!(
            String::from_utf8(remote.output)
                .unwrap()
                .contains("absolute")
        );

        let noncanonical_leaf = QuartermasterProcessRunner
            .run(invocation(
                Namespace::new(),
                &["install", "/mnt/Not-Canonical"],
            ))
            .await;
        assert_eq!(noncanonical_leaf.exit_code, 2);
        assert!(
            String::from_utf8(noncanonical_leaf.output)
                .unwrap()
                .contains("--name")
        );
    }

    #[tokio::test]
    async fn source_read_budget_rejects_an_infinite_file_that_underreports_stat() {
        let shell = Shell::new(InProcessTransport::new(Arc::new(
            UnderreportedInfiniteFile::new(),
        )));
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            read_bounded_source_file(&shell, "/SKILL.md", 16),
        )
        .await
        .expect("bounded source read must not wait for EOF");

        assert!(matches!(result, Err(BoundedReadError::LimitExceeded)));
    }

    #[tokio::test]
    async fn q_installs_and_lists_a_multi_skill_distribution_through_proc() {
        let source = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(source.path().join("research")).unwrap();
        std::fs::create_dir_all(source.path().join("shared")).unwrap();
        std::fs::create_dir_all(source.path().join("skills")).unwrap();
        std::fs::write(
            source.path().join("research/SKILL.md"),
            "---\nname: Research\ndescription: Research Skill.\n---\n\nUse shared data.\n",
        )
        .unwrap();
        std::fs::write(source.path().join("shared/data.txt"), "shared").unwrap();
        std::fs::write(source.path().join("shared/tool.sh"), "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                source.path().join("shared/tool.sh"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        std::fs::write(
            source.path().join("skills/web.md"),
            "Use WebSearch and preserve this body.",
        )
        .unwrap();
        let service = crate::PackageService::ephemeral("test").unwrap();
        let shell = q_shell(&service, Some(source.path()), Access::ReadWrite);

        let installed = shell
            .run(
                QUARTERMASTER_EXECUTABLE,
                &[
                    "install".to_string(),
                    "--name".to_string(),
                    "dogfood-pack".to_string(),
                    "/mnt/fixture".to_string(),
                ],
            )
            .await
            .unwrap();
        assert_eq!(
            installed.exit_code,
            0,
            "{}",
            String::from_utf8_lossy(&installed.output)
        );
        let host_source = source.path().to_string_lossy();
        assert!(!String::from_utf8_lossy(&installed.output).contains(host_source.as_ref()));
        let record = service.resolve("dogfood-pack").unwrap();
        assert_eq!(
            record
                .exports
                .iter()
                .map(|export| export.skill_id.as_str())
                .collect::<Vec<_>>(),
            vec!["research", "web"]
        );
        assert_eq!(
            record.exports[1].dependencies[0].identity_key(),
            "runtime_capability:web-search"
        );
        let lease = service.acquire("dogfood-pack").unwrap();
        let package_shell = Shell::new(InProcessTransport::new(lease.file_server().unwrap()));
        assert_eq!(
            package_shell.cat("/source/shared/data.txt").await.unwrap(),
            b"shared"
        );
        #[cfg(unix)]
        assert!(
            package_shell
                .stat("/source/shared/tool.sh")
                .await
                .unwrap()
                .executable
        );

        let listed = shell
            .run(QUARTERMASTER_EXECUTABLE, &["list".to_string()])
            .await
            .unwrap();
        let listing = String::from_utf8(listed.output).unwrap();
        assert!(listing.contains("dogfood-pack"), "{listing}");
        assert!(listing.contains("skills=research,web"), "{listing}");
        assert!(
            listing.contains("unavailable=runtime_capability:web-search"),
            "{listing}"
        );

        let driver = StdioDriver::new(shell.clone());
        let (client, server) = tokio::io::duplex(8 * 1024);
        let (mut client_read, mut client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let task = tokio::spawn(async move {
            driver
                .run(BufReader::new(server_read), server_write)
                .await
                .unwrap();
        });
        client_write.write_all(b"q list\nexit\n").await.unwrap();
        drop(client_write);
        let mut output = Vec::new();
        client_read.read_to_end(&mut output).await.unwrap();
        task.await.unwrap();
        assert!(String::from_utf8(output).unwrap().contains("dogfood-pack"));

        std::fs::write(
            source.path().join("research/SKILL.md"),
            "---\nname: Research\ndescription: Research Skill.\n---\n\nUpdated.\n",
        )
        .unwrap();
        assert!(source.path().join("research/SKILL.md").is_file());
        assert!(!shell.ls("/mnt/fixture").await.unwrap().is_empty());
        let upgraded = shell
            .run(
                QUARTERMASTER_EXECUTABLE,
                &[
                    "upgrade".to_string(),
                    "dogfood-pack".to_string(),
                    "/mnt/fixture".to_string(),
                ],
            )
            .await
            .unwrap();
        assert_eq!(
            upgraded.exit_code,
            0,
            "{}",
            String::from_utf8_lossy(&upgraded.output)
        );
        assert_ne!(
            service.resolve("dogfood-pack").unwrap().revision,
            lease.record().revision
        );
        let repeated = shell
            .run(
                QUARTERMASTER_EXECUTABLE,
                &[
                    "upgrade".to_string(),
                    "dogfood-pack".to_string(),
                    "/mnt/fixture".to_string(),
                ],
            )
            .await
            .unwrap();
        assert!(
            String::from_utf8(repeated.output)
                .unwrap()
                .starts_with("already current")
        );
        let removed = shell
            .run(
                QUARTERMASTER_EXECUTABLE,
                &["uninstall".to_string(), "dogfood-pack".to_string()],
            )
            .await
            .unwrap();
        assert!(
            String::from_utf8(removed.output)
                .unwrap()
                .starts_with("retiring")
        );
        assert!(service.resolve("dogfood-pack").is_err());
        drop(package_shell);
        drop(lease);
        assert!(!service.catalog().packages.contains_key("dogfood-pack"));
    }

    #[tokio::test]
    async fn q_fails_when_package_service_projection_is_read_only() {
        let service = crate::PackageService::ephemeral("test").unwrap();
        let shell = q_shell(&service, None, Access::ReadOnly);
        let result = shell
            .run(QUARTERMASTER_EXECUTABLE, &["list".to_string()])
            .await
            .unwrap();
        assert_eq!(result.exit_code, 1);
        assert!(
            String::from_utf8(result.output)
                .unwrap()
                .contains("NoAccess")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn q_aborts_source_import_when_host_adapter_rejects_a_symlink() {
        use std::os::unix::fs::symlink;

        let source = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(source.path().join("safe")).unwrap();
        std::fs::write(
            source.path().join("safe/SKILL.md"),
            "---\nname: Safe\ndescription: Safe Skill.\n---\n",
        )
        .unwrap();
        symlink("safe/SKILL.md", source.path().join("linked.md")).unwrap();
        let service = crate::PackageService::ephemeral("test").unwrap();
        let shell = q_shell(&service, Some(source.path()), Access::ReadWrite);
        let result = shell
            .run(
                QUARTERMASTER_EXECUTABLE,
                &[
                    "install".to_string(),
                    "--name".to_string(),
                    "symlink-pack".to_string(),
                    "/mnt/fixture".to_string(),
                ],
            )
            .await
            .unwrap();
        assert_eq!(result.exit_code, 1);
        let output = String::from_utf8(result.output).unwrap();
        assert!(
            output.contains("NoAccess") || output.contains("NotFound"),
            "{output}"
        );
        assert!(service.catalog().packages.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn q_excludes_a_git_symlink_without_traversing_it() {
        use std::os::unix::fs::symlink;

        let source = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(source.path().join("safe")).unwrap();
        std::fs::write(
            source.path().join("safe/SKILL.md"),
            "---\nname: Safe\ndescription: Safe Skill.\n---\n",
        )
        .unwrap();
        symlink("safe", source.path().join(".git")).unwrap();
        let service = crate::PackageService::ephemeral("test").unwrap();
        let shell = q_shell(&service, Some(source.path()), Access::ReadWrite);

        let result = shell
            .run(
                QUARTERMASTER_EXECUTABLE,
                &[
                    "install".to_string(),
                    "--name".to_string(),
                    "git-symlink-pack".to_string(),
                    "/mnt/fixture".to_string(),
                ],
            )
            .await
            .unwrap();

        assert_eq!(
            result.exit_code,
            0,
            "{}",
            String::from_utf8_lossy(&result.output)
        );
        assert!(service.catalog().packages.contains_key("git-symlink-pack"));
    }
}
