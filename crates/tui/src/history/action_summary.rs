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
    let style = status.style();
    let header = format!(
        "{} · {}",
        summary_row(
            &bounded(title, 256),
            width.saturating_sub(status.label().len() + 3),
            style
        ),
        status.label()
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
            format!("{}+{added} -{removed}", distinct_path(title, path))
        }
        Some(ToolResultPresentation::FileContent {
            path,
            lines,
            truncated,
        }) => format!(
            "{}{lines} lines{}",
            distinct_path(title, path),
            if *truncated { " · truncated" } else { "" }
        ),
        Some(ToolResultPresentation::Command {
            cmdline,
            exit_code,
            stdout,
            stderr,
            truncated,
            ..
        }) => format!(
            "exit {}{}{}{}",
            exit_code.map_or("unknown".into(), |v| v.to_string()),
            if title.contains(cmdline.lines().next().unwrap_or(cmdline)) {
                String::new()
            } else {
                format!(" · $ {}", bounded(cmdline, 160))
            },
            if *status == ToolStatus::Failed && !stderr.is_empty() {
                format!(" · {}", bounded(stderr, 128))
            } else if *status == ToolStatus::Failed && !stdout.is_empty() {
                format!(" · {}", bounded(stdout, 128))
            } else {
                String::new()
            },
            if *truncated { " · truncated" } else { "" }
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
    let mut rows = vec![header];
    if !child.trim().is_empty() {
        rows.push(format!("  {child}"));
    }
    rows.into_iter()
        .map(|s| summary_row(&s, width, style))
        .collect()
}

pub(super) fn summary_row(text: &str, width: usize, style: Style) -> Line<'static> {
    let clean = clean_text(text);
    let mut row = wrap_styled_lines([Line::styled(clean.clone(), style)], width.max(1))
        .into_iter()
        .next()
        .unwrap_or_default();
    if unicode_width::UnicodeWidthStr::width(clean.as_str()) > width && width > 1 {
        let mut text = row.to_string();
        while unicode_width::UnicodeWidthStr::width(text.as_str()) >= width {
            text.pop();
        }
        text.push('…');
        row = Line::styled(text, style);
    }
    row
}

fn distinct_path(title: &str, path: &str) -> String {
    if path.is_empty() || title.contains(path) {
        String::new()
    } else {
        format!("{} · ", bounded(path, 256))
    }
}

pub(crate) fn bounded(text: &str, bytes: usize) -> String {
    text.lines()
        .find(|line| !clean_text(line).trim().is_empty())
        .map_or_else(String::new, |line| {
            let mut end = line.len().min(bytes);
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line[..end].to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_unicode_titles_do_not_hide_terminal_outcomes() {
        for width in [48, 80, 120] {
            for status in [
                ToolStatus::Complete,
                ToolStatus::Failed,
                ToolStatus::Rejected,
                ToolStatus::Cancelled,
            ] {
                let cell = HistoryCell::Tool {
                    title: format!("Read {}", "路径🦀".repeat(80)),
                    status,
                    preview: None,
                    presentation: None,
                };
                let rows = action_summary(&cell, width);
                assert_eq!(rows.len(), 1);
                assert!(
                    rows[0].to_string().contains(status.label()),
                    "{:?}",
                    rows[0]
                );
                assert!(rows[0].width() <= width);
            }
        }
    }

    #[test]
    fn routine_summaries_keep_status_without_repeating_command_path_or_hint() {
        for width in [48, 80, 120] {
            for status in [ToolStatus::Complete, ToolStatus::Failed] {
                let command = HistoryCell::Tool {
                    title: "Bash cargo test".into(),
                    status,
                    preview: None,
                    presentation: Some(ToolResultPresentation::Command {
                        cmdline: "cargo test".into(),
                        exit_code: Some(101),
                        stdout: String::new(),
                        stderr: "compiler diagnostic".into(),
                        truncated: false,
                    }),
                };
                let rows = action_summary(&command, width);
                let text = rows
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(rows.len() <= 2, "{text}");
                assert_eq!(text.matches("cargo test").count(), 1, "{text}");
                assert!(text.contains("exit 101"), "{text}");
                assert!(
                    !text.contains("tool>") && !text.contains("details:"),
                    "{text}"
                );
                if status == ToolStatus::Failed {
                    assert!(text.contains("failed"), "{text}");
                }
            }
            let file = HistoryCell::Tool {
                title: "Read src/main.rs".into(),
                status: ToolStatus::Complete,
                preview: None,
                presentation: Some(ToolResultPresentation::FileContent {
                    path: "src/main.rs".into(),
                    lines: 42,
                    truncated: true,
                }),
            };
            let text = action_summary(&file, width)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(text.matches("src/main.rs").count(), 1, "{text}");
            assert!(
                text.contains("42 lines") && text.contains("truncated"),
                "{text}"
            );
        }
    }
}
