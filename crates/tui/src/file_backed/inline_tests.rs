use super::*;

#[test]
fn semantic_markdown_production_buffer_removes_markers_and_preserves_roles() {
    use ratatui::style::{Color, Modifier};
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.transcript.push(HistoryCell::Assistant(
            "# 标题\n**strong** and *emphasis* and `literal`\n- item\n```rust\n    let 界 = \"**literal**\";\n```\n```diff\n+    新增\n-    删除\n```\n: prose\n! prose\ntool> prose".into(),
        ));
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer.cell((0, 0)).unwrap().symbol(),
            "标",
            "heading marker at {width}"
        );
        assert!(
            buffer
                .cell((0, 0))
                .unwrap()
                .modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            buffer
                .cell((0, 1))
                .unwrap()
                .modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            buffer
                .cell((11, 1))
                .unwrap()
                .modifier
                .contains(Modifier::ITALIC)
        );
        assert_eq!(buffer.cell((0, 2)).unwrap().symbol(), "•");
        assert_eq!(buffer.cell((4, 4)).unwrap().symbol(), "l");
        assert_eq!(buffer.cell((0, 7)).unwrap().symbol(), "+");
        assert_eq!(buffer.cell((0, 7)).unwrap().fg, Color::Green);
        assert_eq!(buffer.cell((0, 8)).unwrap().fg, Color::Red);
        for row in [10, 11, 12] {
            assert_eq!(
                buffer.cell((0, row)).unwrap().fg,
                Color::Reset,
                "prose must not acquire role styles"
            );
        }
    }
}

#[test]
fn typed_diff_and_partial_scrollback_keep_styles_and_literal_indentation() {
    use crate::history::ToolStatus;
    use alan_agent_protocol::{DiffHunk, DiffLine, ToolResultPresentation};
    use ratatui::style::Color;
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.transcript.push(HistoryCell::Tool {
            title: "Edit".into(),
            status: ToolStatus::Complete,
            preview: None,
            presentation: Some(ToolResultPresentation::Diff {
                path: "example.rs".into(),
                hunks: vec![DiffHunk {
                    header: Some("@@ -1 +1 @@".into()),
                    lines: vec![
                        DiffLine::Removed {
                            text: "    old".into(),
                        },
                        DiffLine::Added {
                            text: "    新".into(),
                        },
                        DiffLine::Context {
                            text: "    ! literal".into(),
                        },
                    ],
                }],
            }),
        });
        let before = app.styled_history_lines(width);
        let drained = app.drain_committed_scrollback(width, 5);
        let retained = app.styled_history_lines(width);
        assert!(!drained.is_empty() && !retained.is_empty());
        assert_eq!([drained, retained.clone()].concat(), before);
        assert!(matches!(app.transcript[0], HistoryCell::Styled(_)));
        let mut terminal = Terminal::new(TestBackend::new(width as u16, 12)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(before.clone()),
                    frame.area(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((7, 3)).unwrap().symbol(), "-");
        assert_eq!(buffer.cell((7, 3)).unwrap().fg, Color::Red);
        assert_eq!(buffer.cell((12, 4)).unwrap().symbol(), "新");
        assert_eq!(buffer.cell((12, 4)).unwrap().fg, Color::Green);
        assert_eq!(buffer.cell((12, 5)).unwrap().symbol(), "!");
        assert_eq!(buffer.cell((12, 5)).unwrap().fg, Color::Reset);
        assert!(
            retained
                .iter()
                .any(|line| line.to_string().contains("    ! literal"))
        );
    }
}

