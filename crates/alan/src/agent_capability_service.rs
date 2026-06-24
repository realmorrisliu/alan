//! Host Service API boundary for Agent Capability execution.
//!
//! This module defines the internal Rust API first. Daemon HTTP/WebSocket routes
//! can wrap this boundary later without making the initial Agent Capability
//! surface an external protocol commitment.

use alan_kernel::{
    ActorId, AgentCapabilityDescriptorId, AgentCapabilityKind, AgentRunDescriptor, AgentRunId,
    AgentRunOwner, AgentRunStatus, AgentSessionReference, AuditReference, ContextGrant,
    ContextGrantId, ContextTargetRef, EffectClass, EvidenceId, ExecutionGuardMetadata,
    NativeReference, PrivacyPolicy, ResultContract, ResultContractId, ResultField, TaskId,
};
use alan_protocol::{
    ContentPart, Event, EventEnvelope, Op, PlanItemStatus, Submission, ToolResultPresentation,
    YieldKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;
use thiserror::Error;

/// Result type returned by Agent Capability Service operations.
pub type AgentCapabilityServiceResult<T> = Result<T, AgentCapabilityServiceError>;

/// Internal Host Service API for bounded Agent Capability runs.
///
/// The first implementation is expected to adapt the existing Agent Execution
/// Engine and daemon-backed session APIs. This trait intentionally exposes
/// Agent Run semantics rather than raw session protocol.
pub trait AgentCapabilityService {
    /// Starts or schedules a bounded Agent Run.
    fn start_run(
        &self,
        request: StartAgentRunRequest,
    ) -> AgentCapabilityServiceResult<StartAgentRunResponse>;

    /// Reads lifecycle and output events for a bounded Agent Run.
    fn read_events(
        &self,
        request: ReadAgentRunEventsRequest,
    ) -> AgentCapabilityServiceResult<ReadAgentRunEventsResponse>;

    /// Resumes a yielded Agent Run.
    fn resume_run(
        &self,
        request: ResumeAgentRunRequest,
    ) -> AgentCapabilityServiceResult<ResumeAgentRunResponse>;

    /// Cancels a pending, running, or yielded Agent Run.
    fn cancel_run(
        &self,
        request: CancelAgentRunRequest,
    ) -> AgentCapabilityServiceResult<CancelAgentRunResponse>;

    /// Records completion from an implementation adapter.
    fn complete_run(
        &self,
        request: CompleteAgentRunRequest,
    ) -> AgentCapabilityServiceResult<CompleteAgentRunResponse>;
}

/// Request to start or schedule an Agent Run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartAgentRunRequest {
    /// Capability descriptor being invoked.
    pub capability_id: AgentCapabilityDescriptorId,
    /// Actor requesting the run.
    pub actor_id: ActorId,
    /// Run owner.
    pub owner: AgentRunOwner,
    /// Typed context grant supplied by the app.
    pub context_grant: ContextGrant,
    /// Typed result contract requested by the app.
    pub result_contract: ResultContract,
    /// Requested effect classes for governance and audit.
    pub requested_effects: Vec<EffectClass>,
    /// Requested or expected execution guard metadata, if known at start.
    pub execution_guard: Option<ExecutionGuardMetadata>,
    /// Optional schedule instant in Unix milliseconds.
    pub schedule_at_unix_ms: Option<i64>,
}

/// Response after accepting an Agent Run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartAgentRunResponse {
    /// Semantic Agent Run descriptor.
    pub agent_run: AgentRunDescriptor,
    /// Stream or polling handle for lifecycle events.
    pub event_stream: AgentRunEventStream,
    /// Adapter-owned implementation binding, if a compatibility path is used.
    pub implementation: Option<AgentCapabilityImplementationRef>,
    /// Audit entry created for the start decision, if available.
    pub audit: Option<AuditReference>,
}

/// Adapter-owned implementation binding for an Agent Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentCapabilityImplementationRef {
    /// Native authority used by the compatibility adapter.
    pub native_ref: NativeReference,
    /// Whether the adapter will create or attach a daemon-backed session.
    pub session_plan: CompatibilityAgentSessionPlan,
}

/// How the compatibility adapter plans to use current daemon-backed sessions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompatibilityAgentSessionPlan {
    /// Attach to an existing daemon-backed Alan Agent session.
    Attach {
        /// Existing adapter-owned session reference.
        session: AgentSessionReference,
    },
    /// Create a daemon-backed Alan Agent session through existing session APIs.
    Create {
        /// Optional workspace hint derived from granted native context.
        workspace_hint: Option<String>,
        /// Optional agent name.
        agent_name: Option<String>,
    },
}

