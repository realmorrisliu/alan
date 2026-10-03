use super::{FileBackedAction, FileBackedApp, FileBackedEvent};
use crate::file_backed::{ProjectAccess, ProjectControl, ProjectMountReceipt};
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
        assert_eq!(
            app.notice.as_deref(),
            Some("Enter content after the input prefix.")
        );
        assert_eq!(
            app.input_intent,
            alan_agent_protocol::parse_input_prefix(input).0
        );
        app.insert_input_text("pwd");
        assert!(app.handle_submit().is_some());
        assert!(app.notice.is_none());
    }
}

#[test]
fn project_picker_defaults_read_only_and_supports_toggle_cancel_and_revoke() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        "/agent/1",
        Some(alan_agent_protocol::UiQueueSnapshot::default()),
    );
    app.project_candidate = Some(std::path::PathBuf::from("/tmp/fixture"));
    assert!(app.handle_command("/project").is_none());
    assert_eq!(app.composer.text(), "/tmp/fixture");
    assert!(app.notice.as_deref().unwrap().contains("read-only"));
    assert_eq!(app.input_prompt_prefix(), "! ");

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert!(app.notice.as_deref().unwrap().contains("read-write"));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.composer.text().is_empty());
    assert!(app.project_selection.is_none());

    app.handle_command("/project");
    let Some(FileBackedAction::Project(ProjectControl::Mount { host_path, access })) =
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
    else {
        panic!("project approval action")
    };
    assert_eq!(host_path, std::path::PathBuf::from("/tmp/fixture"));
    assert_eq!(access, ProjectAccess::ReadOnly);

    app.project = Some(ProjectMountReceipt {
        grant_id: "request-1".into(),
        namespace_path: "/mnt/project-request-1".into(),
        label: "fixture".into(),
        access: ProjectAccess::ReadOnly,
    });
    assert!(matches!(
        app.handle_command("/project revoke"),
        Some(FileBackedAction::Project(ProjectControl::Revoke { grant_id }))
            if grant_id == "request-1"
    ));
}

#[test]
fn settled_paused_project_picker_is_available_without_continuing_queue() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.queue.apply(
        "/agent/1",
        Some(alan_agent_protocol::UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: true,
            pending_submission_ids: vec!["queued".into()],
            ..Default::default()
        }),
    );
    app.activity.state = alan_agent_protocol::UiActivityState::Paused;
    app.handle_command("/project");
    assert_eq!(app.project_selection, Some(ProjectAccess::ReadOnly));
    assert_eq!(
        app.activity.state,
        alan_agent_protocol::UiActivityState::Paused
    );
}

#[test]
fn project_cwd_observation_accepts_normalized_trailing_separator() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    let receipt = ProjectMountReceipt {
        grant_id: "grant-1".into(),
        namespace_path: "/mnt/project-1".into(),
        label: "fixture".into(),
        access: ProjectAccess::ReadWrite,
    };
    app.stage_project_control(
        "/agent/root".into(),
        "mount-input".into(),
        Some((receipt, "/tmp".into())),
        None,
    );

    app.observe_project_action("/agent/root", &super::ActionSnapshot {
        id: "action-1".into(),
        name: "cd".into(),
        status: "completed".into(),
        output: String::new(),
        result: r#"{"call_id":"mount-input","exit_code":0,"outcome":{"cwd":"/mnt/project-1/","success":true}}"#
            .into(),
    });

    assert_eq!(
        app.namespace_cwd,
        std::path::PathBuf::from("/mnt/project-1/")
    );
    assert_eq!(app.pending_project_cwd, None);
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .starts_with("project cwd: /mnt/project-1/")
    );

    app.stage_project_control(
        "/agent/root".into(),
        "revoke-input".into(),
        None,
        Some("grant-1".into()),
    );
    app.observe_project_action(
        "/agent/root",
        &super::ActionSnapshot {
            id: "action-2".into(),
            name: "cd".into(),
            status: "completed".into(),
            output: String::new(),
            result: r#"{"call_id":"another-input","outcome":{"cwd":"/","success":true}}"#.into(),
        },
    );
    assert_eq!(app.take_ready_project_revoke(), None);
    app.observe_project_action(
        "/agent/root",
        &super::ActionSnapshot {
            id: "action-3".into(),
            name: "cd".into(),
            status: "completed".into(),
            output: String::new(),
            result:
                r#"{"call_id":"revoke-input","exit_code":0,"outcome":{"cwd":"/","success":true}}"#
                    .into(),
        },
    );
    assert_eq!(app.take_ready_project_revoke().as_deref(), Some("grant-1"));
    assert!(app.project.is_some());
}

