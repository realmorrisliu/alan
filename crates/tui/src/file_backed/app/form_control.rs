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

    #[cfg(test)]
    pub(in crate::file_backed) fn confirmation_keypress(
        &mut self,
        key: KeyEvent,
    ) -> Option<FileBackedAction> {
        match self.handle_confirmation_key(key) {
            ConfirmationKeyResult::Action(action) => Some(action),
            ConfirmationKeyResult::Swallowed | ConfirmationKeyResult::Ignored => None,
        }
    }

    pub(in crate::file_backed) fn handle_confirmation_key(
        &mut self,
        key: KeyEvent,
    ) -> ConfirmationKeyResult {
        if let Some(res) = self.handle_confirmation_overlay_key(key) {
            return res;
        }
        if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
            return ConfirmationKeyResult::Ignored;
        }
        if self.response_in_flight.is_some() {
            if self.composer.text().is_empty()
                && (matches!(key.code, KeyCode::Char(ch) if ('1'..='9').contains(&ch))
                    || key.code == KeyCode::Enter)
            {
                self.notice = Some("request response is still being sent".into());
                return ConfirmationKeyResult::Swallowed;
            }
            return ConfirmationKeyResult::Ignored;
        }
        if self.pending_yield.is_none()
            && self.form.is_none()
            && !self.activity.waiting_submission_ids.is_empty()
            && self.running_tools.is_empty()
            && self.composer.text().is_empty()
            && matches!(key.code, KeyCode::Char(ch) if ('1'..='9').contains(&ch))
        {
            self.notice = Some("waiting for prompt details…".into());
            return ConfirmationKeyResult::Swallowed;
        }
        let Some(pending) = self.pending_yield.as_ref() else {
            return ConfirmationKeyResult::Ignored;
        };
        if !matches!(pending.kind, YieldKind::Confirmation) || !self.composer.text().is_empty() {
            return ConfirmationKeyResult::Ignored;
        }
        let KeyCode::Char(ch) = key.code else {
            return ConfirmationKeyResult::Ignored;
        };
        let Some(digit) = ch.to_digit(10) else {
            return ConfirmationKeyResult::Ignored;
        };
        let Some(option) = digit
            .checked_sub(1)
            .and_then(|index| pending.options.get(index as usize))
            .cloned()
        else {
            if !pending.options.is_empty() {
                self.notice = Some(Notice::warning(format!(
                    "Select an option (1-{}) or type a response.",
                    pending.options.len()
                )));
                return ConfirmationKeyResult::Swallowed;
            }
            return ConfirmationKeyResult::Ignored;
        };
        let pending = pending.clone();
        match pending.resume_content(&option) {
            Ok(content) => ConfirmationKeyResult::Action(FileBackedAction::Resume {
                request_id: pending.request_id,
                response: response_text_from_content(content),
                retry_input: option,
            }),
            Err(message) => {
                self.notice = Some(Notice::warning(message));
                ConfirmationKeyResult::Swallowed
            }
        }
    }
}

#[derive(Debug)]
pub(in crate::file_backed) enum ConfirmationKeyResult {
    Ignored,
    Swallowed,
    Action(FileBackedAction),
}
