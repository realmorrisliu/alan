use crate::{
    ActorId, AgentRunId, AuditRecordId, BufferId, CommandId, CommandRisk, ContextGrantId,
    EvidenceId, ExecutionGuardId, NativeReference, ObjectId, ResultContractId, TaskId, ViewId,
};
use serde::{Deserialize, Serialize};

/// Stable OS id for an Agent Capability descriptor.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentCapabilityDescriptorId(String);

impl AgentCapabilityDescriptorId {
    /// Creates a descriptor id from a stable OS string such as `agent.explain`.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the stable descriptor id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentCapabilityDescriptorId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// First supported Agent Capability descriptor kinds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCapabilityKind {
    /// Explain bounded app, object, document, code, task, or runtime state.
    Explain,
    /// Summarize bounded content or activity.
    Summarize,
    /// Create a non-executing plan for bounded work.
    Plan,
    /// Propose commands or app actions for later governed execution.
    ProposeCommands,
    /// Delegate bounded subwork to another agent capability, skill package, or worker.
    Delegate,
}

impl AgentCapabilityKind {
    /// Returns the stable OS descriptor id for this capability kind.
    #[must_use]
    pub fn descriptor_id(&self) -> AgentCapabilityDescriptorId {
        let value = match self {
            Self::Explain => "agent.explain",
            Self::Summarize => "agent.summarize",
            Self::Plan => "agent.plan",
            Self::ProposeCommands => "agent.propose_commands",
            Self::Delegate => "agent.delegate",
        };

        AgentCapabilityDescriptorId::new(value)
    }
}

/// Semantic Agent Capability descriptor shared by apps, hosts, and service adapters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentCapabilityDescriptor {
    /// Stable OS descriptor id.
    pub id: AgentCapabilityDescriptorId,
    /// Capability kind.
    pub kind: AgentCapabilityKind,
    /// Human-readable purpose.
    pub summary: String,
    /// Context grant fields this descriptor expects.
    pub context_grant_requirements: Vec<String>,
    /// Default result fields requested by this descriptor.
    pub result_contract_defaults: Vec<ResultField>,
    /// Effect classes this descriptor may request or propose.
    pub allowed_effect_classes: Vec<EffectClass>,
    /// Default command risk before a concrete command proposal is evaluated.
    pub default_command_risk: CommandRisk,
    /// Governance notes for policy and review surfaces.
    pub governance_notes: Vec<String>,
    /// Optional app-facing language or alias guidance.
    pub app_language: Option<String>,
}

impl AgentCapabilityDescriptor {
    /// Creates a descriptor with its stable id derived from the capability kind.
    #[must_use]
    pub fn new(
        kind: AgentCapabilityKind,
        summary: impl Into<String>,
        context_grant_requirements: Vec<String>,
        result_contract_defaults: Vec<ResultField>,
        allowed_effect_classes: Vec<EffectClass>,
        default_command_risk: CommandRisk,
    ) -> Self {
        Self {
            id: kind.descriptor_id(),
            kind,
            summary: summary.into(),
            context_grant_requirements,
            result_contract_defaults,
            allowed_effect_classes,
            default_command_risk,
            governance_notes: Vec::new(),
            app_language: None,
        }
    }

