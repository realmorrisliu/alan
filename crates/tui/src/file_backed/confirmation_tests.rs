use super::*;
use alan_agent_protocol::{
    DiffHunk, DiffLine, InputIntent, ToolResultPresentation, UiActivitySnapshot, UiQueueSnapshot,
    YieldKind,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn render_lines(app: &FileBackedApp, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| super::layout::draw_at(frame, app, 5_000))
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf.cell((x, y)).unwrap().symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn bash_confirmation() -> PendingYieldCell {
    PendingYieldCell {
        request_id: "req-bash-1".into(),
        kind: YieldKind::Confirmation,
        title: "Escalate tool call `bash`".into(),
        prompt: Some("Allow bash execution outside workspace?".into()),
        options: vec!["approve".into(), "allow_session".into(), "reject".into()],
        default_option: Some("approve".into()),
        questions: Vec::new(),
        capability: Some("bash".into()),
        reason: Some("Runs tests outside sandbox".into()),
        presentation: Some(ToolResultPresentation::Command {
            cmdline: "cargo test -p alan-terminal-ui\necho ok".into(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            truncated: false,
        }),
    }
}

fn diff_confirmation() -> PendingYieldCell {
    PendingYieldCell {
        request_id: "req-diff-1".into(),
        kind: YieldKind::Confirmation,
        title: "Escalate tool call `edit_file`".into(),
        prompt: None,
        options: vec!["approve".into(), "allow_session".into(), "reject".into()],
        default_option: Some("allow_session".into()),
        questions: Vec::new(),
        capability: Some("edit_file".into()),
        reason: Some("Writes outside workspace mount".into()),
        presentation: Some(ToolResultPresentation::Diff {
            path: "src/lib.rs".into(),
            hunks: vec![DiffHunk {
                header: Some("@@ -1 +1 @@".into()),
                lines: vec![
                    DiffLine::Removed {
                        text: "old_val()".into(),
                    },
                    DiffLine::Added {
                        text: "new_val()".into(),
                    },
                ],
            }],
        }),
    }
}

#[test]
fn confirmation_selector_renders_pre_execution_command_and_diff_cleanly() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.set_pending_yield(bash_confirmation());
    assert!(app.confirmation.is_some());
    assert!(
        !app.transcript
            .iter()
            .any(|cell| matches!(cell, HistoryCell::PendingYield(_))),
        "active confirmation should not duplicate PendingYield in transcript"
    );

    let screen = render_lines(&app, 100, 18).join("\n");
    assert!(screen.contains("Approval required"), "{screen}");
    assert!(
        screen.contains("Approval · Escalate tool call `bash`"),
        "{screen}"
    );
    assert!(
        screen.contains("$ cargo test -p alan-terminal-ui"),
        "{screen}"
    );
    assert!(screen.contains("echo ok"), "{screen}");
    assert!(!screen.contains("exit "), "{screen}");
    assert!(screen.contains("capability: bash"), "{screen}");
    assert!(
        screen.contains("why: Runs tests outside sandbox"),
        "{screen}"
    );
    assert!(screen.contains("▶ [1] approve (default)"), "{screen}");
    assert!(screen.contains("[4] Reject with note…"), "{screen}");
    assert!(
        screen.contains("↑↓ / 1-4 choose · Enter confirm · Esc cancel"),
        "{screen}"
    );
    assert!(
        !screen.lines().any(|line| line.starts_with(": ")),
        "{screen}"
    );

    app.set_pending_yield(diff_confirmation());
    let conf = app.confirmation.as_ref().unwrap();
    assert_eq!(
        conf.selected, 1,
        "default_option allow_session selects index 1"
    );
    let diff_screen = render_lines(&app, 100, 18).join("\n");
    assert!(
        diff_screen.contains("▶ [2] allow_session (default)"),
        "{diff_screen}"
    );
    assert!(diff_screen.contains("src/lib.rs"), "{diff_screen}");
    assert!(diff_screen.contains("-old_val()"), "{diff_screen}");
    assert!(diff_screen.contains("+new_val()"), "{diff_screen}");
}

#[test]
fn confirmation_keys_swallow_printable_and_support_navigation_digits_and_in_flight() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.set_pending_yield(bash_confirmation());

    // Printable non-digit keys must not leak into composer.
    for ch in ['x', 'a', ' '] {
        assert!(
            app.handle_key(key(KeyCode::Char(ch), KeyModifiers::NONE))
                .is_none()
        );
    }
    assert!(app.composer.text().is_empty());

    // Out-of-range digit sets inline error and does not leak.
    assert!(
        app.handle_key(key(KeyCode::Char('9'), KeyModifiers::NONE))
            .is_none()
    );
    assert!(
        app.confirmation
            .as_ref()
            .and_then(|c| c.error.as_deref())
            .is_some_and(|err| err.contains("1-4"))
    );
    assert!(app.composer.text().is_empty());

    // Navigation: Down/j/Tab & Up/k/BackTab wrap cleanly.
    assert!(
        app.handle_key(key(KeyCode::Down, KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(app.confirmation.as_ref().unwrap().selected, 1);
    assert!(
        app.handle_key(key(KeyCode::Char('j'), KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(app.confirmation.as_ref().unwrap().selected, 2);
    assert!(
        app.handle_key(key(KeyCode::Tab, KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(app.confirmation.as_ref().unwrap().selected, 3);
    assert!(
        app.handle_key(key(KeyCode::Tab, KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(app.confirmation.as_ref().unwrap().selected, 0);
    assert!(
        app.handle_key(key(KeyCode::BackTab, KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(app.confirmation.as_ref().unwrap().selected, 3);
    assert!(
        app.handle_key(key(KeyCode::Char('k'), KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(app.confirmation.as_ref().unwrap().selected, 2);

    // Digit 2 immediately chooses `allow_session`.
    let action = app.handle_key(key(KeyCode::Char('2'), KeyModifiers::NONE));
    let Some(FileBackedAction::Resume {
        request_id,
        response,
        ..
    }) = action
    else {
        panic!("expected Resume action on digit 2, got {action:?}");
    };
    let val: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(val, serde_json::json!({ "choice": "allow_session" }));

    // While response is in flight, all confirmation keys swallow and footer shows Sending response…
    app.begin_resume_write(request_id.clone());
    assert!(
        app.notice.is_none(),
        "confirmation uses inline sending indicator"
    );
    assert!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE))
            .is_none()
    );
    assert!(
        app.handle_key(key(KeyCode::Char('1'), KeyModifiers::NONE))
            .is_none()
    );
    let in_flight_screen = render_lines(&app, 100, 16).join("\n");
    assert!(
        in_flight_screen.contains("Sending response…"),
        "{in_flight_screen}"
    );

    // Completion clears selector and emits single-line settled receipt.
    app.dispatch(FileBackedEvent::ResumeWriteCompleted {
        request_id,
        retry_input: "allow_session".into(),
        result: Ok(()),
    });
    assert!(app.confirmation.is_none());
    assert!(app.pending_yield.is_none());
    let settled_screen = render_lines(&app, 100, 16).join("\n");
    assert!(
        settled_screen.contains("✓ Allowed for session · bash: cargo test -p alan-terminal-ui"),
        "{settled_screen}"
    );
    assert!(!settled_screen.contains("[1] approve"), "{settled_screen}");
    assert!(
        !settled_screen.contains("Waiting for input"),
        "{settled_screen}"
    );
}

#[test]
fn confirmation_reject_with_note_mode_and_external_clear_race_handling() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.set_pending_yield(diff_confirmation());

    // Enter note mode via digit 4 (`Reject with note…`).
    assert!(
        app.handle_key(key(KeyCode::Char('4'), KeyModifiers::NONE))
            .is_none()
    );
    assert!(app.confirmation.as_ref().unwrap().editing_note);

    for ch in "use explicit allowlist".chars() {
        assert!(
            app.handle_key(key(KeyCode::Char(ch), KeyModifiers::NONE))
                .is_none()
        );
    }
    assert!(app.composer.text().is_empty());
    assert_eq!(
        app.confirmation.as_ref().unwrap().note,
        "use explicit allowlist"
    );

    // Note rendering exposes hardware cursor line and note hint.
    let mut terminal = Terminal::new(TestBackend::new(100, 18)).unwrap();
    terminal
        .draw(|frame| super::layout::draw_at(frame, &app, 5_000))
        .unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    let buf = terminal.backend().buffer();
    let row: String = (0..100)
        .map(|x| buf.cell((x, cursor.y)).unwrap().symbol())
        .collect();
    assert!(row.contains("  Note: use explicit allowlist"), "{row}");
    assert_eq!(
        cursor.x as usize,
        "  Note: use explicit allowlist".len(),
        "cursor should sit at end of rejection note"
    );

    // Esc in note mode returns to options without interrupting.
    assert!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE))
            .is_none()
    );
    assert!(!app.confirmation.as_ref().unwrap().editing_note);

    // Esc in selector mode interrupts.
    assert!(matches!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE)),
        Some(FileBackedAction::Interrupt)
    ));

    // Re-enter note mode via Enter on Reject with note…, edit with Home/Backspace, and submit.
    assert!(
        app.handle_key(key(KeyCode::Char('4'), KeyModifiers::NONE))
            .is_none()
    );
    assert!(
        app.handle_key(key(KeyCode::Home, KeyModifiers::NONE))
            .is_none()
    );
    assert!(
        app.handle_key(key(KeyCode::Char('!'), KeyModifiers::NONE))
            .is_none()
    );
    assert!(
        app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE))
            .is_none()
    );
    let Some(FileBackedAction::Resume {
        request_id,
        response,
        retry_input,
    }) = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
    else {
        panic!("Enter in note mode should submit rejection with note");
    };
    let payload: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(
        payload,
        serde_json::json!({
            "choice": "reject",
            "modifications": "use explicit allowlist",
        })
    );

    // Simulate RequestsChanged(None) clearing pending_yield BEFORE ResumeWriteCompleted arrives,
    // plus a user keypress in that window that triggers the in-flight notice.
    app.begin_resume_write(request_id.clone());
    app.clear_pending_yield();
    assert!(app.pending_yield.is_none());
    assert!(
        app.transcript.is_empty(),
        "must not prematurely emit Approval closed while response_in_flight"
    );
    assert!(
        app.handle_key(key(KeyCode::Char('1'), KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(
        app.notice.as_deref(),
        Some("request response is still being sent")
    );

    app.dispatch(FileBackedEvent::ResumeWriteCompleted {
        request_id,
        retry_input,
        result: Ok(()),
    });
    assert!(
        app.notice.is_none(),
        "settled confirmation should clear stale in-flight notice: {:?}",
        app.notice
    );
    let screen = render_lines(&app, 100, 14).join("\n");
    assert!(
        screen.contains("✕ Rejected · edit_file src/lib.rs (use explicit allowlist)"),
        "{screen}"
    );
}

#[test]
fn confirmation_dumb_theme_and_external_close_without_submit() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.theme = crate::theme::Theme::dumb();
    app.set_pending_yield(bash_confirmation());
    let dumb_screen = render_lines(&app, 80, 16).join("\n");
    assert!(dumb_screen.contains("> [1] approve"), "{dumb_screen}");
    assert!(!dumb_screen.contains("▶"), "{dumb_screen}");

    // External close emits concise closed receipt without duplicate PendingYield.
    app.clear_pending_yield();
    let settled = render_lines(&app, 80, 12).join("\n");
    assert!(
        settled.contains("Approval closed · bash: cargo test -p alan-terminal-ui"),
        "{settled}"
    );
    assert!(!settled.contains("[1] approve"), "{settled}");
}

#[test]
fn queued_preview_single_primary_state_and_context_footer() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.activity = UiActivitySnapshot::running(1_000);
    app.track_local_input(
        "q1",
        "/agent/1".into(),
        "first line\nsecond line".into(),
        InputIntent::Agent,
    );
    app.track_local_input(
        "q2",
        "/agent/1".into(),
        "check tests across workspace cleanly".into(),
        InputIntent::Agent,
    );
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: true,
            active_submission_ids: vec!["act-0".into()],
            pending_submission_ids: vec!["q1".into(), "ext-3".into(), "q2".into()],
            ..Default::default()
        }),
    );

    let wide = render_lines(&app, 100, 14).join("\n");
    assert!(wide.contains("Working · "), "{wide}");
    assert!(wide.contains("· 3 queued"), "{wide}");
    assert!(!wide.contains("Paused"), "Running outranks Paused: {wide}");
    assert!(!wide.contains("Input active"), "{wide}");
    assert!(
        wide.contains("  ↳ Queued: first line second line"),
        "{wide}"
    );
    assert!(wide.contains("  ↳ Queued: queued input"), "{wide}");
    assert!(wide.contains("  +1 more queued"), "{wide}");
    assert!(
        wide.contains("Enter queue · ^S steer · Esc stop · ^T status"),
        "{wide}"
    );

    // Narrow Running footer fallback fits cleanly on 1 row.
    let narrow = render_lines(&app, 42, 14).join("\n");
    assert!(
        narrow.contains("queued 3") && narrow.contains("^T status"),
        "{narrow}"
    );

    // Dumb theme uses ASCII `> Queued:` prefix.
    app.theme = crate::theme::Theme::dumb();
    let dumb = render_lines(&app, 80, 14).join("\n");
    assert!(
        dumb.contains("  > Queued: first line second line"),
        "{dumb}"
    );

    // Idle with action details falls back to ^O details · ^T status footer.
    app.theme = crate::theme::Theme::default();
    app.activity = UiActivitySnapshot::idle();
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 2,
            ..Default::default()
        }),
    );
    app.projected_actions
        .insert(("/agent/1".into(), "a1".into()), (1, true));
    let idle = render_lines(&app, 100, 10).join("\n");
    assert!(idle.contains("^O details · ^T status"), "{idle}");
    assert!(!idle.contains("Enter queue"), "{idle}");
}

#[test]
fn confirmation_note_footer_and_idle_details_footer_respect_narrow_widths() {
    use unicode_width::UnicodeWidthStr;

    let mut conf = super::confirmation::ConfirmationState::new(bash_confirmation());
    conf.enter_note_mode();

    for width in [46usize, 47, 48] {
        let hint = conf.footer_hint(width, false, false);
        assert_eq!(hint, "Enter submit · Esc back", "width={width}");
        assert!(UnicodeWidthStr::width(hint.as_str()) <= width);
    }
    let full = conf.footer_hint(49, false, false);
    assert_eq!(full, "Enter submit rejection note · Esc back to options");
    assert!(UnicodeWidthStr::width(full.as_str()) <= 49);

    for width in [10usize, 20, 22] {
        let hint = conf.footer_hint(width, false, false);
        assert!(
            UnicodeWidthStr::width(hint.as_str()) <= width,
            "{width}: {hint}"
        );
    }

    let mut app = FileBackedApp::new("/agent/root".into());
    app.projected_actions
        .insert(("/agent/1".into(), "a1".into()), (1, true));
    let narrow_idle = render_lines(&app, 20, 8);
    let last_line = narrow_idle
        .iter()
        .rfind(|line| !line.is_empty())
        .map(String::as_str)
        .unwrap_or("");
    assert!(last_line.contains("^T status"), "{narrow_idle:?}");
    assert!(UnicodeWidthStr::width(last_line) <= 20, "{last_line:?}");
}

#[test]
fn rollout_switch_clears_confirmation_and_staged_outcome() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.set_pending_yield(bash_confirmation());
    assert!(app.confirmation.is_some());
    assert!(matches!(
        app.handle_key(key(KeyCode::Char('1'), KeyModifiers::NONE)),
        Some(FileBackedAction::Resume { .. })
    ));
    assert!(app.staged_confirmation_outcome.is_some());

    app.reset_for_rollout_switch(Some("rollout-2"));
    assert!(app.pending_yield.is_none());
    assert!(app.confirmation.is_none());
    assert!(app.staged_confirmation_outcome.is_none());
}