impl CompatibilityAgentSessionPlan {
    /// Returns the native reference that preserves current daemon session authority.
    #[must_use]
    pub fn native_ref(&self) -> NativeReference {
        match self {
            Self::Attach { session } => NativeReference::AgentSession(session.clone()),
            Self::Create { .. } => NativeReference::AgentSession(AgentSessionReference {
                adapter: "alan-agent-compat".to_string(),
                session_id: "pending-create".to_string(),
            }),
        }
    }
}

/// Event stream handle for an Agent Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentRunEventStream {
    /// Agent Run being observed.
    pub agent_run_id: AgentRunId,
    /// Stream mode exposed by this Host Service implementation.
    pub mode: AgentRunEventStreamMode,
    /// Optional cursor for polling or resumable streaming.
    pub cursor: Option<String>,
}

/// Supported event stream modes for the internal Host Service boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunEventStreamMode {
    /// In-process event channel or event log.
    Internal,
    /// Polling over stored lifecycle events.
    Polling,
    /// Future daemon stream wrapper.
    DaemonStream,
}

/// Request to read Agent Run lifecycle events.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadAgentRunEventsRequest {
    /// Agent Run to read.
    pub agent_run_id: AgentRunId,
    /// Optional sequence cursor. When omitted, read from implementation default.
    pub after_sequence: Option<u64>,
    /// Maximum event count requested.
    pub limit: Option<u16>,
}

/// Response containing Agent Run lifecycle events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadAgentRunEventsResponse {
    /// Events returned by the service.
    pub events: Vec<AgentRunServiceEvent>,
    /// Latest sequence visible to this read.
    pub latest_sequence: Option<u64>,
    /// Whether the run is in a terminal state.
    pub terminal: bool,
}

/// Agent Run lifecycle event emitted by the Host Service boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentRunServiceEvent {
    /// Agent Run that produced this event.
    pub agent_run_id: AgentRunId,
    /// Monotonic sequence within the run stream.
    pub sequence: u64,
    /// Event kind.
    pub kind: AgentRunServiceEventKind,
    /// Evidence linked to this event, if any.
    pub evidence_ids: Vec<EvidenceId>,
}

/// Event kinds exposed by Agent Capability Service.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRunServiceEventKind {
    /// Run started.
    Started { status: AgentRunStatus },
    /// Human-readable or machine-readable progress message.
    Progress { message: String },
    /// Run yielded for input, approval, or external tool result.
    Yielded {
        checkpoint_id: String,
        reason: String,
        payload: Value,
    },
    /// Yielded run resumed.
    Resumed { checkpoint_id: String },
    /// Partial structured output was produced.
    Output {
        /// Result fields represented by this output event.
        fields: Vec<ResultField>,
        /// Bounded structured payload from the compatibility adapter.
        #[serde(default)]
        payload: Value,
    },
    /// Tool call lifecycle observed from current execution.
    ToolCall {
        /// Current engine tool call id.
        tool_call_id: String,
        /// Tool name.
        name: String,
        /// Tool call status.
        status: CurrentToolCallStatus,
        /// Optional title or result summary.
        summary: Option<String>,
    },
    /// Child-run lifecycle observed from current execution.
    ChildRun {
        /// Current engine child-run id.
        child_run_id: String,
        /// Child-run status.
        status: CurrentChildRunStatus,
        /// Optional child-run summary.
        summary: Option<String>,
    },
    /// Run completed with a result contract.
    Completed {
        result_contract_id: ResultContractId,
    },
    /// Run completed partially with unsupported fields.
    Partial {
        result_contract_id: ResultContractId,
        unsupported_fields: Vec<ResultField>,
    },
    /// Run failed.
    Failed { message: String },
    /// Run was cancelled.
    Cancelled { reason: Option<String> },
}

/// Current tool call status observed by the compatibility adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrentToolCallStatus {
    /// Tool call started.
    Started,
    /// Tool call completed successfully or without an explicit failure signal.
    Completed,
    /// Tool call failed.
    Failed,
}

/// Request to resume a yielded Agent Run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResumeAgentRunRequest {
    /// Agent Run to resume.
    pub agent_run_id: AgentRunId,
    /// Yield checkpoint being resumed.
    pub checkpoint_id: String,
    /// Structured resume payload.
    pub payload: Value,
    /// Audit context for the resume decision, if available.
    pub audit: Option<AuditReference>,
}

/// Response after resuming an Agent Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResumeAgentRunResponse {
    /// Agent Run resumed.
    pub agent_run_id: AgentRunId,
    /// Status after accepting the resume.
    pub status: AgentRunStatus,
}

/// Request to cancel an Agent Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CancelAgentRunRequest {
    /// Agent Run to cancel.
    pub agent_run_id: AgentRunId,
    /// Optional reason for audit and UI.
    pub reason: Option<String>,
}

/// Response after cancelling an Agent Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CancelAgentRunResponse {
    /// Agent Run cancelled.
    pub agent_run_id: AgentRunId,
    /// Status after accepting cancellation.
    pub status: AgentRunStatus,
}

