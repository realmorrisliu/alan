use super::*;
use serde_json::json;

#[test]
fn guardian_breaker_trips_on_three_consecutive_denials() {
    let mut state = AgentMachine::new();
    assert!(!state.record_guardian_review(true));
    assert!(!state.record_guardian_review(true));
    assert!(state.record_guardian_review(true)); // third consecutive trips
}

#[test]
fn guardian_breaker_resets_on_allow() {
    let mut state = AgentMachine::new();
    assert!(!state.record_guardian_review(true));
    assert!(!state.record_guardian_review(true));
    assert!(!state.record_guardian_review(false)); // reset
    assert!(!state.record_guardian_review(true));
    assert!(!state.record_guardian_review(true));
    assert!(state.record_guardian_review(true)); // three consecutive again
}

#[test]
fn guardian_breaker_trips_on_ten_denials_in_window() {
    let mut state = AgentMachine::new();
    // Interleave allow/deny so it never hits 3 consecutive, but reaches 10
    // denials within the rolling window.
    let mut tripped = false;
    for _ in 0..10 {
        state.record_guardian_review(false);
        tripped = state.record_guardian_review(true);
    }
    assert!(tripped);
}

#[test]
fn test_confirmation_set_and_pending() {
    let mut state = AgentMachine::new();
    state.set_confirmation(PendingConfirmation {
        checkpoint_id: "cp-1".to_string(),
        checkpoint_type: "tool_escalation".to_string(),
        summary: "Approve?".to_string(),
        details: json!({}),
        options: vec!["approve".to_string(), "reject".to_string()],
    });

    let latest = state.pending_confirmation().unwrap();
    assert_eq!(latest.checkpoint_id, "cp-1");

    // take_pending removes it
    let taken = state.take_pending("cp-1").unwrap();
    assert!(matches!(taken, PendingYield::Confirmation(_)));
    assert!(state.pending_confirmation().is_none());
}

#[test]
fn test_clear_resets_pending_interactions() {
    let mut state = AgentMachine::new();
    state.set_confirmation(PendingConfirmation {
        checkpoint_id: "cp".to_string(),
        checkpoint_type: "tool_escalation".to_string(),
        summary: "Approve?".to_string(),
        details: json!({}),
        options: vec!["approve".to_string()],
    });
    state.reset_turn();
    assert!(state.pending_confirmation().is_none());
    assert!(!state.has_pending_interaction());
    assert!(matches!(state.turn_activity(), TurnActivityState::Idle));
}

#[test]
fn test_turn_activity_state_roundtrip_and_clear() {
    let mut state = AgentMachine::new();
    assert!(matches!(state.turn_activity(), TurnActivityState::Idle));

    state.set_turn_activity(TurnActivityState::Running);
    assert!(matches!(state.turn_activity(), TurnActivityState::Running));

    state.set_turn_activity(TurnActivityState::Paused);
    assert!(matches!(state.turn_activity(), TurnActivityState::Paused));

    state.reset_turn();
    assert!(matches!(state.turn_activity(), TurnActivityState::Idle));
    assert_eq!(state.compactions_this_turn(), 0);
}

#[test]
fn test_clear_preserves_plan_snapshot() {
    let mut state = AgentMachine::new();
    state.set_plan_snapshot(
        Some("Keep the current plan".to_string()),
        vec![PlanItem {
            id: "plan-1".to_string(),
            content: "Run delegated review".to_string(),
            status: alan_agent_protocol::PlanItemStatus::InProgress,
        }],
    );

    state.reset_turn();

    let snapshot = state.plan_snapshot().expect("plan snapshot should persist");
    assert_eq!(
        snapshot.explanation.as_deref(),
        Some("Keep the current plan")
    );
    assert_eq!(snapshot.items.len(), 1);
}

#[test]
fn test_clear_plan_snapshot_removes_latest_plan() {
    let mut state = AgentMachine::new();
    state.set_plan_snapshot(
        Some("Drop the current plan".to_string()),
        vec![PlanItem {
            id: "plan-1".to_string(),
            content: "Cancelled work".to_string(),
            status: alan_agent_protocol::PlanItemStatus::Pending,
        }],
    );

    state.clear_plan_snapshot();

    assert!(state.plan_snapshot().is_none());
}

