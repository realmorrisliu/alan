use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn composer_submits_and_clears_text() {
    let mut composer = Composer::default();
    composer.handle_key(key(KeyCode::Char('h')));
    composer.handle_key(key(KeyCode::Char('i')));
    assert_eq!(
        composer.handle_key(key(KeyCode::Enter)),
        ComposerKeyOutcome::Submit
    );
    assert_eq!(composer.take_submit(), Some("hi".into()));
    assert_eq!(composer.text(), "");
}

#[test]
fn submitted_and_recalled_input_preserves_whitespace_and_newlines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    std::fs::write(&path, "legacy command\n\"quoted legacy\"\nalan-history-v1\t\"quoted\"\nalan-history-v1\tliteral\n").unwrap();
    let text = "!printf '%s\\n' 'a b'\n  printf done  \n";
    let mut composer = Composer::with_history(load_history(&path, 100), Some(path.clone()));
    composer.set_text(text);
    assert_eq!(composer.take_submit().as_deref(), Some(text));
    composer.remember(text);
    composer.handle_key(key(KeyCode::Up));
    assert_eq!(composer.text(), text);
    let loaded = load_history(&path, 100);
    assert_eq!(
        loaded
            .iter()
            .map(|entry| entry.body.as_str())
            .collect::<Vec<_>>(),
        [
            "legacy command",
            "\"quoted legacy\"",
            "alan-history-v1\t\"quoted\"",
            "alan-history-v1\tliteral",
            text
        ]
    );
    std::fs::remove_file(&path).unwrap();
    assert_eq!(load_history(&path, 1)[0].body, text);
    let mut restarted = Composer::with_history(loaded, Some(path));
    restarted.handle_key(key(KeyCode::Up));
    assert_eq!(restarted.text(), text);
    restarted.set_text(" \n ");
    assert!(restarted.take_submit().is_none());
    assert_eq!(restarted.text(), " \n ");
}

#[test]
fn composer_inserts_paste_at_cursor() {
    let mut composer = Composer::default();
    composer.set_text("ac");
    composer.handle_key(key(KeyCode::Left));
    composer.insert_text("b\n");
    assert_eq!(composer.text(), "ab\nc");
}

#[test]
fn ctrl_a_and_ctrl_e_jump_to_line_ends() {
    let mut composer = Composer::default();
    composer.set_text("hello");
    composer.handle_key(key_mod(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(composer.cursor(), 0);
    composer.handle_key(key_mod(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(composer.cursor(), 5);
}

#[test]
fn ctrl_w_deletes_previous_word() {
    let mut composer = Composer::default();
    composer.set_text("hello world");
    composer.handle_key(key_mod(KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "hello ");
}

#[test]
fn ctrl_u_deletes_to_line_start() {
    let mut composer = Composer::default();
    composer.set_text("hello world");
    composer.handle_key(key(KeyCode::Left));
    composer.handle_key(key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "d");
}

#[test]
fn alt_left_moves_by_word() {
    let mut composer = Composer::default();
    composer.set_text("hello world");
    composer.handle_key(key_mod(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(composer.cursor(), 6);
    composer.handle_key(key_mod(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(composer.cursor(), 0);
}

#[test]
fn history_recall_walks_previous_submissions() {
    let mut composer = Composer::default();
    composer.remember("first");
    composer.remember("second");
    composer.set_text("draft");
    composer.handle_key(key(KeyCode::Up));
    assert_eq!(composer.text(), "second");
    composer.handle_key(key(KeyCode::Up));
    assert_eq!(composer.text(), "first");
    composer.handle_key(key(KeyCode::Down));
    assert_eq!(composer.text(), "second");
    composer.handle_key(key(KeyCode::Down));
    assert_eq!(composer.text(), "draft");
}

#[test]
fn history_dedupes_adjacent_entries() {
    let mut composer = Composer::default();
    composer.remember("same");
    composer.remember("same");
    composer.handle_key(key(KeyCode::Up));
    assert_eq!(composer.text(), "same");
    composer.handle_key(key(KeyCode::Up));
    // only one entry, stays put
    assert_eq!(composer.text(), "same");
}

#[test]
fn history_persists_across_launches_via_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tui_history");
    {
        let mut composer = Composer::with_history(Vec::new(), Some(path.clone()));
        composer.remember("persisted entry");
    }
    let loaded = load_history(&path, 100);
    assert_eq!(
        loaded,
        vec![HistoryEntry::agent("persisted entry".to_string())]
    );
    let mut next = Composer::with_history(loaded, Some(path));
    next.handle_key(key(KeyCode::Up));
    assert_eq!(next.text(), "persisted entry");
}
