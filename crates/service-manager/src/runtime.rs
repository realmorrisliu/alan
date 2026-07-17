//! Service Manager boot and runtime ownership.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use alan_agent_engine::{
    AgentProcessConfig, LlmClient, ProcessLaunchContext, ProcessPackageKind,
    ProcessPackageReference, ProcessPackageSkillReference, ToolRegistry,
    configure_runtime_tool_execution_binding, provider_capabilities_for_config,
};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat,
};
use alan_kernel::{Access, Credentials, LiveNamespace, Namespace, Pid, Status};
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    BootManifest, BootUnit, ConnectionService, ConnectionStoreBindings, ConnectionsFile,
    HostMountExportAdapter, HostMountService, LocalEntryService, ManagerState, PackageService,
    RestartDecision, ServiceManagerFs, UnavailableHostMountExportAdapter,
    agent_runtime::{
        AgentRuntimeFileServers, AgentRuntimeService, RootAgentProcess, RootAgentTemplate,
    },
    quartermaster::QUARTERMASTER_EXECUTABLE,
};

pub const BOOT_ID_PATH: &str = "/proc/host/boot_id";
pub const BOOT_STATE_PATH: &str = "/proc/host/state";

const LLM_CONNECTION: &str = "default";
const SERVICE_MANAGER_EXECUTABLE: &str = "/bin/service-manager";
static NEXT_BOOT_FID: AtomicU64 = AtomicU64::new(800_000);

/// A stable namespace mount whose backing File-Server exists only while its
/// owning service Process is running. Rebinding installs a fresh server so
/// buffered fids from a previous service lifetime cannot commit after restart.
struct SwitchableFileServer {
    inner: tokio::sync::RwLock<Option<Arc<dyn FileServer>>>,
}

