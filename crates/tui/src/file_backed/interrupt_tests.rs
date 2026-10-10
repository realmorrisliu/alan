use super::*;

#[tokio::test]
async fn ctrl_c_routes_accepted_input_before_running_ui_arrives() {
    use super::super::app::{FileBackedAction, FileBackedEvent};
    use alan_agent_protocol::{InputIntent, InputMode, UserInputRecord};
    use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};

    let (shell, _root, _namespace, pid) = super::super::stdio_tests::live_root_agent().await;
    let mut app = FileBackedApp::new("/agent/root".into());
    let record = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, "task");
    super::super::file_surface::write_agent_input(
        &shell,
        &app.agent_path,
        Some(pid.parse().unwrap()),
        &record,
    )
    .await
    .unwrap();
    app.accept_input();
    let pending = VecDeque::from([PendingRootAgentTurn {
        input: record.body.clone(),
        submission_id: record.submission_id.clone(),
        submitted_process: Some(pid.parse().unwrap()),
        submitted_at_ms: 20,
    }]);
    app.composer.set_text("next unsent draft");
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.modal.active);
    let ctrl_c = || {
        FileBackedEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )))
    };
    let action = super::super::dispatch_with_pending_submissions(&mut app, ctrl_c(), &pending);
    assert!(
        matches!(action, Some(FileBackedAction::Interrupt)),
        "accepted input must route Ctrl+C to interrupt before Running UI arrives"
    );
    assert_eq!(app.activity.state, UiActivityState::Idle);
    assert_eq!(app.composer.text(), "next unsent draft");
    let (target, command) =
        interrupt_control(&app.agent_path, &pending, Some(pid.parse().unwrap())).unwrap();
    super::super::file_surface::write_machine_ctl(&shell, &target, &command)
        .await
        .unwrap();
    let events =
        String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap()).unwrap();
    assert!(events.contains(&format!("ctl:queue-v1 interrupt {}", record.submission_id)));
    assert!(!events.contains("ctl:interrupt"));

    // The host-local chooser still consumes Ctrl+C, even with an accepted input.
    app.queue.apply(
        &format!("/agent/{pid}"),
        Some(alan_agent_protocol::UiQueueSnapshot::default()),
    );
    app.project_candidate = Some(std::path::PathBuf::from("/tmp/fixture"));
    assert!(app.handle_command("/project").is_none());
    assert!(app.project_selection.is_some());
    assert!(
        super::super::dispatch_with_pending_submissions(&mut app, ctrl_c(), &pending).is_none()
    );
    assert!(app.project_selection.is_none());
    app.composer.set_text("next unsent draft");

    // Once authoritative completion removes the submission, idle draft clearing returns.
    let completion = UiEvent::InputCompleted {
        submission_ids: vec![record.submission_id],
        status: alan_agent_protocol::UiInputStatus::Cancelled,
        error: None,
    };
    let mut pending = pending;
    observe_root_agent_completion(&mut pending, &completion, &mut app);
    assert!(pending.is_empty());
    assert!(
        super::super::dispatch_with_pending_submissions(&mut app, ctrl_c(), &pending).is_none()
    );
    assert!(app.composer.text().is_empty());
    assert_eq!(app.notice.as_deref(), Some("draft cleared"));
}

#[tokio::test]
async fn pending_input_interrupt_is_targeted_before_activity_is_observed() {
    let (shell, root, _, pid) = super::super::stdio_tests::live_root_agent().await;
    let pending = PendingRootAgentTurn {
        input: "queued task".into(),
        submission_id: "00000000-0000-4000-8000-000000000001".into(),
        submitted_process: Some(pid.parse().unwrap()),
        submitted_at_ms: 20,
    };
    let pending_turns = VecDeque::from([
        pending,
        PendingRootAgentTurn {
            input: "later task".into(),
            submission_id: "00000000-0000-4000-8000-000000000002".into(),
            submitted_process: Some(pid.parse().unwrap()),
            submitted_at_ms: 21,
        },
    ]);
    root.set_root_process("99999").await;
    for refreshed_pid in [Some(99999), None] {
        let (agent_path, command) =
            interrupt_control("/agent/root", &pending_turns, refreshed_pid).unwrap();
        assert_eq!(agent_path, format!("/agent/{pid}"));
        super::super::file_surface::write_machine_ctl(&shell, &agent_path, &command)
            .await
            .unwrap();
    }
    let events =
        String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap()).unwrap();
    assert!(events.contains(&format!(
        "ctl:queue-v1 interrupt {}",
        pending_turns.front().unwrap().submission_id
    )));
    assert!(!events.contains("ctl:interrupt"));
    assert!(!events.contains(&pending_turns.back().unwrap().submission_id));
}