#[test]
fn partial_markdown_drain_keeps_fence_context_during_stream_append_and_resize() {
    use ratatui::style::{Color, Modifier};
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.transcript.push(HistoryCell::Assistant(
            "# Heading\n```diff\n+    first\n-    second".into(),
        ));
        let before = app.styled_history_lines(width);
        let drained = app.drain_committed_scrollback(width, 5);
        let retained = app.styled_history_lines(width);
        assert!(!drained.is_empty());
        assert_eq!([drained, retained].concat(), before);
        app.append_to_open_assistant_cell("\n+    界\n```\n**done**".into());
        for resized in [40, 60, 80, 120] {
            let mut terminal = Terminal::new(TestBackend::new(resized, 12)).unwrap();
            terminal.draw(|frame| draw(frame, &app)).unwrap();
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer.cell((0, 0)).unwrap().fg, Color::Green);
            assert_eq!(buffer.cell((0, 1)).unwrap().fg, Color::Red);
            assert_eq!(buffer.cell((5, 2)).unwrap().symbol(), "界");
            assert_eq!(buffer.cell((5, 2)).unwrap().fg, Color::Green);
            assert!(
                buffer
                    .cell((0, 4))
                    .unwrap()
                    .modifier
                    .contains(Modifier::BOLD)
            );
        }
    }
}

#[test]
fn quiet_running_clock_redraws_only_when_displayed_second_changes() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.apply_ui_activity_snapshot(UiActivitySnapshot::running(1_250));
    assert_eq!(activity_elapsed_second(&app, 1_249), 0);
    assert_eq!(activity_elapsed_second(&app, 2_249), 0);
    assert_eq!(activity_elapsed_second(&app, 2_250), 1);
    assert!(frame_needs_redraw(true, &app, 1_250, None));
    assert!(frame_needs_redraw(false, &app, 1_250, None));

    // Model 33 ms ticks after drawing 0s, without any UI/file events.
    for now_ms in (1_283..2_250).step_by(33) {
        assert!(
            !frame_needs_redraw(false, &app, now_ms, Some(0)),
            "same displayed second must not redraw the active viewport"
        );
    }
    assert!(frame_needs_redraw(false, &app, 2_250, Some(0)));
    assert!(!frame_needs_redraw(false, &app, 2_283, Some(1)));
    assert!(frame_needs_redraw(true, &app, 2_283, Some(1)));
    assert!(frame_needs_redraw(false, &app, 5_250, Some(1)));

    for snapshot in [UiActivitySnapshot::idle(), UiActivitySnapshot::paused(None)] {
        app.apply_ui_activity_snapshot(snapshot);
        assert!(frame_needs_redraw(true, &app, 6_250, Some(1)));
        assert!(!frame_needs_redraw(false, &app, 6_250, Some(1)));
    }
}

#[test]
fn blank_prompt_cursor_uses_nonzero_viewport_origin() {
    let app = FileBackedApp::new("/agent/root".to_string());
    let area = ratatui::layout::Rect::new(4, 5, 80, 2);
    let mut terminal = Terminal::with_options(
        TestBackend::new(90, 12),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        },
    )
    .unwrap();

    terminal
        .draw(|frame| {
            assert_eq!(frame.area(), area);
            draw(frame, &app);
        })
        .unwrap();

    assert_eq!(
        terminal.backend().buffer().cell((4, 6)).unwrap().symbol(),
        ":"
    );
    assert_eq!(
        terminal.backend().cursor_position(),
        ratatui::layout::Position::new(6, 6)
    );
}

