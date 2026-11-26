//! File-backed TUI input handling and application state transitions.
use std::collections::BTreeMap;

use alan_agent_protocol::{
    InputIntent, UiActivitySnapshot, UiActivityState, UiEvent, UiInputStatus, UiNoticeKind,
    UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot, UiThinkingState, YieldKind,
};
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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
use super::{ProjectAccess, ProjectControl, ProjectMountReceipt, project_dispatch};

pub(in crate::file_backed) mod action_modal;
mod attachment;
pub(in crate::file_backed) mod form_control;
mod grouping;
mod history;
mod image_paste;
mod model_control;
mod mouse;
mod notice;
pub(in crate::file_backed) use notice::{Notice, humanize_protocol_message};
pub(in crate::file_backed) mod presentation;
mod project;
mod slash;
mod submit;
use project::default_commands;

mod events;
pub(super) use events::{FileBackedAction, FileBackedEvent};

#[derive(Clone)]
pub(super) struct FileBackedApp {
    pub(super) tape_consumed_offset: usize,
    pub(super) agent_path: String,
    pub(super) model_chooser: model_control::ModelChooser,
    pub(super) resume_picker: super::resume_picker::ResumePicker,
    pub(super) fork_picker: super::fork_picker::ForkPicker,
    pub(super) login: super::login::LoginFlowState,
    pub(super) login_available: bool,
    pub(super) settings: super::settings_flow::SettingsFlowState,
    pub(super) settings_available: bool,
    pub(super) issue: super::issue_flow::IssueFlowState,
    pub(super) issue_available: bool,
    pub(super) issue_repo: String,
    pub(super) pending_login_submission: Option<super::login::LoginSubmission>,
    pub(super) connection_configured: Option<bool>,
    pub(super) turn_prompts: Vec<(String, bool)>,
    pub(super) rollout_switching: bool,
    pub(super) last_summary_revision: Option<u64>,
    pub(super) model: super::model::ModelProjection,
    pub(super) usage: super::usage::UsageProjection,
    pub(super) budget: super::budget::BudgetProjection,
    pub(super) status_diagnostics: super::status_panel::StatusDiagnostics,
    pub(super) cow_branch_count: usize,
    pub(super) ephemeral: bool,
    pub(super) no_resume_update: bool,
    pub(super) last_frame_origin: std::cell::Cell<(u16, u16)>,
    pub(super) last_menu_origin_y: std::cell::Cell<Option<u16>>,
    pub(super) effective_model: Option<String>,
    pub(super) composer: Composer,
    pub(super) input_intent: InputIntent,
    pub(super) history_draft_intent: Option<InputIntent>,
    pub(super) transcript: Vec<HistoryCell>,
    pub(super) action_cells: BTreeMap<String, usize>,
    /// Existing observation fingerprint plus whether its rows were committed.
    pub(super) projected_actions: BTreeMap<(String, String), (u64, bool)>,
    pub(super) modal: action_modal::ActionModal,
    pub(super) local_inputs: BTreeMap<String, super::queue::LocalInput>,
    pub(super) skills: super::skills::SkillProjection,
    pub(super) queue: super::queue::QueueProjection,
    pub(super) activity: UiActivitySnapshot,
    pub(super) plan: UiPlanSnapshot,
    pub(super) plan_revision: usize,
    pub(super) plan_owners: Vec<String>,
    pub(super) thinking: UiThinkingSnapshot,
    pub(super) running_tools: Vec<RunningTool>,
    pub(super) pending_yield: Option<PendingYieldCell>,
    pub(super) response_in_flight: Option<String>,
    pub(super) form: Option<FormState>,
    pub(super) confirmation: Option<super::confirmation::ConfirmationState>,
    pub(super) staged_confirmation_outcome: Option<super::confirmation::ConfirmationOutcomeDraft>,
    pub(super) viewport_size: (u16, u16),
    pub(super) completion: Option<CompletionState>,
    pub(super) completion_sources: CompletionSources,
    pub(super) project_candidate: Option<std::path::PathBuf>,
    pub(super) project_selection: Option<ProjectAccess>,
    pub(super) project: Option<ProjectMountReceipt>,
    pub(super) project_host_root: Option<std::path::PathBuf>,
    pub(super) git_status: Option<super::git_status::GitStatus>,
    pub(super) sandbox_backend: Option<String>,
    pub(super) trust_posture_label: Option<String>,
    pub(super) update_notice: Option<String>,
    pub(super) release_channel: Option<String>,
    pub(super) pending_attachments: Vec<alan_agent_protocol::ContentPart>,
    pub(super) attachment_revision: u64,
    pub(super) namespace_cwd: std::path::PathBuf,
    pub(super) retained_project_mount: Option<(ProjectMountReceipt, std::path::PathBuf)>,
    pub(super) pending_project_control: Option<project::PendingProjectControl>,
    pub(super) project_cleanup: Option<String>,
    pub(super) project_cleanup_attempted: bool,
    pub(super) project_host_pending: bool,
    pub(super) uncertain_project_mount: Option<(String, ProjectControl)>,
    ready_project_revoke: Option<String>,
    pub(super) streams: super::streams::StreamsState,
    pub(super) last_input_failed: bool,
    pub(super) auto_continue_on_empty_cancel: bool,
    pub(super) expand_thinking: bool,
    pub(super) theme: crate::theme::Theme,
    pub(super) notice: Option<Notice>,
    pub(super) last_diagnostic_notice: Option<String>,
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
    /// Single-turn latch allowing a late `UiThinkingState::Complete` from the
    /// just-closed live turn to insert before its `Assistant` cell when
    /// `machine/tape` beat `machine/ui/events`, without ever claiming a
    /// subsequent turn's thinking or a hydrated session's assistant cell.
    pub(super) closed_assistant_accepts_late_thinking: bool,
    /// Set once `machine/ui/events` closes the current turn (`InputCompleted`
    /// or `Running -> Idle`), cleared on the next `user` tape boundary so a
    /// late `assistant` tape record cannot arm the latch for a future turn.
    pub(super) ui_turn_closed: bool,
    /// Whether the active turn has already opened a `user` boundary in this attachment,
    /// surviving scrollback pruning and interactive confirmation yields until the turn closes.
    pub(super) active_turn_has_user_boundary: bool,
    pub(super) scrollback_front_is_partial: bool,
}

