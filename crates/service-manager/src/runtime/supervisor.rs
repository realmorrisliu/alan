//! Service supervision and service-handle lifecycle ownership.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat,
};
use alan_kernel::{Access, Credentials, LiveNamespace, Pid, Status};
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;

use crate::{
    BootManifest, BootUnit, ConnectionService, HostMountService, LocalEntryService, ManagerState,
    PackageService, RestartDecision,
    agent_runtime::{AgentRuntimeService, RootAgentProcess, RootAgentTemplate},
    process_spawn::spawn_unit_process,
};

/// A stable namespace mount whose backing File-Server exists only while its
/// owning service Process is running. Rebinding installs a fresh server so
/// buffered fids from a previous service lifetime cannot commit after restart.
pub(super) struct SwitchableFileServer {
    inner: tokio::sync::RwLock<Option<Arc<dyn FileServer>>>,
}

impl SwitchableFileServer {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: tokio::sync::RwLock::new(None),
        })
    }

    async fn bind(&self, inner: Arc<dyn FileServer>) {
        *self.inner.write().await = Some(inner);
    }

    async fn deactivate(&self) {
        *self.inner.write().await = None;
    }

    pub(super) async fn while_bound<T>(&self, action: impl FnOnce() -> Result<T>) -> Result<T> {
        let inner = self.inner.read().await;
        ensure!(inner.is_some(), "Package Service is unavailable");
        action()
    }
}

#[async_trait]
impl FileServer for SwitchableFileServer {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let inner = self.inner.read().await;
        inner
            .as_ref()
            .ok_or(ErrorCode::NoAccess)?
            .walk(fid, newfid, names)
            .await
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let inner = self.inner.read().await;
        inner
            .as_ref()
            .ok_or(ErrorCode::NoAccess)?
            .open(fid, mode)
            .await
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let inner = self.inner.read().await;
        inner
            .as_ref()
            .ok_or(ErrorCode::NoAccess)?
            .read(fid, offset, count)
            .await
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let inner = self.inner.read().await;
        inner
            .as_ref()
            .ok_or(ErrorCode::NoAccess)?
            .write(fid, offset, data)
            .await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let inner = self.inner.read().await;
        inner.as_ref().ok_or(ErrorCode::NoAccess)?.stat(fid).await
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let inner = self.inner.read().await;
        inner
            .as_ref()
            .ok_or(ErrorCode::NoAccess)?
            .create(fid, newfid, name, kind)
            .await
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let inner = self.inner.read().await;
        inner.as_ref().ok_or(ErrorCode::NoAccess)?.remove(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        let inner = self.inner.read().await;
        inner.as_ref().ok_or(ErrorCode::NoAccess)?.clunk(fid).await
    }
}

#[derive(Clone, Copy)]
pub(super) struct ActiveUnit {
    pub(super) pid: Pid,
    pub(super) started_at: Instant,
}

pub(super) struct SupervisorEnvironment {
    pub(super) root: RootAgentProcess,
    pub(super) root_template: RootAgentTemplate,
    pub(super) manifest: BootManifest,
    pub(super) state: Arc<tokio::sync::Mutex<ManagerState>>,
    pub(super) procfs: alan_kernel::ProcFs,
    pub(super) srvfs: Arc<alan_kernel::SrvFs>,
    pub(super) system_namespace: LiveNamespace,
    pub(super) agent_runtime: Arc<AgentRuntimeService>,
    pub(super) agent_root: Arc<alan_agentfs::AgentRootFs>,
    pub(super) llmfs: Arc<alan_llmfs::LlmFs>,
    pub(super) routefs: Arc<alan_routefs::RouteFs>,
    pub(super) host_mount: Arc<HostMountService>,
    pub(super) connection: Arc<ConnectionService>,
    pub(super) package: Arc<PackageService>,
    pub(super) package_handle: Arc<SwitchableFileServer>,
    pub(super) active_units: BTreeMap<String, ActiveUnit>,
    pub(super) manager_pid: Pid,
    pub(super) local_entry: Arc<LocalEntryService>,
}

