use super::*;

#[test]
fn explicit_command_action_renders_its_program_output() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.mark_command_submission("submission-1");

    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a0".to_string(),
            name: "bash".to_string(),
            status: "failed".to_string(),
            output: r#"{"stdout":"cargo output\n","stderr":"warning line\n","exit_code":3}"#
                .to_string(),
            result: r#"{"call_id":"submission-1","exit_code":3}"#.to_string(),
        },
    );

    assert_eq!(
        app.transcript,
        vec![HistoryCell::Rendered(vec![
            "cargo output".to_string(),
            "stderr> warning line".to_string(),
            "error> command exited with status 3".to_string(),
        ])]
    );
}

#[test]
fn reattached_command_restores_intent_and_output_from_tape_action_correlation() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    let tape = r#"{"version":1,"kind":"message","role":"user","content":"git status","submission_id":"submission-1"}
"#;
    let action = ActionSnapshot {
        id: "a0".to_string(),
        name: "bash".to_string(),
        status: "failed".to_string(),
        output: r#"{"stdout":"working tree dirty\n","stderr":"warning line\n","exit_code":3}"#
            .to_string(),
        result: r#"{"call_id":"submission-1","exit_code":3}"#.to_string(),
    };

    hydrate_tape_history(&mut app, tape, std::slice::from_ref(&action));

    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::Command("git status".to_string()),
            HistoryCell::Rendered(vec![
                "working tree dirty".to_string(),
                "stderr> warning line".to_string(),
                "error> command exited with status 3".to_string(),
            ]),
        ]
    );
    assert!(app.classify_command_submission("submission-1"));
    assert_eq!(app.action_cells.get("a0"), Some(&1));

    let updated_action = ActionSnapshot {
        output: r#"{"stdout":"updated output\n","stderr":"","exit_code":0}"#.to_string(),
        result: r#"{"call_id":"submission-1","exit_code":0}"#.to_string(),
        status: "completed".to_string(),
        ..action
    };
    sync_action_snapshot(&mut app, updated_action);

    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::Command("git status".to_string()),
            HistoryCell::Rendered(vec!["updated output".to_string()]),
        ]
    );
}

#[test]
fn reattached_pending_command_keeps_tape_correlation_for_a_later_action() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    let submission_id = "8ed8a9bb-a344-4a39-8225-325b22c92756";
    let tape = format!(
        "{{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"git status\",\"submission_id\":\"{submission_id}\"}}\n"
    );

    hydrate_tape_history(&mut app, &tape, &[]);
    assert_eq!(
        app.transcript,
        vec![HistoryCell::User("git status".to_string())]
    );

    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a0".to_string(),
            name: "bash".to_string(),
            status: "running".to_string(),
            output: String::new(),
            result: format!(r#"{{"call_id":"{submission_id}"}}"#),
        },
    );
    assert_eq!(
        app.transcript,
        vec![HistoryCell::Command("git status".to_string())]
    );

    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a0".to_string(),
            name: "bash".to_string(),
            status: "completed".to_string(),
            output: r#"{"stdout":"working tree clean\n","stderr":"","exit_code":0}"#.to_string(),
            result: format!(r#"{{"call_id":"{submission_id}","exit_code":0}}"#),
        },
    );
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::Command("git status".to_string()),
            HistoryCell::Rendered(vec!["working tree clean".to_string()]),
        ]
    );
}

#[test]
fn live_remote_command_uses_tape_and_action_submission_correlation() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "git status".to_string(),
        submission_id: Some("8ed8a9bb-a344-4a39-8225-325b22c92756".to_string()),
    });

    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a-remote".to_string(),
            name: "bash".to_string(),
            status: "completed".to_string(),
            output: r#"{"stdout":"working tree clean\n","stderr":"","exit_code":0}"#.to_string(),
            result: r#"{"call_id":"8ed8a9bb-a344-4a39-8225-325b22c92756","exit_code":0}"#
                .to_string(),
        },
    );

    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::Command("git status".to_string()),
            HistoryCell::Rendered(vec!["working tree clean".to_string()]),
        ]
    );
}

#[test]
fn live_remote_command_correlates_when_the_action_event_arrives_first() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    let running_action = ActionSnapshot {
        id: "a-remote".to_string(),
        name: "bash".to_string(),
        status: "running".to_string(),
        output: String::new(),
        result: r#"{"call_id":"8ed8a9bb-a344-4a39-8225-325b22c92756","exit_code":0}"#.to_string(),
    };
    sync_action_snapshot(&mut app, running_action);
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "git status".to_string(),
        submission_id: Some("8ed8a9bb-a344-4a39-8225-325b22c92756".to_string()),
    });
    assert_eq!(
        app.transcript,
        vec![HistoryCell::Command("git status".to_string())]
    );

    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a-remote".to_string(),
            name: "bash".to_string(),
            status: "completed".to_string(),
            output: r#"{"stdout":"working tree clean\n","stderr":"","exit_code":0}"#.to_string(),
            result: r#"{"call_id":"8ed8a9bb-a344-4a39-8225-325b22c92756","exit_code":0}"#
                .to_string(),
        },
    );

    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::Command("git status".to_string()),
            HistoryCell::Rendered(vec!["working tree clean".to_string()]),
        ]
    );
}

