use super::*;

#[test]
fn line_drain_keeps_partial_records_until_newline() {
    let mut pending = b"{\"role\":\"user\"}\n{\"role\"".to_vec();

    assert_eq!(
        drain_lines(&mut pending),
        vec![br#"{"role":"user"}"#.to_vec()]
    );
    assert_eq!(pending, b"{\"role\"");

    pending.extend_from_slice(b": \"assistant\"}\n");
    assert_eq!(
        drain_lines(&mut pending),
        vec![br#"{"role": "assistant"}"#.to_vec()]
    );
    assert!(pending.is_empty());
}

#[test]
fn snapshot_restores_the_latest_matching_turn_after_root_process_change() {
    let snapshot = stdio_task_snapshot_from_history(
        "same task",
        br#"{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"old answer"}
{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"intermediate response"}
{"version":1,"kind":"message","role":"assistant","content":"current answer"}
"#,
        br#"{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
    )
    .unwrap();

    assert!(snapshot.task_seen);
    assert_eq!(snapshot.assistant_answer.as_deref(), Some("current answer"));
    assert_eq!(snapshot.activity_state, Some(UiActivityState::Idle));
    assert!(snapshot.task_error.is_none());
}
