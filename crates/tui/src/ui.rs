use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::app::TuiApp;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(frame: &mut Frame<'_>, app: &TuiApp) {
    let area = frame.area();
    let width = area.width as usize;
    let live_height = app.live_region_height(width).max(2);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(live_height)])
        .split(area);

    draw_transcript(frame, app, chunks[0]);
    draw_live_region(frame, app, chunks[1]);
}

fn draw_transcript(frame: &mut Frame<'_>, app: &TuiApp, area: ratatui::layout::Rect) {
    let mut transcript: Vec<Line<'_>> = app
        .rendered_history_lines(area.width as usize)
        .into_iter()
        .map(style_transcript_line)
        .collect();
    if transcript.is_empty() {
        transcript.push(Line::from(vec![
            Span::styled("alan", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(" ready", Style::default().fg(Color::DarkGray)),
        ]));
    }

    frame.render_widget(
        Paragraph::new(transcript)
            .wrap(Wrap { trim: false })
            .block(Block::default()),
        area,
    );
}

fn style_transcript_line(line: String) -> Line<'static> {
    let style = if line.starts_with("you>") {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if line.starts_with("thinking") {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC)
    } else if line.starts_with("tool>") || line.starts_with("plan>") {
        Style::default().fg(Color::Blue)
    } else if line.starts_with("error>") {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    Line::styled(line, style)
}

fn draw_live_region(frame: &mut Frame<'_>, app: &TuiApp, area: ratatui::layout::Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    if let Some(label) = app.reducer.activity_label() {
        lines.push(activity_line(app, label));
    }

    if let Some(notice) = &app.reducer.transient_notice {
        lines.push(Line::styled(
            format!("· {notice}"),
            Style::default().fg(Color::Yellow),
        ));
    }

    if let Some(form) = &app.form {
        for (text, focused) in form.render_lines() {
            let style = if focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if text.trim_start().starts_with('!') {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            lines.push(Line::styled(text, style));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }

    if let Some(state) = &app.completion {
        for (idx, candidate) in state
            .matches
            .iter()
            .take(crate::app::MAX_COMPLETION_ROWS)
            .enumerate()
        {
            let trigger = match state.kind {
                crate::completion::CompletionKind::Command => "/",
                crate::completion::CompletionKind::Skill => "$",
                crate::completion::CompletionKind::File => "@",
            };
            let mut label = format!("{trigger}{}", candidate.label);
            if let Some(detail) = &candidate.detail {
                label.push_str(&format!("  — {detail}"));
            }
            let style = if idx == state.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            lines.push(Line::styled(format!("  {label}"), style));
        }
    }

    let composer_prompt = if app.reducer.pending_yield.is_some() {
        "» "
    } else {
        "> "
    };
    lines.push(Line::from(vec![
        Span::styled(composer_prompt, Style::default().fg(Color::Green)),
        Span::raw(app.composer.text().to_string()),
    ]));

    lines.push(hint_line(app));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn activity_line(app: &TuiApp, label: &str) -> Line<'static> {
    let elapsed = app
        .reducer
        .turn_started
        .map(|started| started.elapsed().as_secs())
        .unwrap_or(0);
    let frame_idx = app
        .reducer
        .turn_started
        .map(|started| (started.elapsed().as_millis() / 80) as usize % SPINNER.len())
        .unwrap_or(0);
    let model = app
        .session
        .resolved_model
        .clone()
        .unwrap_or_else(|| "model".to_string());
    Line::from(vec![
        Span::styled(
            format!("{} ", SPINNER[frame_idx]),
            Style::default().fg(Color::Magenta),
        ),
        Span::styled(
            label.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · esc to interrupt · {elapsed}s"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(format!("   {model}"), Style::default().fg(Color::DarkGray)),
    ])
}

fn hint_line(app: &TuiApp) -> Line<'static> {
    let hint = match &app.reducer.pending_yield {
        Some(pending)
            if matches!(pending.kind, alan_protocol::YieldKind::Confirmation)
                && !pending.options.is_empty() =>
        {
            let choices = pending
                .options
                .iter()
                .enumerate()
                .map(|(idx, option)| format!("{}={option}", idx + 1))
                .collect::<Vec<_>>()
                .join(" · ");
            format!("{choices}  · or type a reply and press Enter")
        }
        Some(_) => "reply and press Enter".to_string(),
        None => "⏎ send · ⇧⏎ newline · / commands · ↑ history · ctrl+q quit".to_string(),
    };
    Line::styled(hint, Style::default().fg(Color::DarkGray))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_client::CreateSession;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(app: &TuiApp) -> String {
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    fn app() -> TuiApp {
        TuiApp::new(CreateSession {
            session_id: "s-1".into(),
            profile_id: None,
            provider: None,
            resolved_model: Some("claude-opus".into()),
            durability: None,
        })
    }

    #[test]
    fn renders_ready_state_and_hint() {
        let buffer = render(&app());
        assert!(buffer.contains("alan ready"));
        assert!(buffer.contains("commands"));
    }

    #[test]
    fn renders_activity_line_during_turn() {
        let mut app = app();
        app.reducer
            .apply_envelope(envelope(alan_protocol::Event::TurnStarted {}));
        let buffer = render(&app);
        assert!(buffer.contains("esc to interrupt"));
        assert!(buffer.contains("claude-opus"));
    }

    fn envelope(event: alan_protocol::Event) -> alan_protocol::EventEnvelope {
        alan_protocol::EventEnvelope {
            event_id: "e-1".into(),
            sequence: 1,
            session_id: "s-1".into(),
            submission_id: None,
            turn_id: "t-1".into(),
            item_id: "i-1".into(),
            timestamp_ms: 1,
            event,
        }
    }
}
