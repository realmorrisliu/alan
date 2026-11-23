//! Bottom live-region confirmation selector and settled transcript receipt helpers.

use alan_agent_protocol::{ToolResultPresentation, YieldKind};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::history::{
    ConfirmationOutcome, HistoryCell, PendingYieldCell, clamp_text_width, clean_text,
    metadata_style, presentation_rows_with_theme,
};
use crate::theme::Theme;

use super::app::form_control::ConfirmationKeyResult;
use super::app::{FileBackedAction, FileBackedApp};
use super::file_surface::response_text_from_content;

const CONFIRMATION_PREVIEW_MAX_LINES: usize = 8;
const CONFIRMATION_PROMPT_MAX_LINES: usize = 3;
pub(in crate::file_backed) const REJECT_WITH_NOTE_LABEL: &str = "Reject with note…";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::file_backed) struct ConfirmationOutcomeDraft {
    pub request_id: String,
    pub outcome: ConfirmationOutcome,
    pub choice: String,
    pub summary: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::file_backed) struct ConfirmationState {
    pub pending: PendingYieldCell,
    pub selected: usize,
    pub has_reject_with_note: bool,
    pub editing_note: bool,
    pub note: String,
    pub note_cursor: usize,
    pub scroll_from_bottom: usize,
    pub error: Option<String>,
}

pub(in crate::file_backed) struct ConfirmationRender {
    pub lines: Vec<Line<'static>>,
    pub note_line_index: Option<usize>,
    pub note_cursor_bytes: usize,
}

impl ConfirmationState {
    pub fn new(pending: PendingYieldCell) -> Self {
        let selected = pending
            .default_option
            .as_deref()
            .and_then(|def| {
                pending
                    .options
                    .iter()
                    .position(|opt| opt.eq_ignore_ascii_case(def))
            })
            .unwrap_or(0);
        let has_reject_with_note = pending
            .options
            .iter()
            .any(|opt| opt.eq_ignore_ascii_case("reject"));
        Self {
            pending,
            selected,
            has_reject_with_note,
            editing_note: false,
            note: String::new(),
            note_cursor: 0,
            scroll_from_bottom: 0,
            error: None,
        }
    }

    pub fn sync(&mut self, pending: PendingYieldCell) {
        if self.pending.request_id != pending.request_id
            || self.pending.options != pending.options
            || self.pending.default_option != pending.default_option
        {
            *self = Self::new(pending);
        } else {
            self.pending = pending;
        }
    }

    pub fn total_items(&self) -> usize {
        self.pending.options.len() + usize::from(self.has_reject_with_note)
    }

    pub fn selected_is_reject_with_note(&self) -> bool {
        self.has_reject_with_note && self.selected == self.pending.options.len()
    }

    pub fn move_up(&mut self) {
        let total = self.total_items();
        if total > 0 {
            self.selected = (self.selected + total - 1) % total;
        }
        self.scroll_from_bottom = 0;
    }

    pub fn move_down(&mut self) {
        let total = self.total_items();
        if total > 0 {
            self.selected = (self.selected + 1) % total;
        }
        self.scroll_from_bottom = 0;
    }

    pub fn enter_note_mode(&mut self) {
        if self.has_reject_with_note {
            self.selected = self.pending.options.len();
            self.editing_note = true;
            self.note_cursor = self.note.len();
            self.scroll_from_bottom = 0;
            self.error = None;
        }
    }

    pub fn exit_note_mode(&mut self) {
        self.editing_note = false;
        self.error = None;
    }

