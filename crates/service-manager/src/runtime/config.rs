//! Service Manager configuration and LLM factory types.

use std::sync::Arc;

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry};
use alan_ap::InProcessTransport;
use alan_kernel::Access;
use anyhow::Result;

use crate::{
    BootManifest, ConnectionStoreBindings, ConnectionsFile, HostMountExportAdapter,
    ProcessLaunchContext, UnavailableHostMountExportAdapter,
};

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
    /// Optional explicit Boot Manifest override (defaults to [`BootManifest::system`]).
    pub boot_manifest: Option<BootManifest>,
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
            boot_manifest: None,
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

    /// Override the Boot Manifest used by Service Manager.
    pub fn with_boot_manifest(mut self, manifest: BootManifest) -> Self {
        self.boot_manifest = Some(manifest);
        self
    }
}