/// Request from an adapter to record Agent Run completion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentRunRequest {
    /// Agent Run being completed.
    pub agent_run_id: AgentRunId,
    /// Result contract associated with completion.
    pub result_contract_id: ResultContractId,
    /// Terminal status.
    pub status: AgentRunStatus,
    /// Fields requested but not structurally supported by the adapter.
    pub unsupported_fields: Vec<ResultField>,
    /// Evidence linked to completion.
    pub evidence_ids: Vec<EvidenceId>,
    /// Completion audit reference, if available.
    pub audit: Option<AuditReference>,
}

/// Response after recording completion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompleteAgentRunResponse {
    /// Completed Agent Run.
    pub agent_run_id: AgentRunId,
    /// Terminal status.
    pub status: AgentRunStatus,
}

/// Errors returned by the Agent Capability Service boundary.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum AgentCapabilityServiceError {
    /// Agent Run was not found by the Host Service implementation.
    #[error("agent run not found: {0}")]
    RunNotFound(AgentRunId),
    /// The run exists but cannot accept the requested operation in its state.
    #[error("agent run {agent_run_id} is in invalid state {status:?} for {operation}")]
    InvalidState {
        /// Agent Run id.
        agent_run_id: AgentRunId,
        /// Current status.
        status: AgentRunStatus,
        /// Requested operation.
        operation: &'static str,
    },
    /// Context Grant referenced by the request is unavailable or rejected.
    #[error("context grant rejected: {0}")]
    ContextGrantRejected(ContextGrantId),
    /// Requested capability is unsupported by this implementation.
    #[error("unsupported capability: {0}")]
    UnsupportedCapability(AgentCapabilityDescriptorId),
    /// Result Contract cannot be satisfied structurally.
    #[error("result contract unsupported: {0}")]
    UnsupportedResultContract(ResultContractId),
    /// Implementation adapter failed.
    #[error("adapter error: {0}")]
    Adapter(String),
}

/// Configuration for the compatibility Agent Capability adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentCapabilityCompatibilityConfig {
    /// Whether the adapter accepts Agent Capability requests.
    pub enabled: bool,
}

impl Default for AgentCapabilityCompatibilityConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Compatibility adapter over the current Alan Agent execution/session machinery.
///
/// This first slice keeps the adapter in-process and fixture-friendly. It maps
/// Agent Capability requests into the existing `alan_protocol::Submission`
/// shape and records Agent Run lifecycle events without adding daemon routes.
#[derive(Debug, Default)]
pub struct AgentCapabilityCompatibilityAdapter {
    config: AgentCapabilityCompatibilityConfig,
    runs: Mutex<BTreeMap<AgentRunId, CompatibilityRunRecord>>,
}

impl AgentCapabilityCompatibilityAdapter {
    /// Creates an enabled compatibility adapter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an adapter with explicit configuration.
    #[must_use]
    pub fn with_config(config: AgentCapabilityCompatibilityConfig) -> Self {
        Self {
            config,
            runs: Mutex::new(BTreeMap::new()),
        }
    }

    /// Creates a disabled adapter that leaves legacy daemon/TUI session paths untouched.
    #[must_use]
    pub fn disabled() -> Self {
        Self::with_config(AgentCapabilityCompatibilityConfig { enabled: false })
    }

    /// Returns the current execution input mapped for a started run.
    pub fn current_execution_input(
        &self,
        agent_run_id: AgentRunId,
    ) -> AgentCapabilityServiceResult<CurrentAgentExecutionInput> {
        let runs = self
            .runs
            .lock()
            .map_err(|_| AgentCapabilityServiceError::Adapter("run store poisoned".to_string()))?;
        runs.get(&agent_run_id)
            .map(|record| record.execution_input.clone())
            .ok_or(AgentCapabilityServiceError::RunNotFound(agent_run_id))
    }
}

impl AgentCapabilityService for AgentCapabilityCompatibilityAdapter {
    fn start_run(
        &self,
        request: StartAgentRunRequest,
    ) -> AgentCapabilityServiceResult<StartAgentRunResponse> {
        if !self.config.enabled {
            return Err(AgentCapabilityServiceError::Adapter(
                "agent capability compatibility adapter is disabled".to_string(),
            ));
        }

        let execution_input = map_start_request_to_current_execution_input(&request)?;
        let run_status = if request.schedule_at_unix_ms.is_some() {
            AgentRunStatus::Pending
        } else {
            AgentRunStatus::Running
        };
        let agent_run = AgentRunDescriptor {
            id: AgentRunId::new(),
            capability_id: request.capability_id.clone(),
            owner: request.owner.clone(),
            actor_id: request.actor_id,
            context_grant_id: request.context_grant.id,
            result_contract_id: request.result_contract.id,
            task_id: Some(TaskId::new()),
            status: run_status.clone(),
        };
        let implementation = AgentCapabilityImplementationRef {
            native_ref: execution_input.session_plan.native_ref(),
            session_plan: execution_input.session_plan.clone(),
        };
        let start_event = AgentRunServiceEvent {
            agent_run_id: agent_run.id,
            sequence: 1,
            kind: AgentRunServiceEventKind::Started { status: run_status },
            evidence_ids: Vec::new(),
        };
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AgentCapabilityServiceError::Adapter("run store poisoned".to_string()))?;
        runs.insert(
            agent_run.id,
            CompatibilityRunRecord {
                descriptor: agent_run.clone(),
                execution_input,
                events: vec![start_event],
            },
        );

