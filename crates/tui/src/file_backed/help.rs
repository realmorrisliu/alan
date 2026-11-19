//! Structured two-column `/help` formatting.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme::Theme;

const COL_WIDTH: usize = 26;

const COMMAND_ROWS: &[(&str, &str)] = &[
    (
        "/new /resume [query]",
        "fresh view or pick retained rollout",
    ),
    ("/model [query]", "fuzzy model and reasoning effort picker"),
    ("/tape /man [topic]", "inspect Turing Tape or alan9 manuals"),
    ("/status /summary", "inspect status or continuity summary"),
    (
        "/project [revoke]",
        "mount or revoke a project directory (Tab rw/ro)",
    ),
    (
        "/compact /rollback",
        "summarize context or undo the last turn",
    ),
    ("/continue /discard", "resume or clear a paused queue"),
    (
        "/clear /help /quit",
        "clear transcript, show help, exit alan",
    ),
];

const INPUT_ROWS: &[(&str, &str)] = &[
    ("Enter", "submit (: task, ! command, Tab completes / $ @)"),
    ("Ctrl+S", "send live steer message while a turn is running"),
    (
        "Shift/Alt+Enter, Ctrl+J",
        "insert newline (trailing \\+Enter also inserts)",
    ),
    ("Ctrl+A/E, Home/End", "move to start or end of line"),
    ("Alt+B/F, Alt+Backspace", "move or delete by word"),
    (
        "Ctrl+K/U, Delete",
        "kill to line end, clear line, delete forward",
    ),
    (
        "Ctrl+_, Ctrl+G",
        "undo edit, or compose in external $EDITOR",
    ),
];

const MODAL_ROWS: &[(&str, &str)] = &[
    ("Ctrl+R", "toggle expanded thinking blocks"),
    (
        "Ctrl+C / Esc",
        "interrupt active work or clear composer draft",
    ),
    ("Ctrl+D", "exit when composer is empty"),
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

/// If `notice` is a structured two-column help block, style section headers and left keys.
pub(in crate::file_backed) fn try_render_help_lines(
    notice: &str,
    theme: Theme,
) -> Option<Vec<Line<'static>>> {
    if !notice.starts_with("Commands:\n") {
        return None;
    }
    if theme.is_dumb {
        return Some(notice.lines().map(|l| Line::raw(l.to_string())).collect());
    }
    let mut lines = Vec::new();
    for raw in notice.lines() {
        if matches!(
            raw,
            "Commands:" | "Input & Editing:" | "Shortcuts & Modals:"
        ) {
            lines.push(Line::styled(raw.to_string(), theme.accent_bold()));
        } else if let Some(rest) = raw.strip_prefix("  ") {
            let split_at = rest
                .char_indices()
                .nth(COL_WIDTH)
                .map_or(rest.len(), |(idx, _)| idx);
            let left = &rest[..split_at];
            let right = &rest[split_at..];
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(left.to_string(), theme.accent_style()),
                Span::styled(right.to_string(), Style::default()),
            ]));
        } else {
            lines.push(Line::raw(raw.to_string()));
        }
    }
    Some(lines)
}