    /// Returns the V1 descriptor taxonomy.
    #[must_use]
    pub fn v1_taxonomy() -> Vec<Self> {
        vec![
            Self::new(
                AgentCapabilityKind::Explain,
                "Explain bounded app, object, document, code, task, or runtime state.",
                vec![
                    "app_id".to_string(),
                    "target_refs".to_string(),
                    "allowed_reads".to_string(),
                    "privacy_policy".to_string(),
                ],
                vec![
                    ResultField::Answer,
                    ResultField::Citations,
                    ResultField::Evidence,
                    ResultField::Uncertainty,
                    ResultField::FollowUpQuestions,
                    ResultField::AuditSummary,
                ],
                vec![EffectClass::Inspect, EffectClass::Draft],
                CommandRisk::Low,
            ),
            Self::new(
                AgentCapabilityKind::Summarize,
                "Summarize bounded content or activity.",
                vec![
                    "app_id".to_string(),
                    "target_refs".to_string(),
                    "content_bounds".to_string(),
                    "evidence_requirements".to_string(),
                ],
                vec![
                    ResultField::Summary,
                    ResultField::Citations,
                    ResultField::Evidence,
                    ResultField::Uncertainty,
                    ResultField::AuditSummary,
                ],
                vec![EffectClass::Inspect, EffectClass::Draft],
                CommandRisk::Low,
            ),
            Self::new(
                AgentCapabilityKind::Plan,
                "Create a non-executing plan for bounded work.",
                vec![
                    "app_id".to_string(),
                    "task_goal".to_string(),
                    "target_refs".to_string(),
                    "allowed_reads".to_string(),
                ],
                vec![
                    ResultField::Plan,
                    ResultField::Evidence,
                    ResultField::Uncertainty,
                    ResultField::AuditSummary,
                ],
                vec![EffectClass::Inspect, EffectClass::Draft],
                CommandRisk::Low,
            ),
            Self::new(
                AgentCapabilityKind::ProposeCommands,
                "Propose commands or app actions for later governed execution.",
                vec![
                    "app_id".to_string(),
                    "target_refs".to_string(),
                    "allowed_commands".to_string(),
                    "privacy_policy".to_string(),
                ],
                vec![
                    ResultField::ProposedCommands,
                    ResultField::Evidence,
                    ResultField::Uncertainty,
                    ResultField::AuditSummary,
                ],
                vec![
                    EffectClass::Inspect,
                    EffectClass::Draft,
                    EffectClass::Modify,
                    EffectClass::Delete,
                    EffectClass::Publish,
                    EffectClass::Execute,
                    EffectClass::CrossApp,
                ],
                CommandRisk::Medium,
            ),
            Self::new(
                AgentCapabilityKind::Delegate,
                "Delegate bounded subwork to another capability, skill package, or worker.",
                vec![
                    "app_id".to_string(),
                    "parent_agent_run_id".to_string(),
                    "delegation_target".to_string(),
                    "context_subset".to_string(),
                ],
                vec![
                    ResultField::Artifacts,
                    ResultField::Evidence,
                    ResultField::AuditSummary,
                ],
                vec![EffectClass::Delegate],
                CommandRisk::Medium,
            ),
        ]
    }
}

/// Effect categories used by Command Governance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectClass {
    /// Inspect state without mutation.
    Inspect,
    /// Create draft-only output.
    Draft,
    /// Modify bounded state or native resources.
    Modify,
    /// Delete state or native resources.
    Delete,
    /// Publish or externally expose state.
    Publish,
    /// Execute code, shell, process, tool, or extension behavior.
    Execute,
    /// Delegate work to another agent run, skill, worker, or app.
    Delegate,
    /// Write memory.
    Remember,
    /// Cross app boundaries.
    CrossApp,
}

/// Metadata about the guard expected or observed for effect execution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionGuardMetadata {
    /// Stable metadata id.
    pub id: ExecutionGuardId,
    /// Guard mechanism kind.
    pub kind: ExecutionGuardKind,
    /// Strength of the guard.
    pub strength: ExecutionGuardStrength,
    /// Human-readable or machine-readable target scope summary.
    pub target_scope: Option<String>,
    /// How auditable the guarded action is.
    pub auditability: Auditability,
}

/// Guard mechanisms that can constrain execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionGuardKind {
    /// OS sandbox such as Seatbelt or Landlock.
    OsSandbox,
    /// Workspace path guard.
    WorkspacePathGuard,
    /// App-defined object or domain guard.
    AppObjectGuard,
    /// Domain validator.
    DomainValidator,
    /// Human approval checkpoint.
    HumanApprovalGate,
    /// No known guard.
    None,
    /// Other named guard kind.
    Other(String),
}

/// Strength of an execution guard.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionGuardStrength {
    /// No guard applies.
    None,
    /// Guard is descriptive only.
    Advisory,
    /// Guard is best effort but not strict isolation.
    BestEffort,
    /// Guard is enforced by a trusted boundary.
    Enforced,
}

/// How well an effect or decision can be audited.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Auditability {
    /// No useful audit trail.
    None,
    /// Intent can be audited.
    IntentOnly,
    /// Intent and governance decision can be audited.
    Decision,
    /// Intent, decision, effect, and evidence can be audited.
    Full,
}

/// Privacy boundary requested by a Context Grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyPolicy {
    /// Content may be treated as public within the app's normal behavior.
    Public,
    /// Content is private to the owning app unless explicitly granted.
    AppPrivate,
    /// Content is private to the user.
    UserPrivate,
    /// Content is system-level private state.
    SystemPrivate,
}

/// Semantic reference to context granted by an app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextTargetRef {
    /// Object target.
    Object { id: ObjectId },
    /// Buffer target.
    Buffer { id: BufferId },
    /// View target.
    View { id: ViewId },
    /// Task target.
    Task { id: TaskId },
    /// Native authority target.
    Native { native_ref: NativeReference },
}

/// Bounded selection inside a context target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectionRange {
    /// Optional display label.
    pub label: Option<String>,
    /// Adapter-owned start anchor.
    pub start: Option<String>,
    /// Adapter-owned end anchor.
    pub end: Option<String>,
}

