use alan_protocol::ContentPart;
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::completion::{self, CompletionCandidate, CompletionSources, CompletionState};
use crate::composer::{Composer, ComposerKeyOutcome};
use crate::daemon_client::CreateSession;
use crate::form::FormState;
use crate::history::{RenderOpts, SessionReducer};

/// Maximum visible composer height before it scrolls internally.
const MAX_COMPOSER_LINES: u16 = 10;
/// Maximum completion rows shown at once.
pub const MAX_COMPLETION_ROWS: usize = 6;

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

#[derive(Debug)]
pub enum AppEvent {
    Terminal(TerminalEvent),
    Daemon(Box<alan_protocol::EventEnvelope>),
    Hydrated(SessionHydration),
    Status(String),
    Error(String),
}

#[derive(Debug, PartialEq)]
pub enum AppAction {
    SubmitTurn(String),
    Resume {
        request_id: String,
        content: Vec<ContentPart>,
    },
    Interrupt,
    Compact,
    Rollback(u32),
    Quit,
}

/// A pending-input signal recovered from a reconnect snapshot, enough to let the
/// user answer the outstanding yield.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingHydration {
    pub request_id: String,
    pub kind: String,
}

/// A persisted message from `/history`, rendered into the transcript on attach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HydratedMessage {
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionHydration {
    pub history: Vec<HydratedMessage>,
    pub replay_events: usize,
    pub pending: Option<PendingHydration>,
    /// The original `Yield` event recovered from the buffered event log, carrying
    /// the full payload (form questions / approval command+diff). When present it
    /// reconstructs a fully-resumable prompt; otherwise the minimal `pending`
    /// signal is used as a fallback.
    pub pending_event: Option<Box<alan_protocol::EventEnvelope>>,
}

