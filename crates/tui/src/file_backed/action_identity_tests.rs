use super::*;

#[test]
fn changed_records_indices_and_same_id_in_new_root_remain_independent() {
    let mut app = FileBackedApp::new("/agent/one".into());
    let snapshot = |id: &str, output: &str| ActionSnapshot {
        id: id.into(),
        name: "read".into(),
        status: "completed".into(),
        output: output.into(),
        result: String::new(),
    };
    sync_action_snapshot(&mut app, snapshot("a", "old"));
    sync_action_snapshot(&mut app, snapshot("b", "second"));
    app.prune_rendered_prefix(app.render_opts(80), 3);
    assert_eq!(app.action_cells["b"], 0);
    sync_action_snapshot(&mut app, snapshot("b", "changed"));
    assert_eq!(app.action_cells["b"], 0);
    assert_eq!(app.transcript.len(), 1);
    app.drain_committed_scrollback(80, 1);
    sync_action_snapshot(&mut app, snapshot("b", "changed"));
    assert!(app.transcript.is_empty());
    sync_action_snapshot(&mut app, snapshot("b", "new evidence"));
    assert_eq!(app.transcript.len(), 1);
    app.reset_for_root_process_change();
    app.agent_path = "/agent/two".into();
    sync_action_snapshot(&mut app, snapshot("b", "new evidence"));
    assert_eq!(app.transcript.len(), 2);
    assert_eq!(app.action_cells["b"], 1);
}

#[tokio::test]
async fn invalid_partial_ranges_and_display_bound_never_claim_full_recovery() {
    let (shell, path, id) = action_fixture("界 retained original", "").await;
    for (offset, length, original, needle) in [
        (1, 2, 22, "not UTF-8"),
        (0, 999, 22, "range missing"),
        (0, 3, 22, "range only"),
        (0, 262145, 262145, "display bound"),
        (u64::MAX, 3, 3, "invalid range"),
    ] {
        let result = serde_json::json!({"type":"evidence_projection","preview":"x","reference":{"path":format!("{path}/actions/{id}/output"),"offset":offset,"length":length},"truncation":{"full_content_recoverable":true,"original_bytes":original,"preview_bytes":1}}).to_string();
        shell
            .write(&format!("{path}/actions/{id}/result"), result.as_bytes())
            .await
            .unwrap();
        let text = action_detail_io::read_detail(&shell, &path, &id)
            .await
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(needle), "{needle}: {text}");
        assert!(!text.contains("Recovered reference (retained original)"));
    }
}