        let agent_run_id = agent_run.id;

        Ok(StartAgentRunResponse {
            agent_run,
            event_stream: AgentRunEventStream {
                agent_run_id,
                mode: AgentRunEventStreamMode::Internal,
                cursor: None,
            },
            implementation: Some(implementation),
            audit: None,
        })
    }

    fn read_events(
        &self,
        request: ReadAgentRunEventsRequest,
    ) -> AgentCapabilityServiceResult<ReadAgentRunEventsResponse> {
        let runs = self
            .runs
            .lock()
            .map_err(|_| AgentCapabilityServiceError::Adapter("run store poisoned".to_string()))?;
        let record =
            runs.get(&request.agent_run_id)
                .ok_or(AgentCapabilityServiceError::RunNotFound(
                    request.agent_run_id,
                ))?;
        let limit = usize::from(request.limit.unwrap_or(100));
        let events = record
            .events
            .iter()
            .filter(|event| {
                request
                    .after_sequence
                    .is_none_or(|after| event.sequence > after)
            })
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();

        Ok(ReadAgentRunEventsResponse {
            latest_sequence: record.events.last().map(|event| event.sequence),
            terminal: is_terminal_agent_run_status(&record.descriptor.status),
            events,
        })
    }

    fn resume_run(
        &self,
        request: ResumeAgentRunRequest,
    ) -> AgentCapabilityServiceResult<ResumeAgentRunResponse> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AgentCapabilityServiceError::Adapter("run store poisoned".to_string()))?;
        let record =
            runs.get_mut(&request.agent_run_id)
                .ok_or(AgentCapabilityServiceError::RunNotFound(
                    request.agent_run_id,
                ))?;
        record.descriptor.status = AgentRunStatus::Running;
        record.push_event(AgentRunServiceEventKind::Resumed {
            checkpoint_id: request.checkpoint_id,
        });

        Ok(ResumeAgentRunResponse {
            agent_run_id: request.agent_run_id,
            status: AgentRunStatus::Running,
        })
    }

    fn cancel_run(
        &self,
        request: CancelAgentRunRequest,
    ) -> AgentCapabilityServiceResult<CancelAgentRunResponse> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AgentCapabilityServiceError::Adapter("run store poisoned".to_string()))?;
        let record =
            runs.get_mut(&request.agent_run_id)
                .ok_or(AgentCapabilityServiceError::RunNotFound(
                    request.agent_run_id,
                ))?;
        record.descriptor.status = AgentRunStatus::Cancelled;
        record.push_event(AgentRunServiceEventKind::Cancelled {
            reason: request.reason.clone(),
        });

        Ok(CancelAgentRunResponse {
            agent_run_id: request.agent_run_id,
            status: AgentRunStatus::Cancelled,
        })
    }

    fn complete_run(
        &self,
        request: CompleteAgentRunRequest,
    ) -> AgentCapabilityServiceResult<CompleteAgentRunResponse> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AgentCapabilityServiceError::Adapter("run store poisoned".to_string()))?;
        let record =
            runs.get_mut(&request.agent_run_id)
                .ok_or(AgentCapabilityServiceError::RunNotFound(
                    request.agent_run_id,
                ))?;
        record.descriptor.status = request.status.clone();
        let kind = if request.status == AgentRunStatus::Partial
            || !request.unsupported_fields.is_empty()
        {
            AgentRunServiceEventKind::Partial {
                result_contract_id: request.result_contract_id,
                unsupported_fields: request.unsupported_fields,
            }
        } else if request.status == AgentRunStatus::Failed {
            AgentRunServiceEventKind::Failed {
                message: "agent run failed".to_string(),
            }
        } else {
            AgentRunServiceEventKind::Completed {
                result_contract_id: request.result_contract_id,
            }
        };
        record.push_event_with_evidence(kind, request.evidence_ids);

        Ok(CompleteAgentRunResponse {
            agent_run_id: request.agent_run_id,
            status: request.status,
        })
    }
}

#[derive(Clone, Debug)]
struct CompatibilityRunRecord {
    descriptor: AgentRunDescriptor,
    execution_input: CurrentAgentExecutionInput,
    events: Vec<AgentRunServiceEvent>,
}

