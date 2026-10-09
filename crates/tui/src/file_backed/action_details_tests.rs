#[path = "action_detail_polish_tests.rs"]
mod detail_polish_tests;
#[path = "action_polish_tests.rs"]
mod polish_tests;
use super::*;
use std::time::Duration;
#[path = "action_acceptance_tests.rs"]
mod acceptance_tests;
#[path = "action_async_tests.rs"]
mod async_tests;
#[path = "action_buffer_tests.rs"]
mod buffer_tests;
#[path = "action_evidence_tests.rs"]
mod evidence_tests;
#[path = "action_identity_tests.rs"]
mod identity_tests;
#[path = "action_review_tests.rs"]
mod review_tests;
use crate::file_backed::stdio_tests::live_root_agent;
use alan_ap::{Fid, FileServer, OpenMode};

async fn action_fixture(output: &str, result: &str) -> (alan_shell::Shell, String, String) {
    let (shell, root, _, pid) = live_root_agent().await;
    let fid = Fid(900);
    root.walk(
        Fid::ROOT,
        fid,
        &[pid.clone(), "actions".into(), "clone".into()],
    )
    .await
    .unwrap();
    root.open(fid, OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(root.read(fid, 0, 64).await.unwrap()).unwrap();
    root.clunk(fid).await.unwrap();
    let path = format!("/agent/{pid}");
    for (field, value) in [
        ("name", "legacy tool"),
        ("output", output),
        ("result", result),
        ("status", "completed"),
    ] {
        shell
            .write(&format!("{path}/actions/{id}/{field}"), value.as_bytes())
            .await
            .unwrap();
    }
    (shell, path, id)
}

#[test]
fn details_plain_pager_aliases_preserve_draft_and_page_saturating() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.composer.set_text("界 draft");
    for code in [KeyCode::Char(' '), KeyCode::Char('b')] {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    assert_eq!(app.composer.text(), "界 draft b");
    let saved = (
        app.composer.text().to_string(),
        app.composer.cursor(),
        app.input_intent,
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(
        app.modal.scroll, 10,
        "plain Space must page details forward"
    );
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
    assert_eq!(app.modal.scroll, 10, "plain b must page details backward");
    for modifiers in [
        KeyModifiers::SHIFT,
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
    ] {
        for code in [KeyCode::Char(' '), KeyCode::Char('b')] {
            app.handle_key(KeyEvent::new(code, modifiers));
            assert_eq!(app.modal.scroll, 10, "modified alias must not page");
        }
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
    assert_eq!(app.modal.scroll, 0);
    app.modal.scroll = usize::MAX - 5;
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.modal.scroll, usize::MAX);
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    assert_eq!(app.modal.scroll, 0);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.modal.active);
    assert_eq!(
        (
            app.composer.text().to_string(),
            app.composer.cursor(),
            app.input_intent
        ),
        saved
    );
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
    assert_eq!(app.composer.text(), "界 draft b b");
}

#[tokio::test]
async fn actual_inline_near_bottom_retained_modal_uses_full_viewport() {
    use ratatui::{
        Terminal, TerminalOptions, Viewport,
        backend::{Backend, TestBackend},
    };
    let output = (0..60)
        .map(|i| format!("retained row {i}\n"))
        .collect::<String>();
    let (shell, path, id) = action_fixture(&output, "retained result").await;
    for width in [40, 60, 73, 80, 120] {
        let mut app = FileBackedApp::new(path.clone());
        file_surface::sync_action_from_file(&shell, &path, &id, &mut app)
            .await
            .unwrap();
        app.drain_committed_scrollback(width, 1);
        app.dispatch(FileBackedEvent::Terminal(crossterm::event::Event::Paste(
            ": 界🙂 draft".into(),
        )));
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        let saved = (
            app.composer.text().to_string(),
            app.composer.cursor(),
            app.input_intent,
        );
        let mut backend = TestBackend::new(width as u16, 22);
        backend.set_cursor_position((0, 20)).unwrap();
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(2),
            },
        )
        .unwrap();
        let anchor = super::completion_anchor_tests::frame(&mut terminal, &app);
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        action_detail_io::start_pending(&shell, &mut app, &tx);
        app.dispatch(rx.recv().await.unwrap());
        app.dispatch(
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
        super::completion_anchor_tests::frame(&mut terminal, &app);
        assert_eq!(
            terminal.get_frame().area().height,
            22,
            "full retained modal clipped"
        );
        assert_eq!(terminal.get_frame().area().y, 0);
        let before = terminal.backend().buffer().clone();
        let hint = (0..width as u16)
            .map(|x| before.cell((x, 0)).unwrap().symbol())
            .collect::<String>();
        for label in ["↔ Action", "Space/b page", "Esc"] {
            assert!(hint.contains(label), "hint clipped at {width}: {hint}");
        }
        for (forward, backward) in [
            (KeyCode::PageDown, KeyCode::PageUp),
            (KeyCode::Char(' '), KeyCode::Char('b')),
        ] {
            app.handle_key(KeyEvent::new(forward, KeyModifiers::NONE));
            super::completion_anchor_tests::frame(&mut terminal, &app);
            assert_eq!(app.modal.scroll, 10);
            assert_ne!(terminal.backend().buffer(), &before);
            app.handle_key(KeyEvent::new(backward, KeyModifiers::NONE));
            super::completion_anchor_tests::frame(&mut terminal, &app);
            assert_eq!(terminal.backend().buffer(), &before);
        }
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        super::completion_anchor_tests::frame(&mut terminal, &app);
        assert!(!app.modal.active);
        assert_eq!(
            (
                app.composer.text().to_string(),
                app.composer.cursor(),
                app.input_intent
            ),
            saved
        );
        // Full-screen details can relocate Inline, but the restored draft's caret is exact.
        assert!(terminal.backend().cursor_position().y <= anchor);
    }
}

