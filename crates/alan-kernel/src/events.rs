use crate::{
    ActorId, ArtifactDescriptor, ArtifactId, BufferDescriptor, CommandId, CommandInvocation,
    CommandTarget, EventId, EvidenceDescriptor, EvidenceId, NativeReference, ObjectDescriptor,
    QueryInvocation, SubscriptionMessage, TaskDescriptor, TaskId, ViewDescriptor, ViewId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current Alan Kernel activity event schema version.
pub const KERNEL_EVENT_SCHEMA_VERSION: u16 = 1;

/// Schema-versioned Alan Kernel activity event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KernelEvent {
    /// Schema version for this event envelope.
    pub schema_version: u16,
    /// Stable activity event id.
    pub event_id: EventId,
    /// Monotonic sequence within the ledger or stream that emitted the event.
    pub sequence: u64,
    /// Unix epoch timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Actor responsible for the event.
    pub actor_id: ActorId,
    /// Event that directly caused this event, if known.
    pub causation_id: Option<EventId>,
    /// Event-chain id shared by related commands, tasks, artifacts, and evidence.
    pub correlation_id: EventId,
    /// Typed event payload.
    pub kind: KernelEventKind,
}

impl KernelEvent {
    /// Creates a root event whose correlation id is its own event id.
    #[must_use]
    pub fn root(
        event_id: EventId,
        sequence: u64,
        timestamp_ms: u64,
        actor_id: ActorId,
        kind: KernelEventKind,
    ) -> Self {
        Self {
            schema_version: KERNEL_EVENT_SCHEMA_VERSION,
            event_id,
            sequence,
            timestamp_ms,
            actor_id,
            causation_id: None,
            correlation_id: event_id,
            kind,
        }
    }

    /// Creates a caused event in an existing correlation chain.
    #[must_use]
    pub fn caused_by(
        event_id: EventId,
        sequence: u64,
        timestamp_ms: u64,
        actor_id: ActorId,
        causation_id: EventId,
        correlation_id: EventId,
        kind: KernelEventKind,
    ) -> Self {
        Self {
            schema_version: KERNEL_EVENT_SCHEMA_VERSION,
            event_id,
            sequence,
            timestamp_ms,
            actor_id,
            causation_id: Some(causation_id),
            correlation_id,
            kind,
        }
    }
}

/// Typed Kernel event payloads consumed by ledgers and projections.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KernelEventKind {
    /// A mutating or work-initiating command was requested.
    CommandInvoked {
        /// Command invocation intent.
        invocation: CommandInvocation,
    },
    /// A read-only query was requested.
    QueryInvoked {
        /// Query invocation record.
        invocation: QueryInvocation,
    },
    /// A subscription emitted an observational update or invalidation.
    SubscriptionObserved {
        /// Subscription observation message.
        message: SubscriptionMessage,
    },
    /// Object descriptor was created or refreshed.
    ObjectUpserted {
        /// Current object descriptor.
        descriptor: ObjectDescriptor,
    },
    /// Buffer descriptor was created or refreshed.
    BufferUpserted {
        /// Current buffer descriptor.
        descriptor: BufferDescriptor,
    },
    /// View descriptor was created or refreshed.
    ViewUpserted {
        /// Current view descriptor.
        descriptor: ViewDescriptor,
    },
    /// Artifact descriptor was recorded outside a task-specific event.
    ArtifactRecorded {
        /// Current artifact descriptor.
        descriptor: ArtifactDescriptor,
    },
    /// Evidence descriptor was recorded outside a task-specific event.
    EvidenceRecorded {
        /// Current evidence descriptor.
        descriptor: EvidenceDescriptor,
    },
    /// Command availability changed for a target.
    CommandAvailabilityChanged {
        /// Target whose command availability changed.
        target: CommandTarget,
        /// Available commands for the target.
        command_ids: Vec<CommandId>,
    },
    /// A semantic view was marked dirty and should be refreshed by hosts.
    ViewInvalidated {
        /// Dirty view id.
        view_id: ViewId,
        /// Optional invalidation reason.
        reason: Option<String>,
    },
    /// Task lifecycle event.
    Task {
        /// Task event payload.
        event: TaskEvent,
    },
}

/// Task lifecycle event linked to a task id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskEvent {
    /// Task whose lifecycle changed.
    pub task_id: TaskId,
    /// Typed lifecycle payload.
    pub kind: TaskEventKind,
}

