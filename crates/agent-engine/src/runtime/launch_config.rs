//! Agent Process launch configuration and resolution.

use super::RuntimeConfig;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Agent configuration before Process-specific launch inputs are applied.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub core_config: crate::config::Config,
    pub runtime_config: RuntimeConfig,
    explicit_runtime_overrides: ExplicitRuntimeOverrides,
}

#[derive(Debug, Clone, Copy, Default)]
struct ExplicitRuntimeOverrides {
    model: bool,
    max_tool_loops: bool,
    tool_repeat_limit: bool,
    llm_request_timeout_secs: bool,
    prompt_snapshot_enabled: bool,
    prompt_snapshot_max_chars: bool,
    context_window_tokens: bool,
    compaction_soft_trigger_ratio: bool,
    compaction_hard_trigger_ratio: bool,
    request_control_intent: bool,
    streaming_mode: bool,
    partial_stream_recovery_mode: bool,
    durability_required: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        let runtime_config = RuntimeConfig::default();
        Self {
            core_config: crate::config::Config::default(),
            runtime_config,
            explicit_runtime_overrides: ExplicitRuntimeOverrides::default(),
        }
    }
}

impl From<crate::config::Config> for AgentConfig {
    fn from(config: crate::config::Config) -> Self {
        let runtime_config = RuntimeConfig::from(&config);
        Self {
            core_config: config,
            runtime_config,
            explicit_runtime_overrides: ExplicitRuntimeOverrides::default(),
        }
    }
}

impl AgentConfig {
    /// Override the effective model for this launch across agent-root overlays.
    pub fn set_model_override(&mut self, model: impl Into<String>) {
        self.core_config.set_effective_model(model);
        sync_runtime_context_window_budget(&self.core_config, &mut self.runtime_config);
        sync_runtime_request_control_intent(&self.core_config, &mut self.runtime_config);
        self.explicit_runtime_overrides.model = true;
    }

    /// Override named model reasoning effort for this launch across overlays.
    pub fn set_model_reasoning_effort_override(
        &mut self,
        model_reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) {
        self.core_config.model_reasoning_effort = model_reasoning_effort;
        self.runtime_config.request_control_intent =
            crate::RequestControlIntent::reasoning_effort(model_reasoning_effort);
        self.explicit_runtime_overrides.request_control_intent = true;
    }

    /// Override streaming mode for this runtime launch, preserving it across agent-root overlays.
    pub fn set_streaming_mode_override(&mut self, streaming_mode: crate::config::StreamingMode) {
        self.core_config.streaming_mode = streaming_mode;
        self.runtime_config.streaming_mode = streaming_mode;
        self.explicit_runtime_overrides.streaming_mode = true;
    }

    /// Override partial stream recovery mode for this launch across agent-root overlays.
    pub fn set_partial_stream_recovery_mode_override(
        &mut self,
        partial_stream_recovery_mode: crate::config::PartialStreamRecoveryMode,
    ) {
        self.core_config.partial_stream_recovery_mode = partial_stream_recovery_mode;
        self.runtime_config.partial_stream_recovery_mode = partial_stream_recovery_mode;
        self.explicit_runtime_overrides.partial_stream_recovery_mode = true;
    }

    /// Override machine durability requirement for this launch across agent-root overlays.
    pub fn set_durability_required_override(&mut self, durability_required: bool) {
        self.core_config.durability.required = durability_required;
        self.runtime_config.durability_required = durability_required;
        self.explicit_runtime_overrides.durability_required = true;
    }

    pub fn refresh_runtime_derived_fields(&mut self) {
        sync_runtime_context_window_budget(&self.core_config, &mut self.runtime_config);
        sync_runtime_request_control_intent(&self.core_config, &mut self.runtime_config);
    }

    pub fn with_definition_overlays(&self, overlay_paths: &[PathBuf]) -> Result<Self> {
        let mut merge_base_core_config = self.core_config.clone();
        if self.explicit_runtime_overrides.request_control_intent {
            merge_base_core_config.model_reasoning_effort = None;
        }

        let mut core_config = merge_base_core_config.with_definition_overlays(overlay_paths)?;
        let mut runtime_config = merge_runtime_config_from_core_overlay(
            &merge_base_core_config,
            &core_config,
            &self.runtime_config,
            self.explicit_runtime_overrides,
        );
        self.reapply_explicit_runtime_overrides(&mut core_config, &mut runtime_config);

        Ok(Self {
            core_config,
            runtime_config,
            explicit_runtime_overrides: self.explicit_runtime_overrides,
        })
    }

    pub fn with_definition_overlay_content(&self, content: &str, source: &Path) -> Result<Self> {
        let mut merge_base_core_config = self.core_config.clone();
        if self.explicit_runtime_overrides.request_control_intent {
            merge_base_core_config.model_reasoning_effort = None;
        }
        let mut core_config =
            merge_base_core_config.with_definition_overlay_content(content, source)?;
        let mut runtime_config = merge_runtime_config_from_core_overlay(
            &merge_base_core_config,
            &core_config,
            &self.runtime_config,
            self.explicit_runtime_overrides,
        );
        self.reapply_explicit_runtime_overrides(&mut core_config, &mut runtime_config);
        Ok(Self {
            core_config,
            runtime_config,
            explicit_runtime_overrides: self.explicit_runtime_overrides,
        })
    }

