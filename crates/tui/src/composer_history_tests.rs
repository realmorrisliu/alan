use super::*;

#[test]
fn torn_history_tail_preserves_complete_entries_and_next_successful_append() {
    for tail in [
        b"{\"body\":\"unfinished".to_vec(),
        vec![0xce],
        vec![b'x'; 8193],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history");
        let before = HistoryEntry {
            body: "before\n  λ".into(),
            intent: InputIntent::Command,
        };
        assert!(!append_history_line(&path, &before).unwrap());
        let records = history_records_path(&path, 2);
        let complete_prefix = std::fs::read(&records).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&records)
            .unwrap()
            .write_all(&tail)
            .unwrap();

        // A killed or failed writer can leave invalid UTF-8 as well as JSON.
        // Readers retain the healthy prefix without modifying the torn file.
        let mut composer = Composer::from_history_path(path.clone());
        assert_eq!(composer.history, vec![before.clone()]);
        assert!(composer.history_notice().is_some());
        assert_eq!(
            std::fs::read(&records).unwrap(),
            [complete_prefix.clone(), tail].concat()
        );

        let after = HistoryEntry {
            body: ":literal agent\n\"escaped\"\\ λ  ".into(),
            intent: InputIntent::Agent,
        };
        composer.remember_input(&after.body, after.intent);
        let raw = std::fs::read_to_string(&records).unwrap();
        let parsed: Vec<HistoryEntry> = raw
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(parsed, vec![before.clone(), after.clone()]);
        assert!(raw.as_bytes().starts_with(&complete_prefix));
        assert!(raw.ends_with('\n'));
        let restarted = Composer::from_history_path(path);
        assert_eq!(restarted.history, vec![before, after]);
        assert!(restarted.history_notice().is_none());
    }
}

#[test]
fn torn_first_record_does_not_swallow_the_next_command() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    std::fs::write(history_records_path(&path, 2), b"{\"body\":").unwrap();
    let entry = HistoryEntry {
        body: "printf 'next'\n  ".into(),
        intent: InputIntent::Command,
    };
    assert!(append_history_line(&path, &entry).unwrap());
    assert_eq!(load_history(&path, 10), vec![entry.clone()]);
    assert!(!append_history_line(&path, &entry).unwrap());
    assert_eq!(load_history(&path, 10), vec![entry.clone(), entry]);
}

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
