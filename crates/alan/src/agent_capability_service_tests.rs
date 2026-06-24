use super::{
    AgentCapabilityCompatibilityAdapter, AgentRunEventStream, AgentRunEventStreamMode,
    AgentRunServiceEvent, AgentRunServiceEventKind, CurrentChildRunLifecycleEvent,
    CurrentChildRunStatus, CurrentExecutionOutput, CurrentToolCallStatus,
    ReadAgentRunEventsResponse, ResultContractFieldStatus, StartAgentRunRequest,
    legacy_daemon_session_paths, map_current_child_run_to_agent_run_event,
    map_current_event_to_agent_run_event, map_current_output_to_result_contract,
    map_start_request_to_current_execution_input,
};
use crate::agent_capability_service::AgentCapabilityService;
use alan_kernel::{
    ActorId, AgentCapabilityKind, AgentRunDescriptor, AgentRunId, AgentRunOwner, AgentRunStatus,
    AgentSessionReference, ContextGrant, ContextGrantId, ContextReadGrant, ContextTargetRef,
    EffectClass, EvidenceId, EvidenceRequirement, NativeReference, ObjectId, PrivacyPolicy,
    ResultContract, ResultContractId, ResultField,
};
use alan_protocol::{ContentPart, Event, EventEnvelope, Op, YieldKind};

#[test]
fn start_request_serializes_os_boundary_without_session_protocol() {
    let object_id = ObjectId::new();
    let result_contract = ResultContract {
        id: ResultContractId::new(),
        fields: vec![ResultField::Answer, ResultField::Evidence],
        allow_partial: true,
        report_unsupported_fields: true,
    };
    let request = StartAgentRunRequest {
        capability_id: AgentCapabilityKind::Explain.descriptor_id(),
        actor_id: ActorId::new(),
        owner: AgentRunOwner::Object { id: object_id },
        context_grant: ContextGrant {
            id: ContextGrantId::new(),
            app_id: "updf".to_string(),
            task_goal: "Explain selection.".to_string(),
            target_refs: vec![],
            selections: vec![],
            allowed_reads: vec![],
            allowed_commands: vec![],
            privacy_policy: PrivacyPolicy::AppPrivate,
            evidence_requirement: EvidenceRequirement {
                require_citations: true,
                require_evidence: true,
                min_evidence_count: Some(1),
            },
            result_contract_id: Some(result_contract.id),
        },
        result_contract,
        requested_effects: vec![EffectClass::Inspect],
        execution_guard: None,
        schedule_at_unix_ms: None,
    };

    let json = serde_json::to_string(&request).expect("serialize start request");

    assert!(json.contains("agent.explain"));
    assert!(!json.contains("session_id"));
}

#[test]
fn event_response_can_report_yield_and_completion() {
    let agent_run_id = AgentRunId::new();
    let result_contract_id = ResultContractId::new();
    let response = ReadAgentRunEventsResponse {
        events: vec![
            AgentRunServiceEvent {
                agent_run_id,
                sequence: 1,
                kind: AgentRunServiceEventKind::Yielded {
                    checkpoint_id: "approval-1".to_string(),
                    reason: "approval required".to_string(),
                    payload: serde_json::json!({"command": "publish"}),
                },
                evidence_ids: vec![],
            },
            AgentRunServiceEvent {
                agent_run_id,
                sequence: 2,
                kind: AgentRunServiceEventKind::Completed { result_contract_id },
                evidence_ids: vec![],
            },
        ],
        latest_sequence: Some(2),
        terminal: true,
    };

    assert_eq!(response.events.len(), 2);
    assert!(response.terminal);
}

#[test]
fn stream_handle_uses_agent_run_identity() {
    let agent_run = AgentRunDescriptor {
        id: AgentRunId::new(),
        capability_id: AgentCapabilityKind::Plan.descriptor_id(),
        owner: AgentRunOwner::App {
            app_id: "alan-agent".to_string(),
        },
        actor_id: ActorId::new(),
        context_grant_id: ContextGrantId::new(),
        result_contract_id: ResultContractId::new(),
        task_id: None,
        status: AgentRunStatus::Pending,
    };
    let stream = AgentRunEventStream {
        agent_run_id: agent_run.id,
        mode: AgentRunEventStreamMode::Internal,
        cursor: None,
    };

    assert_eq!(stream.agent_run_id, agent_run.id);
}

