//! The agent-runtime projection (introduce-alan-kernel-runtime §3/§5/§6): the
//! engine's `EventEnvelope` stream is projected onto the agent file layout —
//! assistant text → `io/output`, tool calls → `actions/<id>/`, yields →
//! `requests/<id>/`, and every event → the aggregate `events` stream. This slice
//! projects the event alphabet only, so it is exercised with synthetic
//! envelopes and needs no live LLM.

use alan_agent_protocol::{Event, EventEnvelope, YieldKind};
use alan_agentfs::AgentFs;
use alan_ap::{Fid, FileServer, OpenMode};

fn env(sequence: u64, event: Event) -> EventEnvelope {
    EventEnvelope {
        event_id: sequence.to_string(),
        sequence,
        session_id: "sess-1".to_string(),
        submission_id: None,
        turn_id: "turn-1".to_string(),
        item_id: format!("item-{sequence}"),
        timestamp_ms: 0,
        event,
    }
}

/// Walk to `path`, open for read, and read it back as a UTF-8 string.
async fn read_text(fs: &AgentFs, path: &[&str], fid: Fid) -> String {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await.expect("walk");
    fs.open(fid, OpenMode::Read).await.expect("open");
    let bytes = fs.read(fid, 0, 65536).await.expect("read");
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn assistant_text_is_projected_to_io_output() {
    let fs = AgentFs::new();
    fs.ingest(env(
        1,
        Event::TextDelta {
            chunk: "Hello".into(),
            is_final: false,
        },
    ))
    .await;
    fs.ingest(env(
        2,
        Event::TextDelta {
            chunk: " world".into(),
            is_final: true,
        },
    ))
    .await;

    assert_eq!(
        read_text(&fs, &["io", "output"], Fid(1)).await,
        "Hello world"
    );
}

#[tokio::test]
async fn a_tool_call_is_projected_to_an_action_tree() {
    let fs = AgentFs::new();
    fs.ingest(env(
        1,
        Event::ToolCallStarted {
            id: "t1".into(),
            name: "read".into(),
            title: None,
            audit: None,
        },
    ))
    .await;
    fs.ingest(env(
        2,
        Event::ToolCallCompleted {
            id: "t1".into(),
            name: Some("read".into()),
            success: Some(true),
            result_preview: None,
            presentation: None,
            audit: None,
        },
    ))
    .await;

    assert_eq!(
        read_text(&fs, &["actions", "t1", "name"], Fid(1)).await,
        "read"
    );
    assert_eq!(
        read_text(&fs, &["actions", "t1", "status"], Fid(2))
            .await
            .trim(),
        "completed"
    );
}

#[tokio::test]
async fn a_failed_tool_call_records_failed_not_partial() {
    let fs = AgentFs::new();
    fs.ingest(env(
        1,
        Event::ToolCallStarted {
            id: "t9".into(),
            name: "bash".into(),
            title: None,
            audit: None,
        },
    ))
    .await;
    fs.ingest(env(
        2,
        Event::ToolCallCompleted {
            id: "t9".into(),
            name: Some("bash".into()),
            success: Some(false),
            result_preview: None,
            presentation: None,
            audit: None,
        },
    ))
    .await;

    assert_eq!(
        read_text(&fs, &["actions", "t9", "status"], Fid(1))
            .await
            .trim(),
        "failed"
    );
}

#[tokio::test]
async fn a_yield_is_projected_to_a_request_tree() {
    let fs = AgentFs::new();
    fs.ingest(env(
        1,
        Event::Yield {
            request_id: "r1".into(),
            kind: YieldKind::Confirmation,
            payload: serde_json::json!({"prompt": "approve?"}),
        },
    ))
    .await;

    assert_eq!(
        read_text(&fs, &["requests", "r1", "kind"], Fid(1))
            .await
            .trim(),
        "confirmation"
    );
    assert_eq!(
        read_text(&fs, &["requests", "r1", "status"], Fid(2))
            .await
            .trim(),
        "pending"
    );
}

#[tokio::test]
async fn the_events_stream_is_watchable_by_blocking_read() {
    use std::sync::Arc;
    use std::time::Duration;

    let fs = Arc::new(AgentFs::new());
    // A watcher tails `events` from the live edge; with no events yet, the read
    // blocks rather than returning empty (observation = blocking read).
    let watcher = fs.clone();
    let handle = tokio::spawn(async move {
        watcher
            .walk(Fid::ROOT, Fid(7), &["events".to_string()])
            .await
            .unwrap();
        watcher.open(Fid(7), OpenMode::Read).await.unwrap();
        watcher.read(Fid(7), 0, 65536).await.unwrap()
    });

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        !handle.is_finished(),
        "watcher should block until an event is projected"
    );

    fs.ingest(env(
        1,
        Event::TextDelta {
            chunk: "hi".into(),
            is_final: true,
        },
    ))
    .await;
    let record = tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .expect("watcher did not wake")
        .unwrap();
    assert!(
        String::from_utf8(record)
            .unwrap()
            .contains("\"sequence\":1")
    );
}

#[tokio::test]
async fn every_event_appends_a_record_to_the_aggregate_events_stream() {
    let fs = AgentFs::new();
    fs.ingest(env(1, Event::TurnStarted {})).await;
    fs.ingest(env(
        2,
        Event::TextDelta {
            chunk: "hi".into(),
            is_final: true,
        },
    ))
    .await;

    let events = read_text(&fs, &["events"], Fid(1)).await;
    let lines: Vec<&str> = events.lines().collect();
    assert_eq!(lines.len(), 2, "one record per ingested event");
    // Records are the serialized envelopes, so they carry the sequence.
    assert!(lines[0].contains("\"sequence\":1"));
    assert!(lines[1].contains("\"sequence\":2"));
}