impl SessionHydration {
    pub fn from_values(history: &serde_json::Value, reconnect: &serde_json::Value) -> Self {
        // `/history` returns `{ messages: [{ role, content, tool_name? }] }`.
        let history = history
            .get("messages")
            .and_then(serde_json::Value::as_array)
            .map(|messages| {
                messages
                    .iter()
                    .filter_map(|message| {
                        Some(HydratedMessage {
                            role: message
                                .get("role")
                                .and_then(serde_json::Value::as_str)?
                                .to_string(),
                            content: message
                                .get("content")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            tool_name: message
                                .get("tool_name")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // The reconnect snapshot exposes a pending yield under
        // `notifications.signals[] { signal_type, request_id, yield_kind }` — both
        // `pending_yield` (confirmation/custom) and `pending_structured_input`.
        let pending = reconnect
            .get("notifications")
            .and_then(|n| n.get("signals"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .find(|signal| {
                matches!(
                    signal
                        .get("signal_type")
                        .and_then(serde_json::Value::as_str),
                    Some("pending_yield" | "pending_structured_input")
                )
            })
            .and_then(|signal| {
                Some(PendingHydration {
                    request_id: signal
                        .get("request_id")
                        .and_then(serde_json::Value::as_str)?
                        .to_string(),
                    kind: signal
                        .get("yield_kind")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("confirmation")
                        .to_string(),
                })
            });
        Self {
            history,
            replay_events: reconnect
                .get("replay")
                .and_then(|r| r.get("buffered_event_count"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize,
            pending,
            pending_event: None,
        }
    }
}

#[derive(Debug)]
pub struct TuiApp {
    pub session: CreateSession,
    pub reducer: SessionReducer,
    pub composer: Composer,
    pub should_quit: bool,
    pub completion: Option<CompletionState>,
    pub form: Option<FormState>,
    completion_sources: CompletionSources,
    last_sequence: Option<u64>,
}

impl TuiApp {
    pub fn new(session: CreateSession) -> Self {
        Self {
            session,
            reducer: SessionReducer::default(),
            composer: Composer::default(),
            should_quit: false,
            completion: None,
            form: None,
            completion_sources: CompletionSources {
                commands: default_commands(),
                ..CompletionSources::default()
            },
            last_sequence: None,
        }
    }

    /// Replace the `$` skill completion candidates (sourced from the daemon catalog).
    pub fn set_skill_candidates(&mut self, skills: Vec<CompletionCandidate>) {
        self.completion_sources.skills = skills;
    }

    /// Replace the `@` file completion candidates (workspace path index).
    pub fn set_file_candidates(&mut self, files: Vec<CompletionCandidate>) {
        self.completion_sources.files = files;
    }

    pub fn dispatch(&mut self, event: AppEvent) -> Option<AppAction> {
        match event {
            AppEvent::Terminal(TerminalEvent::Key(key)) => self.handle_key(key),
            AppEvent::Terminal(TerminalEvent::Paste(text)) => {
                // Route paste into the focused form field when a form is open,
                // otherwise into the composer.
                if let Some(form) = self.form.as_mut() {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        form.insert_char(ch);
                    }
                } else {
                    self.composer.insert_text(&text);
                }
                None
            }
            AppEvent::Terminal(TerminalEvent::Resize(width, height)) => {
                tracing::debug!(width, height, "terminal resized");
                None
            }
            AppEvent::Terminal(_) => None,
            AppEvent::Daemon(envelope) => {
                self.record_sequence_gap(&envelope);
                self.reducer.apply_envelope(*envelope);
                self.sync_form();
                None
            }
            AppEvent::Hydrated(hydration) => {
                tracing::debug!(
                    history_messages = hydration.history.len(),
                    replay_events = hydration.replay_events,
                    pending = hydration.pending.is_some(),
                    "session hydrated"
                );
                self.restore_history(hydration.history);
                // Prefer the full Yield event from the buffered log (real form
                // questions / approval payload); fall back to the minimal signal
                // reconstruction only when the buffer no longer has it.
                if let Some(event) = hydration.pending_event {
                    self.reducer.apply_envelope(*event);
                } else if let Some(pending) = hydration.pending {
                    self.restore_pending_yield(pending);
                }
                self.sync_form();
                None
            }
            AppEvent::Status(message) => {
                tracing::debug!(%message, "status");
                self.reducer.transient_notice = Some(message);
                None
            }
            AppEvent::Error(message) => {
                self.reducer.transient_notice = Some(message);
                None
            }
        }
    }

    /// Render persisted `/history` messages into the transcript on attach, so a
    /// reattached session shows the prior conversation instead of an empty pane.
    fn restore_history(&mut self, history: Vec<HydratedMessage>) {
        for message in history {
            let cell = match message.role.as_str() {
                "user" if !message.content.is_empty() => {
                    crate::history::HistoryCell::User(message.content)
                }
                "assistant" if !message.content.is_empty() => {
                    crate::history::HistoryCell::Assistant(message.content)
                }
                "tool" => crate::history::HistoryCell::Tool {
                    title: message.tool_name.unwrap_or_else(|| "tool".to_string()),
                    status: crate::history::ToolStatus::Complete,
                    preview: Some(message.content).filter(|c| !c.is_empty()),
                    presentation: None,
                },
                // Skip system/context and empty messages.
                _ => continue,
            };
            self.reducer.cells.push(cell);
        }
    }

    /// Reconstruct a resumable pending yield from a reconnect snapshot signal so
    /// the user can answer an outstanding approval/input after reattaching. The
    /// snapshot carries only the request id + kind (no payload), so confirmation
    /// options default to approve/reject; full transcript replay is a follow-up.
    fn restore_pending_yield(&mut self, pending: PendingHydration) {
        let kind = match pending.kind.as_str() {
            "confirmation" => alan_protocol::YieldKind::Confirmation,
            "structured_input" => alan_protocol::YieldKind::StructuredInput,
            "dynamic_tool" => alan_protocol::YieldKind::DynamicTool,
            other => alan_protocol::YieldKind::Custom(other.to_string()),
        };
        let options = if matches!(kind, alan_protocol::YieldKind::Confirmation) {
            vec!["approve".to_string(), "reject".to_string()]
        } else {
            Vec::new()
        };
        let cell = crate::history::PendingYieldCell {
            request_id: pending.request_id,
            kind,
            title: "pending input restored".to_string(),
            prompt: None,
            options,
            questions: Vec::new(),
            capability: None,
            reason: None,
            presentation: None,
        };
        self.reducer.pending_yield = Some(cell.clone());
        self.reducer
            .cells
            .push(crate::history::HistoryCell::PendingYield(cell));
    }

    fn record_sequence_gap(&mut self, envelope: &alan_protocol::EventEnvelope) {
        if let Some(previous) = self.last_sequence
            && envelope.sequence > previous.saturating_add(1)
        {
            tracing::warn!(
                expected = previous + 1,
                received = envelope.sequence,
                "event stream gap detected"
            );
        }
        self.last_sequence = Some(envelope.sequence);
    }

    pub fn render_opts(&self, width: usize) -> RenderOpts {
        RenderOpts::new(width, self.reducer.expand_thinking)
    }

    pub fn rendered_history_lines(&self, width: usize) -> Vec<String> {
        self.render_history_lines(self.render_opts(width))
    }

    /// Number of lines the bottom live region occupies for the given composer width.
    pub fn live_region_height(&self, width: usize) -> u16 {
        let mut height = 0;
        if let Some(form) = &self.form {
            height += form.render_lines().len() as u16;
        } else {
            height += 1; // hint line
            height += self.composer_height(width);
            height += self.completion_height();
        }
        if self.reducer.activity_label().is_some() {
            height += 1;
        }
        if self.reducer.transient_notice.is_some() {
            height += 1;
        }
        height.max(2)
    }

    fn completion_height(&self) -> u16 {
        self.completion
            .as_ref()
            .map(|state| state.matches.len().min(MAX_COMPLETION_ROWS) as u16)
            .unwrap_or(0)
    }

    fn composer_height(&self, width: usize) -> u16 {
        let width = width.max(1);
        let lines = self
            .composer
            .text()
            .split('\n')
            .map(|line| {
                let visual = unicode_width::UnicodeWidthStr::width(line);
                ((visual / width) + 1) as u16
            })
            .sum::<u16>()
            .max(1);
        lines.min(MAX_COMPOSER_LINES)
    }

    pub fn drain_committed_scrollback(
        &mut self,
        viewport_width: usize,
        viewport_height: usize,
    ) -> Vec<String> {
        let opts = self.render_opts(viewport_width);
        let reserved = self.live_region_height(viewport_width) as usize + 1;
        let max_lines = viewport_height.saturating_sub(reserved).max(2);
        let lines = self.render_history_lines(opts);
        if lines.len() <= max_lines {
            return Vec::new();
        }
        let drain_count = lines.len() - max_lines;
        let pruned_count = self.prune_rendered_prefix(opts, drain_count);
        lines.into_iter().take(pruned_count).collect()
    }

    pub fn clear_pending_yield(&mut self, request_id: &str) {
        if self
            .reducer
            .pending_yield
            .as_ref()
            .is_some_and(|pending| pending.request_id == request_id)
        {
            self.reducer.pending_yield = None;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<AppAction> {
        // A pending form/confirmation takes priority over a completion popup that
        // was open when the yield arrived: otherwise a stale popup would swallow
        // Enter/Tab/Up/Down meant for the prompt. Drop it and route to the prompt.
        let pending_input = self.form.is_some() || self.reducer.pending_yield.is_some();
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
                Some(AppAction::Quit)
            }
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.reducer.expand_thinking = !self.reducer.expand_thinking;
                None
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => Some(AppAction::Interrupt),
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
                    ComposerKeyOutcome::Interrupt => Some(AppAction::Interrupt),
                    ComposerKeyOutcome::Changed | ComposerKeyOutcome::Ignored => None,
                }
            }
        }
    }

    /// While the completion popup is open, navigation/accept/dismiss keys are
    /// consumed by the popup. Returns true when the key was handled here.
    fn consume_completion_key(&mut self, key: KeyEvent) -> bool {
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
                // When a turn is running or input is pending, Esc must still
                // interrupt (the UI advertises "esc to interrupt"); let it bubble
                // to the global interrupt branch. Otherwise it dismisses the popup.
                if self.reducer.turn_active || self.reducer.pending_yield.is_some() {
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

    /// Single-key confirmation: a digit selects the matching option of a pending
    /// confirmation yield without typing into the composer.
    fn confirmation_keypress(&mut self, key: KeyEvent) -> Option<AppAction> {
        if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
            return None;
        }
        let pending = self.reducer.pending_yield.as_ref()?;
        if !matches!(pending.kind, alan_protocol::YieldKind::Confirmation) {
            return None;
        }
        if !self.composer.text().is_empty() {
            return None;
        }
        let KeyCode::Char(ch) = key.code else {
            return None;
        };
        let index = ch.to_digit(10).filter(|d| *d >= 1)? as usize - 1;
        let option = pending.options.get(index)?.clone();
        let pending = pending.clone();
        match pending.resume_content(&option) {
            Ok(content) => Some(AppAction::Resume {
                request_id: pending.request_id,
                content,
            }),
            Err(message) => {
                self.reducer.transient_notice = Some(message);
                None
            }
        }
    }

    fn accept_completion(&mut self) {
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

    /// Keep the interactive form in sync with the pending yield: open it for a
    /// multi-question structured-input request, close it otherwise.
    fn sync_form(&mut self) {
        match &self.reducer.pending_yield {
            Some(pending)
                if matches!(pending.kind, alan_protocol::YieldKind::StructuredInput)
                    && pending.questions.len() > 1 =>
            {
                if self
                    .form
                    .as_ref()
                    .is_none_or(|form| form.request_id != pending.request_id)
                {
                    self.form = Some(FormState::new(
                        pending.request_id.clone(),
                        pending.questions.clone(),
                    ));
                }
            }
            _ => self.form = None,
        }
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> Option<AppAction> {
        match key.code {
            KeyCode::Esc => return Some(AppAction::Interrupt),
            KeyCode::Tab | KeyCode::Down => {
                if let Some(form) = self.form.as_mut() {
                    form.next_field();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(form) = self.form.as_mut() {
                    form.prev_field();
                }
            }
            KeyCode::Backspace => {
                if let Some(form) = self.form.as_mut() {
                    form.backspace();
                }
            }
            KeyCode::Enter => return self.submit_form(),
            KeyCode::Char(ch) => {
                if let Some(form) = self.form.as_mut() {
                    form.insert_char(ch);
                }
            }
            _ => {}
        }
        None
    }

    fn submit_form(&mut self) -> Option<AppAction> {
        let pending = self.reducer.pending_yield.clone()?;
        let form = self.form.as_mut()?;
        match pending.resume_content(&form.answers_json()) {
            Ok(content) => {
                self.form = None;
                Some(AppAction::Resume {
                    request_id: pending.request_id,
                    content,
                })
            }
            Err(message) => {
                form.error = Some(message);
                None
            }
        }
    }

    fn refresh_completion(&mut self) {
        if self.reducer.pending_yield.is_some() {
            self.completion = None;
            return;
        }
        self.completion = completion::detect(
            self.composer.text(),
            self.composer.cursor(),
            &self.completion_sources,
        );
    }

    fn handle_submit(&mut self) -> Option<AppAction> {
        if let Some(pending) = self.reducer.pending_yield.clone() {
            let text = self.composer.text().trim().to_string();
            self.composer.set_text("");
            match pending.resume_content(&text) {
                Ok(content) => {
                    return Some(AppAction::Resume {
                        request_id: pending.request_id,
                        content,
                    });
                }
                Err(message) => {
                    self.composer.set_text(text);
                    self.reducer.transient_notice = Some(message);
                    return None;
                }
            }
        }

        let text = self.composer.take_submit()?;
        self.composer.remember(&text);
        if let Some(action) = self.handle_command(&text) {
            return action;
        }
        self.reducer
            .cells
            .push(crate::history::HistoryCell::User(text.clone()));
        Some(AppAction::SubmitTurn(text))
    }

    /// Handle a leading-`/` client command. Returns `Some(_)` when the text was a
    /// command (the inner action may itself be `None` for handled-no-op commands).
    fn handle_command(&mut self, text: &str) -> Option<Option<AppAction>> {
        let command = text.strip_prefix('/')?;
        let name = command.split_whitespace().next().unwrap_or("");
        match name {
            "quit" => {
                self.should_quit = true;
                Some(Some(AppAction::Quit))
            }
            "compact" => Some(Some(AppAction::Compact)),
            "rollback" => Some(Some(AppAction::Rollback(1))),
            "clear" => {
                self.reducer.cells.clear();
                Some(None)
            }
            "help" => {
                self.reducer.transient_notice = Some(
                    "/compact /rollback /clear /quit · ctrl+r toggle thinking · esc interrupt"
                        .into(),
                );
                Some(None)
            }
            _ => {
                self.reducer.transient_notice = Some(format!("unknown command: /{name}"));
                Some(None)
            }
        }
    }

    fn render_history_lines(&self, opts: RenderOpts) -> Vec<String> {
        self.reducer
            .cells
            .iter()
            .flat_map(|cell| cell.render_lines(opts))
            .collect()
    }

    fn prune_rendered_prefix(&mut self, opts: RenderOpts, lines_to_prune: usize) -> usize {
        let mut remaining = lines_to_prune;
        let mut cells_to_remove = 0;
        let mut pruned = 0;

        while remaining > 0 && cells_to_remove < self.reducer.cells.len() {
            let cell_lines = self.reducer.cells[cells_to_remove].render_lines(opts).len();
            if cell_lines > remaining {
                break;
            }
            remaining -= cell_lines;
            pruned += cell_lines;
            cells_to_remove += 1;
        }

        if cells_to_remove > 0 {
            self.reducer.cells.drain(0..cells_to_remove);
        }

        if remaining > 0
            && let Some(cell) = self.reducer.cells.first_mut()
            && cell.trim_rendered_prefix(opts, remaining)
        {
            pruned += remaining;
        }

        pruned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryCell;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn app() -> TuiApp {
        TuiApp::new(CreateSession {
            session_id: "s-1".into(),
            profile_id: None,
            provider: None,
            resolved_model: None,
            durability: None,
        })
    }

    #[test]
    fn enter_submits_composer_text_as_turn() {
        let mut app = app();
        app.composer.set_text("hello");
        let action = app.dispatch(AppEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
        assert_eq!(action, Some(AppAction::SubmitTurn("hello".into())));
        assert!(matches!(app.reducer.cells[0], HistoryCell::User(_)));
    }

    #[test]
    fn slash_command_clear_empties_transcript_without_turn() {
        let mut app = app();
        app.reducer.cells.push(HistoryCell::User("old".into()));
        app.composer.set_text("/clear");
        let action = app.dispatch(AppEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
        assert_eq!(action, None);
        assert!(app.reducer.cells.is_empty());
    }

    fn press(app: &mut TuiApp, code: KeyCode, modifiers: KeyModifiers) -> Option<AppAction> {
        app.dispatch(AppEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            code, modifiers,
        ))))
    }

    #[test]
    fn slash_opens_command_completion_and_tab_accepts() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('o'), KeyModifiers::NONE);
        let state = app.completion.as_ref().expect("command completion open");
        assert_eq!(state.matches[0].value, "compact");
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "/compact ");
        assert!(app.completion.is_none());
    }

    #[test]
    fn dollar_skill_completion_uses_catalog_candidates() {
        let mut app = app();
        app.set_skill_candidates(vec![CompletionCandidate::new("code-review", None)]);
        for ch in "use $co".chars() {
            press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
        }
        let state = app.completion.as_ref().expect("skill completion open");
        assert_eq!(state.matches[0].value, "code-review");
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "use $code-review ");
    }

    #[test]
    fn esc_interrupts_during_turn_even_with_completion_open() {
        let mut app = app();
        app.reducer.turn_active = true;
        // Open a command completion popup.
        press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(app.completion.is_some());
        // Esc must interrupt the running turn, not merely dismiss the popup.
        let action = press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(action, Some(AppAction::Interrupt));
    }

    #[test]
    fn esc_dismisses_completion_when_idle() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(app.completion.is_some());
        let action = press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(action, None);
        assert!(app.completion.is_none());
    }

    #[test]
    fn missing_skill_catalog_degrades_to_free_typing() {
        let mut app = app();
        for ch in "use $co".chars() {
            press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
        }
        assert!(app.completion.is_none());
        assert_eq!(app.composer.text(), "use $co");
    }

    #[test]
    fn ctrl_r_toggles_thinking_expansion() {
        let mut app = app();
        assert!(!app.reducer.expand_thinking);
        app.dispatch(AppEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Char('r'),
            KeyModifiers::CONTROL,
        ))));
        assert!(app.reducer.expand_thinking);
    }

    #[test]
    fn daemon_sequence_gap_does_not_create_transcript_cell() {
        let mut app = app();
        app.dispatch(AppEvent::Daemon(Box::new(envelope(1))));
        app.dispatch(AppEvent::Daemon(Box::new(envelope(3))));
        assert!(app.reducer.cells.is_empty());
    }

    #[test]
    fn hydration_without_pending_creates_no_transcript_cell() {
        let mut app = app();
        let hydration = SessionHydration::from_values(
            &serde_json::json!({ "messages": [{}, {}] }),
            &serde_json::json!({ "replay": { "buffered_event_count": 1 } }),
        );
        assert_eq!(hydration.replay_events, 1);
        assert!(hydration.pending.is_none());
        app.dispatch(AppEvent::Hydrated(hydration));
        assert!(app.reducer.cells.is_empty());
        assert!(app.reducer.pending_yield.is_none());
    }

    #[test]
    fn hydration_renders_persisted_history_messages() {
        let mut app = app();
        let hydration = SessionHydration::from_values(
            &serde_json::json!({ "messages": [
                { "role": "user", "content": "hi", "timestamp": "t" },
                { "role": "assistant", "content": "hello", "timestamp": "t" },
                { "role": "tool", "content": "ok", "tool_name": "read_file", "timestamp": "t" },
                { "role": "system", "content": "ignored", "timestamp": "t" }
            ]}),
            &serde_json::json!({ "replay": { "buffered_event_count": 0 } }),
        );
        app.dispatch(AppEvent::Hydrated(hydration));
        let cells = &app.reducer.cells;
        assert_eq!(cells.len(), 3, "system message should be skipped");
        assert!(matches!(&cells[0], HistoryCell::User(t) if t == "hi"));
        assert!(matches!(&cells[1], HistoryCell::Assistant(t) if t == "hello"));
        assert!(matches!(&cells[2], HistoryCell::Tool { title, .. } if title == "read_file"));
    }

    #[test]
    fn hydration_restores_structured_input_pending_signal() {
        let mut app = app();
        let hydration = SessionHydration::from_values(
            &serde_json::json!({ "messages": [] }),
            &serde_json::json!({
                "replay": { "buffered_event_count": 0 },
                "notifications": { "signals": [
                    { "signal_type": "pending_structured_input", "request_id": "req-si", "yield_kind": "structured_input" }
                ]}
            }),
        );
        app.dispatch(AppEvent::Hydrated(hydration));
        let pending = app.reducer.pending_yield.clone().expect("pending restored");
        assert_eq!(pending.request_id, "req-si");
        assert_eq!(pending.kind, alan_protocol::YieldKind::StructuredInput);
    }

    #[test]
    fn hydration_applies_full_yield_event_over_signal_fallback() {
        let mut app = app();
        let mut hydration = SessionHydration::from_values(
            &serde_json::json!({ "messages": [] }),
            &serde_json::json!({ "notifications": { "signals": [
                { "signal_type": "pending_structured_input", "request_id": "req-f", "yield_kind": "structured_input" }
            ]}}),
        );
        // Buffered event log carries the full payload (real form questions).
        hydration.pending_event = Some(Box::new(envelope_with_event(
            1,
            alan_protocol::Event::Yield {
                request_id: "req-f".into(),
                kind: alan_protocol::YieldKind::StructuredInput,
                payload: serde_json::json!({
                    "title": "Pick",
                    "questions": [{
                        "id": "env", "label": "env", "prompt": "env?",
                        "kind": "single_select", "required": true,
                        "options": [{"value": "a", "label": "A"}]
                    }]
                }),
            },
        )));
        app.dispatch(AppEvent::Hydrated(hydration));
        let pending = app.reducer.pending_yield.clone().expect("pending restored");
        assert_eq!(pending.request_id, "req-f");
        assert!(
            !pending.questions.is_empty(),
            "full Yield payload restores the form questions for keyed answers"
        );
    }

    #[test]
    fn hydration_restores_resumable_pending_yield_from_snapshot() {
        let mut app = app();
        // Real reconnect snapshot shape: notifications.signals[].pending_yield.
        let hydration = SessionHydration::from_values(
            &serde_json::json!({ "messages": [] }),
            &serde_json::json!({
                "replay": { "buffered_event_count": 0 },
                "notifications": { "signals": [
                    { "signal_type": "pending_yield", "request_id": "req-x", "yield_kind": "confirmation" }
                ]}
            }),
        );
        app.dispatch(AppEvent::Hydrated(hydration));
        let pending = app.reducer.pending_yield.clone().expect("pending restored");
        assert_eq!(pending.request_id, "req-x");
        assert_eq!(pending.options, vec!["approve", "reject"]);
        // The user can answer it with a single key after reconnecting.
        let action = press(&mut app, KeyCode::Char('1'), KeyModifiers::NONE);
        assert!(matches!(
            action,
            Some(AppAction::Resume { request_id, .. }) if request_id == "req-x"
        ));
    }

    #[test]
    fn invalid_structured_input_keeps_pending_yield_as_transient() {
        let mut app = app();
        app.dispatch(AppEvent::Daemon(Box::new(envelope_with_event(
            1,
            alan_protocol::Event::Yield {
                request_id: "r-1".into(),
                kind: alan_protocol::YieldKind::StructuredInput,
                payload: serde_json::json!({
                    "title": "Pick environment",
                    "questions": [{
                        "id": "env",
                        "label": "Environment",
                        "prompt": "Environment?",
                        "kind": "single_select",
                        "required": true,
                        "options": [{"value": "prod", "label": "Production"}]
                    }]
                }),
            },
        ))));
        app.composer.set_text("staging");

        let action = app.dispatch(AppEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));

        assert_eq!(action, None);
        assert!(app.reducer.pending_yield.is_some());
        assert_eq!(app.composer.text(), "staging");
        assert!(
            app.reducer
                .transient_notice
                .as_deref()
                .is_some_and(|notice| notice.contains("env must be one of"))
        );
    }

    #[test]
    fn multi_question_structured_input_opens_form_and_submits() {
        let mut app = app();
        app.dispatch(AppEvent::Daemon(Box::new(envelope_with_event(
            1,
            alan_protocol::Event::Yield {
                request_id: "form-1".into(),
                kind: alan_protocol::YieldKind::StructuredInput,
                payload: serde_json::json!({
                    "title": "Deploy",
                    "questions": [
                        {"id": "service", "label": "Service", "prompt": "Service?", "kind": "text", "required": true},
                        {"id": "env", "label": "Env", "prompt": "Env?", "kind": "single_select",
                         "required": true, "options": [{"value": "prod", "label": "Production"}]}
                    ]
                }),
            },
        ))));
        assert!(
            app.form.is_some(),
            "multi-question yield should open a form"
        );

        for ch in "api".chars() {
            press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
        }
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        for ch in "prod".chars() {
            press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
        }
        let action = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        match action {
            Some(AppAction::Resume {
                request_id,
                content,
            }) => {
                assert_eq!(request_id, "form-1");
                assert!(matches!(
                    &content[0],
                    ContentPart::Structured { data }
                        if data["service"] == "api" && data["env"] == "prod"
                ));
            }
            other => panic!("expected Resume, got {other:?}"),
        }
        assert!(app.form.is_none());
    }

    #[test]
    fn paste_routes_into_active_form_field() {
        let mut app = app();
        app.dispatch(AppEvent::Daemon(Box::new(envelope_with_event(
            1,
            alan_protocol::Event::Yield {
                request_id: "form-2".into(),
                kind: alan_protocol::YieldKind::StructuredInput,
                payload: serde_json::json!({
                    "title": "Deploy",
                    "questions": [
                        {"id": "path", "label": "Path", "prompt": "Path?", "kind": "text", "required": true},
                        {"id": "note", "label": "Note", "prompt": "Note?", "kind": "text", "required": false}
                    ]
                }),
            },
        ))));
        assert!(
            app.form.is_some(),
            "multi-question yield should open a form"
        );
        app.dispatch(AppEvent::Terminal(TerminalEvent::Paste(
            "/etc/hosts".to_string(),
        )));
        // Paste lands in the focused form field, not the hidden composer.
        assert_eq!(app.form.as_ref().unwrap().fields[0].value, "/etc/hosts");
        assert!(app.composer.text().is_empty());
    }

    #[test]
    fn digit_key_answers_confirmation_without_typing() {
        let mut app = app();
        app.dispatch(AppEvent::Daemon(Box::new(envelope_with_event(
            1,
            alan_protocol::Event::Yield {
                request_id: "r-9".into(),
                kind: alan_protocol::YieldKind::Confirmation,
                payload: serde_json::json!({
                    "message": "Approve?",
                    "options": ["approve", "reject"]
                }),
            },
        ))));

        let action = press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE);
        match action {
            Some(AppAction::Resume {
                request_id,
                content,
            }) => {
                assert_eq!(request_id, "r-9");
                assert!(
                    matches!(&content[0], ContentPart::Structured { data } if data["choice"] == "reject")
                );
            }
            other => panic!("expected Resume, got {other:?}"),
        }
        assert!(app.composer.text().is_empty());
    }

    #[test]
    fn valid_resume_submission_keeps_pending_yield_until_acknowledged() {
        let mut app = app();
        app.dispatch(AppEvent::Daemon(Box::new(envelope_with_event(
            1,
            alan_protocol::Event::Yield {
                request_id: "r-1".into(),
                kind: alan_protocol::YieldKind::Confirmation,
                payload: serde_json::json!({
                    "message": "Approve?",
                    "options": ["approve", "reject"]
                }),
            },
        ))));
        app.composer.set_text("approve");

        let action = app.dispatch(AppEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));

        assert!(matches!(
            action,
            Some(AppAction::Resume { ref request_id, .. }) if request_id == "r-1"
        ));
        assert!(app.reducer.pending_yield.is_some());
        app.clear_pending_yield("r-1");
        assert!(app.reducer.pending_yield.is_none());
    }

