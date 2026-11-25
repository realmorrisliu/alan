use std::path::PathBuf;
use std::sync::Arc;

use alan_agent_engine::{
    AgentProcessConfig, LlmClient, ProcessDescriptor, ProcessFileTree, ReusableLlmClient,
    SharedLlmProvider, SystemPromptMode, tools::ToolRegistry, validate_response_schema,
};
use alan_ap::{FileServer, InProcessTransport};
use alan_kernel::{Access, Credentials, Namespace};
use alan_service_manager::{
    ConnectionsFile, HostMountExportAdapter, LlmClientFactory, ProcessLaunchContext,
    ServiceManagerConfig, UnavailableHostMountExportAdapter,
};
use anyhow::{Context, Result};

use super::policy_hook::ToolPolicyHook;
use crate::boot::{ProductLlmClientFactory, prepare_system_store_launch};
use crate::host_mounts::NativeHostMountExportAdapter;
use crate::paths::{HostStorePaths, SystemStorePaths};

/// Backing storage mode for an [`super::EmbeddedAlan`] instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageMode {
    /// Fully in-memory kernel/memory/package stores; leaves zero files on host disk.
    Ephemeral,
    /// App-owned isolated disk directories; never touches Alan's default user data dir.
    Isolated {
        /// Directory for System Store subtrees (`connections`, `packages`, `services`, `memory`).
        system_store_root: PathBuf,
        /// Optional directory for Host Store credentials and managed auth state.
        host_store_root: Option<PathBuf>,
    },
}

/// Model/credential source for an embedded Alan instance.
#[derive(Debug)]
pub enum LlmSource {
    /// Host-supplied factory controlling model resolution and credentials.
    Factory(Arc<dyn LlmClientFactory>),
    /// Pre-built [`LlmClient`] wrapped into a reusable provider shared across sessions/retries.
    Client(LlmClient),
    /// Shared, cloneable [`SharedLlmProvider`] reusable across multiple sessions and child agents.
    SharedProvider(SharedLlmProvider),
    /// Use Alan's `ConnectionService` + `HostStore` scoped to [`StorageMode::Isolated::host_store_root`].
    IsolatedHostStore,
    /// Explicitly opt into reading the user's shared `alan connection` profiles and Host credentials
    /// (`SystemStorePaths::detect()` + `HostStorePaths::detect()`) while keeping rollouts/memory
    /// isolated to this instance's [`StorageMode`].
    SharedProductCredentials,
}

#[derive(Debug)]
struct ReusableClientFactory(ReusableLlmClient);

impl LlmClientFactory for ReusableClientFactory {
    fn create(
        &self,
        _base_config: &alan_agent_engine::Config,
        _selected_profile: Option<&str>,
        _connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        Ok(self.0.client())
    }
}

/// Configuration for booting an isolated [`super::EmbeddedAlan`] instance.
pub struct EmbeddedConfig {
    pub storage: StorageMode,
    pub process: AgentProcessConfig,
    pub llm: LlmSource,
    pub tools: ToolRegistry,
    pub host_mount_adapter: Arc<dyn HostMountExportAdapter>,
    pub namespace_mounts: Vec<(String, InProcessTransport, Access)>,
    pub definition: Option<ProcessFileTree>,
    pub resume_root: bool,
    /// System prompt composition mode (`Append` default or `Replace` custom prompt).
    pub system_prompt_mode: SystemPromptMode,
    /// Whether Alan's preinstalled built-in coding skills are projected onto Agent Processes.
    pub builtin_skills: bool,
    /// Optional default JSON Schema constraining assistant structured responses.
    pub response_schema: Option<serde_json::Value>,
    /// Optional synchronous policy hook invoked before executing non-virtual Tools.
    pub tool_policy_hook: Option<Arc<dyn ToolPolicyHook>>,
    pub(super) host_mounts_enabled: bool,
}

impl EmbeddedConfig {
    /// Create an ephemeral, zero-host-access embedded configuration.
    pub fn ephemeral(llm: LlmSource) -> Self {
        Self {
            storage: StorageMode::Ephemeral,
            process: AgentProcessConfig::default(),
            llm,
            tools: ToolRegistry::new(),
            host_mount_adapter: Arc::new(UnavailableHostMountExportAdapter),
            namespace_mounts: Vec::new(),
            definition: None,
            resume_root: false,
            system_prompt_mode: SystemPromptMode::Append,
            builtin_skills: true,
            response_schema: None,
            tool_policy_hook: None,
            host_mounts_enabled: false,
        }
    }