#[test]
fn context_grant_maps_to_current_execution_input_for_v1_capabilities() {
    for capability in [
        AgentCapabilityKind::Explain,
        AgentCapabilityKind::Summarize,
        AgentCapabilityKind::Plan,
        AgentCapabilityKind::ProposeCommands,
    ] {
        let request = fixture_start_request(capability);
        let input = map_start_request_to_current_execution_input(&request).expect("map input");
        let Op::Turn { parts, .. } = input.submission.op else {
            panic!("expected turn submission");
        };
        let [ContentPart::Text { text }] = parts.as_slice() else {
            panic!("expected single text part");
        };

        assert!(text.contains("Agent Capability request"));
        assert!(text.contains(request.capability_id.as_str()));
        assert!(text.contains("goal: Explain selected code."));
        assert_eq!(input.context_summary.allowed_read_count, 1);
        assert!(input.unsupported_context_fields.is_empty());
        assert!(matches!(
            input.session_plan,
            super::CompatibilityAgentSessionPlan::Attach { .. }
        ));
    }
}

#[test]
fn current_events_yields_tools_and_child_runs_map_to_agent_run_events() {
    let agent_run_id = AgentRunId::new();
    let result_contract_id = ResultContractId::new();
    let yielded = map_current_event_to_agent_run_event(
        agent_run_id,
        result_contract_id,
        &envelope(
            7,
            Event::Yield {
                request_id: "approval-1".to_string(),
                kind: YieldKind::Confirmation,
                payload: serde_json::json!({"question": "Proceed?"}),
            },
        ),
    );
    let tool = map_current_event_to_agent_run_event(
        agent_run_id,
        result_contract_id,
        &envelope(
            8,
            Event::ToolCallCompleted {
                id: "tool-1".to_string(),
                name: Some("bash".to_string()),
                success: Some(true),
                result_preview: Some("ok".to_string()),
                presentation: None,
                audit: None,
            },
        ),
    );
    let evidence_id = EvidenceId::new();
    let child = map_current_child_run_to_agent_run_event(
        agent_run_id,
        9,
        CurrentChildRunLifecycleEvent {
            child_run_id: "child-1".to_string(),
            status: CurrentChildRunStatus::Completed,
            summary: Some("subtask complete".to_string()),
            evidence_ids: vec![evidence_id],
        },
    );

    assert!(matches!(
        yielded.kind,
        AgentRunServiceEventKind::Yielded { checkpoint_id, .. } if checkpoint_id == "approval-1"
    ));
    assert!(matches!(
        tool.kind,
        AgentRunServiceEventKind::ToolCall {
            tool_call_id,
            status: CurrentToolCallStatus::Completed,
            ..
        } if tool_call_id == "tool-1"
    ));
    assert!(matches!(
        child.kind,
        AgentRunServiceEventKind::ChildRun {
            child_run_id,
            status: CurrentChildRunStatus::Completed,
            ..
        } if child_run_id == "child-1"
    ));
    assert_eq!(child.evidence_ids, vec![evidence_id]);
}

#[test]
fn result_contract_mapping_reports_partial_and_unsupported_fields() {
    let evidence_id = EvidenceId::new();
    let contract = ResultContract {
        id: ResultContractId::new(),
        fields: vec![
            ResultField::Answer,
            ResultField::Citations,
            ResultField::Evidence,
            ResultField::ProposedCommands,
        ],
        allow_partial: true,
        report_unsupported_fields: true,
    };
    let output = CurrentExecutionOutput {
        answer: Some("The selected code parses config.".to_string()),
        evidence_ids: vec![evidence_id],
        partial: true,
        ..CurrentExecutionOutput::default()
    };

    let report = map_current_output_to_result_contract(&contract, &output);

    assert!(report.partial);
    assert_eq!(
        report.unsupported_fields,
        vec![ResultField::Citations, ResultField::ProposedCommands]
    );
    assert!(report.fields.iter().any(|field| {
        field.field == ResultField::Answer && field.status == ResultContractFieldStatus::Satisfied
    }));
    assert!(report.fields.iter().any(|field| {
        field.field == ResultField::ProposedCommands
            && field.status == ResultContractFieldStatus::Unsupported
    }));
    assert_eq!(report.evidence_ids, vec![evidence_id]);
}

