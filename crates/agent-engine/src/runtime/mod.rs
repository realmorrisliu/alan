//! Agent Runtime — the execution engine for the AI Turing Machine.
//!
//! Drives the agent loop: receive input → LLM generation → tool execution → state transition.

mod agent_process;
mod child_agents;
mod child_run_termination_tool;
mod child_runs;
mod compaction;
mod controller;
mod delegated_child_run;
mod delegated_skill_evidence;
mod delegated_skill_tool;
mod delegation_capabilities;
mod engine;
mod guardian;
mod interaction_tools;
mod launch_config;
mod loop_guard;
mod memory_flush;
mod memory_promotion;
mod memory_recall;
mod memory_surfaces;
mod mount_request_tool;
mod prompt_cache;
mod response_guardrails;
mod steering_queue;
mod submission_handlers;
mod tool_authorization;
mod tool_effect_lifecycle;
mod tool_execution;
mod tool_orchestrator;
mod tool_packages;
mod tool_policy;
mod tool_presentation;
mod transition;
mod turn_driver;
mod turn_executor;
mod turn_support;
mod ui_surfaces;
mod virtual_tool;
mod virtual_tools;

pub use agent_process::{
    AgentProcessLifecycle, AssembledChildAgentProcess, ChildAgentProcessAssembler,
    ChildAgentProcessAssemblyPlan, ChildAgentProcessAssemblyRequest,
};
pub use child_runs::{
    ChildRunRecord, ChildRunRegistryError, ChildRunStatus, ChildRunTerminationMode,
    ChildRunTerminationRequest,
};
pub use controller::{
    AgentMachineDurabilityState, RuntimeController, RuntimeHandle, RuntimeStartupMetadata,
};
pub use engine::spawn_with_namespace_environment;
pub use launch_config::{
    AgentConfig, AgentProcessConfig, configure_runtime_tool_execution_binding,
    effective_core_config_for_runtime,
};
pub use transition::{
    ApprovedMountGrant, ApprovedMountGrantAccess, MountGrantApplicator,
    MountGrantApplicatorFactory, NamespaceMountApplication, NamespaceMountControl,
    NamespaceRuntimeEnvironment, NamespaceToolActionOutput, NamespaceTurnOutput,
    NamespaceTurnRuntime, NamespaceTurnRuntimeConfig,
};

// Re-export the memory-promotion job for internal runtime modules.
pub(crate) use memory_promotion::TurnMemoryPromotionJob;
pub use tool_packages::ToolPackageManifest;

/// Configuration for the agent runtime
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Maximum number of tool loops per turn (0 = unlimited)
    pub max_tool_loops: usize,
    /// Maximum consecutive repeats of the same tool call
    pub tool_repeat_limit: usize,
    /// LLM request timeout in seconds
    pub llm_request_timeout_secs: u64,
    /// Temperature for generation
    pub temperature: f32,
    /// Max tokens for generation
    pub max_tokens: u32,
    /// Whether to enable prompt snapshots for debugging
    pub prompt_snapshot_enabled: bool,
    /// Maximum characters for prompt snapshots
    pub prompt_snapshot_max_chars: usize,
    /// Context compaction trigger threshold (number of messages)
    pub compaction_trigger_messages: usize,
    /// Number of recent messages to keep after compaction
    pub compaction_keep_last: usize,
    /// Prompt context window budget used for compaction heuristics.
    pub context_window_tokens: u32,
    /// Utilization ratio at which automatic compaction should first attempt a silent flush.
    pub compaction_soft_trigger_ratio: f32,
    /// Utilization ratio at which automatic compaction becomes mandatory.
    pub compaction_hard_trigger_ratio: f32,
    /// Governance configuration for policy loading/profile selection.
    pub governance: alan_agent_protocol::GovernanceConfig,
    /// Loaded policy engine for this runtime/machine.
    pub policy_engine: crate::policy::PolicyEngine,
    /// Request-control intent carried by runtime/machine launch.
    pub request_control_intent: crate::RequestControlIntent,
    /// Streaming strategy (`auto`/`on`/`off`).
    pub streaming_mode: crate::config::StreamingMode,
    /// Recovery strategy when streaming is interrupted after visible output.
    pub partial_stream_recovery_mode: crate::config::PartialStreamRecoveryMode,
    /// Whether machine durability is required for startup.
    pub durability_required: bool,
    /// Durable service backing inherited by child Agent Processes.
    pub store_bindings: Option<crate::AgentRuntimeStoreBindings>,
    /// Memory Service backing inherited only with an explicit Memory handle.
    pub memory_store_backing: Option<std::path::PathBuf>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_tool_loops: 0, // Unlimited by default
            tool_repeat_limit: 4,
            llm_request_timeout_secs: 180,
            temperature: 0.3,
            max_tokens: 2048,
            prompt_snapshot_enabled: false,
            prompt_snapshot_max_chars: 8000,
            compaction_trigger_messages: 60,
            compaction_keep_last: 20,
            context_window_tokens: crate::config::Config::default()
                .effective_context_window_tokens(),
            compaction_soft_trigger_ratio: crate::config::Config::default()
                .effective_compaction_soft_trigger_ratio(),
            compaction_hard_trigger_ratio: crate::config::Config::default()
                .effective_compaction_hard_trigger_ratio(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            policy_engine: crate::policy::PolicyEngine::autonomous(),
            request_control_intent: crate::RequestControlIntent::default(),
            streaming_mode: crate::config::StreamingMode::Auto,
            partial_stream_recovery_mode: crate::config::PartialStreamRecoveryMode::ContinueOnce,
            durability_required: false,
            store_bindings: None,
            memory_store_backing: None,
        }
    }
}

