use super::*;
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};

#[test]
fn semantic_empty_tail_does_not_block_completed_action_drain() {
    for width in [40, 60, 80, 120] {
        let source = "```diff\n+committed";
        let delta = "\n-literal\n+next\n```";
        let full = format!("{source}{delta}");
        let mut app = FileBackedApp::new("/agent/root".into());
        app.transcript.push(HistoryCell::User("task".into()));
        app.push_output(source.into());
        assert_eq!(
            app.drain_committed_scrollback(width, 1)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [": task", "", "╭─ diff", "+committed"]
        );
        assert!(app.styled_history_lines(width).is_empty());
        for id in ["first", "second"] {
            file_surface::sync_action_snapshot(
                &mut app,
                ActionSnapshot {
                    id: id.into(),
                    name: format!("edit {id}"),
                    status: "completed".into(),
                    output: format!("output {id}"),
                    result: "success".into(),
                },
            );
        }
        let tool_rows = app.styled_history_lines(width);
        assert!(!tool_rows.is_empty());
        assert_eq!(app.action_cells.get("first"), Some(&1));
        assert_eq!(app.action_cells.get("second"), Some(&2));
        assert_eq!(
            app.drain_committed_scrollback(width, 1),
            tool_rows,
            "empty assistant tail blocked completed actions at width {width}"
        );
        assert!(app.action_cells.is_empty());
        assert!(app.styled_history_lines(width).is_empty());
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(app.transcript[0].assistant_source(), Some(source));
        // A later completion must not overwrite the retained tail via a stale index.
        file_surface::sync_action_snapshot(
            &mut app,
            ActionSnapshot {
                id: "first".into(),
                name: "later edit".into(),
                status: "completed".into(),
                output: "later output".into(),
                result: "success".into(),
            },
        );
        assert_eq!(app.action_cells.get("first"), Some(&1));
        let later = app.styled_history_lines(width);
        assert_eq!(app.drain_committed_scrollback(width, 1), later);
        assert!(app.action_cells.is_empty());
        app.push_output(delta.into());
        let expected = app.styled_history_lines(width);
        assert_eq!(
            expected.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["-literal", "+next", "╰──"]
        );
        assert_eq!(expected[0].spans[0].style.fg, Some(Color::Red));
        assert_eq!(expected[1].spans[0].style.fg, Some(Color::Green));
        app.apply_tape_record(tests::tape_message("assistant", &full));
        assert_eq!(app.styled_history_lines(width), expected);
        // Restored viewport/history retains the source boundary, not committed tool cells.
        app.merge_reconnected_idle_history(vec![
            HistoryCell::User("task".into()),
            HistoryCell::Assistant(full.clone()),
        ]);
        assert_eq!(app.styled_history_lines(width), expected);
        app.transcript.insert(0, HistoryCell::User("task".into()));
        assert!(app.merge_reconnected_history(
            vec![
                HistoryCell::User("task".into()),
                HistoryCell::Assistant(full.clone())
            ],
            "task",
            0
        ));
        assert_eq!(app.transcript.len(), 2);
        assert_eq!(app.transcript[1].assistant_source(), Some(full.as_str()));
        assert_eq!(app.styled_history_lines(width)[1..], expected);
        let mut terminal = Terminal::new(TestBackend::new(width as u16, 12)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((0, 1)).unwrap().symbol(), "-");
        assert_eq!(buffer.cell((0, 1)).unwrap().fg, Color::Red);
        assert_eq!(buffer.cell((0, 2)).unwrap().symbol(), "+");
        assert_eq!(buffer.cell((0, 2)).unwrap().fg, Color::Green);
        assert!(app.action_cells.is_empty());

        // A bounded prefix drain shifts a still-retained action past the tail.
        let mut shifted = FileBackedApp::new("/agent/root".into());
        shifted.push_output(source.into());
        shifted.drain_committed_scrollback(width, 1);
        for id in ["first", "second"] {
            file_surface::sync_action_snapshot(
                &mut shifted,
                ActionSnapshot {
                    id: id.into(),
                    name: id.into(),
                    status: "completed".into(),
                    output: id.into(),
                    result: "success".into(),
                },
            );
        }
        let first_rows = crate::history::action_summary(&shifted.transcript[1], width).len();
        assert_eq!(
            shifted.prune_rendered_prefix(shifted.render_opts(width), first_rows),
            first_rows
        );
        assert!(!shifted.action_cells.contains_key("first"));
        assert_eq!(shifted.action_cells.get("second"), Some(&1));
        assert!(matches!(shifted.transcript[1], HistoryCell::Tool { .. }));
        assert_eq!(shifted.transcript[0].assistant_source(), Some(source));
        let rest = shifted.styled_history_lines(width);
        assert_eq!(shifted.drain_committed_scrollback(width, 1), rest);
        assert!(shifted.action_cells.is_empty());
        assert!(shifted.styled_history_lines(width).is_empty());
    }
}

