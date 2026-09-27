use super::{FileBackedAction, FileBackedApp};
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
fn explicit_input_prefix_is_consumed_once_and_preserves_its_body() {
    use alan_agent_protocol::InputIntent;
    for (input, intent, body) in [
        (
            "!printf '%s' 'x'\n  echo done \n",
            InputIntent::Command,
            "printf '%s' 'x'\n  echo done \n",
        ),
        (":!literal", InputIntent::ForceAgent, "!literal"),
        ("!!literal", InputIntent::Command, "!literal"),
        (":/clear", InputIntent::ForceAgent, "/clear"),
        ("plain ! text", InputIntent::Agent, "plain ! text"),
    ] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.insert_input_text(input);
        let Some(FileBackedAction::Submit(record)) = app.handle_submit() else {
            panic!("expected input record")
        };
        assert_eq!(record.intent, intent);
        assert_eq!(record.body, body);
        record.validate().unwrap();
        assert_eq!(
            app.composer.text(),
            body,
            "unaccepted input stays in the editor"
        );
        assert!(app.transcript.is_empty());
        app.accept_input();
        assert!(app.composer.text().is_empty());
        assert!(app.transcript.is_empty());
    }
    for input in ["!", ":", "! \n"] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.insert_input_text(input);
        assert!(app.handle_submit().is_none());
        assert_eq!(
            app.composer.text(),
            alan_agent_protocol::parse_input_prefix(input).1
        );
        assert!(app.transcript.is_empty());
    }
}

#[test]
fn pending_response_keeps_prefixes_as_literal_response_data() {
    for input in ["!answer", ":!answer", "/clear"] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.set_pending_yield(crate::history::PendingYieldCell {
            request_id: "request-1".into(),
            kind: alan_agent_protocol::YieldKind::Custom("text".into()),
            title: "Answer".into(),
            prompt: None,
            options: Vec::new(),
            default_option: None,
            questions: Vec::new(),
            capability: None,
            reason: None,
            presentation: None,
        });
        app.composer.set_text(input);
        let Some(FileBackedAction::Resume {
            request_id,
            response,
        }) = app.handle_submit()
        else {
            panic!("expected response to existing request")
        };
        assert_eq!(request_id, "request-1");
        assert_eq!(response, input);
    }
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
    app.accept_input();
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

#[test]
fn prefix_inserted_at_the_start_of_an_existing_agent_draft_is_literal() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    for pasted in [false, true] {
        for prefix in ["!", ":"] {
            let mut app = FileBackedApp::new("/agent/root".into());
            app.composer.set_text("explain this");
            app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
            if pasted {
                app.dispatch(super::FileBackedEvent::Terminal(
                    super::TerminalEvent::Paste(prefix.into()),
                ));
            } else {
                app.handle_key(KeyEvent::new(
                    KeyCode::Char(prefix.chars().next().unwrap()),
                    KeyModifiers::NONE,
                ));
            }
            let Some(FileBackedAction::Submit(record)) = app.handle_submit() else {
                panic!("input")
            };
            assert_eq!(record.intent, alan_agent_protocol::InputIntent::ForceAgent);
            assert_eq!(record.body, format!("{prefix}explain this"));
        }
    }
}

#[test]
fn draft_intent_survives_editing_history_and_rejected_empty_input() {
    use alan_agent_protocol::InputIntent;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    let mut app = FileBackedApp::new("/agent/root".into());
    app.insert_input_text("explain");
    app.handle_key(key(KeyCode::Home));
    app.insert_input_text("!");
    assert_eq!(app.composer.text(), "!explain");
    app.handle_key(key(KeyCode::Home));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Backspace));
    app.insert_input_text("!");
    let Some(FileBackedAction::Submit(record)) = app.handle_submit() else {
        panic!("input")
    };
    assert_eq!(record.intent, InputIntent::ForceAgent);
    assert_eq!(record.body, "!explain");
    app.accept_input();
    app.insert_input_text("!draft");
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.input_intent, InputIntent::ForceAgent);
    assert_eq!(app.composer.text(), "!explain");
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.input_intent, InputIntent::Command);
    assert_eq!(app.composer.text(), "draft");
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert!(app.handle_submit().is_none());
    assert_eq!(app.input_intent, InputIntent::Command);
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(app.input_intent, InputIntent::Agent);
    app.insert_input_text(":!literal");
    assert_eq!(app.composer.text(), "!literal");
    assert_eq!(app.input_intent, InputIntent::ForceAgent);
}

