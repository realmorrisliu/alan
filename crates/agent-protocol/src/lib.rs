//! Protocol definitions for the alan agent.
//!
//! This crate defines the `Op` input alphabet, transition-local `Event` records,
//! and AgentFS file schemas shared by the Agent Execution Engine and hosts.

mod adaptive;
mod compaction;
mod content;
mod event;
mod host_auth;
mod memory;
mod op;
mod process;
mod reasoning;
mod spawn;
mod ui_surface;

pub use adaptive::{
    AdaptiveForm, AdaptivePresentationHint, ConfirmationYieldPayload, CustomYieldPayload,
    StructuredInputKind, StructuredInputOption, StructuredInputQuestion,
    StructuredInputYieldPayload,
};
pub use compaction::{
    AppliedCompactionOutcome, CompactionAttemptSnapshot, CompactionMode, CompactionOutcome,
    CompactionPressureLevel, CompactionReason, CompactionRequestMetadata, CompactionResult,
    CompactionSkipReason, CompactionTrigger, FailedCompactionOutcome, SkippedCompactionOutcome,
};
pub use content::{ContentPart, parts_to_text};
pub use event::{DiffHunk, DiffLine, Event, ToolDecisionAudit, ToolResultPresentation, YieldKind};
pub use host_auth::{
    AuthErrorCode, AuthErrorResponse, AuthEvent, AuthEventEnvelope, AuthLoginMethod,
    AuthPendingLoginSummary, AuthProviderId, AuthStatusKind, AuthStatusSnapshot,
};
pub use memory::{MemoryFlushAttemptSnapshot, MemoryFlushResult, MemoryFlushSkipReason};
pub use op::{
    GovernanceConfig, GovernanceProfile, InputMode, Op, PlanItem, PlanItemStatus, Submission,
    ToolCapability, TurnContext,
};
pub use process::{
    AGENT_DEFINITION_DESCRIPTOR, AGENT_EXECUTABLE_RESULT_VERSION, AgentExecutablePause,
    AgentExecutableRequest, AgentExecutableResult, AgentExecutableStatus, MEMORY_STORE_DESCRIPTOR,
    ProcessExecSpec, ProcessNamespaceAccess, ProcessNamespaceManifest, ProcessNamespaceMount,
};
pub use reasoning::{ReasoningControls, ReasoningEffort};
pub use spawn::{
    AGENT_DEFINITION_DESCRIPTOR_NAME, DelegatedCapabilityDecision, DelegatedCapabilityRecovery,
    DelegatedCapabilityRequirement, DelegatedNamespaceSummary, DelegatedSpawnContext, SpawnHandle,
    SpawnHostMount, SpawnLaunchInputs, SpawnMountAccess, SpawnRuntimeOverrides, SpawnSpec,
    SpawnSpecValidationError, SpawnTarget, SpawnToolProfileOverride,
};
pub use ui_surface::{
    UI_SURFACE_VERSION, UiActivitySnapshot, UiActivityState, UiEvent, UiNoticeKind,
    UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot, UiThinkingState,
};
