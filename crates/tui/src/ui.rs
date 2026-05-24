use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::TuiApp;

pub fn draw(frame: &mut Frame<'_>, app: &TuiApp) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),
            Constraint::Length(if app.reducer.pending_yield.is_some() {
                5
            } else {
                3
            }),
        ])
        .split(area);

    let mut transcript = Vec::new();
    for cell in app.history_cells() {
        transcript.extend(
            cell.render_lines(chunks[0].width as usize)
                .into_iter()
                .map(Line::from),
        );
    }
    if transcript.is_empty() {
        transcript.push(Line::from(vec![
            Span::styled("alan", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" ready"),
        ]));
    }

    frame.render_widget(
        Paragraph::new(transcript)
            .wrap(Wrap { trim: false })
            .block(Block::default()),
        chunks[0],
    );

    let composer_title = if let Some(pending) = &app.reducer.pending_yield {
        format!("{} - reply and press Enter", pending.title)
    } else {
        "message - Enter to send, Shift+Enter for newline, Ctrl+Q to quit".to_string()
    };
    frame.render_widget(
        Paragraph::new(app.composer.text().to_string())
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::TOP).title(composer_title)),
        chunks[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_client::CreateSession;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_composer_and_initial_ready_state() {
        let app = TuiApp::new(CreateSession {
            session_id: "s-1".into(),
            profile_id: None,
            provider: None,
            resolved_model: None,
            durability: None,
        });
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let buffer = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(buffer.contains("alan ready"));
        assert!(buffer.contains("message"));
    }
}