    pub fn insert_note_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let pos = self.note_cursor.min(self.note.len());
        self.note.insert(pos, ch);
        self.note_cursor = pos + ch.len_utf8();
        self.error = None;
    }

    pub fn insert_note_text(&mut self, text: &str) {
        let filtered: String = text.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }
        let pos = self.note_cursor.min(self.note.len());
        self.note.insert_str(pos, &filtered);
        self.note_cursor = pos + filtered.len();
        self.error = None;
    }

    pub fn backspace_note(&mut self) {
        let pos = self.note_cursor.min(self.note.len());
        if pos > 0 {
            let prev = self.note[..pos]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            self.note.drain(prev..pos);
            self.note_cursor = prev;
            self.error = None;
        }
    }

    pub fn delete_note_forward(&mut self) {
        let pos = self.note_cursor.min(self.note.len());
        if pos < self.note.len() {
            let len = self.note[pos..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(0);
            self.note.drain(pos..pos + len);
            self.error = None;
        }
    }

    pub fn move_note_left(&mut self) {
        let pos = self.note_cursor.min(self.note.len());
        if pos > 0 {
            self.note_cursor = self.note[..pos]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
        }
    }

    pub fn move_note_right(&mut self) {
        let pos = self.note_cursor.min(self.note.len());
        if pos < self.note.len() {
            let len = self.note[pos..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(0);
            self.note_cursor = (pos + len).min(self.note.len());
        }
    }

    pub fn move_note_home(&mut self) {
        self.note_cursor = 0;
    }

    pub fn move_note_end(&mut self) {
        self.note_cursor = self.note.len();
    }

    pub fn clear_note(&mut self) {
        self.note.clear();
        self.note_cursor = 0;
        self.error = None;
    }

    pub fn footer_hint(&self, width: usize, in_flight: bool, scrollable: bool) -> String {
        if in_flight {
            return clamp_text_width("Sending response…", width);
        }
        if self.editing_note {
            let full = "Enter submit rejection note · Esc back to options";
            let medium = "Enter submit · Esc back";
            return if UnicodeWidthStr::width(full) <= width {
                full.to_string()
            } else if UnicodeWidthStr::width(medium) <= width {
                medium.to_string()
            } else {
                clamp_text_width("Enter · Esc", width)
            };
        }
        let total = self.total_items();
        let full = if scrollable {
            format!("↑↓ / 1-{total} choose · PgUp/PgDn scroll · Enter confirm · Esc cancel")
        } else {
            format!("↑↓ / 1-{total} choose · Enter confirm · Esc cancel")
        };
        if UnicodeWidthStr::width(full.as_str()) <= width {
            full
        } else if scrollable && width >= 32 {
            format!("↑↓/1-{total} · PgUp/PgDn · Enter · Esc")
        } else if width >= 28 {
            format!("↑↓/1-{total} · Enter · Esc")
        } else {
            clamp_text_width(&format!("1-{total} Enter Esc"), width)
        }
    }

    pub fn render(&self, theme: Theme, width: usize, in_flight: bool) -> ConfirmationRender {
        let scrollable = false;
        let gutter = theme.gutter();
        let wait_gutter = theme.wait_style();
        let mut lines = Vec::new();

        let title_lines: Vec<&str> = self
            .pending
            .title
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.trim().is_empty())
            .collect();
        let head_title = title_lines.first().copied().unwrap_or("Approval required");
        lines.push(Line::from(vec![
            Span::styled(gutter, wait_gutter),
            Span::styled("Approval · ", theme.bold_if_capable(theme.wait_style())),
            Span::styled(clean_text(head_title), theme.accent_bold()),
        ]));
        for extra in title_lines.iter().skip(1).take(2) {
            lines.push(Line::from(vec![
                Span::styled(gutter, wait_gutter),
                Span::raw(clean_text(extra)),
            ]));
        }

        if let Some(prompt) = &self.pending.prompt
            && !prompt.trim().is_empty()
            && prompt.trim() != self.pending.title.trim()
        {
            let prompt_lines: Vec<&str> = prompt
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.trim().is_empty())
                .collect();
            for segment in prompt_lines.iter().take(CONFIRMATION_PROMPT_MAX_LINES) {
                lines.push(Line::from(vec![
                    Span::styled(gutter, wait_gutter),
                    Span::raw(clean_text(segment)),
                ]));
            }
            if prompt_lines.len() > CONFIRMATION_PROMPT_MAX_LINES {
                lines.push(Line::from(vec![
                    Span::styled(gutter, wait_gutter),
                    Span::styled(
                        format!(
                            "… +{} more lines",
                            prompt_lines.len() - CONFIRMATION_PROMPT_MAX_LINES
                        ),
                        metadata_style(theme),
                    ),
                ]));
            }
        }

        if let Some(capability) = &self.pending.capability {
            lines.push(Line::from(vec![
                Span::styled(gutter, wait_gutter),
                Span::styled(
                    format!("capability: {}", clean_text(capability)),
                    metadata_style(theme),
                ),
            ]));
        }
        if let Some(reason) = &self.pending.reason {
            lines.push(Line::from(vec![
                Span::styled(gutter, wait_gutter),
                Span::styled(format!("why: {}", clean_text(reason)), theme.wait_style()),
            ]));
        }

        if let Some(presentation) = &self.pending.presentation {
            let rows = presentation_rows_with_theme(presentation, false, theme);
            let total = rows.len();
            for (row_gutter_style, text, style) in
                rows.into_iter().take(CONFIRMATION_PREVIEW_MAX_LINES)
            {
                let bar_style = if row_gutter_style == Style::default() && !theme.is_dumb {
                    wait_gutter
                } else {
                    row_gutter_style
                };
                lines.push(Line::from(vec![
                    Span::styled(gutter, bar_style),
                    Span::styled(clean_text(&text), style),
                ]));
            }
            if total > CONFIRMATION_PREVIEW_MAX_LINES {
                lines.push(Line::from(vec![
                    Span::styled(gutter, wait_gutter),
                    Span::styled(
                        format!("… +{} more lines", total - CONFIRMATION_PREVIEW_MAX_LINES),
                        metadata_style(theme),
                    ),
                ]));
            }
        }

        let default_opt = self
            .pending
            .default_option
            .as_deref()
            .or_else(|| self.pending.options.first().map(String::as_str));
        for (idx, option) in self.pending.options.iter().enumerate() {
            let selected = idx == self.selected;
            let is_default = default_opt == Some(option.as_str());
            lines.push(render_option_line(
                idx, option, is_default, selected, theme, false,
            ));
        }
        if self.has_reject_with_note {
            let idx = self.pending.options.len();
            let selected = idx == self.selected;
            lines.push(render_option_line(
                idx,
                REJECT_WITH_NOTE_LABEL,
                false,
                selected,
                theme,
                true,
            ));
        }

        let mut note_line_index = None;
        if self.editing_note {
            note_line_index = Some(lines.len());
            lines.push(Line::from(vec![
                Span::styled("  Note: ", theme.accent_bold()),
                Span::raw(self.note.clone()),
            ]));
        }
        if let Some(error) = &self.error {
            lines.push(Line::styled(
                format!("  ! {}", clean_text(error)),
                theme.block_style(),
            ));
        }

        let hint = self.footer_hint(width, in_flight, scrollable);
        let hint_style = if in_flight {
            theme.wait_style()
        } else {
            theme.muted_style()
        };
        lines.push(Line::styled(hint, hint_style));

        ConfirmationRender {
            lines,
            note_line_index,
            note_cursor_bytes: self.note_cursor.min(self.note.len()),
        }
    }

    fn build_submit(
        &self,
        choice: String,
        note: Option<String>,
    ) -> Result<(FileBackedAction, ConfirmationOutcomeDraft), String> {
        let content = self
            .pending
            .resume_confirmation_with_note(&choice, note.as_deref())?;
        let clean_note = note.map(|n| clean_text(n.trim())).filter(|n| !n.is_empty());
        let outcome = ConfirmationOutcome::from_choice(&choice);
        let draft = ConfirmationOutcomeDraft {
            request_id: self.pending.request_id.clone(),
            outcome,
            choice: choice.clone(),
            summary: confirmation_summary(&self.pending),
            note: clean_note,
        };
        let action = FileBackedAction::Resume {
            request_id: self.pending.request_id.clone(),
            response: response_text_from_content(content),
            retry_input: choice,
        };
        Ok((action, draft))
    }
}