#[test]
fn confirmation_overflow_scrolls_to_top_preview_and_snaps_back_on_selection() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.viewport_size = (80, 10);
    let mut long_bash = bash_confirmation();
    long_bash.presentation = Some(ToolResultPresentation::Command {
        cmdline: "head-line-one\ncmd-line-two\ncmd-line-three\ncmd-line-four\ncmd-line-five\ntail-line-six".into(),
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        truncated: false,
    });
    app.set_pending_yield(long_bash);

    let initial = render_lines(&app, 80, 10).join("\n");
    assert!(initial.contains("PgUp/PgDn scroll"), "{initial}");
    assert!(initial.contains("[1] approve (default)"), "{initial}");
    assert!(
        !initial.contains("$ head-line-one"),
        "top preview is initially clipped on 80x10: {initial}"
    );

    // PageUp / Home reveal clipped approval header, explanation, and command start.
    assert!(
        app.handle_key(key(KeyCode::PageUp, KeyModifiers::NONE))
            .is_none()
    );
    let scrolled_mid = render_lines(&app, 80, 10).join("\n");
    assert!(app.confirmation.as_ref().unwrap().scroll_from_bottom > 0);
    assert!(
        app.handle_key(key(KeyCode::Home, KeyModifiers::NONE))
            .is_none()
    );
    let scrolled_top = render_lines(&app, 80, 10).join("\n");
    assert!(
        scrolled_top.contains("Approval · Escalate tool call `bash`"),
        "{scrolled_top}"
    );
    assert!(scrolled_top.contains("$ head-line-one"), "{scrolled_top}");
    assert!(
        scrolled_top.contains("why: Runs tests outside sandbox"),
        "{scrolled_top}"
    );
    assert_ne!(scrolled_mid, initial);
    assert!(app.composer.text().is_empty());

    // Selection navigation or PageDown/End returns cleanly to options and footer.
    assert!(
        app.handle_key(key(KeyCode::Down, KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(app.confirmation.as_ref().unwrap().scroll_from_bottom, 0);
    let snapped = render_lines(&app, 80, 10).join("\n");
    assert!(snapped.contains("▶ [2] allow_session"), "{snapped}");
    assert!(snapped.contains("PgUp/PgDn scroll"), "{snapped}");
}

#[test]
fn confirmation_esc_preserves_empty_queue_auto_continue_and_respects_queued_items() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            active_submission_ids: vec!["sub-1".into()],
            ..Default::default()
        }),
    );
    app.set_pending_yield(bash_confirmation());
    assert!(matches!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE)),
        Some(FileBackedAction::Interrupt)
    ));
    assert!(app.auto_continue_on_empty_cancel);

    app.clear_pending_yield();
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 2,
            paused: true,
            ..Default::default()
        }),
    );
    app.activity = UiActivitySnapshot::paused(Some(1));
    assert!(app.take_ready_auto_continue_on_empty_cancel());
    assert!(!app.take_ready_auto_continue_on_empty_cancel());

    // When another submission is pending in the queue, Esc must not auto-continue.
    let mut queued_app = FileBackedApp::new("/agent/root".into());
    queued_app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            active_submission_ids: vec!["sub-1".into()],
            pending_submission_ids: vec!["sub-2".into()],
            ..Default::default()
        }),
    );
    queued_app.set_pending_yield(bash_confirmation());
    assert!(matches!(
        queued_app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE)),
        Some(FileBackedAction::Interrupt)
    ));
    assert!(!queued_app.auto_continue_on_empty_cancel);
}