impl SwitchableFileServer {
    fn new() -> Arc<Self> {
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

    async fn while_bound<T>(&self, action: impl FnOnce() -> Result<T>) -> Result<T> {
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

/// Explicit inputs supplied by the platform Host to Service Manager.
pub struct ServiceManagerConfig {
    pub channel_id: String,
    pub process: AgentProcessConfig,
    pub connection_store: Option<ConnectionStoreBindings>,
    pub package_store: Option<std::path::PathBuf>,
    pub llm_factory: Arc<dyn LlmClientFactory>,
    pub host_mount_adapter: Arc<dyn HostMountExportAdapter>,
    pub tools: ToolRegistry,
}

pub trait LlmClientFactory: std::fmt::Debug + Send + Sync {
    fn create(
        &self,
        base_config: &alan_agent_engine::Config,
        selected_profile: Option<&str>,
        connections: &ConnectionsFile,
    ) -> Result<LlmClient>;
}

#[derive(Debug)]
struct OneShotLlmClientFactory(std::sync::Mutex<Option<LlmClient>>);

impl LlmClientFactory for OneShotLlmClientFactory {
    fn create(
        &self,
        _base_config: &alan_agent_engine::Config,
        _selected_profile: Option<&str>,
        _connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("LLM factory lock poisoned"))?
            .take()
            .context("ephemeral LLM client was already consumed")
    }
}

impl ServiceManagerConfig {
    /// Explicit ephemeral/test inputs. Product callers never select this implicitly.
    pub fn ephemeral(
        channel_id: impl Into<String>,
        mut process: AgentProcessConfig,
        llm_client: LlmClient,
        tools: ToolRegistry,
    ) -> Self {
        if process
            .launch_context
            .namespace
            .resolve("/lib/agents/root")
            .is_err()
        {
            process.launch_context.namespace.mount(
                "/lib/agents/root",
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                Access::ReadOnly,
            );
        }
        if process.launch_context.namespace.resolve("/memory").is_err() {
            process.launch_context.namespace.mount(
                "/memory",
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                Access::ReadWrite,
            );
        }
        Self {
            channel_id: channel_id.into(),
            connection_store: None,
            package_store: None,
            process,
            llm_factory: Arc::new(OneShotLlmClientFactory(std::sync::Mutex::new(Some(
                llm_client,
            )))),
            host_mount_adapter: Arc::new(UnavailableHostMountExportAdapter),
            tools,
        }
    }

    pub fn with_factory(
        channel_id: impl Into<String>,
        process: AgentProcessConfig,
        connection_store: Option<ConnectionStoreBindings>,
        package_store: Option<std::path::PathBuf>,
        llm_factory: Arc<dyn LlmClientFactory>,
        host_mount_adapter: Arc<dyn HostMountExportAdapter>,
        tools: ToolRegistry,
    ) -> Self {
        Self {
            channel_id: channel_id.into(),
            process,
            connection_store,
            package_store,
            llm_factory,
            host_mount_adapter,
            tools,
        }
    }
}

/// One running Service-Manager-owned Alan OS instance.
pub struct ServiceManager {
    boot_id: Uuid,
    state: Arc<tokio::sync::Mutex<ManagerState>>,
    procfs: alan_kernel::ProcFs,
    manager_pid: Pid,
    root_pid: Arc<AtomicU64>,
    local_entry: Arc<LocalEntryService>,
    host_mount: Arc<HostMountService>,
    connection: Arc<ConnectionService>,
    package: Arc<PackageService>,
    package_handle: Arc<SwitchableFileServer>,
    supervisor_shutdown: tokio::sync::oneshot::Sender<()>,
    supervisor_task: tokio::task::JoinHandle<Result<()>>,
}

impl ServiceManager {
    pub async fn boot(mut config: ServiceManagerConfig) -> Result<Self> {
        ensure!(
            matches!(config.channel_id.as_str(), "stable" | "dev" | "test"),
            "invalid Alan OS Host channel `{}`",
            config.channel_id
        );
        ensure!(
            config.process.launch_context.package_references.is_empty(),
            "initial package references must be resolved by Package Service"
        );
        ensure!(
            config
                .process
                .launch_context
                .host_mounts
                .iter()
                .all(|grant| !overlaps_package_namespace(&grant.namespace_path)),
            "Host Mount grants overlapping /lib/pkg are not accepted"
        );
        ensure!(
            config
                .process
                .launch_context
                .namespace
                .describe()
                .iter()
                .all(|(path, _)| !overlaps_package_namespace(path)),
            "namespace mounts overlapping /lib/pkg are not accepted"
        );
        configure_runtime_tool_execution_binding(&config.process, &mut config.tools)?;

        let boot_id = Uuid::new_v4();
        let manifest = BootManifest::system().context("load system /lib/boot units")?;
        let package_service = match config.package_store.take() {
            Some(store) => PackageService::open(&config.channel_id, store)?,
            None => PackageService::ephemeral(&config.channel_id)?,
        };
        seed_preinstalled_packages(&package_service)?;
        let resolved_definition = alan_agent_engine::ResolvedAgentDefinition::from_launch_context(
            &config.process.launch_context,
            &config
                .process
                .agent_config
                .core_config
                .resolved_skill_overrides(),
            config.process.core_config_source,
        )?;
        config.process.agent_config =
            resolved_definition.apply_to_agent_config(&config.process.agent_config)?;
        let preferred_connection = config
            .process
            .agent_config
            .core_config
            .connection_profile
            .clone();
        let connection_service = match config.connection_store.as_ref() {
            Some(bindings) => ConnectionService::open(&config.channel_id, bindings)?,
            None => ConnectionService::ephemeral(&config.channel_id),
        };
        let llm_connection = preferred_connection
            .or_else(|| connection_service.default_profile())
            .unwrap_or_else(|| LLM_CONNECTION.to_string());
        let connections = connection_service.metadata();
        let connection_base_config = config.process.agent_config.core_config.clone();
        let selected_profile = connection_service
            .has_profile(&llm_connection)
            .then_some(llm_connection.as_str());
        if let Some(profile) = selected_profile {
            connections.apply_profile_metadata_to_config(
                Some(profile),
                &mut config.process.agent_config.core_config,
            )?;
        }
        config
            .tools
            .set_config(Arc::new(config.process.agent_config.core_config.clone()));
        let bootstrap = match config.llm_factory.create(
            &connection_base_config,
            selected_profile,
            &connections,
        ) {
            Ok(client) => Some((llm_connection.clone(), client)),
            Err(_) => {
                tracing::warn!(
                    profile_id = %llm_connection,
                    "LLM connection is unavailable during boot; Alan OS will continue without a callable provider"
                );
                None
            }
        };
        let generation_capabilities = bootstrap
            .as_ref()
            .map(|(_, client)| client.capabilities())
            .unwrap_or_else(|| {
                provider_capabilities_for_config(&config.process.agent_config.core_config)
            });
        let host_capabilities = alan_agent_engine::skills::build_skill_host_capabilities(
            config.tools.list_tools().into_iter().map(str::to_string),
            true,
        );
        let mut assembled = assemble_environment(AssembleInputs {
            boot_id,
            manifest,
            connection_service,
            package_service,
            llm_connection,
            bootstrap,
            llm_factory: config.llm_factory.clone(),
            connection_base_config,
            host_mount_adapter: config.host_mount_adapter.clone(),
            process: config.process,
            tools: config.tools,
            host_capabilities,
            generation_capabilities,
        })
        .await?;
        let initial_ready = assembled.root.wait_until_ready().await;
        let root_pid = Arc::new(AtomicU64::new(assembled.root.pid().0));
        let state = assembled.state.clone();
        let procfs = assembled.procfs.clone();
        let manager_pid = assembled.manager_pid;
        let local_entry = assembled.local_entry.clone();
        let host_mount = assembled.host_mount.clone();
        let connection = assembled.connection.clone();
        let package = assembled.package.clone();
        let package_handle = assembled.package_handle.clone();
        let root = assembled.root.namespace();
        let mut runtime = SupervisorRuntime {
            manifest: assembled.manifest,
            state: state.clone(),
            procfs: procfs.clone(),
            srvfs: assembled.srvfs,
            system_namespace: assembled.system_namespace,
            manager_pid,
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
            local_entry: local_entry.clone(),
            root: Some(assembled.root),
            root_pid: root_pid.clone(),
            root_template: assembled.root_template,
        };
        match initial_ready {
            Ok(_) => state
                .lock()
                .await
                .mark_ready("root-agent")
                .map_err(|error| anyhow::anyhow!("mark Root Agent ready: {error:?}"))?,
            Err(error) => {
                let active = runtime
                    .active
                    .get("root-agent")
                    .copied()
                    .context("Root Agent launch was not tracked")?;
                runtime.procfs.record_exit(active.pid, 1).await;
                runtime.handle_exit("root-agent", active, 1).await?;
                state
                    .lock()
                    .await
                    .note_error("root-agent", error.to_string())
                    .map_err(|code| anyhow::anyhow!("record Root Agent boot error: {code:?}"))?;
                loop {
                    let Some(deadline) = runtime.pending.remove("root-agent") else {
                        anyhow::bail!(
                            "Root Agent exhausted boot restart budget: {}",
                            state
                                .lock()
                                .await
                                .unit("root-agent")
                                .and_then(|unit| unit.error)
                                .unwrap_or_else(|| error.to_string())
                        );
                    };
                    tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
                    match runtime.launch("root-agent").await {
                        Ok(()) => break,
                        Err(error) => runtime.handle_launch_failure("root-agent", error).await?,
                    }
                }
            }
        }
        verify_readiness(&root, boot_id, &state).await?;

        let (supervisor_shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
        let supervisor_task = tokio::spawn(run_supervisor(runtime, shutdown_rx));

        Ok(Self {
            boot_id,
            state,
            procfs,
            manager_pid,
            root_pid,
            local_entry,
            host_mount,
            connection,
            package,
            package_handle,
            supervisor_shutdown,
            supervisor_task,
        })
    }

    pub fn boot_id(&self) -> Uuid {
        self.boot_id
    }

    /// The authorized local-entry service used to create one Shell Process per renderer.
    pub fn local_entry(&self) -> Arc<LocalEntryService> {
        self.local_entry.clone()
    }

    /// Native adapter entry to the Host Mount authority.
    pub fn host_mount(&self) -> Arc<HostMountService> {
        self.host_mount.clone()
    }

    /// Native adapter entry to the Connection authority.
    pub fn connection(&self) -> Arc<ConnectionService> {
        self.connection.clone()
    }

    /// Resolve and retain one installed package revision in a future Process launch context.
    pub async fn reference_package(
        &self,
        launch_context: &mut ProcessLaunchContext,
        package_id: &str,
    ) -> Result<()> {
        self.package_handle
            .while_bound(|| project_package_reference(&self.package, launch_context, package_id))
            .await
    }

    pub fn manager_pid(&self) -> Pid {
        self.manager_pid
    }

    pub fn root_pid(&self) -> Pid {
        Pid(self.root_pid.load(Ordering::Acquire))
    }

    pub fn state(&self) -> Arc<tokio::sync::Mutex<ManagerState>> {
        self.state.clone()
    }

    /// Record a unit exit through `/proc`; the Service Manager loop owns the response.
    pub async fn terminate_unit(&self, name: &str, exit_code: i32) -> Result<()> {
        let pid = self
            .state
            .lock()
            .await
            .unit(name)
            .and_then(|unit| unit.pid)
            .with_context(|| format!("unit `{name}` is not running"))?;
        self.procfs.record_exit(Pid(pid), exit_code).await;
        Ok(())
    }

    pub async fn shutdown(self) -> Result<()> {
        self.state.lock().await.mark_stopping();
        let _ = self.supervisor_shutdown.send(());
        self.supervisor_task
            .await
            .context("Service Manager supervisor task failed")??;
        self.procfs.record_exit(self.manager_pid, 0).await;
        Ok(())
    }
}

struct AssembledEnvironment {
    root: RootAgentProcess,
    root_template: RootAgentTemplate,
    manifest: BootManifest,
    state: Arc<tokio::sync::Mutex<ManagerState>>,
    procfs: alan_kernel::ProcFs,
    srvfs: Arc<alan_kernel::SrvFs>,
    system_namespace: LiveNamespace,
    agent_runtime: Arc<AgentRuntimeService>,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
    routefs: Arc<alan_routefs::RouteFs>,
    host_mount: Arc<HostMountService>,
    connection: Arc<ConnectionService>,
    package: Arc<PackageService>,
    package_handle: Arc<SwitchableFileServer>,
    active_units: BTreeMap<String, ActiveUnit>,
    manager_pid: Pid,
    local_entry: Arc<LocalEntryService>,
}

struct AssembleInputs {
    boot_id: Uuid,
    manifest: BootManifest,
    connection_service: Arc<ConnectionService>,
    package_service: Arc<PackageService>,
    llm_connection: String,
    bootstrap: Option<(String, LlmClient)>,
    llm_factory: Arc<dyn LlmClientFactory>,
    connection_base_config: alan_agent_engine::Config,
    host_mount_adapter: Arc<dyn HostMountExportAdapter>,
    process: AgentProcessConfig,
    tools: ToolRegistry,
    host_capabilities: alan_agent_engine::skills::SkillHostCapabilities,
    generation_capabilities: alan_llm::ProviderCapabilities,
}

#[derive(Clone, Copy)]
struct ActiveUnit {
    pid: Pid,
    started_at: Instant,
}

struct SupervisorRuntime {
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

async fn assemble_environment(inputs: AssembleInputs) -> Result<AssembledEnvironment> {
    let AssembleInputs {
        boot_id,
        manifest,
        connection_service,
        package_service,
        llm_connection,
        bootstrap,
        llm_factory,
        connection_base_config,
        host_mount_adapter,
        mut process,
        tools,
        host_capabilities,
        generation_capabilities,
    } = inputs;
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    connection_service
        .attach_callable_registry(
            llmfs.clone(),
            llm_factory,
            connection_base_config,
            bootstrap,
        )
        .await?;
    let routefs = Arc::new(alan_routefs::RouteFs::new());
    let srvfs = Arc::new(alan_kernel::SrvFs::new());
    ensure!(
        !tools.list_tools().contains(&"q"),
        "Tool name `q` is reserved for Quartermaster"
    );
    // System and Shell Processes are lifecycle records supervised by Alan OS;
    // they must not be fed to an executable runner merely because `/bin/q`
    // exists. Process-specific `/proc` views add the Quartermaster runner below.
    let procfs = alan_kernel::ProcFs::new();
    let agent_root = Arc::new(alan_agentfs::AgentRootFs::new(Arc::new(procfs.clone())));
    let state = Arc::new(tokio::sync::Mutex::new(ManagerState::new(manifest.clone())));
    let manager_fs = Arc::new(ServiceManagerFs::new(state.clone()));
    let host_mount_service = HostMountService::new(host_mount_adapter);
    let package_handle = SwitchableFileServer::new();
    let agent_runtime = AgentRuntimeService::new(
        procfs.clone(),
        AgentRuntimeFileServers::from_refs(&agent_root, &llmfs, &srvfs, &routefs),
        host_mount_service.clone(),
        connection_service.clone(),
        tools.process_runner(),
    );

    let mut namespace = process.launch_context.namespace.child();
    mount_standard_namespace_roots(&mut namespace);
    mount_boot_units(&mut namespace);
    mount_system_executables(&mut namespace, &manifest);
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    namespace.mount(
        "/srv",
        InProcessTransport::new(srvfs.clone()),
        Access::ReadWrite,
    );
    namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.for_spawner(
            None,
            namespace.clone(),
            Credentials::system(),
        ))),
        Access::ReadWrite,
    );
    mount_tool_packages(&mut namespace, &tools)?;
    namespace.mount(
        "/proc/host",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_files([
            ("boot_id".to_string(), format!("{boot_id}\n").into_bytes()),
            ("state".to_string(), b"ready\n".to_vec()),
        ]))),
        Access::ReadOnly,
    );

    let system_namespace = LiveNamespace::new(namespace);
    let mut local_entry = None;
    let mut active_units = BTreeMap::new();
    let manager_pid = spawn_process(
        &procfs,
        None,
        system_namespace.clone(),
        Credentials::system(),
        SERVICE_MANAGER_EXECUTABLE,
    )
    .await?;
    ensure!(
        manager_pid == Pid(1),
        "Service Manager must be the first Process"
    );
    procfs
        .bind_live_namespace(manager_pid, system_namespace.clone())
        .await;
    system_namespace.replace_mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.for_live_spawner(
            Some(manager_pid),
            system_namespace.clone(),
            Credentials::system(),
        ))),
        Access::ReadWrite,
    );
    srvfs
        .post(
            "service-manager",
            InProcessTransport::new(manager_fs),
            Access::ReadWrite,
        )
        .await;

    for unit in manifest.ordered().filter(|unit| unit.name != "root-agent") {
        loop {
            let attempt = async {
                if unit.name == "local-entry" {
                    mount_service_handles(&system_namespace, &srvfs).await?;
                }
                let (pid, unit_namespace) = spawn_unit_process(
                    &procfs,
                    manager_pid,
                    &system_namespace,
                    Credentials::system(),
                    unit,
                    &[],
                )
                .await?;
                state
                    .lock()
                    .await
                    .start_attempt(&unit.name, pid)
                    .map_err(|error| anyhow::anyhow!("track {} start: {error:?}", unit.name))?;
                if unit.name == "local-entry" {
                    let service = local_entry.get_or_insert_with(|| {
                        LocalEntryService::new(procfs.clone(), unit_namespace)
                    });
                    service
                        .set_service_pid(pid)
                        .await
                        .map_err(|error| anyhow::anyhow!("bind Local Entry Process: {error:?}"))?;
                }
                publish_unit_handles(
                    unit,
                    &SystemServiceHandles {
                        srvfs: &srvfs,
                        agent_root: &agent_root,
                        llmfs: &llmfs,
                        routefs: &routefs,
                        host_mount: &host_mount_service,
                        connection: &connection_service,
                        package: &package_service,
                        package_handle: &package_handle,
                        local_entry: local_entry.as_ref(),
                    },
                )
                .await?;
                wait_unit_ready(unit, pid, &procfs, &srvfs).await?;
                Ok::<_, anyhow::Error>(ActiveUnit {
                    pid,
                    started_at: Instant::now(),
                })
            }
            .await;

            match attempt {
                Ok(active) => {
                    state
                        .lock()
                        .await
                        .mark_ready(&unit.name)
                        .map_err(|error| anyhow::anyhow!("mark {} ready: {error:?}", unit.name))?;
                    active_units.insert(unit.name.clone(), active);
                    break;
                }
                Err(error) => {
                    let pid = state
                        .lock()
                        .await
                        .unit(&unit.name)
                        .and_then(|unit| unit.pid);
                    if let Some(pid) = pid {
                        procfs.record_exit(Pid(pid), 1).await;
                    } else {
                        state
                            .lock()
                            .await
                            .start_failure(&unit.name, error.to_string())
                            .map_err(|code| {
                                anyhow::anyhow!("track {} failure: {code:?}", unit.name)
                            })?;
                    }
                    for handle in &unit.published_handles {
                        srvfs.unpost(handle).await;
                    }
                    let mut manager = state.lock().await;
                    let decision = manager.record_exit(&unit.name, 1, 0).map_err(|code| {
                        anyhow::anyhow!("record {} failure: {code:?}", unit.name)
                    })?;
                    manager
                        .note_error(&unit.name, error.to_string())
                        .map_err(|code| anyhow::anyhow!("record {} error: {code:?}", unit.name))?;
                    drop(manager);
                    match decision {
                        RestartDecision::RestartAfterMs(delay) => {
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                        }
                        RestartDecision::FailBoot => {
                            return Err(error).context(format!(
                                "required unit `{}` exhausted boot restart budget",
                                unit.name
                            ));
                        }
                        RestartDecision::Stop | RestartDecision::Degrade => return Err(error),
                    }
                }
            }
        }
    }

    mount_service_handles(&system_namespace, &srvfs).await?;
    let local_entry = local_entry.context("Local Entry Boot Unit did not start")?;
    let root_unit = manifest
        .get("root-agent")
        .context("Root Agent Boot Unit is missing")?;
    let mut root_template_context = process.launch_context.rebound(
        system_namespace.snapshot(),
        process.launch_context.credentials.clone(),
    );
    for source in alan_agent_engine::skills::preinstalled_skill_package_sources() {
        project_package_reference(
            &package_service,
            &mut root_template_context,
            &source.package_id,
        )?;
    }
    process.launch_context = root_template_context;
    let root_template = RootAgentTemplate::new(
        process,
        host_capabilities,
        generation_capabilities,
        llm_connection,
    );
    let root = agent_runtime
        .launch_root(manager_pid, &system_namespace, root_unit, &root_template)
        .await?;
    let root_pid = root.pid();
    if let Err(error) = state.lock().await.start_attempt("root-agent", root_pid) {
        agent_runtime.detach_root(root, 1).await;
        return Err(anyhow::anyhow!("track Root Agent start: {error:?}"));
    }
    active_units.insert(
        "root-agent".to_string(),
        ActiveUnit {
            pid: root_pid,
            started_at: Instant::now(),
        },
    );

    Ok(AssembledEnvironment {
        root,
        root_template,
        manifest,
        state,
        procfs,
        srvfs,
        system_namespace,
        agent_runtime,
        agent_root,
        llmfs,
        routefs,
        host_mount: host_mount_service,
        connection: connection_service,
        package: package_service,
        package_handle,
        active_units,
        manager_pid,
        local_entry,
    })
}