#[test]
fn standalone_cd_action_is_a_successful_silent_command_result() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.mark_command_submission("submission-cd");

    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a0".to_string(),
            name: "cd".to_string(),
            status: "completed".to_string(),
            output: r#"{"stdout":"","stderr":""}"#.to_string(),
            result: r#"{"call_id":"submission-cd","exit_code":0,"outcome":{"success":true,"cwd":"/mnt/project/src"}}"#.to_string(),
        },
    );

    assert_eq!(app.transcript, vec![HistoryCell::Rendered(Vec::new())]);
}

#[test]
fn failed_standalone_cd_renders_its_diagnostic_and_exit_status() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.mark_command_submission("submission-cd");

    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a0".to_string(),
            name: "cd".to_string(),
            status: "failed".to_string(),
            output: r#"{"stdout":"","stderr":"directory is not authorized"}"#.to_string(),
            result: r#"{"call_id":"submission-cd","exit_code":1,"outcome":{"success":false,"error":"directory is not authorized"}}"#.to_string(),
        },
    );

    assert_eq!(
        app.transcript,
        vec![HistoryCell::Rendered(vec![
            "stderr> directory is not authorized".to_string(),
            "error> command exited with status 1".to_string(),
        ])]
    );
}

#[test]
fn submitted_command_keeps_its_prompt_intent_in_transcript() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.composer.insert_text("!git status");

    let Some(FileBackedAction::Submit(input)) = app.handle_submit() else {
        panic!("submission was not dispatched");
    };
    assert_eq!(input.intent, InputIntent::Command);
    assert_eq!(input.body, "git status");
    app.accept_submission(&input);
    assert_eq!(
        app.transcript,
        vec![HistoryCell::Command("git status".to_string())]
    );
    assert_eq!(app.rendered_history_lines(80), vec!["alan! git status"]);
}

#[test]
fn rejected_submission_does_not_leave_optimistic_history_or_echo_state() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.transcript
        .push(HistoryCell::User("previous task".to_string()));
    app.reconciler.on_local_submit("previous task");
    app.composer.insert_text("rejected task");

    let Some(FileBackedAction::Submit(input)) = app.handle_submit() else {
        panic!("submission was not dispatched");
    };
    app.restore_rejected_submission(&input);

    assert_eq!(
        app.transcript,
        vec![HistoryCell::User("previous task".to_string())]
    );
    assert!(matches!(
        app.reconciler.on_user_record("previous task"),
        crate::reconcile::UserDecision::Drop
    ));
}

#[test]
fn agent_body_with_leading_space_before_slash_is_submitted_as_agent_work() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.composer.insert_text(" /help");

    assert!(app.enter_submits_agent_task());
    let Some(FileBackedAction::Submit(input)) = app.handle_submit() else {
        panic!("submission was not dispatched");
    };
    assert_eq!(input.intent, InputIntent::Agent);
    assert_eq!(input.body, " /help");
    app.accept_submission(&input);
    assert_eq!(
        app.transcript,
        vec![HistoryCell::User(" /help".to_string())]
    );
}

#[test]
fn keyboard_slash_after_explicit_prefix_preserves_input_intent() {
    for (prefix, body, intent) in [
        ("!", "/usr/bin/git status", InputIntent::Command),
        (":", "/help", InputIntent::ForceAgent),
    ] {
        let mut app = FileBackedApp::new("/agent/1".to_string());
        for character in format!("{prefix}{body}").chars() {
            press(&mut app, KeyCode::Char(character), KeyModifiers::NONE);
        }

        assert!(matches!(
            app.handle_submit(),
            Some(FileBackedAction::Submit(input))
                if input.intent == intent && input.body == body
        ));
    }
}

#[test]
fn file_completion_preserves_explicit_input_intent() {
    for (prefix, intent) in [("!", InputIntent::Command), (":", InputIntent::ForceAgent)] {
        let mut app = FileBackedApp::new("/agent/1".to_string());
        app.set_file_candidates(vec![CompletionCandidate::new("src/main.rs", None)]);
        app.composer.insert_text(&format!("{prefix}inspect @src"));
        app.refresh_completion();

        assert!(app.completion.is_some());
        app.accept_completion();

        assert!(matches!(
            app.handle_submit(),
            Some(FileBackedAction::Submit(input))
                if input.intent == intent && input.body == "inspect @src/main.rs "
        ));
    }
}

#[test]
fn confirmation_digit_builds_resume_response() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.set_pending_yield(PendingYieldCell {
        request_id: "r1".to_string(),
        kind: YieldKind::Confirmation,
        title: "Approve?".to_string(),
        prompt: None,
        options: vec!["approve".to_string(), "reject".to_string()],
        default_option: None,
        questions: Vec::new(),
        capability: None,
        reason: None,
        presentation: None,
    });

    let action = app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Key(
        KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
    )));
    match action {
        Some(FileBackedAction::Resume {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "r1");
            assert_eq!(response, r#"{"choice":"approve"}"#);
        }
        other => panic!("expected resume action, got {other:?}"),
    }
}

#[test]
fn pending_text_response_keeps_prefix_characters_literal() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.set_pending_yield(PendingYieldCell {
        request_id: "r1".to_string(),
        kind: YieldKind::StructuredInput,
        title: "Reply".to_string(),
        prompt: None,
        options: Vec::new(),
        default_option: None,
        questions: Vec::new(),
        capability: None,
        reason: None,
        presentation: None,
    });

    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        "!literal".to_string(),
    )));
    assert_eq!(app.composer.intent(), InputIntent::Agent);
    assert_eq!(app.composer.text(), "!literal");

    assert!(matches!(
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE),
        Some(FileBackedAction::Resume { request_id, response })
            if request_id == "r1" && response == "!literal"
    ));
}