#[test]
fn wrapped_unicode_draft_cursor_uses_nonzero_viewport_origin_after_scroll() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("abcdefghijk\n界終");
    let area = ratatui::layout::Rect::new(4, 5, 8, 3);
    let mut terminal = Terminal::with_options(
        TestBackend::new(20, 12),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        },
    )
    .unwrap();

    terminal
        .draw(|frame| {
            assert_eq!(frame.area(), area);
            draw(frame, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer.cell((6, 6)).unwrap().symbol(), "界");
    assert_eq!(buffer.cell((8, 6)).unwrap().symbol(), "終");
    assert_eq!(
        terminal.backend().cursor_position(),
        ratatui::layout::Position::new(10, 6)
    );
}

#[test]
fn scrollback_drains_by_rendered_lines() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.transcript
        .push(HistoryCell::Assistant("long streamed output ".repeat(40)));
    let before = app.styled_history_lines(32);

    let drained = app.drain_committed_scrollback(32, 10);
    let retained = app.styled_history_lines(32);

    assert!(!drained.is_empty());
    assert_eq!(drained, before[..drained.len()]);
    assert_eq!(
        retained.iter().map(ToString::to_string).collect::<Vec<_>>(),
        before[drained.len()..]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    assert!(retained.len() + live_region_height(&app, 32) as usize <= 10);
    assert!(matches!(
        app.transcript[0],
        HistoryCell::AssistantTail { .. }
    ));
}

#[test]
fn scrollback_retention_counts_physical_rows_at_narrow_widths() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.transcript = ["abcdefghijklmnop", "qrstuvwxyzabcdef", "uvwxyzabcdefghij"]
        .into_iter()
        .map(|text| HistoryCell::Assistant(text.to_string()))
        .collect();

    let drained = app.drain_committed_scrollback(8, 5);
    let retained = app.rendered_history_lines(8);
    let height = inline_viewport_height(&app, 8, 5);
    let mut terminal = Terminal::new(TestBackend::new(8, height)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();

    assert_eq!(drained.len(), 2);
    assert_eq!(retained.len(), 1);
    assert_eq!(terminal.backend().cursor_position().y, 3);
    let prompt = (0..8)
        .map(|column| {
            terminal
                .backend()
                .buffer()
                .cell((column, 3))
                .unwrap()
                .symbol()
        })
        .collect::<String>();
    assert!(prompt.starts_with(":"));
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
fn ctrl_d_does_not_discard_a_whitespace_only_draft() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("   ");

    let action = press(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);

    assert!(action.is_none());
    assert!(!app.should_quit);
    assert_eq!(app.composer.text(), "   ");
}

#[test]
fn ctrl_d_does_not_detach_while_agent_input_is_pending() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
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

    let action = press(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);

    assert!(action.is_none());
    assert!(!app.should_quit);
    assert!(app.pending_yield.is_some());
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

    assert_eq!(line(0), ": pwd");
    assert_eq!(line(1), "/workspace/alan");
    assert!(line(2).starts_with("no project · model unknown · ready"));
    assert_eq!(line(3), ":");
    assert_eq!(
        backend.cursor_position(),
        ratatui::layout::Position::new(2, 3)
    );
}

#[test]
fn prompt_shows_the_process_selected_next_model() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    crate::file_backed::model_tests::install_header_model(&mut app, "gpt-6-luna");

    let backend = render(&app);
    let line = (0..80)
        .map(|column| backend.buffer().cell((column, 0)).unwrap().symbol())
        .collect::<String>()
        .trim_end()
        .to_string();

    assert!(line.starts_with("no project · next gpt-6-luna · high · ready"));
}

#[test]
fn model_and_status_remain_visible_across_prompt_widths() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    crate::file_backed::model_tests::install_header_model(&mut app, "gpt-6-luna");
    app.apply_ui_activity_snapshot(UiActivitySnapshot::running(1));

    for width in [40, 60, 80, 120] {
        let line = app.context_line(width);
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(text.contains("next gpt-6-luna"), "{text}");
        assert!(text.ends_with("working"), "{text}");
    }

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
    for (width, status) in [(40, "approval"), (60, "waiting for approval")] {
        let line = app.context_line(width);
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(text.contains("next gpt-6-luna"), "{text}");
        assert!(text.ends_with(status), "{text}");
    }
}

