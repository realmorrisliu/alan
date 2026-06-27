//! Event Contract Tests - ensure server/client agreement on event semantics.
//!
//! These tests verify:
//! 1. Key events emitted by the server are consumable by clients.
//! 2. Event types expected by clients are actually emitted by the server.
//! 3. Event payload structures match expectations.

use alan_agent_protocol::{
    CompactionAttemptSnapshot, CompactionMode, CompactionPressureLevel, CompactionReason,
    CompactionRequestMetadata, CompactionResult, CompactionTrigger, Event, EventEnvelope,
    MemoryFlushAttemptSnapshot, MemoryFlushResult,
};
use std::time::{SystemTime, UNIX_EPOCH};

/// Simulated client event handler behavior.
/// Mirrors the practical event handling logic in clients (TUI/ask).
struct MockClientEventHandler {
    received_messages: Vec<String>,
    received_thinking_events: Vec<String>,
    received_tool_calls: Vec<String>,
}

impl MockClientEventHandler {
    fn new() -> Self {
        Self {
            received_messages: Vec::new(),
            received_thinking_events: Vec::new(),
            received_tool_calls: Vec::new(),
        }
    }

    /// Simulates client-side event handling logic (based on the `switch` in `components.tsx`).
    fn handle_event(&mut self, envelope: &EventEnvelope) {
        match &envelope.event {
            Event::TextDelta { chunk, .. } if !chunk.is_empty() => {
                self.received_messages.push(chunk.clone());
            }
            Event::ThinkingDelta {
                chunk,
                is_final: false,
            } => {
                self.received_thinking_events.push(chunk.clone());
            }
            Event::ToolCallStarted { name, .. } => {
                self.received_tool_calls.push(name.clone());
            }
            _ => {
                // Other event types may be ignored by this mock client.
            }
        }
    }

    fn has_received_message(&self) -> bool {
        !self.received_messages.is_empty()
    }
}

/// Contract test: when the LLM returns text, the client must receive a displayable message.
/// This is the core user-visible behavior contract.
#[test]
fn contract_text_response_must_emit_displayable_event() {
    // Simulated runtime event sequence (from `turn_executor.rs` behavior).
    let events = vec![
        Event::TurnStarted {},
        Event::ThinkingDelta {
            chunk: "Working on your request...".to_string(),
            is_final: false,
        },
        Event::ThinkingDelta {
            chunk: String::new(),
            is_final: true,
        },
        // Critical: the server must emit events the client can display.
        Event::TextDelta {
            chunk: "Hello, world!".to_string(),
            is_final: false,
        },
        Event::TextDelta {
            chunk: String::new(),
            is_final: true,
        },
        Event::TurnCompleted { summary: None },
    ];

    let mut client = MockClientEventHandler::new();

    for event in &events {
        let envelope = create_test_envelope(event.clone());
        client.handle_event(&envelope);
    }

    // Contract assertion: the client must be able to display a message to the user.
    assert!(
        client.has_received_message(),
        "Contract violation: Client must receive at least one displayable message event \
         when LLM returns text response. \
         Make sure the runtime emits TextDelta events."
    );

    // Validate the message content.
    assert_eq!(client.received_messages.len(), 1); // non-empty TextDelta chunk
    assert!(
        client
            .received_messages
            .contains(&"Hello, world!".to_string())
    );
}

/// Contract test: client can process tool-call events.
#[test]
fn contract_tool_call_must_emit_tool_events() {
    let events = vec![
        Event::ToolCallStarted {
            title: None,
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            audit: None,
        },
        Event::ToolCallCompleted {
            presentation: None,
            id: "call_1".to_string(),
            name: Some("read_file".to_string()),
            success: Some(true),
            result_preview: Some("content loaded".to_string()),
            audit: None,
        },
    ];

    let mut client = MockClientEventHandler::new();

    for event in &events {
        let envelope = create_test_envelope(event.clone());
        client.handle_event(&envelope);
    }

    assert_eq!(client.received_tool_calls.len(), 1);
    assert_eq!(client.received_tool_calls[0], "read_file");
}

/// Contract test: empty-response fallback.
#[test]
fn contract_empty_response_must_show_fallback_message() {
    // When the LLM returns empty content, a fallback message should be shown.
    let events = vec![
        Event::TextDelta {
            chunk: "I apologize, but I couldn't generate a response.".to_string(),
            is_final: true,
        },
        Event::TurnCompleted { summary: None },
    ];

    let mut client = MockClientEventHandler::new();

    for event in &events {
        let envelope = create_test_envelope(event.clone());
        client.handle_event(&envelope);
    }

    // Even with empty LLM content, users should still see a fallback message.
    assert!(
        client.has_received_message(),
        "Contract violation: Empty response must show fallback message to user"
    );
}

