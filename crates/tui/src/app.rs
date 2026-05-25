use alan_protocol::ContentPart;
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::composer::{Composer, ComposerKeyOutcome};
use crate::daemon_client::CreateSession;
use crate::history::{HistoryCell, SessionReducer};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHydration {
    pub history_messages: usize,
    pub replay_events: usize,
    pub pending_signal: bool,
}

impl SessionHydration {
    pub fn from_values(history: &serde_json::Value, reconnect: &serde_json::Value) -> Self {
        Self {
            history_messages: history
                .get("messages")
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len),
            replay_events: reconnect
                .get("replay")
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len),
            pending_signal: reconnect
                .get("pending_signal")
                .is_some_and(|value| !value.is_null()),
        }
    }

    fn status_message(&self) -> String {
        let pending = if self.pending_signal {
            ", pending input restored"
        } else {
            ""
        };
        format!(
            "hydrated {} history message(s), {} replay event(s){}",
            self.history_messages, self.replay_events, pending
        )
    }
}

#[derive(Debug)]
pub struct TuiApp {
    pub session: CreateSession,
    pub reducer: SessionReducer,
    pub composer: Composer,
    pub should_quit: bool,
    last_sequence: Option<u64>,
    committed_rendered_lines: usize,
}

impl TuiApp {
    pub fn new(session: CreateSession) -> Self {
        Self {
            session,
            reducer: SessionReducer::default(),
            composer: Composer::default(),
            should_quit: false,
            last_sequence: None,
            committed_rendered_lines: 0,
        }
    }

    pub fn dispatch(&mut self, event: AppEvent) -> Option<AppAction> {
        match event {
            AppEvent::Terminal(TerminalEvent::Key(key)) => self.handle_key(key),
            AppEvent::Terminal(TerminalEvent::Paste(text)) => {
                self.composer.insert_text(&text);
                None
            }
            AppEvent::Terminal(TerminalEvent::Resize(width, height)) => {
                self.committed_rendered_lines = 0;
                self.reducer.cells.push(HistoryCell::Status(format!(
                    "terminal resized to {width}x{height}"
                )));
                None
            }
            AppEvent::Terminal(_) => None,
            AppEvent::Daemon(envelope) => {
                self.record_sequence_gap(&envelope);
                self.reducer.apply_envelope(*envelope);
                None
            }
            AppEvent::Hydrated(hydration) => {
                self.reducer
                    .cells
                    .push(HistoryCell::Status(hydration.status_message()));
                None
            }
            AppEvent::Status(message) => {
                self.reducer.cells.push(HistoryCell::Status(message));
                None
            }
            AppEvent::Error(message) => {
                self.reducer.cells.push(HistoryCell::Error {
                    message,
                    recoverable: true,
                });
                None
            }
        }
    }

    fn record_sequence_gap(&mut self, envelope: &alan_protocol::EventEnvelope) {
        if let Some(previous) = self.last_sequence
            && envelope.sequence > previous.saturating_add(1)
        {
            self.reducer.cells.push(HistoryCell::Warning(format!(
                "event stream gap detected: expected {}, received {}",
                previous + 1,
                envelope.sequence
            )));
        }
        self.last_sequence = Some(envelope.sequence);
    }

    pub fn history_cells(&self) -> &[HistoryCell] {
        &self.reducer.cells
    }

    pub fn rendered_history_lines(&self, width: usize) -> Vec<String> {
        let committed = self.committed_rendered_lines;
        self.render_history_lines(width)
            .into_iter()
            .skip(committed)
            .collect()
    }

    pub fn drain_committed_scrollback(
        &mut self,
        viewport_width: usize,
        viewport_height: usize,
    ) -> Vec<String> {
        let max_lines = viewport_height.saturating_sub(4).max(4);
        let lines = self.render_history_lines(viewport_width);
        let committed = self.committed_rendered_lines.min(lines.len());
        let visible_lines = lines.len().saturating_sub(committed);
        if visible_lines <= max_lines {
            self.committed_rendered_lines = committed;
            return Vec::new();
        }
        let drain_count = visible_lines - max_lines;
        self.committed_rendered_lines = committed + drain_count;
        lines
            .into_iter()
            .skip(committed)
            .take(drain_count)
            .collect()
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
                code: KeyCode::Esc, ..
            } => Some(AppAction::Interrupt),
            KeyEvent {
                code: KeyCode::Char('/'),
                modifiers,
                ..
            } if modifiers.is_empty() && self.composer.text().is_empty() => {
                self.composer.set_text("/");
                None
            }
            _ => match self.composer.handle_key(key) {
                ComposerKeyOutcome::Submit => self.handle_submit(),
                ComposerKeyOutcome::Interrupt => Some(AppAction::Interrupt),
                ComposerKeyOutcome::Changed | ComposerKeyOutcome::Ignored => None,
            },
        }
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
                    self.reducer.cells.push(HistoryCell::Error {
                        message,
                        recoverable: true,
                    });
                    return None;
                }
            }
        }

        let text = self.composer.take_submit()?;
        if text == "/quit" {
            self.should_quit = true;
            return Some(AppAction::Quit);
        }
        if text == "/compact" {
            return Some(AppAction::Compact);
        }
        if text == "/rollback" {
            return Some(AppAction::Rollback(1));
        }
        self.reducer.cells.push(HistoryCell::User(text.clone()));
        Some(AppAction::SubmitTurn(text))
    }

    fn render_history_lines(&self, width: usize) -> Vec<String> {
        self.history_cells()
            .iter()
            .flat_map(|cell| cell.render_lines(width))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(matches!(app.history_cells()[0], HistoryCell::User(_)));
    }

    #[test]
    fn daemon_sequence_gap_becomes_warning_cell() {
        let mut app = app();
        app.dispatch(AppEvent::Daemon(Box::new(envelope(1))));
        app.dispatch(AppEvent::Daemon(Box::new(envelope(3))));
        assert!(app.history_cells().iter().any(|cell| {
            matches!(cell, HistoryCell::Warning(message) if message.contains("event stream gap"))
        }));
    }

    #[test]
    fn hydration_status_does_not_expose_raw_json() {
        let mut app = app();
        let hydration = SessionHydration::from_values(
            &serde_json::json!({ "messages": [{}, {}] }),
            &serde_json::json!({ "replay": [{}], "pending_signal": { "type": "confirmation" } }),
        );
        app.dispatch(AppEvent::Hydrated(hydration));
        assert!(matches!(
            app.history_cells().last(),
            Some(HistoryCell::Status(message))
                if message == "hydrated 2 history message(s), 1 replay event(s), pending input restored"
        ));
    }

    #[test]
    fn invalid_structured_input_keeps_pending_yield() {
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
        assert!(app.history_cells().iter().any(|cell| {
            matches!(cell, HistoryCell::Error { message, recoverable: true } if message.contains("env must be one of"))
        }));
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

        let drained = app.drain_committed_scrollback(32, 8);

        assert!(!drained.is_empty());
        assert!(app.rendered_history_lines(32).len() <= 4);
        assert!(matches!(app.history_cells()[0], HistoryCell::Assistant(_)));
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
