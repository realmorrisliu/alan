use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::completion::CompletionKind;
use crate::theme::DotState;
use crate::transcript_ui::wrapped_line_count;

use super::app::action_modal::ModalMode;
use super::{FileBackedApp, MAX_COMPLETION_ROWS, MAX_COMPOSER_LINES, SPINNER};

#[cfg(test)]
pub(super) fn draw(frame: &mut Frame<'_>, app: &FileBackedApp) {
    draw_at(frame, app, super::unix_time_ms());
}

pub(super) fn draw_at(frame: &mut Frame<'_>, app: &FileBackedApp, now_ms: u64) {
    let area = frame.area();
    let width = area.width as usize;
    if app.modal.active {
        let header_line = match app.modal.mode {
            ModalMode::Actions => {
                let count = app.modal.actions.len();
                let owner = app
                    .modal
                    .actions
                    .get(app.modal.selected)
                    .map(|entry| entry.owner.as_str())
                    .unwrap_or(&app.modal.owner_path);
                let idx = if count == 0 {
                    0
                } else {
                    app.modal.selected + 1
                };
                Line::styled(
                    format!("{idx}/{count} · Action · {owner}"),
                    app.theme.accent_bold(),
                )
            }
            ModalMode::Plans => {
                let count = app.modal.plans.len();
                let owner = app
                    .modal
                    .plans
                    .get(app.modal.selected)
                    .map(|entry| entry.owner.as_str())
                    .unwrap_or(&app.modal.owner_path);
                let idx = if count == 0 {
                    0
                } else {
                    app.modal.selected + 1
                };
                Line::styled(
                    format!("{idx}/{count} · Plan · {owner}"),
                    app.theme.accent_bold(),
                )
            }
            ModalMode::Tape => Line::styled(
                format!("Turing Tape · {}", app.modal.owner_path),
                app.theme.accent_bold(),
            ),
            ModalMode::Man => Line::styled(
                format!("Manual · {}", app.modal.owner_path),
                app.theme.accent_bold(),
            ),
        };
        let switch = match (app.modal.mode, width >= 56, width >= 48) {
            (ModalMode::Actions, true, _) => " · p Plans · t Tape",
            (ModalMode::Actions, false, true) => " · p Plans",
            (ModalMode::Plans, true, _) => " · p Actions · t Tape",
            (ModalMode::Plans, false, true) => " · p Actions",
            (ModalMode::Tape, _, true) => " · t Actions · p Plans",
            (ModalMode::Man, true, _) => " · a Actions · p Plans · t Tape",
            (ModalMode::Man, false, true) => " · a Actions",
            _ => "",
        };
        let hint_text = if width >= 56 {
            format!("↑↓/jk · ↔ select · d · Space/b page · Esc{switch}")
        } else if width >= 48 {
            format!("↑↓/↔ · d · Space/b page · Esc{switch}")
        } else if width >= 32 {
            "↑↓/↔ · d · Space/b page · Esc".into()
        } else {
            "↑↓ · ↔ · d · Esc".into()
        };
        let mut rows = vec![
            header_line,
            Line::styled(hint_text, app.theme.muted_style()),
        ];
        if app.modal.mode == ModalMode::Actions && !app.running_tools.is_empty() {
            let active_list: Vec<String> = app
                .running_tools
                .iter()
                .map(|tool| format!("{} · running", crate::history::clean_text(&tool.title)))
                .collect();
            rows.push(Line::styled(
                format!("Active · {}", active_list.join(" | ")),
                app.theme.accent_style(),
            ));
        }
        let header_rows = rows.len();
        let display_rows = if app.modal.mode == ModalMode::Actions {
            filter_modal_rows(&app.modal.rows, app.modal.show_raw_json, app.theme)
        } else {
            app.modal.rows.clone()
        };
        let detail = crate::history::wrap_styled_lines(display_rows, width);
        let start = app.modal.scroll.min(detail.len().saturating_sub(1));
        if detail.is_empty() {
            rows.push(Line::styled(
                "Loading retained details…",
                app.theme.muted_style(),
            ));
        }
        rows.extend(
            detail
                .into_iter()
                .skip(start)
                .take(area.height.saturating_sub(header_rows as u16) as usize),
        );
        frame.render_widget(Paragraph::new(rows), area);
        return;
    }
    if app.resume_picker.active {
        super::resume_picker::draw(frame, app);
        return;
    }
    if app.issue.is_active() {
        super::issue_flow::draw(frame, app, now_ms);
        return;
    }
    let mut lines = history_lines(app, width);
    let history_height = wrapped_line_count(&lines, width);
    let (mut live_lines, prompt_start) =
        live_region_lines_in(app, width, Some(area.height.max(1) as usize), now_ms);
    let menu_start = prompt_start.map(|start| start + app.composer_lines().len());
    let menu = menu_start
        .map(|start| live_lines.split_off(start))
        .unwrap_or_default();
    let prompt_start =
        prompt_start.map(|index| history_height + wrapped_line_count(&live_lines[..index], width));
    let focused_field = app.form.as_ref().and_then(|form| {
        let fields = form.render_lines();
        let focus = fields.iter().position(|(_, focused)| *focused)?;
        let start = live_lines.len().checked_sub(fields.len())? + focus;
        Some((
            history_height + wrapped_line_count(&live_lines[..start], width),
            wrapped_line_count(&live_lines[start..=start], width),
            wrapped_line_count(&[Line::from(form.focused_input_line()?)], width),
        ))
    });
    let conf_cursor = app.confirmation.as_ref().and_then(|conf| {
        confirmation_cursor_position(
            conf,
            app.theme,
            width,
            app.response_in_flight.is_some(),
            history_height,
            &live_lines,
        )
    });
    let overlay_active = app.form.is_some() || app.confirmation.is_some();
    // Menu growth never changes the base input viewport or its scroll offset.
    let base_height = base_region_height(app, width)
        .saturating_sub(usize::from(!overlay_active))
        .saturating_add(history_height)
        .min(
            area.height
                .saturating_sub(u16::from(!overlay_active && area.height >= 3))
                as usize,
        ) as u16;
    let conf_scroll_y =
        app.confirmation_scroll_y(&live_lines, history_height, width, base_height as usize);
    let conf_end_y = app
        .confirmation
        .as_ref()
        .map(|_| history_height + wrapped_line_count(&live_lines, width));
    lines.extend(live_lines);

    let base_area = Rect::new(area.x, area.y, area.width, base_height);
    let cursor_position = prompt_start
        .map(|start| composer_cursor_position(app, width, start))
        .or(conf_cursor);
    let scroll_y = conf_cursor
        .map(|(_, y)| {
            let bottom = conf_end_y
                .map(|end| end.min(u16::MAX as usize) as u16)
                .unwrap_or(u16::MAX);
            y.saturating_add(2)
                .min(bottom)
                .saturating_sub(base_height)
                .min(y)
        })
        .or(conf_scroll_y)
        .or_else(|| cursor_position.map(|(_, y)| y.saturating_add(1).saturating_sub(base_height)))
        .or_else(|| {
            // Forms have no composer cursor. Keep the focused field visible,
            // scrolling decoration/history away before response controls.
            focused_field.map(|(start, height, input_height)| {
                let end = start
                    + if height > base_height as usize {
                        input_height
                    } else {
                        height
                    };
                end.saturating_sub(base_height as usize)
                    .min(u16::MAX as usize) as u16
            })
        });
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CollapsedSection {
    None,
    RawBytes,
    ResultMetadata,
}

fn filter_modal_rows(
    rows: &[Line<'static>],
    show_raw_json: bool,
    theme: crate::theme::Theme,
) -> Vec<Line<'static>> {
    if show_raw_json {
        return rows.to_vec();
    }
    let mut filtered = Vec::with_capacity(rows.len());
    let mut collapsed = CollapsedSection::None;

    for row in rows {
        let text = row.to_string();
        if text == "Result metadata" {
            if collapsed != CollapsedSection::ResultMetadata {
                filtered.push(Line::styled(
                    "  [result metadata collapsed · press d to expand]",
                    theme.muted_style(),
                ));
                collapsed = CollapsedSection::ResultMetadata;
            }
            continue;
        }
        if text == "Original raw bytes" {
            if collapsed != CollapsedSection::RawBytes {
                filtered.push(Line::styled(
                    "  [raw JSON/bytes collapsed · press d to expand]",
                    theme.muted_style(),
                ));
                collapsed = CollapsedSection::RawBytes;
            }
            continue;
        }
        if matches!(
            text.as_str(),
            "Name" | "Status" | "Original output" | "Recovered reference (retained original)"
        ) {
            collapsed = CollapsedSection::None;
        }
        if collapsed != CollapsedSection::None {
            continue;
        }
        filtered.push(row.clone());
    }
    filtered
}

fn live_region_lines(app: &FileBackedApp, width: usize) -> (Vec<Line<'static>>, Option<usize>) {
    live_region_lines_at(app, width, super::unix_time_ms())
}

pub(super) fn live_region_lines_at(
    app: &FileBackedApp,
    width: usize,
    now_ms: u64,
) -> (Vec<Line<'static>>, Option<usize>) {
    live_region_lines_in(
        app,
        width,
        Some(app.viewport_size.1.max(1) as usize),
        now_ms,
    )
}

fn live_region_lines_in(
    app: &FileBackedApp,
    width: usize,
    viewport_height: Option<usize>,
    now_ms: u64,
) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = Vec::new();
    // Keep the live composer visually separate from retained output. These
    // rows share the ordinary viewport budget, including cursor placement.
    lines.push(Line::default());
    let rule_char = if app.theme.is_dumb { "-" } else { "─" };
    lines.push(Line::styled(
        rule_char.repeat(width),
        app.theme.rule_style(),
    ));
    lines.push(app.context_line(width));
    if let Some(line) = attention_line_at_width(app, now_ms, width) {
        lines.push(line);
    }
    if matches!(app.activity.state, super::UiActivityState::Running)
        && !app.has_pending_response()
        && matches!(
            app.thinking.state,
            alan_agent_protocol::UiThinkingState::Streaming
        )
    {
        lines.extend(thinking_preview_lines(app, width));
    }
    if let Some(notice) = app.composer.history_notice() {
        let (dot, _) = app.theme.status_dot(DotState::Wait);
        lines.push(Line::styled(
            format!("{dot} {notice}"),
            app.theme.wait_style(),
        ));
    }
    if let Some(notice) = &app.notice {
        if let Some(help_lines) = super::help::try_render_help_lines(notice, app.theme, width) {
            lines.extend(help_lines);
        } else {
            let (prefix, style) = match notice.kind {
                alan_agent_protocol::UiNoticeKind::Error => ("Error · ", app.theme.block_style()),
                alan_agent_protocol::UiNoticeKind::Warning => {
                    ("Warning · ", app.theme.wait_style())
                }
                _ => ("· ", app.theme.muted_style()),
            };
            let mut iter = notice.split('\n');
            if let Some(first) = iter.next() {
                lines.push(Line::styled(format!("{prefix}{first}"), style));
            }
            for continuation in iter {
                lines.push(Line::styled(format!("  {continuation}"), style));
            }
        }
    }
    for tool in &app.running_tools {
        let (dot, dot_style) = app.theme.status_dot(DotState::Run);
        lines.push(Line::from(vec![
            Span::styled(format!("{dot} "), dot_style),
            Span::styled(tool.title.clone(), app.theme.accent_style()),
            Span::styled(" · running", app.theme.muted_style()),
        ]));
        for tail in &tool.tail_lines {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}", app.theme.gutter()), app.theme.rule_style()),
                Span::styled(tail.clone(), app.theme.muted_style()),
            ]));
        }
    }

    if let Some(form) = &app.form {
        for (text, focused) in form.render_lines() {
            let style = if focused {
                app.theme.accent_bold()
            } else if text.trim_start().starts_with('!') {
                app.theme.block_style()
            } else {
                Style::default()
            };
            lines.push(Line::styled(text, style));
        }
        (lines, None)
    } else if let Some(conf_lines) = app.confirmation_live_lines(width, viewport_height) {
        lines.extend(conf_lines);
        (lines, None)
    } else {
        if !app.login.is_entering_input() && app.project_selection.is_none() {
            lines.extend(queued_preview_lines(app, width));
        }
        let prompt_start = lines.len();
        lines.extend(app.composer_lines());
        if let Some(state) = &app.completion {
            let start = state.selected.saturating_sub(MAX_COMPLETION_ROWS - 1);
            let visible: Vec<(usize, &crate::completion::CompletionCandidate)> = state
                .matches
                .iter()
                .enumerate()
                .skip(start)
                .take(MAX_COMPLETION_ROWS)
                .collect();
            let trigger = if app.model_chooser.active
                || app.login.is_active()
                || app.settings.is_active()
                || app.fork_picker.active
            {
                ""
            } else {
                match state.kind {
                    CompletionKind::Command => "/",
                    CompletionKind::Skill => "$",
                    CompletionKind::File => "@",
                }
            };
            let has_details = visible.iter().any(|(_, c)| c.detail.is_some());
            let max_left = width.saturating_sub(14).clamp(10, 32);
            let left_col_width = if has_details {
                visible
                    .iter()
                    .map(|(_, c)| {
                        unicode_width::UnicodeWidthStr::width(trigger)
                            + unicode_width::UnicodeWidthStr::width(c.label.as_str())
                    })
                    .max()
                    .unwrap_or(0)
                    .min(max_left)
            } else {
                0
            };
            let visible_len = visible.len();
            for (idx, candidate) in visible {
                let selected = idx == state.selected;
                let (prefix_style, left_style, detail_style) =
                    app.theme.completion_row_styles(selected);
                let prefix = if selected {
                    if app.theme.is_dumb { "> " } else { "▶ " }
                } else {
                    "  "
                };
                let left_raw = format!("{trigger}{}", candidate.label);
                let mut spans = vec![Span::styled(prefix, prefix_style)];
                if let Some(detail) = &candidate.detail {
                    let left_w = unicode_width::UnicodeWidthStr::width(left_raw.as_str());
                    let pad = left_col_width.saturating_sub(left_w) + 2;
                    spans.push(Span::styled(left_raw, left_style));
                    spans.push(Span::styled(" ".repeat(pad), prefix_style));
                    spans.push(Span::styled(detail.clone(), detail_style));
                } else {
                    spans.push(Span::styled(left_raw, left_style));
                }
                lines.push(Line::from(spans));
            }
            let total = state.matches.len();
            if total > MAX_COMPLETION_ROWS {
                let end = start + visible_len;
                lines.push(Line::styled(
                    format!("  ({}-{}/{total} · ↑↓)", start + 1, end),
                    app.theme.muted_style(),
                ));
            }
        }
        if app.completion.is_none()
            && app.project_selection.is_none()
            && app.pending_yield.is_none()
            && !app.login.is_entering_input()
        {
            // Reuse the stable row below the composer; completion keeps priority.
            if matches!(app.activity.state, super::UiActivityState::Running) {
                let hint = if width >= 52 {
                    "Enter queue next · Ctrl+S steer now · Esc interrupt"
                } else if width >= 38 {
                    "Enter queue · Ctrl+S steer · Esc stop"
                } else {
                    "Ctrl+S steer · Esc stop"
                };
                lines.push(Line::styled(
                    crate::history::clamp_text_width(hint, width),
                    app.theme.muted_style(),
                ));
            } else if !app.projected_actions.is_empty() || !app.plan_owners.is_empty() {
                let hint = if width >= 40 {
                    "Ctrl+O details (p: plans, t: tape)"
                } else {
                    "Ctrl+O details (p: plans)"
                };
                lines.push(Line::styled(
                    crate::history::clamp_text_width(hint, width),
                    app.theme.muted_style(),
                ));
            }
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
    if app.modal.active || app.resume_picker.active || app.issue.is_active() {
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
    if app.modal.active || app.resume_picker.active || app.issue.is_active() {
        return terminal_height.max(1).min(u16::MAX as usize) as u16;
    }
    wrapped_line_count(&history_lines(app, width), width)
        .saturating_add(live_region_height(app, width) as usize)
        .max(1)
        .min(terminal_height.max(1)) as u16
}

fn composer_cursor_position(app: &FileBackedApp, width: usize, prompt_start: usize) -> (u16, u16) {
    let (text, cursor) = if let Some((buf, cur, masked)) = app.login.input_display() {
        if masked {
            let display = "*".repeat(buf.chars().count());
            let mapped = buf[..cur.min(buf.len())].chars().count();
            (display, mapped)
        } else {
            (buf.to_string(), cur.min(buf.len()))
        }
    } else {
        let text = app.composer.text().to_string();
        let cursor = app.composer.cursor().min(text.len());
        (text, cursor)
    };
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
    let line = composer_lines.get(line_index).cloned().unwrap_or_default();
    line_cursor_position(line, cursor_in_line, width, prompt_start + preceding_lines)
}

fn confirmation_cursor_position(
    conf: &super::confirmation::ConfirmationState,
    theme: crate::theme::Theme,
    width: usize,
    in_flight: bool,
    history_height: usize,
    live_lines: &[Line<'static>],
) -> Option<(u16, u16)> {
    let rendered = conf.render(theme, width, in_flight);
    let rel_idx = rendered.note_line_index?;
    let conf_start = live_lines.len().checked_sub(rendered.lines.len())?;
    let line_index = conf_start + rel_idx;
    let line = live_lines.get(line_index)?.clone();
    let preceding = history_height + wrapped_line_count(&live_lines[..line_index], width);
    Some(line_cursor_position(
        line,
        rendered.note_cursor_bytes,
        width,
        preceding,
    ))
}

fn line_cursor_position(
    mut line: Line<'static>,
    cursor: usize,
    width: usize,
    line_start_y: usize,
) -> (u16, u16) {
    let cursor_column = mark_cursor_in_line(&mut line, cursor).unwrap_or_default();
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
    let y = line_start_y + marker_position.1 + row_offset;
    (x as u16, y.min(u16::MAX as usize) as u16)
}

fn queued_preview_lines(app: &FileBackedApp, width: usize) -> Vec<Line<'static>> {
    let Some(queue) = app.queue.snapshot.as_ref().filter(|q| q.known) else {
        return Vec::new();
    };
    if queue.pending_submission_ids.is_empty() {
        return Vec::new();
    }
    let arrow = if app.theme.is_dumb { "> " } else { "↳ " };
    let prefix = format!("  {arrow}Queued: ");
    let prefix_width = unicode_width::UnicodeWidthStr::width(prefix.as_str());
    let body_budget = width.saturating_sub(prefix_width).max(4);
    let mut lines = Vec::new();
    for id in queue.pending_submission_ids.iter().take(2) {
        let preview_raw = app
            .local_inputs
            .get(id)
            .filter(|input| !input.body.trim().is_empty())
            .map(|input| {
                input
                    .body
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| "queued input".to_string());
        let cleaned = crate::history::clean_text(&preview_raw);
        let truncated = crate::history::clamp_text_width(&cleaned, body_budget);
        lines.push(Line::from(vec![
            Span::styled(prefix.clone(), app.theme.muted_style()),
            Span::styled(truncated, app.theme.muted_style()),
        ]));
    }
    let extra = queue.pending_submission_ids.len().saturating_sub(2);
    if extra > 0 {
        lines.push(Line::styled(
            format!("  +{extra} more queued"),
            app.theme.muted_style(),
        ));
    }
    lines
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
    // Only Running work without a pending response advances quietly; dirty events always draw.
    dirty
        || app.issue.is_drafting()
        || (matches!(app.activity.state, super::UiActivityState::Running)
            && !app.has_pending_response()
            && last_drawn_second != Some(activity_elapsed_second(app, now_ms)))
}

pub(super) fn activity_elapsed_second(app: &FileBackedApp, now_ms: u64) -> u64 {
    app.activity_started_at_ms()
        .map(|started_at_ms| now_ms.saturating_sub(started_at_ms) / 1_000)
        .unwrap_or(0)
}

fn attention_line_at_width(
    app: &FileBackedApp,
    now_ms: u64,
    width: usize,
) -> Option<Line<'static>> {
    let line = attention_line(app, now_ms)?;
    if line.width() <= width {
        return Some(line);
    }
    // Required responses and execution controls outrank background metadata.
    // Use at most two physical rows so an urgent cue cannot evict its action.
    let queue = app.queue.snapshot.as_ref().filter(|q| q.known);
    let primary = if app.form.is_some() {
        "Input required"
    } else if let Some(pending) = &app.pending_yield {
        if matches!(pending.kind, alan_agent_protocol::YieldKind::Confirmation) {
            "Approval required"
        } else {
            "Input required"
        }
    } else if app.has_pending_response() {
        "Input required"
    } else if app.project_selection.is_some() {
        "Select project"
    } else if app.activity.state == super::UiActivityState::Running {
        "Run Esc stop"
    } else if app.activity.state == super::UiActivityState::Paused
        || queue.is_some_and(|q| q.paused)
    {
        "Paused /continue"
    } else if app.model_chooser.uncertain.is_some() || app.model_chooser.pending.is_some() {
        "Model /status"
    } else if app.last_input_failed {
        "Failed /status"
    } else if !app.queue.owner.is_empty() && !app.queue.is_fresh_unknown() {
        "Queue /status"
    } else {
        "Context /status"
    };
    let secondary = if app.model_chooser.uncertain.is_some() {
        "model uncertain".into()
    } else if app.model_chooser.pending.is_some() {
        "model pending".into()
    } else {
        match queue {
            Some(q) if !q.uncertain_submission_ids.is_empty() => "uncertain".into(),
            Some(q) if q.paused && !app.has_pending_response() && primary != "Paused /continue" => {
                "paused".into()
            }
            Some(q) if q.deferred => "deferred".into(),
            Some(q) if !q.pending_submission_ids.is_empty() => {
                format!("{} queued", q.pending_submission_ids.len())
            }
            None if !app.queue.owner.is_empty() && !app.queue.is_fresh_unknown() => {
                "q unknown".into()
            }
            _ => String::new(),
        }
    };
    let combined = if secondary.is_empty() {
        primary.to_string()
    } else {
        format!("{primary} {secondary}")
    };
    let text = if wrapped_line_count(&[Line::from(combined.as_str())], width) <= 2 {
        combined
    } else if wrapped_line_count(&[Line::from(primary)], width) <= 2 {
        primary.into()
    } else {
        // A pane too small even for the primary control still keeps its input usable.
        primary.chars().take(width).collect()
    };
    Some(Line::styled(
        text,
        if app.model_chooser.uncertain.is_some() {
            app.theme.block_style()
        } else {
            line.style
        },
    ))
}

pub(super) fn attention_line(app: &FileBackedApp, now_ms: u64) -> Option<Line<'static>> {
    let labels = app.attention_labels();
    if app.activity_label().is_some() {
        let mut line = activity_line(app, now_ms);
        if !labels.is_empty() {
            let suffix_style = if app.model_chooser.uncertain.is_some() {
                app.theme.block_style()
            } else {
                app.theme.wait_style()
            };
            line.spans.push(Span::styled(
                format!(" · {}", labels.join(" · ")),
                suffix_style,
            ));
        }
        Some(line)
    } else if labels.is_empty() {
        None
    } else {
        let (dot_state, style) = if app.last_input_failed || app.model_chooser.uncertain.is_some() {
            (DotState::Block, app.theme.block_style())
        } else if app.model_chooser.pending.is_some() || app.project_selection.is_some() {
            (DotState::Wait, app.theme.accent_style())
        } else {
            (DotState::Wait, app.theme.wait_style())
        };
        let (dot, _) = app.theme.status_dot(dot_state);
        Some(Line::styled(format!("{dot} {}", labels.join(" · ")), style))
    }
}

fn activity_line(app: &FileBackedApp, now_ms: u64) -> Line<'static> {
    if app.has_pending_response() || !matches!(app.activity.state, super::UiActivityState::Running)
    {
        let cue = app.activity_label().unwrap_or("Input required");
        let (dot, style) = app.theme.status_dot(DotState::Wait);
        return Line::styled(format!("{dot} {cue}"), style);
    }
    let elapsed = activity_elapsed_second(app, now_ms);
    let frame_idx = (elapsed as usize) % SPINNER.len();
    let verb = if matches!(
        app.thinking.state,
        alan_agent_protocol::UiThinkingState::Streaming
    ) {
        "Thinking"
    } else {
        "Working"
    };
    Line::from(vec![
        Span::styled(format!("{} ", SPINNER[frame_idx]), app.theme.run_style()),
        Span::styled(
            format!("{verb} · {elapsed}s · Esc interrupt"),
            Style::default(),
        ),
    ])
}

fn thinking_preview_lines(app: &FileBackedApp, width: usize) -> Vec<Line<'static>> {
    let cleaned = crate::history::clean_text(&app.thinking.text);
    let body_width = width.saturating_sub(4).max(1);
    let mut wrapped_rows = Vec::new();
    for line in cleaned.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        for row in textwrap::wrap(trimmed, body_width) {
            wrapped_rows.push(row.into_owned());
        }
    }
    if wrapped_rows.is_empty() {
        return Vec::new();
    }
    let max_rows = if app.expand_thinking { 8 } else { 2 };
    let start = wrapped_rows.len().saturating_sub(max_rows);
    let gutter = format!("  {}", app.theme.gutter());
    let text_style = app
        .theme
        .italic_if_capable(app.theme.dim_if_capable(app.theme.muted_style()));
    wrapped_rows
        .into_iter()
        .skip(start)
        .map(|row| {
            Line::from(vec![
                Span::styled(gutter.clone(), app.theme.rule_style()),
                Span::styled(row, text_style),
            ])
        })
        .collect()
}
