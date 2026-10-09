use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::completion::CompletionKind;
use crate::transcript_ui::wrapped_line_count;

use super::{FileBackedApp, MAX_COMPLETION_ROWS, MAX_COMPOSER_LINES, SPINNER};

#[cfg(test)]
pub(super) fn draw(frame: &mut Frame<'_>, app: &FileBackedApp) {
    draw_at(frame, app, super::unix_time_ms());
}

pub(super) fn draw_at(frame: &mut Frame<'_>, app: &FileBackedApp, now_ms: u64) {
    let area = frame.area();
    let width = area.width as usize;
    if app.modal.active {
        let mut rows = vec![Line::from(format!(
            "{}/{} · ↔ Action · Space/b page · Esc",
            app.modal.selected + 1,
            app.modal.ids.len()
        ))];
        let detail = crate::history::wrap_styled_lines(app.modal.rows.clone(), width);
        let start = app.modal.scroll.min(detail.len().saturating_sub(1));
        if detail.is_empty() {
            rows.push(Line::from("Loading retained Action files…"));
        }
        rows.extend(
            detail
                .into_iter()
                .skip(start)
                .take(area.height.saturating_sub(1) as usize),
        );
        frame.render_widget(Paragraph::new(rows), area);
        return;
    }
    let mut lines = history_lines(app, width);
    let history_height = wrapped_line_count(&lines, width);
    let (mut live_lines, prompt_start) = live_region_lines_at(app, width, now_ms);
    let menu_start = prompt_start.map(|start| start + app.composer_lines().len());
    let menu = menu_start
        .map(|start| live_lines.split_off(start))
        .unwrap_or_default();
    let prompt_start =
        prompt_start.map(|index| history_height + wrapped_line_count(&live_lines[..index], width));
    lines.extend(live_lines);

    // Menu growth never changes the base input viewport or its scroll offset.
    let base_height = base_region_height(app, width)
        .saturating_sub(usize::from(app.form.is_none()))
        .saturating_add(history_height)
        .min(
            area.height
                .saturating_sub(u16::from(app.form.is_none() && area.height >= 3))
                as usize,
        ) as u16;
    let base_area = Rect::new(area.x, area.y, area.width, base_height);
    let cursor_position = prompt_start.map(|start| composer_cursor_position(app, width, start));
    let scroll_y = cursor_position.map(|(_, y)| y.saturating_add(1).saturating_sub(base_height));
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(
        paragraph.scroll((scroll_y.unwrap_or_default(), 0)),
        base_area,
    );
    let menu_area = Rect::new(
        area.x,
        area.y + base_height,
        area.width,
        area.height - base_height,
    );
    if menu_area.height > 0 {
        let selected = app.completion.as_ref().map_or(0, |state| state.selected);
        let window_start = selected.saturating_sub(MAX_COMPLETION_ROWS - 1);
        let selected_offset = selected - window_start;
        let selected_row = crate::history::wrap_styled_lines(
            menu[..selected_offset.min(menu.len())].to_vec(),
            width,
        )
        .len();
        let scroll = if selected_row >= menu_area.height as usize {
            selected_row
        } else {
            0
        };
        frame.render_widget(
            Paragraph::new(crate::history::wrap_styled_lines(menu, width))
                .scroll((scroll as u16, 0)),
            menu_area,
        );
    }
    if let Some((x, y)) = cursor_position
        && area.height > 0
    {
        let scroll_y = scroll_y.unwrap_or_default();
        frame.set_cursor_position((
            area.x + x.min(area.width.saturating_sub(1)),
            area.y + y.saturating_sub(scroll_y).min(area.height - 1),
        ));
    }
}

fn history_lines(app: &FileBackedApp, width: usize) -> Vec<Line<'static>> {
    app.styled_history_lines(width)
}

#[cfg(test)]
#[path = "history_notice_tests.rs"]
mod history_notice_tests;

fn live_region_lines(app: &FileBackedApp, width: usize) -> (Vec<Line<'static>>, Option<usize>) {
    live_region_lines_at(app, width, super::unix_time_ms())
}