fn render_option_line(
    idx: usize,
    label: &str,
    is_default: bool,
    selected: bool,
    theme: Theme,
    is_reject_note: bool,
) -> Line<'static> {
    let prefix = if selected {
        if theme.is_dumb { "> " } else { "▶ " }
    } else {
        "  "
    };
    let mut prefix_style = if selected {
        theme.accent_bold()
    } else {
        Style::default()
    };
    let mut num_style = if selected {
        theme.accent_bold()
    } else {
        theme.muted_style()
    };
    let mut label_style = if is_reject_note {
        theme.block_style()
    } else {
        match label {
            "approve" => theme.bold_if_capable(theme.run_style()),
            "allow_session" => theme.accent_bold(),
            "reject" => theme.block_style(),
            _ => Style::default(),
        }
    };
    let mut default_style = metadata_style(theme);

    if selected && !theme.is_dumb {
        if theme.selection_bg != Color::Reset {
            prefix_style = prefix_style.bg(theme.selection_bg);
            num_style = num_style.bg(theme.selection_bg);
            label_style = label_style.bg(theme.selection_bg);
            default_style = default_style.bg(theme.selection_bg);
        }
        label_style = label_style.add_modifier(Modifier::BOLD);
    }

    let mut spans = vec![
        Span::styled(prefix, prefix_style),
        Span::styled(format!("[{}] ", idx + 1), num_style),
        Span::styled(clean_text(label), label_style),
    ];
    if is_default {
        spans.push(Span::styled(" (default)", default_style));
    }
    Line::from(spans)
}

