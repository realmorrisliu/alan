//! Platform inputs for booting Service Manager.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use alan_agent_engine::{
    AgentProcessConfig, Config, LlmClient, MemoryDomain, ProcessDescriptor, ProcessFileTree,
    ToolRegistry,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, Credentials, Namespace};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use alan_service_manager::{
    ConnectionsFile, LlmClientFactory, ProcessLaunchContext, ServiceManager, ServiceManagerConfig,
};
use anyhow::{Context, Result, bail};

use crate::paths::{HostStorePaths, SystemStorePaths};
use crate::{SecretStore, apply_profile_to_config};

/// Host-supplied adapters and durable bindings needed by Service Manager.
pub struct HostBootConfig {
    service_manager: ServiceManagerConfig,
    /// Keep this invocation's Root rollout out of `metadata/root-rollout`.
    pub ephemeral_rollout: bool,
    /// Skip writing `metadata/root-rollout` while keeping normal memory, context, and `--resume`.
    pub no_resume_update: bool,
    saved_grants: Vec<String>,
    saved_grant_store_path: Option<std::path::PathBuf>,
    cow_runtime: Option<crate::CowHostRuntime>,
    reified_mounts: Option<crate::ReifiedMountManager>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProductLlmClientFactory {
    credentials_dir: std::path::PathBuf,
    managed_auth: Option<std::path::PathBuf>,
    _installation_access: Option<Arc<crate::installation::InstallationAccess>>,
}

impl ProductLlmClientFactory {
    pub(crate) fn new(
        credentials_dir: std::path::PathBuf,
        managed_auth: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            credentials_dir,
            managed_auth,
            _installation_access: None,
        }
    }
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

impl ProductLlmClientFactory {
    pub(crate) fn resolve(
        &self,
        base_config: &Config,
        selected_profile: Option<&str>,
        connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        let Some(selected_profile) = selected_profile else {
            return Ok(LlmClient::new(UnconfiguredLlmProvider));
        };
        let mut core_config = base_config.clone();
        let secret_store = SecretStore::from_directory(&self.credentials_dir)?;
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

impl LlmClientFactory for ProductLlmClientFactory {
    fn create(
        &self,
        base_config: &Config,
        selected_profile: Option<&str>,
        connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        let client = self.resolve(base_config, selected_profile, connections)?;
        let Some(profile_id) = selected_profile else {
            return Ok(client);
        };
        let resolved = connections.resolve_profile(Some(profile_id))?;
        if resolved.credential_kind != alan_service_manager::CredentialKind::SecretString {
            return Ok(client);
        }
        let credential_id = resolved.credential_id;
        let initial_secret = match credential_id.as_deref() {
            Some(id) => SecretStore::from_directory(&self.credentials_dir)?.load(id)?,
            None => None,
        };
        let provider_name = client.provider_name();
        let generation = client.supports_generation();
        let evaluation = client.supports_choice_evaluation();
        Ok(LlmClient::new(LiveSecretProvider {
            factory: self.clone(),
            base_config: base_config.clone(),
            profile_id: profile_id.to_string(),
            credential_id,
            connections: connections.clone(),
            provider_name,
            generation,
            evaluation,
            cached: Some(CachedSecretClient {
                secret: initial_secret,
                client,
            }),
        }))
    }
}

struct CachedSecretClient {
    secret: Option<String>,
    client: LlmClient,
}

// Keep Process profile settings fixed while checking the owning Host credential store at
// each request boundary. Reuse the built `LlmClient` (and its underlying HTTP client/pool)
// across requests while the resolved secret is unchanged; invalidate immediately on rotation
// or revocation. In-flight requests retain their original authorization.
struct LiveSecretProvider {
    factory: ProductLlmClientFactory,
    base_config: Config,
    profile_id: String,
    credential_id: Option<String>,
    connections: ConnectionsFile,
    provider_name: &'static str,
    generation: bool,
    evaluation: bool,
    cached: Option<CachedSecretClient>,
}

impl LiveSecretProvider {
    fn client_mut(&mut self) -> Result<&mut LlmClient> {
        let current_secret = match self.credential_id.as_deref() {
            Some(id) => SecretStore::from_directory(&self.factory.credentials_dir)?.load(id)?,
            None => None,
        };
        let cache_hit = self
            .cached
            .as_ref()
            .is_some_and(|cached| cached.secret == current_secret);
        if !cache_hit {
            self.cached = None;
            let client = self.factory.resolve(
                &self.base_config,
                Some(&self.profile_id),
                &self.connections,
            )?;
            self.cached = Some(CachedSecretClient {
                secret: current_secret,
                client,
            });
        }
        Ok(&mut self
            .cached
            .as_mut()
            .expect("cached LLM client initialized")
            .client)
    }
}

#[async_trait::async_trait]
impl LlmProvider for LiveSecretProvider {
    fn supports_generation(&self) -> bool {
        self.generation
    }
    fn supports_choice_evaluation(&self) -> bool {
        self.evaluation
    }
    async fn evaluate_choice(
        &mut self,
        request: alan_llm::ChoiceEvaluationRequest,
    ) -> Result<alan_llm::ChoiceEvaluationResponse> {
        self.client_mut()?.evaluate_choice(request).await
    }

    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.client_mut()?.generate(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.client_mut()?.chat(system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.client_mut()?.generate_stream(request).await
    }

    fn provider_name(&self) -> &'static str {
        self.provider_name
    }

    fn account_identity(&self) -> Option<&str> {
        self.cached
            .as_ref()
            .and_then(|cached| cached.client.account_identity())
    }

    async fn model_catalog(&self) -> Result<Option<Vec<alan_llm::ProviderModel>>> {
        let current_secret = match self.credential_id.as_deref() {
            Some(id) => SecretStore::from_directory(&self.factory.credentials_dir)?.load(id)?,
            None => None,
        };
        if let Some(cached) = self.cached.as_ref()
            && cached.secret == current_secret
        {
            return cached.client.model_catalog().await;
        }
        let client =
            self.factory
                .resolve(&self.base_config, Some(&self.profile_id), &self.connections)?;
        client.model_catalog().await
    }
}

impl HostBootConfig {
    /// Build product inputs from canonical stores and native adapters.
    pub fn product() -> Result<Self> {
        Self::product_with_root_resume(false)
    }

    /// Build product inputs and optionally restore the selected Root Agent rollout.
    pub fn product_with_root_resume(resume_root: bool) -> Result<Self> {
        crate::installation::validate_current_invocation()?;
        let installation_access =
            Arc::new(crate::installation::InstallationPaths::detect()?.access()?);
        let system_store = SystemStorePaths::detect()?;
        let host_store = HostStorePaths::detect()?;
        let namespace = Namespace::new();
        let mut process = AgentProcessConfig::from(Config::load_with_metadata()?);
        let mut launch_context =
            prepare_system_store_launch(&system_store, namespace, &mut process, None)?;
        let instance_id = format!("alan-{}", std::process::id());
        let (cow_manager, _gc_report) = crate::CowWorkspaceManager::new(
            crate::CowWorkspaceConfig::new(system_store.clone(), instance_id.clone())?,
        )?;
        let (reified_mounts, _reified_gc_report) =
            crate::ReifiedMountManager::new(system_store.clone(), instance_id)?;
        let cow_runtime = crate::CowHostRuntime::new(cow_manager);
        launch_context.namespace.mount(
            "/mnt/branch",
            InProcessTransport::new(cow_runtime.branch_fs().clone()),
            Access::ReadWrite,
        );
        let connection_store = system_store.connection_bindings()?;
        let packages_dir = system_store.packages()?;
        let root_package_bindings =
            alan_service_manager::PackageService::load_root_bindings_from_store(&packages_dir)?;
        let tools = product_tool_registry(Arc::new(process.agent_config.core_config.clone()));
        let secret_store = SecretStore::from_directory(&host_store.credentials)?;
        let llm_factory = Arc::new(ProductLlmClientFactory {
            credentials_dir: host_store.credentials.clone(),
            managed_auth: Some(host_store.managed_auth),
            _installation_access: Some(installation_access),
        });

        Ok(Self {
            service_manager: ServiceManagerConfig {
                process,
                resume_root,
                resume_rollout_id: None,
                ephemeral_rollout: false,
                no_resume_update: false,
                input_shadow: None,
                launch_context,
                connection_store: Some(connection_store),
                package_store: Some(packages_dir),
                in_memory_stores: false,
                llm_factory,
                host_mount_adapter: Arc::new(crate::host_mounts::NativeHostMountExportAdapter),
                tools,
                include_builtin_skills: true,
                root_package_bindings,
                explicit_root_package_bindings: Vec::new(),
                tool_secret_resolver: Some(Arc::new(secret_store)),
                workspace_isolation_port: Some(cow_runtime.isolation_adapter().clone()),
                boot_manifest: None,
            },
            ephemeral_rollout: false,
            no_resume_update: false,
            saved_grants: Vec::new(),
            saved_grant_store_path: None,
            cow_runtime: Some(cow_runtime),
            reified_mounts: Some(reified_mounts),
        })
    }

    /// Explicit test-only inputs. Product callers never select this implicitly.
    pub fn ephemeral(
        process: AgentProcessConfig,
        llm_client: LlmClient,
        tools: ToolRegistry,
    ) -> Self {
        let mut launch_context = ProcessLaunchContext::root();
        launch_context.namespace.mount(
            "/mnt/branch",
            InProcessTransport::new(Arc::new(alan_branchfs::BranchFs::new())),
            Access::ReadWrite,
        );
        let mut config =
            ServiceManagerConfig::ephemeral(process, launch_context, llm_client, tools);
        config.host_mount_adapter = Arc::new(crate::host_mounts::NativeHostMountExportAdapter);
        Self {
            service_manager: config,
            ephemeral_rollout: false,
            no_resume_update: false,
            saved_grants: Vec::new(),
            saved_grant_store_path: None,
            cow_runtime: None,
            reified_mounts: None,
        }
    }

    /// Attach a [`crate::CowWorkspaceManager`] to this host boot configuration, binding
    /// `/mnt/branch` to `CowBranchWorkspaceAdapter` and enabling exit `.patch` flush.
    pub fn with_cow_workspace_manager(mut self, manager: crate::CowWorkspaceManager) -> Self {
        let runtime = crate::CowHostRuntime::new(manager);
        self.service_manager.launch_context.namespace.mount(
            "/mnt/branch",
            InProcessTransport::new(runtime.branch_fs().clone()),
            Access::ReadWrite,
        );
        self.service_manager.workspace_isolation_port = Some(runtime.isolation_adapter().clone());
        self.cow_runtime = Some(runtime);
        self
    }

    /// Inspect the attached CoW host runtime, if configured.
    pub fn cow_runtime(&self) -> Option<&crate::CowHostRuntime> {
        self.cow_runtime.as_ref()
    }

    /// Attach a [`crate::ReifiedMountManager`] to this host boot configuration.
    pub fn with_reified_mount_manager(mut self, manager: crate::ReifiedMountManager) -> Self {
        self.reified_mounts = Some(manager);
        self
    }

    /// Inspect the attached reified NFSv3 mount manager, if configured.
    pub fn reified_mounts(&self) -> Option<&crate::ReifiedMountManager> {
        self.reified_mounts.as_ref()
    }

    /// Configure explicit invocation-only trust posture (`Governed`, `ConfinedTrusted`,
    /// or `HostAccountTrusted`). Never read from repo configuration files.
    pub fn with_trust_posture(mut self, trust_posture: alan_agent_engine::TrustPosture) -> Self {
        self.service_manager.process.trust_posture = trust_posture;
        self
    }

    /// Active explicit invocation trust posture.
    pub fn trust_posture(&self) -> alan_agent_engine::TrustPosture {
        self.service_manager.process.trust_posture
    }

    /// Configure a dedicated Connection profile for Guardian escalation reviews.
    pub fn with_guardian_profile(mut self, profile: impl Into<String>) -> Result<Self> {
        let profile = profile.into();
        anyhow::ensure!(
            !profile.trim().is_empty(),
            "guardian profile must not be empty"
        );
        self.service_manager
            .process
            .agent_config
            .set_guardian_profile_override(Some(profile));
        Ok(self)
    }

    /// Configure a dedicated Connection profile for Smart Approval (`--smart`) evaluation.
    pub fn with_smart_evaluator_profile(mut self, profile: impl Into<String>) -> Result<Self> {
        let profile = profile.into();
        anyhow::ensure!(
            !profile.trim().is_empty(),
            "smart evaluator profile must not be empty"
        );
        self.service_manager
            .process
            .agent_config
            .set_smart_evaluator_profile_override(Some(profile));
        Ok(self)
    }

    /// Explicitly restore a specific Root Agent rollout id (`--resume <id>`).
    pub fn with_root_resume_id(mut self, rollout_id: Option<String>) -> Self {
        self.service_manager = self.service_manager.with_resume_rollout_id(rollout_id);
        self
    }

    /// Activate saved System Store Host Mount grant ids onto the Root Agent at boot.
    pub fn with_saved_grants(mut self, grant_ids: Vec<String>) -> Self {
        self.saved_grants = grant_ids;
        self
    }

    /// Override the saved-grant store path (intended for isolated host/CLI tests).
    pub fn with_saved_grant_store_path(mut self, path: std::path::PathBuf) -> Self {
        self.saved_grant_store_path = Some(path);
        self
    }

    /// Explicitly restore the selected Root Agent rollout for this invocation.
    pub fn with_root_resume(mut self) -> Self {
        self.service_manager.resume_root = true;
        self
    }

    /// Keep this invocation's Root rollout out of `metadata/root-rollout`: the run neither reads
    /// the selected rollout nor becomes the selection for earlier or later invocations.
    pub fn with_ephemeral_rollout(mut self, ephemeral: bool) -> Self {
        self.ephemeral_rollout = ephemeral;
        self.service_manager.ephemeral_rollout = ephemeral;
        self.service_manager
            .process
            .set_ephemeral_context(ephemeral);
        self
    }

    /// Whether this invocation is excluded from the selected-rollout channel.
    pub fn ephemeral_rollout(&self) -> bool {
        self.ephemeral_rollout
    }

    /// Skip writing `metadata/root-rollout` for this invocation while preserving personal memory,
    /// project startup context, and optional `--resume` recovery.
    pub fn with_no_resume_update(mut self, no_resume_update: bool) -> Self {
        self.no_resume_update = no_resume_update;
        self.service_manager.no_resume_update = no_resume_update;
        self
    }

    /// Whether this invocation skips updating `metadata/root-rollout`.
    pub fn no_resume_update(&self) -> bool {
        self.no_resume_update
    }

    /// Attach read-only Project Ledger and unread workflow inbox context for Root Agent startup.
    ///
    /// Ignored when `ephemeral_rollout` is enabled so `--ephemeral` sessions stay completely clean.
    pub fn with_project_startup_context(
        mut self,
        context: Option<alan_agent_engine::ProjectStartupContext>,
    ) -> Self {
        let effective = if self.ephemeral_rollout {
            None
        } else {
            context
        };
        self.service_manager
            .process
            .set_project_startup_context(effective);
        self
    }

    /// Read-only Project Ledger and inbox context currently attached to Root Agent startup.
    pub fn project_startup_context(&self) -> Option<&alan_agent_engine::ProjectStartupContext> {
        self.service_manager
            .process
            .project_startup_context
            .as_ref()
    }

    /// Whether Root Agent startup skips Project Ledger and continuity context.
    pub fn ephemeral_context(&self) -> bool {
        self.service_manager.process.ephemeral_context
    }

    /// Select an advice-only evaluator for this invocation's Root Agent input.
    pub fn with_input_shadow(
        mut self,
        profile: impl Into<String>,
        surface: alan_agent_engine::runtime::EvaluationSurface,
    ) -> Self {
        self.service_manager.input_shadow = Some(alan_service_manager::InputShadowSelection {
            profile: profile.into(),
            surface,
        });
        self
    }

    /// Replace durable Root package bindings (for example cleared on `--ephemeral`).
    pub fn with_root_package_bindings(
        mut self,
        bindings: Vec<alan_service_manager::RootPackageBinding>,
    ) -> Self {
        self.service_manager.root_package_bindings = bindings;
        self
    }

    /// Add fail-closed per-invocation Root package bindings (`--package` / `--fileserver`).
    pub fn with_explicit_root_package_bindings(
        mut self,
        bindings: Vec<alan_service_manager::RootPackageBinding>,
    ) -> Self {
        self.service_manager.explicit_root_package_bindings = bindings;
        self
    }

    /// Attach an empty in-memory Root Agent Definition descriptor at `/lib/agents/root`
    /// for ephemeral test hosts that exercise child Agent Process spawning.
    pub fn with_default_root_agent_definition(mut self) -> Result<Self> {
        let tree = ProcessFileTree::new(BTreeMap::from([("agent.toml".into(), Vec::new())]))?;
        self.service_manager.launch_context =
            self.service_manager.launch_context.clone().with_descriptor(
                alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::with_file_tree("/lib/agents/root", tree)?,
            );
        Ok(self)
    }

    /// Boot an independent in-process instance without a listener or background launcher.
    /// The caller owns the returned manager and must shut it down before exiting.
    pub async fn boot_foreground(self) -> Result<ServiceManager> {
        self.boot_service_manager().await
    }

    pub(crate) async fn boot_service_manager(self) -> Result<ServiceManager> {
        let (manager, _, _) = self.boot_service_manager_with_cow().await?;
        Ok(manager)
    }

    pub(crate) async fn boot_service_manager_with_cow(
        mut self,
    ) -> Result<(
        ServiceManager,
        Option<crate::CowHostRuntime>,
        Option<crate::ReifiedMountManager>,
    )> {
        self.service_manager.ephemeral_rollout = self.ephemeral_rollout;
        self.service_manager.no_resume_update = self.no_resume_update;
        let saved_grants = std::mem::take(&mut self.saved_grants);
        let store_path = self.saved_grant_store_path.take();
        let cow_runtime = self.cow_runtime.take();
        let reified_mounts = self.reified_mounts.take();
        let manager = ServiceManager::boot(self.service_manager).await?;
        if let Some(runtime) = cow_runtime.as_ref() {
            runtime.attach_host_mount_and_root_pid(manager.host_mount(), manager.root_pid_atomic());
            runtime.ensure_base_branch().await?;
        }
        if !saved_grants.is_empty() {
            let store = match store_path {
                Some(path) => {
                    crate::host_mounts::saved_grants::SavedProjectGrantStore::from_path(path)
                }
                None => match crate::host_mounts::saved_grants::SavedProjectGrantStore::system() {
                    Ok(store) => store,
                    Err(err) => {
                        let _ = manager.shutdown().await;
                        return Err(err);
                    }
                },
            };
            let root_pid = manager.root_pid();
            let host_mount = manager.host_mount();
            for grant_id in &saved_grants {
                if let Err(err) = store.activate_grant_for_process(grant_id, &host_mount, root_pid)
                {
                    let _ = manager.shutdown().await;
                    return Err(err);
                }
            }
            if let Some(runtime) = cow_runtime.as_ref()
                && let Some(first_grant) = saved_grants.first()
            {
                runtime
                    .branch_adapter()
                    .set_default_source_grant(Some(first_grant.clone()));
            }
        }
        Ok((manager, cow_runtime, reified_mounts))
    }
}

pub(crate) fn prepare_system_store_launch(
    system_store: &SystemStorePaths,
    namespace: Namespace,
    process: &mut AgentProcessConfig,
    definition_override: Option<ProcessFileTree>,
) -> Result<ProcessLaunchContext> {
    prepare_system_store_launch_for_domain(
        system_store,
        namespace,
        process,
        definition_override,
        &MemoryDomain::Default,
    )
}

/// Bind an explicit domain-partitioned Memory Store (`default`, `personal`, `continuity`,
/// `app:<id>`, `project:<slug>`) into `/memory` for one System Store launch.
pub(crate) fn prepare_system_store_launch_for_domain(
    system_store: &SystemStorePaths,
    mut namespace: Namespace,
    process: &mut AgentProcessConfig,
    definition_override: Option<ProcessFileTree>,
    memory_domain: &MemoryDomain,
) -> Result<ProcessLaunchContext> {
    let memory_store_backing = system_store.memory_store_for_domain(memory_domain)?;
    std::fs::create_dir_all(&memory_store_backing)
        .context("failed to prepare Memory Store backing")?;
    let memory_store =
        alan_hostfs::HostDirFs::new(&memory_store_backing, alan_hostfs::HostDirAccess::ReadWrite)
            .context("failed to open Memory Store backing")?;
    namespace.mount(
        "/memory",
        InProcessTransport::new(Arc::new(memory_store)),
        Access::ReadWrite,
    );

    let root_definition_tree = match definition_override {
        Some(tree) => {
            if namespace.resolve("/lib/agents/root").is_err() {
                namespace.mount(
                    "/lib/agents/root",
                    InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                    Access::ReadOnly,
                );
            }
            tree
        }
        None => {
            let root_definition = system_store.agent_definitions()?.join("root");
            std::fs::create_dir_all(&root_definition)
                .context("failed to prepare system Root Agent Definition")?;
            let root_definition_fs =
                alan_hostfs::HostDirFs::new(&root_definition, alan_hostfs::HostDirAccess::ReadOnly)
                    .context("failed to open system Root Agent Definition")?;
            let tree = snapshot_agent_definition(&root_definition)?;
            namespace.mount(
                "/lib/agents/root",
                InProcessTransport::new(Arc::new(root_definition_fs)),
                Access::ReadOnly,
            );
            tree
        }
    };

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
    Ok(launch_context)
}

fn product_tool_registry(core_config: Arc<Config>) -> ToolRegistry {
    let mut tools = ToolRegistry::with_config(core_config);
    alan_tools::register_builtin_tool_catalog(&mut tools);
    for tool in alan_tools::create_all_tools() {
        tools.register_boxed(tool);
    }
    tools
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
    #[tokio::test]
    async fn foreground_instances_have_independent_lifetimes() {
        use alan_ap::InProcessTransport;
        use alan_shell::Shell;
        let boot = || {
            super::HostBootConfig::ephemeral(
                alan_agent_engine::AgentProcessConfig::default(),
                alan_agent_engine::LlmClient::new(alan_llm::MockLlmProvider::new()),
                alan_agent_engine::ToolRegistry::new(),
            )
            .boot_foreground()
        };
        let first = boot().await.unwrap();
        let second = boot().await.unwrap();
        assert_ne!(first.boot_id(), second.boot_id());
        let first_shell = Shell::new(InProcessTransport::new(
            first.local_entry().namespace_for_local_client(),
        ));
        let second_shell = Shell::new(InProcessTransport::new(
            second.local_entry().namespace_for_local_client(),
        ));
        first_shell.ls("/agent/root").await.unwrap();
        second_shell.ls("/agent/root").await.unwrap();
        first.shutdown().await.unwrap();
        assert!(first_shell.ls("/agent/root").await.is_err());
        second_shell.ls("/agent/root").await.unwrap();
        second.shutdown().await.unwrap();
        assert!(second_shell.ls("/agent/root").await.is_err());
    }

    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn product_root_agent_has_all_seven_builtin_tools() {
        let tools = product_tool_registry(Arc::new(Config::default()));
        let names = tools
            .list_tools()
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();

        let expected: BTreeSet<String> = [
            "bash",
            "edit_file",
            "glob",
            "grep",
            "list_dir",
            "read_file",
            "write_file",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(names, expected);
        for tool in &expected {
            assert!(tools.has_tool_factory(tool), "missing Tool factory: {tool}");
            assert!(tools.has(tool), "missing registered Tool: {tool}");
        }
    }

    #[tokio::test]
    async fn missing_connection_keeps_host_bootable_but_generation_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let factory = ProductLlmClientFactory {
            credentials_dir: temp.path().to_path_buf(),
            managed_auth: None,
            _installation_access: None,
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

#[cfg(test)]
#[path = "boot/live_credential_tests.rs"]
mod live_credential_tests;
