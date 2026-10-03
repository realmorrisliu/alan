use super::*;
use crate::completion::CompletionCandidate;
use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::{Backend, TestBackend},
};

// Match the production Inline rebuild: position the backend at the CURRENT
// viewport origin, then construct Inline(height). Never normalize cursor Y.
pub(super) fn frame(terminal: &mut Terminal<TestBackend>, app: &FileBackedApp) -> u16 {
    let size = terminal.size().unwrap();
    let top = terminal.get_frame().area().y;
    let desired = inline_viewport_height(app, size.width as usize, size.height as usize);
    let base_height =
        super::layout::base_viewport_height(app, size.width as usize, size.height as usize);
    let height = crate::terminal::anchored_inline_height(base_height, desired, size.height, top);
    if height != terminal.get_frame().area().height {
        let mut backend = terminal.backend().clone();
        backend.set_cursor_position((0, top)).unwrap();
        *terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )
        .unwrap();
        terminal.clear().unwrap();
    }
    terminal.draw(|f| draw(f, app)).unwrap();
    terminal.backend().cursor_position().y
}

fn inline(width: u16, pane: u16, top: u16) -> Terminal<TestBackend> {
    let mut backend = TestBackend::new(width, pane);
    backend.set_cursor_position((0, top)).unwrap();
    Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(2),
        },
    )
    .unwrap()
}

fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let size = terminal.size().unwrap();
    (0..size.height)
        .map(|y| {
            (0..size.width)
                .map(|x| terminal.backend().buffer().cell((x, y)).unwrap().symbol())
                .collect()
        })
        .collect()
}

#[test]
fn actual_inline_first_ready_reserves_selected_row_even_without_spare() {
    for width in [40, 60, 73, 80, 120] {
        for pane in [3, 5, 10] {
            for trigger in ['/', '$', '@'] {
                for text in [String::new(), "界🙂 wrapped ".repeat(25)] {
                    if trigger == '/' && !text.is_empty() {
                        continue;
                    }
                    let mut app = FileBackedApp::new("/agent/root".into());
                    for i in 0..30 {
                        app.transcript
                            .push(HistoryCell::Assistant(format!("history{i}")));
                    }
                    let candidates = (0..9)
                        .map(|i| {
                            CompletionCandidate::new(
                                format!("ref{i}{}", "界🙂long".repeat(20)),
                                None,
                            )
                        })
                        .collect::<Vec<_>>();
                    app.set_skill_candidates(candidates.clone());
                    app.set_file_candidates(candidates);
                    app.composer.set_text(&text);
                    app.drain_committed_scrollback(width as usize, pane as usize);
                    let retained = app.styled_history_lines(width as usize);
                    let mut terminal = inline(width, pane, pane - 2);
                    let anchor = frame(&mut terminal, &app);
                    assert!(
                        anchor + 1 < pane,
                        "first ready has no below-input row: {width}x{pane}"
                    );
                    assert!(rows(&terminal)[anchor as usize + 1].trim().is_empty());
                    app.handle_key(KeyEvent::new(KeyCode::Char(trigger), KeyModifiers::NONE));
                    for code in [
                        KeyCode::Down,
                        KeyCode::Down,
                        KeyCode::Up,
                        KeyCode::Char('r'),
                        KeyCode::Backspace,
                    ] {
                        assert_eq!(frame(&mut terminal, &app), anchor);
                        let state = app.completion.as_ref().unwrap();
                        let label = &state.matches[state.selected].label;
                        let prefix = label
                            .chars()
                            .take(if trigger == '/' { 8 } else { 4 })
                            .collect::<String>();
                        let painted = rows(&terminal);
                        assert!(
                            painted
                                .iter()
                                .skip(anchor as usize + 1)
                                .any(|r| r.contains(&prefix)),
                            "selected row missing: {painted:?}"
                        );
                        assert!(
                            (0..anchor).all(|y| (0..width).all(|x| terminal
                                .backend()
                                .buffer()
                                .cell((x, y))
                                .unwrap()
                                .bg
                                != ratatui::style::Color::Cyan)),
                            "selected candidate above composer: {painted:?}"
                        );
                        assert!(
                            app.drain_committed_scrollback(width as usize, pane as usize)
                                .is_empty()
                        );
                        assert_eq!(app.styled_history_lines(width as usize), retained);
                        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
                    }
                    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
                    frame(&mut terminal, &app); // accepted long text may legitimately wrap
                    assert!(app.completion.is_none());
                }
            }
        }
    }
}

