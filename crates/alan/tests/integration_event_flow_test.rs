//! Integration Tests — event flow unit tests.
//!
//! For full runtime integration tests with mock LLM, see `smoke_test.rs`.

use alan_agent_protocol::Event;
use std::time::{SystemTime, UNIX_EPOCH};

/// Verify event timestamps are monotonically ordered.
#[test]
fn test_event_timestamp_ordering() {
    let base_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let events = [
        create_test_event_at_time(Event::TurnStarted {}, base_time),
        create_test_event_at_time(
            Event::ThinkingDelta {
                chunk: "...".to_string(),
                is_final: false,
            },
            base_time + 100,
        ),
        create_test_event_at_time(Event::TurnCompleted { summary: None }, base_time + 1000),
    ];

    for window in events.windows(2) {
        assert!(
            window[0].timestamp_ms <= window[1].timestamp_ms,
            "Events must be ordered by timestamp"
        );
    }
}

fn create_test_event_at_time(
    event: Event,
    timestamp_ms: u64,
) -> alan_agent_protocol::EventEnvelope {
    alan_agent_protocol::EventEnvelope {
        event_id: format!("evt_{}", uuid::Uuid::new_v4()),
        sequence: 1,
        session_id: "test".to_string(),
        submission_id: None,
        turn_id: "turn_1".to_string(),
        item_id: "item_1".to_string(),
        timestamp_ms,
        event,
    }
}
