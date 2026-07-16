//! File-backed TUI input handling and application state transitions.

use std::collections::BTreeMap;

use alan_agent_protocol::{
    UiActivitySnapshot, UiActivityState, UiEvent, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot,
    UiThinkingSnapshot, UiThinkingState, YieldKind,
};
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::completion::{self, CompletionCandidate, CompletionSources, CompletionState};
use crate::composer::{Composer, ComposerKeyOutcome};
use crate::form::FormState;
use crate::history::{HistoryCell, PendingYieldCell, RenderOpts, RunningTool};
use crate::reconcile::{AssistantDecision, StreamAction, StreamReconciler, UserDecision};

use super::file_surface::{TapeRecordV1, response_text_from_content};
use super::{MAX_COMPLETION_ROWS, MAX_COMPOSER_LINES};

fn default_commands() -> Vec<CompletionCandidate> {
    [
        ("compact", "summarize context"),
        ("rollback", "undo the last turn"),
        ("clear", "clear the transcript"),
        ("help", "show key bindings"),
        ("quit", "exit alan"),
    ]
    .into_iter()
    .map(|(value, detail)| CompletionCandidate::new(value, Some(detail.to_string())))
    .collect()
}

pub(super) enum FileBackedEvent {
    Terminal(TerminalEvent),
    Output(String),
    RequestsChanged,
    ActionsChanged,
    Ui(UiEvent),
    Tape(TapeRecordV1),
    Error(String),
}

#[derive(Debug)]
pub(super) enum FileBackedAction {
    Submit(String),
    Resume {
        request_id: String,
        response: String,
    },
    MachineCtl {
        command: String,
        success_notice: String,
    },
    Interrupt,
    Quit,
}

pub(super) struct FileBackedApp {
    pub(super) agent_path: String,
    pub(super) composer: Composer,
    pub(super) transcript: Vec<HistoryCell>,
    pub(super) action_cells: BTreeMap<String, usize>,
    pub(super) activity: UiActivitySnapshot,
    pub(super) plan: UiPlanSnapshot,
    pub(super) thinking: UiThinkingSnapshot,
    pub(super) running_tools: Vec<RunningTool>,
    pub(super) pending_yield: Option<PendingYieldCell>,
    pub(super) form: Option<FormState>,
    pub(super) completion: Option<CompletionState>,
    pub(super) completion_sources: CompletionSources,
    pub(super) expand_thinking: bool,
    pub(super) notice: Option<String>,
    pub(super) should_quit: bool,
    /// The pure state machine reconciling the optimistic `io/output` stream
    /// preview against the authoritative `machine/tape` records. All
    /// suppression/echo/matching logic lives there; this app only locates
    /// cells and applies the returned decisions.
    pub(super) reconciler: StreamReconciler,
    /// First transcript cell for a remote turn whose stream/UI/action cells
    /// reached this watcher before its user tape record. When the boundary
    /// arrives, insert the user cell before the whole block and shift side
    /// indexes such as `action_cells`.
    pub(super) pending_remote_turn_start: Option<usize>,
}

impl FileBackedApp {
    pub(super) fn new(agent_path: String) -> Self {
        Self {
            notice: Some(format!("local renderer host attached to {agent_path}")),
            agent_path,
            composer: Composer::default(),
            transcript: Vec::new(),
            action_cells: BTreeMap::new(),
            activity: UiActivitySnapshot::idle(),
            plan: UiPlanSnapshot::empty(),
            thinking: UiThinkingSnapshot::idle(),
            running_tools: Vec::new(),
            pending_yield: None,
            form: None,
            completion: None,
            completion_sources: CompletionSources {
                commands: default_commands(),
                ..CompletionSources::default()
            },
            expand_thinking: false,
            should_quit: false,
            reconciler: StreamReconciler::new(),
            pending_remote_turn_start: None,
        }
    }

    pub(super) fn set_skill_candidates(&mut self, skills: Vec<CompletionCandidate>) {
        self.completion_sources.skills = skills;
    }

    pub(super) fn set_file_candidates(&mut self, files: Vec<CompletionCandidate>) {
        self.completion_sources.files = files;
    }

