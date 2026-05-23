//! alan Core — the AI Turing Machine runtime.
//!
//! This crate implements a generic agent runtime modeled as a Turing machine:
//! - **Tape**: `tape::Tape` — manages conversation context
//! - **State**: `Session` — holds tape, tools, skills, and runtime config
//! - **Transition**: The agent loop drives LLM generation and tool execution
//! - **Persistence**: `RolloutRecorder` — checkpoints every state transition
//!
//! The core is intentionally agnostic of LLM providers, tool implementations,
//! hosting concerns, and domain-specific behavior. It defines interfaces
//! (`Tool` trait, `ToolRegistry`) that outer crates implement.

mod agent_definition;
mod agent_root;
mod approval;
mod config;
mod connections;
mod install_channel;
mod llm;
mod models;
mod paths;
mod policy;
mod request_controls;
mod retry;
mod rollout;
mod session;
pub mod tape;

pub mod manager;
pub mod prompts;
pub mod runtime;
pub mod skills;
pub mod tools;

pub use agent_definition::ResolvedAgentDefinition;
pub use agent_root::{
    AgentRootKind, AgentRootLayout, AgentRootPaths, DEFAULT_AGENT_NAME, ResolvedAgentRoots,
    normalize_agent_name, normalize_named_agent_name, workspace_agent_root_dir,
    workspace_agent_root_dir_from_alan_dir, workspace_alan_dir, workspace_memory_dir,
    workspace_memory_dir_from_alan_dir, workspace_named_agent_root_dir, workspace_named_agents_dir,
    workspace_persona_dir, workspace_persona_dir_from_alan_dir, workspace_public_skills_dir,
    workspace_sessions_dir, workspace_sessions_dir_from_alan_dir, workspace_skills_dir,
    workspace_skills_dir_from_alan_dir,
};
pub use config::{
    Config, ConfigSourceKind, LlmProvider, LoadedConfig, PartialStreamRecoveryMode, StreamingMode,
};
pub use connections::{
    ConnectionCredential, ConnectionProfile, ConnectionsFile, CredentialKind, ProviderDescriptor,
    ResolvedConnectionProfile, SecretStore, default_credential_backend, normalize_profile_settings,
    provider_catalog, sanitize_identifier, validate_profile_settings,
};
pub use install_channel::{INSTALL_CHANNEL_ENV, InstallChannel, InstallChannelDescriptor};
pub use llm::{
    CompatibilityTier, GenerationRequest, GenerationResponse, InstructionRole, LlmClient,
    LlmProjection, ProviderCapabilities, TokenUsage, ToolCall, ToolDefinition,
};
pub use manager::{
    PersistedLlmProvider, WorkspaceConfigState, WorkspaceInfo, WorkspaceState, WorkspaceStatus,
};
pub use models::{ModelCatalog, ModelInfo};
pub use paths::AlanHomePaths;
pub use policy::{PolicyAction, PolicyDecision, PolicyEngine, PolicyProfile, PolicyRule};
pub use prompts::PromptLoader;
pub use request_controls::{
    RequestControlDiagnostic, RequestControlIntent, RequestControlResolutionInput,
    RequestControlSource, ResolvedRequestControls, provider_capabilities_for_config,
    resolve_request_controls, resolve_session_request_controls, resolve_turn_request_controls,
};
pub use rollout::{
    CheckpointRecord, EventRecord, MessageRecord, RolloutItem, RolloutRecorder, SessionMeta,
    ToolCallRecord, session_storage_key,
};
pub use runtime::{
    AgentConfig, RuntimeController, RuntimeEventEnvelope, RuntimeHandle, WorkspaceRuntimeConfig,
    spawn, spawn_with_llm_client,
};
pub use session::{
    ROLLBACK_NON_DURABLE_WARNING, Session, latest_compaction_attempt_from_rollout_items,
    latest_memory_flush_attempt_from_rollout_items,
};
pub use tools::ToolRegistry;