fn live_region_lines_at(
    app: &FileBackedApp,
    width: usize,
    now_ms: u64,
) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = Vec::new();
    lines.push(app.context_line(width));
    if app.activity_label().is_some() {
        lines.push(activity_line(app, now_ms));
    }
    if let Some(notice) = app.composer.history_notice() {
        lines.push(Line::styled(
            format!("· {notice}"),
            Style::default().fg(Color::Yellow),
        ));
    }
    if let Some(notice) = &app.notice {
        lines.push(Line::styled(
            format!("· {notice}"),
            Style::default().fg(Color::Yellow),
        ));
    }
    for tool in &app.running_tools {
        lines.push(Line::styled(
            format!("{} · running", tool.title),
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
        let prompt_start = lines.len();
        lines.extend(app.composer_lines());
        if let Some(state) = &app.completion {
            let start = state.selected.saturating_sub(MAX_COMPLETION_ROWS - 1);
            for (idx, candidate) in state
                .matches
                .iter()
                .enumerate()
                .skip(start)
                .take(MAX_COMPLETION_ROWS)
            {
                let trigger = if app.model_chooser.active {
                    ""
                } else {
                    match state.kind {
                        CompletionKind::Command => "/",
                        CompletionKind::Skill => "$",
                        CompletionKind::File => "@",
                    }
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
                let prefix = if idx == state.selected { "▶ " } else { "  " };
                lines.push(Line::styled(format!("{prefix}{label}"), style));
            }
        }
        if app.completion.is_none()
            && app.project_selection.is_none()
            && app.pending_yield.is_none()
            && !app.projected_actions.is_empty()
        {
            // Reuse the stable row below the composer; completion keeps priority.
            lines.push(Line::styled(
                "Ctrl+O details",
                Style::default().fg(Color::DarkGray),
            ));
        }
        (lines, Some(prompt_start))
    }
}

pub(super) fn history_prefix_to_drain(
    lines: &[Line<'static>],
    width: usize,
    max_retained_height: usize,
) -> usize {
    let mut retained_count = 0;
    let mut retained_height = 0;
    for line in lines.iter().rev() {
        let height = wrapped_line_count(std::slice::from_ref(line), width);
        if retained_height + height > max_retained_height {
            break;
        }
        retained_height += height;
        retained_count += 1;
    }
    lines.len() - retained_count
}

pub(super) fn live_region_height(app: &FileBackedApp, width: usize) -> u16 {
    let (lines, start) = live_region_lines(app, width);
    let menu_height = start.map_or(0, |start| {
        wrapped_line_count(&lines[start + app.composer_lines().len()..], width)
    });
    base_region_height(app, width)
        .saturating_add(menu_height.saturating_sub(usize::from(start.is_some())))
        .min(u16::MAX as usize) as u16
}

pub(super) fn base_region_height(app: &FileBackedApp, width: usize) -> usize {
    let (lines, prompt_start) = live_region_lines(app, width);
    let Some(prompt_start) = prompt_start else {
        return wrapped_line_count(&lines, width).max(1);
    };

    let before_prompt = wrapped_line_count(&lines[..prompt_start], width);
    let rendered_composer_height = wrapped_line_count(&app.composer_lines(), width);
    let cursor_height = (composer_cursor_position(app, width, before_prompt)
        .1
        .saturating_add(1) as usize)
        .saturating_sub(before_prompt);
    // One ordinary blank spacing row is stable from first readiness. Menus
    // reuse it; retention and viewport sizing share this same budget.
    (before_prompt
        + rendered_composer_height
            .max(cursor_height)
            .min(MAX_COMPOSER_LINES)
        + 1)
    .max(1)
}

pub(super) fn base_viewport_height(
    app: &FileBackedApp,
    width: usize,
    terminal_height: usize,
) -> u16 {
    // Details are stable full-screen content, never transient candidates.
    if app.modal.active {
        return terminal_height.max(1).min(u16::MAX as usize) as u16;
    }
    wrapped_line_count(&history_lines(app, width), width)
        .saturating_add(base_region_height(app, width))
        .max(1)
        .min(terminal_height.max(1)) as u16
}

pub(super) fn inline_viewport_height(
    app: &FileBackedApp,
    width: usize,
    terminal_height: usize,
) -> u16 {
    if app.modal.active {
        return terminal_height.max(1).min(u16::MAX as usize) as u16;
    }
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
    // Ratatui omits control graphemes. Keep offsets in the original input,
    // rather than accumulating lengths in that filtered display projection.
    let source_start = |symbol: &str| symbol.as_ptr() as usize - span.content.as_ptr() as usize;
    let marker_index = graphemes
        .iter()
        .position(|grapheme| cursor < source_start(grapheme.symbol) + grapheme.symbol.len())
        .or_else(|| graphemes.len().checked_sub(1))?;
    let marker = &graphemes[marker_index];
    let marker_start = source_start(marker.symbol);
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

pub(super) fn frame_needs_redraw(
    dirty: bool,
    app: &FileBackedApp,
    now_ms: u64,
    last_drawn_second: Option<u64>,
) -> bool {
    // Only Running work without actual user controls advances quietly; dirty events always draw.
    dirty
        || (matches!(app.activity.state, super::UiActivityState::Running)
            && app.pending_yield.is_none()
            && app.form.is_none()
            && last_drawn_second != Some(activity_elapsed_second(app, now_ms)))
}

pub(super) fn activity_elapsed_second(app: &FileBackedApp, now_ms: u64) -> u64 {
    app.activity_started_at_ms()
        .map(|started_at_ms| now_ms.saturating_sub(started_at_ms) / 1_000)
        .unwrap_or(0)
}

fn activity_line(app: &FileBackedApp, now_ms: u64) -> Line<'static> {
    if app.pending_yield.is_some()
        || app.form.is_some()
        || !matches!(app.activity.state, super::UiActivityState::Running)
    {
        let cue = if app.form.is_some() {
            "input form"
        } else if app.pending_yield.as_ref().is_some_and(|pending| {
            matches!(pending.kind, alan_agent_protocol::YieldKind::Confirmation)
        }) {
            "waiting for approval"
        } else {
            app.activity_label().unwrap_or("waiting for input")
        };
        return Line::styled(format!("· {cue}"), Style::default().fg(Color::Yellow));
    }
    let elapsed = activity_elapsed_second(app, now_ms);
    let frame_idx = (elapsed as usize) % SPINNER.len();
    Line::from(vec![
        Span::styled(
            format!("{} ", SPINNER[frame_idx]),
            Style::default().fg(Color::Green),
        ),
        Span::styled(
            format!("· ctrl+c/esc interrupt · {elapsed}s"),
            Style::default(),
        ),
    ])
}
