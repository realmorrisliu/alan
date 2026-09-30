//! File-backed TUI input handling and application state transitions.

use std::collections::BTreeMap;

use alan_agent_protocol::{
    InputIntent, UiActivitySnapshot, UiActivityState, UiEvent, UiInputStatus, UiNoticeKind,
    UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot, UiThinkingState, YieldKind,
};
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::completion::{self, CompletionCandidate, CompletionSources, CompletionState};
use crate::composer::{Composer, ComposerKeyOutcome};
use crate::form::FormState;
use crate::history::{HistoryCell, PendingYieldCell, RenderOpts, RunningTool};
use crate::reconcile::{AssistantDecision, StreamAction, StreamReconciler};
use crate::transcript_ui::{
    INLINE_COMMAND_PROMPT_PREFIX, INLINE_PROMPT_PREFIX, INLINE_WAITING_PROMPT_PREFIX,
};

use super::file_surface::{ActionSnapshot, TapeRecordV1, response_text_from_content};
use super::{ProjectAccess, ProjectControl, ProjectMountReceipt};

mod history;
mod presentation;
mod project;
fn default_commands() -> Vec<CompletionCandidate> {
    [
        ("project", "select or revoke a project directory"),
        ("compact", "summarize context"),
        ("rollback", "undo the last turn"),
        ("continue", "continue paused input"),
        ("discard", "discard paused input"),
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
    ResumeWriteCompleted {
        request_id: String,
        retry_input: String,
        result: Result<(), String>,
    },
    ControlWriteCompleted {
        success_notice: String,
        error_prefix: String,
        result: Result<(), String>,
    },
    RootAgentPidRefresh(Result<Option<u64>, String>),
    RequestsChanged,
    ActionsChanged {
        agent_path: String,
        action_id: String,
    },
    Ui(UiEvent),
    Tape(TapeRecordV1),
    Error(String),
    TerminalError(String),
}

#[derive(Debug)]
pub(super) enum FileBackedAction {
    Submit(alan_agent_protocol::UserInputRecord),
    Resume {
        request_id: String,
        response: String,
        retry_input: String,
    },
    MachineCtl {
        command: String,
        success_notice: String,
    },
    Project(ProjectControl),
    Interrupt,
    Quit,
}

#[derive(Clone)]
pub(super) struct FileBackedApp {
    pub(super) tape_consumed_offset: usize,
    pub(super) agent_path: String,
    pub(super) effective_model: Option<String>,
    pub(super) composer: Composer,
    pub(super) input_intent: InputIntent,
    history_draft_intent: Option<InputIntent>,
    pub(super) transcript: Vec<HistoryCell>,
    pub(super) action_cells: BTreeMap<String, usize>,
    pub(super) activity: UiActivitySnapshot,
    pub(super) plan: UiPlanSnapshot,
    pub(super) thinking: UiThinkingSnapshot,
    pub(super) running_tools: Vec<RunningTool>,
    pub(super) pending_yield: Option<PendingYieldCell>,
    response_in_flight: Option<String>,
    pub(super) form: Option<FormState>,
    pub(super) completion: Option<CompletionState>,
    pub(super) completion_sources: CompletionSources,
    pub(super) project_candidate: Option<std::path::PathBuf>,
    pub(super) project_selection: Option<ProjectAccess>,
    pub(super) project: Option<ProjectMountReceipt>,
    pub(super) namespace_cwd: std::path::PathBuf,
    pub(super) pending_project_cwd: Option<String>,
    pending_project_cwd_submission_id: Option<String>,
    pending_project_revoke: Option<(String, String)>,
    ready_project_revoke: Option<String>,
    pub(super) last_input_failed: bool,
    pub(super) expand_thinking: bool,
    pub(super) notice: Option<String>,
    pub(super) expected_terminal_error: Option<String>,
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
    pub(super) scrollback_front_is_partial: bool,
}

impl FileBackedApp {
    pub(super) fn new(agent_path: String) -> Self {
        Self {
            tape_consumed_offset: 0,
            notice: None,
            expected_terminal_error: None,
            agent_path,
            effective_model: None,
            composer: Composer::default(),
            input_intent: InputIntent::Agent,
            history_draft_intent: None,
            transcript: Vec::new(),
            action_cells: BTreeMap::new(),
            activity: UiActivitySnapshot::idle(),
            plan: UiPlanSnapshot::empty(),
            thinking: UiThinkingSnapshot::idle(),
            running_tools: Vec::new(),
            pending_yield: None,
            response_in_flight: None,
            form: None,
            completion: None,
            completion_sources: CompletionSources {
                commands: default_commands(),
                ..CompletionSources::default()
            },
            project_candidate: None,
            project_selection: None,
            project: None,
            namespace_cwd: std::path::PathBuf::from("/"),
            pending_project_cwd: None,
            pending_project_cwd_submission_id: None,
            pending_project_revoke: None,
            ready_project_revoke: None,
            last_input_failed: false,
            expand_thinking: false,
            should_quit: false,
            reconciler: StreamReconciler::new(),
            pending_remote_turn_start: None,
            scrollback_front_is_partial: false,
        }
    }

    pub(super) fn set_skill_candidates(&mut self, skills: Vec<CompletionCandidate>) {
        self.completion_sources.skills = skills;
    }

    pub(super) fn set_effective_model(&mut self, model: Option<String>) {
        self.effective_model = model;
    }

    pub(super) fn set_file_candidates(&mut self, files: Vec<CompletionCandidate>) {
        self.completion_sources.files = files;
    }

    pub(super) fn dispatch(&mut self, event: FileBackedEvent) -> Option<FileBackedAction> {
        match event {
            FileBackedEvent::Terminal(TerminalEvent::Key(key)) => self.handle_key(key),
            FileBackedEvent::Terminal(TerminalEvent::Paste(text)) => {
                if self.project_selection.is_some() {
                    let text = text
                        .chars()
                        .filter(|ch| !ch.is_control())
                        .collect::<String>();
                    self.composer.insert_text(&text);
                } else if let Some(form) = self.form.as_mut() {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        form.insert_char(ch);
                    }
                } else {
                    self.insert_input_text(&text);
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
            FileBackedEvent::ResumeWriteCompleted {
                request_id,
                retry_input,
                result,
            } => {
                self.finish_resume_write(&request_id, &retry_input, result);
                None
            }
            FileBackedEvent::ControlWriteCompleted {
                success_notice,
                error_prefix,
                result,
            } => {
                match result {
                    Ok(()) => self.notice = Some(success_notice),
                    Err(error) => self.push_error(format!("{error_prefix}: {error}")),
                }
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
            FileBackedEvent::RootAgentPidRefresh(_)
            | FileBackedEvent::RequestsChanged
            | FileBackedEvent::ActionsChanged { .. } => None,
            FileBackedEvent::Error(message) | FileBackedEvent::TerminalError(message) => {
                self.push_error(message);
                None
            }
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        let pending_input =
            self.form.is_some() || self.pending_yield.is_some() || self.project_selection.is_some();
        if key.code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if !pending_input && self.composer.text().is_empty() {
                self.should_quit = true;
                return Some(FileBackedAction::Quit);
            }
            return None;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.project_selection.is_some() {
                self.cancel_project_selection();
                return None;
            }
            if self.pending_yield.is_none() && !self.turn_active() {
                if !self.composer.text().is_empty() {
                    self.composer.set_text("");
                    self.input_intent = InputIntent::Agent;
                    self.notice = Some("draft cleared".into());
                }
                return None;
            }
            return Some(FileBackedAction::Interrupt);
        }
        if self.project_selection.is_some() {
            return self.handle_project_key(key);
        }
        if pending_input
            || self.input_intent == InputIntent::Command
            || (self.input_intent == InputIntent::ForceAgent
                && self
                    .completion
                    .as_ref()
                    .is_some_and(|state| state.kind == completion::CompletionKind::Command))
        {
            self.completion = None;
        } else if key.code == KeyCode::Enter
            && self
                .completion
                .as_ref()
                .is_some_and(|state| state.kind == completion::CompletionKind::Command)
        {
            self.accept_completion();
            return self.handle_submit();
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
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                self.insert_input_text(&ch.to_string());
                self.refresh_completion();
                None
            }
            _ => {
                if matches!(key.code, KeyCode::Up | KeyCode::Down) {
                    self.recall_input(key);
                    self.refresh_completion();
                    return None;
                }
                if self.pending_yield.is_none()
                    && key.code == KeyCode::Backspace
                    && self.composer.text().is_empty()
                {
                    self.input_intent = InputIntent::Agent;
                }
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

    pub(super) fn insert_input_text(&mut self, text: &str) {
        let text = if self.pending_yield.is_none() && self.input_intent == InputIntent::Agent {
            if self.composer.text().is_empty() {
                let (intent, body) = alan_agent_protocol::parse_input_prefix(text);
                self.input_intent = intent;
                body
            } else {
                if self.composer.cursor() == 0 && text.starts_with(['!', ':']) {
                    self.input_intent = InputIntent::ForceAgent;
                }
                text
            }
        } else {
            text
        };
        self.composer.insert_text(text);
    }

    fn recall_input(&mut self, key: KeyEvent) {
        if !self.composer.is_recalling() {
            self.history_draft_intent = Some(self.input_intent);
        }
        self.composer.handle_key(key);
        if let Some(intent) = self.composer.recalled_intent() {
            self.input_intent = intent;
        } else if let Some(intent) = self.history_draft_intent.take() {
            self.input_intent = intent;
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
        if self.pending_yield.is_some() || self.input_intent == InputIntent::Command {
            self.completion = None;
            return;
        }
        self.completion = completion::detect(
            self.composer.text(),
            self.composer.cursor(),
            &self.completion_sources,
        )
        .filter(|state| {
            self.input_intent != InputIntent::ForceAgent
                || state.kind != completion::CompletionKind::Command
        });
    }

    pub(super) fn submit_form(&mut self) -> Option<FileBackedAction> {
        if self.response_in_flight.is_some() {
            self.notice = Some("request response is still being sent".into());
            return None;
        }
        let pending = self.pending_yield.clone()?;
        let form = self.form.as_mut()?;
        let retry_input = form.answers_json();
        match pending.resume_content(&retry_input) {
            Ok(content) => Some(FileBackedAction::Resume {
                request_id: pending.request_id,
                response: response_text_from_content(content),
                retry_input,
            }),
            Err(message) => {
                form.error = Some(message);
                None
            }
        }
    }

    pub(super) fn confirmation_keypress(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        if self.response_in_flight.is_some() {
            return None;
        }
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
                retry_input: option,
            }),
            Err(message) => {
                self.notice = Some(message);
                None
            }
        }
    }

    pub(super) fn handle_submit(&mut self) -> Option<FileBackedAction> {
        if self.response_in_flight.is_some() {
            self.notice = Some("request response is still being sent".into());
            return None;
        }
        if self.project_selection.is_some() {
            let path = self.composer.text().trim();
            if path.is_empty() {
                self.notice = Some("enter a project directory path".into());
                return None;
            }
            return Some(FileBackedAction::Project(ProjectControl::Mount {
                host_path: std::path::PathBuf::from(path),
                access: self.project_selection.unwrap_or(ProjectAccess::ReadOnly),
            }));
        }
        if let Some(pending) = self.pending_yield.clone() {
            let text = self.composer.text().trim().to_string();
            self.completion = None;
            match pending.resume_content(&text) {
                Ok(content) => {
                    self.composer.set_text("");
                    self.input_intent = InputIntent::Agent;
                    self.history_draft_intent = None;
                    return Some(FileBackedAction::Resume {
                        request_id: pending.request_id,
                        response: response_text_from_content(content),
                        retry_input: text,
                    });
                }
                Err(message) => {
                    self.notice = Some(message);
                    return None;
                }
            }
        }

        let text = self.composer.text().to_owned();
        if text.trim().is_empty() {
            if self.input_intent != InputIntent::Agent {
                self.notice = Some("Enter content after the input prefix.".into());
            }
            return None;
        }
        self.notice = None;
        self.completion = None;
        if self.input_intent == InputIntent::Agent && text.trim().starts_with('/') {
            self.composer.set_text("");
            self.composer.remember(&text);
            return self.handle_command(text.trim());
        }
        let record = alan_agent_protocol::UserInputRecord::new(
            self.input_intent,
            alan_agent_protocol::InputMode::FollowUp,
            text,
        );
        Some(FileBackedAction::Submit(record))
    }

    pub(super) fn begin_resume_write(&mut self, request_id: String) {
        self.response_in_flight = Some(request_id);
        self.notice = Some("sending response…".into());
    }

    fn finish_resume_write(
        &mut self,
        request_id: &str,
        retry_input: &str,
        result: Result<(), String>,
    ) {
        if self.response_in_flight.as_deref() != Some(request_id) {
            return;
        }
        self.response_in_flight = None;
        match result {
            Ok(()) => {
                if self
                    .pending_yield
                    .as_ref()
                    .is_some_and(|pending| pending.request_id == request_id)
                {
                    self.clear_pending_yield();
                }
                self.notice = Some("response sent".into());
            }
            Err(error) => {
                self.notice = Some(format!("resume failed: {error}"));
                if self
                    .pending_yield
                    .as_ref()
                    .is_some_and(|pending| pending.request_id == request_id)
                    && self.form.is_none()
                {
                    self.composer.set_text(retry_input);
                }
            }
        }
    }

    pub(super) fn accept_input(&mut self) {
        let body = self.composer.text().to_owned();
        self.composer.remember_input(&body, self.input_intent);
        self.composer.set_text("");
        self.input_intent = InputIntent::Agent;
        self.history_draft_intent = None;
        self.last_input_failed = false;
        // Submission may be queued behind another client. Tape owns turn boundaries.
    }

    #[cfg(test)]
    pub(super) fn enter_submits_agent_task(&self) -> bool {
        if self.form.is_some() || self.pending_yield.is_some() || self.completion.is_some() {
            return false;
        }
        let text = self.composer.text().trim();
        !text.is_empty() && (self.input_intent != InputIntent::Agent || !text.starts_with('/'))
    }

    pub(super) fn handle_command(&mut self, text: &str) -> Option<FileBackedAction> {
        let command = text.strip_prefix('/')?;
        let name = command.split_whitespace().next().unwrap_or("");
        match name {
            "project" if command.trim() == "project" => {
                if self.turn_active() || self.pending_yield.is_some() {
                    self.notice =
                        Some("wait for the current Agent turn before selecting a project".into());
                } else if self.project.is_some() {
                    self.notice = Some("revoke the active project before selecting another".into());
                } else {
                    self.project_selection = Some(ProjectAccess::ReadOnly);
                    self.input_intent = InputIntent::Command;
                    self.composer.set_text(
                        self.project_candidate
                            .as_ref()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                    self.notice = Some(format!(
                        "project path · {} · Enter approve · Tab toggle · Esc cancel",
                        ProjectAccess::ReadOnly.label()
                    ));
                }
                None
            }
            "project" if command.trim() == "project revoke" => {
                if self.turn_active() || self.pending_yield.is_some() {
                    self.notice =
                        Some("wait for the current Agent turn before revoking a project".into());
                    None
                } else if let Some(project) = &self.project {
                    Some(FileBackedAction::Project(ProjectControl::Revoke {
                        grant_id: project.grant_id.clone(),
                    }))
                } else {
                    self.notice = Some("no project grant is active".into());
                    None
                }
            }
            "project" => {
                self.notice = Some("usage: /project or /project revoke".into());
                None
            }
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
            "continue" | "discard" if command.trim() == name => {
                Some(FileBackedAction::MachineCtl {
                    command: format!("queue-v1 {name}"),
                    success_notice: format!("queue {name} requested"),
                })
            }
            "clear" => {
                self.transcript.clear();
                self.action_cells.clear();
                self.pending_remote_turn_start = None;
                self.scrollback_front_is_partial = false;
                None
            }
            "help" => {
                self.notice = Some(
                    "`: ` sends an Agent message · `!` runs a shell command · /project opens the picker (read-only by default; Tab toggles read-write; Enter mounts; Esc cancels) · /project revoke · /compact /rollback /continue /discard /clear /quit · Enter runs slash commands; Tab accepts completion · ctrl+r thinking · ctrl+c clears an idle draft or interrupts active work"
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
        let max_lines = viewport_height
            .saturating_sub(super::live_region_height(self, viewport_width) as usize);
        let lines = self.rendered_history_lines(viewport_width);
        let drain_count = super::history_prefix_to_drain(&lines, viewport_width, max_lines);
        if drain_count == 0 {
            return Vec::new();
        }
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
            self.scrollback_front_is_partial = false;
        }

        if remaining > 0
            && let Some(cell) = self.transcript.first_mut()
            && cell.trim_rendered_prefix(opts, remaining)
        {
            pruned += remaining;
            self.scrollback_front_is_partial = true;
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

    pub(super) fn reset_for_root_process_change(&mut self) {
        self.tape_consumed_offset = 0;
        self.action_cells.clear();
        self.activity = UiActivitySnapshot::idle();
        self.plan = UiPlanSnapshot::empty();
        self.thinking = UiThinkingSnapshot::idle();
        self.running_tools.clear();
        self.pending_yield = None;
        self.form = None;
        self.completion = None;
        self.notice = None;
        self.reconciler = StreamReconciler::new();
        self.pending_remote_turn_start = None;
    }

    /// Keep this renderer's earlier transcript while adding the current turn
    /// recovered from a replacement Root Agent Process.
    pub(super) fn merge_reconnected_history(
        &mut self,
        current: Vec<HistoryCell>,
        submitted_input: &str,
        prior_matching_turns: usize,
    ) -> bool {
        super::history_merge::merge_reconnected_history(
            self,
            current,
            submitted_input,
            prior_matching_turns,
        )
    }

    /// Append replacement-process history not already present in this renderer.
    pub(super) fn merge_reconnected_idle_history(&mut self, current: Vec<HistoryCell>) {
        super::history_merge::merge_idle_history(self, current);
    }

    pub(super) fn render_opts(&self, width: usize) -> RenderOpts {
        RenderOpts::new(width, self.expand_thinking)
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
