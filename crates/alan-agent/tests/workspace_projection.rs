use alan_agent::{
    ALAN_AGENT_APP_ID, AgentWorkspaceChildRunEvent, AgentWorkspaceChildRunStatus,
    AgentWorkspaceEffectKind, AgentWorkspaceEvidenceInput, AgentWorkspaceHydratedMessage,
    AgentWorkspaceMemoryObservation, AgentWorkspaceMemoryObservationKind, AgentWorkspaceObjectRole,
    AgentWorkspaceProjector, AgentWorkspaceRolloutRecord, AgentWorkspaceSessionMetadata,
    COMPATIBILITY_SESSION_ADAPTER,
};
use alan_kernel::{
    FileReference, KernelEventKind, NativeReference, ProjectionStore, TaskEventKind, ViewModel,
};
use alan_protocol::{
    Event, EventEnvelope, MemoryFlushAttemptSnapshot, MemoryFlushResult, PlanItem, PlanItemStatus,
    ToolResultPresentation, YieldKind,
};
use serde_json::json;

#[test]
fn session_metadata_projects_to_workspace_objects_and_commands() {
    let projector = AgentWorkspaceProjector::new(session_metadata());
    let model = projector.model();

    let session_object = model
        .objects
        .iter()
        .find(|object| {
            object.metadata.attributes["workspace_role"]
                == json!(AgentWorkspaceObjectRole::CompatibilitySession)
        })
        .expect("compatibility session object");
    assert_eq!(
        session_object.metadata.attributes["app_id"],
        ALAN_AGENT_APP_ID
    );
    assert!(matches!(
        session_object.native_ref.as_ref(),
        Some(NativeReference::AgentSession(session))
            if session.adapter == COMPATIBILITY_SESSION_ADAPTER && session.session_id == "sess-1"
    ));

    let command_names = model
        .commands
        .iter()
        .map(|command| command.name.as_str())
        .collect::<Vec<_>>();
    assert!(command_names.contains(&"agent.submit_turn"));
    assert!(command_names.contains(&"agent.resume_yield"));
    assert!(command_names.contains(&"agent.approve_command"));
    assert!(command_names.contains(&"agent.inspect_evidence"));

    let store = ProjectionStore::rebuild(projector.kernel_events());
    assert!(store.object(session_object.id).is_some());
    assert!(store.command_availability().iter().any(|entry| {
        entry
            .command_ids
            .contains(&projector.ids().commands.submit_turn)
    }));
}

#[test]
fn local_user_submission_and_hydrated_history_project_to_semantic_conversation() {
    let mut projector = AgentWorkspaceProjector::new(session_metadata());
    let initial_version = projector.snapshots().conversation.version;

    let empty_events = projector.apply_user_submission("");
    assert!(empty_events.is_empty());
    assert_eq!(projector.snapshots().conversation.version, initial_version);

    projector.apply_hydrated_message(AgentWorkspaceHydratedMessage {
        role: "user".to_string(),
        content: "prior question".to_string(),
        tool_name: None,
    });
    projector.apply_hydrated_message(AgentWorkspaceHydratedMessage {
        role: "assistant".to_string(),
        content: "prior answer".to_string(),
        tool_name: None,
    });
    projector.apply_hydrated_message(AgentWorkspaceHydratedMessage {
        role: "tool".to_string(),
        content: "read ok".to_string(),
        tool_name: Some("read_file".to_string()),
    });
    projector.apply_user_submission("new question");

    let snapshots = projector.snapshots();
    let ViewModel::Conversation(conversation) = snapshots.conversation.model else {
        panic!("conversation snapshot");
    };

    let block_text = conversation
        .blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>();
    assert!(block_text.contains(&"prior question"));
    assert!(block_text.contains(&"prior answer"));
    assert!(block_text.contains(&"read_file: read ok"));
    assert!(block_text.contains(&"new question"));
    assert!(projector.kernel_events().iter().any(|event| {
        matches!(&event.kind, KernelEventKind::CommandInvoked { invocation }
            if invocation.command_id == projector.ids().commands.submit_turn
                && invocation.arguments["text"] == "new question")
    }));
}