impl CompatibilityRunRecord {
    fn push_event(&mut self, kind: AgentRunServiceEventKind) {
        self.push_event_with_evidence(kind, Vec::new());
    }

    fn push_event_with_evidence(
        &mut self,
        kind: AgentRunServiceEventKind,
        evidence_ids: Vec<EvidenceId>,
    ) {
        let sequence = self.events.last().map_or(1, |event| event.sequence + 1);
        self.events.push(AgentRunServiceEvent {
            agent_run_id: self.descriptor.id,
            sequence,
            kind,
            evidence_ids,
        });
    }
}

/// Adapter-owned input for the current Agent Execution Engine.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurrentAgentExecutionInput {
    /// Capability being adapted.
    pub capability_id: AgentCapabilityDescriptorId,
    /// Session creation or attach plan for the current daemon-backed session APIs.
    pub session_plan: CompatibilityAgentSessionPlan,
    /// Current engine submission produced from the Context Grant.
    pub submission: Submission,
    /// Bounded summary of the granted context.
    pub context_summary: AgentCapabilityContextSummary,
    /// Context Grant fields this adapter could not structurally map yet.
    pub unsupported_context_fields: Vec<String>,
    /// Audit-facing notes produced during mapping.
    pub audit_notes: Vec<String>,
}

/// Bounded summary of Context Grant input passed into the current engine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentCapabilityContextSummary {
    /// App that owns the Context Grant.
    pub app_id: String,
    /// User-visible or app-visible task goal.
    pub task_goal: String,
    /// Privacy boundary for granted context.
    pub privacy_policy: PrivacyPolicy,
    /// Number of target refs granted.
    pub target_count: usize,
    /// Number of selections granted.
    pub selection_count: usize,
    /// Number of allowed reads granted.
    pub allowed_read_count: usize,
    /// Number of command grants supplied.
    pub allowed_command_count: usize,
}

/// Maps an Agent Capability request into the current Agent Execution Engine input shape.
pub fn map_start_request_to_current_execution_input(
    request: &StartAgentRunRequest,
) -> AgentCapabilityServiceResult<CurrentAgentExecutionInput> {
    ensure_supported_v1_capability(&request.capability_id)?;
    let session_plan = session_plan_from_context_grant(&request.context_grant);
    let text = build_current_engine_turn_text(request);
    let submission = Submission::new(Op::Turn {
        parts: vec![ContentPart::text(text)],
        context: None,
    });

    Ok(CurrentAgentExecutionInput {
        capability_id: request.capability_id.clone(),
        session_plan,
        submission,
        context_summary: AgentCapabilityContextSummary {
            app_id: request.context_grant.app_id.clone(),
            task_goal: request.context_grant.task_goal.clone(),
            privacy_policy: request.context_grant.privacy_policy.clone(),
            target_count: request.context_grant.target_refs.len(),
            selection_count: request.context_grant.selections.len(),
            allowed_read_count: request.context_grant.allowed_reads.len(),
            allowed_command_count: request.context_grant.allowed_commands.len(),
        },
        unsupported_context_fields: unsupported_context_fields(request),
        audit_notes: vec![
            "Context Grant translated internally for current Agent Execution Engine".to_string(),
            "Daemon-backed session identity remains a native reference".to_string(),
        ],
    })
}

/// Current execution output collected by the compatibility adapter.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CurrentExecutionOutput {
    /// Assistant answer text, if available.
    pub answer: Option<String>,
    /// Summary text, if available.
    pub summary: Option<String>,
    /// Structured plan payload, if available.
    pub plan: Option<Value>,
    /// Citation payloads, if available.
    pub citations: Vec<Value>,
    /// Evidence ids observed during execution.
    pub evidence_ids: Vec<EvidenceId>,
    /// Proposed command payloads, if available.
    pub proposed_commands: Vec<Value>,
    /// Follow-up questions, if available.
    pub follow_up_questions: Vec<String>,
    /// Uncertainty or confidence notes, if available.
    pub uncertainty: Option<String>,
    /// Artifact payloads, if available.
    pub artifacts: Vec<Value>,
    /// Audit summary, if available.
    pub audit_summary: Option<String>,
    /// Whether current execution produced only partial output.
    pub partial: bool,
}

/// Structural result report for a Result Contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResultContractReport {
    /// Result Contract being reported.
    pub result_contract_id: ResultContractId,
    /// Per-field output reports.
    pub fields: Vec<ResultContractFieldReport>,
    /// Requested fields this adapter cannot structurally satisfy yet.
    pub unsupported_fields: Vec<ResultField>,
    /// Whether the result is partial.
    pub partial: bool,
    /// Evidence linked to the result report.
    pub evidence_ids: Vec<EvidenceId>,
}