pub(in crate::file_backed) fn confirmation_summary(pending: &PendingYieldCell) -> String {
    let first_line = pending
        .title
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Approval");
    match &pending.presentation {
        Some(ToolResultPresentation::Command { cmdline, .. }) => {
            let cmd = cmdline
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("");
            if cmd.is_empty() {
                clean_text(first_line)
            } else if first_line.starts_with("Escalate tool call") {
                format!("bash: {}", clean_text(cmd))
            } else {
                format!("{} · $ {}", clean_text(first_line), clean_text(cmd))
            }
        }
        Some(ToolResultPresentation::Diff { path, .. }) => {
            if first_line.starts_with("Escalate tool call") {
                let tool = if first_line.contains("write_file") {
                    "write_file"
                } else if first_line.contains("edit_file") {
                    "edit_file"
                } else {
                    first_line
                };
                format!("{tool} {}", clean_text(path))
            } else if first_line.contains(path.as_str()) {
                clean_text(first_line)
            } else {
                format!("{} · {}", clean_text(first_line), clean_text(path))
            }
        }
        _ => clean_text(first_line),
    }
}

impl FileBackedApp {
    pub(in crate::file_backed) fn confirmation_live_lines(
        &self,
        width: usize,
        viewport_height: Option<usize>,
    ) -> Option<Vec<Line<'static>>> {
        let conf = self.confirmation.as_ref()?;
        let in_flight = self.response_in_flight.is_some();
        let mut rendered = conf.render(self.theme, width, in_flight);
        let scrollable = viewport_height.is_some_and(|h| {
            h > 0 && crate::transcript_ui::wrapped_line_count(&rendered.lines, width) > h
        });
        if scrollable
            && !conf.editing_note
            && let Some(last) = rendered.lines.last_mut()
        {
            let hint_style = if in_flight {
                self.theme.wait_style()
            } else {
                self.theme.muted_style()
            };
            *last = Line::styled(conf.footer_hint(width, in_flight, true), hint_style);
        }
        Some(rendered.lines)
    }

    pub(in crate::file_backed) fn confirmation_scroll_bounds(&self) -> (usize, usize) {
        let Some(conf) = self.confirmation.as_ref() else {
            return (0, 1);
        };
        let width = self.viewport_size.0.max(1) as usize;
        let height = self.viewport_size.1.max(1) as usize;
        let rendered = conf.render(self.theme, width, self.response_in_flight.is_some());
        let conf_height = crate::transcript_ui::wrapped_line_count(&rendered.lines, width);
        (conf_height.saturating_sub(height), (height / 2).max(1))
    }

    pub(in crate::file_backed) fn confirmation_scroll_y(
        &self,
        live_lines: &[Line<'static>],
        history_height: usize,
        width: usize,
        base_height: usize,
    ) -> Option<u16> {
        let conf = self.confirmation.as_ref()?;
        let conf_len = conf
            .render(self.theme, width, self.response_in_flight.is_some())
            .lines
            .len();
        let conf_start = live_lines.len().saturating_sub(conf_len);
        let conf_top = history_height
            + crate::transcript_ui::wrapped_line_count(&live_lines[..conf_start], width);
        let conf_end = history_height + crate::transcript_ui::wrapped_line_count(live_lines, width);
        let max_scroll = conf_end.saturating_sub(base_height);
        let max_back = max_scroll.saturating_sub(conf_top);
        Some(
            max_scroll
                .saturating_sub(conf.scroll_from_bottom.min(max_back))
                .min(u16::MAX as usize) as u16,
        )
    }

    pub(in crate::file_backed) fn sync_confirmation(&mut self) {
        match &self.pending_yield {
            Some(pending)
                if matches!(pending.kind, YieldKind::Confirmation)
                    && !pending.options.is_empty() =>
            {
                let request_id = pending.request_id.clone();
                if let Some(conf) = self.confirmation.as_mut() {
                    conf.sync(pending.clone());
                } else {
                    self.confirmation = Some(ConfirmationState::new(pending.clone()));
                }
                self.remove_pending_yield_cell(&request_id);
            }
            _ => self.confirmation = None,
        }
    }

    pub(in crate::file_backed) fn remove_pending_yield_cell(&mut self, request_id: &str) {
        let indices: Vec<usize> = self
            .transcript
            .iter()
            .enumerate()
            .filter_map(|(idx, cell)| match cell {
                HistoryCell::PendingYield(pending) if pending.request_id == request_id => Some(idx),
                _ => None,
            })
            .collect();
        for index in indices.into_iter().rev() {
            self.remove_receipt_cell(index);
        }
    }

    pub(in crate::file_backed) fn handle_confirmation_overlay_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<ConfirmationKeyResult> {
        if self
            .confirmation
            .as_ref()
            .is_some_and(|conf| !conf.editing_note)
            && self.response_in_flight.is_none()
            && self.handle_confirmation_scroll_key(key)
        {
            return Some(ConfirmationKeyResult::Swallowed);
        }
        let conf = self.confirmation.as_mut()?;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if ctrl && !alt && matches!(key.code, KeyCode::Char('q' | 'z')) {
            return Some(ConfirmationKeyResult::Ignored);
        }
        if self.response_in_flight.is_some() {
            return Some(ConfirmationKeyResult::Swallowed);
        }

        if conf.editing_note {
            match key.code {
                KeyCode::Esc => {
                    conf.exit_note_mode();
                    return Some(ConfirmationKeyResult::Swallowed);
                }
                KeyCode::Enter if !ctrl && !alt => {
                    let note = (!conf.note.trim().is_empty()).then(|| conf.note.clone());
                    return Some(match conf.build_submit("reject".to_string(), note) {
                        Ok((action, draft)) => {
                            self.staged_confirmation_outcome = Some(draft);
                            ConfirmationKeyResult::Action(action)
                        }
                        Err(err) => {
                            if let Some(conf) = self.confirmation.as_mut() {
                                conf.error = Some(err);
                            }
                            ConfirmationKeyResult::Swallowed
                        }
                    });
                }
                KeyCode::Left if !ctrl && !alt => conf.move_note_left(),
                KeyCode::Right if !ctrl && !alt => conf.move_note_right(),
                KeyCode::Home => conf.move_note_home(),
                KeyCode::End => conf.move_note_end(),
                KeyCode::Char('a') if ctrl && !alt => conf.move_note_home(),
                KeyCode::Char('e') if ctrl && !alt => conf.move_note_end(),
                KeyCode::Char('u') if ctrl && !alt => conf.clear_note(),
                KeyCode::Backspace => conf.backspace_note(),
                KeyCode::Delete => conf.delete_note_forward(),
                KeyCode::Char(ch) if !ctrl && !alt => conf.insert_note_char(ch),
                _ => {}
            }
            return Some(ConfirmationKeyResult::Swallowed);
        }

        match key.code {
            KeyCode::Esc => {
                return Some(ConfirmationKeyResult::Action(FileBackedAction::Interrupt));
            }
            KeyCode::Up | KeyCode::BackTab => conf.move_up(),
            KeyCode::Down | KeyCode::Tab => conf.move_down(),
            KeyCode::Char('k') if !ctrl && !alt => conf.move_up(),
            KeyCode::Char('j') if !ctrl && !alt => conf.move_down(),
            KeyCode::Enter if !ctrl && !alt => {
                if conf.selected_is_reject_with_note() {
                    conf.enter_note_mode();
                    return Some(ConfirmationKeyResult::Swallowed);
                }
                let Some(choice) = conf.pending.options.get(conf.selected).cloned() else {
                    return Some(ConfirmationKeyResult::Swallowed);
                };
                return Some(match conf.build_submit(choice, None) {
                    Ok((action, draft)) => {
                        self.staged_confirmation_outcome = Some(draft);
                        ConfirmationKeyResult::Action(action)
                    }
                    Err(err) => {
                        if let Some(conf) = self.confirmation.as_mut() {
                            conf.error = Some(err);
                        }
                        ConfirmationKeyResult::Swallowed
                    }
                });
            }
            KeyCode::Char(ch) if ('1'..='9').contains(&ch) && !ctrl && !alt => {
                let idx = (ch as u8 - b'1') as usize;
                let total = conf.total_items();
                if idx >= total {
                    conf.error = Some(format!("Select an option (1-{total})."));
                    return Some(ConfirmationKeyResult::Swallowed);
                }
                conf.selected = idx;
                if conf.selected_is_reject_with_note() {
                    conf.enter_note_mode();
                    return Some(ConfirmationKeyResult::Swallowed);
                }
                let Some(choice) = conf.pending.options.get(idx).cloned() else {
                    return Some(ConfirmationKeyResult::Swallowed);
                };
                return Some(match conf.build_submit(choice, None) {
                    Ok((action, draft)) => {
                        self.staged_confirmation_outcome = Some(draft);
                        ConfirmationKeyResult::Action(action)
                    }
                    Err(err) => {
                        if let Some(conf) = self.confirmation.as_mut() {
                            conf.error = Some(err);
                        }
                        ConfirmationKeyResult::Swallowed
                    }
                });
            }
            _ => {}
        }
        Some(ConfirmationKeyResult::Swallowed)
    }

    fn handle_confirmation_scroll_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let delta: Option<isize> = match key.code {
            KeyCode::PageUp => Some(1),
            KeyCode::PageDown => Some(-1),
            KeyCode::Char('b') if !ctrl && !alt => Some(1),
            KeyCode::Char(' ') if !ctrl && !alt => Some(-1),
            KeyCode::Char('u') if ctrl && !alt => Some(1),
            KeyCode::Char('d') if ctrl && !alt => Some(-1),
            KeyCode::Home if !ctrl && !alt => Some(isize::MAX),
            KeyCode::End if !ctrl && !alt => Some(isize::MIN),
            _ => None,
        };
        let Some(direction) = delta else {
            return false;
        };
        let (max_scroll, page_step) = self.confirmation_scroll_bounds();
        let Some(conf) = self.confirmation.as_mut() else {
            return false;
        };
        conf.scroll_from_bottom = match direction {
            isize::MAX => max_scroll,
            isize::MIN => 0,
            d if d > 0 => conf
                .scroll_from_bottom
                .saturating_add(page_step)
                .min(max_scroll),
            _ => conf
                .scroll_from_bottom
                .min(max_scroll)
                .saturating_sub(page_step),
        };
        true
    }

    pub(in crate::file_backed) fn handle_confirmation_paste(&mut self, text: &str) -> bool {
        let Some(conf) = self.confirmation.as_mut() else {
            return false;
        };
        if self.response_in_flight.is_none() && conf.editing_note {
            conf.insert_note_text(text);
        }
        true
    }

    pub(in crate::file_backed) fn stage_fallback_confirmation_outcome(
        &mut self,
        pending: &PendingYieldCell,
        retry_input: &str,
    ) {
        if !matches!(pending.kind, YieldKind::Confirmation) {
            return;
        }
        let choice = retry_input.trim();
        let choice = if choice.is_empty() {
            pending
                .default_option
                .as_deref()
                .or_else(|| pending.options.first().map(String::as_str))
                .unwrap_or("approve")
        } else {
            choice
        };
        self.staged_confirmation_outcome = Some(ConfirmationOutcomeDraft {
            request_id: pending.request_id.clone(),
            outcome: ConfirmationOutcome::from_choice(choice),
            choice: choice.to_string(),
            summary: confirmation_summary(pending),
            note: None,
        });
    }

    pub(in crate::file_backed) fn settle_confirmation_on_resume_ok(
        &mut self,
        request_id: &str,
    ) -> bool {
        let draft = if self
            .staged_confirmation_outcome
            .as_ref()
            .is_some_and(|d| d.request_id == request_id)
        {
            self.staged_confirmation_outcome.take()
        } else {
            self.pending_yield
                .as_ref()
                .filter(|p| p.request_id == request_id && matches!(p.kind, YieldKind::Confirmation))
                .map(|pending| ConfirmationOutcomeDraft {
                    request_id: request_id.to_string(),
                    outcome: ConfirmationOutcome::Closed,
                    choice: String::new(),
                    summary: confirmation_summary(pending),
                    note: None,
                })
        };
        let Some(draft) = draft else {
            return false;
        };
        self.remove_pending_yield_cell(request_id);
        self.push_turn_preview_cell(HistoryCell::ConfirmationReceipt {
            outcome: draft.outcome,
            choice: draft.choice,
            summary: draft.summary,
            note: draft.note,
        });
        if self
            .pending_yield
            .as_ref()
            .is_some_and(|pending| pending.request_id == request_id)
        {
            self.pending_yield = None;
            self.form = None;
            self.confirmation = None;
            self.refresh_completion();
        }
        true
    }

    pub(in crate::file_backed) fn settle_confirmation_on_external_clear(&mut self) {
        if self.response_in_flight.is_some() {
            return;
        }
        let Some(conf) = self.confirmation.take() else {
            return;
        };
        self.staged_confirmation_outcome = None;
        self.remove_pending_yield_cell(&conf.pending.request_id);
        self.push_turn_preview_cell(HistoryCell::ConfirmationReceipt {
            outcome: ConfirmationOutcome::Closed,
            choice: String::new(),
            summary: confirmation_summary(&conf.pending),
            note: None,
        });
    }
}
