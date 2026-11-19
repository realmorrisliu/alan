use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn universal_newline_fallbacks_insert_newlines() {
    let mut composer = Composer::default();
    composer.set_text("a");
    assert_eq!(
        composer.handle_key(key_mod(KeyCode::Char('j'), KeyModifiers::CONTROL)),
        ComposerKeyOutcome::Changed
    );
    composer.insert_text("b");
    assert_eq!(
        composer.handle_key(key_mod(KeyCode::Enter, KeyModifiers::ALT)),
        ComposerKeyOutcome::Changed
    );
    composer.insert_text("c\\");
    assert_eq!(
        composer.handle_key(key(KeyCode::Enter)),
        ComposerKeyOutcome::Changed
    );
    composer.insert_text("d");
    assert_eq!(composer.text(), "a\nb\nc\nd");
    assert_eq!(
        composer.handle_key(key(KeyCode::Enter)),
        ComposerKeyOutcome::Submit
    );
}

#[test]
fn multiline_up_down_navigates_lines_before_history_recall() {
    let mut composer = Composer::default();
    composer.remember("older history");
    composer.set_text("first line\nsecond\nthird line");
    // Cursor starts at end of line 3 ("third line", col 10).
    assert_eq!(composer.cursor(), composer.text().len());

    // Up moves to end of shorter line 2 ("second") while retaining preferred column 10.
    composer.handle_key(key(KeyCode::Up));
    assert_eq!(composer.text(), "first line\nsecond\nthird line");
    assert_eq!(composer.cursor(), "first line\nsecond".len());

    // Up again moves to col 10 of line 1 ("first line").
    composer.handle_key(key(KeyCode::Up));
    assert_eq!(composer.cursor(), "first line".len());
    assert!(!composer.is_recalling());

    // Up from first line recalls history.
    composer.handle_key(key(KeyCode::Up));
    assert!(composer.is_recalling());
    assert_eq!(composer.text(), "older history");

    // Down on last (only) line restores stashed multiline draft.
    composer.handle_key(key(KeyCode::Down));
    assert!(!composer.is_recalling());
    assert_eq!(composer.text(), "first line\nsecond\nthird line");

    // Move to line 1 col 2, then Down steps through line 2 and line 3 at col 2, then recalls.
    composer.set_text_with_cursor("abcde\nx\n12345", 2);
    composer.handle_key(key(KeyCode::Down));
    assert_eq!(composer.cursor(), "abcde\nx".len());
    composer.handle_key(key(KeyCode::Down));
    assert_eq!(composer.cursor(), "abcde\nx\n12".len());
}

#[test]
fn multiline_home_end_ctrl_a_ctrl_e_and_line_kills() {
    let mut composer = Composer::default();
    // Put cursor at "beta |gamma" (byte 11).
    composer.set_text_with_cursor("alpha\nbeta gamma\nomega", 11);

    // Ctrl+A / Ctrl+E move to start/end of current line.
    composer.handle_key(key_mod(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(composer.cursor(), 6);
    composer.handle_key(key_mod(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(composer.cursor(), 16);

    // Home / End jump to buffer boundaries.
    composer.handle_key(key(KeyCode::Home));
    assert_eq!(composer.cursor(), 0);
    composer.handle_key(key(KeyCode::End));
    assert_eq!(composer.cursor(), composer.text().len());

    // Ctrl+K kills from cursor to end of current line only.
    composer.set_text_with_cursor("alpha\nbeta gamma\nomega", 11);
    composer.handle_key(key_mod(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "alpha\nbeta \nomega");
    assert_eq!(composer.cursor(), 11);

    // Ctrl+K at line end joins next line.
    composer.handle_key(key_mod(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "alpha\nbeta omega");

    // Ctrl+U kills only to current line start on multiline drafts.
    composer.handle_key(key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(composer.text(), "alpha\nomega");
    assert_eq!(composer.cursor(), 6);
}

#[test]
fn delete_alt_word_editing_and_undo_work() {
    let mut composer = Composer::default();
    composer.set_text("one two three");
    composer.handle_key(key_mod(KeyCode::Char('b'), KeyModifiers::ALT));
    assert_eq!(composer.cursor(), 8); // start of "three"
    composer.handle_key(key_mod(KeyCode::Char('b'), KeyModifiers::ALT));
    assert_eq!(composer.cursor(), 4); // start of "two"
    composer.handle_key(key_mod(KeyCode::Char('f'), KeyModifiers::ALT));
    assert_eq!(composer.cursor(), 7); // end of "two"

    // Delete forward removes the space after "two".
    composer.handle_key(key(KeyCode::Delete));
    assert_eq!(composer.text(), "one twothree");

    // Alt+Backspace deletes previous word back ("two").
    composer.set_text("one two three");
    composer.handle_key(key_mod(KeyCode::Backspace, KeyModifiers::ALT));
    assert_eq!(composer.text(), "one two ");

    // Ctrl+_ undoes the word deletion cleanly.
    assert_eq!(
        composer.handle_key(key_mod(KeyCode::Char('_'), KeyModifiers::CONTROL)),
        ComposerKeyOutcome::Changed
    );
    assert_eq!(composer.text(), "one two three");
}