/// Per-field output report for a Result Contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResultContractFieldReport {
    /// Requested field.
    pub field: ResultField,
    /// Field status.
    pub status: ResultContractFieldStatus,
    /// Structured field value when available.
    pub value: Option<Value>,
}

/// Status for a Result Contract field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultContractFieldStatus {
    /// Field was structurally satisfied.
    Satisfied,
    /// Field was only partially satisfied.
    Partial,
    /// Field is unsupported by this compatibility slice.
    Unsupported,
}

/// Maps current execution output into a structural Result Contract report.
#[must_use]
pub fn map_current_output_to_result_contract(
    contract: &ResultContract,
    output: &CurrentExecutionOutput,
) -> ResultContractReport {
    let mut fields = Vec::new();
    let mut unsupported_fields = Vec::new();

    for field in &contract.fields {
        let value = value_for_result_field(field, output);
        let status = if value.is_some() {
            ResultContractFieldStatus::Satisfied
        } else if contract.report_unsupported_fields {
            unsupported_fields.push(field.clone());
            ResultContractFieldStatus::Unsupported
        } else {
            ResultContractFieldStatus::Partial
        };
        fields.push(ResultContractFieldReport {
            field: field.clone(),
            status,
            value,
        });
    }

    ResultContractReport {
        result_contract_id: contract.id,
        partial: output.partial || !unsupported_fields.is_empty(),
        fields,
        unsupported_fields,
        evidence_ids: output.evidence_ids.clone(),
    }
}

/// Current child-run status observed by the compatibility adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrentChildRunStatus {
    /// Child run started.
    Started,
    /// Child run is running.
    Running,
    /// Child run yielded.
    Yielded,
    /// Child run completed.
    Completed,
    /// Child run failed.
    Failed,
    /// Child run was cancelled.
    Cancelled,
}

/// Current child-run lifecycle record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CurrentChildRunLifecycleEvent {
    /// Current engine child-run id.
    pub child_run_id: String,
    /// Child-run status.
    pub status: CurrentChildRunStatus,
    /// Optional summary.
    pub summary: Option<String>,
    /// Evidence linked to the child run.
    pub evidence_ids: Vec<EvidenceId>,
}

/// Rollout evidence observed by the compatibility adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CurrentRolloutEvidence {
    /// Evidence id allocated by the semantic layer.
    pub evidence_id: EvidenceId,
    /// Evidence summary.
    pub summary: String,
}

/// Maps a current protocol event into an Agent Run lifecycle event.
#[must_use]
pub fn map_current_event_to_agent_run_event(
    agent_run_id: AgentRunId,
    result_contract_id: ResultContractId,
    envelope: &EventEnvelope,
) -> AgentRunServiceEvent {
    AgentRunServiceEvent {
        agent_run_id,
        sequence: envelope.sequence,
        kind: map_current_event_kind(result_contract_id, &envelope.event),
        evidence_ids: Vec::new(),
    }
}

/// Maps current child-run lifecycle into an Agent Run lifecycle event.
#[must_use]
pub fn map_current_child_run_to_agent_run_event(
    agent_run_id: AgentRunId,
    sequence: u64,
    child_run: CurrentChildRunLifecycleEvent,
) -> AgentRunServiceEvent {
    AgentRunServiceEvent {
        agent_run_id,
        sequence,
        kind: AgentRunServiceEventKind::ChildRun {
            child_run_id: child_run.child_run_id,
            status: child_run.status,
            summary: child_run.summary,
        },
        evidence_ids: child_run.evidence_ids,
    }
}

/// Maps rollout evidence into an Agent Run lifecycle event.
#[must_use]
pub fn map_rollout_evidence_to_agent_run_event(
    agent_run_id: AgentRunId,
    sequence: u64,
    evidence: CurrentRolloutEvidence,
) -> AgentRunServiceEvent {
    AgentRunServiceEvent {
        agent_run_id,
        sequence,
        kind: AgentRunServiceEventKind::Progress {
            message: format!("evidence observed: {}", evidence.summary),
        },
        evidence_ids: vec![evidence.evidence_id],
    }
}

/// Legacy daemon session endpoints preserved when the compatibility adapter is disabled.
#[must_use]
pub fn legacy_daemon_session_paths() -> &'static [&'static str] {
    &[
        crate::daemon::api_contract::paths::SESSIONS,
        crate::daemon::api_contract::paths::SESSION_READ,
        crate::daemon::api_contract::paths::SESSION_RECONNECT_SNAPSHOT,
        crate::daemon::api_contract::paths::SESSION_HISTORY,
        crate::daemon::api_contract::paths::SESSION_EVENTS_READ,
        crate::daemon::api_contract::paths::SESSION_SUBMIT,
        crate::daemon::api_contract::paths::SESSION_RESUME,
        crate::daemon::api_contract::paths::SESSION_COMPACT,
        crate::daemon::api_contract::paths::SESSION_ROLLBACK,
        crate::daemon::api_contract::paths::SESSION_EVENTS,
        crate::daemon::api_contract::paths::SESSION_WS,
    ]
}