#[test]
fn semantic_whole_streaming_cell_drain_keeps_open_fence_context() {
    for width in [40, 60, 80, 120] {
        let source = "```diff\n+committed";
        let delta = "\n-\t**literal**\n+new\n```\n**done**";
        let full = format!("{source}{delta}");
        let mut app = FileBackedApp::new("/agent/root".into());
        app.transcript.push(HistoryCell::User("task".into()));
        app.push_output(source.into());
        let drained = app.drain_committed_scrollback(width, 1);
        assert_eq!(
            drained.iter().map(ToString::to_string).collect::<Vec<_>>(),
            [": task", "", "╭─ diff", "+committed"]
        );
        assert!(app.styled_history_lines(width).is_empty());
        assert!(app.drain_committed_scrollback(width, 1).is_empty());
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(app.transcript[0].assistant_source(), Some(source));
        app.push_output(delta.into());
        let check = |app: &FileBackedApp| {
            let index = app
                .current_assistant_cell()
                .expect("assistant role survives full drain");
            assert_eq!(
                app.transcript[index].assistant_source(),
                Some(full.as_str())
            );
            let lines = app.transcript[index]
                .render_styled_lines(crate::history::RenderOpts::new(width, false));
            assert_eq!(
                lines.iter().map(ToString::to_string).collect::<Vec<_>>(),
                ["-    **literal**", "+new", "╰──", "done"]
            );
            assert_eq!(lines[0].spans[0].style.fg, Some(Color::Red));
            assert_eq!(lines[1].spans[0].style.fg, Some(Color::Green));
        };
        check(&app);
        app.apply_tape_record(tests::tape_message("assistant", &full));
        check(&app);
        app.merge_reconnected_idle_history(vec![
            HistoryCell::User("task".into()),
            HistoryCell::Assistant(full.clone()),
        ]);
        check(&app);
        // Restore the retained input boundary required by active reconnect's contract.
        app.transcript.insert(0, HistoryCell::User("task".into()));
        assert!(app.merge_reconnected_history(
            vec![
                HistoryCell::User("task".into()),
                HistoryCell::Assistant(full.clone())
            ],
            "task",
            0
        ));
        check(&app);
        assert_eq!(app.transcript.len(), 2);
        let mut terminal = Terminal::new(TestBackend::new(width as u16, 12)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((0, 1)).unwrap().symbol(), "-");
        assert_eq!(buffer.cell((0, 1)).unwrap().fg, Color::Red);
        assert_eq!(buffer.cell((5, 1)).unwrap().symbol(), "*");
        assert_eq!(buffer.cell((0, 2)).unwrap().fg, Color::Green);
        assert!(
            buffer
                .cell((0, 4))
                .unwrap()
                .modifier
                .contains(Modifier::BOLD)
        );
    }
}

