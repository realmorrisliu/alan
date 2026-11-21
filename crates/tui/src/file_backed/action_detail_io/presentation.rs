//! Pure presentation of acquired structured text; never substitutes summary previews.
use ratatui::text::Line;

use crate::theme::Theme;

pub(super) fn acquired_content(rows: &mut Vec<Line<'static>>, text: &str, theme: Theme) -> bool {
    let structured = structured_text(text, theme);
    let readable = !structured.is_empty();
    rows.extend(structured);
    if readable {
        rows.push(Line::styled("Original raw bytes", theme.muted_style()));
    }
    rows.extend(
        text.lines()
            .map(|s| Line::from(crate::history::clean_text(s))),
    );
    readable
}

pub(super) fn structured_text(text: &str, theme: Theme) -> Vec<Line<'static>> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    if let (Some(stdout), Some(stderr), Some(exit_code)) = (
        value["stdout"].as_str(),
        value["stderr"].as_str(),
        value["exit_code"]
            .as_i64()
            .and_then(|code| i32::try_from(code).ok()),
    ) {
        let mut rows = Vec::new();
        for (label, stream) in [("stdout", stdout), ("stderr", stderr)] {
            rows.push(Line::styled(label, theme.muted_style()));
            rows.extend(
                stream
                    .lines()
                    .map(|line| Line::from(crate::history::clean_text(line))),
            );
        }
        rows.push(Line::styled(
            format!("exit {exit_code}"),
            theme.muted_style(),
        ));
        return rows;
    }
    if value["success"].as_bool() == Some(false)
        && let Some(error) = value["error"].as_str()
        && !error.trim().is_empty()
    {
        let mut lines = error.lines();
        let first = lines.next().unwrap_or("");
        let mut rows = vec![Line::styled(
            crate::history::clean_text(&format!("Error: {first}")),
            theme.block_style(),
        )];
        rows.extend(
            lines.map(|line| Line::styled(crate::history::clean_text(line), theme.block_style())),
        );
        return rows;
    }
    let (
        Some("text"),
        Some(path),
        Some(content),
        Some(total),
        Some(start),
        Some(end),
        Some(truncated),
    ) = (
        value["type"].as_str(),
        value["path"].as_str(),
        value["content"].as_str(),
        value["total_lines"].as_u64(),
        value["start_line"].as_u64(),
        value["end_line"].as_u64(),
        value["truncated"].as_bool(),
    )
    else {
        return Vec::new();
    };
    if start > end || end > total {
        return Vec::new();
    }
    let mut rows = vec![Line::styled(
        crate::history::clean_text(&format!(
            "Readable text: {path} · lines {start}–{end} of {total}{}",
            if truncated { " · truncated" } else { "" }
        )),
        theme.muted_style(),
    )];
    rows.extend(
        content
            .lines()
            .map(|line| Line::from(crate::history::clean_text(line))),
    );
    rows
}
