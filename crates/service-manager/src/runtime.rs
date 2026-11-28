//! Service Manager boot and runtime ownership.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use alan_agent_engine::{
    AgentProcessConfig, LlmClient, ToolRegistry, provider_capabilities_for_config,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, Credentials, LiveNamespace, Pid};
use anyhow::{Context, Result, ensure};
use uuid::Uuid;

pub(crate) mod embed_mounts;
mod packages;
pub(crate) mod service_activation;
pub(crate) mod sessions;
mod supervisor;
mod trust;

pub use packages::prepare_fileserver_launch_grant;
pub use sessions::{SessionHandle, SessionLaunchOptions};

#[cfg(test)]
use packages::project_package_reference_async;
#[cfg(test)]
pub(crate) use packages::seed_preinstalled_packages;
#[cfg(not(test))]
use packages::seed_preinstalled_packages;
pub(crate) use packages::{namespace_with_package_references, validate_package_reference_mounts};
use packages::{overlaps_package_namespace, project_package_reference};

use supervisor::{
    ActiveUnit, SERVICE_MANAGER_SRV_HANDLE, SupervisorEnvironment, SupervisorRuntime,
    SystemServiceHandles, create_supervised_handles, mount_boot_units, mount_service_handles,
    mount_standard_namespace_roots, mount_system_executables, mount_tool_packages,
    publish_unit_handles, verify_readiness, wait_unit_ready,
};

use crate::{
    BootManifest, ConnectionService, ConnectionStoreBindings, ConnectionsFile,
    HostMountExportAdapter, HostMountService, LocalEntryService, ManagerState, PackageService,
    ProcessLaunchContext, RestartDecision, ServiceManagerFs, SupervisedHandle,
    UnavailableHostMountExportAdapter,
    agent_runtime::{AgentRuntimeService, RootAgentTemplate},
    process_spawn::{spawn_process, spawn_unit_process},
};

pub const BOOT_ID_PATH: &str = "/proc/host/boot_id";
pub const BOOT_STATE_PATH: &str = "/proc/host/state";

const LLM_CONNECTION: &str = "default";
const SERVICE_MANAGER_EXECUTABLE: &str = "/bin/service-manager";

/// Explicit Root-only advice evaluator selection, separate from generation.
#[derive(Debug, Clone)]
pub struct InputShadowSelection {
    pub profile: String,
    pub surface: alan_agent_engine::runtime::EvaluationSurface,
}

/// Explicit inputs supplied by the platform Host to Service Manager.
pub struct ServiceManagerConfig {
    pub process: AgentProcessConfig,
    /// Restore the previously selected Root Agent rollout for this invocation.
    pub resume_root: bool,
    /// Optional explicit Root Agent rollout id when resuming (`--resume <id>`).
    pub resume_rollout_id: Option<String>,
    /// Never read or overwrite the `metadata/root-rollout` selector, so a throwaway run neither
    /// resumes nor becomes the selected session of earlier or later invocations.
    pub ephemeral_rollout: bool,
    /// Skip writing `metadata/root-rollout` while keeping normal memory, context, and `--resume`.
    pub no_resume_update: bool,
    pub input_shadow: Option<InputShadowSelection>,
    pub launch_context: ProcessLaunchContext,
    pub connection_store: Option<ConnectionStoreBindings>,
    pub package_store: Option<std::path::PathBuf>,
    pub in_memory_stores: bool,
    pub llm_factory: Arc<dyn LlmClientFactory>,
    pub host_mount_adapter: Arc<dyn HostMountExportAdapter>,
    pub tools: ToolRegistry,
    /// Whether to project Alan's built-in skills onto Agent Processes.
    pub include_builtin_skills: bool,
    /// Durable Root package bindings loaded from System Store (`SkillsOnly` or `Active`).
    pub root_package_bindings: Vec<crate::RootPackageBinding>,
    /// Explicit per-invocation Root bindings (`--package` / `--fileserver`, fail-closed).
    pub explicit_root_package_bindings: Vec<crate::RootPackageBinding>,
    /// Optional Host-owned secret resolver hook for confined packaged Tool execution.
    pub tool_secret_resolver: Option<Arc<dyn alan_agent_engine::tools::ToolSecretResolver>>,
    /// Optional Host-owned CoW/worktree isolation port for M8 Tier-3 workstreams.
    pub workspace_isolation_port: Option<Arc<dyn alan_agent_engine::WorkspaceIsolationPort>>,
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
struct ReusableLlmClientFactory(alan_agent_engine::ReusableLlmClient);

impl LlmClientFactory for ReusableLlmClientFactory {
    fn create(
        &self,
        _base_config: &alan_agent_engine::Config,
        _selected_profile: Option<&str>,
        _connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        Ok(self.0.client())
    }
}

impl ServiceManagerConfig {
    /// Explicit ephemeral/test inputs. Product callers never select this implicitly.
    pub fn ephemeral(
        process: AgentProcessConfig,
        mut launch_context: ProcessLaunchContext,
        llm_client: LlmClient,
        tools: ToolRegistry,
    ) -> Self {
        if launch_context
            .namespace
            .resolve("/lib/agents/root")
            .is_err()
        {
            launch_context.namespace.mount(
                "/lib/agents/root",
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                Access::ReadOnly,
            );
        }
        if launch_context.namespace.resolve("/memory").is_err() {
            launch_context.namespace.mount(
                "/memory",
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                Access::ReadWrite,
            );
        }
        Self {
            launch_context,
            connection_store: None,
            package_store: None,
            in_memory_stores: false,
            process,
            resume_root: false,
            resume_rollout_id: None,
            ephemeral_rollout: false,
            no_resume_update: false,
            input_shadow: None,
            llm_factory: Arc::new(ReusableLlmClientFactory(llm_client.into_reusable())),
            host_mount_adapter: Arc::new(UnavailableHostMountExportAdapter),
            tools,
            include_builtin_skills: true,
            root_package_bindings: Vec::new(),
            explicit_root_package_bindings: Vec::new(),
            tool_secret_resolver: None,
            workspace_isolation_port: None,
        }
    }

