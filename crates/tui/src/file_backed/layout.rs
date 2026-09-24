use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::completion::CompletionKind;
use crate::transcript_ui::style_transcript_line;

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
    if let Some(label) = app.activity_label() {
        lines.push(activity_line(app, label));
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

fn wrapped_line_count(lines: &[Line<'_>], width: usize) -> usize {
    if lines.is_empty() {
        return 0;
    }
    let width = width.max(1).min(u16::MAX as usize) as u16;
    Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .line_count(width)
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
    let segments = before_cursor.split('\n').collect::<Vec<_>>();
    let line_index = segments.len().saturating_sub(1);
    let line = segments.last().copied().unwrap_or_default();
    let preceding_lines = app
        .composer_lines()
        .into_iter()
        .take(line_index)
        .map(|line| wrapped_line_count(std::slice::from_ref(&line), width))
        .sum::<usize>();
    let column = unicode_width::UnicodeWidthStr::width(app.input_prompt_prefix())
        + unicode_width::UnicodeWidthStr::width(line);
    let width = width.max(1);
    let row = prompt_start + preceding_lines + column / width;
    ((column % width) as u16, row as u16)
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