    fn reapply_explicit_runtime_overrides(
        &self,
        core_config: &mut crate::config::Config,
        runtime_config: &mut RuntimeConfig,
    ) {
        if self.explicit_runtime_overrides.model {
            core_config.set_effective_model(self.core_config.effective_model().to_string());
            sync_runtime_context_window_budget(core_config, runtime_config);
            sync_runtime_request_control_intent(core_config, runtime_config);
        }
        if self.explicit_runtime_overrides.request_control_intent {
            self.runtime_config
                .request_control_intent
                .apply_to_config(core_config);
            runtime_config.request_control_intent = self.runtime_config.request_control_intent;
        }
        if self.explicit_runtime_overrides.streaming_mode {
            core_config.streaming_mode = self.runtime_config.streaming_mode;
            runtime_config.streaming_mode = self.runtime_config.streaming_mode;
        }
        if self.explicit_runtime_overrides.partial_stream_recovery_mode {
            core_config.partial_stream_recovery_mode =
                self.runtime_config.partial_stream_recovery_mode;
            runtime_config.partial_stream_recovery_mode =
                self.runtime_config.partial_stream_recovery_mode;
        }
        if self.explicit_runtime_overrides.durability_required {
            core_config.durability.required = self.runtime_config.durability_required;
            runtime_config.durability_required = self.runtime_config.durability_required;
        }
    }
}

fn sync_runtime_context_window_budget(
    core_config: &crate::config::Config,
    runtime_config: &mut RuntimeConfig,
) {
    runtime_config.context_window_tokens = core_config.effective_context_window_tokens();
}

fn sync_runtime_request_control_intent(
    core_config: &crate::config::Config,
    runtime_config: &mut RuntimeConfig,
) {
    runtime_config.request_control_intent = crate::RequestControlIntent::from_config(core_config);
}

fn merge_runtime_config_from_core_overlay(
    base_core_config: &crate::config::Config,
    overlaid_core_config: &crate::config::Config,
    current_runtime_config: &RuntimeConfig,
    explicit_runtime_overrides: ExplicitRuntimeOverrides,
) -> RuntimeConfig {
    let base_runtime = RuntimeConfig::from(base_core_config);
    let overlaid_runtime = RuntimeConfig::from(overlaid_core_config);
    let mut merged_runtime = current_runtime_config.clone();

    macro_rules! sync_if_unmodified {
        ($field:ident) => {
            if !explicit_runtime_overrides.$field && merged_runtime.$field == base_runtime.$field {
                merged_runtime.$field = overlaid_runtime.$field;
            }
        };
    }

    sync_if_unmodified!(max_tool_loops);
    sync_if_unmodified!(tool_repeat_limit);
    sync_if_unmodified!(llm_request_timeout_secs);
    sync_if_unmodified!(prompt_snapshot_enabled);
    sync_if_unmodified!(prompt_snapshot_max_chars);
    sync_if_unmodified!(context_window_tokens);
    sync_if_unmodified!(compaction_soft_trigger_ratio);
    sync_if_unmodified!(compaction_hard_trigger_ratio);
    sync_if_unmodified!(request_control_intent);
    sync_if_unmodified!(streaming_mode);
    sync_if_unmodified!(partial_stream_recovery_mode);
    sync_if_unmodified!(durability_required);

    merged_runtime
}

/// Host inputs for starting one Agent Process runtime.
#[derive(Debug, Clone)]
pub struct AgentProcessConfig {
    /// Agent execution configuration.
    pub agent_config: AgentConfig,
    /// Source used before applying the explicit Agent Definition descriptor.
    pub core_config_source: crate::ConfigSourceKind,
    /// Agent Definition and immutable package capabilities resolved by Agent Runtime Service.
    pub agent_definition: crate::ResolvedAgentDefinition,
    /// Alan OS working directory selected by Agent Runtime Service.
    pub namespace_cwd: PathBuf,
    /// Whether Agent Runtime Service passed the Memory Store descriptor.
    pub memory_store_bound: bool,
    /// Durable service backing selected by the Host; never exposed as Process identity.
    pub store_bindings: Option<crate::AgentRuntimeStoreBindings>,
    /// Memory Service backing paired with the explicit Memory Store descriptor.
    pub memory_store_backing: Option<PathBuf>,
    /// Optional execution record used to recover Agent Machine state for a new Process.
    pub recovery_rollout_path: Option<PathBuf>,
}

