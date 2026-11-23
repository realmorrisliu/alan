//! TUI rendering and cursor layout for `/issue` drafting, editing, and submission.

use ratatui::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{IssueDraft, IssueDraftSource, IssueFieldFocus, IssuePhase};
use crate::file_backed::{FileBackedApp, SPINNER};
use crate::history::clean_text;

pub(in crate::file_backed) fn draw(frame: &mut Frame<'_>, app: &FileBackedApp, now_ms: u64) {
    let area = frame.area();
    let width = area.width as usize;
    let height = area.height as usize;
    let (lines, cursor, body_start_row) = build_view(app, width, now_ms);
    let skip = if height == 0 || lines.len() <= height {
        0
    } else if let Some((_, cy)) = cursor {
        let cy = cy as usize;
        if cy >= height {
            (cy + 1)
                .saturating_sub(height)
                .min(body_start_row.max(cy + 1 - height))
        } else {
            0
        }
    } else {
        0
    };
    let visible: Vec<Line<'static>> = lines.into_iter().skip(skip).take(height).collect();
    frame.render_widget(Paragraph::new(visible), area);
    if let Some((cx, cy)) = cursor
        && height > 0
        && (cy as usize) >= skip
        && (cy as usize) < skip + height
    {
        frame.set_cursor_position((
            area.x + cx.min(area.width.saturating_sub(1)),
            area.y + (cy - skip as u16).min(area.height - 1),
        ));
    }
}

#[cfg(test)]
pub(in crate::file_backed) fn render_lines(
    app: &FileBackedApp,
    width: usize,
) -> Vec<Line<'static>> {
    build_view(app, width, 0).0
}

