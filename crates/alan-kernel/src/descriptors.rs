use crate::{
    ActorId, ArtifactId, BufferId, CommandId, EventId, EvidenceId, NativeReference, ObjectId,
    QueryId, SubscriptionId, TaskId, ViewId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Shared human-readable and machine-readable metadata for descriptors.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DescriptorMetadata {
    /// Short display title.
    pub title: String,
    /// Optional longer summary.
    pub summary: Option<String>,
    /// Searchable or filterable tags.
    pub tags: Vec<String>,
    /// Adapter or domain-specific structured attributes.
    pub attributes: BTreeMap<String, Value>,
}

impl DescriptorMetadata {
    /// Creates descriptor metadata with a title and no optional attributes.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            summary: None,
            tags: Vec::new(),
            attributes: BTreeMap::new(),
        }
    }
}

/// Classifies the actor that initiated or performed Alan Kernel activity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    /// A human operator or end user.
    Human,
    /// An agent acting through the environment.
    Agent,
    /// A loaded extension or plugin.
    Extension,
    /// Runtime, host, or adapter system behavior.
    System,
}

/// Describes a human, agent, extension, or system actor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActorDescriptor {
    /// Stable Alan Kernel actor id.
    pub id: ActorId,
    /// Actor class.
    pub kind: ActorKind,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
    /// Optional external authority for adapter-owned actors.
    pub native_ref: Option<NativeReference>,
}

/// Classifies inspectable resources represented by Alan Kernel objects.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// A local file or directory.
    File,
    /// A Git repository or worktree.
    GitRepository,
    /// An agent session owned by an adapter.
    AgentSession,
    /// A terminal process or pty handle owned by a host.
    Terminal,
    /// A domain-owned object from an Alan App.
    DomainResource,
    /// A synthetic object with no direct native resource.
    Synthetic,
}

/// Describes an inspectable resource with optional native authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectDescriptor {
    /// Stable Alan Kernel object id.
    pub id: ObjectId,
    /// Object class.
    pub kind: ObjectKind,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
    /// External source of truth, if the object is backed by native authority.
    pub native_ref: Option<NativeReference>,
    /// Capability labels available for this object.
    pub capabilities: Vec<String>,
}

/// Classifies active work contexts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BufferKind {
    /// Work context over an object.
    Object,
    /// Work context over a task.
    Task,
    /// Work context over a query result.
    QueryResult,
    /// Work context over domain-owned state.
    DomainState,
    /// Ephemeral work context with no durable authority.
    Scratch,
}

/// Identifies what a buffer is currently working over.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BufferSource {
    /// The buffer is backed by an object.
    Object { id: ObjectId },
    /// The buffer is backed by a task.
    Task { id: TaskId },
    /// The buffer is backed by a query result.
    Query { id: QueryId },
    /// The buffer is backed by a native domain resource.
    Native { native_ref: NativeReference },
    /// The buffer is ephemeral.
    Scratch,
}

/// Describes an active work context over objects, tasks, query results, or domain state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BufferDescriptor {
    /// Stable Alan Kernel buffer id.
    pub id: BufferId,
    /// Buffer class.
    pub kind: BufferKind,
    /// Source the buffer is working over.
    pub source: BufferSource,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
}

/// Classifies semantic presentations of buffers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    /// Conversation-oriented view.
    Conversation,
    /// Hierarchical task view.
    TaskTree,
    /// Command palette view.
    CommandPalette,
    /// Form or approval view.
    Form,
    /// Object list view.
    ObjectList,
    /// Text document read or review view.
    TextDocument,
    /// Diff view.
    Diff,
    /// Log stream view.
    LogStream,
    /// Schema-versioned dynamic view.
    Dynamic,
}

/// Describes a semantic presentation of a buffer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewDescriptor {
    /// Stable Alan Kernel view id.
    pub id: ViewId,
    /// Buffer being presented.
    pub buffer_id: BufferId,
    /// Semantic view class.
    pub kind: ViewKind,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
}

/// Classifies command risk before policy and execution decisions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRisk {
    /// The command only inspects state.
    ReadOnly,
    /// The command mutates bounded Alan Kernel state.
    Low,
    /// The command may touch native resources or long-running work.
    Medium,
    /// The command may perform broad or irreversible side effects.
    High,
}

