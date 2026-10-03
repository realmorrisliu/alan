use super::*;

#[derive(Default, Clone)]
pub(in crate::file_backed) struct ActionModal {
    pub generation: u64,
    pub active: bool,
    pub ids: Vec<String>,
    pub selected: usize,
    pub scroll: usize,
    pub rows: Vec<Line<'static>>,
    pub pending: bool,
    pub owner_path: String,
}

impl FileBackedApp {
    pub(super) fn modal_key(&mut self, key: KeyEvent, has_pending_submission: bool) -> bool {
        if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
            // Approval/form keys keep their existing owner.
            if self.form.is_some()
                || self.pending_yield.is_some()
                || self.project_selection.is_some()
            {
                return true;
            }
            self.modal.active = !self.modal.active;
            self.modal.generation += 1;
            self.modal.ids.clear();
            self.modal.rows.clear();
            self.modal.selected = 0;
            self.modal.scroll = 0;
            self.modal.pending = self.modal.active;
            self.modal.owner_path = self.agent_path.clone();
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
        match key.code {
            KeyCode::Esc => {
                self.modal.active = false;
                self.modal.generation += 1;
            }
            KeyCode::Up | KeyCode::Left | KeyCode::Down | KeyCode::Right
                if !self.modal.ids.is_empty() =>
            {
                let index = if matches!(key.code, KeyCode::Up | KeyCode::Left) {
                    self.modal.selected.saturating_sub(1)
                } else {
                    (self.modal.selected + 1).min(self.modal.ids.len() - 1)
                };
                if index != self.modal.selected {
                    self.modal.selected = index;
                    self.modal.generation += 1;
                    self.modal.rows.clear();
                    self.modal.scroll = 0;
                    self.modal.pending = true;
                }
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
