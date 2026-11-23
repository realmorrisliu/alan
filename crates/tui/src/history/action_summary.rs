use crate::theme::{DotState, Theme};

use super::*;

const COMPACT_DIFF_PREVIEW_LINES: usize = 8;

/// Routine Action projection, distinct from retained full presentation/detail.
#[cfg(test)]
pub(crate) fn action_summary(cell: &HistoryCell, width: usize) -> Vec<Line<'static>> {
    action_summary_with_theme(cell, width, Theme::default())
}

/// Routine Action projection rendered with the active [`Theme`].
pub(crate) fn action_summary_with_theme(
    cell: &HistoryCell,
    width: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let HistoryCell::Tool {
        title,
        status,
        preview,
        presentation,
        ..
    } = cell
    else {
        return cell.render_styled_lines(RenderOpts::new(width, false).with_theme(theme));
    };
    let width = width.max(1);
    let style = status.style(theme);
    let (dot, dot_style) = status.dot(theme);
    let dot_prefix = format!("{dot} ");
    let dot_width = unicode_width::UnicodeWidthStr::width(dot_prefix.as_str());
    let status_suffix = format!(" · {}", status.label());
    let suffix_width = unicode_width::UnicodeWidthStr::width(status_suffix.as_str());
    let title_budget = width.saturating_sub(dot_width + suffix_width).max(1);
    let clamped_title = clamp_text_width(&clean_text(&bounded(title, 256)), title_budget);
    let header_line = fit_line_to_width(
        Line::from(vec![
            Span::styled(dot_prefix, dot_style),
            Span::styled(clamped_title, style),
            Span::styled(status_suffix, style),
        ]),
        width,
    );
    if let Some(ToolResultPresentation::Diff { path, hunks }) = presentation {
        let mut added = 0usize;
        let mut removed = 0usize;
        for line in hunks.iter().flat_map(|h| &h.lines) {
            match line {
                DiffLine::Added { .. } => added += 1,
                DiffLine::Removed { .. } => removed += 1,
                _ => {}
            }
        }
        let mut rows = vec![header_line];
        let stats_line = format!("  {}+{added} -{removed}", distinct_path(title, path));
        rows.push(summary_row(&stats_line, width, style));
        append_compact_diff_rows(hunks, width, theme, &mut rows);
        return rows;
    }
    let child = match presentation {
        Some(ToolResultPresentation::Diff { .. }) => unreachable!("handled above"),
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
            } else if !stdout.is_empty() {
                format!(" · {}", bounded(stdout, 128))
            } else if !stderr.is_empty() {
                format!(" · {}", bounded(stderr, 128))
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
    let mut rows = vec![header_line];
    if !child.trim().is_empty() {
        rows.push(summary_row(&format!("  {child}"), width, style));
    }
    rows
}

fn append_compact_diff_rows(
    hunks: &[alan_agent_protocol::DiffHunk],
    width: usize,
    theme: Theme,
    rows: &mut Vec<Line<'static>>,
) {
    let total_lines: usize = hunks.iter().map(|h| h.lines.len()).sum();
    if total_lines == 0 {
        return;
    }
    let include_context = total_lines <= COMPACT_DIFF_PREVIEW_LINES;
    let gutter = theme.gutter();
    let mut candidates: Vec<(Style, String, Style)> = Vec::new();
    for hunk in hunks {
        if let Some(header) = &hunk.header
            && hunks.len() > 1
        {
            candidates.push((theme.rule_style(), header.clone(), theme.muted_style()));
        }
        for line in &hunk.lines {
            match line {
                DiffLine::Added { text } => {
                    candidates.push((
                        theme.diff_add_style(),
                        format!("+{text}"),
                        theme.diff_add_style(),
                    ));
                }
                DiffLine::Removed { text } => {
                    candidates.push((
                        theme.diff_remove_style(),
                        format!("-{text}"),
                        theme.diff_remove_style(),
                    ));
                }
                DiffLine::Context { text } if include_context => {
                    candidates.push((theme.rule_style(), format!(" {text}"), Style::default()));
                }
                DiffLine::Context { .. } => {}
            }
        }
    }
    let hidden = candidates.len().saturating_sub(COMPACT_DIFF_PREVIEW_LINES);
    for (gstyle, text, style) in candidates.into_iter().take(COMPACT_DIFF_PREVIEW_LINES) {
        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled(gutter.to_string(), gstyle),
            Span::styled(clean_text(&text), style),
        ]);
        rows.push(fit_line_to_width(line, width));
    }
    if hidden > 0 {
        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled(gutter.to_string(), theme.rule_style()),
            Span::styled(
                format!("… +{hidden} more diff lines (Ctrl+O for full diff)"),
                theme.muted_style(),
            ),
        ]);
        rows.push(fit_line_to_width(line, width));
    }
}

