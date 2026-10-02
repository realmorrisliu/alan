use super::*;

#[test]
fn configured_history_bad_load_and_unwritable_save_are_safe_nonfatal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secret-path");
    std::fs::create_dir(&path).unwrap();
    let mut composer = Composer::from_history_path(path);
    assert_eq!(
        composer.history_notice(),
        Some("Composer history unavailable; recall is session-only")
    );
    composer.remember_input("secret-content", InputIntent::Command);
    assert_eq!(
        composer.history_notice(),
        Some("Composer history unavailable; recall is session-only")
    );
    composer.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(composer.text(), "secret-content");

    let blocked = dir.path().join("blocked-secret");
    std::fs::write(&blocked, "secret-error").unwrap();
    let mut composer = Composer::from_history_path(blocked.join("history"));
    composer.remember("private body");
    assert_eq!(
        composer.history_notice(),
        Some("Composer history unavailable; recall is session-only")
    );
    composer.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(composer.text(), "private body");
}

#[test]
fn configured_history_keeps_legacy_limit_and_typed_canonical_command() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let legacy = (0..1100)
        .map(|i| format!("legacy{i}\n"))
        .collect::<String>();
    std::fs::write(&path, legacy).unwrap();
    let mut composer = Composer::from_history_path(path.clone());
    assert_eq!(composer.history.len(), crate::HISTORY_LIMIT);
    assert_eq!(composer.history[0].body, "legacy100");
    let (intent, body) = alan_agent_protocol::parse_input_prefix("!printf x\n  ");
    composer.remember_input(body, intent);
    composer.remember_input("!literal agent", InputIntent::Agent);
    let mut restarted = Composer::from_history_path(path);
    assert_eq!(restarted.history.len(), crate::HISTORY_LIMIT);
    restarted.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(restarted.text(), "!literal agent");
    assert_eq!(restarted.recalled_intent(), Some(InputIntent::Agent));
    restarted.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(restarted.text(), "printf x\n  ");
    assert_eq!(restarted.recalled_intent(), Some(InputIntent::Command));
}