pub(super) struct SupervisorRuntime {
    manifest: BootManifest,
    state: Arc<tokio::sync::Mutex<ManagerState>>,
    procfs: alan_kernel::ProcFs,
    srvfs: Arc<alan_kernel::SrvFs>,
    system_namespace: LiveNamespace,
    manager_pid: Pid,
    active: BTreeMap<String, ActiveUnit>,
    pending: BTreeMap<String, Instant>,
    agent_runtime: Arc<AgentRuntimeService>,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
    routefs: Arc<alan_routefs::RouteFs>,
    host_mount: Arc<HostMountService>,
    connection: Arc<ConnectionService>,
    package: Arc<PackageService>,
    package_handle: Arc<SwitchableFileServer>,
    local_entry: Arc<LocalEntryService>,
    root: Option<RootAgentProcess>,
    root_pid: Arc<AtomicU64>,
    root_template: RootAgentTemplate,
}

async fn run_supervisor(
    mut runtime: SupervisorRuntime,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_millis(25));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = interval.tick() => runtime.tick().await?,
        }
    }
    runtime.stop().await
}

impl SupervisorRuntime {
    pub(super) fn from_assembled(
        assembled: SupervisorEnvironment,
        root_pid: Arc<AtomicU64>,
    ) -> Self {
        Self {
            manifest: assembled.manifest,
            state: assembled.state,
            procfs: assembled.procfs,
            srvfs: assembled.srvfs,
            system_namespace: assembled.system_namespace,
            manager_pid: assembled.manager_pid,
            active: assembled.active_units,
            pending: BTreeMap::new(),
            agent_runtime: assembled.agent_runtime,
            agent_root: assembled.agent_root,
            llmfs: assembled.llmfs,
            routefs: assembled.routefs,
            host_mount: assembled.host_mount,
            connection: assembled.connection,
            package: assembled.package,
            package_handle: assembled.package_handle,
            local_entry: assembled.local_entry,
            root: Some(assembled.root),
            root_pid,
            root_template: assembled.root_template,
        }
    }

    pub(super) async fn settle_initial_root(&mut self) -> Result<()> {
        let initial_ready = self
            .root
            .as_mut()
            .context("Root Agent launch was not retained")?
            .wait_until_ready()
            .await;
        match initial_ready {
            Ok(_) => self
                .state
                .lock()
                .await
                .mark_ready("root-agent")
                .map_err(|error| anyhow::anyhow!("mark Root Agent ready: {error:?}")),
            Err(error) => {
                let active = self
                    .active
                    .get("root-agent")
                    .copied()
                    .context("Root Agent launch was not tracked")?;
                self.procfs.record_exit(active.pid, 1).await;
                self.handle_exit("root-agent", active, 1).await?;
                self.state
                    .lock()
                    .await
                    .note_error("root-agent", error.to_string())
                    .map_err(|code| anyhow::anyhow!("record Root Agent boot error: {code:?}"))?;
                loop {
                    let Some(deadline) = self.pending.remove("root-agent") else {
                        anyhow::bail!(
                            "Root Agent exhausted boot restart budget: {}",
                            self.state
                                .lock()
                                .await
                                .unit("root-agent")
                                .and_then(|unit| unit.error)
                                .unwrap_or_else(|| error.to_string())
                        );
                    };
                    tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
                    match self.launch("root-agent").await {
                        Ok(()) => break,
                        Err(error) => self.handle_launch_failure("root-agent", error).await?,
                    }
                }
                Ok(())
            }
        }
    }