/// Typed task lifecycle payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEventKind {
    /// Task started and has an initial descriptor.
    Started {
        /// Initial task descriptor.
        descriptor: TaskDescriptor,
    },
    /// Task made progress without reaching a terminal state.
    Progress {
        /// Progress payload.
        progress: TaskProgress,
    },
    /// Task appended output to a semantic stream.
    OutputAppended {
        /// Output payload.
        output: TaskOutputChunk,
    },
    /// Task yielded and is waiting for external input.
    Yielded {
        /// Yield checkpoint.
        checkpoint: TaskYieldCheckpoint,
    },
    /// Task resumed from a prior yield checkpoint.
    Resumed {
        /// Resume payload.
        resume: TaskResumeRecord,
    },
    /// A side effect has been planned but not yet committed.
    SideEffectPlanned {
        /// Planned effect descriptor.
        effect: TaskSideEffect,
    },
    /// A side effect has been committed after the external mutation succeeded.
    SideEffectCommitted {
        /// Committed effect descriptor.
        effect: TaskSideEffect,
        /// Evidence proving or describing the committed effect.
        evidence_ids: Vec<EvidenceId>,
    },
    /// Task created an artifact.
    ArtifactCreated {
        /// Artifact descriptor.
        artifact: ArtifactDescriptor,
    },
    /// Task attached evidence.
    EvidenceAttached {
        /// Evidence descriptor.
        evidence: EvidenceDescriptor,
    },
    /// Task completed successfully.
    Completed {
        /// Optional completion summary.
        summary: Option<String>,
        /// Artifacts produced by the completed task.
        artifact_ids: Vec<ArtifactId>,
        /// Evidence supporting the completion.
        evidence_ids: Vec<EvidenceId>,
    },
    /// Task failed.
    Failed {
        /// Failure payload.
        failure: TaskFailure,
    },
    /// Task was cancelled.
    Cancelled {
        /// Optional cancellation reason.
        reason: Option<String>,
    },
}

/// Progress update for a task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskProgress {
    /// Optional current step label.
    pub label: Option<String>,
    /// Optional completed work units.
    pub completed: Option<u64>,
    /// Optional total work units.
    pub total: Option<u64>,
    /// Optional bounded progress fraction from 0.0 to 1.0.
    pub fraction: Option<f32>,
    /// Optional human-readable progress message.
    pub message: Option<String>,
}

/// Semantic output stream classification for task output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutputStream {
    /// User-visible text.
    Text,
    /// Model or agent thinking text.
    Thinking,
    /// Tool, command, or process log.
    Log,
    /// Standard output from a native process.
    Stdout,
    /// Standard error from a native process.
    Stderr,
    /// Host or app-specific stream.
    Other(String),
}

/// Output appended by a task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskOutputChunk {
    /// Output stream.
    pub stream: TaskOutputStream,
    /// Appended text or bounded payload.
    pub content: String,
    /// Whether this chunk completes the stream.
    pub terminal: bool,
}

/// Kinds of task yield checkpoints.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskYieldKind {
    /// Human approval or confirmation.
    Confirmation,
    /// Structured input requested from a human, agent, or extension.
    StructuredInput,
    /// Client-side or host-side dynamic tool checkpoint.
    DynamicTool,
    /// App or adapter-specific checkpoint.
    Other(String),
}

/// Yield checkpoint that can be projected as a form or approval surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskYieldCheckpoint {
    /// Adapter-owned request id.
    pub request_id: String,
    /// Yield kind.
    pub kind: TaskYieldKind,
    /// Bounded payload needed to render or route the checkpoint.
    pub payload: Value,
    /// Whether a resume command can continue the task from this checkpoint.
    pub resumable: bool,
}

/// Record that a yielded task resumed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskResumeRecord {
    /// Adapter-owned request id that was resumed.
    pub request_id: String,
    /// Optional summary of the response, without requiring the raw response to be logged.
    pub response_summary: Option<String>,
    /// Optional bounded structured response payload.
    pub response_payload: Option<Value>,
}

/// Side-effect class for a task event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSideEffectKind {
    /// Bounded Kernel state mutation.
    KernelState,
    /// Native filesystem mutation.
    FileSystem,
    /// Native process, shell, tool, or extension execution.
    Execution,
    /// Network or external service effect.
    Network,
    /// Terminal input or process interaction.
    Terminal,
    /// App or adapter-specific effect.
    Other(String),
}

/// Planned or committed side effect for a task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskSideEffect {
    /// Stable adapter-owned side-effect id.
    pub effect_id: String,
    /// Side-effect class.
    pub kind: TaskSideEffectKind,
    /// Human-readable effect summary.
    pub summary: String,
    /// Native resources that may be touched or were touched.
    pub native_refs: Vec<NativeReference>,
    /// Optional bounded structured effect payload.
    pub payload: Option<Value>,
}

