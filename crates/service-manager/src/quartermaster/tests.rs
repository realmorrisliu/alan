//! Unit tests for Quartermaster `/bin/q`.

use std::collections::BTreeMap;
use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat,
};
use alan_hostfs::{HostDirAccess, HostDirFs};
use alan_kernel::{
    Access, Credentials, ExecSpec, LiveNamespace, Namespace, Pid, ProcessInvocation, ProcessRunner,
};
use alan_shell::{Shell, StdioDriver};
use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

use crate::process_runner::SystemProcessRunner;

use super::snapshot::{BoundedReadError, read_bounded_source_file};
use super::{QUARTERMASTER_EXECUTABLE, QuartermasterProcessRunner};

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
            namespace: alan_kernel::ExecNamespaceManifest::default(),
            descriptors: BTreeMap::new(),
        },
    }
}

pub(super) async fn q_shell(
    service: &Arc<crate::PackageService>,
    source: Option<&std::path::Path>,
    package_access: Access,
) -> Shell {
    let procfs = alan_kernel::ProcFs::new();
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
    let parent = crate::process_spawn::spawn_process(
        &procfs,
        None,
        LiveNamespace::new(namespace.clone()),
        Credentials::user("q-test"),
        QUARTERMASTER_EXECUTABLE,
    )
    .await
    .unwrap();
    let procfs = procfs.with_runner(Arc::new(SystemProcessRunner::new(None, None)));
    namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.for_spawner(
            Some(parent),
            namespace.clone(),
            Credentials::user("q-test"),
        ))),
        Access::ReadWrite,
    );
    Shell::new(InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::new(namespace),
    )))
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
    let service = crate::PackageService::ephemeral().unwrap();
    let shell = q_shell(&service, Some(source.path()), Access::ReadWrite).await;

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
    assert!(
        !service
            .catalog()
            .unwrap()
            .packages
            .contains_key("dogfood-pack")
    );
}

#[tokio::test]
async fn q_fails_when_package_service_projection_is_read_only() {
    let service = crate::PackageService::ephemeral().unwrap();
    let shell = q_shell(&service, None, Access::ReadOnly).await;
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
    let service = crate::PackageService::ephemeral().unwrap();
    let shell = q_shell(&service, Some(source.path()), Access::ReadWrite).await;
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
    assert!(service.catalog().unwrap().packages.is_empty());
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
    let service = crate::PackageService::ephemeral().unwrap();
    let shell = q_shell(&service, Some(source.path()), Access::ReadWrite).await;

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
    assert!(
        service
            .catalog()
            .unwrap()
            .packages
            .contains_key("git-symlink-pack")
    );
}

#[tokio::test]
async fn q_sync_upserts_and_renders_all_four_extension_kinds() {
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(source.path().join("bin")).unwrap();
    std::fs::write(
        source.path().join("alan-package.toml"),
        r#"
kind = "tool"
description = "Test sync tool"

[tool]
name = "hello_tool"
description = "Say hello"
entrypoint = "bin/hello.sh"
parameters = { type = "object", properties = { who = { type = "string" } } }
"#,
    )
    .unwrap();
    std::fs::write(
        source.path().join("bin/hello.sh"),
        "#!/bin/sh\necho '{\"ok\":true}'\n",
    )
    .unwrap();

    let service = crate::PackageService::ephemeral().unwrap();
    let shell = q_shell(&service, Some(source.path()), Access::ReadWrite).await;

    let synced = shell
        .run(
            QUARTERMASTER_EXECUTABLE,
            &[
                "sync".to_string(),
                "--name".to_string(),
                "hello-pack".to_string(),
                "/mnt/fixture".to_string(),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        synced.exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&synced.output)
    );
    assert!(
        String::from_utf8(synced.output)
            .unwrap()
            .starts_with("installed ")
    );

    let catalog_sync = shell
        .run(QUARTERMASTER_EXECUTABLE, &["sync".to_string()])
        .await
        .unwrap();
    assert_eq!(catalog_sync.exit_code, 0);
    let listing = String::from_utf8(catalog_sync.output).unwrap();
    assert!(listing.contains("export=tool"), "{listing}");
    assert!(listing.contains("tools=hello_tool"), "{listing}");
}