#[test]
fn protocol_events_project_to_conversation_task_form_and_evidence() {
    let mut projector = AgentWorkspaceProjector::new(session_metadata());
    projector.apply_envelope(&envelope(1, "turn-1", "item-1", Event::TurnStarted {}));
    projector.apply_envelope(&envelope(
        2,
        "turn-1",
        "item-2",
        Event::TextDelta {
            chunk: "Hello".to_string(),
            is_final: false,
        },
    ));
    projector.apply_envelope(&envelope(
        3,
        "turn-1",
        "tool-start",
        Event::ToolCallStarted {
            id: "tool-1".to_string(),
            name: "read_file".to_string(),
            title: Some("Read README".to_string()),
            audit: None,
        },
    ));
    projector.apply_envelope(&envelope(
        4,
        "turn-1",
        "tool-end",
        Event::ToolCallCompleted {
            id: "tool-1".to_string(),
            name: Some("read_file".to_string()),
            success: Some(true),
            result_preview: Some("README content".to_string()),
            presentation: Some(ToolResultPresentation::FileContent {
                path: "README.md".to_string(),
                lines: 12,
                truncated: false,
            }),
            audit: None,
        },
    ));
    projector.apply_envelope(&envelope(
        5,
        "turn-1",
        "yield",
        Event::Yield {
            request_id: "approval-1".to_string(),
            kind: YieldKind::Confirmation,
            payload: json!({
                "message": "Approve write?",
                "options": ["approve", "deny"],
                "default_option": "deny"
            }),
        },
    ));

    let snapshots = projector.snapshots();
    let ViewModel::Conversation(conversation) = snapshots.conversation.model else {
        panic!("conversation snapshot");
    };
    assert!(
        conversation
            .blocks
            .iter()
            .any(|block| block.text == "Hello")
    );
    assert!(
        conversation
            .blocks
            .iter()
            .any(|block| block.text == "Approve write?")
    );

    let ViewModel::TaskTree(task_tree) = snapshots.task_tree.model else {
        panic!("task tree snapshot");
    };
    assert_eq!(task_tree.roots.len(), 1);
    assert_eq!(task_tree.roots[0].children.len(), 1);
    assert_eq!(task_tree.roots[0].children[0].label, "Read README");

    let ViewModel::Form(form) = snapshots.approval_form.model else {
        panic!("approval form snapshot");
    };
    assert_eq!(form.title, "Yield approval-1");
    assert_eq!(form.fields[0].id, "choice");
    assert_eq!(form.fields[0].value, "deny");

    let ViewModel::ObjectList(evidence) = snapshots.evidence.model else {
        panic!("evidence snapshot");
    };
    assert!(
        evidence
            .objects
            .iter()
            .any(|item| item.title == "Tool result: read_file")
    );

    let store = ProjectionStore::rebuild(projector.kernel_events());
    assert!(!store.dirty_views().is_empty());
    assert!(projector.kernel_events().iter().any(
        |event| matches!(&event.kind, KernelEventKind::Task {
                event
            } if matches!(event.kind, TaskEventKind::Yielded { .. }))
    ));
}

#[test]
fn plan_and_memory_observations_project_to_artifacts_evidence_and_memory_review() {
    let mut projector = AgentWorkspaceProjector::new(session_metadata());
    projector.apply_envelope(&envelope(1, "turn-1", "item-1", Event::TurnStarted {}));
    projector.apply_envelope(&envelope(
        2,
        "turn-1",
        "plan",
        Event::PlanUpdated {
            explanation: Some("Next steps".to_string()),
            items: vec![PlanItem {
                id: "p1".to_string(),
                content: "Inspect files".to_string(),
                status: PlanItemStatus::InProgress,
            }],
        },
    ));
    projector.apply_envelope(&envelope(
        3,
        "turn-1",
        "memory",
        Event::MemoryFlushObserved {
            attempt: MemoryFlushAttemptSnapshot {
                attempt_id: "flush-1".to_string(),
                compaction_mode: alan_protocol::CompactionMode::Manual,
                pressure_level: alan_protocol::CompactionPressureLevel::Soft,
                result: MemoryFlushResult::Success,
                skip_reason: None,
                source_messages: Some(3),
                output_path: Some("/tmp/memory.md".to_string()),
                warning_message: None,
                error_message: None,
                timestamp: "2026-06-24T00:00:00Z".to_string(),
            },
        },
    ));

    let snapshots = projector.snapshots();
    let ViewModel::Conversation(conversation) = snapshots.conversation.model else {
        panic!("conversation snapshot");
    };
    assert!(
        conversation
            .blocks
            .iter()
            .any(|block| block.text.contains("Inspect files"))
    );

    let ViewModel::ObjectList(memory) = snapshots.memory_review.model else {
        panic!("memory review snapshot");
    };
    assert_eq!(memory.objects[0].title, "Memory flush succeeded");

    let store = ProjectionStore::rebuild(projector.kernel_events());
    assert!(
        projector
            .kernel_events()
            .iter()
            .any(|event| matches!(event.kind, KernelEventKind::ArtifactRecorded { .. }))
    );
    assert!(
        store
            .dirty_views()
            .contains_key(&projector.ids().surfaces.memory_review_view)
    );
}

