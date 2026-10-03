use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

pub(crate) const INLINE_PROMPT_PREFIX: &str = ": ";
pub(crate) const INLINE_WAITING_PROMPT_PREFIX: &str = "? ";
pub(crate) const INLINE_COMMAND_PROMPT_PREFIX: &str = "! ";
pub(crate) const INLINE_PROMPT_CONTINUATION: &str = "  ";

pub(crate) fn wrapped_line_count(lines: &[Line<'_>], width: usize) -> usize {
    if lines.is_empty() {
        return 0;
    }
    let width = width.max(1).min(u16::MAX as usize) as u16;
    Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .line_count(width)
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
