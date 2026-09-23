use super::*;

#[test]
fn reattached_action_indices_follow_removed_error_cells() {
    let mut reattached = FileBackedApp::new("/agent/root".to_string());
    reattached.transcript = vec![
        HistoryCell::User("previous task".to_string()),
        HistoryCell::Assistant("previous answer".to_string()),
        HistoryCell::User("current task".to_string()),
    ];
    let previous_transcript = std::mem::take(&mut reattached.transcript);
    reattached.reset_for_root_process_change();

    reattached.transcript = vec![
        HistoryCell::User("current task".to_string()),
        HistoryCell::Error("recoverable provider failure".to_string()),
        HistoryCell::Tool {
            title: "bash".to_string(),
            status: ToolStatus::Complete,
            preview: Some("first result".to_string()),
            presentation: None,
        },
        HistoryCell::Assistant("current answer".to_string()),
    ];
    reattached.action_cells.insert("action-1".to_string(), 2);

    let current_transcript = std::mem::take(&mut reattached.transcript);
    reattached.transcript = previous_transcript;
    let current_transcript =
        remove_error_cells_and_remap_actions(current_transcript, &mut reattached.action_cells);

    assert!(reattached.merge_reconnected_history(current_transcript, "current task", 0));
    assert_eq!(reattached.action_cells.get("action-1"), Some(&3));

    reattached.upsert_action_cell(
        "action-1".to_string(),
        HistoryCell::Tool {
            title: "bash".to_string(),
            status: ToolStatus::Failed,
            preview: Some("updated result".to_string()),
            presentation: None,
        },
    );
    assert!(matches!(
        reattached.transcript.get(3),
        Some(HistoryCell::Tool {
            status: ToolStatus::Failed,
            ..
        })
    ));
    assert_eq!(
        reattached.transcript.last(),
        Some(&HistoryCell::Assistant("current answer".to_string()))
    );
}