fn ensure_supported_v1_capability(
    capability_id: &AgentCapabilityDescriptorId,
) -> AgentCapabilityServiceResult<()> {
    let supported = [
        AgentCapabilityKind::Explain.descriptor_id(),
        AgentCapabilityKind::Summarize.descriptor_id(),
        AgentCapabilityKind::Plan.descriptor_id(),
        AgentCapabilityKind::ProposeCommands.descriptor_id(),
    ];

    if supported.iter().any(|id| id == capability_id) {
        Ok(())
    } else {
        Err(AgentCapabilityServiceError::UnsupportedCapability(
            capability_id.clone(),
        ))
    }
}

fn session_plan_from_context_grant(grant: &ContextGrant) -> CompatibilityAgentSessionPlan {
    if let Some(session) = find_agent_session_ref(grant) {
        return CompatibilityAgentSessionPlan::Attach { session };
    }

    CompatibilityAgentSessionPlan::Create {
        workspace_hint: workspace_hint_from_context_grant(grant),
        agent_name: Some("default".to_string()),
    }
}

fn find_agent_session_ref(grant: &ContextGrant) -> Option<AgentSessionReference> {
    grant
        .target_refs
        .iter()
        .chain(grant.selections.iter().map(|selection| &selection.target))
        .chain(grant.allowed_reads.iter().map(|read| &read.target))
        .chain(
            grant
                .allowed_commands
                .iter()
                .filter_map(|command| command.target.as_ref()),
        )
        .find_map(|target| match native_ref_from_context_target(target) {
            Some(NativeReference::AgentSession(session)) => Some(session.clone()),
            _ => None,
        })
}

fn workspace_hint_from_context_grant(grant: &ContextGrant) -> Option<String> {
    grant
        .target_refs
        .iter()
        .chain(grant.allowed_reads.iter().map(|read| &read.target))
        .filter_map(native_ref_from_context_target)
        .find_map(|native_ref| match native_ref {
            NativeReference::File(file) => Some(file.path.clone()),
            NativeReference::GitRepository(git) => Some(git.worktree_path.clone()),
            _ => None,
        })
}

fn native_ref_from_context_target(target: &ContextTargetRef) -> Option<&NativeReference> {
    match target {
        ContextTargetRef::Native { native_ref } => Some(native_ref),
        _ => None,
    }
}

fn unsupported_context_fields(request: &StartAgentRunRequest) -> Vec<String> {
    let mut unsupported = Vec::new();

    if request.schedule_at_unix_ms.is_some() {
        unsupported.push("schedule_at_unix_ms".to_string());
    }

    if request
        .requested_effects
        .iter()
        .any(|effect| !matches!(effect, EffectClass::Inspect | EffectClass::Draft))
    {
        unsupported.push("non_inspect_or_draft_effects".to_string());
    }

    unsupported
}

fn build_current_engine_turn_text(request: &StartAgentRunRequest) -> String {
    let mut lines = vec![
        "Agent Capability request".to_string(),
        format!("capability: {}", request.capability_id),
        format!("app: {}", request.context_grant.app_id),
        format!("goal: {}", request.context_grant.task_goal),
        format!("privacy: {:?}", request.context_grant.privacy_policy),
        format!("targets: {}", request.context_grant.target_refs.len()),
        format!("selections: {}", request.context_grant.selections.len()),
        format!(
            "allowed_reads: {}",
            request.context_grant.allowed_reads.len()
        ),
        format!(
            "allowed_commands: {}",
            request.context_grant.allowed_commands.len()
        ),
        format!(
            "requested_result_fields: {:?}",
            request.result_contract.fields
        ),
    ];

    if request.capability_id == AgentCapabilityKind::ProposeCommands.descriptor_id() {
        lines.push("mode: propose commands; do not execute proposed commands".to_string());
    }

    lines.join("\n")
}

fn value_for_result_field(field: &ResultField, output: &CurrentExecutionOutput) -> Option<Value> {
    match field {
        ResultField::Answer => output.answer.clone().map(Value::String),
        ResultField::Summary => output.summary.clone().map(Value::String),
        ResultField::Plan => output.plan.clone(),
        ResultField::Citations if !output.citations.is_empty() => {
            Some(Value::Array(output.citations.clone()))
        }
        ResultField::Evidence if !output.evidence_ids.is_empty() => Some(Value::Array(
            output
                .evidence_ids
                .iter()
                .map(|id| Value::String(id.to_string()))
                .collect(),
        )),
        ResultField::ProposedCommands if !output.proposed_commands.is_empty() => {
            Some(Value::Array(output.proposed_commands.clone()))
        }
        ResultField::FollowUpQuestions if !output.follow_up_questions.is_empty() => {
            Some(Value::Array(
                output
                    .follow_up_questions
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ))
        }
        ResultField::Uncertainty => output.uncertainty.clone().map(Value::String),
        ResultField::Artifacts if !output.artifacts.is_empty() => {
            Some(Value::Array(output.artifacts.clone()))
        }
        ResultField::AuditSummary => output.audit_summary.clone().map(Value::String),
        _ => None,
    }
}

