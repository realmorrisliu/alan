//! alan Core — the AI Turing Machine runtime.
//!
//! This crate implements a generic agent runtime modeled as a Turing machine:
//! - **Tape**: `tape::Tape` — manages conversation context
//! - **State**: `AgentMachine` — holds tape, tools, skills, and runtime config
//! - **Transition**: Accepted submissions drive LLM generation and Tool execution
//! - **Persistence**: `RolloutRecorder` — checkpoints every state transition
//!
//! The core is intentionally agnostic of hosting concerns and domain-specific
//! behavior. Live generation, tools, and state flow through the agent namespace.

mod agent_definition;
mod agent_machine;
mod approval;
mod config;
mod evidence;
mod file_tree;
mod install_channel;
mod llm;
mod models;
mod policy;
mod process_launch;
mod request_controls;
mod retry;
mod rollout;
pub mod tape;

pub mod prompts;
pub mod runtime;
pub mod skills;
pub mod tools;

pub use agent_definition::ResolvedAgentDefinition;
pub use agent_machine::{
    ROLLBACK_NON_DURABLE_WARNING, latest_compaction_attempt_from_rollout_items,
    latest_memory_flush_attempt_from_rollout_items,
};
pub use alan_agent_protocol::{
    AGENT_DEFINITION_DESCRIPTOR as AGENT_DEFINITION_FD,
    AGENT_DEFINITION_DESCRIPTOR_NAME as AGENT_DEFINITION_DESCRIPTOR, AgentExecutablePause,
    AgentExecutableRequest, AgentExecutableResult, AgentExecutableStatus, ContentPart,
    MEMORY_STORE_DESCRIPTOR as MEMORY_STORE_FD, Op, SpawnHandle, SpawnHostMount, SpawnMountAccess,
    SpawnTarget, Submission, UiActivitySnapshot, UiActivityState, UiNoticeKind, UiNoticeSnapshot,
    YieldKind,
};
pub use config::{
    Config, ConfigSourceKind, LlmProvider, LoadedConfig, PartialStreamRecoveryMode, StreamingMode,
};
pub use file_tree::ProcessFileTree;
pub use install_channel::{INSTALL_CHANNEL_ENV, InstallChannel, InstallChannelDescriptor};
pub use llm::{
    CompatibilityTier, GenerationRequest, GenerationResponse, InstructionRole, LlmClient,
    ProviderCapabilities, TokenUsage, ToolCall, ToolDefinition,
};
pub use models::{ModelCatalog, ModelInfo};
pub use policy::{PolicyAction, PolicyDecision, PolicyEngine, PolicyRule};
pub use process_launch::{
    AgentRuntimeStoreBindings, MEMORY_STORE_DESCRIPTOR, ProcessDescriptor, ProcessPackageKind,
    ProcessPackageReference, ProcessPackageSkillReference,
};
pub use prompts::PromptLoader;
pub use request_controls::{
    RequestControlDiagnostic, RequestControlIntent, RequestControlResolutionInput,
    RequestControlSource, ResolvedRequestControls, provider_capabilities_for_config,
    resolve_request_controls, resolve_runtime_request_controls, resolve_turn_request_controls,
};
pub use rollout::{
    AgentMachineMeta, CheckpointRecord, EventRecord, MessageRecord, RolloutItem, RolloutRecorder,
    ToolCallRecord, process_storage_key,
};
pub use runtime::{
    AgentConfig, AgentProcessConfig, RuntimeController, RuntimeHandle,
    spawn_with_namespace_environment,
};
pub use tools::ToolRegistry;