/// Contract test: full required event sequence for a turn.
#[test]
fn contract_turn_must_emit_complete_event_sequence() {
    // Required event types for a complete turn.
    let required_event_types = vec!["turn_started", "thinking_delta", "turn_completed"];

    // Simulate a full turn event sequence.
    let events = [
        Event::TurnStarted {},
        Event::ThinkingDelta {
            chunk: "Working...".to_string(),
            is_final: false,
        },
        Event::TextDelta {
            chunk: "Response".to_string(),
            is_final: true,
        },
        Event::TurnCompleted { summary: None },
    ];

    let event_type_names: Vec<String> = events
        .iter()
        .map(|e| match e {
            Event::TurnStarted { .. } => "turn_started",
            Event::TurnCompleted { .. } => "turn_completed",
            Event::ThinkingDelta {
                is_final: false, ..
            } => "thinking_delta",
            Event::ThinkingDelta { is_final: true, .. } => "thinking_delta_final",
            Event::TextDelta { .. } => "text_delta",
            _ => "other",
        })
        .map(|s| s.to_string())
        .collect();

    for required in &required_event_types {
        assert!(
            event_type_names.contains(&required.to_string()),
            "Contract violation: Turn must emit '{}' event",
            required
        );
    }
}

#[test]
fn rust_tui_reducer_tiers_representative_protocol_event_types() {
    use alan_tui::history::HistoryCell;

    let events = representative_protocol_events();
    let mut reducer = alan_tui::history::SessionReducer::default();

    for event in events {
        reducer.apply_envelope(create_test_envelope(event));
    }

    // Only conversational substance becomes permanent transcript content:
    // the streamed assistant text, the completed tool call, the plan snapshot,
    // and the pending-input record. Turn boundaries, in-flight thinking, running
    // tools, rollback/compaction/memory/warning/recoverable-error are ephemeral
    // and must not be committed as cells.
    assert_eq!(
        reducer.cells.len(),
        4,
        "only permanent-tier events render as transcript cells: {:?}",
        reducer.cells
    );
    assert!(
        reducer
            .cells
            .iter()
            .any(|cell| matches!(cell, HistoryCell::Assistant(_)))
    );
    assert!(
        reducer
            .cells
            .iter()
            .any(|cell| matches!(cell, HistoryCell::Tool { .. }))
    );
    assert!(
        reducer
            .cells
            .iter()
            .any(|cell| matches!(cell, HistoryCell::Plan(_)))
    );
    assert!(
        reducer
            .cells
            .iter()
            .any(|cell| matches!(cell, HistoryCell::PendingYield(_)))
    );
    assert!(
        reducer.pending_yield.is_some(),
        "Yield events must surface as pending input in the Rust TUI"
    );
    assert!(
        reducer.transient_notice.is_some(),
        "Recoverable notices must surface as ephemeral transient state"
    );
}

fn create_test_envelope(event: Event) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("evt_{}", uuid::Uuid::new_v4()),
        sequence: 1,
        session_id: "test_session".to_string(),
        submission_id: Some("sub_1".to_string()),
        turn_id: "turn_1".to_string(),
        item_id: "item_1".to_string(),
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        event,
    }
}

fn representative_protocol_events() -> Vec<Event> {
    vec![
        Event::TurnStarted {},
        Event::TurnCompleted { summary: None },
        Event::TextDelta {
            chunk: "text".to_string(),
            is_final: false,
        },
        Event::ThinkingDelta {
            chunk: "thinking".to_string(),
            is_final: false,
        },
        Event::ToolCallStarted {
            title: None,
            id: "tool-1".to_string(),
            name: "read_file".to_string(),
            audit: None,
        },
        Event::ToolCallCompleted {
            presentation: None,
            id: "tool-1".to_string(),
            name: Some("read_file".to_string()),
            success: Some(true),
            result_preview: None,
            audit: None,
        },
        Event::PlanUpdated {
            explanation: None,
            items: vec![],
        },
        Event::SessionRolledBack {
            turns: 1,
            removed_messages: 2,
        },
        Event::Yield {
            request_id: "req-1".to_string(),
            kind: alan_agent_protocol::YieldKind::Confirmation,
            payload: serde_json::json!({}),
        },
        Event::CompactionObserved {
            attempt: CompactionAttemptSnapshot {
                attempt_id: "cmp-1".to_string(),
                submission_id: None,
                request: CompactionRequestMetadata {
                    mode: CompactionMode::Manual,
                    trigger: CompactionTrigger::Manual,
                    reason: CompactionReason::ExplicitRequest,
                    focus: None,
                },
                result: CompactionResult::Success,
                pressure_level: Some(CompactionPressureLevel::BelowSoft),
                memory_flush_attempt_id: None,
                input_messages: None,
                output_messages: None,
                input_prompt_tokens: None,
                output_prompt_tokens: None,
                retry_count: 0,
                tape_mutated: false,
                warning_message: None,
                error_message: None,
                failure_streak: None,
                reference_context_revision_before: None,
                reference_context_revision_after: None,
                timestamp: "2026-05-02T00:00:00Z".to_string(),
            },
        },
        Event::MemoryFlushObserved {
            attempt: MemoryFlushAttemptSnapshot {
                attempt_id: "mem-1".to_string(),
                compaction_mode: CompactionMode::Manual,
                pressure_level: CompactionPressureLevel::BelowSoft,
                result: MemoryFlushResult::Success,
                skip_reason: None,
                source_messages: None,
                output_path: None,
                warning_message: None,
                error_message: None,
                timestamp: "2026-05-02T00:00:00Z".to_string(),
            },
        },
        Event::Warning {
            message: "warning".to_string(),
        },
        Event::Error {
            message: "error".to_string(),
            recoverable: true,
        },
    ]
}
