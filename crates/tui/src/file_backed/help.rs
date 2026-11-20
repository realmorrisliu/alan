//! Structured two-column `/help` formatting with hanging-indent wrapping.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

const COL_WIDTH: usize = 26;

const COMMAND_ROWS: &[(&str, &str)] = &[
    (
        "/login /model /settings",
        "connect, pick model, or settings & reset",
    ),
    ("/new /resume [query]", "fresh session or pick rollout"),
    ("/fork [query]", "roll back to earlier prompt & edit"),
    (
        "/permissions /project",
        "cycle trust or mount dir (Tab rw/ro)",
    ),
    ("/tape /man [topic]", "inspect Turing Tape or alan9 manuals"),
    ("/status /summary", "inspect status or project summary"),
    ("/compact /rollback", "summarize context or undo last turn"),
    ("/continue /discard", "resume or clear a paused queue"),
    ("/clear /help /quit", "clear view, show help, exit alan"),
];

const INPUT_ROWS: &[(&str, &str)] = &[
    (
        "Enter / Ctrl+S",
        "submit (: task, ! cmd, Tab / $ @) / steer",
    ),
    (
        "Shift/Alt+Enter, Ctrl+J",
        "insert newline (\\+Enter also inserts)",
    ),
    ("Ctrl+A/E, Alt+B/F", "line start/end, word back/forward"),
    (
        "Ctrl+K/U, Ctrl+_, Ctrl+G",
        "kill/clear line, undo edit, or $EDITOR",
    ),
];

const MODAL_ROWS: &[(&str, &str)] = &[
    (
        "Shift+Tab / Ctrl+R",
        "cycle trust posture / expand thinking",
    ),
    ("Ctrl+Z / Ctrl+D", "suspend (fg resumes) / exit if empty"),
    ("Ctrl+C / Esc", "interrupt, clear draft, or dismiss notice"),
    (
        "Ctrl+O",
        "details modal (↑↓/jk, ←→/[] item, d JSON, p Plans, t Tape, PgUp/PgDn, arrows, Esc)",
    ),
];

/// Render plain structured two-column help text for `Notice` storage.
pub(in crate::file_backed) fn help_text() -> String {
    let mut lines = Vec::new();
    append_section(&mut lines, "Commands:", COMMAND_ROWS);
    append_section(&mut lines, "Input & Editing:", INPUT_ROWS);
    append_section(&mut lines, "Shortcuts & Modals:", MODAL_ROWS);
    lines.join("\n")
}

fn append_section(out: &mut Vec<String>, header: &str, rows: &[(&str, &str)]) {
    out.push(header.to_string());
    for (left, right) in rows {
        out.push(format!("  {:<width$} {}", left, right, width = COL_WIDTH));
    }
}

/// If `notice` is a structured two-column help block, format it for `width`
/// so wrapped right-column descriptions preserve hanging indentation.
pub(in crate::file_backed) fn try_render_help_lines(
    notice: &str,
    theme: Theme,
    width: usize,
) -> Option<Vec<Line<'static>>> {
    if !notice.starts_with("Commands:\n") {
        return None;
    }
    let width = width.max(16);
    let mut lines = Vec::new();
    for raw in notice.lines() {
        if matches!(
            raw,
            "Commands:" | "Input & Editing:" | "Shortcuts & Modals:"
        ) {
            if theme.is_dumb {
                lines.push(Line::raw(raw.to_string()));
            } else {
                lines.push(Line::styled(raw.to_string(), theme.accent_bold()));
            }
            continue;
        }
        if let Some(rest) = raw.strip_prefix("  ") {
            let split_at = rest
                .char_indices()
                .nth(COL_WIDTH)
                .map_or(rest.len(), |(idx, _)| idx);
            let left_raw = rest[..split_at].trim_end();
            let right = rest[split_at..].trim_start();
            render_help_row(&mut lines, left_raw, right, theme, width);
        } else {
            lines.push(Line::raw(raw.to_string()));
        }
    }
    Some(lines)
}

fn render_help_row(
    lines: &mut Vec<Line<'static>>,
    left: &str,
    right: &str,
    theme: Theme,
    width: usize,
) {
    let left_style = if theme.is_dumb {
        Style::default()
    } else {
        theme.accent_style()
    };
    let left_w = left.width();
    if width >= 44 && left_w <= COL_WIDTH && width > 2 + COL_WIDTH + 8 {
        let indent = 2 + COL_WIDTH + 1;
        let desc_width = width.saturating_sub(indent).max(8);
        let padded_left = format!("{left:<col$}", col = COL_WIDTH);
        let wrapped = textwrap::wrap(right, desc_width);
        if wrapped.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(padded_left, left_style),
            ]));
            return;
        }
        let mut iter = wrapped.into_iter();
        if let Some(first) = iter.next() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(padded_left, left_style),
                Span::raw(" "),
                Span::raw(first.into_owned()),
            ]));
        }
        let pad = " ".repeat(indent);
        for cont in iter {
            lines.push(Line::from(vec![
                Span::raw(pad.clone()),
                Span::raw(cont.into_owned()),
            ]));
        }
    } else {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(left.to_string(), left_style),
        ]));
        let desc_width = width.saturating_sub(4).max(8);
        for cont in textwrap::wrap(right, desc_width) {
            lines.push(Line::raw(format!("    {cont}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_wraps_right_column_with_hanging_indentation() {
        let text = help_text();
        let lines = try_render_help_lines(&text, Theme::midnight(), 80).expect("help lines");
        assert!(
            lines.len() <= 21,
            "help should fit comfortably in 80x24 without hiding Commands header: {}",
            lines.len()
        );
        let rendered: Vec<String> = lines.iter().map(ToString::to_string).collect();
        let ctrl_o_idx = rendered
            .iter()
            .position(|l| l.starts_with("  Ctrl+O"))
            .expect("Ctrl+O row present");
        let continuation = rendered
            .get(ctrl_o_idx + 1)
            .expect("Ctrl+O wraps onto continuation line at width 80");
        let expected_indent = " ".repeat(2 + COL_WIDTH + 1);
        assert!(
            continuation.starts_with(&expected_indent),
            "wrapped continuation lost hanging indent: {continuation:?}"
        );
        assert!(
            rendered.iter().all(|l| l.width() <= 80),
            "help line exceeded 80 cols: {rendered:?}"
        );
    }

    #[test]
    fn help_includes_new_commands_and_shortcuts_in_dumb_and_narrow_modes() {
        let text = help_text();
        for token in ["/login", "/fork", "/permissions", "Shift+Tab", "Ctrl+Z"] {
            assert!(text.contains(token), "missing {token} in help text");
        }
        let narrow = try_render_help_lines(&text, Theme::dumb(), 36).expect("narrow help");
        assert!(narrow.iter().all(|l| l.to_string().width() <= 36));
    }
}