/// Capability or permission label required to inspect or invoke a Kernel surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityRequirement {
    /// Stable capability label understood by policy or app adapters.
    pub name: String,
    /// Optional user-visible or audit-facing reason for the requirement.
    pub reason: Option<String>,
}

/// Describes whether a command has a known undo or recovery path.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandRecoveryPolicy {
    /// No undo or recovery path is known.
    #[default]
    None,
    /// Re-running the same command can safely repair or refresh the state.
    Retryable,
    /// A dedicated command can undo or compensate for this command.
    RecoveryCommand {
        /// Command that owns the undo or recovery behavior.
        command_id: CommandId,
    },
    /// Recovery is external to Alan Kernel and described for audit or UI.
    External {
        /// Human-readable recovery instructions or adapter-owned recovery key.
        description: String,
    },
}

/// Invocation surfaces that may expose a command or query.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationSurface {
    /// Main UI control or menu.
    Ui,
    /// Command palette.
    CommandPalette,
    /// Keyboard or modal grammar binding.
    Keyboard,
    /// Agent-facing action proposal or tool projection.
    Agent,
    /// Automation or scripting surface.
    Automation,
    /// Host-local or app-defined surface.
    Other(String),
}

/// Metadata that helps hosts expose an invocation without owning its semantics.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InvocationHintMetadata {
    /// Surfaces where the command or query is expected to appear.
    pub preferred_surfaces: Vec<InvocationSurface>,
    /// Stable aliases for command palettes, modal grammars, or natural language routing.
    pub aliases: Vec<String>,
    /// Optional keyboard shortcuts in host-neutral display form.
    pub keyboard_shortcuts: Vec<String>,
    /// Optional confirmation copy for risky or unusual invocations.
    pub confirmation: Option<String>,
    /// Adapter or app-specific hint data.
    pub attributes: BTreeMap<String, Value>,
}

/// Identifies the semantic target for a command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandTarget {
    /// Command targets an actor.
    Actor { id: ActorId },
    /// Command targets an object.
    Object { id: ObjectId },
    /// Command targets a buffer.
    Buffer { id: BufferId },
    /// Command targets a view.
    View { id: ViewId },
    /// Command targets a task.
    Task { id: TaskId },
    /// Command targets a native resource.
    Native { native_ref: NativeReference },
    /// Command has no preselected target.
    None,
}

/// Describes an available command without binding it to an execution backend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandDescriptor {
    /// Stable Alan Kernel command id.
    pub id: CommandId,
    /// Stable machine-readable command name.
    pub name: String,
    /// Semantic target the command applies to.
    pub target: CommandTarget,
    /// Optional JSON schema for invocation arguments.
    #[serde(default)]
    pub args_schema: Option<Value>,
    /// Capability labels required by policy before invocation.
    #[serde(default)]
    pub required_capabilities: Vec<CapabilityRequirement>,
    /// Risk class used by policy and review surfaces.
    pub risk: CommandRisk,
    /// Undo or recovery semantics exposed to audit and host surfaces.
    #[serde(default)]
    pub recovery: CommandRecoveryPolicy,
    /// Host-neutral invocation hints for palettes, controls, modal grammar, and agents.
    #[serde(default)]
    pub invocation_hints: InvocationHintMetadata,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
}

/// Identifies the semantic target for a query.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryTarget {
    /// Query targets the whole Alan Kernel semantic state.
    Kernel,
    /// Query targets an object.
    Object { id: ObjectId },
    /// Query targets a buffer.
    Buffer { id: BufferId },
    /// Query targets a view.
    View { id: ViewId },
    /// Query targets a task.
    Task { id: TaskId },
    /// Query targets a native resource.
    Native { native_ref: NativeReference },
}

/// Describes a read-only semantic query.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryDescriptor {
    /// Stable Alan Kernel query id.
    pub id: QueryId,
    /// Stable machine-readable query name.
    pub name: String,
    /// Semantic target inspected by the query.
    pub target: QueryTarget,
    /// Optional JSON schema for query parameters.
    #[serde(default)]
    pub parameters_schema: Option<Value>,
    /// Optional JSON schema for returned read-only data.
    #[serde(default)]
    pub result_schema: Option<Value>,
    /// Capability labels required by policy before read-only inspection.
    #[serde(default)]
    pub required_capabilities: Vec<CapabilityRequirement>,
    /// Host-neutral invocation hints for palettes, controls, modal grammar, and agents.
    #[serde(default)]
    pub invocation_hints: InvocationHintMetadata,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
}

