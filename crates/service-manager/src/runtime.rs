//! Service Manager boot and runtime ownership.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use alan_agent_engine::{
    AgentProcessConfig, LlmClient, ProcessPackageKind, ProcessPackageReference,
    ProcessPackageSkillReference, ToolRegistry, provider_capabilities_for_config,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, Credentials, LiveNamespace, Namespace, Pid};
use anyhow::{Context, Result, ensure};
use uuid::Uuid;

mod supervisor;

use supervisor::{
    ActiveUnit, SupervisorEnvironment, SupervisorRuntime, SwitchableFileServer,
    SystemServiceHandles, mount_service_handles, publish_unit_handles, wait_unit_ready,
};

use crate::{
    BootManifest, ConnectionService, ConnectionStoreBindings, ConnectionsFile,
    HostMountExportAdapter, HostMountService, LocalEntryService, ManagerState, PackageService,
    ProcessLaunchContext, RestartDecision, ServiceManagerFs, UnavailableHostMountExportAdapter,
    agent_runtime::{AgentRuntimeFileServers, AgentRuntimeService, RootAgentTemplate},
    process_spawn::{spawn_process, spawn_unit_process},
    quartermaster::QUARTERMASTER_EXECUTABLE,
};

pub const BOOT_ID_PATH: &str = "/proc/host/boot_id";
pub const BOOT_STATE_PATH: &str = "/proc/host/state";

const LLM_CONNECTION: &str = "default";
const SERVICE_MANAGER_EXECUTABLE: &str = "/bin/service-manager";

/// Explicit inputs supplied by the platform Host to Service Manager.
pub struct ServiceManagerConfig {
    pub channel_id: String,
    pub process: AgentProcessConfig,
    pub launch_context: ProcessLaunchContext,
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
            channel_id: channel_id.into(),
            launch_context,
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
    #[cfg(test)]
    root_namespace: InProcessTransport,
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
        let boot_id = Uuid::new_v4();
        let manifest = BootManifest::system().context("load system /lib/boot units")?;
        let package_service = match config.package_store.take() {
            Some(store) => PackageService::open(&config.channel_id, store)?,
            None => PackageService::ephemeral(&config.channel_id)?,
        };
        seed_preinstalled_packages(&package_service)?;
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
        let assembled = assemble_environment(AssembleInputs {
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
            launch_context: config.launch_context,
            tools: config.tools,
            host_capabilities,
            generation_capabilities,
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
        let package_handle = assembled.package_handle.clone();
        let root = assembled.root.namespace();
        let mut runtime = SupervisorRuntime::from_assembled(assembled, root_pid.clone());
        runtime.settle_initial_root().await?;
        verify_readiness(&root, boot_id, &state).await?;

        let (supervisor_shutdown, supervisor_task) = runtime.start();

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
            #[cfg(test)]
            root_namespace: root,
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
    launch_context: ProcessLaunchContext,
    tools: ToolRegistry,
    host_capabilities: alan_agent_engine::skills::SkillHostCapabilities,
    generation_capabilities: alan_llm::ProviderCapabilities,
}

async fn assemble_environment(inputs: AssembleInputs) -> Result<SupervisorEnvironment> {
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
        mut launch_context,
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
        AgentRuntimeFileServers::from_refs(&agent_root, &llmfs),
        host_mount_service.clone(),
        connection_service.clone(),
        tools.process_runner(),
    );

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
    let root_template_context = launch_context.rebound(
        system_namespace.snapshot(),
        launch_context.credentials.clone(),
    );
    launch_context = root_template_context;
    for source in alan_agent_engine::skills::preinstalled_skill_package_sources() {
        project_package_reference(&package_service, &mut launch_context, &source.package_id)?;
    }
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

    Ok(SupervisorEnvironment {
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

pub(crate) fn validate_package_reference_mounts(
    launch_context: &ProcessLaunchContext,
) -> Result<()> {
    for reference in &launch_context.package_references {
        let exact_mounts = launch_context.namespace.union_at(&reference.namespace_path);
        ensure!(
            exact_mounts.len() == 1 && exact_mounts[0].access == Access::ReadOnly,
            "package reference {} requires one exact read-only Process namespace mount",
            reference.namespace_path
        );
    }
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
