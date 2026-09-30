use super::*;

/// Routine Action projection, distinct from retained full presentation/detail.
pub(crate) fn action_summary(cell: &HistoryCell, width: usize) -> Vec<Line<'static>> {
    let HistoryCell::Tool {
        title,
        status,
        preview,
        presentation,
    } = cell
    else {
        return cell.render_styled_lines(RenderOpts::new(width, false));
    };
    let width = width.max(1);
    let style = Style::default().fg(if *status == ToolStatus::Failed {
        Color::Red
    } else {
        Color::Cyan
    });
    let title = format!(
        "tool> {} {}",
        if *status == ToolStatus::Failed {
            "✗"
        } else {
            "✓"
        },
        bounded(title, 256)
    );
    let child = match presentation {
        Some(ToolResultPresentation::Diff { path, hunks }) => {
            let mut added = 0;
            let mut removed = 0;
            for line in hunks.iter().flat_map(|h| &h.lines) {
                match line {
                    DiffLine::Added { .. } => added += 1,
                    DiffLine::Removed { .. } => removed += 1,
                    _ => {}
                }
            }
            format!("{} · +{added} -{removed}", bounded(path, 256))
        }
        Some(ToolResultPresentation::FileContent {
            path,
            lines,
            truncated,
        }) => format!(
            "{} · {lines} lines{}",
            bounded(path, 256),
            if *truncated { " · truncated" } else { "" }
        ),
        Some(ToolResultPresentation::Command {
            cmdline,
            exit_code,
            stderr,
            ..
        }) => format!(
            "exit {} · $ {}{}",
            exit_code.map_or("unknown".into(), |v| v.to_string()),
            bounded(cmdline, 160),
            if *status == ToolStatus::Failed && !stderr.is_empty() {
                format!(" · {}", bounded(stderr, 128))
            } else {
                String::new()
            }
        ),
        Some(ToolResultPresentation::PlainText { body }) => bounded(body, 256),
        Some(ToolResultPresentation::Listing { rows }) => format!(
            "{} rows · {}",
            rows.len(),
            rows.first().map_or(String::new(), |v| bounded(v, 256))
        ),
        None => preview
            .as_deref()
            .map_or(String::new(), |v| bounded(v, 256)),
    };
    [title, format!("  {child}"), "  details: Ctrl+O".into()]
        .into_iter()
        .map(|s| {
            let mut row = wrap_styled_lines([Line::styled(clean_text(&s), style)], width)
                .into_iter()
                .next()
                .unwrap_or_default();
            if unicode_width::UnicodeWidthStr::width(clean_text(&s).as_str()) > width && width > 1 {
                let mut text = row.to_string();
                while unicode_width::UnicodeWidthStr::width(text.as_str()) >= width {
                    text.pop();
                }
                text.push('…');
                row = Line::styled(text, style);
            }
            wrap_styled_lines([row], width)
                .into_iter()
                .next()
                .unwrap_or_default()
        })
        .collect()
}

pub(crate) fn bounded(text: &str, bytes: usize) -> String {
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}
