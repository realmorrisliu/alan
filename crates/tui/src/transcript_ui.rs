use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

pub(crate) const INLINE_PROMPT_PREFIX: &str = "alan: ";
pub(crate) const INLINE_COMMAND_PROMPT_PREFIX: &str = "alan! ";
pub(crate) const INLINE_WAITING_PROMPT_PREFIX: &str = "alan » ";

pub(crate) fn wrapped_line_count(lines: &[Line<'_>], width: usize) -> usize {
    if lines.is_empty() {
        return 0;
    }
    let width = width.max(1).min(u16::MAX as usize) as u16;
    Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .line_count(width)
}

pub(crate) fn style_transcript_line(line: String) -> Line<'static> {
    let style = if line.starts_with(INLINE_PROMPT_PREFIX.trim_end())
        || line.starts_with(INLINE_COMMAND_PROMPT_PREFIX.trim_end())
    {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if line.starts_with("thinking") {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC)
    } else if line.starts_with("tool>") || line.starts_with("plan>") {
        Style::default().fg(Color::Blue)
    } else if line.starts_with("stderr>") {
        Style::default().fg(Color::DarkGray)
    } else if line.starts_with("error>") {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    Line::styled(line, style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_line_count_includes_physical_rows() {
        let lines = [Line::raw("abcdefghijklmnop")];

        assert_eq!(wrapped_line_count(&lines, 8), 2);
    }
}