    /// Create an isolated-disk embedded configuration with native Host Mount capability enabled
    /// on explicit host calls (zero Host directories are mounted at boot).
    pub fn isolated(system_store_root: impl Into<PathBuf>, llm: LlmSource) -> Self {
        Self {
            storage: StorageMode::Isolated {
                system_store_root: system_store_root.into(),
                host_store_root: None,
            },
            process: AgentProcessConfig::default(),
            llm,
            tools: ToolRegistry::new(),
            host_mount_adapter: Arc::new(NativeHostMountExportAdapter),
            namespace_mounts: Vec::new(),
            definition: None,
            resume_root: false,
            system_prompt_mode: SystemPromptMode::Append,
            builtin_skills: true,
            response_schema: None,
            tool_policy_hook: None,
            host_mounts_enabled: true,
        }
    }

    /// Set an isolated Host Store root when using [`StorageMode::Isolated`].
    pub fn with_host_store(mut self, host_store_root: impl Into<PathBuf>) -> Result<Self> {
        match &mut self.storage {
            StorageMode::Isolated {
                host_store_root: slot,
                ..
            } => {
                *slot = Some(host_store_root.into());
                Ok(self)
            }
            StorageMode::Ephemeral => {
                anyhow::bail!("ephemeral storage mode cannot attach a Host Store directory")
            }
        }
    }

    /// Override the default [`AgentProcessConfig`].
    pub fn with_process_config(mut self, process: AgentProcessConfig) -> Self {
        self.process = process;
        self
    }