impl From<&crate::config::Config> for RuntimeConfig {
    fn from(config: &crate::config::Config) -> Self {
        Self {
            max_tool_loops: config.max_tool_loops.unwrap_or(0),
            tool_repeat_limit: config.tool_repeat_limit,
            llm_request_timeout_secs: config.llm_request_timeout_secs as u64,
            temperature: 0.3,
            max_tokens: 2048,
            prompt_snapshot_enabled: config.prompt_snapshot_enabled,
            prompt_snapshot_max_chars: config.prompt_snapshot_max_chars,
            compaction_trigger_messages: 60,
            compaction_keep_last: 20,
            context_window_tokens: config.effective_context_window_tokens(),
            compaction_soft_trigger_ratio: config.effective_compaction_soft_trigger_ratio(),
            compaction_hard_trigger_ratio: config.effective_compaction_hard_trigger_ratio(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            policy_engine: crate::policy::PolicyEngine::autonomous(),
            request_control_intent: crate::RequestControlIntent::from_config(config),
            streaming_mode: config.streaming_mode,
            partial_stream_recovery_mode: config.partial_stream_recovery_mode,
            durability_required: config.durability.required,
            store_bindings: None,
            memory_store_backing: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert_eq!(config.max_tool_loops, 0); // Unlimited
        assert_eq!(config.tool_repeat_limit, 4);
        assert_eq!(config.llm_request_timeout_secs, 180);
        assert_eq!(config.temperature, 0.3);
        assert_eq!(config.max_tokens, 2048);
        assert!(!config.prompt_snapshot_enabled);
        assert_eq!(config.prompt_snapshot_max_chars, 8000);
        assert_eq!(config.compaction_trigger_messages, 60);
        assert_eq!(config.compaction_keep_last, 20);
        assert_eq!(config.context_window_tokens, 1_050_000);
        assert!((config.compaction_soft_trigger_ratio - 0.72).abs() < f32::EPSILON);
        assert!((config.compaction_hard_trigger_ratio - 0.8).abs() < f32::EPSILON);
        assert_eq!(config.streaming_mode, crate::config::StreamingMode::Auto);
        assert_eq!(
            config.partial_stream_recovery_mode,
            crate::config::PartialStreamRecoveryMode::ContinueOnce
        );
        assert!(!config.durability_required);
    }

    #[test]
    fn test_runtime_config_clone() {
        let config = RuntimeConfig::default();
        let cloned = config.clone();
        assert_eq!(config.max_tool_loops, cloned.max_tool_loops);
        assert_eq!(config.tool_repeat_limit, cloned.tool_repeat_limit);
    }

    #[test]
    fn test_runtime_config_debug() {
        let config = RuntimeConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("RuntimeConfig"));
        assert!(debug_str.contains("max_tool_loops"));
    }

    #[test]
    fn test_runtime_config_from_core_config() {
        let core_config = crate::config::Config::default();
        let runtime_config = RuntimeConfig::from(&core_config);

        assert_eq!(
            runtime_config.tool_repeat_limit,
            core_config.tool_repeat_limit
        );
        assert_eq!(
            runtime_config.llm_request_timeout_secs,
            core_config.llm_request_timeout_secs as u64
        );
        assert_eq!(
            runtime_config.prompt_snapshot_enabled,
            core_config.prompt_snapshot_enabled
        );
        assert_eq!(
            runtime_config.prompt_snapshot_max_chars,
            core_config.prompt_snapshot_max_chars
        );
        assert_eq!(
            runtime_config.context_window_tokens,
            core_config.effective_context_window_tokens()
        );
        assert_eq!(
            runtime_config.compaction_soft_trigger_ratio,
            core_config.effective_compaction_soft_trigger_ratio()
        );
        assert_eq!(
            runtime_config.compaction_hard_trigger_ratio,
            core_config.effective_compaction_hard_trigger_ratio()
        );
        assert_eq!(
            runtime_config.durability_required,
            core_config.durability.required
        );
    }
}