    pub(super) fn start(
        self,
    ) -> (
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<Result<()>>,
    ) {
        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run_supervisor(self, shutdown_rx));
        (shutdown, task)
    }

    async fn tick(&mut self) -> Result<()> {
        for name in self.state.lock().await.take_retry_requests() {
            if !self.active.contains_key(&name) {
                self.pending.insert(name, Instant::now());
            }
        }

        let active = self
            .active
            .iter()
            .map(|(name, active)| (name.clone(), *active))
            .collect::<Vec<_>>();
        for (name, active) in active {
            if name == "root-agent"
                && self.root.as_ref().is_none_or(RootAgentProcess::is_finished)
                && self.procfs.try_observe_process_lifecycle(active.pid)
                    == Some((Status::Running, None))
            {
                self.procfs.record_exit(active.pid, 0).await;
            }
            if let Some((Status::Exited, exit_code)) =
                self.procfs.try_observe_process_lifecycle(active.pid)
            {
                self.handle_exit(&name, active, exit_code.unwrap_or(1))
                    .await?;
            }
        }

        let now = Instant::now();
        let due = self
            .pending
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        for name in due {
            self.pending.remove(&name);
            if let Err(error) = self.launch(&name).await {
                self.handle_launch_failure(&name, error).await?;
            }
        }
        Ok(())
    }

    async fn handle_exit(&mut self, name: &str, active: ActiveUnit, exit_code: i32) -> Result<()> {
        self.active.remove(name);
        self.invalidate_handles(name).await;
        if name == "root-agent" {
            if let Some(root) = self.root.take() {
                self.agent_runtime.detach_root(root, exit_code).await;
            }
            self.root_pid.store(0, Ordering::Release);
        }
        let stable_for_ms =
            u64::try_from(active.started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        let decision = self
            .state
            .lock()
            .await
            .record_exit(name, exit_code, stable_for_ms)
            .map_err(|error| anyhow::anyhow!("record `{name}` exit: {error:?}"))?;
        self.apply_restart_decision(name, decision);
        Ok(())
    }

    async fn handle_launch_failure(&mut self, name: &str, error: anyhow::Error) -> Result<()> {
        let pid = self.state.lock().await.unit(name).and_then(|unit| unit.pid);
        if let Some(pid) = pid {
            self.procfs.record_exit(Pid(pid), 1).await;
            self.active.remove(name);
            if name == "root-agent" {
                self.agent_runtime.release_process(Pid(pid)).await;
                self.root_pid.store(0, Ordering::Release);
            }
        }
        self.invalidate_handles(name).await;
        let mut state = self.state.lock().await;
        if pid.is_none() {
            state
                .start_failure(name, error.to_string())
                .map_err(|code| anyhow::anyhow!("track `{name}` launch failure: {code:?}"))?;
        }
        let decision = state
            .record_exit(name, 1, 0)
            .map_err(|code| anyhow::anyhow!("record `{name}` launch failure: {code:?}"))?;
        state
            .note_error(name, error.to_string())
            .map_err(|code| anyhow::anyhow!("record `{name}` launch error: {code:?}"))?;
        drop(state);
        self.apply_restart_decision(name, decision);
        Ok(())
    }

    fn apply_restart_decision(&mut self, name: &str, decision: RestartDecision) {
        if let RestartDecision::RestartAfterMs(delay) = decision {
            self.pending.insert(
                name.to_string(),
                Instant::now() + Duration::from_millis(delay),
            );
        }
    }

    async fn launch(&mut self, name: &str) -> Result<()> {
        let unit = self
            .manifest
            .get(name)
            .cloned()
            .with_context(|| format!("unknown Boot Unit `{name}`"))?;
        if name == "root-agent" {
            return self.launch_root(&unit).await;
        }

        let (pid, _) = spawn_unit_process(
            &self.procfs,
            self.manager_pid,
            &self.system_namespace,
            Credentials::system(),
            &unit,
            &[],
        )
        .await?;
        self.state
            .lock()
            .await
            .start_attempt(name, pid)
            .map_err(|error| anyhow::anyhow!("track `{name}` restart: {error:?}"))?;
        if name == "local-entry" {
            self.local_entry
                .set_service_pid(pid)
                .await
                .map_err(|error| anyhow::anyhow!("replace Local Entry Process: {error:?}"))?;
        }
        publish_unit_handles(
            &unit,
            &SystemServiceHandles {
                srvfs: &self.srvfs,
                agent_root: &self.agent_root,
                llmfs: &self.llmfs,
                routefs: &self.routefs,
                host_mount: &self.host_mount,
                connection: &self.connection,
                package: &self.package,
                package_handle: &self.package_handle,
                local_entry: Some(&self.local_entry),
            },
        )
        .await?;
        wait_unit_ready(&unit, pid, &self.procfs, &self.srvfs).await?;
        self.state
            .lock()
            .await
            .mark_ready(name)
            .map_err(|error| anyhow::anyhow!("mark `{name}` ready: {error:?}"))?;
        self.active.insert(
            name.to_string(),
            ActiveUnit {
                pid,
                started_at: Instant::now(),
            },
        );
        Ok(())
    }

    async fn launch_root(&mut self, unit: &BootUnit) -> Result<()> {
        let mut root = self
            .agent_runtime
            .launch_root(
                self.manager_pid,
                &self.system_namespace,
                unit,
                &self.root_template,
            )
            .await?;
        let pid = root.pid();
        if let Err(error) = self.state.lock().await.start_attempt("root-agent", pid) {
            self.agent_runtime.detach_root(root, 1).await;
            return Err(anyhow::anyhow!("track Root Agent restart: {error:?}"));
        }
        if let Err(error) = root.wait_until_ready().await {
            self.agent_runtime.detach_root(root, 1).await;
            return Err(error).context("replacement Root Agent failed before readiness");
        }
        self.state
            .lock()
            .await
            .mark_ready("root-agent")
            .map_err(|error| anyhow::anyhow!("mark Root Agent ready: {error:?}"))?;
        self.active.insert(
            unit.name.clone(),
            ActiveUnit {
                pid,
                started_at: Instant::now(),
            },
        );
        self.root_pid.store(pid.0, Ordering::Release);
        self.root = Some(root);
        Ok(())
    }

    async fn invalidate_handles(&self, name: &str) {
        if let Some(unit) = self.manifest.get(name) {
            for handle in &unit.published_handles {
                if handle == "package" {
                    self.package_handle.deactivate().await;
                }
                self.srvfs.unpost(handle).await;
            }
        }
    }

    async fn stop(mut self) -> Result<()> {
        self.package_handle.deactivate().await;
        if let Some(root) = self.root.take() {
            self.agent_runtime.shutdown_root(root).await?;
        }
        for (name, active) in self.active {
            self.procfs.record_exit(active.pid, 0).await;
            if let Some(unit) = self.manifest.get(&name) {
                for handle in &unit.published_handles {
                    self.srvfs.unpost(handle).await;
                }
            }
        }
        Ok(())
    }
}