impl FileBackedApp {
    pub(super) fn new(agent_path: String) -> Self {
        Self {
            tape_consumed_offset: 0,
            notice: None,
            last_diagnostic_notice: None,
            expected_terminal_error: None,
            agent_path,
            model_chooser: model_control::ModelChooser::default(),
            resume_picker: super::resume_picker::ResumePicker::default(),
            fork_picker: super::fork_picker::ForkPicker::default(),
            login: super::login::LoginFlowState::default(),
            login_available: false,
            settings: super::settings_flow::SettingsFlowState::default(),
            settings_available: false,
            issue: super::issue_flow::IssueFlowState::default(),
            issue_available: false,
            issue_repo: super::issue_flow::DEFAULT_ISSUE_REPO.to_string(),
            pending_login_submission: None,
            connection_configured: None,
            turn_prompts: Vec::new(),
            rollout_switching: false,
            last_summary_revision: None,
            model: super::model::ModelProjection::default(),
            usage: super::usage::UsageProjection::default(),
            budget: super::budget::BudgetProjection::default(),
            status_diagnostics: super::status_panel::StatusDiagnostics::default(),
            cow_branch_count: 0,
            ephemeral: false,
            no_resume_update: false,
            last_frame_origin: std::cell::Cell::new((0, 0)),
            last_menu_origin_y: std::cell::Cell::new(None),
            effective_model: None,
            composer: Composer::default(),
            input_intent: InputIntent::Agent,
            history_draft_intent: None,
            transcript: Vec::new(),
            action_cells: BTreeMap::new(),
            projected_actions: BTreeMap::new(),
            modal: action_modal::ActionModal::default(),
            local_inputs: BTreeMap::new(),
            skills: super::skills::SkillProjection::default(),
            queue: super::queue::QueueProjection::default(),
            activity: UiActivitySnapshot::idle(),
            plan: UiPlanSnapshot::empty(),
            plan_revision: 0,
            plan_owners: Vec::new(),
            thinking: UiThinkingSnapshot::idle(),
            running_tools: Vec::new(),
            pending_yield: None,
            response_in_flight: None,
            form: None,
            confirmation: None,
            staged_confirmation_outcome: None,
            viewport_size: (80, 24),
            completion: None,
            completion_sources: CompletionSources {
                commands: default_commands(),
                ..CompletionSources::default()
            },
            project_candidate: None,
            project_selection: None,
            project: None,
            project_host_root: None,
            git_status: None,
            sandbox_backend: None,
            trust_posture_label: None,
            update_notice: None,
            release_channel: None,
            pending_attachments: Vec::new(),
            attachment_revision: 0,
            namespace_cwd: std::path::PathBuf::from("/"),
            retained_project_mount: None,
            pending_project_control: None,
            project_cleanup: None,
            project_cleanup_attempted: false,
            project_host_pending: false,
            uncertain_project_mount: None,
            ready_project_revoke: None,
            streams: super::streams::StreamsState::default(),
            last_input_failed: false,
            auto_continue_on_empty_cancel: false,
            expand_thinking: false,
            theme: crate::theme::Theme::default(),
            should_quit: false,
            reconciler: StreamReconciler::new(),
            pending_remote_turn_start: None,
            closed_assistant_accepts_late_thinking: false,
            ui_turn_closed: false,
            active_turn_has_user_boundary: false,
            scrollback_front_is_partial: false,
        }
    }