    #[test]
    fn scrollback_drains_by_rendered_lines() {
        let mut app = app();
        app.reducer
            .cells
            .push(HistoryCell::Assistant("long streamed output ".repeat(40)));

        let drained = app.drain_committed_scrollback(32, 10);

        assert!(!drained.is_empty());
        assert!(app.rendered_history_lines(32).len() <= 8);
        assert!(matches!(app.reducer.cells[0], HistoryCell::Assistant(_)));
    }

    #[test]
    fn scrollback_prunes_committed_cells() {
        let mut app = app();
        for idx in 0..20 {
            app.reducer
                .cells
                .push(HistoryCell::User(format!("message {idx}")));
        }

        let before = app.rendered_history_lines(80).len();
        let drained = app.drain_committed_scrollback(80, 8);

        assert!(!drained.is_empty());
        assert!(app.reducer.cells.len() < 20);
        assert!(app.rendered_history_lines(80).len() < before);
    }

    #[test]
    fn pruned_render_tail_wraps_at_resized_width() {
        let mut app = app();
        app.reducer
            .cells
            .push(HistoryCell::Assistant("long streamed output ".repeat(40)));

        app.drain_committed_scrollback(80, 8);
        let drained_after_narrow_resize = app.drain_committed_scrollback(20, 8);

        assert!(!drained_after_narrow_resize.is_empty());
        assert!(app.rendered_history_lines(20).len() <= 8);
    }