pub(super) struct SystemServiceHandles<'a> {
    pub(super) srvfs: &'a Arc<alan_kernel::SrvFs>,
    pub(super) agent_root: &'a Arc<alan_agentfs::AgentRootFs>,
    pub(super) llmfs: &'a Arc<alan_llmfs::LlmFs>,
    pub(super) routefs: &'a Arc<alan_routefs::RouteFs>,
    pub(super) host_mount: &'a Arc<HostMountService>,
    pub(super) connection: &'a Arc<ConnectionService>,
    pub(super) package: &'a Arc<PackageService>,
    pub(super) package_handle: &'a Arc<SwitchableFileServer>,
    pub(super) local_entry: Option<&'a Arc<LocalEntryService>>,
}

pub(super) async fn publish_unit_handles(
    unit: &BootUnit,
    services: &SystemServiceHandles<'_>,
) -> Result<()> {
    for handle in &unit.published_handles {
        let tree = match handle.as_str() {
            "route" => InProcessTransport::new(services.routefs.clone()),
            "llm" => InProcessTransport::new(services.llmfs.clone()),
            "agent-runtime" => InProcessTransport::new(services.agent_root.clone()),
            "connection" => InProcessTransport::new(services.connection.file_server()),
            "package" => {
                services
                    .package_handle
                    .bind(services.package.file_server())
                    .await;
                InProcessTransport::new(services.package_handle.clone())
            }
            "host-mount" => InProcessTransport::new(services.host_mount.file_server()),
            "local-entry" => InProcessTransport::new(
                services
                    .local_entry
                    .context("Local Entry Service has no Process")?
                    .clone(),
            ),
            other => anyhow::bail!(
                "Boot Unit `{}` publishes unknown handle `{other}`",
                unit.name
            ),
        };
        services.srvfs.post(handle, tree, Access::ReadWrite).await;
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

pub(super) async fn wait_unit_ready(
    unit: &BootUnit,
    pid: Pid,
    procfs: &alan_kernel::ProcFs,
    srvfs: &Arc<alan_kernel::SrvFs>,
) -> Result<()> {
    tokio::time::timeout(std::time::Duration::from_millis(unit.timeout_ms), async {
        loop {
            match procfs.try_observe_process_lifecycle(pid) {
                Some((Status::Exited, exit_code)) => anyhow::bail!(
                    "unit `{}` exited before readiness with {:?}",
                    unit.name,
                    exit_code
                ),
                Some((Status::Running, _)) => {
                    let mut ready = true;
                    for handle in &unit.published_handles {
                        ready &= srvfs.lookup(handle).await.is_some();
                    }
                    if ready {
                        return Ok::<_, anyhow::Error>(());
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .with_context(|| format!("unit `{}` publication timed out", unit.name))??;
    Ok(())
}
