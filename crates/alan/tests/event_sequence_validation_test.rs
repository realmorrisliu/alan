//! Event Sequence Validation Tests
//!
//! These tests validate event sequences produced by the runtime under
//! different scenarios and ensure clients can handle and display them.

use alan_agent_protocol::Event;

/// Scenario: normal text response
#[test]
fn sequence_text_response() {
    // Event sequence shape based on `run_turn_with_cancel` in `turn_executor.rs`.
    let expected_sequence = vec![
        EventPattern::new("turn_started").required(),
        EventPattern::new("thinking_delta").required(),
        EventPattern::new("thinking_delta_final").required(),
        // Message content can be one or more `TextDelta` chunks.
        EventPattern::new("text_delta").required(),
        EventPattern::new("turn_completed").required(),
    ];

    let actual_events = simulate_text_response_turn();

    validate_sequence(&actual_events, &expected_sequence);
}

/// Scenario: response with tool calls
#[test]
fn sequence_tool_call_response() {
    let expected_sequence = vec![
        EventPattern::new("turn_started").required(),
        EventPattern::new("thinking_delta").required(),
        EventPattern::new("thinking_delta_final").optional(),
        // Tool call events.
        EventPattern::new("tool_call_started").required(),
        EventPattern::new("tool_call_completed").required(),
        // There may be a final message after tool calls.
        EventPattern::new("text_delta").optional(),
        EventPattern::new("turn_completed").required(),
    ];

    let actual_events = simulate_tool_call_turn();

    validate_sequence(&actual_events, &expected_sequence);
}

/// Scenario: empty response fallback
#[test]
fn sequence_empty_response_fallback() {
    let expected_sequence = vec![
        EventPattern::new("turn_started").required(),
        EventPattern::new("thinking_delta").required(),
        EventPattern::new("thinking_delta_final").optional(),
        // Fallback message.
        EventPattern::new("text_delta").required(),
        EventPattern::new("turn_completed").required(),
    ];

    let actual_events = simulate_empty_fallback_turn();

    validate_sequence(&actual_events, &expected_sequence);
}

/// Event pattern definition
struct EventPattern {
    event_type: String,
    required: bool,
}

impl EventPattern {
    fn new(event_type: &str) -> Self {
        Self {
            event_type: event_type.to_string(),
            required: false,
        }
    }

    fn required(mut self) -> Self {
        self.required = true;
        self
    }

    #[allow(dead_code)]
    fn optional(mut self) -> Self {
        self.required = false;
        self
    }
}

/// Get event type as a string
fn get_event_type(event: &Event) -> String {
    match event {
        Event::TurnStarted { .. } => "turn_started",
        Event::TurnCompleted { .. } => "turn_completed",
        Event::ThinkingDelta {
            is_final: false, ..
        } => "thinking_delta",
        Event::ThinkingDelta { is_final: true, .. } => "thinking_delta_final",
        Event::TextDelta { .. } => "text_delta",
        Event::Yield { .. } => "yield",
        Event::ToolCallStarted { .. } => "tool_call_started",
        Event::ToolCallCompleted { .. } => "tool_call_completed",
        Event::PlanUpdated { .. } => "plan_updated",
        Event::SessionRolledBack { .. } => "session_rolled_back",
        Event::CompactionObserved { .. } => "compaction_observed",
        Event::MemoryFlushObserved { .. } => "memory_flush_observed",
        Event::Warning { .. } => "warning",
        Event::Error { .. } => "error",
    }
    .to_string()
}

/// Validate whether an event sequence matches the expected pattern
fn validate_sequence(events: &[Event], expected: &[EventPattern]) {
    let actual_types: Vec<String> = events.iter().map(get_event_type).collect();

    println!("Expected sequence:");
    for (i, pattern) in expected.iter().enumerate() {
        let req_marker = if pattern.required { "[R]" } else { "[O]" };
        println!("  {} {} {}", i, req_marker, pattern.event_type);
    }

    println!("\nActual sequence:");
    for (i, event_type) in actual_types.iter().enumerate() {
        println!("  {} {}", i, event_type);
    }

    // Check that required events exist.
    for pattern in expected.iter().filter(|p| p.required) {
        let found = actual_types.contains(&pattern.event_type);
        assert!(
            found,
            "Required event '{}' not found in sequence",
            pattern.event_type
        );
    }

    // Check order: required events should appear in the expected relative order.
    let expected_iter = expected.iter().filter(|p| p.required);
    let mut actual_iter = actual_types.iter();

    for expected_pattern in expected_iter {
        let found = actual_iter
            .by_ref()
            .any(|actual| *actual == expected_pattern.event_type);
        assert!(
            found,
            "Required event '{}' appears out of order or is missing",
            expected_pattern.event_type
        );
    }
}

// ==================== Simulation helpers ====================

/// Simulate event sequence for a normal text response
fn simulate_text_response_turn() -> Vec<Event> {
    vec![
        Event::TurnStarted {},
        Event::ThinkingDelta {
            chunk: "Working on your request...".to_string(),
            is_final: false,
        },
        Event::ThinkingDelta {
            chunk: String::new(),
            is_final: true,
        },
        Event::TextDelta {
            chunk: "Hello! ".to_string(),
            is_final: false,
        },
        Event::TextDelta {
            chunk: "How can I help you?".to_string(),
            is_final: false,
        },
        Event::TextDelta {
            chunk: String::new(),
            is_final: true,
        },
        Event::TurnCompleted {
            summary: Some("Task completed".to_string()),
        },
    ]
}

/// Simulate event sequence for a tool-call response
fn simulate_tool_call_turn() -> Vec<Event> {
    vec![
        Event::TurnStarted {},
        Event::ThinkingDelta {
            chunk: "I need to read a file...".to_string(),
            is_final: false,
        },
        Event::ThinkingDelta {
            chunk: String::new(),
            is_final: true,
        },
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
            result_preview: Some("file content".to_string()),
            audit: None,
        },
        Event::TextDelta {
            chunk: "I found the file content.".to_string(),
            is_final: true,
        },
        Event::TurnCompleted {
            summary: Some("Task completed".to_string()),
        },
    ]
}

/// Simulate event sequence for an empty-response fallback
fn simulate_empty_fallback_turn() -> Vec<Event> {
    vec![
        Event::TurnStarted {},
        Event::ThinkingDelta {
            chunk: "Working...".to_string(),
            is_final: false,
        },
        Event::ThinkingDelta {
            chunk: String::new(),
            is_final: true,
        },
        Event::TextDelta {
            chunk: "I apologize, but I couldn't generate a response.".to_string(),
            is_final: true,
        },
        Event::TurnCompleted {
            summary: Some("Turn completed with empty response fallback".to_string()),
        },
    ]
}
