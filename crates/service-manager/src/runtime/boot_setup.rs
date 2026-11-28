//! Namespace mounting, handle publication, and event-driven unit readiness helpers.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use alan_agent_engine::ToolRegistry;
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, Credentials, LiveNamespace, Namespace, Pid, ProcessWaitRecord, Status};
use anyhow::{Context, Result, ensure};

use crate::{
    BootManifest, BootUnit, ConnectionService, HostMountService, LocalEntryService, ManagerState,
    PackageService, SupervisedHandle, quartermaster::QUARTERMASTER_EXECUTABLE,
};

pub(super) struct SystemServiceHandles<'a> {
    pub(super) srvfs: &'a Arc<alan_kernel::SrvFs>,
    pub(super) agent_root: &'a Arc<alan_agentfs::AgentRootFs>,
    pub(super) llmfs: &'a Arc<alan_llmfs::LlmFs>,
    pub(super) routefs: &'a Arc<alan_routefs::RouteFs>,
    pub(super) host_mount: &'a Arc<HostMountService>,
    pub(super) connection: &'a Arc<ConnectionService>,
    pub(super) package: &'a Arc<PackageService>,
    pub(super) supervised_handles: &'a BTreeMap<String, Arc<SupervisedHandle>>,
    pub(super) local_entry: Option<&'a Arc<LocalEntryService>>,
}

pub(super) async fn publish_unit_handles(
    unit: &BootUnit,
    services: &SystemServiceHandles<'_>,
) -> Result<()> {
    for handle in &unit.published_handles {
        let server: Arc<dyn FileServer> = match handle.as_str() {
            "route" => services.routefs.clone(),
            "llm" => services.llmfs.clone(),
            "agent-runtime" => services.agent_root.clone(),
            "connection" => services.connection.file_server(),
            "package" => services.package.file_server(),
            "host-mount" => services.host_mount.file_server(),
            "local-entry" => services
                .local_entry
                .context("Local Entry Service has no Process")?
                .clone(),
            other => anyhow::bail!(
                "Boot Unit `{}` publishes unknown handle `{other}`",
                unit.name
            ),
        };
        let supervised = services.supervised_handles.get(handle).with_context(|| {
            format!(
                "Boot Unit `{}` handle `{handle}` has no SupervisedHandle",
                unit.name
            )
        })?;
        supervised.bind(server);
        services
            .srvfs
            .post(
                handle,
                InProcessTransport::new(supervised.clone()),
                Access::ReadWrite,
            )
            .await;
    }
    Ok(())
}

pub(super) async fn mount_service_handles(
    namespace: &LiveNamespace,
    srvfs: &Arc<alan_kernel::SrvFs>,
) -> Result<()> {
    let (llm_tree, llm_access) = srvfs.lookup("llm").await.context("lookup /srv/llm")?;
    namespace.replace_mount("/mnt/llm", llm_tree, llm_access);
    let (route_tree, route_access) = srvfs
        .lookup(alan_routefs::SRV_HANDLE)
        .await
        .context("lookup /srv/route")?;
    namespace.replace_mount(alan_routefs::MOUNT_PATH, route_tree, route_access);
    let (connection_tree, connection_access) = srvfs
        .lookup("connection")
        .await
        .context("lookup /srv/connection")?;
    namespace.replace_mount("/mnt/connections", connection_tree, connection_access);
    let (manager_tree, manager_access) = srvfs
        .lookup("service-manager")
        .await
        .context("lookup /srv/service-manager")?;
    namespace.replace_mount("/mnt/service-manager", manager_tree, manager_access);
    let (host_mount_tree, host_mount_access) = srvfs
        .lookup("host-mount")
        .await
        .context("lookup /srv/host-mount")?;
    namespace.replace_mount("/mnt/host-mount", host_mount_tree, host_mount_access);
    let (package_tree, package_access) = srvfs
        .lookup("package")
        .await
        .context("lookup /srv/package")?;
    namespace.replace_mount("/mnt/package", package_tree, package_access);
    Ok(())
}

/// Open a dedicated read-only `/proc/<pid>/wait` fid on a fresh `/proc` view.
pub(crate) async fn open_proc_wait(
    procfs: &alan_kernel::ProcFs,
    pid: Pid,
) -> Result<(alan_kernel::ProcFs, Fid)> {
    let view = procfs.for_spawner(None, Namespace::new(), Credentials::system());
    let fid = Fid(1);
    view.walk(Fid::ROOT, fid, &[pid.0.to_string(), "wait".to_string()])
        .await
        .map_err(|err| anyhow::anyhow!("walk /proc/{}/wait: {err:?}", pid.0))?;
    if let Err(err) = view.open(fid, OpenMode::Read).await {
        let _ = view.clunk(fid).await;
        anyhow::bail!("open /proc/{}/wait: {err:?}", pid.0);
    }
    Ok((view, fid))
}

/// Read the single terminal JSON line from an already-opened `/proc/<pid>/wait` fid.
pub(crate) async fn read_opened_wait_record(
    view: &alan_kernel::ProcFs,
    fid: Fid,
    pid: Pid,
) -> Result<ProcessWaitRecord> {
    let mut bytes = Vec::new();
    let mut offset = 0_u64;
    loop {
        let chunk = view
            .read(fid, offset, 4096)
            .await
            .map_err(|err| anyhow::anyhow!("read /proc/{}/wait: {err:?}", pid.0))?;
        if chunk.is_empty() {
            break;
        }
        offset += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
        if bytes.contains(&b'\n') {
            break;
        }
    }
    let line = std::str::from_utf8(&bytes)
        .with_context(|| format!("decode /proc/{}/wait utf8", pid.0))?
        .lines()
        .next()
        .with_context(|| format!("empty /proc/{}/wait stream", pid.0))?;
    serde_json::from_str::<ProcessWaitRecord>(line)
        .with_context(|| format!("parse /proc/{}/wait record", pid.0))
}

