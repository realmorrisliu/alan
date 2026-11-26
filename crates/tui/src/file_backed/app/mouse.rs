//! Modal-only SGR mouse event handling (`Ctrl+T`, `Ctrl+O`, `/model`, `/resume`).
//!
//! Normal inline operation keeps SGR mouse capture OFF so native terminal scrollback
//! and drag-select copy remain completely intact.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::action_modal::ModalMode;
use super::{FileBackedAction, FileBackedApp};

impl FileBackedApp {
    /// Whether SGR mouse capture should currently be enabled for an open modal/picker.
    pub(in crate::file_backed) fn modal_mouse_capture_wanted(&self) -> bool {
        !self.theme.is_dumb
            && (self.modal.active || self.model_chooser.active || self.resume_picker.active)
    }

    /// Handle mouse scroll and left-click events strictly while a modal/picker is open.
    pub(in crate::file_backed) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        has_pending_submission: bool,
    ) -> Option<FileBackedAction> {
        if self.modal.active {
            self.handle_inspector_modal_mouse(mouse);
            return None;
        }
        if self.resume_picker.active {
            return self.handle_resume_picker_mouse(mouse);
        }
        if self.model_chooser.active {
            return self.handle_model_chooser_mouse(mouse, has_pending_submission);
        }
        None
    }

    fn handle_inspector_modal_mouse(&mut self, mouse: MouseEvent) {
        let (origin_x, origin_y) = self.last_frame_origin.get();
        let width = self.viewport_size.0.max(1) as usize;
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.modal.scroll = self.modal.scroll.saturating_sub(3);
            }
            MouseEventKind::ScrollDown => {
                self.modal.scroll = self.modal.scroll.saturating_add(3);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(rel_row) = mouse.row.checked_sub(origin_y) else {
                    return;
                };
                let Some(rel_col) = mouse.column.checked_sub(origin_x) else {
                    return;
                };
                if let Some(target_mode) = super::super::modal_view::hit_test_modal_tab(
                    self.modal.mode,
                    self.theme,
                    width,
                    rel_col,
                    rel_row,
                ) {
                    if target_mode != self.modal.mode {
                        self.switch_modal_mode(target_mode);
                    }
                    return;
                }
                // Clicking header row (row 0) left/right halves steps between items when applicable.
                if rel_row == 0 {
                    let key = if (rel_col as usize) < width / 2 {
                        KeyCode::Left
                    } else {
                        KeyCode::Right
                    };
                    self.modal_key(KeyEvent::new(key, KeyModifiers::NONE), false);
                }
            }
            _ => {}
        }
    }

    fn handle_resume_picker_mouse(&mut self, mouse: MouseEvent) -> Option<FileBackedAction> {
        let total = self.resume_picker.total_rows();
        if total == 0 {
            return None;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.resume_picker.selected = self.resume_picker.selected.saturating_sub(1);
                None
            }
            MouseEventKind::ScrollDown => {
                self.resume_picker.selected = (self.resume_picker.selected + 1).min(total - 1);
                None
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let (_, origin_y) = self.last_frame_origin.get();
                let rel_row = mouse.row.checked_sub(origin_y)? as usize;
                let clicked_idx = super::super::resume_picker::row_to_item_index(self, rel_row)?;
                if clicked_idx == self.resume_picker.selected {
                    self.resume_picker_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                } else {
                    self.resume_picker.selected = clicked_idx;
                    None
                }
            }
            _ => None,
        }
    }

    fn handle_model_chooser_mouse(
        &mut self,
        mouse: MouseEvent,
        has_pending_submission: bool,
    ) -> Option<FileBackedAction> {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.model_key(
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                has_pending_submission,
            ),
            MouseEventKind::ScrollDown => self.model_key(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                has_pending_submission,
            ),
            MouseEventKind::Down(MouseButton::Left) => {
                let menu_top = self.last_menu_origin_y.get()?;
                let rel_row = mouse.row.checked_sub(menu_top)? as usize;
                let state = self.completion.as_ref()?;
                let window_start = state
                    .selected
                    .saturating_sub(super::super::MAX_COMPLETION_ROWS - 1);
                let target = window_start + rel_row;
                if target >= self.model_chooser.filtered.len() {
                    return None;
                }
                if target == self.model_chooser.index {
                    self.model_key(
                        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                        has_pending_submission,
                    )
                } else {
                    self.model_chooser.index = target;
                    self.model_chooser.effort_index = 0;
                    self.refresh_model_choice_public();
                    None
                }
            }
            _ => None,
        }
    }

    /// Switch the active inspector modal tab and initialize immediate content.
    pub(in crate::file_backed) fn switch_modal_mode(&mut self, mode: ModalMode) {
        self.modal.reset_for_mode(mode, true);
        self.modal.owner_path = self.agent_path.clone();
        match mode {
            ModalMode::Tape => {
                self.modal.rows = super::super::tape_view::build_tape_lines_from_app(self);
            }
            ModalMode::Status => {
                self.modal.rows = super::super::status_panel::build_rows(self);
            }
            ModalMode::Actions | ModalMode::Plans | ModalMode::Man => {}
        }
    }
}
