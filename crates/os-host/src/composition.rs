//! Temporary fixed Alan OS boot composition.
//!
//! `implement-minimal-service-manager` must delete this module and replace the
//! hard-coded boot order with boot units owned by Service Manager.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alan_agent_engine::runtime::effective_core_config_for_runtime;
use alan_agent_engine::{
    AgentProcessConfig, Config, LlmClient, ProcessDescriptor, ProcessLaunchContext,
    RuntimeController, ToolRegistry, configure_runtime_tool_execution_binding,
    spawn_with_namespace_environment,
};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, Credentials, LiveNamespace, Namespace, Pid};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use anyhow::{Context, Result, ensure};
use uuid::Uuid;

use crate::host_mounts::LiveNamespaceMountGrantApplicatorFactory;
use crate::paths::{HostStorePaths, SystemStorePaths};

pub const BOOT_ID_PATH: &str = "/proc/host/boot_id";
pub const BOOT_STATE_PATH: &str = "/proc/host/state";
pub const TEMPORARY_FIXED_COMPOSITION_SUCCESSOR: &str = "implement-minimal-service-manager";

const LLM_CONNECTION: &str = "default";
const ROOT_AGENT_EXECUTABLE: &str = "/bin/alan-agent";
static NEXT_BOOT_FID: AtomicU64 = AtomicU64::new(800_000);

/// Concrete inputs for the temporary fixed boot builder.
pub struct FixedBootConfig {
    pub channel_id: String,
    pub process: AgentProcessConfig,
    pub llm_client: LlmClient,
    pub tools: ToolRegistry,
}

impl FixedBootConfig {
    /// Build product inputs from the channel System Store and explicit Process descriptors.
    pub fn product(channel_id: &str) -> Result<Self> {
        let system_store = SystemStorePaths::detect(channel_id)?;
        let host_store = HostStorePaths::detect(channel_id)?;
        let memory_store_backing = system_store.memory_stores()?.join("default");
        std::fs::create_dir_all(&memory_store_backing)
            .context("failed to prepare Memory Store backing")?;

        let memory_store = alan_hostfs::HostDirFs::new(
            &memory_store_backing,
            alan_hostfs::HostDirAccess::ReadWrite,
        )
        .context("failed to open Memory Store backing")?;
        let mut namespace = Namespace::new();
        namespace.mount(
            "/memory",
            InProcessTransport::new(Arc::new(memory_store)),
            Access::ReadWrite,
        );

        let mut process = AgentProcessConfig::from(Config::load_with_metadata()?);
        process.launch_context = ProcessLaunchContext::new(namespace, Credentials::system(), "/")?
            .with_descriptor(
                alan_agent_engine::MEMORY_STORE_DESCRIPTOR,
                ProcessDescriptor::new("/memory")?,
            );
        process.store_bindings = Some(system_store.agent_runtime_bindings()?);
        process.memory_store_backing = Some(memory_store_backing);
        process.connection_store = Some(system_store.connection_bindings(&host_store)?);
        process.chatgpt_auth_storage_path = Some(host_store.managed_auth);
        process.mount_grant_applicator_factory =
            Some(Arc::new(LiveNamespaceMountGrantApplicatorFactory));

        let core_config = effective_core_config_for_runtime(&process)?;
        let llm_client = LlmClient::from_core_config_with_chatgpt_auth_storage_path(
            &core_config,
            process.chatgpt_auth_storage_path.clone(),
        )
        .context("failed to create Root Agent LLM connection")?;
        let tools = ToolRegistry::with_config(Arc::new(core_config));

        Ok(Self {
            channel_id: channel_id.to_string(),
            process,
            llm_client,
            tools,
        })
    }

    /// Explicit ephemeral/test inputs. Product callers never select this implicitly.
    pub fn ephemeral(
        channel_id: impl Into<String>,
        process: AgentProcessConfig,
        llm_client: LlmClient,
        tools: ToolRegistry,
    ) -> Self {
        Self {
            channel_id: channel_id.into(),
            process,
            llm_client,
            tools,
        }
    }
}

