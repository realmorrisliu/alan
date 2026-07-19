//! Agent Runtime Service ownership of Agent Process assembly and lifecycle.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use alan_agent_engine::{
    AgentProcessConfig, RuntimeController,
    runtime::{
        AgentProcessLifecycle, AssembledChildAgentProcess, ChildAgentProcessAssembler,
        ChildAgentProcessAssemblyPlan, ChildAgentProcessAssemblyRequest, MountGrantApplicator,
        MountGrantApplicatorFactory,
    },
    spawn_with_namespace_environment,
    tools::{ToolExecutionAuthority, ToolProcessRunner},
};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{
    Access, Credentials, ExecNamespaceAccess, ExecNamespaceManifest, ExecNamespaceMount, ExecSpec,
    LiveNamespace, Pid,
};
use alan_llm::ProviderCapabilities;
use anyhow::{Context, Result, ensure};

use crate::{
    BootUnit, ConnectionService, HostMountApplicatorFactory, HostMountService,
    quartermaster::SystemProcessRunner,
    runtime::{namespace_with_package_references, spawn_unit_process},
};

pub(crate) struct RootAgentTemplate {
    process: AgentProcessConfig,
    host_capabilities: alan_agent_engine::skills::SkillHostCapabilities,
    generation_capabilities: ProviderCapabilities,
    llm_connection: String,
}

impl RootAgentTemplate {
    pub(crate) fn new(
        process: AgentProcessConfig,
        host_capabilities: alan_agent_engine::skills::SkillHostCapabilities,
        generation_capabilities: ProviderCapabilities,
        llm_connection: String,
    ) -> Self {
        Self {
            process,
            host_capabilities,
            generation_capabilities,
            llm_connection,
        }
    }
}

pub(crate) struct RootAgentProcess {
    pid: Pid,
    namespace: InProcessTransport,
    controller: RuntimeController,
}

impl RootAgentProcess {
    pub(crate) fn pid(&self) -> Pid {
        self.pid
    }

    pub(crate) fn namespace(&self) -> InProcessTransport {
        self.namespace.clone()
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.controller.is_finished()
    }

    pub(crate) async fn wait_until_ready(&mut self) -> Result<()> {
        self.controller.wait_until_ready().await.map(|_| ())
    }
}

pub(crate) struct AgentRuntimeService {
    procfs: alan_kernel::ProcFs,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
    srvfs: Arc<alan_kernel::SrvFs>,
    routefs: Arc<alan_routefs::RouteFs>,
    host_mount: Arc<HostMountService>,
    connection: Arc<ConnectionService>,
    tool_runner: ToolProcessRunner,
}

pub(crate) struct AgentRuntimeFileServers {
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
    srvfs: Arc<alan_kernel::SrvFs>,
    routefs: Arc<alan_routefs::RouteFs>,
}

impl AgentRuntimeFileServers {
    pub(crate) fn new(
        agent_root: Arc<alan_agentfs::AgentRootFs>,
        llmfs: Arc<alan_llmfs::LlmFs>,
        srvfs: Arc<alan_kernel::SrvFs>,
        routefs: Arc<alan_routefs::RouteFs>,
    ) -> Self {
        Self {
            agent_root,
            llmfs,
            srvfs,
            routefs,
        }
    }

    pub(crate) fn from_refs(
        agent_root: &Arc<alan_agentfs::AgentRootFs>,
        llmfs: &Arc<alan_llmfs::LlmFs>,
        srvfs: &Arc<alan_kernel::SrvFs>,
        routefs: &Arc<alan_routefs::RouteFs>,
    ) -> Self {
        Self::new(
            agent_root.clone(),
            llmfs.clone(),
            srvfs.clone(),
            routefs.clone(),
        )
    }
}

impl AgentRuntimeService {
    pub(crate) fn new(
        procfs: alan_kernel::ProcFs,
        file_servers: AgentRuntimeFileServers,
        host_mount: Arc<HostMountService>,
        connection: Arc<ConnectionService>,
        tool_runner: ToolProcessRunner,
    ) -> Arc<Self> {
        let AgentRuntimeFileServers {
            agent_root,
            llmfs,
            srvfs,
            routefs,
        } = file_servers;
        Arc::new(Self {
            procfs,
            agent_root,
            llmfs,
            srvfs,
            routefs,
            host_mount,
            connection,
            tool_runner,
        })
    }