#[test]
fn test_active_skills_roundtrip_and_clear() {
    let mut state = AgentMachine::new();
    state.set_active_skills(vec![crate::skills::ActiveSkillEnvelope::available(
        crate::skills::SkillMetadata {
            id: "deploy".to_string(),
            package_id: Some("skill:deploy".to_string()),
            name: "Deploy".to_string(),
            description: "Deploy service".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/tmp/deploy/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: crate::skills::SkillScope::Descriptor,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: crate::skills::SkillContentSource::File(std::path::PathBuf::from(
                "/tmp/deploy/SKILL.md",
            )),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        crate::skills::SkillActivationReason::ExplicitMention {
            mention: "deploy".to_string(),
        },
    )]);

    assert_eq!(state.active_skills().len(), 1);
    assert_eq!(state.active_skills()[0].metadata.id, "deploy");

    state.reset_turn();
    assert!(state.active_skills().is_empty());
}

#[test]
fn test_active_turn_message_start_tracks_turn_start_and_compaction() {
    let mut state = AgentMachine::new();
    assert_eq!(state.active_turn_message_start(), None);

    state.begin_turn(5);
    assert_eq!(state.active_turn_message_start(), Some(5));

    state.note_tape_compaction(2);
    assert_eq!(state.active_turn_message_start(), Some(3));

    state.note_tape_compaction(10);
    assert_eq!(state.active_turn_message_start(), Some(0));

    state.reset_turn();
    assert_eq!(state.active_turn_message_start(), None);
}

#[test]
fn dropped_plan_boundary_does_not_become_active_after_compaction() {
    let mut state = AgentMachine::new();
    state.begin_turn(2);
    state.set_plan_snapshot(Some("old plan".to_string()), Vec::new());
    state.begin_turn(5);

    state.note_tape_compaction(5);

    assert_eq!(state.active_turn_message_start(), Some(0));
    assert!(!state.plan_snapshot_is_from_active_turn());
}

#[test]
fn test_auto_mid_turn_compaction_budget_and_growth_guard() {
    let mut state = AgentMachine::new();
    assert!(state.can_auto_mid_turn_compact(4_000, 8_192));

    state.record_auto_mid_turn_compaction(3_200);
    assert_eq!(state.compactions_this_turn(), 1);
    assert!(!state.can_auto_mid_turn_compact(3_300, 8_192));
    assert!(state.can_auto_mid_turn_compact(3_600, 8_192));

    state.record_auto_mid_turn_compaction(3_400);
    assert_eq!(state.compactions_this_turn(), 2);
    assert!(!state.can_auto_mid_turn_compact(3_700, 8_192));
    assert!(state.can_auto_mid_turn_compact(7_980, 8_192));

    state.reset_turn();
    assert!(state.can_auto_mid_turn_compact(4_000, 8_192));
}

#[test]
fn test_auto_mid_turn_compaction_emergency_helper() {
    assert!(is_auto_mid_turn_compaction_emergency(4_000, 4_128));
    assert!(!is_auto_mid_turn_compaction_emergency(4_000, 4_400));
    assert!(!is_auto_mid_turn_compaction_emergency(4_000, 0));
}

#[test]
fn test_latest_pending_key_tracks_cross_type_insertion_order() {
    let mut state = AgentMachine::new();
    state.set_confirmation(PendingConfirmation {
        checkpoint_id: "cp-1".to_string(),
        checkpoint_type: "manual".to_string(),
        summary: "Approve?".to_string(),
        details: json!({}),
        options: vec!["approve".to_string()],
    });
    assert_eq!(state.latest_pending_key().as_deref(), Some("cp-1"));

    state.set_structured_input(PendingStructuredInputRequest {
        request_id: "input-1".to_string(),
        title: "Input".to_string(),
        prompt: "Value?".to_string(),
        questions: vec![],
    });
    assert_eq!(state.latest_pending_key().as_deref(), Some("input-1"));

    let _ = state.take_pending("input-1");
    assert_eq!(state.latest_pending_key().as_deref(), Some("cp-1"));
}

#[test]
fn test_turn_state_buffers_inband_submissions_fifo() {
    let mut state = AgentMachine::new();
    state.push_buffered_inband_submission(Submission {
        intent: Default::default(),
        id: "s1".to_string(),
        op: alan_agent_protocol::Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("one")],
            mode: alan_agent_protocol::InputMode::Steer,
        },
    });
    state.push_buffered_inband_submission(Submission {
        intent: Default::default(),
        id: "s2".to_string(),
        op: alan_agent_protocol::Op::Resume {
            request_id: "latest".to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                serde_json::json!({"choice": "approve"}),
            )],
        },
    });

    assert_eq!(state.buffered_inband_user_input_count(), 1);
    assert_eq!(
        state
            .pop_buffered_inband_submission()
            .as_ref()
            .map(|s| s.id.as_str()),
        Some("s1")
    );
    assert_eq!(
        state
            .pop_buffered_inband_submission()
            .as_ref()
            .map(|s| s.id.as_str()),
        Some("s2")
    );
    assert!(state.pop_buffered_inband_submission().is_none());
}