#[test]
fn one_enter_executes_a_selected_slash_command_and_idle_ctrl_c_clears_the_draft() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.set_text("/comp");
    app.refresh_completion();
    assert!(matches!(
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(FileBackedAction::MachineCtl { command, .. }) if command == "compact"
    ));

    app.composer.set_text("unsent draft");
    assert!(
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .is_none()
    );
    assert!(app.composer.text().is_empty());
    assert_eq!(app.notice.as_deref(), Some("draft cleared"));
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
            retry_input,
        }) = app.handle_submit()
        else {
            panic!("expected response to existing request")
        };
        assert_eq!(request_id, "request-1");
        assert_eq!(response, input);
        assert_eq!(retry_input, input);
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
fn request_response_write_keeps_the_yield_retryable_without_blocking_repeats() {
    use alan_agent_protocol::YieldKind;

    let mut app = FileBackedApp::new("/agent/root".into());
    app.set_pending_yield(crate::history::PendingYieldCell {
        request_id: "request-1".into(),
        kind: YieldKind::Confirmation,
        title: "Approve?".into(),
        prompt: None,
        options: vec!["approve".into(), "reject".into()],
        default_option: None,
        questions: Vec::new(),
        capability: None,
        reason: None,
        presentation: None,
    });
    app.composer.set_text("approve");

    let Some(FileBackedAction::Resume {
        request_id,
        retry_input,
        ..
    }) = app.handle_submit()
    else {
        panic!("confirmation should produce a response")
    };
    app.begin_resume_write(request_id.clone());
    assert!(app.handle_submit().is_none());

    app.dispatch(FileBackedEvent::ResumeWriteCompleted {
        request_id: request_id.clone(),
        retry_input: retry_input.clone(),
        result: Err("temporarily unavailable".into()),
    });
    assert_eq!(app.composer.text(), "approve");
    assert!(app.pending_yield.is_some());
    assert!(app.response_in_flight.is_none());

    assert!(matches!(
        app.handle_submit(),
        Some(FileBackedAction::Resume { .. })
    ));
    app.begin_resume_write(request_id.clone());
    app.dispatch(FileBackedEvent::ResumeWriteCompleted {
        request_id,
        retry_input,
        result: Ok(()),
    });
    assert!(app.pending_yield.is_none());
    assert_eq!(app.notice.as_deref(), Some("response sent"));
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

#[test]
fn command_prompt_edits_only_the_visible_body_and_recalls_intent() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = FileBackedApp::new("/agent/root".into());
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    app.handle_key(key(KeyCode::Char('!')));
    assert_eq!(app.input_prompt_prefix(), "! ");
    assert_eq!(app.composer.text(), "");
    assert!(app.handle_submit().is_none());
    app.handle_key(key(KeyCode::Backspace));
    assert_eq!(app.input_prompt_prefix(), ": ");
    app.dispatch(super::FileBackedEvent::Terminal(
        super::TerminalEvent::Paste("!echo x\n你好".into()),
    ));
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.composer.cursor(), 0);
    app.handle_key(key(KeyCode::Char('!')));
    assert_eq!(app.composer.text(), "!echo x\n你好");
    let Some(FileBackedAction::Submit(record)) = app.handle_submit() else {
        panic!("input")
    };
    assert_eq!(record.body, "!echo x\n你好");
    app.accept_input();
    assert_eq!(app.input_prompt_prefix(), ": ");
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.input_prompt_prefix(), "! ");
    assert_eq!(app.composer.text(), "!echo x\n你好");
    app.handle_key(key(KeyCode::Down));
    app.insert_input_text(":!literal");
    assert_eq!(app.input_prompt_prefix(), ": ");
    assert_eq!(app.composer.text(), "!literal");
}

#[test]
fn pending_request_history_recall_keeps_body_and_intent_together() {
    use alan_agent_protocol::{InputIntent, YieldKind};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.remember("explain this code");
    app.insert_input_text("!");
    app.set_pending_yield(crate::history::PendingYieldCell {
        request_id: "request".into(),
        kind: YieldKind::Custom("text".into()),
        title: "Answer".into(),
        prompt: None,
        options: vec![],
        default_option: None,
        questions: vec![],
        capability: None,
        reason: None,
        presentation: None,
    });
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    app.clear_pending_yield();
    let Some(FileBackedAction::Submit(record)) = app.handle_submit() else {
        panic!("submission")
    };
    assert_eq!(record.intent, InputIntent::Agent);
    assert_eq!(record.body, "explain this code");
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.input_intent, InputIntent::Command);
    assert!(app.composer.text().is_empty());
}

#[test]
fn live_and_rehydrated_transcripts_preserve_command_route_and_body() {
    let body = "printf '你好'\n  pwd";
    let raw = serde_json::json!({
        "version": 1, "kind": "message", "role": "user", "content": body,
        "submission_id": "command", "input_intent": "command",
    })
    .to_string();
    let mut app = FileBackedApp::new("/agent/root".into());
    app.apply_tape_record(serde_json::from_str(&raw).unwrap());
    let hydrated = crate::file_backed::file_surface::parse_tape_history(&raw);
    assert_eq!(app.transcript, hydrated);
    assert_eq!(hydrated, [HistoryCell::Command(body.into())]);
    let lines = hydrated[0].render_lines(RenderOpts::new(80, false));
    assert_eq!(lines, ["! printf '你好'", "    pwd"]);
    let legacy = crate::file_backed::file_surface::parse_tape_history(
        r#"{"version":1,"kind":"message","role":"user","content":"!literal"}"#,
    );
    assert_eq!(legacy, [HistoryCell::User("!literal".into())]);
    assert_eq!(
        legacy[0].render_lines(RenderOpts::new(80, false)),
        [": !literal"]
    );
}