    pub(crate) async fn launch_root(
        self: &Arc<Self>,
        parent_pid: Pid,
        system_namespace: &LiveNamespace,
        unit: &BootUnit,
        template: &RootAgentTemplate,
    ) -> Result<RootAgentProcess> {
        let launch_context = &template.process.launch_context;
        let credentials = launch_context.credentials.clone();
        let source_namespace =
            namespace_with_package_references(system_namespace.snapshot(), launch_context)?;
        let extra_mounts = launch_context
            .host_mounts
            .iter()
            .map(|grant| (grant.namespace_path.clone(), grant.access))
            .collect::<Vec<_>>();
        let (pid, namespace) = spawn_unit_process(
            &self.procfs,
            parent_pid,
            &source_namespace,
            credentials.clone(),
            unit,
            &extra_mounts,
        )
        .await?;

        let launch: Result<(InProcessTransport, RuntimeController)> = async {
            let llm = Arc::new(self.llmfs.connection_view(&template.llm_connection));
            namespace.replace_mount(
                "/mnt/llm",
                InProcessTransport::new(llm.clone()),
                Access::ReadWrite,
            );
            self.host_mount.register_process(pid, namespace.clone());
            namespace.replace_mount(
                "/mnt/host-mount",
                InProcessTransport::new(self.host_mount.file_server_for_process(pid.0)),
                Access::ReadWrite,
            );
            if self.connection.has_profile(&template.llm_connection) {
                self.connection.select(pid.0, &template.llm_connection)?;
            }

            self.agent_root
                .bind_process(pid.0.to_string(), Arc::new(alan_agentfs::AgentFs::new()))
                .await;
            self.agent_root.set_root_process(pid.0.to_string()).await;

            let tool_runner = self.tool_runner.clone();
            let procfs_with_runner =
                self.procfs
                    .clone()
                    .with_runner(Arc::new(SystemProcessRunner::new(Some(Arc::new(
                        tool_runner.clone(),
                    )))));
            self.procfs
                .bind_live_namespace(pid, namespace.clone())
                .await;
            namespace.replace_mount(
                "/proc",
                InProcessTransport::new(Arc::new(procfs_with_runner.for_live_spawner(
                    Some(pid),
                    namespace.clone(),
                    credentials.clone(),
                ))),
                Access::ReadWrite,
            );

            let root = InProcessTransport::new(Arc::new(
                alan_kernel::MountFs::from_live_namespace(namespace.clone()),
            ));
            let launch_context = launch_context.rebound_live(namespace.clone(), credentials);
            let mount_applicator =
                self.mount_applicator(pid, namespace.clone(), &launch_context, &tool_runner);
            self.register_tool_execution_binding(
                pid,
                &launch_context,
                template
                    .process
                    .store_bindings
                    .as_ref()
                    .map(|stores| stores.tmp.clone()),
            )?;
            let environment = alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
                root.clone(),
                format!("/agent/{}", pid.0),
                template.llm_connection.clone(),
            )
            .with_launch_context(launch_context.clone())
            .with_tool_process_context(pid, tool_runner)
            .with_mount_grant_applicator(mount_applicator)
            .with_child_process_assembler(self.child_process_assembler(pid));
            let mut process = template.process.clone();
            process.launch_context = launch_context;
            let controller = spawn_with_namespace_environment(
                process,
                environment,
                template.host_capabilities.clone(),
                template.generation_capabilities,
            )?;
            Ok((root, controller))
        }
        .await;
        let (namespace, controller) = match launch {
            Ok(launched) => launched,
            Err(error) => {
                self.procfs.record_exit(pid, 1).await;
                self.release_process(pid).await;
                return Err(error);
            }
        };