#[test]
fn test_turn_state_drain_buffered_inband_submissions_preserves_order() {
    let mut state = AgentMachine::new();
    state.push_buffered_inband_submission(Submission {
        intent: Default::default(),
        id: "s1".to_string(),
        op: alan_agent_protocol::Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("one")],
            mode: alan_agent_protocol::InputMode::Steer,
        },
    });
    state.push_buffered_inband_submission(Submission {
        intent: Default::default(),
        id: "s2".to_string(),
        op: alan_agent_protocol::Op::Resume {
            request_id: "latest".to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                serde_json::json!({"choice": "approve"}),
            )],
        },
    });

    let drained = state.drain_buffered_inband_submissions();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained.front().map(|s| s.id.as_str()), Some("s1"));
    assert_eq!(drained.back().map(|s| s.id.as_str()), Some("s2"));
    assert!(state.pop_buffered_inband_submission().is_none());
}

#[test]
fn test_clear_buffered_inband_submissions_returns_count() {
    let mut state = AgentMachine::new();
    state.push_buffered_inband_submission(Submission {
        intent: Default::default(),
        id: "s1".to_string(),
        op: alan_agent_protocol::Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("one")],
            mode: alan_agent_protocol::InputMode::Steer,
        },
    });
    state.push_buffered_inband_submission(Submission {
        intent: Default::default(),
        id: "s2".to_string(),
        op: alan_agent_protocol::Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("two")],
            mode: alan_agent_protocol::InputMode::Steer,
        },
    });

    let count = state.clear_buffered_inband_submissions();
    assert_eq!(count, 2);
    assert!(state.pop_buffered_inband_submission().is_none());
}

#[test]
fn test_queue_next_turn_inputs_overflow_is_rejected() {
    let mut state = AgentMachine::new();
    for _ in 0..MAX_QUEUED_NEXT_TURN_INPUTS {
        assert!(
            state
                .queue_next_turn_input(vec![ContentPart::text("queued")])
                .is_some()
        );
    }
    assert!(
        state
            .queue_next_turn_input(vec![ContentPart::text("overflow")])
            .is_none()
    );
}

#[test]
fn test_tool_replay_batch_roundtrip() {
    let mut state = AgentMachine::new();
    let tool_calls = vec![
        NormalizedToolCall {
            id: "call-1".to_string(),
            name: "web_search".to_string(),
            arguments: json!({"query": "rust"}),
        },
        NormalizedToolCall {
            id: "call-2".to_string(),
            name: "memory_write".to_string(),
            arguments: json!({"key": "test", "value": "data"}),
        },
    ];

    state.set_tool_replay_batch("cp-1", tool_calls, true);

    let retrieved = state.take_tool_replay_batch("cp-1").unwrap();
    assert!(retrieved.resume_with_generation);
    assert_eq!(retrieved.tool_calls.len(), 2);
    assert_eq!(retrieved.tool_calls[0].id, "call-1");
    assert_eq!(retrieved.tool_calls[1].id, "call-2");

    // Should be removed after take
    assert!(state.take_tool_replay_batch("cp-1").is_none());
}
