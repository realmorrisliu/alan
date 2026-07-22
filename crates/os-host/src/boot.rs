//! Platform inputs for booting Service Manager.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use alan_agent_engine::{
    AgentProcessConfig, Config, InstallChannel, LlmClient, ProcessDescriptor, ProcessFileTree,
    ToolRegistry,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, Credentials, Namespace};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use alan_service_manager::{
    ConnectionsFile, LlmClientFactory, ProcessLaunchContext, ServiceManagerConfig,
};
use anyhow::{Context, Result, bail};

use crate::paths::{HostStorePaths, SystemStorePaths};
use crate::{
    LegacyConnectionPaths, SecretStore, apply_profile_to_config, migrate_legacy_connections,
};

/// Host-supplied adapters and durable bindings needed by Service Manager.
pub struct HostBootConfig(ServiceManagerConfig);

#[derive(Debug)]
struct ProductLlmClientFactory {
    credentials_dir: std::path::PathBuf,
    keychain_service: Option<String>,
    managed_auth: Option<std::path::PathBuf>,
}

#[cfg(target_os = "macos")]
fn load_macos_keychain_secret(service: &str, credential_id: &str) -> Result<Option<String>> {
    let output = std::process::Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            service,
            "-a",
            credential_id,
            "-w",
        ])
        .output()
        .context("read macOS Keychain credential")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        if detail.contains("could not be found") {
            return Ok(None);
        }
        bail!(
            "macOS Keychain rejected credential lookup: {}",
            detail.trim()
        );
    }
    let secret = String::from_utf8(output.stdout)
        .context("macOS Keychain credential is not UTF-8")?
        .trim_end_matches(['\r', '\n'])
        .to_string();
    Ok((!secret.is_empty()).then_some(secret))
}

#[cfg(not(target_os = "macos"))]
fn load_macos_keychain_secret(_service: &str, _credential_id: &str) -> Result<Option<String>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
fn macos_keychain_service(channel_id: &str) -> String {
    format!("app.alanworks.macos.{channel_id}.connections")
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
        let resolved = connections.resolve_profile(Some(selected_profile))?;
        let secret_store = match (
            self.keychain_service.as_deref(),
            resolved.credential_id.as_deref(),
        ) {
            (Some(service), Some(credential_id)) => {
                match load_macos_keychain_secret(service, credential_id)? {
                    Some(secret) => SecretStore::with_resolved_secret(
                        &self.credentials_dir,
                        credential_id,
                        secret,
                    )?,
                    None => SecretStore::from_directory(&self.credentials_dir)?,
                }
            }
            _ => SecretStore::from_directory(&self.credentials_dir)?,
        };
        apply_profile_to_config(
            connections,
            Some(selected_profile),
            &secret_store,
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
        let root_definition_tree = snapshot_agent_definition(&root_definition)?;
        namespace.mount(
            "/lib/agents/root",
            InProcessTransport::new(Arc::new(root_definition_fs)),
            Access::ReadOnly,
        );

        let mut process = AgentProcessConfig::from(Config::load_with_metadata()?);
        let launch_context = ProcessLaunchContext::new(namespace, Credentials::system(), "/")?
            .with_descriptor(
                alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::with_file_tree("/lib/agents/root", root_definition_tree)?,
            )
            .with_descriptor(
                alan_agent_engine::MEMORY_STORE_DESCRIPTOR,
                ProcessDescriptor::new("/memory")?,
            );
        process.store_bindings = Some(system_store.agent_runtime_bindings()?);
        process.memory_store_backing = Some(memory_store_backing);
        let connection_store = system_store.connection_bindings()?;
        let tools = ToolRegistry::with_config(Arc::new(process.agent_config.core_config.clone()));
        let llm_factory = Arc::new(ProductLlmClientFactory {
            credentials_dir: host_store.credentials.clone(),
            keychain_service: {
                #[cfg(target_os = "macos")]
                {
                    Some(macos_keychain_service(channel_id))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    None
                }
            },
            managed_auth: Some(host_store.managed_auth),
        });

        Ok(Self(ServiceManagerConfig {
            channel_id: channel_id.into(),
            process,
            launch_context,
            connection_store: Some(connection_store),
            package_store: Some(system_store.packages()?),
            llm_factory,
            host_mount_adapter: Arc::new(crate::host_mounts::NativeHostMountExportAdapter),
            tools,
        }))
    }

    /// Explicit test-only inputs. Product callers never select this implicitly.
    pub fn ephemeral(
        channel_id: impl Into<String>,
        process: AgentProcessConfig,
        llm_client: LlmClient,
        tools: ToolRegistry,
    ) -> Self {
        let mut config = ServiceManagerConfig::ephemeral(
            channel_id,
            process,
            ProcessLaunchContext::root(),
            llm_client,
            tools,
        );
        config.host_mount_adapter = Arc::new(crate::host_mounts::NativeHostMountExportAdapter);
        Self(config)
    }

    pub(crate) fn into_service_manager(self) -> ServiceManagerConfig {
        self.0
    }

    pub(crate) fn channel_id(&self) -> &str {
        &self.0.channel_id
    }
}

fn snapshot_agent_definition(root: &Path) -> Result<ProcessFileTree> {
    let metadata = std::fs::symlink_metadata(root)
        .with_context(|| format!("inspect Agent Definition {}", root.display()))?;
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Agent Definition root must be a real directory"
    );
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .with_context(|| format!("read Agent Definition {}", directory.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "Agent Definition contains a symlink: {}",
                path.display()
            );
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            anyhow::ensure!(
                metadata.is_file(),
                "Agent Definition contains a special file: {}",
                path.display()
            );
            let relative = path
                .strip_prefix(root)
                .expect("Agent Definition traversal stays below its root")
                .to_str()
                .context("Agent Definition path is not UTF-8")?
                .to_string();
            files.insert(relative, std::fs::read(&path)?);
        }
    }
    ProcessFileTree::new(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_connection_keeps_host_bootable_but_generation_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let factory = ProductLlmClientFactory {
            credentials_dir: temp.path().to_path_buf(),
            keychain_service: None,
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

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_adapter_is_channel_isolated() {
        assert_eq!(
            macos_keychain_service("stable"),
            "app.alanworks.macos.stable.connections"
        );
        assert_eq!(
            macos_keychain_service("dev"),
            "app.alanworks.macos.dev.connections"
        );
    }
}
