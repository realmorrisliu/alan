use super::FileBackedApp;
use crate::history::{HistoryCell, RenderOpts};
use std::collections::BTreeMap;

#[test]
fn idle_reconnect_matches_history_after_committed_scrollback_pruning() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript = vec![
        HistoryCell::Rendered(vec!["old prompt".to_string()]),
        HistoryCell::Rendered(vec!["old answer".to_string()]),
        HistoryCell::Rendered(vec!["retained prompt".to_string()]),
        HistoryCell::Rendered(vec!["retained answer".to_string()]),
    ];
    assert_eq!(app.prune_rendered_prefix(RenderOpts::new(80, false), 2), 2);
    app.action_cells = BTreeMap::from([
        ("before-match".to_string(), 1),
        ("shared".to_string(), 3),
        ("new".to_string(), 4),
    ]);

    app.merge_reconnected_idle_history(vec![
        HistoryCell::Rendered(vec!["old prompt".to_string()]),
        HistoryCell::Rendered(vec!["old answer".to_string()]),
        HistoryCell::Rendered(vec!["retained prompt".to_string()]),
        HistoryCell::Rendered(vec!["retained answer".to_string()]),
        HistoryCell::Rendered(vec!["new prompt".to_string()]),
    ]);

    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::Rendered(vec!["retained prompt".to_string()]),
            HistoryCell::Rendered(vec!["retained answer".to_string()]),
            HistoryCell::Rendered(vec!["new prompt".to_string()]),
        ]
    );
    assert_eq!(
        app.action_cells,
        BTreeMap::from([("shared".to_string(), 1), ("new".to_string(), 2)])
    );
}

#[test]
fn idle_reconnect_keeps_intervening_turns_when_retained_history_repeats() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript = vec![HistoryCell::Rendered(vec!["same turn".to_string()])];

    app.merge_reconnected_idle_history(vec![
        HistoryCell::Rendered(vec!["same turn".to_string()]),
        HistoryCell::Rendered(vec!["intervening turn".to_string()]),
        HistoryCell::Rendered(vec!["same turn".to_string()]),
        HistoryCell::Rendered(vec!["new turn".to_string()]),
    ]);

    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::Rendered(vec!["same turn".to_string()]),
            HistoryCell::Rendered(vec!["intervening turn".to_string()]),
            HistoryCell::Rendered(vec!["same turn".to_string()]),
            HistoryCell::Rendered(vec!["new turn".to_string()]),
        ]
    );
}

#[test]
fn idle_reconnect_matches_a_partially_pruned_assistant_cell() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    let full_response = "aaa bbb ccc ddd eee fff".to_string();
    app.transcript = vec![HistoryCell::Assistant(full_response.clone())];
    assert_eq!(app.prune_rendered_prefix(RenderOpts::new(16, false), 1), 1);
    let retained = app.transcript[0].clone();

    app.merge_reconnected_idle_history(vec![
        HistoryCell::Assistant(full_response),
        HistoryCell::User("new turn".to_string()),
    ]);

    assert_eq!(
        app.transcript,
        vec![retained, HistoryCell::User("new turn".to_string())]
    );
}

#[test]
fn idle_reconnect_matches_a_partially_pruned_rendered_cell() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    let full_prompt = "aaa bbb ccc ddd eee fff".to_string();
    app.transcript = vec![HistoryCell::User(full_prompt.clone())];
    assert_eq!(app.prune_rendered_prefix(RenderOpts::new(16, false), 1), 1);
    let retained = app.transcript[0].clone();

    app.merge_reconnected_idle_history(vec![
        HistoryCell::User(full_prompt),
        HistoryCell::Assistant("new turn".to_string()),
    ]);

    assert_eq!(
        app.transcript,
        vec![retained, HistoryCell::Assistant("new turn".to_string())]
    );
}

#[test]
fn queued_local_input_does_not_replace_the_active_tape_boundary() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.apply_tape_record(super::super::tests::tape_message("user", "same prompt"));
    app.push_output("ear".into());
    app.composer.set_text("same prompt");
    assert!(matches!(
        app.handle_submit(),
        Some(super::FileBackedAction::Submit(_))
    ));
    app.push_output("lier".into());
    app.apply_tape_record(super::super::tests::tape_message("assistant", "earlier"));
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("same prompt".into()),
            HistoryCell::Assistant("earlier".into()),
        ]
    );
    app.push_output("later".into());
    app.apply_tape_record(super::super::tests::tape_message("user", "same prompt"));
    app.apply_tape_record(super::super::tests::tape_message("assistant", "later"));
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("same prompt".into()),
            HistoryCell::Assistant("earlier".into()),
            HistoryCell::User("same prompt".into()),
            HistoryCell::Assistant("later".into()),
        ]
    );
}