/// Failure payload for a task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskFailure {
    /// Stable error code or class.
    pub code: String,
    /// Human-readable failure message.
    pub message: String,
    /// Whether the failure can be retried by policy or a recovery command.
    pub retryable: bool,
    /// Evidence supporting the failure.
    pub evidence_ids: Vec<EvidenceId>,
}

#[cfg(test)]
mod tests {
    use super::{
        KernelEvent, KernelEventKind, TaskEvent, TaskEventKind, TaskOutputChunk, TaskOutputStream,
        TaskProgress, TaskYieldCheckpoint, TaskYieldKind,
    };
    use crate::{
        ActorId, CommandDescriptor, CommandInvocation, CommandRecoveryPolicy, CommandRisk,
        CommandTarget, DescriptorMetadata, EventId, InvocationHintMetadata, TaskDescriptor,
        TaskStatus,
    };
    use serde_json::json;

    #[test]
    fn kernel_event_records_schema_actor_causation_and_correlation() {
        let actor_id = ActorId::new();
        let root_event_id = EventId::new();
        let command = CommandDescriptor {
            id: crate::CommandId::new(),
            name: "agent.submit_turn".to_string(),
            target: CommandTarget::None,
            args_schema: None,
            required_capabilities: Vec::new(),
            risk: CommandRisk::Medium,
            recovery: CommandRecoveryPolicy::Retryable,
            invocation_hints: InvocationHintMetadata::default(),
            metadata: DescriptorMetadata::new("Submit turn"),
        };
        let invocation = CommandInvocation::from_descriptor(&command, actor_id, json!({}));
        let root = KernelEvent::root(
            root_event_id,
            1,
            1_772_000_000_000,
            actor_id,
            KernelEventKind::CommandInvoked { invocation },
        );

        let task_event_id = EventId::new();
        let task = TaskDescriptor {
            id: crate::TaskId::new(),
            actor_id,
            parent_task_id: None,
            command_id: Some(command.id),
            status: TaskStatus::Running,
            metadata: DescriptorMetadata::new("Agent turn"),
        };
        let caused = KernelEvent::caused_by(
            task_event_id,
            2,
            1_772_000_000_010,
            actor_id,
            root.event_id,
            root.correlation_id,
            KernelEventKind::Task {
                event: TaskEvent {
                    task_id: task.id,
                    kind: TaskEventKind::Started { descriptor: task },
                },
            },
        );

        assert_eq!(root.schema_version, super::KERNEL_EVENT_SCHEMA_VERSION);
        assert_eq!(root.correlation_id, root.event_id);
        assert_eq!(caused.causation_id, Some(root.event_id));
        assert_eq!(caused.correlation_id, root.correlation_id);
    }

    #[test]
    fn task_event_kind_covers_progress_output_yield_and_terminal_states() {
        let task_id = crate::TaskId::new();
        let progress = TaskEvent {
            task_id,
            kind: TaskEventKind::Progress {
                progress: TaskProgress {
                    label: Some("discover".to_string()),
                    completed: Some(1),
                    total: Some(3),
                    fraction: Some(0.33),
                    message: Some("reading inputs".to_string()),
                },
            },
        };
        let output = TaskEvent {
            task_id,
            kind: TaskEventKind::OutputAppended {
                output: TaskOutputChunk {
                    stream: TaskOutputStream::Text,
                    content: "partial answer".to_string(),
                    terminal: false,
                },
            },
        };
        let yielded = TaskEvent {
            task_id,
            kind: TaskEventKind::Yielded {
                checkpoint: TaskYieldCheckpoint {
                    request_id: "confirm-1".to_string(),
                    kind: TaskYieldKind::Confirmation,
                    payload: json!({"question": "Proceed?"}),
                    resumable: true,
                },
            },
        };
        let completed = TaskEvent {
            task_id,
            kind: TaskEventKind::Completed {
                summary: Some("done".to_string()),
                artifact_ids: Vec::new(),
                evidence_ids: Vec::new(),
            },
        };

        assert!(matches!(progress.kind, TaskEventKind::Progress { .. }));
        assert!(matches!(output.kind, TaskEventKind::OutputAppended { .. }));
        assert!(matches!(yielded.kind, TaskEventKind::Yielded { .. }));
        assert!(matches!(completed.kind, TaskEventKind::Completed { .. }));
    }
}