#[test]
fn slash_project_character_sequence_keeps_dynamic_hints_cursor_and_history() {
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new("/agent/root".into());
        let mut typed = String::new();
        let mut heights = Vec::new();
        for ch in "/project".chars() {
            typed.push(ch);
            assert!(
                app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
                    .is_none()
            );
            assert_eq!(app.composer.text(), typed);
            assert_eq!(app.composer.cursor(), typed.len());
            let state = app
                .completion
                .as_ref()
                .expect("useful slash hints remain visible");
            let labels = state
                .matches
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>();
            assert!(labels.contains(&"project"), "{typed}: {labels:?}");
            if typed.len() >= 3 {
                assert_eq!(labels, ["project"]);
            }
            let expected_hint_rows = state
                .matches
                .iter()
                .take(MAX_COMPLETION_ROWS)
                .map(|c| {
                    let hint = ratatui::text::Line::from(format!(
                        "  /{}  - {}",
                        c.label,
                        c.detail.as_ref().unwrap()
                    ));
                    crate::transcript_ui::wrapped_line_count(&[hint], width)
                })
                .sum::<usize>();
            let height = inline_viewport_height(&app, width, 22);
            assert_eq!(height as usize, expected_hint_rows + 2, "{width}: {typed}");
            heights.push(height);
            let mut terminal = Terminal::new(TestBackend::new(width as u16, height)).unwrap();
            terminal.draw(|frame| draw(frame, &app)).unwrap();
            let rendered = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| {
                            terminal
                                .backend()
                                .buffer()
                                .cell((x as u16, y))
                                .unwrap()
                                .symbol()
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                rendered.contains("/project"),
                "{width}: {typed}: {rendered}"
            );
            assert!(rendered.contains("select or revoke"), "{rendered}");
            let cursor = terminal.backend().cursor_position();
            assert_eq!((cursor.x, cursor.y), ((2 + typed.len()) as u16, 1));
            assert!(app.drain_committed_scrollback(width, 22).is_empty());
            assert!(
                app.transcript.is_empty(),
                "completion rows must remain transient"
            );
        }
        assert!(
            heights[0] > heights[1] && heights[1] > heights[2],
            "{width}: {heights:?}"
        );
        assert!(heights[2..].iter().all(|h| *h == heights[2]));
        let saved_height = *heights.last().unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        assert_eq!(inline_viewport_height(&app, width, 22), 22);
        app.modal.rows = vec![ratatui::text::Line::from("transient detail page")];
        assert!(app.drain_committed_scrollback(width, 22).is_empty());
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(inline_viewport_height(&app, width, 22), saved_height);
        assert_eq!(app.composer.text(), "/project");
        assert!(app.transcript.is_empty());
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(app.composer.text().starts_with("/project"));
        app.handle_key(KeyEvent::new(KeyCode::Char('界'), KeyModifiers::NONE));
        assert!(app.composer.text().contains('界'));
        assert_eq!(app.composer.cursor(), app.composer.text().len());
    }
}

#[test]
fn completion_candidates_render_below_the_inline_prompt_without_entering_history() {
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

    assert_eq!(line(3), ": /");
    assert!(line(4).contains("/project"));
    assert!(line(5).contains("/compact"));
    assert!(line(7).contains("/continue"));
    assert!(line(8).contains("/discard"));
    assert!(line(9).contains("/clear"));
    assert_eq!(app.transcript.len(), 2, "candidates are transient UI state");
    assert_eq!(inline_viewport_height(&app, 80, 12), 10);
}

#[test]
fn wrapped_completion_candidates_reserve_their_rendered_height_below_the_prompt() {
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
            text.starts_with(": /")
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
    assert_eq!(height, 4);

    let mut terminal = Terminal::new(TestBackend::new(8, height)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    assert_eq!(
        terminal.backend().cursor_position(),
        ratatui::layout::Position::new(3, 2)
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
        ratatui::layout::Position::new(3, 4)
    );
    assert_eq!(height, 6);
}

#[test]
fn word_wrapped_current_prompt_positions_cursor_after_the_rendered_word() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("hi longword");

    let width = 12;
    let height = inline_viewport_height(&app, width, 24);
    let mut terminal = Terminal::new(TestBackend::new(width as u16, height)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();

    assert_eq!(
        terminal.backend().cursor_position(),
        ratatui::layout::Position::new(8, 2)
    );
}

#[test]
fn long_composer_scrolls_its_editable_tail_and_cursor_into_view() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text(format!("{}x", "a\n".repeat(10)));

    let height = inline_viewport_height(&app, 10, 24);
    let mut terminal = Terminal::new(TestBackend::new(10, height)).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();

    assert_eq!(height, 12);
    assert_eq!(
        terminal.backend().cursor_position(),
        ratatui::layout::Position::new(3, 10)
    );
    let last_row = (0..10)
        .map(|column| {
            terminal
                .backend()
                .buffer()
                .cell((column, 10))
                .unwrap()
                .symbol()
        })
        .collect::<String>();
    assert_eq!(last_row.trim_end(), "  x");
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

    assert!(line(1).contains("你") && line(1).contains("好"));
    assert_eq!(line(2), "  x");
    assert_eq!(
        backend.cursor_position(),
        ratatui::layout::Position::new(3, 2)
    );
}