fn map_current_event_kind(
    result_contract_id: ResultContractId,
    event: &Event,
) -> AgentRunServiceEventKind {
    match event {
        Event::TurnStarted {} => AgentRunServiceEventKind::Started {
            status: AgentRunStatus::Running,
        },
        Event::TurnCompleted { .. } => AgentRunServiceEventKind::Completed { result_contract_id },
        Event::TextDelta { chunk, .. } => AgentRunServiceEventKind::Output {
            fields: vec![ResultField::Answer],
            payload: serde_json::json!({ "text": chunk }),
        },
        Event::ThinkingDelta { chunk, .. } => AgentRunServiceEventKind::Progress {
            message: format!("thinking: {chunk}"),
        },
        Event::ToolCallStarted {
            id, name, title, ..
        } => AgentRunServiceEventKind::ToolCall {
            tool_call_id: id.clone(),
            name: name.clone(),
            status: CurrentToolCallStatus::Started,
            summary: title.clone(),
        },
        Event::ToolCallCompleted {
            id,
            name,
            success,
            result_preview,
            presentation,
            ..
        } => AgentRunServiceEventKind::ToolCall {
            tool_call_id: id.clone(),
            name: name.clone().unwrap_or_else(|| "unknown".to_string()),
            status: if success == &Some(false) {
                CurrentToolCallStatus::Failed
            } else {
                CurrentToolCallStatus::Completed
            },
            summary: result_preview
                .clone()
                .or_else(|| presentation.as_ref().map(tool_presentation_summary)),
        },
        Event::PlanUpdated { explanation, items } => AgentRunServiceEventKind::Output {
            fields: vec![ResultField::Plan],
            payload: serde_json::json!({
                "explanation": explanation,
                "items": items.iter().map(|item| {
                    serde_json::json!({
                        "id": item.id,
                        "content": item.content,
                        "status": match item.status {
                            PlanItemStatus::Pending => "pending",
                            PlanItemStatus::InProgress => "in_progress",
                            PlanItemStatus::Completed => "completed",
                        }
                    })
                }).collect::<Vec<_>>()
            }),
        },
        Event::SessionRolledBack { turns, .. } => AgentRunServiceEventKind::Progress {
            message: format!("session rolled back {turns} turns"),
        },
        Event::Yield {
            request_id,
            kind,
            payload,
        } => AgentRunServiceEventKind::Yielded {
            checkpoint_id: request_id.clone(),
            reason: yield_reason(kind),
            payload: payload.clone(),
        },
        Event::CompactionObserved { .. } => AgentRunServiceEventKind::Progress {
            message: "context compaction observed".to_string(),
        },
        Event::MemoryFlushObserved { .. } => AgentRunServiceEventKind::Progress {
            message: "memory flush observed".to_string(),
        },
        Event::Warning { message } => AgentRunServiceEventKind::Progress {
            message: format!("warning: {message}"),
        },
        Event::Error {
            message,
            recoverable,
        } => {
            if *recoverable {
                AgentRunServiceEventKind::Progress {
                    message: format!("recoverable error: {message}"),
                }
            } else {
                AgentRunServiceEventKind::Failed {
                    message: message.clone(),
                }
            }
        }
    }
}

fn tool_presentation_summary(presentation: &ToolResultPresentation) -> String {
    match presentation {
        ToolResultPresentation::Diff { path, .. } => format!("diff: {path}"),
        ToolResultPresentation::FileContent { path, .. } => format!("file: {path}"),
        ToolResultPresentation::Command { cmdline, .. } => format!("command: {cmdline}"),
        ToolResultPresentation::Listing { rows } => format!("listing rows: {}", rows.len()),
        ToolResultPresentation::PlainText { body } => body.clone(),
    }
}

fn yield_reason(kind: &YieldKind) -> String {
    match kind {
        YieldKind::Confirmation => "confirmation required".to_string(),
        YieldKind::StructuredInput => "structured input required".to_string(),
        YieldKind::DynamicTool => "dynamic tool result required".to_string(),
        YieldKind::Custom(kind) => format!("custom yield required: {kind}"),
    }
}

fn is_terminal_agent_run_status(status: &AgentRunStatus) -> bool {
    matches!(
        status,
        AgentRunStatus::Completed
            | AgentRunStatus::Partial
            | AgentRunStatus::Failed
            | AgentRunStatus::Cancelled
    )
}

#[cfg(test)]
#[path = "agent_capability_service_tests.rs"]
mod tests;
