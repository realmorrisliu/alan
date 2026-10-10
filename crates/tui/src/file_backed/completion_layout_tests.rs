use super::*;
use ratatui::{Terminal, backend::TestBackend};

pub(super) fn render(app: &FileBackedApp, width: u16, pane: u16) -> (Vec<String>, (u16, u16)) {
    render_at(app, width, pane, 0)
}

pub(super) fn render_at(
    app: &FileBackedApp,
    width: u16,
    pane: u16,
    top: u16,
) -> (Vec<String>, (u16, u16)) {
    let height = inline_viewport_height(app, width as usize, (pane - top) as usize);
    // The inline owner retains its top when height changes; do not bottom-anchor
    // a fresh viewport for each candidate count.
    let area = ratatui::layout::Rect::new(0, top, width, height);
    let mut terminal = Terminal::with_options(
        TestBackend::new(width, pane),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        },
    )
    .unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let cursor = terminal.backend().cursor_position();
    let rows = (area.y..pane)
        .map(|y| {
            (0..width)
                .map(|x| terminal.backend().buffer().cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect();
    (rows, (cursor.x, cursor.y - area.y))
}

#[test]
fn project_per_key_anchor_and_menu_order() {
    for width in [40, 48, 60, 73, 80, 120] {
        for pane in [3, 5, 10, 24] {
            let mut app = FileBackedApp::new("/agent/root".into());
            let mut anchor = None;
            for ch in "/project".chars() {
                app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
                let (rows, (x, y)) = render(&app, width, pane);
                let prompt = rows.iter().position(|r| r.starts_with(": ")).unwrap();
                assert_eq!(y as usize, prompt);
                assert_eq!(x as usize, 2 + app.composer.text().len());
                assert_eq!(
                    *anchor.get_or_insert(y),
                    y,
                    "candidate filtering moved input: {rows:?}"
                );
                assert!(
                    rows[..prompt].iter().all(|r| !r.contains("/project")),
                    "menu above input: {rows:?}"
                );
                assert!(
                    rows.iter().skip(prompt + 1).any(|r| r.contains("/project")),
                    "menu missing: {rows:?}"
                );
                let (edge_rows, edge_cursor) = render_at(&app, width, pane + 7, 7);
                assert_eq!(edge_cursor, (x, y));
                assert_eq!(edge_rows, rows);
                assert!(
                    app.drain_committed_scrollback(width as usize, pane as usize)
                        .is_empty()
                );
            }
            for code in [
                KeyCode::Backspace,
                KeyCode::Down,
                KeyCode::Up,
                KeyCode::Tab,
                KeyCode::Esc,
            ] {
                app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
                let (_, (_, y)) = render(&app, width, pane);
                assert_eq!(Some(y), anchor);
                assert!(app.transcript.is_empty());
            }
        }
    }
}