/// Selected target context granted to an Agent Run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextSelection {
    /// Target being selected.
    pub target: ContextTargetRef,
    /// Bounded selection range.
    pub range: SelectionRange,
}

/// Read grant for an Agent Run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextReadGrant {
    /// Granted read target.
    pub target: ContextTargetRef,
    /// Optional reason for audit and UI.
    pub reason: Option<String>,
}

/// Command grant for an Agent Run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AllowedCommandGrant {
    /// Optional known command id.
    pub command_id: Option<CommandId>,
    /// Stable command name when no command id exists yet.
    pub command_name: Option<String>,
    /// Target the command may apply to.
    pub target: Option<ContextTargetRef>,
    /// Effect classes this grant allows the run to propose or invoke.
    pub allowed_effect_classes: Vec<EffectClass>,
}

/// Evidence expected from an Agent Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRequirement {
    /// Whether citations are required for user-visible claims.
    pub require_citations: bool,
    /// Whether supporting evidence records are required.
    pub require_evidence: bool,
    /// Optional minimum number of evidence records.
    pub min_evidence_count: Option<u16>,
}

/// Typed app-provided input authority for an Agent Run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextGrant {
    /// Stable grant id.
    pub id: ContextGrantId,
    /// App that owns the grant.
    pub app_id: String,
    /// User-visible or app-visible task goal.
    pub task_goal: String,
    /// Targets granted to the run.
    pub target_refs: Vec<ContextTargetRef>,
    /// Bounded selections granted to the run.
    pub selections: Vec<ContextSelection>,
    /// Reads permitted by the grant.
    pub allowed_reads: Vec<ContextReadGrant>,
    /// Commands permitted for proposal or invocation.
    pub allowed_commands: Vec<AllowedCommandGrant>,
    /// Privacy boundary for granted context.
    pub privacy_policy: PrivacyPolicy,
    /// Evidence expected from the run.
    pub evidence_requirement: EvidenceRequirement,
    /// Expected result shape, if already allocated.
    pub result_contract_id: Option<ResultContractId>,
}

/// Result fields an app can request from an Agent Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultField {
    /// User-visible answer.
    Answer,
    /// Summary text or structured summary.
    Summary,
    /// Ordered plan.
    Plan,
    /// Citations for claims.
    Citations,
    /// Evidence references.
    Evidence,
    /// Proposed commands for later governed execution.
    ProposedCommands,
    /// Follow-up questions.
    FollowUpQuestions,
    /// Uncertainty or confidence notes.
    Uncertainty,
    /// Produced artifacts.
    Artifacts,
    /// Audit summary.
    AuditSummary,
}

/// Typed output contract requested from an Agent Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResultContract {
    /// Stable result contract id.
    pub id: ResultContractId,
    /// Requested result fields.
    pub fields: Vec<ResultField>,
    /// Whether partial results are acceptable.
    pub allow_partial: bool,
    /// Whether unsupported requested fields should be surfaced explicitly.
    pub report_unsupported_fields: bool,
}

/// Owner of a bounded Agent Run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRunOwner {
    /// Run is owned by an app.
    App { app_id: String },
    /// Run is owned by an object.
    Object { id: ObjectId },
    /// Run is owned by a task.
    Task { id: TaskId },
}

/// Coarse Agent Run lifecycle state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatus {
    /// Run is requested but not started.
    Pending,
    /// Run is executing.
    Running,
    /// Run is waiting for external input or approval.
    Yielded,
    /// Run completed successfully.
    Completed,
    /// Run completed with partial output.
    Partial,
    /// Run failed.
    Failed,
    /// Run was cancelled.
    Cancelled,
}

/// Semantic description of a bounded Agent Capability run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentRunDescriptor {
    /// Stable Agent Run id.
    pub id: AgentRunId,
    /// Capability being run.
    pub capability_id: AgentCapabilityDescriptorId,
    /// Run owner.
    pub owner: AgentRunOwner,
    /// Actor that requested or owns the run.
    pub actor_id: ActorId,
    /// Context grant used by the run.
    pub context_grant_id: ContextGrantId,
    /// Result contract requested from the run.
    pub result_contract_id: ResultContractId,
    /// Optional task projection id.
    pub task_id: Option<TaskId>,
    /// Current coarse lifecycle state.
    pub status: AgentRunStatus,
}