    pub(super) fn dispatch(&mut self, event: FileBackedEvent) -> Option<FileBackedAction> {
        match event {
            FileBackedEvent::Terminal(TerminalEvent::Key(key)) => self.handle_key(key),
            FileBackedEvent::Terminal(TerminalEvent::Paste(text)) => {
                if let Some(form) = self.form.as_mut() {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        form.insert_char(ch);
                    }
                } else {
                    self.composer.insert_text(&text);
                    self.refresh_completion();
                }
                None
            }
            FileBackedEvent::Terminal(TerminalEvent::Resize(_, _)) => None,
            FileBackedEvent::Terminal(_) => None,
            FileBackedEvent::Output(text) => {
                self.push_output(text);
                None
            }
            FileBackedEvent::Ui(event) => {
                self.apply_ui_event(event);
                None
            }
            FileBackedEvent::Tape(record) => {
                self.apply_tape_record(record);
                None
            }
            FileBackedEvent::RequestsChanged | FileBackedEvent::ActionsChanged => None,
            FileBackedEvent::Error(message) => {
                self.push_error(message);
                None
            }
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        let pending_input = self.form.is_some() || self.pending_yield.is_some();
        if pending_input {
            self.completion = None;
        } else if self.completion.is_some() && self.consume_completion_key(key) {
            return None;
        }
        if self.form.is_some() {
            return self.handle_form_key(key);
        }
        if let Some(action) = self.confirmation_keypress(key) {
            return Some(action);
        }
        match key {
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                Some(FileBackedAction::Quit)
            }
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.expand_thinking = !self.expand_thinking;
                None
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => Some(FileBackedAction::Interrupt),
            KeyEvent {
                code: KeyCode::Char('/'),
                modifiers,
                ..
            } if modifiers.is_empty() && self.composer.text().is_empty() => {
                self.composer.set_text("/");
                self.refresh_completion();
                None
            }
            _ => {
                let outcome = self.composer.handle_key(key);
                self.refresh_completion();
                match outcome {
                    ComposerKeyOutcome::Submit => self.handle_submit(),
                    ComposerKeyOutcome::Interrupt => Some(FileBackedAction::Interrupt),
                    ComposerKeyOutcome::Changed | ComposerKeyOutcome::Ignored => None,
                }
            }
        }
    }

    pub(super) fn consume_completion_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                if let Some(state) = self.completion.as_mut() {
                    state.move_up();
                }
                true
            }
            KeyCode::Down => {
                if let Some(state) = self.completion.as_mut() {
                    state.move_down();
                }
                true
            }
            KeyCode::Esc => {
                if self.turn_active() || self.pending_yield.is_some() {
                    false
                } else {
                    self.completion = None;
                    true
                }
            }
            KeyCode::Tab => {
                self.accept_completion();
                true
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.accept_completion();
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_form_key(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        match key.code {
            KeyCode::Esc => Some(FileBackedAction::Interrupt),
            KeyCode::Tab | KeyCode::Down => {
                if let Some(form) = self.form.as_mut() {
                    form.next_field();
                }
                None
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(form) = self.form.as_mut() {
                    form.prev_field();
                }
                None
            }
            KeyCode::Backspace => {
                if let Some(form) = self.form.as_mut() {
                    form.backspace();
                }
                None
            }
            KeyCode::Enter => self.submit_form(),
            KeyCode::Char(ch) => {
                if let Some(form) = self.form.as_mut() {
                    form.insert_char(ch);
                }
                None
            }
            _ => None,
        }
    }

    pub(super) fn accept_completion(&mut self) {
        let Some(state) = self.completion.take() else {
            return;
        };
        if let Some(candidate) = state.selected_candidate() {
            let value = candidate.value.clone();
            let (new_text, cursor) = completion::apply(self.composer.text(), &state, &value);
            self.composer.set_text_with_cursor(new_text, cursor);
        }
        self.refresh_completion();
    }

    pub(super) fn refresh_completion(&mut self) {
        if self.pending_yield.is_some() {
            self.completion = None;
            return;
        }
        self.completion = completion::detect(
            self.composer.text(),
            self.composer.cursor(),
            &self.completion_sources,
        );
    }

    pub(super) fn submit_form(&mut self) -> Option<FileBackedAction> {
        let pending = self.pending_yield.clone()?;
        let form = self.form.as_mut()?;
        match pending.resume_content(&form.answers_json()) {
            Ok(content) => {
                self.form = None;
                Some(FileBackedAction::Resume {
                    request_id: pending.request_id,
                    response: response_text_from_content(content),
                })
            }
            Err(message) => {
                form.error = Some(message);
                None
            }
        }
    }

    pub(super) fn confirmation_keypress(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
            return None;
        }
        let pending = self.pending_yield.as_ref()?;
        if !matches!(pending.kind, YieldKind::Confirmation) || !self.composer.text().is_empty() {
            return None;
        }
        let KeyCode::Char(ch) = key.code else {
            return None;
        };
        let index = ch.to_digit(10).filter(|digit| *digit >= 1)? as usize - 1;
        let option = pending.options.get(index)?.clone();
        let pending = pending.clone();
        match pending.resume_content(&option) {
            Ok(content) => Some(FileBackedAction::Resume {
                request_id: pending.request_id,
                response: response_text_from_content(content),
            }),
            Err(message) => {
                self.notice = Some(message);
                None
            }
        }
    }

    pub(super) fn handle_submit(&mut self) -> Option<FileBackedAction> {
        if let Some(pending) = self.pending_yield.clone() {
            let text = self.composer.text().trim().to_string();
            self.composer.set_text("");
            self.completion = None;
            match pending.resume_content(&text) {
                Ok(content) => {
                    return Some(FileBackedAction::Resume {
                        request_id: pending.request_id,
                        response: response_text_from_content(content),
                    });
                }
                Err(message) => {
                    self.composer.set_text(text);
                    self.notice = Some(message);
                    return None;
                }
            }
        }

        let text = self.composer.take_submit()?;
        self.completion = None;
        self.composer.remember(&text);
        if let Some(action) = self.handle_command(&text) {
            return Some(action);
        }
        self.transcript.push(HistoryCell::User(text.clone()));
        self.reconciler.on_local_submit(&text);
        self.pending_remote_turn_start = None;
        Some(FileBackedAction::Submit(text))
    }

    pub(super) fn handle_command(&mut self, text: &str) -> Option<FileBackedAction> {
        let command = text.strip_prefix('/')?;
        let name = command.split_whitespace().next().unwrap_or("");
        match name {
            "quit" => {
                self.should_quit = true;
                Some(FileBackedAction::Quit)
            }
            "compact" => Some(FileBackedAction::MachineCtl {
                command: "compact".to_string(),
                success_notice: "compact requested".to_string(),
            }),
            "rollback" => Some(FileBackedAction::MachineCtl {
                command: "rollback".to_string(),
                success_notice: "rollback requested".to_string(),
            }),
            "clear" => {
                self.transcript.clear();
                self.action_cells.clear();
                self.pending_remote_turn_start = None;
                None
            }
            "help" => {
                self.notice = Some(
                    "/compact /rollback /clear /quit · ctrl+r toggle thinking · esc interrupt"
                        .to_string(),
                );
                None
            }
            _ => {
                self.notice = Some(format!("unknown command: /{name}"));
                None
            }
        }
    }

    pub(super) fn set_pending_yield(&mut self, pending: PendingYieldCell) {
        self.pending_yield = Some(pending.clone());
        self.sync_form();
        self.completion = None;
        // The request watcher can fire on `created:<id>` before the runtime has
        // written kind/prompt/options, so the first sync may insert a sparse
        // cell; later syncs for the same request must update it in place.
        if let Some(existing) = self.transcript.iter_mut().find_map(|cell| match cell {
            HistoryCell::PendingYield(existing) if existing.request_id == pending.request_id => {
                Some(existing)
            }
            _ => None,
        }) {
            *existing = pending;
        } else {
            self.push_turn_preview_cell(HistoryCell::PendingYield(pending));
        }
    }

    pub(super) fn clear_pending_yield(&mut self) {
        self.pending_yield = None;
        self.form = None;
        self.refresh_completion();
    }

    pub(super) fn sync_form(&mut self) {
        match &self.pending_yield {
            Some(pending)
                if matches!(pending.kind, YieldKind::StructuredInput)
                    && pending.questions.len() > 1 =>
            {
                // Rebuild when the question set itself changes (e.g. fields
                // arrived after the request-created event), not just on a new
                // request id — otherwise the form keeps stale questions.
                if self.form.as_ref().is_none_or(|form| {
                    form.request_id != pending.request_id
                        || form.fields.len() != pending.questions.len()
                        || form
                            .fields
                            .iter()
                            .zip(pending.questions.iter())
                            .any(|(field, question)| &field.question != question)
                }) {
                    self.form = Some(FormState::new(
                        pending.request_id.clone(),
                        pending.questions.clone(),
                    ));
                }
            }
            _ => self.form = None,
        }
    }

    pub(super) fn push_output(&mut self, text: String) {
        match self.reconciler.on_stream(text) {
            StreamAction::Drop => {}
            StreamAction::Append(text) => self.append_to_open_assistant_cell(text),
            StreamAction::StartNew(text) => {
                self.mark_pending_remote_turn_start_if_unbounded();
                self.transcript.push(HistoryCell::Assistant(text));
            }
        }
    }

    pub(super) fn push_turn_preview_cell(&mut self, cell: HistoryCell) {
        self.mark_pending_remote_turn_start_if_unbounded();
        self.transcript.push(cell);
    }

    pub(super) fn insert_user_boundary(&mut self, content: String) {
        let index = self
            .pending_remote_turn_start
            .take()
            .unwrap_or(self.transcript.len())
            .min(self.transcript.len());
        self.transcript.insert(index, HistoryCell::User(content));
        self.shift_action_cells_for_insert(index);
    }

    pub(super) fn flush_held_stream_after_boundary(&mut self) {
        if let Some(stream) = self.reconciler.take_flushed_stream() {
            self.transcript.push(HistoryCell::Assistant(stream));
        }
    }

    pub(super) fn append_to_open_assistant_cell(&mut self, text: String) {
        if let Some(index) = self.current_assistant_cell()
            && let Some(HistoryCell::Assistant(existing)) = self.transcript.get_mut(index)
        {
            existing.push_str(&text);
            return;
        }
        self.mark_pending_remote_turn_start_if_unbounded();
        self.transcript.push(HistoryCell::Assistant(text));
    }

    pub(super) fn mark_pending_remote_turn_start_if_unbounded(&mut self) {
        if self.pending_remote_turn_start.is_some() {
            return;
        }
        if self.reconciler.awaiting_boundary() || !self.current_turn_has_user_boundary() {
            self.pending_remote_turn_start = Some(self.transcript.len());
        }
    }

    pub(super) fn current_turn_has_user_boundary(&self) -> bool {
        for cell in self.transcript.iter().rev() {
            match cell {
                HistoryCell::User(_) => return true,
                HistoryCell::Assistant(_) => return false,
                _ => {}
            }
        }
        false
    }

    /// The index of the current turn's assistant cell: the most recent
    /// `Assistant` cell with no user message or yield after it. Interposed
    /// plan/notice cells are scanned over; a boundary stops the scan.
    pub(super) fn current_assistant_cell(&self) -> Option<usize> {
        for (idx, cell) in self.transcript.iter().enumerate().rev() {
            match cell {
                HistoryCell::Assistant(_) => return Some(idx),
                HistoryCell::User(_) | HistoryCell::PendingYield(_) => return None,
                _ => {}
            }
        }
        None
    }

    /// Reconcile a live `machine/tape` record with the transcript. All the
    /// matching/suppression/echo logic lives in [`StreamReconciler`]; this
    /// only locates the current-turn cell and applies the returned decision.
    pub(super) fn apply_tape_record(&mut self, record: TapeRecordV1) {
        if record.kind != "message" {
            return;
        }
        match record.role.as_str() {
            "user" => {
                match self.reconciler.on_user_record(&record.content) {
                    UserDecision::Drop => {}
                    UserDecision::Push(content) => self.insert_user_boundary(content),
                }
                self.flush_held_stream_after_boundary();
            }
            "assistant" => {
                let idx = self.current_assistant_cell();
                let preview = idx.and_then(|i| match &self.transcript[i] {
                    HistoryCell::Assistant(text) => Some(text.clone()),
                    _ => None,
                });
                match self
                    .reconciler
                    .on_assistant_record(record.content, preview.as_deref())
                {
                    AssistantDecision::Drop => {}
                    AssistantDecision::ReplacePreview(content) => {
                        if let Some(HistoryCell::Assistant(existing)) =
                            idx.map(|i| &mut self.transcript[i])
                        {
                            *existing = content;
                        }
                    }
                    AssistantDecision::Push(content) => {
                        self.transcript.push(HistoryCell::Assistant(content))
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn push_error(&mut self, message: String) {
        self.notice = Some(message.clone());
        self.transcript.push(HistoryCell::Error(message));
    }

    pub(super) fn apply_ui_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Activity { snapshot } => self.apply_ui_activity_snapshot(snapshot),
            UiEvent::Plan { snapshot } => self.apply_ui_plan_snapshot(snapshot),
            UiEvent::Thinking { snapshot } => self.apply_ui_thinking_snapshot(snapshot),
            UiEvent::Notice { snapshot } => self.apply_ui_notice_snapshot(snapshot),
            UiEvent::Error {
                message,
                recoverable,
            } => {
                if recoverable {
                    self.notice = Some(message);
                } else {
                    self.push_error(message);
                }
            }
        }
    }

    pub(super) fn apply_ui_activity_snapshot(&mut self, snapshot: UiActivitySnapshot) {
        self.activity = snapshot;
    }

    pub(super) fn apply_ui_plan_snapshot(&mut self, snapshot: UiPlanSnapshot) {
        let changed = self.plan != snapshot;
        self.plan = snapshot.clone();
        if changed && !snapshot.items.is_empty() {
            self.push_turn_preview_cell(HistoryCell::Plan(
                snapshot
                    .items
                    .into_iter()
                    .map(|item| crate::history::PlanLine {
                        status: item.status,
                        content: item.content,
                    })
                    .collect(),
            ));
        }
    }

    pub(super) fn apply_ui_thinking_snapshot(&mut self, snapshot: UiThinkingSnapshot) {
        let changed = self.thinking != snapshot;
        self.thinking = snapshot.clone();
        if changed
            && matches!(snapshot.state, UiThinkingState::Complete)
            && !snapshot.text.trim().is_empty()
        {
            self.push_turn_preview_cell(HistoryCell::Thinking {
                text: snapshot.text,
                duration_secs: snapshot.duration_secs.unwrap_or(0),
            });
        }
    }

    pub(super) fn apply_ui_notice_snapshot(&mut self, snapshot: UiNoticeSnapshot) {
        self.notice = match snapshot.kind {
            UiNoticeKind::None => None,
            _ if snapshot.message.trim().is_empty() => None,
            _ => Some(snapshot.message),
        };
    }

    pub(super) fn activity_label(&self) -> Option<&str> {
        match self.activity.state {
            UiActivityState::Idle => None,
            UiActivityState::Paused => Some("waiting for input"),
            UiActivityState::Running
                if matches!(self.thinking.state, UiThinkingState::Streaming) =>
            {
                Some("thinking")
            }
            UiActivityState::Running => Some("working"),
        }
    }

    pub(super) fn turn_active(&self) -> bool {
        !matches!(self.activity.state, UiActivityState::Idle)
    }

    pub(super) fn activity_started_at_ms(&self) -> Option<u64> {
        self.activity.started_at_ms
    }

    pub(super) fn upsert_action_cell(&mut self, action_id: String, cell: HistoryCell) {
        if let Some(index) = self.action_cells.get(&action_id).copied()
            && let Some(existing) = self.transcript.get_mut(index)
        {
            *existing = cell;
            return;
        }
        self.mark_pending_remote_turn_start_if_unbounded();
        let index = self.transcript.len();
        self.transcript.push(cell);
        self.action_cells.insert(action_id, index);
    }

    pub(super) fn rendered_history_lines(&self, width: usize) -> Vec<String> {
        let opts = self.render_opts(width);
        self.transcript
            .iter()
            .flat_map(|cell| cell.render_lines(opts))
            .collect()
    }

    pub(super) fn drain_committed_scrollback(
        &mut self,
        viewport_width: usize,
        viewport_height: usize,
    ) -> Vec<String> {
        let opts = self.render_opts(viewport_width);
        let reserved = self.live_region_height(viewport_width) as usize + 1;
        let max_lines = viewport_height.saturating_sub(reserved).max(2);
        let lines = self.rendered_history_lines(viewport_width);
        if lines.len() <= max_lines {
            return Vec::new();
        }
        let drain_count = lines.len() - max_lines;
        let pruned_count = self.prune_rendered_prefix(opts, drain_count);
        lines.into_iter().take(pruned_count).collect()
    }

    pub(super) fn prune_rendered_prefix(
        &mut self,
        opts: RenderOpts,
        lines_to_prune: usize,
    ) -> usize {
        let mut remaining = lines_to_prune;
        let mut cells_to_remove = 0;
        let mut pruned = 0;

        while remaining > 0 && cells_to_remove < self.transcript.len() {
            let cell_lines = self.transcript[cells_to_remove].render_lines(opts).len();
            if cell_lines > remaining {
                break;
            }
            remaining -= cell_lines;
            pruned += cell_lines;
            cells_to_remove += 1;
        }

        if cells_to_remove > 0 {
            self.transcript.drain(0..cells_to_remove);
            self.shift_action_cells(cells_to_remove);
            self.shift_pending_remote_turn_start(cells_to_remove);
        }

        if remaining > 0
            && let Some(cell) = self.transcript.first_mut()
            && cell.trim_rendered_prefix(opts, remaining)
        {
            pruned += remaining;
        }

        pruned
    }

    pub(super) fn shift_action_cells(&mut self, removed_prefix_len: usize) {
        self.action_cells = self
            .action_cells
            .iter()
            .filter_map(|(action_id, index)| {
                if *index < removed_prefix_len {
                    None
                } else {
                    Some((action_id.clone(), index - removed_prefix_len))
                }
            })
            .collect();
    }

    pub(super) fn shift_action_cells_for_insert(&mut self, inserted_at: usize) {
        for index in self.action_cells.values_mut() {
            if *index >= inserted_at {
                *index += 1;
            }
        }
    }

    pub(super) fn shift_pending_remote_turn_start(&mut self, removed_prefix_len: usize) {
        self.pending_remote_turn_start = self
            .pending_remote_turn_start
            .map(|index| index.saturating_sub(removed_prefix_len));
    }

    pub(super) fn seed_reconciler_from_tape_history(&mut self, raw: &str) {
        self.reconciler = StreamReconciler::new();
        self.pending_remote_turn_start = None;
        for line in raw.lines() {
            let Ok(record) = serde_json::from_str::<TapeRecordV1>(line) else {
                continue;
            };
            if record.kind == "message" {
                self.reconciler.on_hydrated_message_record(&record.role);
            }
        }
    }

    pub(super) fn render_opts(&self, width: usize) -> RenderOpts {
        RenderOpts::new(width, self.expand_thinking)
    }

    pub(super) fn live_region_height(&self, width: usize) -> u16 {
        let header_lines = 1usize;
        let activity_lines = usize::from(self.activity_label().is_some());
        let notice_lines = usize::from(self.notice.is_some());
        let tool_lines = self.running_tools.len();
        let body_lines = if let Some(form) = &self.form {
            form.render_lines().len()
        } else {
            self.composer_height(width) + self.completion_height() as usize + 1
        };
        (header_lines + activity_lines + notice_lines + tool_lines + body_lines) as u16
    }

    pub(super) fn completion_height(&self) -> u16 {
        self.completion
            .as_ref()
            .map(|state| state.matches.len().min(MAX_COMPLETION_ROWS) as u16)
            .unwrap_or(0)
    }

    pub(super) fn composer_height(&self, width: usize) -> usize {
        let width = width.max(1);
        let lines = self
            .composer
            .text()
            .split('\n')
            .map(|line| {
                let visual = unicode_width::UnicodeWidthStr::width(line);
                (visual / width) + 1
            })
            .sum::<usize>()
            .max(1);
        lines.min(MAX_COMPOSER_LINES)
    }

    pub(super) fn composer_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let segments = self.composer.text().split('\n').collect::<Vec<_>>();
        for (idx, segment) in segments.iter().enumerate() {
            let prompt = if idx == 0 {
                if self.pending_yield.is_some() {
                    "» "
                } else {
                    "> "
                }
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                Span::styled(prompt, Style::default().fg(Color::Green)),
                Span::raw((*segment).to_string()),
            ]));
        }
        if lines.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                if self.pending_yield.is_some() {
                    "» "
                } else {
                    "> "
                },
                Style::default().fg(Color::Green),
            )]));
        }
        lines
    }

    pub(super) fn hint_line(&self) -> Line<'static> {
        let hint = match &self.pending_yield {
            Some(pending)
                if matches!(pending.kind, YieldKind::Confirmation)
                    && !pending.options.is_empty() =>
            {
                let choices = pending
                    .options
                    .iter()
                    .enumerate()
                    .map(|(idx, option)| format!("{}={option}", idx + 1))
                    .collect::<Vec<_>>()
                    .join(" · ");
                format!("{choices}  · or type a reply and press Enter")
            }
            Some(_) => "reply and press Enter".to_string(),
            None => "enter send · shift+enter newline · / commands · ctrl+r thinking · ctrl+q quit"
                .to_string(),
        };
        Line::styled(hint, Style::default().fg(Color::DarkGray))
    }
}
