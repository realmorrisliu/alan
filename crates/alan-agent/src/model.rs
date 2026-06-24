use alan_kernel::{
    ActorDescriptor, ActorId, BufferDescriptor, BufferId, CommandDescriptor, CommandId,
    NativeReference, ObjectDescriptor, ViewDescriptor, ViewId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable app id for the built-in Alan Agent app.
pub const ALAN_AGENT_APP_ID: &str = "alan.agent";

/// Adapter label used for current daemon-backed compatibility sessions.
pub const COMPATIBILITY_SESSION_ADAPTER: &str = "alan-agent-compat";

/// Host-independent metadata for a current daemon-backed Alan Agent session.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceSessionMetadata {
    /// Adapter-owned session id.
    pub session_id: String,
    /// Workspace directory, alias, or display identifier if known.
    pub workspace_dir: Option<String>,
    /// Resolved agent name if known.
    pub agent_name: Option<String>,
    /// Resolved connection profile id if known.
    pub profile_id: Option<String>,
    /// Provider family if known.
    pub provider: Option<String>,
    /// Resolved model if known.
    pub resolved_model: Option<String>,
}

impl AgentWorkspaceSessionMetadata {
    /// Creates metadata for a compatibility session id.
    #[must_use]
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            ..Self::default()
        }
    }
}

/// Message recovered from current Host Service history APIs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceHydratedMessage {
    /// Message role from the compatibility history source.
    pub role: String,
    /// Text content.
    pub content: String,
    /// Optional tool name for tool-role messages.
    pub tool_name: Option<String>,
}

/// Child-run lifecycle input from the current Agent Execution Engine or delegated skills.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceChildRunEvent {
    /// Current engine child-run id.
    pub child_run_id: String,
    /// Child-run lifecycle status.
    pub status: AgentWorkspaceChildRunStatus,
    /// Optional delegated skill or launch target.
    pub delegated_skill: Option<String>,
    /// Optional human-readable summary.
    pub summary: Option<String>,
    /// Evidence records attached to the child run.
    pub evidence: Vec<AgentWorkspaceEvidenceInput>,
}

/// Child-run lifecycle states projected into Agent Workspace tasks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkspaceChildRunStatus {
    /// Child run started.
    Started,
    /// Child run is running.
    Running,
    /// Child run yielded for input or approval.
    Yielded,
    /// Child run completed successfully.
    Completed,
    /// Child run failed.
    Failed,
    /// Child run was cancelled.
    Cancelled,
}

/// Memory observation input from recall, promotion, or flush surfaces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceMemoryObservation {
    /// Memory observation kind.
    pub kind: AgentWorkspaceMemoryObservationKind,
    /// Human-readable title.
    pub title: String,
    /// Optional preview text.
    pub preview: Option<String>,
    /// Optional native authority for durable memory content.
    pub native_ref: Option<NativeReference>,
    /// Bounded structured payload.
    pub payload: Value,
}

/// Memory event classes visible in Agent Workspace memory review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkspaceMemoryObservationKind {
    /// Memory was recalled into current context.
    Recall,
    /// Memory was promoted from current work into a durable layer.
    Promotion,
    /// Memory was flushed during or before compaction.
    Flush,
}

/// Rollout or event-log record projected into Agent Workspace evidence surfaces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentWorkspaceRolloutRecord {
    /// Rollout artifact.
    Artifact {
        /// Artifact title.
        title: String,
        /// Optional native authority for artifact content.
        native_ref: Option<NativeReference>,
        /// Bounded structured payload.
        payload: Value,
    },
    /// Planned or committed side effect.
    Effect {
        /// Adapter-owned effect id.
        effect_id: String,
        /// Effect kind.
        kind: AgentWorkspaceEffectKind,
        /// Human-readable effect summary.
        summary: String,
        /// Whether the side effect already committed.
        committed: bool,
        /// Native resources associated with the effect.
        native_refs: Vec<NativeReference>,
        /// Optional bounded payload.
        payload: Option<Value>,
    },
    /// Yield, checkpoint, or replay recovery point.
    Checkpoint {
        /// Adapter-owned checkpoint id.
        checkpoint_id: String,
        /// Human-readable title.
        title: String,
        /// Bounded structured payload.
        payload: Value,
    },
    /// Evidence record.
    Evidence(AgentWorkspaceEvidenceInput),
}

/// Host-independent side-effect class used by Agent Workspace rollout projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkspaceEffectKind {
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

/// Evidence input that can be attached to tasks, child runs, memory, or rollouts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceEvidenceInput {
    /// Evidence title.
    pub title: String,
    /// Optional native authority for evidence content.
    pub native_ref: Option<NativeReference>,
    /// Bounded structured payload.
    pub payload: Value,
}

/// Agent Workspace object roles owned by the built-in Alan Agent app.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkspaceObjectRole {
    /// Current daemon-backed compatibility session.
    CompatibilitySession,
    /// Bounded Agent Run projected from current session work.
    AgentRun,
    /// Supervisor-raised task inbox.
    SupervisorTaskInbox,
    /// Memory review area.
    MemoryEntries,
    /// Evidence browser.
    Evidence,
    /// Artifact browser.
    Artifacts,
    /// Plan browser.
    Plans,
}

impl AgentWorkspaceObjectRole {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::CompatibilitySession => "compatibility_session",
            Self::AgentRun => "agent_run",
            Self::SupervisorTaskInbox => "supervisor_task_inbox",
            Self::MemoryEntries => "memory_entries",
            Self::Evidence => "evidence",
            Self::Artifacts => "artifacts",
            Self::Plans => "plans",
        }
    }
}