#[test]
fn folded_prefix_uses_visible_body_geometry_at_multiple_widths() {
    for width in [8, 12, 80] {
        let mut ordinary = FileBackedApp::new("/agent/root".into());
        ordinary.composer.set_text("你好 abcdef\nx");
        let height = inline_viewport_height(&ordinary, width, 24);
        let mut expected = Terminal::new(TestBackend::new(width as u16, height)).unwrap();
        expected.draw(|frame| draw(frame, &ordinary)).unwrap();
        for prefix in ["!", ":"] {
            let mut app = FileBackedApp::new("/agent/root".into());
            app.insert_input_text(&format!("{prefix}你好 abcdef\nx"));
            assert_eq!(inline_viewport_height(&app, width, 24), height);
            let mut actual = Terminal::new(TestBackend::new(width as u16, height)).unwrap();
            actual.draw(|frame| draw(frame, &app)).unwrap();
            assert_eq!(
                actual.backend().cursor_position(),
                expected.backend().cursor_position()
            );
        }
    }
}

#[test]
fn pasted_controls_keep_original_utf8_cursor_offsets() {
    let mut inputs = ["\t中", "a\t中", "\t👩‍💻e\u{301}", "\u{1b}中\0", "\t中\n\t界x"]
        .map(str::to_owned)
        .to_vec();
    inputs.push(format!("\t{}👩‍💻e\u{301}", "界".repeat(70)));
    for width in [40, 60, 73, 80, 120] {
        for input in &inputs {
            let mut app = FileBackedApp::new("/agent/root".into());
            app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
                input.clone(),
            )));
            assert_eq!(app.composer.text(), input);
            let visible = input
                .chars()
                .filter(|c| !c.is_control() || *c == '\n')
                .collect::<String>();
            let mut expected = FileBackedApp::new("/agent/root".into());
            expected.composer.set_text(visible);
            for _ in 0..3 {
                let mut actual_terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
                actual_terminal.draw(|frame| draw(frame, &app)).unwrap();
                let mut expected_terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
                expected_terminal
                    .draw(|frame| draw(frame, &expected))
                    .unwrap();
                assert_eq!(
                    actual_terminal.backend().buffer(),
                    expected_terminal.backend().buffer()
                );
                assert_eq!(
                    actual_terminal.backend().cursor_position(),
                    expected_terminal.backend().cursor_position()
                );
                app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
                // Controls have no terminal width; derive expected cursor from the original prefix.
                let before = &app.composer.text()[..app.composer.cursor()];
                let cursor = before
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\n')
                    .map(char::len_utf8)
                    .sum();
                expected
                    .composer
                    .set_text_with_cursor(expected.composer.text().to_owned(), cursor);
            }
        }
    }
}

#[test]
fn compact_header_keeps_project_access_model_and_effort_at_pane_width() {
    let mut app = FileBackedApp::new("/agent/root".into());
    crate::file_backed::model_tests::install_header_model(&mut app, "gpt-6.1-sol");
    app.model
        .snapshot
        .as_mut()
        .unwrap()
        .selected_next
        .as_mut()
        .unwrap()
        .reasoning
        .effort = Some(alan_agent_protocol::ReasoningEffort::Medium);
    app.project = Some(ProjectMountReceipt {
        grant_id: "grant".into(),
        namespace_path: "/mnt/project".into(),
        label: "fixture".into(),
        access: ProjectAccess::ReadOnly,
    });
    app.namespace_cwd = "/mnt/project".into();
    for known in [false, true] {
        app.queue.apply(
            "/agent/1",
            Some(alan_agent_protocol::UiQueueSnapshot {
                known,
                revision: u64::from(known),
                ..Default::default()
            }),
        );
        for width in [69, 73, 80, 120] {
            let line = app.context_line(width);
            let text = line.to_string();
            assert!(line.width() <= width, "{width}: {text}");
            assert!(text.contains("/fixture · read-only"), "{width}: {text}");
            assert!(text.contains("next gpt-6.1-sol"), "{width}: {text}");
            assert!(text.contains(if known { "queued 0" } else { "queue unknown" }));
            if known || width >= 73 {
                assert!(text.contains("medium"), "{width}: {text}");
            }
        }
    }
}