#[test]
fn child_runs_memory_observations_and_rollout_records_project_to_workspace_evidence() {
    let mut projector = AgentWorkspaceProjector::new(session_metadata());
    projector.apply_envelope(&envelope(1, "turn-1", "item-1", Event::TurnStarted {}));
    projector.apply_child_run_event(AgentWorkspaceChildRunEvent {
        child_run_id: "child-1".to_string(),
        status: AgentWorkspaceChildRunStatus::Started,
        delegated_skill: Some("repo-coding".to_string()),
        summary: Some("Investigate bug".to_string()),
        evidence: vec![AgentWorkspaceEvidenceInput {
            title: "Child launch evidence".to_string(),
            native_ref: None,
            payload: json!({"child_run_id": "child-1"}),
        }],
    });
    projector.apply_child_run_event(AgentWorkspaceChildRunEvent {
        child_run_id: "child-1".to_string(),
        status: AgentWorkspaceChildRunStatus::Completed,
        delegated_skill: Some("repo-coding".to_string()),
        summary: Some("Bug isolated".to_string()),
        evidence: Vec::new(),
    });
    projector.apply_memory_observation(AgentWorkspaceMemoryObservation {
        kind: AgentWorkspaceMemoryObservationKind::Recall,
        title: "Memory recalled".to_string(),
        preview: Some("prior decision".to_string()),
        native_ref: None,
        payload: json!({"source": "workspace"}),
    });
    projector.apply_memory_observation(AgentWorkspaceMemoryObservation {
        kind: AgentWorkspaceMemoryObservationKind::Promotion,
        title: "Memory promoted".to_string(),
        preview: Some("new durable fact".to_string()),
        native_ref: Some(NativeReference::File(FileReference {
            path: "/workspace/.alan/memory.md".to_string(),
            version: Some("rev-1".to_string()),
        })),
        payload: json!({"target": "workspace"}),
    });
    projector.apply_rollout_record(AgentWorkspaceRolloutRecord::Artifact {
        title: "Rollout transcript".to_string(),
        native_ref: Some(NativeReference::File(FileReference {
            path: "/workspace/rollout.jsonl".to_string(),
            version: Some("sha256:abc".to_string()),
        })),
        payload: json!({"bytes": 120}),
    });
    projector.apply_rollout_record(AgentWorkspaceRolloutRecord::Effect {
        effect_id: "effect-1".to_string(),
        kind: AgentWorkspaceEffectKind::FileSystem,
        summary: "write file".to_string(),
        committed: true,
        native_refs: Vec::new(),
        payload: Some(json!({"path": "README.md"})),
    });
    projector.apply_rollout_record(AgentWorkspaceRolloutRecord::Checkpoint {
        checkpoint_id: "checkpoint-1".to_string(),
        title: "Approval checkpoint".to_string(),
        payload: json!({"request_id": "approval-1"}),
    });

    let snapshots = projector.snapshots();
    let ViewModel::TaskTree(task_tree) = snapshots.task_tree.model else {
        panic!("task tree snapshot");
    };
    assert_eq!(task_tree.roots[0].children[0].label, "Investigate bug");
    assert_eq!(task_tree.roots[0].children[0].status, "completed");

    let ViewModel::ObjectList(memory) = snapshots.memory_review.model else {
        panic!("memory review snapshot");
    };
    assert!(
        memory
            .objects
            .iter()
            .any(|item| item.title == "Memory recalled")
    );
    assert!(
        memory
            .objects
            .iter()
            .any(|item| item.title == "Memory promoted")
    );

    let ViewModel::ObjectList(evidence) = snapshots.evidence.model else {
        panic!("evidence snapshot");
    };
    assert!(
        evidence
            .objects
            .iter()
            .any(|item| item.title == "Approval checkpoint")
    );
    assert!(
        evidence
            .objects
            .iter()
            .any(|item| item.title == "Child launch evidence")
    );

    assert!(
        projector
            .kernel_events()
            .iter()
            .any(|event| matches!(event.kind, KernelEventKind::ArtifactRecorded { .. }))
    );
    assert!(projector.kernel_events().iter().any(|event| {
        matches!(&event.kind, KernelEventKind::Task {
            event
        } if matches!(event.kind, TaskEventKind::SideEffectCommitted { .. }))
    }));
    assert!(projector.kernel_events().iter().any(|event| {
        matches!(&event.kind, KernelEventKind::Task {
            event
        } if matches!(event.kind, TaskEventKind::Yielded { .. }))
    }));
}

fn session_metadata() -> AgentWorkspaceSessionMetadata {
    AgentWorkspaceSessionMetadata {
        session_id: "sess-1".to_string(),
        workspace_dir: Some("/workspace".to_string()),
        agent_name: Some("default".to_string()),
        profile_id: Some("chatgpt-main".to_string()),
        provider: Some("chatgpt".to_string()),
        resolved_model: Some("gpt-5.3-codex".to_string()),
    }
}

fn envelope(sequence: u64, turn_id: &str, item_id: &str, event: Event) -> EventEnvelope {
    EventEnvelope {
        event_id: sequence.to_string(),
        sequence,
        session_id: "sess-1".to_string(),
        submission_id: Some("sub-1".to_string()),
        turn_id: turn_id.to_string(),
        item_id: item_id.to_string(),
        timestamp_ms: sequence * 10,
        event,
    }
}