    /// Attach a Host-owned secret resolver hook for confined packaged Tool execution.
    pub fn with_tool_secret_resolver(
        mut self,
        resolver: Arc<dyn alan_agent_engine::tools::ToolSecretResolver>,
    ) -> Self {
        self.tool_secret_resolver = Some(resolver);
        self
    }

    /// Control whether Alan's preinstalled built-in skills are projected onto Agent Processes.
    pub fn with_include_builtin_skills(mut self, include: bool) -> Self {
        self.include_builtin_skills = include;
        self
    }

    /// Override durable Root package bindings projected onto Agent Processes.
    pub fn with_root_package_bindings(mut self, bindings: Vec<crate::RootPackageBinding>) -> Self {
        self.root_package_bindings = bindings;
        self
    }

    /// Add fail-closed per-invocation Root package bindings (`--package` / `--fileserver`).
    pub fn with_explicit_root_package_bindings(
        mut self,
        bindings: Vec<crate::RootPackageBinding>,
    ) -> Self {
        self.explicit_root_package_bindings = bindings;
        self
    }

    /// Keep this invocation's Root rollout out of the selected-rollout channel.
    pub fn with_ephemeral_rollout(mut self, ephemeral: bool) -> Self {
        self.ephemeral_rollout = ephemeral;
        self
    }

    /// Skip updating the global `metadata/root-rollout` pointer for this invocation.
    pub fn with_no_resume_update(mut self, no_resume_update: bool) -> Self {
        self.no_resume_update = no_resume_update;
        self
    }

    /// Set explicit invocation-only trust posture on the Root Agent Process config.
    pub fn with_trust_posture(mut self, trust_posture: alan_agent_engine::TrustPosture) -> Self {
        self.process.trust_posture = trust_posture;
        self
    }