#[test]
fn replacement_that_later_becomes_idle_reports_unknown_and_releases_pending() {
    let mut pending = VecDeque::from([
        PendingRootAgentTurn {
            input: "task".into(),
            submission_id: "input-one".into(),
            submitted_process: Some(1),
            submitted_at_ms: 20,
        },
        PendingRootAgentTurn {
            input: "task two".into(),
            submission_id: "input-two".into(),
            submitted_process: Some(2),
            submitted_at_ms: 21,
        },
    ]);
    let mut app = FileBackedApp::new("/agent/root".into());
    app.activity.state = UiActivityState::Running;
    settle_unknown_replaced_input(&mut pending, Some(1), &mut app);
    assert!(
        !pending.is_empty(),
        "a running turn is not settled by Root identity refresh"
    );
    settle_unknown_replaced_input(&mut pending, Some(2), &mut app);
    assert!(!pending.is_empty());
    app.activity.state = UiActivityState::Idle;
    settle_unknown_replaced_input(&mut pending, Some(2), &mut app);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.front().unwrap().submission_id, "input-two");
    settle_unknown_replaced_input(&mut pending, Some(3), &mut app);
    assert!(pending.is_empty());
    assert!(
        matches!(app.transcript.last(), Some(crate::history::HistoryCell::Error(message))
            if message.contains("outcome is unknown"))
    );
}
#[test]
fn matching_non_success_completion_is_visible_before_pending_is_released() {
    for (status, error) in [
        (
            alan_agent_protocol::UiInputStatus::Failed,
            Some("reason".to_string()),
        ),
        (alan_agent_protocol::UiInputStatus::Cancelled, None),
        (
            alan_agent_protocol::UiInputStatus::Cancelled,
            Some("   ".to_string()),
        ),
        (
            alan_agent_protocol::UiInputStatus::Cancelled,
            Some("user stopped".to_string()),
        ),
    ] {
        let mut pending = VecDeque::from([PendingRootAgentTurn {
            input: "task".into(),
            submission_id: "mine".into(),
            submitted_process: Some(1),
            submitted_at_ms: 20,
        }]);
        let mut app = FileBackedApp::new("/agent/root".into());
        let mut event = UiEvent::InputCompleted {
            submission_ids: vec!["other".into()],
            status,
            error: error.clone(),
        };
        assert_eq!(pending.len(), 1);
        assert!(app.transcript.is_empty());
        observe_root_agent_completion(&mut pending, &event, &mut app);
        assert_eq!(pending.len(), 1, "other submissions must remain pending");
        assert!(
            app.transcript.is_empty(),
            "other submissions must not add history"
        );
        if let UiEvent::InputCompleted { submission_ids, .. } = &mut event {
            *submission_ids = vec!["mine".into()];
        }
        observe_root_agent_completion(&mut pending, &event, &mut app);
        assert!(pending.is_empty());
        let last = app.transcript.last();
        match status {
            alan_agent_protocol::UiInputStatus::Failed => assert!(
                matches!(last, Some(crate::history::HistoryCell::Error(message)) if message == "reason")
            ),
            alan_agent_protocol::UiInputStatus::Cancelled => {
                let expected = error
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .map(|reason| format!("Input cancelled: {reason}"))
                    .unwrap_or_else(|| "Input cancelled".to_string());
                assert!(
                    matches!(last, Some(crate::history::HistoryCell::Rendered(lines)) if lines.join(" ") == expected)
                );
            }
            alan_agent_protocol::UiInputStatus::Completed => unreachable!(),
        }
        app.apply_ui_event(event);
        if status == alan_agent_protocol::UiInputStatus::Failed {
            app.apply_ui_event(UiEvent::Notice {
                snapshot: alan_agent_protocol::UiNoticeSnapshot::new(
                    alan_agent_protocol::UiNoticeKind::Error,
                    "Error handling submission: reason",
                ),
            });
            assert!(
                app.notice.is_none(),
                "paired failure is already in permanent history"
            );
            app.apply_ui_event(UiEvent::Error {
                message: "Error handling submission: reason".into(),
                recoverable: true,
            });
            assert_eq!(app.transcript.len(), 1);
            // Only the immediately paired terminal error is suppressed.
            app.apply_ui_event(UiEvent::Error {
                message: "Error handling submission: reason".into(),
                recoverable: true,
            });
            assert_eq!(app.transcript.len(), 2);
        }
    }
}