#[test]
fn real_inline_absolute_anchor_survives_transient_candidates() {
    for width in [40, 60, 73, 80, 120] {
        for pane in [3, 5, 10, 22] {
            for spare in [0, 1, 2] {
                let mut terminal = inline(width, pane, pane.saturating_sub(2 + spare));
                let mut app = FileBackedApp::new("/agent/root".into());
                let anchor = frame(&mut terminal, &app);
                for code in [
                    KeyCode::Char('/'),
                    KeyCode::Char('p'),
                    KeyCode::Char('r'),
                    KeyCode::Backspace,
                    KeyCode::Down,
                    KeyCode::Up,
                    KeyCode::Esc,
                ] {
                    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
                    assert_eq!(
                        frame(&mut terminal, &app),
                        anchor,
                        "absolute cursor moved: width={width} pane={pane} spare={spare} key={code:?}"
                    );
                    let painted = rows(&terminal);
                    assert!(
                        painted
                            .iter()
                            .take(anchor as usize)
                            .all(|row| !row.contains("/project"))
                    );
                    if spare > 0 && app.completion.is_some() {
                        assert!(
                            painted
                                .iter()
                                .skip(anchor as usize + 1)
                                .any(|row| row.contains("/")),
                            "readable menu missing: {painted:?}"
                        );
                    }
                }
                app.composer.set_text("/");
                app.refresh_completion();
                frame(&mut terminal, &app);
                app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
                assert_eq!(frame(&mut terminal, &app), anchor);
            }
        }
    }
}

#[test]
fn real_inline_unicode_reference_menu_clips_below_visible_composer() {
    for width in [40, 60, 73, 80, 120] {
        for pane in [3, 5, 10, 22, 30] {
            for bottom in [true, false] {
                for trigger in ['$', '@'] {
                    for text in [
                        "界🙂 ".to_string(),
                        format!("{}\n界🙂 ", "中文🙂 ".repeat(18)),
                        format!("{}界🙂 ", "line\n".repeat(15)),
                    ] {
                        let mut app = FileBackedApp::new("/agent/root".into());
                        let candidates = (0..9)
                            .map(|i| {
                                CompletionCandidate::new(
                                    format!("ref{i}"),
                                    Some("中文详情🙂 ".repeat(12)),
                                )
                            })
                            .collect::<Vec<_>>();
                        app.set_skill_candidates(candidates.clone());
                        app.set_file_candidates(candidates);
                        app.composer.set_text(&text);
                        let mut terminal = inline(width, pane, if bottom { pane - 2 } else { 0 });
                        let anchor = frame(&mut terminal, &app);
                        let origin = terminal.get_frame().area().y;
                        let before = rows(&terminal);
                        app.handle_key(KeyEvent::new(KeyCode::Char(trigger), KeyModifiers::NONE));
                        assert_eq!(frame(&mut terminal, &app), anchor);
                        assert_eq!(terminal.get_frame().area().y, origin);
                        assert_eq!(
                            &rows(&terminal)[origin as usize..anchor as usize],
                            &before[origin as usize..anchor as usize],
                            "menu displaced wrapped input"
                        );
                        for _ in 0..8 {
                            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                            assert_eq!(frame(&mut terminal, &app), anchor);
                        }
                        let painted = rows(&terminal);
                        assert!(
                            painted
                                .iter()
                                .take(anchor as usize + 1)
                                .all(|r| !r.contains("ref"))
                        );
                        if anchor + 1 < pane {
                            assert!(
                                painted
                                    .iter()
                                    .skip(anchor as usize + 1)
                                    .any(|r| r.contains("ref8")),
                                "selected item not disclosed: {painted:?}"
                            );
                        }
                        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
                        assert!(app.composer.text().ends_with(&format!("{trigger}ref8 ")));
                        assert_eq!(frame(&mut terminal, &app), anchor);
                        app.composer.set_text(format!("{text}{trigger}"));
                        app.refresh_completion();
                        frame(&mut terminal, &app);
                        for code in [KeyCode::Backspace, KeyCode::Esc] {
                            app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
                            assert_eq!(frame(&mut terminal, &app), anchor);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn transient_menu_does_not_permanently_drain_near_full_history() {
    for width in [40, 60, 73, 80, 120] {
        for pane in [3, 5, 10, 22] {
            for trigger in ['/', '$', '@'] {
                let mut app = FileBackedApp::new("/agent/root".into());
                for i in 0..30 {
                    app.transcript
                        .push(HistoryCell::Assistant(format!("history{i}")));
                }
                app.drain_committed_scrollback(width, pane);
                let retained = app.styled_history_lines(width);
                assert_eq!(retained.is_empty(), pane == 3);
                let candidates = (0..9)
                    .map(|i| {
                        CompletionCandidate::new(
                            format!("ref{i}"),
                            Some("中文🙂 detail ".repeat(12)),
                        )
                    })
                    .collect::<Vec<_>>();
                app.set_skill_candidates(candidates.clone());
                app.set_file_candidates(candidates);
                app.handle_key(KeyEvent::new(KeyCode::Char(trigger), KeyModifiers::NONE));
                assert!(
                    app.drain_committed_scrollback(width, pane).is_empty(),
                    "menu drained history: {width}x{pane} {trigger}"
                );
                for code in [
                    KeyCode::Char('r'),
                    KeyCode::Backspace,
                    KeyCode::Down,
                    KeyCode::Up,
                    KeyCode::Tab,
                    KeyCode::Esc,
                ] {
                    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
                    assert!(app.drain_committed_scrollback(width, pane).is_empty());
                }
                assert_eq!(app.styled_history_lines(width), retained);
                app.composer.set_text("one\ntwo\nthree");
                let growth_drained = app.drain_committed_scrollback(width, pane);
                assert_eq!(
                    growth_drained.is_empty(),
                    retained.is_empty(),
                    "legitimate composer growth must drain any retained history"
                );
            }
        }
    }
}
