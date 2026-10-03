use super::*;
use ratatui::{Terminal, backend::TestBackend, style::Color};

fn screen(app: &FileBackedApp, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}
fn text(buffer: &ratatui::buffer::Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn production_detail_buffers_preserve_diff_command_file_and_pages() {
    let result = serde_json::json!({"title":"original diff", "presentation":{"form":"diff","path":"src/file","hunks":[{"header":"@@ -1,2 +1,2 @@","lines":[{"kind":"added","text":"  \tadded literal"},{"kind":"removed","text":" \tremoved literal"},{"kind":"context","text":"  \tcontext literal"},{"kind":"added","text":"LONG_LITERAL_".repeat(50)}]}]}}).to_string();
    let (shell, path, id) = action_fixture("original diff output", &result).await;
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new(path.clone());
        app.composer.set_text("界 draft 🦀");
        let inline = screen(&app, width, 24);
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        app.modal.rows = action_detail_io::read_detail(&shell, &path, &id).await;
        assert_eq!(inline_viewport_height(&app, width as usize, 24), 24);
        let buffer = screen(&app, width, 24);
        let rendered = text(&buffer);
        assert!(
            rendered.contains("@@ -1,2 +1,2 @@")
                && rendered.contains("+      added literal")
                && rendered.contains("-     removed literal")
                && rendered.contains("       context literal"),
            "{rendered}"
        );
        for (needle, color) in [
            ("added literal", Color::Green),
            ("removed literal", Color::Red),
            ("context literal", Color::Reset),
        ] {
            let y = (0..24)
                .find(|y| {
                    (0..width)
                        .map(|x| buffer.cell((x, *y)).unwrap().symbol())
                        .collect::<String>()
                        .contains(needle)
                })
                .unwrap();
            let x = (0..width)
                .find(|x| buffer.cell((*x, y)).unwrap().symbol() == &needle[..1])
                .unwrap();
            assert_eq!(buffer.cell((x, y)).unwrap().fg, color);
        }
        assert!(
            rendered.matches("LONG_LITERAL_").count() >= 10,
            "{rendered}"
        );
        let before = app.transcript.clone();
        assert!(app.drain_committed_scrollback(width as usize, 1).is_empty());
        assert_eq!(app.transcript, before);
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_ne!(screen(&app, width, 24), buffer);
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(screen(&app, width, 24), buffer);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(screen(&app, width, 24), inline);
        assert!(app.transcript.is_empty());
    }
    for width in [40, 60, 80, 120] {
        for (presentation, output, needles) in [
            (
                serde_json::json!({"form":"command","cmdline":"test command","exit_code":101,"stdout":"  \tstdout literal\nsecond stdout","stderr":" \tstderr literal","truncated":false}),
                "original command",
                vec![
                    "stdout",
                    "stderr",
                    "      stdout literal",
                    "     stderr literal",
                    "exit 101",
                ],
            ),
            (
                serde_json::json!({"form":"file_content","path":"src/literal","lines":2,"truncated":false}),
                "  \t# literal *file*\n   second line",
                vec!["src/literal", "      # literal *file*", "   second line"],
            ),
        ] {
            shell
                .write(
                    &format!("{path}/actions/{id}/result"),
                    serde_json::json!({"presentation":presentation})
                        .to_string()
                        .as_bytes(),
                )
                .await
                .unwrap();
            // output is append-only: use a fresh fixture for exact literal evidence.
            let (fresh, fresh_path, fresh_id) = action_fixture(
                output,
                &serde_json::json!({"presentation":presentation}).to_string(),
            )
            .await;
            let mut app = FileBackedApp::new(fresh_path.clone());
            app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
            app.modal.rows = action_detail_io::read_detail(&fresh, &fresh_path, &fresh_id).await;
            let rendered = text(&screen(&app, width, 32));
            for needle in needles {
                assert!(rendered.contains(needle), "missing {needle}: {rendered}");
            }
        }
    }
}

#[test]
fn modal_keys_and_paste_preserve_composer_intent_completion_and_inline() {
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new("/agent/1".into());
        app.handle_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
        app.composer.set_text("界 draft 🦀");
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        app.completion = Some(crate::completion::CompletionState {
            kind: crate::completion::CompletionKind::Command,
            token_start: 0,
            query: "界".into(),
            matches: vec![],
            selected: 0,
        });
        let saved = (
            app.composer.text().to_string(),
            app.composer.cursor(),
            format!("{:?}", app.input_intent),
            format!("{:?}", app.completion),
        );
        let inline = screen(&app, width, 24);
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        for code in [
            KeyCode::Char('x'),
            KeyCode::Backspace,
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Home,
            KeyCode::PageDown,
            KeyCode::PageUp,
        ] {
            assert!(
                app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
                    .is_none()
            );
        }
        app.dispatch(FileBackedEvent::Terminal(crossterm::event::Event::Paste(
            "界 MUTATION 🦀".into(),
        )));
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            (
                app.composer.text().to_string(),
                app.composer.cursor(),
                format!("{:?}", app.input_intent),
                format!("{:?}", app.completion)
            ),
            saved
        );
        assert_eq!(screen(&app, width, 24), inline);
    }
    let mut app = FileBackedApp::new("/agent/1".into());
    app.composer.set_text("/help");
    app.handle_submit();
    let help = app.notice.unwrap();
    for key in ["Ctrl+O", "arrows", "PgUp/PgDn", "Esc"] {
        assert!(help.contains(key));
    }
}
