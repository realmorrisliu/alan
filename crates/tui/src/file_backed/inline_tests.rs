use super::*;

#[test]
fn scrollback_drains_by_rendered_lines() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.transcript
        .push(HistoryCell::Assistant("long streamed output ".repeat(40)));
    let before = app.rendered_history_lines(32);

    let drained = app.drain_committed_scrollback(32, 10);
    let retained = app.rendered_history_lines(32);

    assert!(!drained.is_empty());
    assert_eq!(drained, before[..drained.len()]);
    assert_eq!(retained, before[drained.len()..]);
    assert!(retained.len() + live_region_height(&app, 32) as usize <= 10);
    assert!(matches!(app.transcript[0], HistoryCell::Assistant(_)));
}

#[test]
fn ctrl_d_at_an_empty_prompt_detaches_without_interrupting_the_agent() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.apply_ui_activity_snapshot(UiActivitySnapshot::running(1));

    let action = press(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);

    assert!(matches!(action, Some(FileBackedAction::Quit)));
    assert_eq!(app.activity.state, UiActivityState::Running);
    assert!(app.should_quit);
}

#[test]
fn ctrl_d_with_prompt_text_does_not_detach() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("keep editing");

    let action = press(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);

    assert!(action.is_none());
    assert!(!app.should_quit);
    assert_eq!(app.composer.text(), "keep editing");
}

#[test]
fn completed_turn_is_followed_by_the_next_inline_alan_prompt() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript = vec![
        HistoryCell::User("pwd".to_string()),
        HistoryCell::Assistant("/workspace/alan".to_string()),
    ];

    let backend = render(&app);
    let line = |row| {
        (0..80)
            .map(|column| {
                backend
                    .buffer()
                    .cell((column, row))
                    .expect("buffer cell")
                    .symbol()
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    };

    assert_eq!(line(0), "alan > pwd");
    assert_eq!(line(1), "/workspace/alan");
    assert_eq!(line(2), "alan >");
    assert_eq!(
        backend.cursor_position(),
        ratatui::layout::Position::new(7, 2)
    );
}

#[test]
fn completion_candidates_render_before_the_inline_prompt_without_entering_history() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript = vec![
        HistoryCell::User("pwd".to_string()),
        HistoryCell::Assistant("/workspace/alan".to_string()),
    ];
    app.composer.set_text("/");
    app.refresh_completion();

    let backend = render(&app);
    let line = |row| {
        (0..80)
            .map(|column| backend.buffer().cell((column, row)).unwrap().symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    };

    assert!(line(2).contains("/compact"));
    assert!(line(4).contains("/clear"));
    assert_eq!(line(7), "alan > /");
    assert_eq!(app.transcript.len(), 2, "candidates are transient UI state");
    assert_eq!(inline_viewport_height(&app, 80, 12), 8);
}

#[test]
fn wrapped_completion_candidates_reserve_their_rendered_height_before_the_prompt() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("/");
    app.refresh_completion();
    let width = 12;
    let height = inline_viewport_height(&app, width, 30);

    let mut terminal = Terminal::new(TestBackend::new(width as u16, height)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let prompt_row = (0..height)
        .find(|row| {
            let text = (0..width)
                .map(|column| {
                    terminal
                        .backend()
                        .buffer()
                        .cell((column as u16, *row))
                        .unwrap()
                        .symbol()
                })
                .collect::<String>();
            text.starts_with("alan > /")
        })
        .expect("the editable prompt is visible after the wrapped candidates");
    assert_eq!(terminal.backend().cursor_position().y, prompt_row);
}

#[test]
fn inline_viewport_reflows_with_terminal_size_and_stays_screen_bounded() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript
        .push(HistoryCell::Assistant("long output ".repeat(40)));

    let wide_height = inline_viewport_height(&app, 80, 24);
    let narrow_height = inline_viewport_height(&app, 20, 24);

    assert!(narrow_height > wide_height);
    assert_eq!(inline_viewport_height(&app, 20, 3), 3);
}

#[test]
fn wrapped_multiline_prompt_keeps_its_cursor_in_the_inline_viewport() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("\nx");

    let height = inline_viewport_height(&app, 8, 24);
    assert_eq!(height, 3);

    let mut terminal = Terminal::new(TestBackend::new(8, height)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    assert_eq!(
        terminal.backend().cursor_position(),
        ratatui::layout::Position::new(0, 2)
    );
}

#[test]
fn preceding_wrapped_prompt_lines_are_included_in_the_cursor_row() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("abcdefghijk\nx");

    let height = inline_viewport_height(&app, 8, 24);
    let mut terminal = Terminal::new(TestBackend::new(8, height)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();

    assert_eq!(
        terminal.backend().cursor_position(),
        ratatui::layout::Position::new(0, 4)
    );
    assert_eq!(height, 5);
}

#[test]
fn multiline_unicode_paste_stays_inline_and_positions_the_cursor_by_display_width() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        "你好\nx".to_string(),
    )));

    let backend = render(&app);
    let line = |row| {
        (0..80)
            .map(|column| backend.buffer().cell((column, row)).unwrap().symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    };

    assert!(line(0).contains("你") && line(0).contains("好"));
    assert_eq!(line(1), "       x");
    assert_eq!(
        backend.cursor_position(),
        ratatui::layout::Position::new(8, 1)
    );
}
