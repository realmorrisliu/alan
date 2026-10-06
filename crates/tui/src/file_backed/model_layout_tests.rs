use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[test]
fn model_picker_real_inline_preserves_absolute_anchor_and_history() {
    use ratatui::{
        Terminal, TerminalOptions, Viewport,
        backend::{Backend, TestBackend},
    };
    for width in [40, 60, 73, 80, 120] {
        for pane in [3, 5, 10, 22] {
            for bottom in [false, true] {
                let mut app = super::model_tests::ready();
                app.composer.set_text("界🙂 retained");
                app.notice = None;
                let mut backend = TestBackend::new(width, pane);
                backend
                    .set_cursor_position((0, if bottom { pane - 2 } else { 0 }))
                    .unwrap();
                let mut terminal = Terminal::with_options(
                    backend,
                    TerminalOptions {
                        viewport: Viewport::Inline(2),
                    },
                )
                .unwrap();
                let anchor = super::completion_anchor_tests::frame(&mut terminal, &app);
                let history = app.styled_history_lines(width as usize);
                app.handle_command("/model");
                for code in [KeyCode::Down, KeyCode::Up, KeyCode::Esc] {
                    assert_eq!(
                        super::completion_anchor_tests::frame(&mut terminal, &app),
                        anchor,
                        "{width}x{pane} bottom={bottom}"
                    );
                    assert!(
                        app.drain_committed_scrollback(width as usize, pane as usize)
                            .is_empty()
                    );
                    assert_eq!(app.styled_history_lines(width as usize), history);
                    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
                }
                assert_eq!(
                    super::completion_anchor_tests::frame(&mut terminal, &app),
                    anchor
                );
                assert_eq!(app.composer.text(), "界🙂 retained");
            }
        }
    }
}
#[test]
fn model_picker_blocks_busy_and_unknown_catalog_and_retains_failed_receipt_truth() {
    let mut app = super::model_tests::ready();
    app.activity = alan_agent_protocol::UiActivitySnapshot::running(0);
    app.handle_command("/model");
    assert!(app.model_chooser.active);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app.activity = alan_agent_protocol::UiActivitySnapshot::idle();
    let owner = app.model.owner.clone();
    app.model.apply(&owner, None);
    app.handle_command("/model");
    assert!(!app.model_chooser.active);
    app = super::model_tests::ready();
    app.handle_command("/model");
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let action = app
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    let FileBackedAction::SelectModel { owner, id, .. } = action else {
        panic!();
    };
    let event = alan_agent_protocol::UiEvent::InputCompleted {
        submission_ids: vec![id],
        status: alan_agent_protocol::UiInputStatus::Failed,
        error: Some("secret not displayed".into()),
    };
    assert!(app.observe_model_receipt(&owner, &event));
    assert_eq!(
        app.model
            .known()
            .unwrap()
            .selected_next
            .as_ref()
            .unwrap()
            .model,
        "B"
    );
    assert!(app.notice.as_ref().unwrap().contains("rejected"));
    assert!(!app.notice.as_ref().unwrap().contains("secret"));
}