#[test]
fn semantic_late_closures_repeated_drain_and_hydration_own_content() {
    use crate::history::RenderOpts;
    for width in [40, 60, 80, 120] {
        for fenced in [false, true] {
            let opening = if fenced { "```diff\n**" } else { "**" };
            let source = format!("{opening}{}", "a".repeat(238));
            let closing = if fenced {
                "**\n```\n: literal\n! literal\ntool> literal"
            } else {
                "**\n: literal\n! literal\ntool> literal"
            };
            let full = format!("{source}{closing}");
            let mut app = FileBackedApp::new("/agent/root".into());
            app.transcript.push(HistoryCell::User("task".into()));
            app.push_output(source);
            let index = app.current_assistant_cell().unwrap();
            app.transcript[index].trim_rendered_prefix(RenderOpts::new(width, false), 1);
            let mut committed =
                app.transcript[index].render_lines(RenderOpts::new(width, false))[0].clone();
            app.transcript[index].trim_rendered_prefix(RenderOpts::new(width, false), 1);
            let resized = if width == 40 { 60 } else { 40 };
            committed
                .push_str(&app.transcript[index].render_lines(RenderOpts::new(resized, false))[0]);
            app.transcript[index].trim_rendered_prefix(RenderOpts::new(resized, false), 1);
            let owned = committed.matches('a').count();
            app.push_output(closing.into());
            let expected = format!(
                "{}{}: literal! literaltool> literal",
                "a".repeat(238 - owned),
                if fenced { "**╰──" } else { "" }
            );
            let check = |app: &FileBackedApp| {
                assert_eq!(app.transcript.len(), 2);
                assert_eq!(app.transcript[1].assistant_source(), Some(full.as_str()));
                assert_eq!(
                    app.transcript[1]
                        .render_lines(RenderOpts::new(resized, false))
                        .concat(),
                    expected
                );
            };
            check(&app);
            app.merge_reconnected_idle_history(vec![
                HistoryCell::User("task".into()),
                HistoryCell::Assistant(full.clone()),
            ]);
            check(&app);
            app.apply_tape_record(tests::tape_message("assistant", &full));
            check(&app);
            assert!(app.merge_reconnected_history(
                vec![
                    HistoryCell::User("task".into()),
                    HistoryCell::Assistant(full.clone())
                ],
                "task",
                0
            ));
            check(&app);
            app.merge_reconnected_idle_history(vec![
                HistoryCell::User("task".into()),
                HistoryCell::Assistant(full.clone()),
            ]);
            check(&app);
            assert_eq!(
                owned
                    + app.transcript[1]
                        .render_lines(RenderOpts::new(resized, false))
                        .concat()
                        .matches('a')
                        .count(),
                238 + 3
            ); // three literal labels contain one a each
            let mut terminal = Terminal::new(TestBackend::new(resized as u16, 20)).unwrap();
            terminal.draw(|frame| draw(frame, &app)).unwrap();
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer.cell((0, 1)).unwrap().symbol(), "a");
            assert_eq!(
                buffer
                    .cell((0, 1))
                    .unwrap()
                    .modifier
                    .contains(Modifier::BOLD),
                !fenced
            );
        }
    }
}

#[test]
fn semantic_idle_hydration_does_not_replay_drained_heading() {
    for width in [40, 60, 80, 120] {
        let source = "# head\n**one**\n**two**";
        let mut app = FileBackedApp::new("/agent/root".into());
        app.transcript.push(HistoryCell::Assistant(source.into()));
        let drained = app.drain_committed_scrollback(width, 5);
        assert_eq!(
            drained.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["", "head"]
        );
        let before = app.styled_history_lines(width);
        app.merge_reconnected_idle_history(vec![HistoryCell::Assistant(source.into())]);
        assert_eq!(
            app.styled_history_lines(width),
            before,
            "identical source hydration replayed at {width}"
        );
        assert_eq!(app.transcript.len(), 1);
        let mut terminal = Terminal::new(TestBackend::new(width as u16, 12)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), "o");
        assert!(
            buffer
                .cell((0, 0))
                .unwrap()
                .modifier
                .contains(Modifier::BOLD)
        );
    }
}