/// One running fixed-composition Alan OS instance.
pub(crate) struct FixedComposition {
    boot_id: Uuid,
    live_namespace: LiveNamespace,
    controller: RuntimeController,
}

impl FixedComposition {
    pub async fn boot(mut config: FixedBootConfig) -> Result<Self> {
        ensure!(
            matches!(config.channel_id.as_str(), "stable" | "dev" | "test"),
            "invalid Alan OS Host channel `{}`",
            config.channel_id
        );
        configure_runtime_tool_execution_binding(&config.process, &mut config.tools)?;

        let boot_id = Uuid::new_v4();
        let generation_capabilities = config.llm_client.capabilities();
        let host_capabilities = alan_agent_engine::skills::build_skill_host_capabilities(
            config.tools.list_tools().into_iter().map(str::to_string),
            true,
        );
        let (environment, root, live_namespace) = assemble_environment(
            boot_id,
            config.llm_client,
            &config.tools,
            &config.process.launch_context,
            config.process.mount_grant_applicator_factory.clone(),
        )
        .await?;
        let mut controller = spawn_with_namespace_environment(
            config.process,
            environment,
            host_capabilities,
            generation_capabilities,
        )?;
        controller
            .wait_until_ready()
            .await
            .context("Root Agent Process failed before readiness")?;
        verify_readiness(&root, boot_id).await?;

        Ok(Self {
            boot_id,
            live_namespace,
            controller,
        })
    }

    pub(crate) fn boot_id(&self) -> Uuid {
        self.boot_id
    }

    /// Create a connection-owned namespace view with an independent fid table.
    pub(crate) fn attachment_server(&self) -> Arc<alan_kernel::MountFs> {
        Arc::new(alan_kernel::MountFs::from_live_namespace(
            self.live_namespace.clone(),
        ))
    }

    pub(crate) async fn shutdown(self) -> Result<()> {
        self.controller.shutdown().await
    }
}

struct RuntimeLlmProvider {
    client: LlmClient,
}

#[async_trait::async_trait]
impl LlmProvider for RuntimeLlmProvider {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.client.generate(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.client.chat(system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.client.generate_stream(request).await
    }

    fn provider_name(&self) -> &'static str {
        self.client.provider_name()
    }
}

async fn assemble_environment(
    boot_id: Uuid,
    llm_client: LlmClient,
    tools: &ToolRegistry,
    launch_context: &ProcessLaunchContext,
    mount_grant_applicator_factory: Option<
        Arc<dyn alan_agent_engine::runtime::MountGrantApplicatorFactory>,
    >,
) -> Result<(
    alan_agent_engine::runtime::NamespaceRuntimeEnvironment,
    InProcessTransport,
    LiveNamespace,
)> {
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection(
        LLM_CONNECTION,
        Box::new(RuntimeLlmProvider { client: llm_client }),
    );
    let routefs = Arc::new(alan_routefs::RouteFs::new());
    let srvfs = Arc::new(alan_kernel::SrvFs::new());
    let procfs = alan_kernel::ProcFs::new();
    let agent_root = Arc::new(alan_agentfs::AgentRootFs::new(Arc::new(procfs.clone())));

    let mut namespace = launch_context.namespace.child();
    mount_standard_namespace_roots(&mut namespace);
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    mount_shared_services(
        &mut namespace,
        Arc::clone(&srvfs),
        llmfs,
        Arc::clone(&routefs),
    )
    .await?;
    mount_tool_packages(&mut namespace, tools)?;
    namespace.mount(
        "/proc/host",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_files([
            ("boot_id".to_string(), format!("{boot_id}\n").into_bytes()),
            ("state".to_string(), b"ready\n".to_vec()),
        ]))),
        Access::ReadOnly,
    );

    let live_namespace = LiveNamespace::new(namespace);
    let root_pid = spawn_root_agent_process(
        &procfs,
        live_namespace.clone(),
        launch_context.credentials.clone(),
    )
    .await?;
    agent_root
        .bind_process(root_pid.0.to_string(), agentfs)
        .await;
    agent_root.set_root_process(root_pid.0.to_string()).await;

    let tool_runner = tools.process_runner();
    let procfs_with_runner = procfs.clone().with_runner(Arc::new(tool_runner.clone()));
    procfs
        .bind_live_namespace(root_pid, live_namespace.clone())
        .await;
    let process_procfs = procfs_with_runner.for_live_spawner(
        Some(root_pid),
        live_namespace.clone(),
        launch_context.credentials.clone(),
    );
    live_namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(process_procfs)),
        Access::ReadWrite,
    );

    let root_mount = Arc::new(alan_kernel::MountFs::from_live_namespace(
        live_namespace.clone(),
    ));
    let root = InProcessTransport::new(root_mount);
    let route_tree = InProcessTransport::new(routefs);
    let environment = alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
        root.clone(),
        format!("/agent/{}", root_pid.0),
        LLM_CONNECTION,
    )
    .with_launch_context(launch_context.clone())
    .with_process_context(procfs, agent_root, root_pid, tool_runner)
    .with_shared_services(InProcessTransport::new(srvfs), route_tree);
    let environment = if let Some(factory) = mount_grant_applicator_factory {
        environment.with_mount_grant_applicator_factory(factory, live_namespace.clone())
    } else {
        environment
    };

    Ok((environment, root, live_namespace))
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