#[test]
fn confirmation_expands_truncated_prompt_and_presentation_details_with_d() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.viewport_size = (90, 30);
    let mut pending = diff_confirmation();
    pending.prompt = Some("prompt-1\nprompt-2\nprompt-3\nprompt-4-hidden\nprompt-5-hidden".into());
    let lines = (1..=12)
        .map(|i| DiffLine::Added {
            text: format!("diff-line-{i:02}"),
        })
        .collect();
    pending.presentation = Some(ToolResultPresentation::Diff {
        path: "src/lib.rs".into(),
        hunks: vec![DiffHunk {
            header: Some("@@ -1 +1,12 @@".into()),
            lines,
        }],
    });
    app.set_pending_yield(pending);

    let collapsed = render_lines(&app, 90, 30).join("\n");
    assert!(collapsed.contains("prompt-3"), "{collapsed}");
    assert!(!collapsed.contains("prompt-4-hidden"), "{collapsed}");
    assert!(collapsed.contains("+diff-line-06"), "{collapsed}");
    assert!(!collapsed.contains("+diff-line-12"), "{collapsed}");
    assert!(collapsed.contains("(d to expand)"), "{collapsed}");

    // Press `d` to expand full prompt and full diff preview cleanly.
    assert!(
        app.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE))
            .is_none()
    );
    assert!(app.confirmation.as_ref().unwrap().expanded_details);
    let expanded = render_lines(&app, 90, 30).join("\n");
    assert!(expanded.contains("prompt-4-hidden"), "{expanded}");
    assert!(expanded.contains("prompt-5-hidden"), "{expanded}");
    assert!(expanded.contains("+diff-line-12"), "{expanded}");
    assert!(expanded.contains("d collapse"), "{expanded}");
    assert!(!expanded.contains("(d to expand)"), "{expanded}");

    // Press `d` again to collapse back to compact view cleanly without leaking `d`.
    assert!(
        app.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE))
            .is_none()
    );
    assert!(!app.confirmation.as_ref().unwrap().expanded_details);
    let recollapsed = render_lines(&app, 90, 30).join("\n");
    assert!(!recollapsed.contains("+diff-line-12"), "{recollapsed}");
    assert!(app.composer.text().is_empty());
}