        Ok(RootAgentProcess {
            pid,
            namespace,
            controller,
        })
    }

    pub(crate) async fn detach_root(&self, root: RootAgentProcess, exit_code: i32) {
        let RootAgentProcess {
            pid, controller, ..
        } = root;
        if !controller.is_finished() {
            controller.abort().await;
        }
        self.procfs.record_exit(pid, exit_code).await;
        self.release_process(pid).await;
    }

    pub(crate) async fn shutdown_root(&self, root: RootAgentProcess) -> Result<()> {
        let RootAgentProcess {
            pid, controller, ..
        } = root;
        let result = controller.shutdown().await;
        self.procfs.record_exit(pid, 0).await;
        self.release_process(pid).await;
        result
    }

    pub(crate) async fn release_process(&self, pid: Pid) {
        self.agent_root.unbind_process(&pid.0.to_string()).await;
        self.host_mount.unregister_process(pid);
        self.connection.release_process(pid.0);
        self.tool_runner.unregister_process(pid);
    }

    fn mount_applicator(
        &self,
        pid: Pid,
        namespace: LiveNamespace,
        launch_context: &alan_agent_engine::ProcessLaunchContext,
        tool_runner: &ToolProcessRunner,
    ) -> Arc<dyn MountGrantApplicator> {
        let factory = HostMountApplicatorFactory::new(self.host_mount.clone());
        let inherited_mount_paths = launch_context.host_mount_namespace_paths();
        let applicator = factory.create(pid, namespace, &inherited_mount_paths);
        if let Some(authority) = factory.tool_execution_authority() {
            tool_runner.register_process_authority(pid, authority);
        }
        applicator
    }

    /// Seed one Process with an explicit Tool binding.
    ///
    /// Host Mount Service reconciles this binding immediately before each Tool Process starts.
    /// Keeping an authority-free seed for a mount-free Process means the first logical Host Mount
    /// can supply native authority without recreating a binding in the Agent Execution Engine.
    fn register_tool_execution_binding(
        &self,
        pid: Pid,
        launch_context: &alan_agent_engine::ProcessLaunchContext,
        scratch_dir: Option<PathBuf>,
    ) -> Result<()> {
        let Some(scratch_dir) = scratch_dir else {
            ensure!(
                !launch_context.has_host_mounts(),
                "Agent Process with Host Mounts requires Agent Runtime Service store bindings",
            );
            return Ok(());
        };
        let binding = if launch_context.host_mounts.is_empty() {
            alan_agent_engine::tools::ToolExecutionBinding::awaiting_host_projection(
                Path::new(&launch_context.cwd).to_path_buf(),
                scratch_dir,
            )
        } else {
            alan_agent_engine::tools::ToolExecutionBinding::from_launch_context(
                launch_context,
                scratch_dir,
            )?
        };
        let binding = if launch_context.has_host_mounts() {
            self.host_mount.reconcile(pid, binding)?
        } else {
            binding
        };
        self.tool_runner.register_process_binding(pid, binding);
        Ok(())
    }
}

impl AgentRuntimeService {
    fn child_process_assembler(
        self: &Arc<Self>,
        parent_pid: Pid,
    ) -> Arc<dyn ChildAgentProcessAssembler> {
        Arc::new(ServiceChildProcessAssembler {
            service: self.clone(),
            parent_pid,
        })
    }

