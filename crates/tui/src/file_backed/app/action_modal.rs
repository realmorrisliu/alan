use super::*;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::file_backed) enum ModalMode {
    #[default]
    Actions,
    Plans,
    Tape,
    Status,
    Man,
}

#[derive(Default, Clone)]
pub(in crate::file_backed) struct ActionModal {
    pub generation: u64,
    pub active: bool,
    pub actions: Vec<super::super::action_detail_io::ActionEntry>,
    pub mode: ModalMode,
    pub plans: Vec<super::super::plan_detail_io::PlanEntry>,
    pub selected: usize,
    pub scroll: usize,
    pub rows: Vec<Line<'static>>,
    pub pending: bool,
    pub owner_path: String,
    pub show_raw_json: bool,
}

impl ActionModal {
    #[allow(
        dead_code,
        reason = "convenience predicate retained for plan-mode checks and tests"
    )]
    pub(in crate::file_backed) fn plan_mode(&self) -> bool {
        self.mode == ModalMode::Plans
    }

    pub(in crate::file_backed) fn reset_for_mode(&mut self, mode: ModalMode, pending: bool) {
        self.generation += 1;
        self.mode = mode;
        self.selected = 0;
        self.scroll = 0;
        self.rows.clear();
        self.actions.clear();
        self.plans.clear();
        self.show_raw_json = false;
        self.pending = pending;
    }
}

impl FileBackedApp {
    pub(super) fn modal_key(&mut self, key: KeyEvent, has_pending_submission: bool) -> bool {
        let is_ctrl_o =
            key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL);
        let is_ctrl_t =
            key.code == KeyCode::Char('t') && key.modifiers.contains(KeyModifiers::CONTROL);
        if is_ctrl_o || is_ctrl_t {
            // Approval/form keys keep their existing owner.
            if self.form.is_some()
                || self.pending_yield.is_some()
                || self.project_selection.is_some()
            {
                return true;
            }
            let target_mode = if is_ctrl_t {
                ModalMode::Status
            } else {
                ModalMode::Actions
            };
            if self.modal.active && self.modal.mode == target_mode {
                self.modal.active = false;
                self.modal.reset_for_mode(target_mode, false);
                self.modal.owner_path = self.agent_path.clone();
            } else {
                self.modal.active = true;
                self.switch_modal_mode(target_mode);
            }
            return true;
        }
        if !self.modal.active {
            return false;
        }
        if key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && (self.turn_active() || has_pending_submission || self.pending_yield.is_some())
        {
            self.modal.active = false;
            self.modal.generation += 1;
            return false;
        }
        let item_count = match self.modal.mode {
            ModalMode::Plans => self.modal.plans.len(),
            ModalMode::Actions => self.modal.actions.len(),
            ModalMode::Tape | ModalMode::Status | ModalMode::Man => 0,
        };
        match key.code {
            KeyCode::Char('p') if key.modifiers == KeyModifiers::NONE => {
                let next = if self.modal.mode == ModalMode::Plans {
                    ModalMode::Actions
                } else {
                    ModalMode::Plans
                };
                self.switch_modal_mode(next);
            }
            KeyCode::Char('t') if key.modifiers == KeyModifiers::NONE => {
                let next = if self.modal.mode == ModalMode::Tape {
                    ModalMode::Actions
                } else {
                    ModalMode::Tape
                };
                self.switch_modal_mode(next);
            }
            KeyCode::Char('s') if key.modifiers == KeyModifiers::NONE => {
                let next = if self.modal.mode == ModalMode::Status {
                    ModalMode::Actions
                } else {
                    ModalMode::Status
                };
                self.switch_modal_mode(next);
            }
            KeyCode::Char('a')
                if key.modifiers == KeyModifiers::NONE && self.modal.mode != ModalMode::Actions =>
            {
                self.switch_modal_mode(ModalMode::Actions);
            }
            KeyCode::Char('d') if key.modifiers == KeyModifiers::NONE => {
                self.modal.show_raw_json = !self.modal.show_raw_json;
                self.modal.scroll = 0;
            }
            KeyCode::Esc => {
                self.modal.active = false;
                self.modal.generation += 1;
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char('[') | KeyCode::Char(']')
                if item_count > 0 =>
            {
                let prev = matches!(key.code, KeyCode::Left | KeyCode::Char('['));
                let index = if prev {
                    self.modal.selected.saturating_sub(1)
                } else {
                    (self.modal.selected + 1).min(item_count - 1)
                };
                if index != self.modal.selected {
                    self.modal.selected = index;
                    self.modal.generation += 1;
                    self.modal.rows.clear();
                    self.modal.scroll = 0;
                    self.modal.pending = self.modal.mode == ModalMode::Actions;
                    if self.modal.mode == ModalMode::Plans {
                        self.modal.rows = super::super::plan_detail_io::rows(
                            &self.modal.plans[self.modal.selected],
                        );
                    }
                }
            }
            KeyCode::Up => {
                self.modal.scroll = self.modal.scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                self.modal.scroll = self.modal.scroll.saturating_add(1);
            }
            KeyCode::Char('k') if key.modifiers == KeyModifiers::NONE => {
                self.modal.scroll = self.modal.scroll.saturating_sub(1);
            }
            KeyCode::Char('j') if key.modifiers == KeyModifiers::NONE => {
                self.modal.scroll = self.modal.scroll.saturating_add(1);
            }
            KeyCode::Char(' ') if key.modifiers == KeyModifiers::NONE => {
                self.modal.scroll = self.modal.scroll.saturating_add(10);
            }
            KeyCode::Char('b') if key.modifiers == KeyModifiers::NONE => {
                self.modal.scroll = self.modal.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => self.modal.scroll = self.modal.scroll.saturating_add(10),
            KeyCode::PageUp => self.modal.scroll = self.modal.scroll.saturating_sub(10),
            KeyCode::Home => self.modal.scroll = 0,
            _ => {}
        }
        true
    }
}