fn mount_standard_namespace_roots(namespace: &mut Namespace) {
    for path in ["/bin", "/lib", "/man", "/mnt"] {
        namespace.mount(
            path,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
    }
}

fn mount_boot_units(namespace: &mut Namespace) {
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

fn mount_system_executables(namespace: &mut Namespace, manifest: &BootManifest) {
    for executable in manifest
        .ordered()
        .map(|unit| unit.executable.as_str())
        .chain([
            SERVICE_MANAGER_EXECUTABLE,
            "/bin/alan-shell",
            QUARTERMASTER_EXECUTABLE,
        ])
    {
        namespace.mount(
            executable,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
    }
}

struct SystemServiceHandles<'a> {
    srvfs: &'a Arc<alan_kernel::SrvFs>,
    agent_root: &'a Arc<alan_agentfs::AgentRootFs>,
    llmfs: &'a Arc<alan_llmfs::LlmFs>,
    routefs: &'a Arc<alan_routefs::RouteFs>,
    host_mount: &'a Arc<HostMountService>,
    connection: &'a Arc<ConnectionService>,
    package: &'a Arc<PackageService>,
    package_handle: &'a Arc<SwitchableFileServer>,
    local_entry: Option<&'a Arc<LocalEntryService>>,
}

async fn publish_unit_handles(unit: &BootUnit, services: &SystemServiceHandles<'_>) -> Result<()> {
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

async fn mount_service_handles(
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

fn mount_tool_packages(namespace: &mut Namespace, tools: &ToolRegistry) -> Result<()> {
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

fn seed_preinstalled_packages(package_service: &Arc<PackageService>) -> Result<()> {
    for source in alan_agent_engine::skills::preinstalled_skill_package_sources() {
        let snapshot =
            crate::PackageSnapshot::from_directory(&source.root_dir).with_context(|| {
                format!(
                    "snapshot first-party package `{}` for Package Service",
                    source.package_id
                )
            })?;
        package_service.seed_preinstalled(&source.package_id, snapshot)?;
    }
    Ok(())
}

pub(crate) fn namespace_with_package_references(
    mut base: Namespace,
    launch_context: &ProcessLaunchContext,
) -> Result<LiveNamespace> {
    for reference in &launch_context.package_references {
        base.mount(
            &reference.namespace_path,
            reference.handle(),
            Access::ReadOnly,
        );
    }
    Ok(LiveNamespace::new(base))
}

fn overlaps_package_namespace(path: &str) -> bool {
    namespace_paths_overlap(path, "/lib/pkg")
}

fn namespace_paths_overlap(left: &str, right: &str) -> bool {
    left == "/"
        || right == "/"
        || left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn project_package_reference(
    package_service: &Arc<PackageService>,
    launch_context: &mut ProcessLaunchContext,
    package_id: &str,
) -> Result<()> {
    let lease = package_service.acquire(package_id)?;
    let record = lease.record().clone();
    let namespace_root = format!("/lib/pkg/{package_id}");
    let handle = InProcessTransport::new(lease.file_server()?);
    launch_context
        .namespace
        .mount(&namespace_root, handle.clone(), Access::ReadOnly);
    let kind = match record.kind {
        crate::PackageKind::Preinstalled => ProcessPackageKind::Preinstalled,
        crate::PackageKind::Installed => ProcessPackageKind::Installed,
    };
    let skills = record
        .exports
        .iter()
        .map(|export| {
            ProcessPackageSkillReference::new(
                &export.skill_id,
                &export.root,
                export.dependencies.clone(),
                lease.skill_descriptor(&export.root)?,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    launch_context.add_package_reference(ProcessPackageReference::new(
        package_id,
        &record.revision,
        kind,
        &namespace_root,
        skills,
        handle,
    )?);
    launch_context.retain_authority(lease);
    Ok(())
}

pub(crate) async fn spawn_process(
    procfs: &alan_kernel::ProcFs,
    parent: Option<Pid>,
    namespace: LiveNamespace,
    credentials: Credentials,
    executable: &str,
) -> Result<Pid> {
    spawn_process_with_descriptors(
        procfs,
        parent,
        namespace,
        credentials,
        executable,
        BTreeMap::new(),
    )
    .await
}

pub(crate) async fn spawn_unit_process(
    procfs: &alan_kernel::ProcFs,
    parent: Pid,
    system_namespace: &LiveNamespace,
    credentials: Credentials,
    unit: &BootUnit,
    extra_mounts: &[(String, Access)],
) -> Result<(Pid, LiveNamespace)> {
    let base = system_namespace.snapshot();
    let declarations = unit
        .mounts
        .iter()
        .map(|mount| {
            (
                mount.path.as_str(),
                mount.source.as_str(),
                match mount.access {
                    crate::MountAccess::Read => Access::ReadOnly,
                    crate::MountAccess::Write => Access::ReadWrite,
                },
            )
        })
        .chain(
            extra_mounts
                .iter()
                .map(|(path, access)| (path.as_str(), path.as_str(), *access)),
        );
    let namespace = base.project_mounts(declarations).map_err(|_| {
        anyhow::anyhow!(
            "Boot Unit `{}` requests an unavailable mount projection",
            unit.name
        )
    })?;
    for descriptor in &unit.descriptors {
        ensure!(
            namespace.resolve(&descriptor.path).is_ok(),
            "Boot Unit `{}` descriptor {} is outside its namespace",
            unit.name,
            descriptor.number
        );
    }
    let descriptors = unit
        .descriptors
        .iter()
        .map(|descriptor| (descriptor.number, descriptor.path.clone()))
        .collect();
    let live_namespace = LiveNamespace::new(namespace);
    let pid = spawn_process_with_descriptors(
        procfs,
        Some(parent),
        live_namespace.clone(),
        credentials,
        &unit.executable,
        descriptors,
    )
    .await?;
    Ok((pid, live_namespace))
}

async fn spawn_process_with_descriptors(
    procfs: &alan_kernel::ProcFs,
    parent: Option<Pid>,
    namespace: LiveNamespace,
    credentials: Credentials,
    executable: &str,
    descriptors: BTreeMap<u32, String>,
) -> Result<Pid> {
    let spawner = procfs.for_live_spawner(parent, namespace.clone(), credentials);
    let fid = Fid(NEXT_BOOT_FID.fetch_add(1, Ordering::Relaxed));
    spawner
        .walk(Fid::ROOT, fid, &["clone".to_string()])
        .await
        .with_context(|| format!("walk {executable} /proc/clone"))?;
    spawner
        .open(fid, OpenMode::ReadWrite)
        .await
        .with_context(|| format!("open {executable} /proc/clone"))?;
    let pid = String::from_utf8(spawner.read(fid, 0, 64).await?)
        .with_context(|| format!("{executable} PID is not UTF-8"))?
        .parse::<u64>()
        .with_context(|| format!("{executable} PID is invalid"))?;
    let exec = alan_kernel::ExecSpec {
        executable: executable.to_string(),
        args: Vec::new(),
        namespace: Some(alan_kernel::ExecNamespaceManifest::from_namespace(
            &namespace.snapshot(),
        )),
        descriptors,
    };
    spawner
        .write(fid, 0, &serde_json::to_vec(&exec)?)
        .await
        .with_context(|| format!("write {executable} exec spec"))?;
    spawner
        .clunk(fid)
        .await
        .with_context(|| format!("commit {executable} Process"))?;
    Ok(Pid(pid))
}

async fn wait_unit_ready(
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

async fn verify_readiness(
    root: &InProcessTransport,
    boot_id: Uuid,
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
    let published_boot_id = String::from_utf8(shell.cat(BOOT_ID_PATH).await?)?;
    ensure!(
        published_boot_id.trim() == boot_id.to_string(),
        "boot ID mismatch"
    );
    ensure!(
        shell.cat(BOOT_STATE_PATH).await? == b"ready\n",
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

#[cfg(test)]
mod tests;