    async fn assemble_child(
        self: &Arc<Self>,
        parent_pid: Pid,
        request: ChildAgentProcessAssemblyRequest,
    ) -> Result<AssembledChildAgentProcess> {
        let ChildAgentProcessAssemblyRequest {
            mut plan,
            scratch_dir,
            executable,
        } = request;
        let handles = ChildNamespaceHandles::new(self, &plan)?;
        validate_tool_mounts(&plan, &handles)?;
        let agent_root_tree = InProcessTransport::new(self.agent_root.clone());
        let spawner_namespace = child_namespace(&plan, agent_root_tree.clone(), &handles);
        let spawner = self.procfs.for_spawner(
            Some(parent_pid),
            spawner_namespace,
            Credentials::user("root-agent"),
        );
        let clone_fid = next_child_fid();
        spawner
            .walk(Fid::ROOT, clone_fid, &["clone".to_string()])
            .await
            .context("walk child /proc/clone")?;
        spawner
            .open(clone_fid, OpenMode::ReadWrite)
            .await
            .context("open child /proc/clone")?;
        let pid = String::from_utf8(
            spawner
                .read(clone_fid, 0, 64)
                .await
                .context("read child /proc/clone pid")?,
        )
        .context("child /proc/clone pid is utf8")?
        .trim()
        .to_string();
        let child_pid = Pid(pid
            .parse::<u64>()
            .with_context(|| format!("parse child pid '{pid}'"))?);
        let lifecycle = Arc::new(ServiceAgentProcessLifecycle {
            service: self.clone(),
            pid: child_pid,
        });

        let child_credentials = Credentials::user("child-agent");
        let live_namespace =
            LiveNamespace::new(child_namespace(&plan, agent_root_tree.clone(), &handles));
        let mount_applicator = self.mount_applicator(
            child_pid,
            live_namespace.clone(),
            &plan.launch_context,
            &self.tool_runner,
        );
        plan.launch_context = plan
            .launch_context
            .rebound_live(live_namespace.clone(), child_credentials.clone());

        let launch = async {
            let exec = child_exec_spec(&plan, &pid, executable);
            let exec_bytes = serde_json::to_vec(&exec).context("serialize child exec spec")?;
            spawner
                .write(clone_fid, 0, &exec_bytes)
                .await
                .context("write child exec spec to /proc/clone")?;
            spawner
                .clunk(clone_fid)
                .await
                .context("commit child /proc/clone")?;
            self.agent_root
                .bind_process(pid.clone(), handles.agent_tree.clone())
                .await;
            if self.connection.has_profile(&plan.llm_connection_name) {
                self.connection
                    .select(child_pid.0, &plan.llm_connection_name)?;
            }

            self.register_tool_execution_binding(child_pid, &plan.launch_context, scratch_dir)?;

            let runtime_procfs = self
                .procfs
                .clone()
                .with_runner(Arc::new(self.tool_runner.clone()));
            runtime_procfs
                .bind_live_namespace(child_pid, live_namespace.clone())
                .await;
            let child_procfs = runtime_procfs.for_live_spawner(
                Some(child_pid),
                live_namespace.clone(),
                child_credentials,
            );
            live_namespace.mount(
                "/proc",
                InProcessTransport::new(Arc::new(child_procfs)),
                Access::ReadWrite,
            );
            live_namespace.replace_mount(
                "/mnt/host-mount",
                InProcessTransport::new(self.host_mount.file_server_for_process(child_pid.0)),
                Access::ReadWrite,
            );
            let root = InProcessTransport::new(Arc::new(
                alan_kernel::MountFs::from_live_namespace(live_namespace.clone()),
            ));
            let mut environment = alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
                root,
                format!("/agent/{pid}"),
                plan.llm_connection_name()?,
            )
            .with_launch_context(plan.launch_context.clone())
            .with_tool_process_context(child_pid, self.tool_runner.clone())
            .with_mount_grant_applicator(mount_applicator)
            .with_child_process_assembler(self.child_process_assembler(child_pid));
            apply_agent_definition_grant(&mut environment, &plan)?;
            let observation_environment =
                child_observation_environment(&self.procfs, &self.agent_root, &pid, &plan).await?;
            Ok((environment, observation_environment))
        }
        .await;

        match launch {
            Ok((environment, observation_environment)) => Ok(AssembledChildAgentProcess {
                pid,
                environment,
                observation_environment,
                lifecycle,
            }),
            Err(error) => {
                lifecycle.finish(1).await;
                Err(error)
            }
        }
    }
}

#[derive(Clone)]
struct ChildNamespaceHandles {
    agent_tree: Arc<alan_agentfs::AgentFs>,
    llm_connection: InProcessTransport,
    srv: InProcessTransport,
    route: InProcessTransport,
    bin_tools: Vec<(String, InProcessTransport)>,
    tool_manifests: Vec<(String, InProcessTransport)>,
}