async fn mount_shared_services(
    namespace: &mut Namespace,
    srvfs: Arc<alan_kernel::SrvFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
    routefs: Arc<alan_routefs::RouteFs>,
) -> Result<()> {
    srvfs
        .post("llm", InProcessTransport::new(llmfs), Access::ReadWrite)
        .await;
    srvfs
        .post(
            alan_routefs::SRV_HANDLE,
            InProcessTransport::new(routefs),
            Access::ReadWrite,
        )
        .await;
    namespace.mount(
        "/srv",
        InProcessTransport::new(srvfs.clone()),
        Access::ReadOnly,
    );
    let (llm_tree, llm_access) = srvfs.lookup("llm").await.context("lookup /srv/llm")?;
    namespace.mount("/mnt/llm", llm_tree, llm_access);
    let (route_tree, route_access) = srvfs
        .lookup(alan_routefs::SRV_HANDLE)
        .await
        .context("lookup /srv/route")?;
    namespace.mount(alan_routefs::MOUNT_PATH, route_tree, route_access);
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

async fn spawn_root_agent_process(
    procfs: &alan_kernel::ProcFs,
    namespace: LiveNamespace,
    credentials: Credentials,
) -> Result<Pid> {
    let spawner = procfs.for_live_spawner(None, namespace.clone(), credentials);
    let fid = Fid(NEXT_BOOT_FID.fetch_add(1, Ordering::Relaxed));
    spawner
        .walk(Fid::ROOT, fid, &["clone".to_string()])
        .await
        .context("walk Root Agent /proc/clone")?;
    spawner
        .open(fid, OpenMode::ReadWrite)
        .await
        .context("open Root Agent /proc/clone")?;
    let pid = String::from_utf8(spawner.read(fid, 0, 64).await?)
        .context("Root Agent PID is not UTF-8")?
        .parse::<u64>()
        .context("Root Agent PID is invalid")?;
    let exec = alan_kernel::ExecSpec {
        executable: ROOT_AGENT_EXECUTABLE.to_string(),
        args: Vec::new(),
        namespace: Some(alan_kernel::ExecNamespaceManifest::from_namespace(
            &namespace.snapshot(),
        )),
    };
    spawner
        .write(fid, 0, &serde_json::to_vec(&exec)?)
        .await
        .context("write Root Agent exec spec")?;
    spawner
        .clunk(fid)
        .await
        .context("commit Root Agent Process")?;
    Ok(Pid(pid))
}

async fn verify_readiness(root: &InProcessTransport, boot_id: Uuid) -> Result<()> {
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
    for required in ["llm", alan_routefs::SRV_HANDLE] {
        ensure!(
            service_handles.iter().any(|entry| entry == required),
            "required service /srv/{required} is absent"
        );
    }
    Ok(())
}