    /// Select an explicit Root rollout id for `--resume <id>`.
    pub fn with_resume_rollout_id(mut self, rollout_id: Option<String>) -> Self {
        if rollout_id.is_some() {
            self.resume_root = true;
        }
        self.resume_rollout_id = rollout_id;
        self
    }
}

/// One running Service-Manager-owned Alan OS instance.
pub struct ServiceManager {
    boot_id: Uuid,
    root_model: Option<String>,
    state: Arc<tokio::sync::Mutex<ManagerState>>,
    procfs: alan_kernel::ProcFs,
    manager_pid: Pid,
    root_pid: Arc<AtomicU64>,
    local_entry: Arc<LocalEntryService>,
    host_mount: Arc<HostMountService>,
    connection: Arc<ConnectionService>,
    package: Arc<PackageService>,
    supervised_handles: BTreeMap<String, Arc<SupervisedHandle>>,
    fileserver_manager: Arc<crate::FileserverManager>,
    tool_secret_resolver: Option<Arc<dyn alan_agent_engine::tools::ToolSecretResolver>>,
    #[cfg(test)]
    root_namespace: InProcessTransport,
    supervisor_cmd: tokio::sync::mpsc::UnboundedSender<supervisor::SupervisorCommand>,
    supervisor_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    supervisor_task: Option<tokio::task::JoinHandle<Result<()>>>,
}

impl ServiceManager {
    pub async fn boot(mut config: ServiceManagerConfig) -> Result<Self> {
        trust::validate_boot_trust_and_resume(&config)?;
        if let Some(resolver) = config.tool_secret_resolver.clone() {
            config.launch_context.set_tool_secret_resolver(resolver);
        }
        let tool_secret_resolver = config.tool_secret_resolver.clone();
        ensure!(
            config.launch_context.package_references.is_empty(),
            "initial package references must be resolved by Package Service"
        );
        ensure!(
            config
                .launch_context
                .namespace
                .describe()
                .iter()
                .all(|(path, _)| !overlaps_package_namespace(path)),
            "namespace mounts overlapping /lib/pkg are not accepted"
        );
        let _ = embed_mounts::caller_mnt_mounts(&config.launch_context.namespace)?;
        let boot_id = Uuid::new_v4();
        let include_builtin_skills = config.include_builtin_skills;
        let manifest = BootManifest::system().context("load system /lib/boot units")?;
        let (package_bootstrap, package_service) =
            if config.in_memory_stores && config.package_store.is_none() {
                let service = PackageService::in_memory();
                seed_preinstalled_packages(&service)?;
                (None, service)
            } else {
                let package_bootstrap = crate::package::PackageBootstrap::new();
                let open = package_bootstrap.opener(config.package_store.take());
                let service = tokio::task::spawn_blocking(move || {
                    let service = open()?;
                    seed_preinstalled_packages(&service)?;
                    Ok::<_, anyhow::Error>(service)
                })
                .await
                .context("join Package bootstrap")??;
                (Some(package_bootstrap), service)
            };
        validate_package_reference_mounts(&config.launch_context)?;
        let resolved_definition = alan_agent_engine::ResolvedAgentDefinition::from_process_inputs(
            config
                .launch_context
                .descriptor(alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR),
            &config.launch_context.package_references,
            &config
                .process
                .agent_config
                .core_config
                .resolved_skill_overrides(),
            config.process.core_config_source,
        )?;
        config.process.agent_config =
            resolved_definition.apply_to_agent_config(&config.process.agent_config)?;
        config.process.agent_definition = resolved_definition;
        config.process.namespace_cwd = std::path::PathBuf::from(&config.launch_context.cwd);
        config.process.memory_store_bound = config
            .launch_context
            .descriptor(alan_agent_engine::MEMORY_STORE_DESCRIPTOR)
            .is_some();
        let preferred_connection = config
            .process
            .agent_config
            .core_config
            .connection_profile
            .clone();
        let allow_profile_switch = preferred_connection.is_none();
        let connection_service = match config.connection_store.as_ref() {
            Some(bindings) => ConnectionService::open(bindings)?,
            None if config.in_memory_stores => ConnectionService::in_memory(),
            None => ConnectionService::ephemeral()?,
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
            Ok(client)
                if selected_profile.is_some() || client.provider_name() != "unconfigured" =>
            {
                Some((llm_connection.clone(), client))
            }
            Ok(_) => {
                tracing::info!(
                    profile_id = %llm_connection,
                    "No connection profile configured during boot; continuing without a callable provider"
                );
                None
            }
            Err(_) => {
                tracing::warn!(
                    profile_id = %llm_connection,
                    "LLM connection is unavailable during boot; Alan OS will continue without a callable provider"
                );
                None
            }
        };
        let root_model = bootstrap.as_ref().map(|_| {
            config
                .process
                .agent_config
                .core_config
                .effective_model()
                .to_string()
        });
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
        let assembled = assemble_environment(AssembleInputs {
            boot_id,
            manifest,
            connection_service,
            package_service,
            llm_connection,
            allow_profile_switch,
            bootstrap,
            llm_factory: config.llm_factory.clone(),
            connection_base_config,
            host_mount_adapter: config.host_mount_adapter.clone(),
            process: config.process,
            resume_root: config.resume_root,
            resume_rollout_id: config.resume_rollout_id,
            ephemeral_rollout: config.ephemeral_rollout,
            no_resume_update: config.no_resume_update,
            input_shadow: config.input_shadow,
            launch_context: config.launch_context,
            tools: config.tools,
            include_builtin_skills,
            root_package_bindings: config.root_package_bindings,
            explicit_root_package_bindings: config.explicit_root_package_bindings,
            host_capabilities,
            generation_capabilities,
            workspace_isolation_port: config.workspace_isolation_port,
        })
        .await?;
        let root_pid = Arc::new(AtomicU64::new(assembled.root.pid().0));
        let state = assembled.state.clone();
        let procfs = assembled.procfs.clone();
        let manager_pid = assembled.manager_pid;
        let local_entry = assembled.local_entry.clone();
        let host_mount = assembled.host_mount.clone();
        let connection = assembled.connection.clone();
        let package = assembled.package.clone();
        let supervised_handles = assembled.supervised_handles.clone();
        let fileserver_manager = assembled.fileserver_manager.clone();
        let root = assembled.root.namespace();
        let mut runtime = SupervisorRuntime::from_assembled(assembled, root_pid.clone());
        if let Err(error) = runtime.settle_initial_root().await {
            let _ = fileserver_manager.stop_all().await;
            return Err(error);
        }
        if let Err(error) = verify_readiness(&root, boot_id, &state).await {
            let _ = fileserver_manager.stop_all().await;
            return Err(error);
        }

        if let Some(package_bootstrap) = package_bootstrap {
            package_bootstrap.finish();
        }
        let (supervisor_shutdown, supervisor_cmd, supervisor_task) = runtime.start();

        Ok(Self {
            boot_id,
            root_model,
            state,
            procfs,
            manager_pid,
            root_pid,
            local_entry,
            host_mount,
            connection,
            package,
            supervised_handles,
            fileserver_manager,
            tool_secret_resolver,
            #[cfg(test)]
            root_namespace: root,
            supervisor_cmd,
            supervisor_shutdown: Some(supervisor_shutdown),
            supervisor_task: Some(supervisor_task),
        })
    }