#[test]
fn confirmation_supports_allow_persistent_display_submit_and_receipt() {
    let mut app = FileBackedApp::new("/agent/root".into());
    let mut pending = bash_confirmation();
    pending.options = vec![
        "approve".into(),
        "allow_session".into(),
        "allow_persistent".into(),
        "reject".into(),
    ];
    app.set_pending_yield(pending);

    let conf = app.confirmation.as_ref().unwrap();
    let rendered = conf.render(app.theme, 100, false);
    let persistent_line = rendered
        .lines
        .iter()
        .find(|line| line.to_string().contains("[3]"))
        .expect("option [3] rendered");
    assert_eq!(persistent_line.to_string(), "  [3] allow & remember");
    assert_eq!(persistent_line.spans[2].style, app.theme.accent_bold());

    // Digit 3 submits the exact wire key `allow_persistent` and settles to `Approved & remembered`.
    let action = app.handle_key(key(KeyCode::Char('3'), KeyModifiers::NONE));
    let Some(FileBackedAction::Resume {
        request_id,
        response,
        retry_input,
    }) = action
    else {
        panic!("expected Resume action on digit 3, got {action:?}");
    };
    let val: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(val, serde_json::json!({ "choice": "allow_persistent" }));
    assert_eq!(retry_input, "allow_persistent");

    app.begin_resume_write(request_id.clone());
    app.dispatch(FileBackedEvent::ResumeWriteCompleted {
        request_id,
        retry_input: "allow_persistent".into(),
        result: Ok(()),
    });
    assert!(app.confirmation.is_none());
    let settled_screen = render_lines(&app, 100, 16).join("\n");
    assert!(
        settled_screen.contains("✓ Approved & remembered · bash: cargo test -p alan-terminal-ui"),
        "{settled_screen}"
    );
    assert_eq!(
        crate::history::ConfirmationOutcome::from_choice("allow_persistent"),
        crate::history::ConfirmationOutcome::AllowedPersistently
    );
    assert_eq!(
        crate::history::ConfirmationOutcome::AllowedPersistently.label("allow_persistent"),
        "Approved & remembered"
    );
}