/// Audit reference with linked evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditReference {
    /// Stable audit record id.
    pub id: AuditRecordId,
    /// Evidence records supporting this audit entry.
    pub evidence_ids: Vec<EvidenceId>,
    /// Human-readable audit notes.
    pub notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        AgentCapabilityDescriptor, AgentCapabilityKind, AgentRunDescriptor, AgentRunOwner,
        AgentRunStatus, AllowedCommandGrant, Auditability, ContextGrant, ContextReadGrant,
        ContextTargetRef, EffectClass, EvidenceRequirement, ExecutionGuardKind,
        ExecutionGuardMetadata, ExecutionGuardStrength, PrivacyPolicy, ResultContract, ResultField,
        SelectionRange,
    };
    use crate::{
        ActorId, AgentRunId, CommandRisk, ContextGrantId, ExecutionGuardId, ObjectId,
        ResultContractId,
    };

    #[test]
    fn context_grant_and_result_contract_round_trip_through_json() {
        let object_id = ObjectId::new();
        let result_contract_id = ResultContractId::new();
        let grant = ContextGrant {
            id: ContextGrantId::new(),
            app_id: "updf".to_string(),
            task_goal: "Explain the selected paragraph.".to_string(),
            target_refs: vec![ContextTargetRef::Object { id: object_id }],
            selections: vec![super::ContextSelection {
                target: ContextTargetRef::Object { id: object_id },
                range: SelectionRange {
                    label: Some("paragraph 4".to_string()),
                    start: Some("p4:start".to_string()),
                    end: Some("p4:end".to_string()),
                },
            }],
            allowed_reads: vec![ContextReadGrant {
                target: ContextTargetRef::Object { id: object_id },
                reason: Some("selected content".to_string()),
            }],
            allowed_commands: vec![AllowedCommandGrant {
                command_id: None,
                command_name: Some("updf.create_note_draft".to_string()),
                target: Some(ContextTargetRef::Object { id: object_id }),
                allowed_effect_classes: vec![EffectClass::Draft],
            }],
            privacy_policy: PrivacyPolicy::AppPrivate,
            evidence_requirement: EvidenceRequirement {
                require_citations: true,
                require_evidence: true,
                min_evidence_count: Some(1),
            },
            result_contract_id: Some(result_contract_id),
        };
        let contract = ResultContract {
            id: result_contract_id,
            fields: vec![
                ResultField::Answer,
                ResultField::Citations,
                ResultField::Evidence,
                ResultField::AuditSummary,
            ],
            allow_partial: true,
            report_unsupported_fields: true,
        };

        let grant_json = serde_json::to_string(&grant).expect("serialize context grant");
        let contract_json = serde_json::to_string(&contract).expect("serialize result contract");
        let decoded_grant: ContextGrant =
            serde_json::from_str(&grant_json).expect("deserialize context grant");
        let decoded_contract: ResultContract =
            serde_json::from_str(&contract_json).expect("deserialize result contract");

        assert_eq!(decoded_grant, grant);
        assert_eq!(decoded_contract, contract);
    }

    #[test]
    fn v1_taxonomy_contains_expected_descriptor_ids() {
        let descriptors = AgentCapabilityDescriptor::v1_taxonomy();
        let ids = descriptors
            .iter()
            .map(|descriptor| descriptor.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "agent.explain",
                "agent.summarize",
                "agent.plan",
                "agent.propose_commands",
                "agent.delegate"
            ]
        );
        assert!(descriptors.iter().any(|descriptor| {
            descriptor.kind == AgentCapabilityKind::ProposeCommands
                && descriptor.default_command_risk == CommandRisk::Medium
                && descriptor
                    .allowed_effect_classes
                    .contains(&EffectClass::Execute)
        }));
    }

    #[test]
    fn agent_run_descriptor_links_grant_contract_actor_and_owner() {
        let owner_object_id = ObjectId::new();
        let context_grant_id = ContextGrantId::new();
        let result_contract_id = ResultContractId::new();
        let run = AgentRunDescriptor {
            id: AgentRunId::new(),
            capability_id: AgentCapabilityKind::Explain.descriptor_id(),
            owner: AgentRunOwner::Object {
                id: owner_object_id,
            },
            actor_id: ActorId::new(),
            context_grant_id,
            result_contract_id,
            task_id: None,
            status: AgentRunStatus::Pending,
        };

        assert_eq!(run.context_grant_id, context_grant_id);
        assert_eq!(run.result_contract_id, result_contract_id);
        assert!(matches!(run.owner, AgentRunOwner::Object { id } if id == owner_object_id));
    }

    #[test]
    fn execution_guard_metadata_records_strength_without_execution_backend() {
        let metadata = ExecutionGuardMetadata {
            id: ExecutionGuardId::new(),
            kind: ExecutionGuardKind::WorkspacePathGuard,
            strength: ExecutionGuardStrength::BestEffort,
            target_scope: Some("/workspace".to_string()),
            auditability: Auditability::Decision,
        };

        assert_eq!(metadata.kind, ExecutionGuardKind::WorkspacePathGuard);
        assert_eq!(metadata.strength, ExecutionGuardStrength::BestEffort);
        assert_eq!(metadata.auditability, Auditability::Decision);
    }
}
