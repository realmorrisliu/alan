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
pub use agent_machine::task_budget::{TaskBudgetBinding, TaskBudgetLimits};
pub use agent_machine::{
    ROLLBACK_NON_DURABLE_WARNING, latest_compaction_attempt_from_rollout_items,
    latest_memory_flush_attempt_from_rollout_items,
};
pub use alan_agent_protocol::{
    AGENT_DEFINITION_DESCRIPTOR as AGENT_DEFINITION_FD,
    AGENT_DEFINITION_DESCRIPTOR_NAME as AGENT_DEFINITION_DESCRIPTOR, AgentExecutablePause,
    AgentExecutableRequest, AgentExecutableResult, AgentExecutableStatus, ContentPart, InputIntent,
    InputMode, MEMORY_STORE_DESCRIPTOR as MEMORY_STORE_FD, Op, OwnerWorkControl, OwnerWorkRequest,
    SpawnHandle, SpawnHostMount, SpawnMountAccess, SpawnTarget, Submission, UiActivitySnapshot,
    UiActivityState, UiNoticeKind, UiNoticeSnapshot, UserInputRecord, YieldKind,
};
pub use alan_llm::SharedLlmProvider;
pub use config::{
    Config, ConfigSourceKind, LlmProvider, LoadedConfig, PartialStreamRecoveryMode, StreamingMode,
};
pub use evidence::redact_secret_tokens;
pub use file_tree::ProcessFileTree;
pub use llm::{
    CompatibilityTier, GenerationRequest, GenerationResponse, InstructionRole, LlmClient,
    ProviderCapabilities, ReusableLlmClient, TokenUsage, ToolCall, ToolDefinition,
};
pub use models::{ModelCatalog, ModelCatalogProvider, ModelInfo};
pub use policy::{
    GitPushDelegationScope, PolicyAction, PolicyDecision, PolicyEngine, PolicyRule, TrustPosture,
    TrustPostureError,
};
pub use process_launch::{
    AgentRuntimeStoreBindings, MEMORY_STORE_DESCRIPTOR, ProcessDescriptor, ProcessPackageKind,
    ProcessPackageReference, ProcessPackageSkillReference,
};
pub use prompts::{PromptLoader, SystemPromptMode};
pub use request_controls::{
    RequestControlDiagnostic, RequestControlIntent, RequestControlResolutionInput,
    RequestControlSource, ResolvedRequestControls, provider_capabilities_for_config,
    resolve_request_controls, resolve_runtime_request_controls, resolve_turn_request_controls,
};
pub use rollout::{
    AgentMachineMeta, CheckpointRecord, EffectRecord, EffectStatus, EventRecord, MessageRecord,
    RolloutItem, RolloutRecorder, ToolCallRecord, process_storage_key,
};
pub use runtime::{
    AgentConfig, AgentProcessConfig, CronStreamPolicy, DomainRecallScope, HookDecision,
    InboxDigestEntry, InboxDigestStatus, MemoryDomain, MemorySearchHit, ProjectBinding,
    ProjectContinuitySummary, ProjectContinuitySummaryView, ProjectLedger, ProjectStartupContext,
    ProjectSummaryDeleteOutcome, ProjectSummaryEdit, ProjectSummaryOrigin,
    ProjectWorkstreamStoreRef, RuntimeController, RuntimeHandle, ToolPolicyHook, WorkstreamCounts,
    WorkstreamPersistence, WorkstreamRecord, WorkstreamState, WorkstreamWriteMode,
    archive_workstream, bind_project_dir, binding_path, build_turn_recall_bundle_for_domains,
    complete_workstream, delete_project_summary, derive_project_key, edit_project_summary,
    format_project_ledger_and_inbox_summary, idle_workstream, inbox_dir, inbox_entry_path,
    inspect_project_summary, ledger_path, list_existing_memory_domains, list_inbox_digests,
    list_project_bindings, list_unread_inbox_digests, load_inbox_digest, load_project_binding,
    load_project_ledger, load_workstream_record, mark_all_inbox_digests_digested,
    mark_inbox_digest_digested, pause_workstream_error, project_dir,
    render_project_startup_context, resolve_bound_project, resolve_domain_store_dir,
    resume_workstream, scrub_inbox_summary, search_memory_domains, search_memory_store,
    spawn_with_namespace_environment, stream_path, streams_dir, upsert_workstream,
    validate_domain_slug, validate_project_key, validate_response_schema, workstream_counts,
    write_inbox_digest,
};
pub use tools::ToolRegistry;
