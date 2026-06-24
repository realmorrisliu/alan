//! Renderer-independent Alan Kernel semantic runtime primitives.
//!
//! `alan-kernel` owns the typed identifiers, descriptors, and native authority
//! references shared by future Alan apps, Host Service adapters, and renderer hosts.
//! It intentionally avoids Alan protocol, terminal renderer, macOS UI, and executor
//! dependencies.

mod agent_capability;
mod descriptors;
mod events;
mod ids;
mod invocation;
mod ledger;
mod native_ref;
mod projection;
mod registry;
mod views;

pub use agent_capability::{
    AgentCapabilityDescriptor, AgentCapabilityDescriptorId, AgentCapabilityKind,
    AgentRunDescriptor, AgentRunOwner, AgentRunStatus, AllowedCommandGrant, AuditReference,
    Auditability, ContextGrant, ContextReadGrant, ContextSelection, ContextTargetRef, EffectClass,
    EvidenceRequirement, ExecutionGuardKind, ExecutionGuardMetadata, ExecutionGuardStrength,
    PrivacyPolicy, ResultContract, ResultField, SelectionRange,
};
pub use descriptors::{
    ActorDescriptor, ActorKind, ArtifactDescriptor, BufferDescriptor, BufferKind, BufferSource,
    CapabilityRequirement, CommandDescriptor, CommandRecoveryPolicy, CommandRisk, CommandTarget,
    DescriptorMetadata, EvidenceDescriptor, InvocationHintMetadata, InvocationSurface,
    ObjectDescriptor, ObjectKind, QueryDescriptor, QueryTarget, SubscriptionDependency,
    SubscriptionDescriptor, TaskDescriptor, TaskStatus, ViewDescriptor, ViewKind,
};
pub use events::{
    KERNEL_EVENT_SCHEMA_VERSION, KernelEvent, KernelEventKind, TaskEvent, TaskEventKind,
    TaskFailure, TaskOutputChunk, TaskOutputStream, TaskProgress, TaskResumeRecord, TaskSideEffect,
    TaskSideEffectKind, TaskYieldCheckpoint, TaskYieldKind,
};
pub use ids::{
    ActorId, AgentRunId, ArtifactId, AuditRecordId, BufferId, CommandId, ContextGrantId, EventId,
    EvidenceId, ExecutionGuardId, ObjectId, QueryId, ResultContractId, SubscriptionId, TaskId,
    ViewId,
};
pub use invocation::{
    CommandInvocation, QueryInvocation, QueryResultReference, SubscriptionMessage,
    SubscriptionMessageKind,
};
pub use ledger::{
    ActivityLedger, ActivityLedgerError, ActivityLedgerResult, InMemoryActivityLedger,
    JsonlActivityLedger,
};
pub use native_ref::{
    AgentSessionReference, DomainResourceReference, FileReference, GitRepositoryReference,
    NativeReference, TerminalHandleReference,
};
pub use projection::{CommandAvailabilityProjection, DirtyView, ProjectionStore};
pub use registry::{
    CommandRegistry, InMemoryCommandRegistry, InMemoryKernelRegistry, InMemoryQueryRegistry,
    InMemorySubscriptionRegistry, QueryRegistry, SubscriptionRegistry,
};
pub use views::{
    CommandPaletteEntry, CommandPaletteViewModel, ConversationBlock, ConversationBlockKind,
    ConversationViewModel, DiffFile, DiffHunk, DiffLine, DiffViewModel, DynamicViewPayload,
    FormField, FormFieldKind, FormViewModel, LogEntry, LogStreamViewModel, ObjectListItem,
    ObjectListViewModel, TaskTreeNode, TaskTreeViewModel, TextDocumentViewModel, ViewAction,
    ViewDiagnostic, ViewDiagnosticSeverity, ViewFocus, ViewModel, ViewSelection, ViewSemanticState,
    ViewSnapshot,
};