#[test]
fn disabled_adapter_preserves_legacy_daemon_session_paths() {
    let adapter = AgentCapabilityCompatibilityAdapter::disabled();
    let request = fixture_start_request(AgentCapabilityKind::Explain);
    let error = adapter
        .start_run(request)
        .expect_err("disabled adapter should reject Agent Capability start");
    let paths = legacy_daemon_session_paths();

    assert!(matches!(
        error,
        super::AgentCapabilityServiceError::Adapter(message)
            if message.contains("disabled")
    ));
    assert!(paths.contains(&crate::daemon::api_contract::paths::SESSIONS));
    assert!(paths.contains(&crate::daemon::api_contract::paths::SESSION_SUBMIT));
    assert!(paths.contains(&crate::daemon::api_contract::paths::SESSION_RECONNECT_SNAPSHOT));
    assert!(
        paths
            .iter()
            .all(|path| !path.contains("agent_capabilities"))
    );
}

#[test]
fn compatibility_adapter_start_exposes_session_native_reference_not_run_identity() {
    let adapter = AgentCapabilityCompatibilityAdapter::new();
    let response = adapter
        .start_run(fixture_start_request(AgentCapabilityKind::Explain))
        .expect("start run");
    let input = adapter
        .current_execution_input(response.agent_run.id)
        .expect("mapped input");
    let implementation = response.implementation.expect("implementation ref");

    assert_eq!(response.event_stream.agent_run_id, response.agent_run.id);
    assert_ne!(
        response.agent_run.id.to_string(),
        match &implementation.native_ref {
            NativeReference::AgentSession(session) => session.session_id.clone(),
            _ => String::new(),
        }
    );
    assert!(matches!(
        input.session_plan,
        super::CompatibilityAgentSessionPlan::Attach { .. }
    ));
}

fn fixture_start_request(kind: AgentCapabilityKind) -> StartAgentRunRequest {
    let object_id = ObjectId::new();
    let result_contract = ResultContract {
        id: ResultContractId::new(),
        fields: vec![ResultField::Answer, ResultField::Evidence],
        allow_partial: true,
        report_unsupported_fields: true,
    };
    let session_ref = NativeReference::AgentSession(AgentSessionReference {
        adapter: "daemon".to_string(),
        session_id: "session-existing".to_string(),
    });

    StartAgentRunRequest {
        capability_id: kind.descriptor_id(),
        actor_id: ActorId::new(),
        owner: AgentRunOwner::Object { id: object_id },
        context_grant: ContextGrant {
            id: ContextGrantId::new(),
            app_id: "alan-agent".to_string(),
            task_goal: "Explain selected code.".to_string(),
            target_refs: vec![ContextTargetRef::Native {
                native_ref: session_ref.clone(),
            }],
            selections: vec![],
            allowed_reads: vec![ContextReadGrant {
                target: ContextTargetRef::Native {
                    native_ref: session_ref,
                },
                reason: Some("existing session transcript".to_string()),
            }],
            allowed_commands: vec![],
            privacy_policy: PrivacyPolicy::AppPrivate,
            evidence_requirement: EvidenceRequirement {
                require_citations: false,
                require_evidence: true,
                min_evidence_count: Some(1),
            },
            result_contract_id: Some(result_contract.id),
        },
        result_contract,
        requested_effects: vec![EffectClass::Inspect],
        execution_guard: None,
        schedule_at_unix_ms: None,
    }
}

fn envelope(sequence: u64, event: Event) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("evt_{sequence:016}"),
        sequence,
        session_id: "session-existing".to_string(),
        submission_id: Some("submission-1".to_string()),
        turn_id: "turn-000001".to_string(),
        item_id: format!("item-000001-{sequence:04}"),
        timestamp_ms: 1_772_000_000_000 + sequence,
        event,
    }
}