#[test]
fn pending_response_resets_intent_only_when_it_consumes_the_draft() {
    use alan_agent_protocol::{InputIntent, YieldKind};
    for accepted in [false, true] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.insert_input_text("!  draft  ");
        app.set_pending_yield(crate::history::PendingYieldCell {
            request_id: "request".into(),
            kind: if accepted {
                YieldKind::Custom("text".into())
            } else {
                YieldKind::Confirmation
            },
            title: "Answer".into(),
            prompt: None,
            options: vec!["yes".into(), "no".into()],
            default_option: None,
            questions: Vec::new(),
            capability: None,
            reason: None,
            presentation: None,
        });
        let response = app.handle_submit();
        app.clear_pending_yield();
        if accepted {
            assert!(matches!(response, Some(FileBackedAction::Resume { .. })));
            assert_eq!(app.composer.text(), "");
            assert_eq!(app.input_intent, InputIntent::Agent);
            app.insert_input_text("explain this");
            let Some(FileBackedAction::Submit(record)) = app.handle_submit() else {
                panic!("input")
            };
            assert_eq!(record.intent, InputIntent::Agent);
        } else {
            assert!(response.is_none());
            assert_eq!(app.composer.text(), "  draft  ");
            assert_eq!(app.input_intent, InputIntent::Command);
        }
    }
}

#[test]
fn explicit_commands_bypass_semantic_completions_and_submit_exact_bodies() {
    use crate::completion::CompletionCandidate;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    for input in ["!echo $co", "!echo @fi", "!/he"] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.set_skill_candidates(vec![CompletionCandidate::new("code-review", None)]);
        app.set_file_candidates(vec![CompletionCandidate::new("file.txt", None)]);
        app.composer.set_text("use $co");
        app.refresh_completion();
        let stale_completion = app.completion.clone();
        assert!(stale_completion.is_some());
        app.composer.set_text("");
        app.dispatch(super::FileBackedEvent::Terminal(
            super::TerminalEvent::Paste(input.into()),
        ));
        assert!(app.completion.is_none());
        app.completion = stale_completion;
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(app.completion.is_none());
        let Some(FileBackedAction::Submit(record)) =
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        else {
            panic!("command")
        };
        assert_eq!(record.intent, alan_agent_protocol::InputIntent::Command);
        assert_eq!(record.body, &input[1..]);
    }
}

#[test]
fn upgrading_history_preserves_legacy_agent_text_and_new_explicit_intent() {
    use crate::composer::{Composer, load_history};
    use alan_agent_protocol::InputIntent;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history");
    std::fs::write(&path, "!legacy\n").unwrap();
    std::fs::write(
        temp.path().join("history.v1.jsonl"),
        format!("{}\n", serde_json::to_string(":!legacy-v1\n  ").unwrap()),
    )
    .unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer = Composer::with_history(load_history(&path, 100), Some(path.clone()));
    app.insert_input_text("!printf x\n  ");
    app.accept_input();
    app.insert_input_text(":!literal");
    app.accept_input();
    let mut restarted = FileBackedApp::new("/agent/root".into());
    restarted.composer = Composer::with_history(load_history(&path, 100), Some(path));
    for (intent, body) in [
        (InputIntent::ForceAgent, "!literal"),
        (InputIntent::Command, "printf x\n  "),
        (InputIntent::Agent, ":!legacy-v1\n  "),
        (InputIntent::Agent, "!legacy"),
    ] {
        restarted.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        let Some(FileBackedAction::Submit(record)) = restarted.handle_submit() else {
            panic!("history input")
        };
        assert_eq!(record.intent, intent);
        assert_eq!(record.body, body);
        assert_eq!(restarted.composer.text(), body);
    }
}

#[test]
fn forced_agent_slash_submits_exact_body_without_local_completion() {
    use crate::completion::{CompletionCandidate, CompletionKind};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.set_text("/clear");
    app.refresh_completion();
    let stale = app.completion.clone();
    assert!(stale.is_some());
    app.composer.set_text("");
    app.dispatch(super::FileBackedEvent::Terminal(
        super::TerminalEvent::Paste(":/clear".into()),
    ));
    assert!(app.completion.is_none());
    app.completion = stale;
    let Some(FileBackedAction::Submit(record)) =
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
    else {
        panic!("forced Agent submission")
    };
    assert_eq!(record.intent, alan_agent_protocol::InputIntent::ForceAgent);
    assert_eq!(record.body, "/clear");
    app.set_skill_candidates(vec![CompletionCandidate::new("code-review", None)]);
    app.composer.set_text("$co");
    app.refresh_completion();
    assert_eq!(app.completion.unwrap().kind, CompletionKind::Skill);
}