#[tokio::test]
async fn action_file_metadata_and_huge_output_use_compact_rows_with_shared_hint() {
    let result =
        serde_json::json!({"title": "Read source", "result_preview": "preview", "presentation": {
            "form": "file_content", "path": "src/main.rs", "lines": 123
        }})
        .to_string();
    let (shell, path, id) = action_fixture(&"escaped JSON payload ".repeat(5000), &result).await;
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new(path.clone());
        file_surface::sync_action_from_file(&shell, &path, &id, &mut app)
            .await
            .unwrap();
        let rows = app.styled_history_lines(width);
        assert!(
            rows.len() <= 2,
            "summary has {} rows at {width}",
            rows.len()
        );
        let text = rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("Read source") && text.contains("123") && !text.contains("Ctrl+O"),
            "{text}"
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width as u16, 12)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let header = (0..width as u16)
            .map(|column| {
                terminal
                    .backend()
                    .buffer()
                    .cell((column, 2))
                    .unwrap()
                    .symbol()
            })
            .collect::<String>();
        assert!(header.starts_with("no project · model unknown"), "{header}");
        let hint = (0..width as u16)
            .map(|column| {
                terminal
                    .backend()
                    .buffer()
                    .cell((column, 4))
                    .unwrap()
                    .symbol()
            })
            .collect::<String>();
        assert!(
            hint.contains("Ctrl+O details"),
            "shared hint missing: {hint}"
        );
    }
}

#[tokio::test]
async fn details_survive_partial_and_full_summary_drain() {
    for width in [40, 60, 80, 120] {
        let output = "first output\nfull output sentinel\nlast output";
        let result = "malformed result sentinel";
        let (shell, path, id) = action_fixture(output, result).await;
        let mut app = FileBackedApp::new(path.clone());
        let snapshot = ActionSnapshot {
            id,
            name: "legacy tool".into(),
            status: "completed".into(),
            output: output.into(),
            result: result.into(),
        };
        sync_action_snapshot(&mut app, snapshot.clone());
        let rows = app.styled_history_lines(width);
        assert_eq!(app.prune_rendered_prefix(app.render_opts(width), 1), 1);
        sync_action_snapshot(&mut app, snapshot.clone());
        assert_eq!(app.styled_history_lines(width), rows[1..]);
        assert_eq!(app.drain_committed_scrollback(width, 1), rows[1..]);
        sync_action_snapshot(&mut app, snapshot);
        assert!(app.styled_history_lines(width).is_empty());
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        assert!(app.modal.active);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        action_detail_io::start_pending(&shell, &mut app, &tx);
        app.dispatch(rx.recv().await.unwrap());
        app.dispatch(
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width as u16, 15)).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let screen = (0..15)
            .map(|y| {
                (0..width as u16)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            screen.contains("full output sentinel") && screen.contains("malformed result sentinel"),
            "{screen}"
        );
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(app.modal.scroll, 10);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.modal.active);
        assert!(app.styled_history_lines(width).is_empty());
    }
}

