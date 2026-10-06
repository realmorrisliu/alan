use super::*;

#[tokio::test]
async fn downstream_drop_releases_silent_event_input() {
    let (input, events) = mpsc::channel(1);
    let output = project_events(events);
    drop(output);
    tokio::time::timeout(std::time::Duration::from_millis(500), input.closed())
        .await
        .expect("projector retained silent upstream after downstream drop");
}

#[tokio::test]
async fn standard_message_delta_refusal_preserves_partial_text_and_usage() {
    assert_standard_message_delta("refusal", "stream_error:safety").await;
}

#[tokio::test]
async fn standard_message_delta_end_turn_preserves_partial_text_and_usage() {
    assert_standard_message_delta("end_turn", "end_turn").await;
}

async fn assert_standard_message_delta(stop_reason: &str, expected: &str) {
    let (tx, input) = mpsc::channel(8);
    let (status_tx, status) = tokio::sync::oneshot::channel();
    let mut output = crate::failure::guard_stream(project_events(input), status);
    for value in [
        serde_json::json!({"type":"message_start","message":{"id":"msg-test","usage":{
            "input_tokens":11,"output_tokens":0,"cache_read_input_tokens":3,"cache_creation_input_tokens":2}}}),
        serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"partial"}}),
        serde_json::json!({"type":"message_delta","delta":{"stop_reason":stop_reason},"usage":{"output_tokens":7}}),
        serde_json::json!({"type":"message_stop"}),
    ] {
        let event = serde_json::from_value(value)
            .expect("standard output-only usage delta must deserialize");
        tx.send(event).await.unwrap();
    }
    drop(tx);
    status_tx.send(None).unwrap();
    let chunks = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut chunks = Vec::new();
        while let Some(chunk) = output.recv().await {
            chunks.push(chunk);
        }
        chunks
    })
    .await
    .unwrap();
    assert_eq!(
        chunks
            .iter()
            .filter_map(|c| c.text.as_deref())
            .collect::<String>(),
        "partial"
    );
    assert_eq!(chunks.iter().filter(|c| c.is_finished).count(), 1);
    let terminal = chunks.last().unwrap();
    assert_eq!(terminal.finish_reason.as_deref(), Some(expected));
    let usage = terminal.usage.as_ref().unwrap();
    assert_eq!(
        usage.prompt_tokens, 16,
        "absent delta input must preserve initial input and caches"
    );
    assert_eq!(usage.cached_prompt_tokens, Some(3));
    assert_eq!(usage.completion_tokens, 7);
    assert_eq!(usage.total_tokens, 23);
}

#[tokio::test]
async fn compatible_message_stop_message_preserves_end_turn() {
    let (tx, input) = mpsc::channel(1);
    let mut output = project_events(input);
    tx.send(serde_json::from_value(serde_json::json!({"type":"message_stop",
        "message":{"id":"msg-test","stop_reason":"end_turn","usage":{"input_tokens":4,"output_tokens":2}}})).unwrap()).await.unwrap();
    drop(tx);
    let terminal = output.recv().await.unwrap();
    assert!(terminal.is_finished);
    assert_eq!(terminal.finish_reason.as_deref(), Some("end_turn"));
    assert_eq!(terminal.usage.unwrap().total_tokens, 6);
    assert!(output.recv().await.is_none());
}