    pub(super) fn set_effective_model(&mut self, model: Option<String>) {
        self.effective_model = model;
    }

    pub(super) fn set_file_candidates(&mut self, files: Vec<CompletionCandidate>) {
        self.completion_sources.files = files;
    }

    #[cfg(test)]
    pub(super) fn dispatch(&mut self, event: FileBackedEvent) -> Option<FileBackedAction> {
        self.dispatch_with_pending_submission(event, false)
    }

    pub(super) fn dispatch_with_pending_submission(
        &mut self,
        event: FileBackedEvent,
        has_pending_submission: bool,
    ) -> Option<FileBackedAction> {
        match event {
            FileBackedEvent::PlanDetails {
                path,
                generation,
                entries,
            } => {
                self.apply_plan_details(path, generation, entries);
                None
            }
            FileBackedEvent::TapeDetails {
                path,
                generation,
                rows,
            } => {
                if self.modal.active
                    && self.modal.mode == action_modal::ModalMode::Tape
                    && path == self.modal.owner_path
                    && generation == self.modal.generation
                {
                    self.modal.rows = rows;
                    self.modal.scroll = 0;
                }
                None
            }
            FileBackedEvent::StatusPanelLoaded {
                generation,
                diagnostics,
            } => {
                self.cow_branch_count = diagnostics.cow_branches.len();
                self.status_diagnostics = diagnostics;
                if self.modal.active
                    && self.modal.mode == action_modal::ModalMode::Status
                    && generation == self.modal.generation
                {
                    self.modal.rows = super::status_panel::build_rows(self);
                }
                None
            }
            FileBackedEvent::CowBranchesUpdated { count } => {
                self.cow_branch_count = count;
                None
            }
            FileBackedEvent::ManDetails {
                generation,
                owner,
                content,
            } => {
                self.apply_man_details(generation, owner, content);
                None
            }
            FileBackedEvent::HistoryCatalogLoaded { generation, result } => {
                self.resume_picker.apply_loaded(generation, result);
                None
            }
            FileBackedEvent::HistorySwitchCompleted { rollout_id, result } => {
                match result {
                    Ok(()) => self.reset_for_rollout_switch(rollout_id.as_deref()),
                    Err(error) => self.push_error(format!("rollout switch failed: {error}")),
                }
                None
            }
            FileBackedEvent::ProjectSummaryResult {
                action_label,
                result,
            } => {
                self.apply_summary_result(action_label, result);
                None
            }
            FileBackedEvent::WorkstreamControlCompleted {
                show_overview,
                result,
            } => {
                self.apply_workstream_control_result(show_overview, result);
                None
            }
            FileBackedEvent::RouteLogLine(line) => {
                self.apply_route_log_line(&line);
                None
            }
            FileBackedEvent::FocusTargetResolved {
                requested_name,
                result,
            } => {
                self.apply_focus_target_resolved(requested_name, result);
                None
            }
            FileBackedEvent::LoginProvidersLoaded { generation, result } => {
                self.apply_login_providers(generation, result);
                None
            }
            FileBackedEvent::LoginProgress {
                generation,
                progress,
            } => {
                self.apply_login_progress(generation, progress);
                None
            }
            FileBackedEvent::LoginStepCompleted { generation, result } => {
                self.apply_login_step_result(generation, result);
                None
            }
            FileBackedEvent::SettingsCompleted { generation, result } => {
                let (_, next_action) = self.apply_settings_result(generation, result);
                next_action
            }
            FileBackedEvent::IssueDrafted { generation, result } => {
                self.apply_issue_draft_result(generation, result);
                None
            }
            FileBackedEvent::IssueSubmitted { generation, result } => {
                self.apply_issue_submit_result(generation, result);
                None
            }
            FileBackedEvent::ActionDetails {
                path,
                generation,
                actions,
                selected,
                rows,
            } => {
                if self.modal.active
                    && self.modal.mode == action_modal::ModalMode::Actions
                    && path == self.modal.owner_path
                    && generation == self.modal.generation
                {
                    match actions {
                        Ok(actions) => {
                            self.modal.selected = selected
                                .as_ref()
                                .and_then(|selected| actions.iter().position(|v| v == selected))
                                .unwrap_or(0);
                            self.modal.actions = actions;
                            self.modal.rows = rows;
                        }
                        Err(error) => self.modal.rows = vec![Line::from(error)],
                    }
                }
                None
            }
            FileBackedEvent::Terminal(TerminalEvent::Key(key)) => {
                if let Some(notice) = &self.notice
                    && (notice.kind != UiNoticeKind::None || notice.runtime_owned)
                    && !notice.trim().is_empty()
                {
                    self.last_diagnostic_notice = Some(notice.to_string());
                }
                self.handle_key_with_pending_submission(key, has_pending_submission)
            }
            FileBackedEvent::Terminal(TerminalEvent::Paste(text)) => {
                if self.login.is_entering_input() {
                    self.login_paste(&text);
                    return None;
                }
                if self.issue.is_editing() {
                    self.issue_paste(&text);
                    return None;
                }
                if self.modal.active
                    || self.model_chooser.active
                    || self.resume_picker.active
                    || self.fork_picker.active
                    || self.login.is_active()
                    || self.settings.is_active()
                    || self.issue.is_active()
                {
                    return None;
                }
                if self.project_selection.is_some() {
                    let text = text
                        .chars()
                        .filter(|ch| !ch.is_control())
                        .collect::<String>();
                    self.composer.insert_text(&text);
                } else if self.handle_confirmation_paste(&text) {
                } else if let Some(form) = self.form.as_mut() {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        form.insert_char(ch);
                    }
                } else if self.pending_yield.is_none() && self.input_intent != InputIntent::Command
                {
                    self.handle_composer_paste(&text);
                } else {
                    self.insert_input_text(&text);
                    self.refresh_completion();
                }
                None
            }
            FileBackedEvent::Terminal(TerminalEvent::Resize(cols, rows)) => {
                self.viewport_size = (cols.max(1), rows.max(1));
                None
            }
            FileBackedEvent::Terminal(TerminalEvent::Mouse(mouse)) => {
                self.handle_mouse(mouse, has_pending_submission)
            }
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
                    Ok(()) => {
                        if !success_notice.is_empty()
                            && !self.login.is_active()
                            && !self.settings.is_active()
                        {
                            self.notice = Some(success_notice.into());
                        }
                    }
                    Err(error) => self.push_error(format!("{error_prefix}: {error}")),
                }
                None
            }
            FileBackedEvent::ModelReceiptsUnavailable { owner } => {
                self.lose_model_receipts(&owner);
                None
            }
            FileBackedEvent::ModelReceipt { owner, event } => {
                self.observe_model_receipt(&owner, &event);
                None
            }
            FileBackedEvent::Ui(event) => {
                match &event {
                    UiEvent::Notice { snapshot }
                        if snapshot.kind != UiNoticeKind::None
                            && !snapshot.message.trim().is_empty() =>
                    {
                        self.last_diagnostic_notice =
                            Some(notice::humanize_protocol_message(&snapshot.message));
                    }
                    UiEvent::Error { message, .. } if !message.trim().is_empty() => {
                        self.last_diagnostic_notice =
                            Some(notice::humanize_protocol_message(message));
                    }
                    UiEvent::InputCompleted {
                        status: UiInputStatus::Failed,
                        error: Some(message),
                        ..
                    } if !message.trim().is_empty() => {
                        self.last_diagnostic_notice =
                            Some(notice::humanize_protocol_message(message));
                    }
                    _ => {}
                }
                self.apply_ui_event(event);
                None
            }
            FileBackedEvent::Tape(record) => {
                self.apply_tape_record(record);
                None
            }
            FileBackedEvent::ModelSelectionWritten {
                owner,
                id,
                success,
                owner_current,
            } => {
                super::model::finish_selection(self, &owner, &id, success, owner_current);
                None
            }
            FileBackedEvent::ObservationRead { .. }
            | FileBackedEvent::ProjectHostCompleted { .. }
            | FileBackedEvent::ProjectCwdWritten { .. }
            | FileBackedEvent::ProjectGrantObserved { .. }
            | FileBackedEvent::ProjectGrantLocated { .. }
            | FileBackedEvent::QueueChanged { .. }
            | FileBackedEvent::QueueUnavailable { .. }
            | FileBackedEvent::RootAgentPidRefresh(_)
            | FileBackedEvent::ModelChanged { .. }
            | FileBackedEvent::ModelUnavailable { .. }
            | FileBackedEvent::SkillsChanged { .. }
            | FileBackedEvent::SkillsUnavailable { .. }
            | FileBackedEvent::UsageChanged { .. }
            | FileBackedEvent::UsageUnavailable { .. }
            | FileBackedEvent::BudgetChanged { .. }
            | FileBackedEvent::BudgetUnavailable { .. }
            | FileBackedEvent::GitStatusUpdated { .. }
            | FileBackedEvent::FileIndexUpdated { .. }
            | FileBackedEvent::RequestsChanged
            | FileBackedEvent::ActionsChanged { .. } => None,
            FileBackedEvent::Error(message) | FileBackedEvent::TerminalError(message) => {
                self.last_diagnostic_notice = Some(notice::humanize_protocol_message(&message));
                self.push_error(message);
                None
            }
        }
    }

    #[cfg(test)]
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        self.handle_key_with_pending_submission(key, false)
    }

    fn handle_key_with_pending_submission(
        &mut self,
        key: KeyEvent,
        has_pending_submission: bool,
    ) -> Option<FileBackedAction> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        if self.model_chooser.active {
            return self.model_key(key, has_pending_submission);
        }
        if self.resume_picker.active {
            return self.resume_picker_key(key);
        }
        if self.fork_picker.active {
            return self.fork_picker_key(key);
        }
        if self.login.is_active() {
            return self.login_key(key);
        }
        if self.settings.is_active() {
            return self.settings_key(key);
        }
        if self.issue.is_active() {
            return self.issue_key(key);
        }
        if self.modal_key(key, has_pending_submission) {
            return None;
        }
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
            // A successfully written input can precede its Running UI snapshot.
            // Admission truth comes from the caller's existing pending queue.
            if !has_pending_submission && self.pending_yield.is_none() && !self.turn_active() {
                if !self.composer.text().is_empty() {
                    self.composer.set_text("");
                    self.input_intent = InputIntent::Agent;
                    self.notice = Some("draft cleared".into());
                }
                return None;
            }
            return Some(self.interrupt_action());
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
        } else if self.completion.is_some()
            && ((key.code == KeyCode::Esc
                && !self.interruptible_work_active(has_pending_submission)
                && {
                    self.completion = None;
                    true
                })
                || (key.code != KeyCode::Esc && self.consume_completion_key(key)))
        {
            return None;
        }
        if self.form.is_some() {
            return self.handle_form_key(key);
        }
        match self.handle_confirmation_key(key) {
            form_control::ConfirmationKeyResult::Action(action) => return Some(action),
            form_control::ConfirmationKeyResult::Swallowed => return None,
            form_control::ConfirmationKeyResult::Ignored => {}
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
                code: KeyCode::Char('z'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
            {
                Some(FileBackedAction::Suspend)
            }
            KeyEvent {
                code: KeyCode::BackTab,
                modifiers,
                ..
            }
            | KeyEvent {
                code: KeyCode::Tab,
                modifiers,
                ..
            } if !pending_input
                && self.completion.is_none()
                && !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT)
                && (key.code == KeyCode::BackTab || modifiers.contains(KeyModifiers::SHIFT)) =>
            {
                self.toggle_trust_posture()
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
                code: KeyCode::Char('g'),
                modifiers,
                ..
            } if !pending_input
                && modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
            {
                Some(FileBackedAction::OpenExternalEditor)
            }
            KeyEvent {
                code: KeyCode::Char('s'),
                modifiers,
                ..
            } if !pending_input
                && modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.submit_with_mode(alan_agent_protocol::InputMode::Steer)
            }
            KeyEvent {
                code: KeyCode::Char('v'),
                modifiers,
                ..
            } if !pending_input
                && self.confirmation.is_none()
                && self.input_intent != InputIntent::Command
                && modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.attach_from_clipboard();
                None
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                if self.interruptible_work_active(has_pending_submission) {
                    Some(self.interrupt_action())
                } else {
                    self.notice = None;
                    None
                }
            }
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
                    && !key.modifiers.contains(KeyModifiers::ALT)
                    && self.composer.text().is_empty()
                {
                    if self.input_intent != InputIntent::Command
                        && !self.pending_attachments.is_empty()
                    {
                        self.pop_last_pending_attachment();
                        return None;
                    }
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
        let was_recalling = self.composer.is_recalling();
        if !was_recalling {
            self.history_draft_intent = Some(self.input_intent);
        }
        self.composer.handle_key(key);
        if let Some(intent) = self.composer.recalled_intent() {
            self.input_intent = intent;
        } else if was_recalling && let Some(intent) = self.history_draft_intent.take() {
            self.input_intent = intent;
        } else if !was_recalling {
            self.history_draft_intent = None;
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

    pub(super) fn turn_active(&self) -> bool {
        !matches!(self.activity.state, UiActivityState::Idle)
    }

    pub(super) fn activity_started_at_ms(&self) -> Option<u64> {
        self.activity.started_at_ms
    }

    #[cfg(test)]
    pub(super) fn rendered_history_lines(&self, width: usize) -> Vec<String> {
        self.styled_history_lines(width)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    pub(super) fn styled_history_lines(&self, width: usize) -> Vec<Line<'static>> {
        let opts = self.render_opts(width);
        self.history_row_projection(opts)
            .into_iter()
            .flatten()
            .collect()
    }

    pub(super) fn drain_committed_scrollback(
        &mut self,
        viewport_width: usize,
        viewport_height: usize,
    ) -> Vec<Line<'static>> {
        if self.modal.active || self.resume_picker.active || self.issue.is_active() {
            return Vec::new();
        }
        let opts = self.render_opts(viewport_width);
        let max_lines =
            viewport_height.saturating_sub(super::layout::base_region_height(self, viewport_width));
        let lines = self.styled_history_lines(viewport_width);
        let drain_count = super::history_prefix_to_drain(&lines, viewport_width, max_lines);
        if drain_count == 0 {
            return Vec::new();
        }
        let pruned_count = self.prune_rendered_prefix(opts, drain_count);
        lines.into_iter().take(pruned_count).collect()
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
    pub(super) fn maybe_insert_welcome_header(&mut self) {
        if self.transcript.is_empty() && !super::welcome::is_disabled() {
            self.transcript
                .push(HistoryCell::Welcome(super::welcome::from_app(self)));
        }
    }

    pub(super) fn render_opts(&self, width: usize) -> RenderOpts {
        RenderOpts::new(width, self.expand_thinking).with_theme(self.theme)
    }
}

#[cfg(test)]
#[path = "image_paste_tests.rs"]
mod image_paste_tests;
#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
