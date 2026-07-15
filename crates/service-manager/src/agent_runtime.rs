//! Agent Runtime Service ownership of Agent Process assembly and lifecycle.

use std::sync::Arc;

use alan_agent_engine::{
    AgentProcessConfig, RuntimeController, ToolRegistry, spawn_with_namespace_environment,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, LiveNamespace, Pid};
use alan_llm::ProviderCapabilities;
use anyhow::Result;

use crate::{
    BootUnit, ConnectionService, HostMountApplicatorFactory, HostMountService,
    quartermaster::SystemProcessRunner,
    runtime::{namespace_with_package_references, spawn_unit_process},
};

pub(crate) struct RootAgentTemplate {
    process: AgentProcessConfig,
    tools: ToolRegistry,
    host_capabilities: alan_agent_engine::skills::SkillHostCapabilities,
    generation_capabilities: ProviderCapabilities,
    llm_connection: String,
}

impl RootAgentTemplate {
    pub(crate) fn new(
        process: AgentProcessConfig,
        tools: ToolRegistry,
        host_capabilities: alan_agent_engine::skills::SkillHostCapabilities,
        generation_capabilities: ProviderCapabilities,
        llm_connection: String,
    ) -> Self {
        Self {
            process,
            tools,
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
}

impl AgentRuntimeService {
    pub(crate) fn new(
        procfs: alan_kernel::ProcFs,
        agent_root: Arc<alan_agentfs::AgentRootFs>,
        llmfs: Arc<alan_llmfs::LlmFs>,
        srvfs: Arc<alan_kernel::SrvFs>,
        routefs: Arc<alan_routefs::RouteFs>,
        host_mount: Arc<HostMountService>,
        connection: Arc<ConnectionService>,
    ) -> Arc<Self> {
        Arc::new(Self {
            procfs,
            agent_root,
            llmfs,
            srvfs,
            routefs,
            host_mount,
            connection,
        })
    }

    pub(crate) async fn launch_root(
        &self,
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
            if self.connection.has_profile(&template.llm_connection) {
                self.connection.select(pid.0, &template.llm_connection)?;
            }

            self.agent_root
                .bind_process(pid.0.to_string(), Arc::new(alan_agentfs::AgentFs::new()))
                .await;
            self.agent_root.set_root_process(pid.0.to_string()).await;

            let tool_runner = template.tools.process_runner();
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
            let launch_context = launch_context.rebound(namespace.snapshot(), credentials);
            let environment = alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
                root.clone(),
                format!("/agent/{}", pid.0),
                template.llm_connection.clone(),
            )
            .with_launch_context(launch_context.clone())
            .with_process_context(
                self.procfs.clone(),
                self.agent_root.clone(),
                pid,
                tool_runner,
            )
            .with_shared_services(
                InProcessTransport::new(self.srvfs.clone()),
                InProcessTransport::new(self.routefs.clone()),
                InProcessTransport::new(llm),
            )
            .with_mount_grant_applicator_factory(
                Arc::new(HostMountApplicatorFactory::new(self.host_mount.clone())),
                namespace,
            );
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

    pub(crate) async fn detach_root(&self, root: RootAgentProcess) {
        let RootAgentProcess {
            pid, controller, ..
        } = root;
        if !controller.is_finished() {
            controller.abort().await;
        }
        self.procfs.record_exit(pid, 1).await;
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
    }
}