fn build_view(
    app: &FileBackedApp,
    width: usize,
    now_ms: u64,
) -> (Vec<Line<'static>>, Option<(u16, u16)>, usize) {
    let theme = app.theme;
    let width = width.max(16);
    match &app.issue.phase {
        IssuePhase::Idle => (Vec::new(), None, 0),
        IssuePhase::Drafting {
            repo,
            started_at_ms,
            ..
        } => {
            let spinner_idx =
                ((now_ms.saturating_sub(*started_at_ms) / 100) as usize) % SPINNER.len();
            let lines = vec![
                Line::styled(
                    format!("Drafting Issue · {}", clean_text(repo)),
                    theme.accent_bold(),
                ),
                Line::from(vec![
                    Span::styled(format!("{} ", SPINNER[spinner_idx]), theme.run_style()),
                    Span::styled(
                        "Summarizing redacted diagnostics with AI… (Esc edit now · Ctrl+C cancel)",
                        theme.muted_style(),
                    ),
                ]),
                Line::styled(
                    "Redacted diagnostics only (no transcript, project code, or secrets)",
                    theme.muted_style(),
                ),
            ];
            (lines, None, 0)
        }
        IssuePhase::Submitting {
            draft,
            submitted_title,
        } => {
            let lines = vec![
                Line::styled(
                    format!("Submitting Issue · {}", clean_text(&draft.repo)),
                    theme.accent_bold(),
                ),
                Line::styled("Please wait… (Esc to close)", theme.muted_style()),
                Line::default(),
                Line::from(format!("Title: {}", clean_text(submitted_title))),
            ];
            (lines, None, 0)
        }
        IssuePhase::Editing(IssueDraft {
            repo,
            title,
            title_cursor,
            body,
            body_cursor,
            focus,
            auto_label,
            draft_source,
            diagnostics,
            warning,
        }) => {
            let hint = if width >= 78 {
                "Tab switch · Enter submit · Shift+Enter/Ctrl+J newline · Ctrl+L label · Esc cancel"
            } else {
                "Tab field · Enter submit · Ctrl+L label · Esc cancel"
            };
            let mut rows = vec![
                Line::styled(
                    format!("New Issue · {}", clean_text(repo)),
                    theme.accent_bold(),
                ),
                Line::styled(hint, theme.muted_style()),
                Line::styled(
                    "Redacted diagnostics only (no transcript, project code, or secrets)",
                    theme.muted_style(),
                ),
            ];
            match draft_source {
                IssueDraftSource::AiDrafted => rows.push(Line::styled(
                    "AI drafted from redacted diagnostics — review & Enter to submit",
                    theme.run_style(),
                )),
                IssueDraftSource::DeterministicFallback
                    if !title.is_empty() || diagnostics.has_real_failure() =>
                {
                    rows.push(Line::styled(
                        "Pre-filled from redacted diagnostics — review & Enter to submit",
                        theme.muted_style(),
                    ));
                }
                IssueDraftSource::Manual | IssueDraftSource::DeterministicFallback => {}
            }
            rows.push(Line::styled(
                if theme.is_dumb { "-" } else { "─" }.repeat(width),
                theme.rule_style(),
            ));
            if let Some(warn) = warning {
                rows.push(Line::styled(format!("! {warn}"), theme.wait_style()));
            }
            let title_focused = *focus == IssueFieldFocus::Title;
            let title_prefix = if title_focused {
                "▶ Title: "
            } else {
                "  Title: "
            };
            let title_style = if title_focused {
                theme.accent_bold()
            } else {
                theme.muted_style()
            };
            let mut cursor_pos = None;
            let wrapped_title =
                wrap_logical_line(title, width.saturating_sub(title_prefix.width()).max(1));
            for (idx, seg) in wrapped_title.iter().enumerate() {
                let prefix = if idx == 0 { title_prefix } else { "         " };
                let row_y = rows.len();
                if title_focused
                    && cursor_pos.is_none()
                    && *title_cursor >= seg.start
                    && *title_cursor <= seg.end
                {
                    let col = seg.text[..(*title_cursor - seg.start).min(seg.text.len())].width();
                    cursor_pos = Some(((prefix.width() + col) as u16, row_y as u16));
                }
                let content_span = if title.is_empty() && !title_focused && idx == 0 {
                    Span::styled("(enter issue title)", theme.muted_style())
                } else {
                    Span::raw(seg.text.clone())
                };
                rows.push(Line::from(vec![
                    Span::styled(prefix, title_style),
                    content_span,
                ]));
            }
            let label_row = if *auto_label {
                Line::from(vec![
                    Span::styled("  Labels: ", theme.muted_style()),
                    Span::styled("[x] alan:auto", theme.accent_style()),
                    Span::styled(" (Ctrl+L toggle)", theme.muted_style()),
                ])
            } else {
                Line::styled(
                    "  Labels: [ ] none (Ctrl+L: alan:auto)",
                    theme.muted_style(),
                )
            };
            rows.push(label_row);
            let body_focused = *focus == IssueFieldFocus::Body;
            let body_header = if body_focused { "▶ Body:" } else { "  Body:" };
            let body_style = if body_focused {
                theme.accent_bold()
            } else {
                theme.muted_style()
            };
            rows.push(Line::styled(body_header, body_style));
            let body_start_row = rows.len();
            let gutter = if theme.is_dumb { "  | " } else { "  │ " };
            let gutter_w = gutter.width();
            let body_avail = width.saturating_sub(gutter_w).max(1);
            let mut offset = 0usize;
            let mut lines_iter = body.split('\n').peekable();
            while let Some(logical) = lines_iter.next() {
                let line_start = offset;
                let line_end = line_start + logical.len();
                for seg in wrap_logical_line(logical, body_avail) {
                    let row_y = rows.len();
                    let abs_start = line_start + seg.start;
                    let abs_end = line_start + seg.end;
                    if body_focused
                        && cursor_pos.is_none()
                        && *body_cursor >= abs_start
                        && *body_cursor <= abs_end
                    {
                        let col =
                            seg.text[..(*body_cursor - abs_start).min(seg.text.len())].width();
                        cursor_pos = Some(((gutter_w + col) as u16, row_y as u16));
                    }
                    rows.push(Line::from(vec![
                        Span::styled(gutter, theme.rule_style()),
                        Span::raw(seg.text),
                    ]));
                }
                offset = line_end + usize::from(lines_iter.peek().is_some());
            }
            (rows, cursor_pos, body_start_row)
        }
    }
}

struct WrappedSeg {
    text: String,
    start: usize,
    end: usize,
}

fn wrap_logical_line(line: &str, max_width: usize) -> Vec<WrappedSeg> {
    if line.is_empty() {
        return vec![WrappedSeg {
            text: String::new(),
            start: 0,
            end: 0,
        }];
    }
    let max_width = max_width.max(1);
    let mut segs = Vec::new();
    let mut seg_start = 0usize;
    let mut used = 0usize;
    for (idx, ch) in line.char_indices() {
        let w = ch.width().unwrap_or(0);
        if used + w > max_width && idx > seg_start {
            segs.push(WrappedSeg {
                text: line[seg_start..idx].to_string(),
                start: seg_start,
                end: idx,
            });
            seg_start = idx;
            used = w;
        } else {
            used += w;
        }
    }
    segs.push(WrappedSeg {
        text: line[seg_start..].to_string(),
        start: seg_start,
        end: line.len(),
    });
    segs
}
