use super::*;

impl FileBackedApp {
    pub(in crate::file_backed) fn handle_form_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<FileBackedAction> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => Some(FileBackedAction::Interrupt),
            KeyCode::Tab => {
                if let Some(form) = self.form.as_mut() {
                    form.next_field();
                }
                None
            }
            KeyCode::BackTab => {
                if let Some(form) = self.form.as_mut() {
                    form.prev_field();
                }
                None
            }
            KeyCode::Down => {
                if let Some(form) = self.form.as_mut() {
                    if form.focused_is_select() {
                        form.next_option();
                    } else {
                        form.next_field();
                    }
                }
                None
            }
            KeyCode::Up => {
                if let Some(form) = self.form.as_mut() {
                    if form.focused_is_select() {
                        form.prev_option();
                    } else {
                        form.prev_field();
                    }
                }
                None
            }
            KeyCode::Left if !ctrl && !alt => {
                if let Some(form) = self.form.as_mut() {
                    form.move_cursor_left();
                }
                None
            }
            KeyCode::Right if !ctrl && !alt => {
                if let Some(form) = self.form.as_mut() {
                    form.move_cursor_right();
                }
                None
            }
            KeyCode::Home | KeyCode::Char('a') if key.code == KeyCode::Home || (ctrl && !alt) => {
                if let Some(form) = self.form.as_mut() {
                    form.move_home();
                }
                None
            }
            KeyCode::End | KeyCode::Char('e') if key.code == KeyCode::End || (ctrl && !alt) => {
                if let Some(form) = self.form.as_mut() {
                    form.move_end();
                }
                None
            }
            KeyCode::Delete => {
                if let Some(form) = self.form.as_mut() {
                    form.delete_forward();
                }
                None
            }
            KeyCode::Backspace => {
                if let Some(form) = self.form.as_mut() {
                    form.backspace();
                }
                None
            }
            KeyCode::Char(' ') if !ctrl && !alt => {
                if let Some(form) = self.form.as_mut()
                    && !form.toggle_selected_option()
                {
                    form.insert_char(' ');
                }
                None
            }
            KeyCode::Enter => {
                if let Some(form) = self.form.as_mut() {
                    form.select_current_option_if_single_empty();
                }
                self.submit_form()
            }
            KeyCode::Char(_) if ctrl || alt => None,
            KeyCode::Char(ch) => {
                if let Some(form) = self.form.as_mut() {
                    form.insert_char(ch);
                }
                None
            }
            _ => None,
        }
    }

    pub(in crate::file_backed) fn confirmation_keypress(
        &mut self,
        key: KeyEvent,
    ) -> Option<FileBackedAction> {
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
                self.notice = Some(Notice::warning(message));
                None
            }
        }
    }
}