/// Buffer and view ids for the first Agent Workspace slice.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceSurfaceIds {
    /// Conversation buffer.
    pub conversation_buffer: BufferId,
    /// Conversation view.
    pub conversation_view: ViewId,
    /// Task tree buffer.
    pub task_tree_buffer: BufferId,
    /// Task tree view.
    pub task_tree_view: ViewId,
    /// Evidence buffer.
    pub evidence_buffer: BufferId,
    /// Evidence browser view.
    pub evidence_view: ViewId,
    /// Memory review buffer.
    pub memory_review_buffer: BufferId,
    /// Memory review view.
    pub memory_review_view: ViewId,
    /// Approval form buffer.
    pub approval_form_buffer: BufferId,
    /// Approval form view.
    pub approval_form_view: ViewId,
    /// Command palette buffer.
    pub command_palette_buffer: BufferId,
    /// Command palette view.
    pub command_palette_view: ViewId,
    /// Audit log buffer.
    pub audit_buffer: BufferId,
    /// Audit log view.
    pub audit_view: ViewId,
}

impl Default for AgentWorkspaceSurfaceIds {
    fn default() -> Self {
        Self {
            conversation_buffer: BufferId::new(),
            conversation_view: ViewId::new(),
            task_tree_buffer: BufferId::new(),
            task_tree_view: ViewId::new(),
            evidence_buffer: BufferId::new(),
            evidence_view: ViewId::new(),
            memory_review_buffer: BufferId::new(),
            memory_review_view: ViewId::new(),
            approval_form_buffer: BufferId::new(),
            approval_form_view: ViewId::new(),
            command_palette_buffer: BufferId::new(),
            command_palette_view: ViewId::new(),
            audit_buffer: BufferId::new(),
            audit_view: ViewId::new(),
        }
    }
}

/// Command ids for commands exposed by the first Agent Workspace slice.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceCommandIds {
    /// Submit a user turn.
    pub submit_turn: CommandId,
    /// Resume a yielded task.
    pub resume_yield: CommandId,
    /// Approve a pending command or operation.
    pub approve_command: CommandId,
    /// Deny a pending command or operation.
    pub deny_command: CommandId,
    /// Interrupt active work.
    pub interrupt: CommandId,
    /// Compact context.
    pub compact: CommandId,
    /// Roll back turn history.
    pub rollback: CommandId,
    /// Inspect selected evidence.
    pub inspect_evidence: CommandId,
    /// Promote a supervisor-raised task into Agent Workspace flow.
    pub promote_supervisor_task: CommandId,
    /// Open memory review.
    pub open_memory_review: CommandId,
}

impl Default for AgentWorkspaceCommandIds {
    fn default() -> Self {
        Self {
            submit_turn: CommandId::new(),
            resume_yield: CommandId::new(),
            approve_command: CommandId::new(),
            deny_command: CommandId::new(),
            interrupt: CommandId::new(),
            compact: CommandId::new(),
            rollback: CommandId::new(),
            inspect_evidence: CommandId::new(),
            promote_supervisor_task: CommandId::new(),
            open_memory_review: CommandId::new(),
        }
    }
}

/// Object ids for the first Agent Workspace slice.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceObjectIds {
    /// Compatibility session object.
    pub compatibility_session: alan_kernel::ObjectId,
    /// Bounded Agent Run object.
    pub agent_run: alan_kernel::ObjectId,
    /// Supervisor task inbox object.
    pub supervisor_tasks: alan_kernel::ObjectId,
    /// Memory review object.
    pub memory_entries: alan_kernel::ObjectId,
    /// Evidence browser object.
    pub evidence: alan_kernel::ObjectId,
    /// Artifact browser object.
    pub artifacts: alan_kernel::ObjectId,
    /// Plan browser object.
    pub plans: alan_kernel::ObjectId,
}

impl Default for AgentWorkspaceObjectIds {
    fn default() -> Self {
        Self {
            compatibility_session: alan_kernel::ObjectId::new(),
            agent_run: alan_kernel::ObjectId::new(),
            supervisor_tasks: alan_kernel::ObjectId::new(),
            memory_entries: alan_kernel::ObjectId::new(),
            evidence: alan_kernel::ObjectId::new(),
            artifacts: alan_kernel::ObjectId::new(),
            plans: alan_kernel::ObjectId::new(),
        }
    }
}

/// Stable ids allocated by one Agent Workspace projection instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceIds {
    /// Human operator actor id.
    pub user_actor: ActorId,
    /// Alan Agent actor id.
    pub agent_actor: ActorId,
    /// Projection adapter actor id.
    pub system_actor: ActorId,
    /// Workspace object ids.
    pub objects: AgentWorkspaceObjectIds,
    /// Workspace surface ids.
    pub surfaces: AgentWorkspaceSurfaceIds,
    /// Workspace command ids.
    pub commands: AgentWorkspaceCommandIds,
}

impl Default for AgentWorkspaceIds {
    fn default() -> Self {
        Self {
            user_actor: ActorId::new(),
            agent_actor: ActorId::new(),
            system_actor: ActorId::new(),
            objects: AgentWorkspaceObjectIds::default(),
            surfaces: AgentWorkspaceSurfaceIds::default(),
            commands: AgentWorkspaceCommandIds::default(),
        }
    }
}

/// Static Agent Workspace descriptors for a compatibility session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceModel {
    /// Actors known to this workspace.
    pub actors: Vec<ActorDescriptor>,
    /// Object descriptors.
    pub objects: Vec<ObjectDescriptor>,
    /// Buffer descriptors.
    pub buffers: Vec<BufferDescriptor>,
    /// View descriptors.
    pub views: Vec<ViewDescriptor>,
    /// Command descriptors.
    pub commands: Vec<CommandDescriptor>,
}