impl ChildNamespaceHandles {
    fn new(service: &AgentRuntimeService, plan: &ChildAgentProcessAssemblyPlan) -> Result<Self> {
        let mut handles = Self {
            agent_tree: Arc::new(alan_agentfs::AgentFs::new()),
            llm_connection: InProcessTransport::new(Arc::new(
                service.llmfs.connection_view(&plan.llm_connection_name()?),
            )),
            srv: InProcessTransport::new(service.srvfs.clone()),
            route: InProcessTransport::new(service.routefs.clone()),
            bin_tools: Vec::new(),
            tool_manifests: Vec::new(),
        };
        for manifest in &plan.tool_packages {
            manifest.validate_for_name(&manifest.name)?;
            let name = &manifest.name;
            handles.bin_tools.push((
                format!("/bin/{name}"),
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            ));
            handles.tool_manifests.push((
                format!("/lib/exec/{name}"),
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                    "manifest",
                    serde_json::to_vec(manifest)?,
                ))),
            ));
        }
        Ok(handles)
    }
}

fn validate_tool_mounts(
    plan: &ChildAgentProcessAssemblyPlan,
    handles: &ChildNamespaceHandles,
) -> Result<()> {
    let expected = plan
        .bin_tool_mounts
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let actual = handles
        .bin_tools
        .iter()
        .map(|(path, _)| path.as_str())
        .collect::<BTreeSet<_>>();
    ensure!(
        expected == actual,
        "child namespace Tool mounts do not match the selected package set"
    );
    Ok(())
}

fn child_namespace(
    plan: &ChildAgentProcessAssemblyPlan,
    agent_root: InProcessTransport,
    handles: &ChildNamespaceHandles,
) -> alan_kernel::Namespace {
    let mut namespace = plan.launch_context.namespace.child();
    namespace.mount(&plan.agent_mount, agent_root, Access::ReadWrite);
    namespace.mount(
        &plan.llm_mount,
        handles.llm_connection.clone(),
        Access::ReadWrite,
    );
    namespace.mount(&plan.srv_mount, handles.srv.clone(), Access::ReadOnly);
    namespace.mount(&plan.route_mount, handles.route.clone(), Access::ReadWrite);
    for (path, tree) in &handles.bin_tools {
        namespace.mount(path, tree.clone(), Access::ReadOnly);
    }
    for (path, tree) in &handles.tool_manifests {
        namespace.mount(path, tree.clone(), Access::ReadOnly);
    }
    namespace
}

fn child_exec_spec(
    plan: &ChildAgentProcessAssemblyPlan,
    pid: &str,
    executable: String,
) -> ExecSpec {
    ExecSpec {
        executable,
        args: Vec::new(),
        namespace: Some(child_namespace_manifest(plan, pid)),
        descriptors: plan
            .launch_context
            .descriptors
            .iter()
            .zip(3_u32..)
            .map(|((_, descriptor), number)| (number, descriptor.path.clone()))
            .collect(),
    }
}