    /// Supply the explicit [`ToolRegistry`] available to Agent Processes.
    pub fn with_tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = tools;
        self
    }

    /// Supply an explicit [`HostMountExportAdapter`] if the host allows host directory mounts.
    pub fn with_host_mount_adapter(mut self, adapter: Arc<dyn HostMountExportAdapter>) -> Self {
        self.host_mount_adapter = adapter;
        self.host_mounts_enabled = true;
        self
    }

    /// Enable or disable the native [`NativeHostMountExportAdapter`] for dynamic `host_mount` grants.
    pub fn with_native_host_mounts(mut self, enabled: bool) -> Self {
        self.host_mounts_enabled = enabled;
        self.host_mount_adapter = if enabled {
            Arc::new(NativeHostMountExportAdapter)
        } else {
            Arc::new(UnavailableHostMountExportAdapter)
        };
        self
    }

    /// Mount an app-owned [`FileServer`] into the root namespace (e.g. `/mnt/my-app`).
    pub fn with_mount(
        mut self,
        path: impl Into<String>,
        server: Arc<dyn FileServer>,
        access: Access,
    ) -> Self {
        self.namespace_mounts
            .push((path.into(), InProcessTransport::new(server), access));
        self
    }

    /// Supply an explicit in-memory Agent Definition tree for `/lib/agents/root`.
    pub fn with_definition(mut self, definition: ProcessFileTree) -> Self {
        self.definition = Some(definition);
        self
    }

    /// Enable Root Agent rollout resumption (requires [`StorageMode::Isolated`]).
    pub fn with_root_resume(mut self, resume_root: bool) -> Self {
        self.resume_root = resume_root;
        self
    }

    /// Set how the system prompt is assembled (`Append` or `Replace`).
    pub fn with_system_prompt_mode(mut self, mode: SystemPromptMode) -> Self {
        self.system_prompt_mode = mode;
        self
    }

    /// Control whether Alan's preinstalled built-in skills are projected onto Agent Processes.
    pub fn with_builtin_skills(mut self, enabled: bool) -> Self {
        self.builtin_skills = enabled;
        self
    }

    /// Set or clear a default JSON Schema constraining structured assistant output.
    pub fn with_response_schema(mut self, schema: Option<serde_json::Value>) -> Result<Self> {
        if let Some(schema) = &schema {
            validate_response_schema(schema).context("invalid EmbeddedConfig response_schema")?;
        }
        self.response_schema = schema;
        Ok(self)
    }

    /// Install a synchronous host policy hook checked before executing non-virtual Tools.
    pub fn with_tool_policy_hook(mut self, hook: Arc<dyn ToolPolicyHook>) -> Self {
        self.tool_policy_hook = Some(hook);
        self
    }

    pub(crate) fn into_service_manager_config(mut self) -> Result<(ServiceManagerConfig, bool)> {
        if !matches!(self.system_prompt_mode, SystemPromptMode::Append) {
            self.process.system_prompt_mode = self.system_prompt_mode;
        }
        if let Some(schema) = self.response_schema {
            validate_response_schema(&schema).context("invalid EmbeddedConfig response_schema")?;
            self.process.response_schema = Some(schema);
        }
        if let Some(hook) = self.tool_policy_hook {
            self.process.tool_policy_hook = Some(hook);
        }

        let mut namespace = Namespace::new();
        for (path, transport, access) in self.namespace_mounts {
            namespace.mount(&path, transport, access);
        }

        let (
            launch_context,
            mut connection_store,
            package_store,
            host_store_paths,
            in_memory_stores,
        ) = match self.storage {
            StorageMode::Ephemeral => {
                if namespace.resolve("/memory").is_err() {
                    namespace.mount(
                        "/memory",
                        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::writable())),
                        Access::ReadWrite,
                    );
                }
                if namespace.resolve("/lib/agents/root").is_err() {
                    namespace.mount(
                        "/lib/agents/root",
                        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                        Access::ReadOnly,
                    );
                }
                let mut ctx = ProcessLaunchContext::new(namespace, Credentials::system(), "/")?
                    .with_descriptor(
                        alan_agent_engine::MEMORY_STORE_DESCRIPTOR,
                        ProcessDescriptor::new("/memory")?,
                    );
                if let Some(tree) = self.definition {
                    ctx = ctx.with_descriptor(
                        alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
                        ProcessDescriptor::with_file_tree("/lib/agents/root", tree)?,
                    );
                }
                (ctx, None, None, None, true)
            }
            StorageMode::Isolated {
                system_store_root,
                host_store_root,
            } => {
                let system_store = SystemStorePaths::from_root(&system_store_root)?;
                let host_store = host_store_root
                    .as_deref()
                    .map(HostStorePaths::from_root)
                    .transpose()?;
                let ctx = prepare_system_store_launch(
                    &system_store,
                    namespace,
                    &mut self.process,
                    self.definition,
                )?;
                let connection_store = Some(system_store.connection_bindings()?);
                let package_store = Some(system_store.packages()?);
                (ctx, connection_store, package_store, host_store, false)
            }
        };

        let mut tool_secret_resolver: Option<
            Arc<dyn alan_agent_engine::tools::ToolSecretResolver>,
        > = match host_store_paths.as_ref() {
            Some(host_store) => Some(Arc::new(crate::SecretStore::from_directory(
                &host_store.credentials,
            )?)),
            None => None,
        };
        let llm_factory: Arc<dyn LlmClientFactory> = match self.llm {
            LlmSource::Factory(factory) => factory,
            LlmSource::Client(client) => Arc::new(ReusableClientFactory(client.into_reusable())),
            LlmSource::SharedProvider(provider) => Arc::new(ReusableClientFactory(
                LlmClient::from_shared_provider(provider).into_reusable(),
            )),
            LlmSource::IsolatedHostStore => {
                let host_store = host_store_paths.context(
                    "LlmSource::IsolatedHostStore requires StorageMode::Isolated with host_store_root",
                )?;
                Arc::new(ProductLlmClientFactory::new(
                    host_store.credentials,
                    Some(host_store.managed_auth),
                ))
            }
            LlmSource::SharedProductCredentials => {
                let product_system_store = SystemStorePaths::detect()?;
                let product_host_store = HostStorePaths::detect()?;
                connection_store = Some(product_system_store.connection_bindings()?);
                tool_secret_resolver = Some(Arc::new(crate::SecretStore::from_directory(
                    &product_host_store.credentials,
                )?));
                Arc::new(ProductLlmClientFactory::new(
                    product_host_store.credentials,
                    Some(product_host_store.managed_auth),
                ))
            }
        };

        let sm_config = ServiceManagerConfig {
            process: self.process,
            resume_root: self.resume_root,
            resume_rollout_id: None,
            ephemeral_rollout: false,
            no_resume_update: false,
            input_shadow: None,
            launch_context,
            connection_store,
            package_store,
            in_memory_stores,
            llm_factory,
            host_mount_adapter: self.host_mount_adapter,
            tools: self.tools,
            include_builtin_skills: self.builtin_skills,
            root_package_bindings: Vec::new(),
            explicit_root_package_bindings: Vec::new(),
            tool_secret_resolver,
        };
        Ok((sm_config, self.host_mounts_enabled))
    }
}