    pub fn boot_id(&self) -> Uuid {
        self.boot_id
    }

    /// The effective model selected for the Root Agent's callable boot binding, if available.
    pub fn root_model(&self) -> Option<&str> {
        self.root_model.as_deref()
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
        if launch_context.tool_secret_resolver.is_none()
            && let Some(resolver) = self.tool_secret_resolver.clone()
        {
            launch_context.set_tool_secret_resolver(resolver);
        }
        let package_handle = self
            .supervised_handles
            .get("package")
            .context("Package Service is unavailable")?;
        package_handle
            .while_bound_async("Package Service is unavailable", || {
                packages::project_package_reference_async(&self.package, launch_context, package_id)
            })
            .await
    }

    /// Supervised Quartermaster `fileserver` plugin instances.
    pub fn fileserver_manager(&self) -> Arc<crate::FileserverManager> {
        self.fileserver_manager.clone()
    }

    pub fn manager_pid(&self) -> Pid {
        self.manager_pid
    }

    pub fn root_pid(&self) -> Pid {
        Pid(self.root_pid.load(Ordering::Acquire))
    }

    pub fn root_pid_atomic(&self) -> Arc<AtomicU64> {
        self.root_pid.clone()
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

    /// Best-effort synchronous signal requesting background supervisor shutdown without blocking.
    pub fn signal_shutdown(&mut self) {
        if let Some(shutdown) = self.supervisor_shutdown.take() {
            let _ = shutdown.send(());
        }
    }

    pub async fn shutdown(mut self) -> Result<()> {
        self.state.lock().await.mark_stopping();
        self.signal_shutdown();
        let task_res = if let Some(task) = self.supervisor_task.take() {
            Some(task.await.context("Service Manager supervisor task failed"))
        } else {
            None
        };
        let _ = self.fileserver_manager.stop_all().await;
        if let Some(res) = task_res {
            res??;
        }
        self.procfs.record_exit(self.manager_pid, 0).await;
        Ok(())
    }
}

impl Drop for ServiceManager {
    fn drop(&mut self) {
        self.signal_shutdown();
    }
}

struct AssembleInputs {
    boot_id: Uuid,
    manifest: BootManifest,
    connection_service: Arc<ConnectionService>,
    package_service: Arc<PackageService>,
    llm_connection: String,
    allow_profile_switch: bool,
    bootstrap: Option<(String, LlmClient)>,
    llm_factory: Arc<dyn LlmClientFactory>,
    connection_base_config: alan_agent_engine::Config,
    host_mount_adapter: Arc<dyn HostMountExportAdapter>,
    process: AgentProcessConfig,
    resume_root: bool,
    resume_rollout_id: Option<String>,
    ephemeral_rollout: bool,
    no_resume_update: bool,
    input_shadow: Option<InputShadowSelection>,
    launch_context: ProcessLaunchContext,
    tools: ToolRegistry,
    include_builtin_skills: bool,
    root_package_bindings: Vec<crate::RootPackageBinding>,
    explicit_root_package_bindings: Vec<crate::RootPackageBinding>,
    host_capabilities: alan_agent_engine::skills::SkillHostCapabilities,
    generation_capabilities: alan_llm::ProviderCapabilities,
    workspace_isolation_port: Option<Arc<dyn alan_agent_engine::WorkspaceIsolationPort>>,
}

async fn assemble_environment(inputs: AssembleInputs) -> Result<SupervisorEnvironment> {
    let AssembleInputs {
        boot_id,
        manifest,
        connection_service,
        package_service,
        llm_connection,
        allow_profile_switch,
        bootstrap,
        llm_factory,
        connection_base_config,
        host_mount_adapter,
        mut process,
        resume_root,
        resume_rollout_id,
        ephemeral_rollout,
        no_resume_update,
        input_shadow,
        mut launch_context,
        tools,
        include_builtin_skills,
        root_package_bindings,
        explicit_root_package_bindings,
        host_capabilities,
        generation_capabilities,
        workspace_isolation_port,
    } = inputs;
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    connection_service.attach_package_service(package_service.clone());
    connection_service
        .attach_callable_registry(
            llmfs.clone(),
            llm_factory,
            connection_base_config,
            bootstrap,
        )
        .await?;
    let routefs = Arc::new(alan_routefs::RouteFs::new_with_workstream_handoff_rules());
    let srvfs = Arc::new(alan_kernel::SrvFs::new());
    ensure!(
        !tools.list_tools().contains(&"q"),
        "Tool name `q` is reserved for Quartermaster"
    );
    ensure!(
        !tools.list_tools().contains(&"agent_work"),
        "Tool name `agent_work` is reserved for Agent work commands"
    );
    // System and Shell Processes are lifecycle records supervised by Alan OS;
    // they must not be fed to an executable runner merely because `/bin/q`
    // exists. Process-specific `/proc` views add the Quartermaster runner below.
    let procfs = alan_kernel::ProcFs::new();
    let agent_root = Arc::new(alan_agentfs::AgentRootFs::new_with_process_events(
        Arc::new(procfs.clone()),
        Arc::new(procfs.clone()),
    ));
    let state = Arc::new(tokio::sync::Mutex::new(ManagerState::new(manifest.clone())));
    let manager_fs = Arc::new(ServiceManagerFs::new(state.clone()));
    let host_mount_service = HostMountService::new(host_mount_adapter);
    let supervised_handles = create_supervised_handles(&manifest);
    let agent_runtime = AgentRuntimeService::new(
        procfs.clone(),
        agent_root.clone(),
        host_mount_service.clone(),
        connection_service.clone(),
        tools.process_runner(),
    );
    if let Some(port) = workspace_isolation_port {
        let _ = agent_runtime.workspace_isolation_port.set(port);
    }

    let caller_mnt_mounts = embed_mounts::caller_mnt_mounts(&launch_context.namespace)?;
    let mut namespace = launch_context.namespace.child();
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
    let manager_handle = supervised_handles[SERVICE_MANAGER_SRV_HANDLE].clone();
    manager_handle.bind(manager_fs);
    srvfs
        .post(
            SERVICE_MANAGER_SRV_HANDLE,
            InProcessTransport::new(manager_handle),
            Access::ReadWrite,
        )
        .await;

    for unit in manifest.ordered().filter(|unit| unit.name != "root-agent") {
        loop {
            let attempt = async {
                if unit.name == "local-entry" {
                    mount_service_handles(&system_namespace, &srvfs).await?;
                }
                let extra_mounts: &[_] = if unit.name == "local-entry" {
                    &caller_mnt_mounts
                } else {
                    &[]
                };
                let (pid, unit_namespace) = spawn_unit_process(
                    &procfs,
                    manager_pid,
                    &system_namespace,
                    Credentials::system(),
                    unit,
                    extra_mounts,
                )
                .await?;
                state
                    .lock()
                    .await
                    .start_attempt(&unit.name, pid)
                    .map_err(|error| anyhow::anyhow!("track {} start: {error:?}", unit.name))?;
                if unit.name == "local-entry" {
                    let service = local_entry.get_or_insert_with(|| {
                        LocalEntryService::new_with_agent_root(
                            procfs.clone(),
                            unit_namespace,
                            agent_root.clone(),
                        )
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
                        supervised_handles: &supervised_handles,
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
                        if let Some(supervised) = supervised_handles.get(handle) {
                            supervised.deactivate();
                        }
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
        .cloned()
        .context("Root Agent Boot Unit is missing")?;
    let root_template_context = launch_context.rebound(
        system_namespace.snapshot(),
        launch_context.credentials.clone(),
    );
    launch_context = root_template_context;
    let reference_service = package_service.clone();
    let mut launch_context = tokio::task::spawn_blocking(move || {
        if include_builtin_skills {
            for package_id in alan_agent_engine::skills::preinstalled_skill_package_ids() {
                project_package_reference(&reference_service, &mut launch_context, package_id)?;
            }
        }
        Ok::<_, anyhow::Error>(launch_context)
    })
    .await
    .context("join initial Package references")??;
    let fileserver_manager = Arc::new(crate::FileserverManager::new(
        package_service.clone(),
        srvfs.clone(),
    ));
    let _ = agent_runtime
        .service_activation
        .set(service_activation::ServiceActivationDeps {
            packages: package_service.clone(),
            fileserver_manager: fileserver_manager.clone(),
            srvfs: srvfs.clone(),
            system_namespace: system_namespace.clone(),
            local_entry: local_entry.clone(),
        });
    let post_bind = async {
        packages::apply_root_package_bindings(
            &package_service,
            &fileserver_manager,
            &srvfs,
            &system_namespace,
            &local_entry,
            &mut launch_context,
            process.trust_posture,
            &root_package_bindings,
            &explicit_root_package_bindings,
        )
        .await?;
        validate_package_reference_mounts(&launch_context)?;
        process.agent_definition = alan_agent_engine::ResolvedAgentDefinition::from_process_inputs(
            launch_context.descriptor(alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR),
            &launch_context.package_references,
            &process.agent_config.core_config.resolved_skill_overrides(),
            process.core_config_source,
        )?;
        process.namespace_cwd = std::path::PathBuf::from(&launch_context.cwd);
        process.memory_store_bound = launch_context
            .descriptor(alan_agent_engine::MEMORY_STORE_DESCRIPTOR)
            .is_some();
        let root_template = RootAgentTemplate::new(
            process,
            launch_context,
            host_capabilities,
            generation_capabilities,
            llm_connection,
            resume_root,
            input_shadow,
        )
        .with_allow_profile_switch(allow_profile_switch)
        .with_ephemeral_rollout(ephemeral_rollout)
        .with_no_resume_update(no_resume_update)
        .with_resume_rollout_id(resume_rollout_id);
        let root = agent_runtime
            .launch_root(manager_pid, &system_namespace, &root_unit, &root_template)
            .await?;
        let root_pid = root.pid();
        if let Err(error) = state.lock().await.start_attempt("root-agent", root_pid) {
            agent_runtime.detach_root(root, 1).await?;
            return Err(anyhow::anyhow!("track Root Agent start: {error:?}"));
        }
        active_units.insert(
            "root-agent".to_string(),
            ActiveUnit {
                pid: root_pid,
                started_at: Instant::now(),
            },
        );
        Ok::<_, anyhow::Error>(SupervisorEnvironment {
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
            supervised_handles,
            active_units,
            manager_pid,
            local_entry,
            fileserver_manager: fileserver_manager.clone(),
        })
    }
    .await;
    match post_bind {
        Ok(env) => Ok(env),
        Err(error) => {
            let _ = fileserver_manager.stop_all().await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
