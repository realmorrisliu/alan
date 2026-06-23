//! Protocol definitions for the alan agent.
//!
//! This crate defines the `Op` (user operations) and `Event` (system events)
//! types that form the communication protocol between the agent core and
//! various frontend interfaces (CLI, REST API, WebSocket).

mod adaptive;
mod compaction;
mod content;
mod event;
mod host_auth;
mod memory;
mod op;
mod reasoning;
mod spawn;

pub use adaptive::{
    AdaptiveForm, AdaptivePresentationHint, AdaptiveYieldCapabilities, ClientCapabilities,
    ConfirmationYieldPayload, CustomYieldPayload, DynamicToolYieldPayload, StructuredInputKind,
    StructuredInputOption, StructuredInputQuestion, StructuredInputYieldPayload,
};
pub use compaction::{
    AppliedCompactionOutcome, CompactionAttemptSnapshot, CompactionMode, CompactionOutcome,
    CompactionPressureLevel, CompactionReason, CompactionRequestMetadata, CompactionResult,
    CompactionSkipReason, CompactionTrigger, FailedCompactionOutcome, SkippedCompactionOutcome,
};
pub use content::{ContentPart, parts_to_text};
pub use event::{
    DiffHunk, DiffLine, Event, EventEnvelope, ToolDecisionAudit, ToolResultPresentation, YieldKind,
};
pub use host_auth::{
    AuthErrorCode, AuthErrorResponse, AuthEvent, AuthEventEnvelope, AuthLoginMethod,
    AuthPendingLoginSummary, AuthProviderId, AuthStatusKind, AuthStatusSnapshot,
};
pub use memory::{MemoryFlushAttemptSnapshot, MemoryFlushResult, MemoryFlushSkipReason};
pub use op::{
    DynamicToolSpec, GovernanceConfig, GovernanceProfile, InputMode, Op, PlanItem, PlanItemStatus,
    Submission, ToolCapability, TurnContext,
};
pub use reasoning::{ReasoningControls, ReasoningEffort};
pub use spawn::{
    SpawnHandle, SpawnLaunchInputs, SpawnRuntimeOverrides, SpawnSpec, SpawnTarget,
    SpawnToolProfileOverride,
};