impl Default for AgentProcessConfig {
    fn default() -> Self {
        Self {
            agent_config: AgentConfig::default(),
            core_config_source: crate::ConfigSourceKind::Default,
            agent_definition: crate::ResolvedAgentDefinition::from_process_inputs(
                None,
                &[],
                &[],
                crate::ConfigSourceKind::Default,
            )
            .expect("empty Agent Definition is valid"),
            namespace_cwd: PathBuf::from("/"),
            memory_store_bound: false,
            store_bindings: None,
            memory_store_backing: None,
            recovery_rollout_path: None,
        }
    }
}

impl From<crate::config::Config> for AgentProcessConfig {
    fn from(config: crate::config::Config) -> Self {
        Self {
            agent_config: AgentConfig::from(config),
            core_config_source: crate::ConfigSourceKind::Default,
            agent_definition: crate::ResolvedAgentDefinition::from_process_inputs(
                None,
                &[],
                &[],
                crate::ConfigSourceKind::Default,
            )
            .expect("empty Agent Definition is valid"),
            namespace_cwd: PathBuf::from("/"),
            memory_store_bound: false,
            store_bindings: None,
            memory_store_backing: None,
            recovery_rollout_path: None,
        }
    }
}

impl From<crate::LoadedConfig> for AgentProcessConfig {
    fn from(loaded: crate::LoadedConfig) -> Self {
        Self {
            agent_config: AgentConfig::from(loaded.config),
            core_config_source: loaded.source,
            agent_definition: crate::ResolvedAgentDefinition::from_process_inputs(
                None,
                &[],
                &[],
                crate::ConfigSourceKind::Default,
            )
            .expect("empty Agent Definition is valid"),
            namespace_cwd: PathBuf::from("/"),
            memory_store_bound: false,
            store_bindings: None,
            memory_store_backing: None,
            recovery_rollout_path: None,
        }
    }
}

pub(super) struct ResolvedRuntimeLaunchConfig {
    pub(super) agent_definition: crate::ResolvedAgentDefinition,
    pub(super) core_config: crate::config::Config,
    pub(super) runtime_config: RuntimeConfig,
}

impl AgentProcessConfig {
    /// Derive execution configuration for a child launch. Agent Runtime Service
    /// remains responsible for descriptors, namespace, connection, and the
    /// resolved Agent Definition.
    pub fn child_for_spawn(&self, spec: &alan_agent_protocol::SpawnSpec) -> Self {
        let mut child = self.clone();
        if !spec.has_handle(alan_agent_protocol::SpawnHandle::Memory) {
            child.agent_config.core_config.memory.store_dir = None;
            child.memory_store_backing = None;
            child.memory_store_bound = false;
        }
        if spec.has_handle(alan_agent_protocol::SpawnHandle::ApprovalScope) {
            child.agent_config.runtime_config.governance =
                self.agent_config.runtime_config.governance.clone();
        } else {
            child.agent_config.runtime_config.governance =
                alan_agent_protocol::GovernanceConfig::default();
        }
        if let Some(model) = spec.runtime_overrides.model.as_deref() {
            child.agent_config.set_model_override(model);
        }
        if let Some(effort) = spec.runtime_overrides.model_reasoning_effort {
            child
                .agent_config
                .set_model_reasoning_effort_override(Some(effort));
        }
        if let Some(policy_path) = spec.runtime_overrides.policy_path.clone() {
            child.agent_config.runtime_config.governance.policy_path = Some(policy_path);
        }
        child.namespace_cwd = spec
            .launch
            .cwd
            .clone()
            .unwrap_or_else(|| PathBuf::from("/"));
        child.recovery_rollout_path = None;
        child
    }

    pub(super) fn resolve_runtime_launch(&self) -> Result<ResolvedRuntimeLaunchConfig> {
        let agent_definition = self.agent_definition.clone();
        let agent_config = agent_definition.apply_to_agent_config(&self.agent_config)?;
        let mut core_config = agent_config.core_config;
        if let Some(memory_store) = self.memory_store_backing.as_ref() {
            anyhow::ensure!(
                self.memory_store_bound,
                "Agent Runtime Service memory backing requires a Memory Store descriptor"
            );
            core_config.memory.store_dir = Some(memory_store.clone());
        } else {
            core_config.memory.store_dir = None;
        }

        let mut runtime_config = agent_config.runtime_config;
        runtime_config.store_bindings = self.store_bindings.clone();
        runtime_config.memory_store_backing = self.memory_store_backing.clone();

        Ok(ResolvedRuntimeLaunchConfig {
            agent_definition,
            core_config,
            runtime_config,
        })
    }
}

pub fn effective_core_config_for_runtime(
    config: &AgentProcessConfig,
) -> Result<crate::config::Config> {
    let resolved = config.resolve_runtime_launch()?;
    crate::resolve_runtime_request_controls(
        &resolved.core_config,
        crate::provider_capabilities_for_config(&resolved.core_config),
        resolved.runtime_config.request_control_intent,
    )?;

    Ok(resolved.core_config)
}

#[cfg(test)]
#[path = "launch_config_tests.rs"]
mod tests;
