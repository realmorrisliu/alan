//! Platform inputs for booting Service Manager.

use std::sync::Arc;

use alan_agent_engine::{
    AgentProcessConfig, Config, ConnectionsFile, HostMountGrant, InstallChannel, LlmClient,
    ProcessDescriptor, ProcessLaunchContext, SecretStore, ToolRegistry,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, Credentials, Namespace};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use alan_service_manager::{LlmClientFactory, ServiceManagerConfig};
use anyhow::{Context, Result, bail};

use crate::paths::{HostStorePaths, SystemStorePaths};
use crate::{LegacyConnectionPaths, migrate_legacy_connections};

/// Host-supplied adapters and durable bindings needed by Service Manager.
pub struct HostBootConfig(ServiceManagerConfig);

#[derive(Debug)]
struct ProductLlmClientFactory {
    credentials_dir: std::path::PathBuf,
    managed_auth: Option<std::path::PathBuf>,
}

struct UnconfiguredLlmProvider;

#[async_trait::async_trait]
impl LlmProvider for UnconfiguredLlmProvider {
    async fn generate(&mut self, _request: GenerationRequest) -> Result<GenerationResponse> {
        bail!("no Connection Service profile selected")
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> Result<String> {
        bail!("no Connection Service profile selected")
    }

    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        bail!("no Connection Service profile selected")
    }

    fn provider_name(&self) -> &'static str {
        "unconfigured"
    }
}

impl LlmClientFactory for ProductLlmClientFactory {
    fn create(
        &self,
        base_config: &Config,
        selected_profile: Option<&str>,
        connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        let Some(selected_profile) = selected_profile else {
            return Ok(LlmClient::new(UnconfiguredLlmProvider));
        };
        let mut core_config = base_config.clone();
        connections.apply_profile_to_config(
            Some(selected_profile),
            &SecretStore::from_directory(&self.credentials_dir)?,
            &mut core_config,
        )?;
        LlmClient::from_core_config_with_chatgpt_auth_storage_path(
            &core_config,
            self.managed_auth.clone(),
        )
        .context("failed to create Root Agent LLM connection")
    }
}

impl HostBootConfig {
    /// Build product inputs from the channel stores and native adapters.
    pub fn product(channel_id: &str) -> Result<Self> {
        let channel = InstallChannel::from_id(channel_id)
            .with_context(|| format!("unknown Alan OS Host channel `{channel_id}`"))?;
        let system_store = SystemStorePaths::detect(channel_id)?;
        let host_store = HostStorePaths::detect(channel_id)?;
        if let Some(legacy) = LegacyConnectionPaths::detect(channel)? {
            migrate_legacy_connections(&legacy, &system_store, &host_store)
                .context("failed to migrate legacy connections before Host boot")?;
        }
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
        let root_definition = system_store.agent_definitions()?.join("root");
        std::fs::create_dir_all(&root_definition)
            .context("failed to prepare system Root Agent Definition")?;
        let root_definition_fs =
            alan_hostfs::HostDirFs::new(&root_definition, alan_hostfs::HostDirAccess::ReadOnly)
                .context("failed to open system Root Agent Definition")?;
        namespace.mount(
            "/lib/agents/root",
            InProcessTransport::new(Arc::new(root_definition_fs)),
            Access::ReadOnly,
        );

        let mut process = AgentProcessConfig::from(Config::load_with_metadata()?);
        process.launch_context = ProcessLaunchContext::new(namespace, Credentials::system(), "/")?
            .with_host_mount(HostMountGrant::new(
                "/lib/agents/root",
                root_definition,
                Access::ReadOnly,
            )?)
            .with_descriptor(
                alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::new("/lib/agents/root")?,
            )
            .with_descriptor(
                alan_agent_engine::MEMORY_STORE_DESCRIPTOR,
                ProcessDescriptor::new("/memory")?,
            );
        process.store_bindings = Some(system_store.agent_runtime_bindings()?);
        process.memory_store_backing = Some(memory_store_backing);
        let connection_store = system_store.connection_bindings(&host_store)?;
        let tools = ToolRegistry::with_config(Arc::new(process.agent_config.core_config.clone()));
        let llm_factory = Arc::new(ProductLlmClientFactory {
            credentials_dir: connection_store.credentials_dir.clone(),
            managed_auth: Some(host_store.managed_auth),
        });

        Ok(Self(ServiceManagerConfig::with_factory(
            channel_id,
            process,
            Some(connection_store),
            llm_factory,
            Arc::new(crate::host_mounts::NativeHostMountExportAdapter),
            tools,
        )))
    }

    /// Explicit test-only inputs. Product callers never select this implicitly.
    pub fn ephemeral(
        channel_id: impl Into<String>,
        process: AgentProcessConfig,
        llm_client: LlmClient,
        tools: ToolRegistry,
    ) -> Self {
        Self(ServiceManagerConfig::ephemeral(
            channel_id, process, llm_client, tools,
        ))
    }

    pub(crate) fn into_service_manager(self) -> ServiceManagerConfig {
        self.0
    }

    pub(crate) fn channel_id(&self) -> &str {
        &self.0.channel_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_connection_keeps_host_bootable_but_generation_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let factory = ProductLlmClientFactory {
            credentials_dir: temp.path().to_path_buf(),
            managed_auth: None,
        };

        let mut client = factory
            .create(&Config::default(), None, &ConnectionsFile::default())
            .unwrap();

        assert_eq!(client.provider_name(), "unconfigured");
        assert_eq!(
            client.chat(None, "hello").await.unwrap_err().to_string(),
            "no Connection Service profile selected"
        );
    }
}