/// Wait for all handles declared by `unit` to appear in `/srv` without polling,
/// aborting immediately if `/proc/<pid>/wait` reports process exit or `unit.timeout_ms` elapses.
pub(super) async fn wait_unit_ready(
    unit: &BootUnit,
    pid: Pid,
    procfs: &alan_kernel::ProcFs,
    srvfs: &Arc<alan_kernel::SrvFs>,
) -> Result<()> {
    let (wait_view, wait_fid) = open_proc_wait(procfs, pid).await?;
    let wait_result = tokio::time::timeout(Duration::from_millis(unit.timeout_ms), async {
        let mut srv_changes = srvfs.subscribe();
        let mut exit_fut = std::pin::pin!(read_opened_wait_record(&wait_view, wait_fid, pid));
        loop {
            if let Some((Status::Exited, exit_code)) = procfs.try_observe_process_lifecycle(pid) {
                anyhow::bail!(
                    "unit `{}` exited before readiness with {:?}",
                    unit.name,
                    exit_code
                );
            }
            let mut all_ready = true;
            for handle in &unit.published_handles {
                if srvfs.lookup(handle).await.is_none() {
                    all_ready = false;
                    break;
                }
            }
            if all_ready {
                if let Some((Status::Exited, exit_code)) = procfs.try_observe_process_lifecycle(pid)
                {
                    anyhow::bail!(
                        "unit `{}` exited before readiness with {:?}",
                        unit.name,
                        exit_code
                    );
                }
                return Ok::<_, anyhow::Error>(());
            }
            tokio::select! {
                biased;
                exit = &mut exit_fut => {
                    let record = exit?;
                    anyhow::bail!(
                        "unit `{}` exited before readiness with Some({})",
                        unit.name,
                        record.exit_code
                    );
                }
                changed = srv_changes.changed() => {
                    if changed.is_err() {
                        anyhow::bail!(
                            "unit `{}` /srv registry closed before readiness",
                            unit.name
                        );
                    }
                }
            }
        }
    })
    .await;

    let _ = wait_view.clunk(wait_fid).await;

    wait_result.with_context(|| format!("unit `{}` publication timed out", unit.name))??;
    Ok(())
}

pub(super) fn mount_standard_namespace_roots(namespace: &mut Namespace) {
    for path in ["/bin", "/lib", "/mnt"] {
        namespace.mount(
            path,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
    }
    namespace.mount(
        "/man",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_tree(
            crate::manpages::tree_entries(),
        ))),
        Access::ReadOnly,
    );
}

pub(super) fn mount_boot_units(namespace: &mut Namespace) {
    namespace.mount(
        "/lib/boot",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_files(
            BootManifest::documents()
                .into_iter()
                .map(|(name, document)| (name.to_string(), document.as_bytes().to_vec())),
        ))),
        Access::ReadOnly,
    );
}

pub(super) fn mount_system_executables(namespace: &mut Namespace, manifest: &BootManifest) {
    for executable in manifest
        .ordered()
        .map(|unit| unit.executable.as_str())
        .chain([
            super::SERVICE_MANAGER_EXECUTABLE,
            "/bin/alan-shell",
            QUARTERMASTER_EXECUTABLE,
            crate::agent_work::EXECUTABLE,
        ])
    {
        namespace.mount(
            executable,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
    }
}

pub(super) fn mount_tool_packages(namespace: &mut Namespace, tools: &ToolRegistry) -> Result<()> {
    namespace.mount(
        "/lib/exec/agent_work",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "manifest",
            crate::agent_work::manifest(),
        ))),
        Access::ReadOnly,
    );
    for name in tools.list_tools() {
        namespace.mount(
            &format!("/bin/{name}"),
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
        namespace.mount(
            &format!("/lib/exec/{name}"),
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                "manifest",
                tools.package_manifest_bytes(name)?,
            ))),
            Access::ReadOnly,
        );
    }
    Ok(())
}

pub(super) async fn verify_readiness(
    root: &InProcessTransport,
    boot_id: uuid::Uuid,
    state: &Arc<tokio::sync::Mutex<ManagerState>>,
) -> Result<()> {
    let shell = alan_shell::Shell::new(root.clone());
    let root_entries = shell.ls("/").await.context("read Standard Namespace")?;
    for required in ["proc", "agent", "srv", "bin", "lib", "man", "mnt"] {
        ensure!(
            root_entries.iter().any(|entry| entry == required),
            "Standard Namespace is missing /{required}"
        );
    }
    shell.ls("/agent/root").await.context("read /agent/root")?;
    let published_boot_id = String::from_utf8(shell.cat(super::BOOT_ID_PATH).await?)?;
    ensure!(
        published_boot_id.trim() == boot_id.to_string(),
        "boot ID mismatch"
    );
    ensure!(
        shell.cat(super::BOOT_STATE_PATH).await? == b"ready\n",
        "Host readiness file is not ready"
    );
    let service_handles = shell.ls("/srv").await.context("read /srv")?;
    for required in [
        "service-manager",
        "agent-runtime",
        "connection",
        "package",
        "host-mount",
        "local-entry",
        "llm",
        alan_routefs::SRV_HANDLE,
    ] {
        ensure!(
            service_handles.iter().any(|entry| entry == required),
            "required service /srv/{required} is absent"
        );
    }
    ensure!(
        state.lock().await.status() == crate::SystemStatus::Ready,
        "Service Manager did not reach ready"
    );
    Ok(())
}
