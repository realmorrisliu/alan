use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

pub(crate) const INLINE_PROMPT_PREFIX: &str = "alan > ";
pub(crate) const INLINE_WAITING_PROMPT_PREFIX: &str = "alan » ";
pub(crate) const INLINE_PROMPT_CONTINUATION: &str = "       ";

pub(crate) fn style_transcript_line(line: String) -> Line<'static> {
    let style = if line.starts_with(INLINE_PROMPT_PREFIX.trim_end()) {
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