#[test]
fn failed_command_is_bounded_and_distinct_actions_are_preserved() {
    let mut app = FileBackedApp::new("/agent/1".into());
    for id in ["one", "two"] {
        let result = serde_json::json!({"title":"Build", "presentation":{"form":"command", "cmdline":"cargo test", "exit_code":101, "stdout":"large stdout".repeat(5000), "stderr":"failure diagnostic".repeat(5000)}}).to_string();
        sync_action_snapshot(
            &mut app,
            ActionSnapshot {
                id: id.into(),
                name: "bash".into(),
                status: "failed".into(),
                output: "raw output".repeat(5000),
                result,
            },
        );
    }
    for width in [40, 60, 80, 120] {
        let rows = app.styled_history_lines(width);
        assert_eq!(rows.len(), 4);
        let text = rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("exit 101") && text.contains("failed"),
            "{text}"
        );
    }
}

#[test]
fn typed_diff_and_listing_metadata_and_agent_scoping_survive_drain() {
    let mut app = FileBackedApp::new("/agent/1".into());
    let presentation = ToolResultPresentation::Diff {
        path: "src/main.rs".into(),
        hunks: vec![],
    };
    let result = serde_json::json!({"title":"Edit", "presentation":presentation}).to_string();
    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a".into(),
            name: "edit".into(),
            status: "completed".into(),
            output: String::new(),
            result,
        },
    );
    let listing = serde_json::json!({"title":"List", "presentation":{"form":"listing", "rows":["one","two"]}}).to_string();
    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "b".into(),
            name: "list".into(),
            status: "completed".into(),
            output: String::new(),
            result: listing,
        },
    );
    let text = app
        .styled_history_lines(80)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("src/main.rs") && text.contains("+0 -0") && text.contains("2 rows"),
        "{text}"
    );
    app.drain_committed_scrollback(80, 1);
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.dispatch(FileBackedEvent::ActionDetails {
        path: app.agent_path.clone(),
        generation: app.modal.generation,
        ids: Ok(vec!["a".into(), "b".into()]),
        id: Some("b".into()),
        rows: vec![],
    });
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.modal.ids[app.modal.selected], "a");
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.modal.ids[app.modal.selected], "b");
    app.reset_for_root_process_change();
    app.agent_path = "/agent/2".into();
    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a".into(),
            name: "different process".into(),
            status: "completed".into(),
            output: String::new(),
            result: String::new(),
        },
    );
    assert_eq!(app.styled_history_lines(80).len(), 1);
}

#[test]
fn modal_paste_preserves_unicode_draft_and_cursor() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.composer.set_text("界 draft 🦀");
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    let cursor = app.composer.cursor();
    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a".into(),
            name: "read".into(),
            status: "completed".into(),
            output: String::new(),
            result: String::new(),
        },
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.dispatch(FileBackedEvent::Terminal(crossterm::event::Event::Paste(
        "MUTATION".into(),
    )));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.composer.text(), "界 draft 🦀");
    assert_eq!(app.composer.cursor(), cursor);
}

#[test]
fn identical_completed_snapshot_after_physical_drain_does_not_replay() {
    for width in [40, 60, 80, 120] {
        let mut app = FileBackedApp::new("/agent/1".into());
        let snapshot = ActionSnapshot {
            id: "a0".into(),
            name: "edit".into(),
            status: "completed".into(),
            output: "updated file".into(),
            result: String::new(),
        };
        sync_action_snapshot(&mut app, snapshot.clone());
        let before = app.styled_history_lines(width);
        assert_eq!(app.drain_committed_scrollback(width, 1), before);
        assert!(app.action_cells.is_empty());
        sync_action_snapshot(&mut app, snapshot);
        assert!(
            app.styled_history_lines(width).is_empty(),
            "completed action replayed after drain at {width}"
        );
    }
}