fn child_namespace_manifest(
    plan: &ChildAgentProcessAssemblyPlan,
    _pid: &str,
) -> ExecNamespaceManifest {
    let mut mounts = vec![
        ExecNamespaceMount::new(plan.agent_mount.clone(), ExecNamespaceAccess::ReadWrite),
        ExecNamespaceMount::new(plan.llm_mount.clone(), ExecNamespaceAccess::ReadWrite),
        ExecNamespaceMount::new(plan.route_mount.clone(), ExecNamespaceAccess::ReadWrite),
        ExecNamespaceMount::new(plan.srv_mount.clone(), ExecNamespaceAccess::ReadOnly),
    ];
    mounts.extend(
        plan.bin_tool_mounts
            .iter()
            .cloned()
            .map(|path| ExecNamespaceMount::new(path, ExecNamespaceAccess::ReadOnly)),
    );
    mounts.extend(plan.bin_tool_names().map(|name| {
        ExecNamespaceMount::new(format!("/lib/exec/{name}"), ExecNamespaceAccess::ReadOnly)
    }));
    mounts.extend(plan.launch_context.host_mounts.iter().map(|grant| {
        ExecNamespaceMount::new(grant.namespace_path.clone(), exec_access(grant.access))
    }));
    mounts.extend(
        plan.launch_context
            .projected_host_mounts()
            .into_iter()
            .map(|(path, access)| ExecNamespaceMount::new(path, exec_access(access))),
    );
    mounts.extend(
        plan.launch_context
            .package_references
            .iter()
            .filter_map(|reference| {
                plan.launch_context
                    .namespace
                    .resolve(&reference.namespace_path)
                    .ok()
                    .map(|resolved| {
                        ExecNamespaceMount::new(
                            reference.namespace_path.clone(),
                            exec_access(resolved.access),
                        )
                    })
            }),
    );
    mounts.extend(
        plan.launch_context
            .descriptors
            .values()
            .filter(|descriptor| {
                !plan
                    .launch_context
                    .package_references
                    .iter()
                    .any(|reference| {
                        Path::new(&descriptor.path).starts_with(&reference.namespace_path)
                    })
            })
            .filter_map(|descriptor| {
                plan.launch_context
                    .namespace
                    .resolve(&descriptor.path)
                    .ok()
                    .map(|resolved| {
                        ExecNamespaceMount::new(
                            descriptor.path.clone(),
                            exec_access(resolved.access),
                        )
                    })
            }),
    );
    mounts.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.access.cmp(&right.access))
    });
    mounts.dedup();
    ExecNamespaceManifest { mounts }
}

fn exec_access(access: Access) -> ExecNamespaceAccess {
    match access {
        Access::ReadOnly => ExecNamespaceAccess::ReadOnly,
        Access::ReadWrite => ExecNamespaceAccess::ReadWrite,
    }
}

fn apply_agent_definition_grant(
    environment: &mut alan_agent_engine::runtime::NamespaceRuntimeEnvironment,
    plan: &ChildAgentProcessAssemblyPlan,
) -> Result<()> {
    let Some(grant) = plan
        .launch_context
        .host_mounts
        .iter()
        .find(|grant| grant.namespace_path == "/agent-definition")
    else {
        return Ok(());
    };
    let applied = environment.mount_control().apply_approved_grant(
        &alan_agent_engine::runtime::ApprovedMountGrant::new(
            grant.namespace_path.clone(),
            grant.host_path.clone(),
            match grant.access {
                Access::ReadOnly => alan_agent_engine::runtime::ApprovedMountGrantAccess::ReadOnly,
                Access::ReadWrite => {
                    alan_agent_engine::runtime::ApprovedMountGrantAccess::ReadWrite
                }
            },
            "Agent Definition launch reference",
        ),
    );
    ensure!(
        applied.namespace_applied,
        "failed to project child Agent Definition: {}",
        applied
            .namespace_error
            .unwrap_or_else(|| "unknown projection error".to_string())
    );
    Ok(())
}

async fn child_observation_environment(
    procfs: &alan_kernel::ProcFs,
    agent_root: &alan_agentfs::AgentRootFs,
    pid: &str,
    plan: &ChildAgentProcessAssemblyPlan,
) -> Result<alan_agent_engine::runtime::NamespaceRuntimeEnvironment> {
    let agent_path = format!("/agent/{pid}");
    let agent_tree = agent_root
        .process_tree(pid)
        .await
        .with_context(|| format!("attach observer to child AgentFS {agent_path}"))?;
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        &agent_path,
        InProcessTransport::new(agent_tree),
        Access::ReadWrite,
    );
    namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        Access::ReadWrite,
    );
    Ok(
        alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
            InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace))),
            agent_path,
            plan.llm_connection_name.clone(),
        )
        .with_launch_context(plan.launch_context.clone()),
    )
}

#[derive(Clone)]
struct ServiceChildProcessAssembler {
    service: Arc<AgentRuntimeService>,
    parent_pid: Pid,
}

