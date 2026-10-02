use super::*;

#[derive(Clone, Default)]
pub(in crate::file_backed) struct ModelChooser {
    pub active: bool,
    pub index: usize,
    pub explicit: bool,
    pub catalog: Option<alan_agent_protocol::UiModelCatalog>,
    pub uncertain: Option<(String, String)>,
    pub pending: Option<(String, String)>,
}
impl FileBackedApp {
    pub(super) fn model_submit_command(&self) -> bool {
        self.input_intent == InputIntent::Agent && self.composer.text().trim() == "/model"
    }
    pub(super) fn model_status_command(&mut self) -> Option<FileBackedAction> {
        self.notice = Some(self.model.status());
        None
    }
    fn model_control_available(&self) -> bool {
        !self.model.owner.is_empty()
            && self.model.owner != "/agent/root"
            && self.model.owner == self.queue.owner
            && self.model.known().is_some()
            && self.model_chooser.pending.is_none()
            && self.model_chooser.uncertain.is_none()
    }
    pub(in crate::file_backed) fn model_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        if !argument.is_empty() {
            self.notice = Some("usage: /model (Process catalog picker)".into());
            return None;
        }
        if !self.model_control_available() {
            self.notice = Some(
                "model selection requires known pinned Process and settled model control".into(),
            );
            return None;
        }
        let Some(catalog) = self.model.known().and_then(|s| s.catalog.as_ref()) else {
            self.notice = Some("Process model catalog unknown or unavailable".into());
            return None;
        };
        if catalog.models.is_empty() {
            self.notice = Some("Process model catalog is empty".into());
            return None;
        }
        self.model_chooser.catalog = Some(catalog.clone());
        self.model_chooser.explicit = false;
        self.model_chooser.active = true;
        self.model_chooser.index = 0;
        self.completion = None;
        self.refresh_model_choice();
        None
    }
    fn refresh_model_choice(&mut self) {
        let choices = self
            .model
            .known()
            .and_then(|s| s.catalog.as_ref())
            .map(|c| {
                c.models
                    .iter()
                    .map(|m| {
                        CompletionCandidate::new(
                            super::super::model::safe(&m.model),
                            Some("selected-next only".into()),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.completion = Some(CompletionState {
            kind: completion::CompletionKind::Command,
            token_start: 0,
            query: String::new(),
            matches: choices,
            selected: self.model_chooser.index,
        });
        self.notice = None;
    }
    pub(in crate::file_backed) fn model_key(
        &mut self,
        key: KeyEvent,
        _admitted: bool,
    ) -> Option<FileBackedAction> {
        self.reconcile_model_chooser();
        if !self.model_chooser.active {
            return None;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('c')
                if key.code == KeyCode::Esc || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.model_chooser.active = false;
                self.completion = None;
                self.notice = None;
            }
            KeyCode::Up => {
                self.model_chooser.explicit = true;
                self.model_chooser.index = self.model_chooser.index.saturating_sub(1);
                self.refresh_model_choice();
            }
            KeyCode::Down => {
                self.model_chooser.explicit = true;
                let count = self
                    .model
                    .known()
                    .and_then(|s| s.catalog.as_ref())
                    .map_or(0, |c| c.models.len());
                self.model_chooser.index =
                    (self.model_chooser.index + 1).min(count.saturating_sub(1));
                self.refresh_model_choice();
            }
            KeyCode::Enter => {
                if !self.model_chooser.explicit {
                    return None;
                }
                if !self.model_control_available() {
                    self.notice = Some("model selection blocked by admitted input".into());
                    return None;
                }
                let choice = self
                    .model
                    .known()?
                    .catalog
                    .as_ref()?
                    .models
                    .get(self.model_chooser.index)?
                    .model
                    .clone();
                if choice.is_empty() || choice.chars().any(char::is_control) {
                    self.notice = Some("invalid catalog choice".into());
                    return None;
                }
                let id = alan_agent_protocol::UserInputRecord::new(
                    InputIntent::Agent,
                    alan_agent_protocol::InputMode::FollowUp,
                    "",
                )
                .submission_id;
                let owner = self.model.owner.clone();
                self.model_chooser.active = false;
                self.model_chooser.pending = Some((owner.clone(), id.clone()));
                self.completion = None;
                self.notice = Some("model selection pending; paused work is not resumed".into());
                return Some(FileBackedAction::SelectModel {
                    owner,
                    id,
                    op: alan_agent_protocol::Op::SelectModel { model: choice },
                });
            }
            _ => {}
        }
        None
    }
    pub(in crate::file_backed) fn reconcile_model_chooser(&mut self) {
        if self.model_chooser.active
            && self.model.known().and_then(|s| s.catalog.as_ref())
                != self.model_chooser.catalog.as_ref()
        {
            self.model_chooser.active = false;
            self.model_chooser.explicit = false;
            self.model_chooser.catalog = None;
            self.completion = None;
            self.notice = Some(
                "model catalog changed or unavailable; reopen /model and explicitly choose".into(),
            );
        }
    }
    pub(in crate::file_backed) fn lose_model_receipts(&mut self, owner: &str) {
        if self
            .model_chooser
            .pending
            .as_ref()
            .is_some_and(|(expected, _)| expected == owner)
        {
            self.model_chooser.uncertain = self.model_chooser.pending.take();
            self.notice = Some(
                "model selection outcome uncertain; receipt owner unavailable; no retry".into(),
            );
        }
    }
    pub(in crate::file_backed) fn observe_model_receipt(
        &mut self,
        owner: &str,
        event: &UiEvent,
    ) -> bool {
        let Some((expected_owner, id)) = self
            .model_chooser
            .pending
            .as_ref()
            .or(self.model_chooser.uncertain.as_ref())
        else {
            return false;
        };
        let UiEvent::InputCompleted {
            submission_ids,
            status,
            ..
        } = event
        else {
            return false;
        };
        if owner != expected_owner || !submission_ids.contains(id) {
            return false;
        }
        self.notice = Some(
            match status {
                UiInputStatus::Completed => {
                    "model selection completed; projection owns selected-next"
                }
                UiInputStatus::Failed => "model selection rejected",
                UiInputStatus::Cancelled => "model selection cancelled",
            }
            .into(),
        );
        self.model_chooser.pending = None;
        self.model_chooser.uncertain = None;
        true
    }
}