#[allow(
    dead_code,
    reason = "convenience wrapper kept alongside theme-aware variant"
)]
pub(crate) fn group_summary_header(count: usize, width: usize) -> Line<'static> {
    group_summary_header_with_theme(count, width, Theme::default())
}

pub(crate) fn group_summary_header_with_theme(
    count: usize,
    width: usize,
    theme: Theme,
) -> Line<'static> {
    let (dot, dot_style) = theme.status_dot(DotState::Run);
    fit_line_to_width(
        Line::from(vec![
            Span::styled(format!("{dot} "), dot_style),
            Span::styled(
                format!("{count} read-only actions · completed"),
                ToolStatus::Complete.style(theme),
            ),
        ]),
        width,
    )
}

#[allow(
    dead_code,
    reason = "convenience wrapper kept alongside theme-aware variant"
)]
pub(crate) fn group_summary_member(cell: &HistoryCell, width: usize) -> Line<'static> {
    group_summary_member_with_theme(cell, width, Theme::default())
}

pub(crate) fn group_summary_member_with_theme(
    cell: &HistoryCell,
    width: usize,
    theme: Theme,
) -> Line<'static> {
    let HistoryCell::Tool { title, .. } = cell else {
        unreachable!("eligible Action")
    };
    let rows = action_summary_with_theme(cell, 512, theme);
    let detail = rows.get(1).map(ToString::to_string).unwrap_or_default();
    let text = if detail.trim().is_empty() {
        format!("  {}", bounded(title, 256))
    } else {
        format!("  {} · {}", bounded(title, 256), detail.trim())
    };
    summary_row(&text, width, ToolStatus::Complete.style(theme))
}

pub(crate) fn clamp_text_width(text: &str, max_width: usize) -> String {
    if unicode_width::UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w >= max_width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

pub(crate) fn fit_line_to_width(line: Line<'static>, width: usize) -> Line<'static> {
    let width = width.max(1);
    if line.width() <= width {
        return line;
    }
    let mut spans = Vec::new();
    let mut used = 0usize;
    let limit = width.saturating_sub(1);
    for span in line.spans {
        let style = span.style;
        let mut chunk = String::new();
        for grapheme in span.styled_graphemes(style) {
            let w = unicode_width::UnicodeWidthStr::width(grapheme.symbol);
            if used + w > limit {
                if !chunk.is_empty() {
                    spans.push(Span::styled(chunk, style));
                }
                if width > 1 {
                    spans.push(Span::styled("…", style));
                }
                return Line::from(spans);
            }
            chunk.push_str(grapheme.symbol);
            used += w;
        }
        if !chunk.is_empty() {
            spans.push(Span::styled(chunk, style));
        }
    }
    Line::from(spans)
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
                    action: None,
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
                    action: None,
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
                action: None,
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
            let edit = HistoryCell::Tool {
                action: None,
                title: "Edit /mnt/p/sample.rs".into(),
                status: ToolStatus::Complete,
                preview: None,
                presentation: Some(ToolResultPresentation::Diff {
                    path: "/mnt/p/sample.rs".into(),
                    hunks: vec![alan_agent_protocol::DiffHunk {
                        header: None,
                        lines: vec![
                            DiffLine::Added { text: "new".into() },
                            DiffLine::Removed { text: "old".into() },
                        ],
                    }],
                }),
            };
            let text = action_summary(&edit, width)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(text.matches("sample.rs").count(), 1, "{text}");
            assert!(text.contains("+1 -1"), "{text}");
        }
    }

    #[test]
    fn command_excerpt_preserves_success_output_and_failure_stderr_priority() {
        for width in [48, 80, 120] {
            for (status, stdout, stderr, expected) in [
                (
                    ToolStatus::Complete,
                    "server> ready\na > b",
                    "",
                    "server> ready",
                ),
                (
                    ToolStatus::Complete,
                    "",
                    "warning: cached",
                    "warning: cached",
                ),
                (
                    ToolStatus::Failed,
                    "prior output",
                    "actual failure",
                    "actual failure",
                ),
            ] {
                let cell = HistoryCell::Tool {
                    action: None,
                    title: "Execute fixture".into(),
                    status,
                    preview: None,
                    presentation: Some(ToolResultPresentation::Command {
                        cmdline: "fixture".into(),
                        exit_code: Some(if status == ToolStatus::Failed { 1 } else { 0 }),
                        stdout: stdout.into(),
                        stderr: stderr.into(),
                        truncated: false,
                    }),
                };
                let rows = action_summary(&cell, width);
                assert_eq!(rows.len(), 2);
                assert!(rows[0].to_string().contains(status.label()));
                assert!(rows[1].to_string().contains(expected), "{rows:?}");
                assert!(rows.iter().all(|row| row.width() <= width));
            }
        }
    }
}