#[test]
fn semantic_tabs_survive_production_code_and_typed_diff_projection() {
    use crate::history::ToolStatus;
    use alan_agent_protocol::{DiffHunk, DiffLine, ToolResultPresentation};
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.transcript.push(HistoryCell::Assistant(
            "```rust\n\tlet 界 = 1;\u{1b}\u{7}\n```\n: prose\n! prose\ntool> prose".into(),
        ));
        app.transcript.push(HistoryCell::Tool {
            title: "Edit".into(),
            status: ToolStatus::Complete,
            preview: None,
            presentation: Some(ToolResultPresentation::Diff {
                path: "a.rs".into(),
                hunks: vec![DiffHunk {
                    header: None,
                    lines: vec![
                        DiffLine::Added {
                            text: "\tnew".into(),
                        },
                        DiffLine::Removed {
                            text: "\told".into(),
                        },
                    ],
                }],
            }),
        });
        let mut terminal = Terminal::new(TestBackend::new(width, 16)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer.cell((4, 2)).unwrap().symbol(),
            "l",
            "code tab indentation at {width}"
        );
        assert_eq!(buffer.cell((12, 9)).unwrap().symbol(), "n");
        assert_eq!(buffer.cell((12, 9)).unwrap().fg, Color::Green);
        assert_eq!(buffer.cell((12, 10)).unwrap().symbol(), "o");
        assert_eq!(buffer.cell((12, 10)).unwrap().fg, Color::Red);
        for row in 4..=6 {
            assert_eq!(buffer.cell((0, row)).unwrap().fg, Color::Reset);
        }
        assert!(
            !app.styled_history_lines(width as usize)
                .iter()
                .any(|line| line.to_string().contains(['\u{1b}', '\u{7}', '\t']))
        );
    }
}

#[test]
fn semantic_second_drain_stream_tape_and_both_merges_keep_fence_context() {
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new("/agent/root".into());
        let source = "# head\n```diff\n+\tone\n-\ttwo\n+\tthree";
        app.transcript.push(HistoryCell::User("task".into()));
        app.push_output(source.into());
        let first = app.drain_committed_scrollback(width, 6);
        assert!(!first.is_empty());
        let resized = if width == 40 { 120 } else { 40 };
        let before = app.styled_history_lines(resized);
        let second = app.drain_committed_scrollback(resized, 5);
        assert_eq!(
            [second.clone(), app.styled_history_lines(resized)].concat(),
            before
        );
        assert!(!second.is_empty());
        let append = "\n+\tfour\n```\n**done**\n: literal\n! literal\ntool> literal";
        app.push_output("\n+\tfour".into());
        let full = format!("{source}{append}");
        let mut projected = app.transcript[0].clone();
        projected.replace_assistant_source(full.clone());
        let expected =
            projected.render_styled_lines(crate::history::RenderOpts::new(resized, false));
        app.merge_reconnected_idle_history(vec![
            HistoryCell::User("task".into()),
            HistoryCell::Assistant(full.clone()),
        ]);
        assert_eq!(
            app.styled_history_lines(resized),
            expected,
            "extended full-source hydration must keep cuts"
        );
        app.apply_tape_record(tests::tape_message("assistant", &full));
        app.merge_reconnected_idle_history(vec![
            HistoryCell::User("task".into()),
            HistoryCell::Assistant(full.clone()),
        ]);
        assert_eq!(app.styled_history_lines(resized), expected);
        // Retained user boundary is needed by the active reconnect contract.
        app.transcript.insert(0, HistoryCell::User("task".into()));
        assert!(app.merge_reconnected_history(
            vec![
                HistoryCell::User("task".into()),
                HistoryCell::Assistant(full)
            ],
            "task",
            0
        ));
        app.transcript.remove(0);
        assert_eq!(app.styled_history_lines(resized), expected);
        let mut terminal = Terminal::new(TestBackend::new(resized as u16, 16)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer.cell((0, 0)).unwrap().symbol(),
            "-",
            "retained: {:?}; first: {:?}; second: {:?}",
            expected,
            first,
            second
        );
        assert_eq!(buffer.cell((0, 0)).unwrap().fg, Color::Red);
        assert_eq!(buffer.cell((5, 2)).unwrap().symbol(), "f");
        assert_eq!(buffer.cell((5, 2)).unwrap().fg, Color::Green);
        assert!(
            buffer
                .cell((0, 4))
                .unwrap()
                .modifier
                .contains(Modifier::BOLD)
        );
        for row in 5..=7 {
            assert_eq!(buffer.cell((0, row)).unwrap().fg, Color::Reset);
        }
    }
}
