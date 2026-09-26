use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::completion::CompletionKind;
use crate::transcript_ui::{style_transcript_line, wrapped_line_count};

use super::{FileBackedApp, MAX_COMPLETION_ROWS, MAX_COMPOSER_LINES, SPINNER};

pub(super) fn draw(frame: &mut Frame<'_>, app: &FileBackedApp) {
    let area = frame.area();
    let width = area.width as usize;
    let mut lines = history_lines(app, width);
    let history_height = wrapped_line_count(&lines, width);
    let (live_lines, prompt_start) = live_region_lines(app);
    let prompt_start =
        prompt_start.map(|index| history_height + wrapped_line_count(&live_lines[..index], width));
    lines.extend(live_lines);

    let cursor_position = prompt_start.map(|start| composer_cursor_position(app, width, start));
    let scroll_y = cursor_position.map(|(_, y)| y.saturating_add(1).saturating_sub(area.height));
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    match scroll_y {
        Some(scroll_y) => frame.render_widget(paragraph.scroll((scroll_y, 0)), area),
        None => frame.render_widget(paragraph, area),
    }
    if let Some((x, y)) = cursor_position
        && area.height > 0
    {
        let scroll_y = scroll_y.unwrap_or_default();
        frame.set_cursor_position((
            x.min(area.width.saturating_sub(1)),
            y.saturating_sub(scroll_y).min(area.height - 1),
        ));
    }
}

fn history_lines(app: &FileBackedApp, width: usize) -> Vec<Line<'static>> {
    app.rendered_history_lines(width)
        .into_iter()
        .map(style_transcript_line)
        .collect()
}

