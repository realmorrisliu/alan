use super::*;
use crate::terminal::{
    TerminalStylePolicy, render_frame_with_policy, render_scrollback_with_policy,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};

#[test]
fn accessibility_native_policy_covers_live_details_and_committed_output() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.transcript.push(HistoryCell::Assistant(
        "# heading\n```diff\n+ added\n```".into(),
    ));
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    for (no_color, term, colors, attributes) in [
        (true, "xterm-256color", 256, true),
        (false, "dumb", 8, false),
        (false, "vt100", 8, true),
    ] {
        let policy = TerminalStylePolicy::from_capabilities(no_color, term, colors);
        for details in [false, true] {
            app.modal.active = details;
            app.modal.rows = app.styled_history_lines(80);
            let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
            terminal
                .draw(|frame| render_frame_with_policy(frame, policy, |frame| draw(frame, &app)))
                .unwrap();
            let buffer = terminal.backend().buffer();
            assert!(buffer.content.iter().any(|cell| cell.symbol() != " "));
            for cell in &buffer.content {
                assert_eq!(cell.fg, Color::Reset, "{term} live/details foreground");
                assert_eq!(cell.bg, Color::Reset, "{term} live/details background");
                if !attributes {
                    assert!(cell.modifier.is_empty());
                }
            }
        }
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 12));
        render_scrollback_with_policy(&mut buffer, app.styled_history_lines(80), policy);
        assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), "h");
        for cell in &buffer.content {
            assert_eq!(cell.fg, Color::Reset, "{term} scrollback");
            assert_eq!(cell.bg, Color::Reset);
            if !attributes {
                assert!(cell.modifier.is_empty());
            }
        }
    }
}

#[test]
fn accessibility_critical_model_status_and_interrupt_use_default_foreground() {
    let mut app = FileBackedApp::new("/agent/root".into());
    crate::file_backed::model_tests::install_header_model(&mut app, "model-critical");
    app.apply_ui_activity_snapshot(UiActivitySnapshot::running(1_250));
    let mut terminal = Terminal::new(TestBackend::new(120, 8)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    for needle in ["model-critical", "working", "ctrl+c/esc interrupt"] {
        let mut found = false;
        for y in 0..8 {
            let row = (0..120)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>();
            if let Some(start) = row.find(needle) {
                found = true;
                for x in start..start + needle.len() {
                    assert_eq!(
                        buffer.cell((x as u16, y)).unwrap().fg,
                        Color::Reset,
                        "{needle}"
                    );
                }
            }
        }
        assert!(found, "missing {needle}");
    }
}

#[test]
fn accessibility_selected_completion_text_cue_keeps_two_cell_prefix_and_anchor() {
    for width in [40, 73, 120] {
        let mut app = FileBackedApp::new("/agent/root".into());
        let mut terminal = Terminal::new(TestBackend::new(width, 10)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let anchor = terminal.backend().cursor_position().y;
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert_eq!(terminal.backend().cursor_position().y, anchor);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((0, anchor + 1)).unwrap().symbol(), "▶");
        assert_eq!(buffer.cell((1, anchor + 1)).unwrap().symbol(), " ");
        assert_eq!(buffer.cell((2, anchor + 1)).unwrap().symbol(), "/");
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert_eq!(terminal.backend().cursor_position().y, anchor);
        let buffer = terminal.backend().buffer();
        let cues = (anchor + 1..10)
            .filter(|y| buffer.cell((0, *y)).unwrap().symbol() == "▶")
            .collect::<Vec<_>>();
        assert_eq!(cues.len(), 1);
        assert!(cues[0] > anchor + 1);
        assert_eq!(buffer.cell((1, cues[0])).unwrap().symbol(), " ");
        assert_eq!(buffer.cell((2, cues[0])).unwrap().symbol(), "/");
    }
}

#[test]
fn accessibility_ordinary_code_keeps_literal_projection_and_source_slot_cutoffs() {
    let source = "prose\n```rust\n\t界🙂 **literal** `tick`\n    tail\n```\nprose";
    let mut cell = HistoryCell::Assistant(source.into());
    let opts = crate::history::RenderOpts::new(16, false);
    let before = cell.render_styled_lines(opts);
    assert_eq!(before.first().unwrap().to_string(), "prose");
    assert_eq!(before.last().unwrap().to_string(), "prose");
    let code = &before[2..before.len() - 2];
    let literal = code.iter().map(ToString::to_string).collect::<String>();
    assert_eq!(literal, "    界🙂 **literal** `tick`    tail");
    assert!(
        code.iter()
            .flat_map(|line| &line.spans)
            .all(|span| span.style == ratatui::style::Style::default()),
        "fenced body must retain terminal-default contrast without wholesale underline"
    );
    assert!(
        before[0]
            .spans
            .iter()
            .all(|span| !span.style.add_modifier.contains(Modifier::UNDERLINED))
    );
    for boundary in [&before[1], &before[before.len() - 2]] {
        assert!(
            boundary.spans.iter().all(|span| {
                span.style.fg.is_none() && span.style.add_modifier == Modifier::DIM
            })
        );
    }
    assert!(cell.trim_rendered_prefix(opts, 3));
    assert_eq!(cell.render_styled_lines(opts), before[3..]);
    // The committed projection must not return after resize or source hydration.
    cell.replace_assistant_source(source.into());
    let resized = cell.render_styled_lines(crate::history::RenderOpts::new(40, false));
    assert!(!resized.iter().any(|line| line.to_string().contains("界")));
    assert!(resized.iter().any(|line| line.to_string().contains("tail")));
}