/// Identifies state observed by a subscription.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SubscriptionDependency {
    /// Observe an object.
    Object { id: ObjectId },
    /// Observe a buffer.
    Buffer { id: BufferId },
    /// Observe a view.
    View { id: ViewId },
    /// Observe a task.
    Task { id: TaskId },
    /// Observe a query result.
    Query { id: QueryId },
    /// Observe command availability for a target.
    CommandAvailability { target: CommandTarget },
}

/// Describes an observation or invalidation subscription.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionDescriptor {
    /// Stable Alan Kernel subscription id.
    pub id: SubscriptionId,
    /// Stable machine-readable subscription name.
    pub name: String,
    /// State this subscription observes.
    pub dependencies: Vec<SubscriptionDependency>,
    /// Capability labels required by policy before observing the dependencies.
    #[serde(default)]
    pub required_capabilities: Vec<CapabilityRequirement>,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
}

/// Current coarse task state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task is known but has not started.
    Pending,
    /// Task is actively running.
    Running,
    /// Task is waiting for external input.
    Yielded,
    /// Task completed successfully.
    Completed,
    /// Task failed.
    Failed,
    /// Task was cancelled.
    Cancelled,
}

/// Describes command execution or long-running work.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskDescriptor {
    /// Stable Alan Kernel task id.
    pub id: TaskId,
    /// Actor responsible for the task.
    pub actor_id: ActorId,
    /// Optional parent task for task-tree rendering.
    pub parent_task_id: Option<TaskId>,
    /// Command that initiated the task, if any.
    pub command_id: Option<CommandId>,
    /// Current coarse status.
    pub status: TaskStatus,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
}

/// Describes a produced Alan Kernel artifact.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    /// Stable Alan Kernel artifact id.
    pub id: ArtifactId,
    /// Producing task, if known.
    pub task_id: Option<TaskId>,
    /// Related object, if known.
    pub object_id: Option<ObjectId>,
    /// Related buffer, if known.
    pub buffer_id: Option<BufferId>,
    /// Native reference for externally owned artifact content.
    pub native_ref: Option<NativeReference>,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
}

/// Describes evidence supporting a task, artifact, decision, or claim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvidenceDescriptor {
    /// Stable Alan Kernel evidence id.
    pub id: EvidenceId,
    /// Related task, if known.
    pub task_id: Option<TaskId>,
    /// Related artifact, if known.
    pub artifact_id: Option<ArtifactId>,
    /// Related activity event, if known.
    pub event_id: Option<EventId>,
    /// Native reference for externally owned evidence content.
    pub native_ref: Option<NativeReference>,
    /// Human-readable and structured metadata.
    pub metadata: DescriptorMetadata,
}

#[cfg(test)]
mod tests {
    use super::{
        BufferDescriptor, BufferKind, BufferSource, DescriptorMetadata, ObjectDescriptor,
        ObjectKind,
    };
    use crate::{FileReference, NativeReference, ObjectId};

    #[test]
    fn object_descriptor_keeps_file_authority_in_native_reference() {
        let object_id = ObjectId::new();
        let descriptor = ObjectDescriptor {
            id: object_id,
            kind: ObjectKind::File,
            metadata: DescriptorMetadata::new("README.md"),
            native_ref: Some(NativeReference::File(FileReference {
                path: "/workspace/README.md".to_string(),
                version: Some("sha256:example".to_string()),
            })),
            capabilities: vec!["read".to_string()],
        };

        assert_eq!(descriptor.id, object_id);
        assert!(matches!(
            descriptor.native_ref,
            Some(NativeReference::File(_))
        ));
    }

    #[test]
    fn buffer_descriptor_is_distinct_from_object_descriptor() {
        let object_id = ObjectId::new();
        let buffer = BufferDescriptor {
            id: crate::BufferId::new(),
            kind: BufferKind::Object,
            source: BufferSource::Object { id: object_id },
            metadata: DescriptorMetadata::new("README.md buffer"),
        };

        assert!(matches!(buffer.source, BufferSource::Object { id } if id == object_id));
    }
}