fn live_region_lines(app: &FileBackedApp) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = Vec::new();
    if let Some(cwd) = &app.activity.cwd {
        lines.push(Line::styled(
            format!("· directory: {cwd}"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if let Some(label) = app.activity_label() {
        lines.push(activity_line(app, label));
    }
    let queued = app.activity.pending_submissions.len();
    let queue_label = if app.activity.queue_paused {
        let action = if app.activity.active_submission.is_some() {
            "waiting for active input to settle"
        } else if app.pending_yield.is_some() || app.form.is_some() {
            "answer the pending request first"
        } else {
            "/continue to run · /discard to remove"
        };
        Some(format!("· queue paused · {queued} pending · {action}"))
    } else if queued > 0 {
        Some(format!("· {queued} queued"))
    } else {
        None
    };
    if let Some(label) = queue_label {
        lines.push(Line::styled(label, Style::default().fg(Color::Yellow)));
    }
    if let Some(notice) = &app.notice {
        lines.push(Line::styled(
            format!("· {notice}"),
            Style::default().fg(Color::Yellow),
        ));
    }
    for tool in &app.running_tools {
        lines.push(Line::styled(
            format!("· tool running: {}", tool.title),
            Style::default().fg(Color::Cyan),
        ));
    }

    if let Some(form) = &app.form {
        for (text, focused) in form.render_lines() {
            let style = if focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if text.trim_start().starts_with('!') {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            lines.push(Line::styled(text, style));
        }
        (lines, None)
    } else {
        if let Some(state) = &app.completion {
            for (idx, candidate) in state.matches.iter().take(MAX_COMPLETION_ROWS).enumerate() {
                let trigger = match state.kind {
                    CompletionKind::Command => "/",
                    CompletionKind::Skill => "$",
                    CompletionKind::File => "@",
                };
                let mut label = format!("{trigger}{}", candidate.label);
                if let Some(detail) = &candidate.detail {
                    label.push_str(&format!("  - {detail}"));
                }
                let style = if idx == state.selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };
                lines.push(Line::styled(format!("  {label}"), style));
            }
        }
        let prompt_start = lines.len();
        lines.extend(app.composer_lines());
        (lines, Some(prompt_start))
    }
}

pub(super) fn history_prefix_to_drain(
    lines: &[String],
    width: usize,
    max_retained_height: usize,
) -> usize {
    let mut retained_count = 0;
    let mut retained_height = 0;
    for line in lines.iter().rev() {
        let rendered = style_transcript_line(line.clone());
        let height = wrapped_line_count(std::slice::from_ref(&rendered), width);
        if retained_height + height > max_retained_height {
            break;
        }
        retained_height += height;
        retained_count += 1;
    }
    lines.len() - retained_count
}

pub(super) fn live_region_height(app: &FileBackedApp, width: usize) -> u16 {
    let (lines, prompt_start) = live_region_lines(app);
    let Some(prompt_start) = prompt_start else {
        return wrapped_line_count(&lines, width).max(1) as u16;
    };

    let before_prompt = wrapped_line_count(&lines[..prompt_start], width);
    let rendered_composer_height = wrapped_line_count(&lines[prompt_start..], width);
    let cursor_height = (composer_cursor_position(app, width, before_prompt)
        .1
        .saturating_add(1) as usize)
        .saturating_sub(before_prompt);
    (before_prompt
        + rendered_composer_height
            .max(cursor_height)
            .min(MAX_COMPOSER_LINES))
    .max(1) as u16
}

pub(super) fn inline_viewport_height(
    app: &FileBackedApp,
    width: usize,
    terminal_height: usize,
) -> u16 {
    wrapped_line_count(&history_lines(app, width), width)
        .saturating_add(live_region_height(app, width) as usize)
        .max(1)
        .min(terminal_height.max(1)) as u16
}

fn composer_cursor_position(app: &FileBackedApp, width: usize, prompt_start: usize) -> (u16, u16) {
    let text = app.composer.text();
    let cursor = app.composer.cursor().min(text.len());
    let before_cursor = text.get(..cursor).unwrap_or_default();
    let line_index = before_cursor.bytes().filter(|byte| *byte == b'\n').count();
    let cursor_in_line = before_cursor
        .rsplit_once('\n')
        .map_or(before_cursor.len(), |(_, line)| line.len());
    let composer_lines = app.composer_lines();
    let preceding_lines = composer_lines
        .iter()
        .take(line_index)
        .map(|line| wrapped_line_count(std::slice::from_ref(line), width))
        .sum::<usize>();
    let mut line = composer_lines.get(line_index).cloned().unwrap_or_default();
    let cursor_column = mark_cursor_in_line(&mut line, cursor_in_line).unwrap_or_default();
    let width = width.max(1).min(u16::MAX as usize) as u16;
    let height = wrapped_line_count(std::slice::from_ref(&line), width as usize)
        .max(1)
        .min(u16::MAX as usize) as u16;
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    Paragraph::new(line)
        .wrap(Wrap { trim: false })
        .render(buffer.area, &mut buffer);

    let marker_index = buffer
        .content()
        .iter()
        .position(|cell| cell.bg == Color::Magenta)
        .unwrap_or_default();
    let marker_position = (marker_index % width as usize, marker_index / width as usize);
    let cursor_column = marker_position.0 + cursor_column;
    let row_offset = cursor_column / width as usize;
    let x = cursor_column % width as usize;
    let y = prompt_start + preceding_lines + marker_position.1 + row_offset;
    (x as u16, y.min(u16::MAX as usize) as u16)
}

fn mark_cursor_in_line(line: &mut Line<'static>, cursor: usize) -> Option<usize> {
    let content_index = 1.min(line.spans.len().saturating_sub(1));
    if let Some((marked, cursor_column)) =
        mark_cursor_in_span(line.spans.get(content_index)?, cursor)
    {
        line.spans.splice(content_index..=content_index, marked);
        return Some(cursor_column);
    }
    let (marked, cursor_column) = mark_cursor_in_span(line.spans.first()?, usize::MAX)?;
    line.spans.splice(0..=0, marked);
    Some(cursor_column)
}

fn mark_cursor_in_span(span: &Span<'static>, cursor: usize) -> Option<(Vec<Span<'static>>, usize)> {
    let graphemes = span.styled_graphemes(Style::default()).collect::<Vec<_>>();
    let mut byte_offset = 0;
    let marker_index = graphemes
        .iter()
        .position(|grapheme| {
            let end = byte_offset + grapheme.symbol.len();
            let contains_cursor = cursor < end;
            byte_offset = end;
            contains_cursor
        })
        .or_else(|| graphemes.len().checked_sub(1))?;
    let marker = &graphemes[marker_index];
    let marker_start = graphemes[..marker_index]
        .iter()
        .map(|grapheme| grapheme.symbol.len())
        .sum::<usize>();
    let marker_end = marker_start + marker.symbol.len();
    let cursor_column = if cursor == usize::MAX || cursor >= span.content.len() {
        unicode_width::UnicodeWidthStr::width(marker.symbol)
    } else {
        let byte_offset = cursor.saturating_sub(marker_start).min(marker.symbol.len());
        unicode_width::UnicodeWidthStr::width(marker.symbol.get(..byte_offset).unwrap_or_default())
    };
    let content = span.content.to_string();
    let before = content[..marker_start].to_string();
    let marker_text = content[marker_start..marker_end].to_string();
    let after = content[marker_end..].to_string();
    let style = span.style;
    let mut marked = Vec::new();
    if !before.is_empty() {
        marked.push(Span::styled(before, style));
    }
    marked.push(Span::styled(marker_text, marker.style.bg(Color::Magenta)));
    if !after.is_empty() {
        marked.push(Span::styled(after, style));
    }
    Some((marked, cursor_column))
}

fn activity_line(app: &FileBackedApp, label: &str) -> Line<'static> {
    let elapsed = app
        .activity_started_at_ms()
        .and_then(|started_at_ms| {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis() as u64;
            Some(now_ms.saturating_sub(started_at_ms) / 1_000)
        })
        .unwrap_or(0);
    let frame_idx = (elapsed as usize) % SPINNER.len();
    Line::from(vec![
        Span::styled(
            format!("{} ", SPINNER[frame_idx]),
            Style::default().fg(Color::Green),
        ),
        Span::styled(
            label.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · ctrl+c/esc interrupt · {elapsed}s"),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_protocol::{InputIntent, UiSubmission};

    #[test]
    fn queue_state_stays_visible_and_only_offers_controls_after_the_active_input_settles() {
        let mut app = FileBackedApp::new("/agent/root".into());
        let input = UiSubmission {
            submission_id: "input".into(),
            intent: InputIntent::Command,
        };
        app.activity.pending_submissions.push(input.clone());
        let visible = |app: &FileBackedApp| {
            live_region_lines(app)
                .0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert!(visible(&app).contains("1 queued"));
        app.activity.queue_paused = true;
        app.activity.active_submission = Some(input);
        let settling = visible(&app);
        assert!(settling.contains("queue paused · 1 pending · waiting for active input to settle"));
        assert!(!settling.contains("/continue"));
        app.activity.active_submission = None;
        let paused = visible(&app);
        assert!(paused.contains("/continue to run · /discard to remove"));
        app.activity.pending_submissions.clear();
        assert!(visible(&app).contains("queue paused · 0 pending"));
        app.activity.queue_paused = false;
        assert!(!visible(&app).contains("queue"));
    }
}