impl std::fmt::Debug for ServiceChildProcessAssembler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceChildProcessAssembler")
            .field("parent_pid", &self.parent_pid)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl ChildAgentProcessAssembler for ServiceChildProcessAssembler {
    async fn assemble(
        &self,
        request: ChildAgentProcessAssemblyRequest,
    ) -> Result<AssembledChildAgentProcess> {
        self.service.assemble_child(self.parent_pid, request).await
    }
}

struct ServiceAgentProcessLifecycle {
    service: Arc<AgentRuntimeService>,
    pid: Pid,
}

impl std::fmt::Debug for ServiceAgentProcessLifecycle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceAgentProcessLifecycle")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl AgentProcessLifecycle for ServiceAgentProcessLifecycle {
    async fn finish(&self, exit_code: i32) {
        self.service.procfs.record_exit(self.pid, exit_code).await;
        self.service.release_process(self.pid).await;
    }
}

static NEXT_CHILD_FID: AtomicU64 = AtomicU64::new(90_000);

fn next_child_fid() -> Fid {
    Fid(NEXT_CHILD_FID.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn service_assembles_and_releases_child_agent_process() {
        let procfs = alan_kernel::ProcFs::new();
        let mut base = alan_kernel::Namespace::new();
        for executable in ["/bin/parent", "/bin/alan-agent"] {
            base.mount(
                executable,
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
                Access::ReadOnly,
            );
        }
        let live = LiveNamespace::new(base);
        let parent_pid = crate::runtime::spawn_process(
            &procfs,
            None,
            live.clone(),
            Credentials::system(),
            "/bin/parent",
        )
        .await
        .unwrap();
        let agent_root = Arc::new(alan_agentfs::AgentRootFs::new(Arc::new(procfs.clone())));
        let tools = alan_agent_engine::ToolRegistry::new();
        let service = AgentRuntimeService::new(
            procfs.clone(),
            AgentRuntimeFileServers::new(
                agent_root.clone(),
                Arc::new(alan_llmfs::LlmFs::new()),
                Arc::new(alan_kernel::SrvFs::new()),
                Arc::new(alan_routefs::RouteFs::new()),
            ),
            HostMountService::unavailable(),
            ConnectionService::ephemeral("test"),
            tools.process_runner(),
        );
        let launch_context = alan_agent_engine::ProcessLaunchContext::new(
            live.snapshot(),
            Credentials::user("parent-agent"),
            "/",
        )
        .unwrap();
        let assembly = service
            .child_process_assembler(parent_pid)
            .assemble(ChildAgentProcessAssemblyRequest {
                plan: ChildAgentProcessAssemblyPlan {
                    agent_mount: "/agent".to_string(),
                    llm_mount: "/mnt/llm".to_string(),
                    llm_connection_name: "default".to_string(),
                    srv_mount: "/srv".to_string(),
                    route_mount: "/mnt/route".to_string(),
                    bin_tool_mounts: Vec::new(),
                    tool_packages: Vec::new(),
                    cwd: Some("/".into()),
                    launch_context,
                },
                scratch_dir: None,
                executable: "/bin/alan-agent".to_string(),
            })
            .await
            .unwrap();

        assert!(agent_root.process_tree(&assembly.pid).await.is_some());
        let shell = alan_shell::Shell::new(assembly.environment.root_transport());
        let root_entries = shell.ls("/").await.unwrap();
        for required in ["agent", "mnt", "proc", "srv"] {
            assert!(root_entries.iter().any(|entry| entry == required));
        }
        let proc_shell = alan_shell::Shell::new(InProcessTransport::new(Arc::new(procfs.clone())));
        assert_eq!(
            String::from_utf8(
                proc_shell
                    .cat(&format!("/{}/parent", assembly.pid))
                    .await
                    .unwrap()
            )
            .unwrap(),
            parent_pid.0.to_string()
        );

        assembly.lifecycle.finish(0).await;
        assert!(agent_root.process_tree(&assembly.pid).await.is_none());
        assert_eq!(
            String::from_utf8(
                proc_shell
                    .cat(&format!("/{}/exit", assembly.pid))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "0"
        );
    }
}
