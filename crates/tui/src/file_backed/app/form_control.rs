use super::*;

impl FileBackedApp {
    pub(in crate::file_backed) fn set_pending_yield(&mut self, pending: PendingYieldCell) {
        if self.modal.active {
            self.modal.active = false;
            self.modal.generation += 1;
            self.modal.pending = false;
        }
        self.pending_yield = Some(pending.clone());
        self.sync_form();
        self.sync_confirmation();
        self.completion = None;
        if self.confirmation.is_some() {
            return;
        }
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

    pub(in crate::file_backed) fn clear_pending_yield(&mut self) {
        self.settle_confirmation_on_external_clear();
        self.pending_yield = None;
        self.form = None;
        self.confirmation = None;
        self.refresh_completion();
    }

    pub(in crate::file_backed) fn sync_form(&mut self) {
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

    pub(in crate::file_backed) fn handle_form_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<FileBackedAction> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => Some(self.interrupt_action()),
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