    #[test]
    fn partial_scrollback_prune_preserves_streaming_text_cell() {
        let mut app = app();
        app.reducer
            .cells
            .push(HistoryCell::Assistant("long streamed output ".repeat(40)));

        let drained = app.drain_committed_scrollback(32, 8);
        app.reducer.apply_envelope(envelope_with_event(
            2,
            alan_protocol::Event::TextDelta {
                chunk: "tail".into(),
                is_final: false,
            },
        ));

        assert!(!drained.is_empty());
        assert_eq!(app.reducer.cells.len(), 1);
        assert!(matches!(
            app.reducer.cells.first(),
            Some(HistoryCell::Assistant(text)) if text.ends_with("tail")
        ));
    }

    fn envelope(sequence: u64) -> alan_protocol::EventEnvelope {
        envelope_with_event(sequence, alan_protocol::Event::TurnStarted {})
    }

    fn envelope_with_event(
        sequence: u64,
        event: alan_protocol::Event,
    ) -> alan_protocol::EventEnvelope {
        alan_protocol::EventEnvelope {
            event_id: format!("e-{sequence}"),
            sequence,
            session_id: "s-1".into(),
            submission_id: None,
            turn_id: "t-1".into(),
            item_id: "i-1".into(),
            timestamp_ms: sequence,
            event,
        }
    }
}
