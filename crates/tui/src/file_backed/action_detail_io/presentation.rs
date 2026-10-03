//! Pure presentation of acquired structured text; never substitutes summary previews.
use ratatui::text::Line;

pub(super) fn acquired_content(rows: &mut Vec<Line<'static>>, text: &str) -> bool {
    let structured = structured_text(text);
    let readable = !structured.is_empty();
    rows.extend(structured);
    if readable {
        rows.push(Line::from("Original raw bytes"));
    }
    rows.extend(
        text.lines()
            .map(|s| Line::from(crate::history::clean_text(s))),
    );
    readable
}

pub(super) fn structured_text(text: &str) -> Vec<Line<'static>> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
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
    let mut rows = vec![Line::from(crate::history::clean_text(&format!(
        "Readable text: {path} · lines {start}–{end} of {total}{}",
        if truncated { " · truncated" } else { "" }
    )))];
    rows.extend(
        content
            .lines()
            .map(|line| Line::from(crate::history::clean_text(line))),
    );
    rows
}
