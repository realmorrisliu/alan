//! Submission, resume response, and queue-continue helpers for `FileBackedApp`.

use super::*;

impl FileBackedApp {
    pub(in crate::file_backed) fn submit_form(&mut self) -> Option<FileBackedAction> {
        if self.pending_project_control.is_some() {
            self.notice =
                Some("project cwd selection is pending; wait for terminal Action evidence".into());
            return None;
        }
        if self.response_in_flight.is_some() {
            self.notice = Some("request response is still being sent".into());
            return None;
        }
        let pending = self.pending_yield.clone()?;
        let form = self.form.as_mut()?;
        let retry_input = form.answers_json();
        match pending.resume_content(&retry_input) {
            Ok(content) => Some(FileBackedAction::Resume {
                request_id: pending.request_id,
                response: response_text_from_content(content),
                retry_input,
            }),
            Err(message) => {
                form.error = Some(message);
                None
            }
        }
    }

    pub(in crate::file_backed) fn handle_submit(&mut self) -> Option<FileBackedAction> {
        self.submit_with_mode(alan_agent_protocol::InputMode::FollowUp)
    }

    pub(in crate::file_backed) fn submit_with_mode(
        &mut self,
        mode: alan_agent_protocol::InputMode,
    ) -> Option<FileBackedAction> {
        if self.model_submit_command() {
            let text = self.composer.text().trim().to_owned();
            self.composer.set_text("");
            self.composer.remember(&text);
            self.completion = None;
            self.notice = None;
            return self.model_command("");
        }
        if self.response_in_flight.is_some() {
            self.notice = Some("request response is still being sent".into());
            return None;
        }
        if self.project_selection.is_some() {
            let path = self.composer.text().trim();
            if path.is_empty() {
                self.notice = Some("enter a project directory path".into());
                return None;
            }
            return Some(FileBackedAction::Project(ProjectControl::Mount {
                operation_id: project_dispatch::project_operation_id(),
                host_path: std::path::PathBuf::from(path),
                access: self.project_selection.unwrap_or(ProjectAccess::ReadWrite),
            }));
        }
        if self.confirmation.is_some() {
            return None;
        }
        if let Some(pending) = self.pending_yield.clone() {
            let text = self.composer.text().trim().to_string();
            self.completion = None;
            match pending.resume_content(&text) {
                Ok(content) => {
                    self.stage_fallback_confirmation_outcome(&pending, &text);
                    self.composer.set_text("");
                    self.input_intent = InputIntent::Agent;
                    self.history_draft_intent = None;
                    return Some(FileBackedAction::Resume {
                        request_id: pending.request_id,
                        response: response_text_from_content(content),
                        retry_input: text,
                    });
                }
                Err(message) => {
                    self.notice = Some(Notice::warning(message));
                    return None;
                }
            }
        }

        let text = self.composer.text().to_owned();
        if text.trim().is_empty()
            && !(self.input_intent != InputIntent::Command && !self.pending_attachments.is_empty())
        {
            if self.input_intent != InputIntent::Agent {
                self.notice = Some("Enter content after the input prefix.".into());
            }
            return None;
        }
        self.notice = None;
        self.completion = None;
        if self.input_intent == InputIntent::Agent && looks_like_slash_command(text.trim()) {
            if mode == alan_agent_protocol::InputMode::Steer {
                self.notice = Some("Slash commands cannot be sent as live steer messages.".into());
                return None;
            }
            self.composer.set_text("");
            self.composer.remember(&text);
            return self.handle_command(text.trim());
        }
        let mut parts = Vec::new();
        let mut seen_hashes = std::collections::BTreeSet::new();
        for part in self.pending_attachments.iter().cloned() {
            super::super::attachment_io::push_deduped_attachment(
                &mut parts,
                &mut seen_hashes,
                part,
            );
        }
        if self.input_intent != InputIntent::Command {
            for part in super::super::attachment_io::extract_prompt_image_attachments(
                &text,
                self.project_host_root.as_deref(),
            ) {
                super::super::attachment_io::push_deduped_attachment(
                    &mut parts,
                    &mut seen_hashes,
                    part,
                );
            }
        }
        let record = alan_agent_protocol::UserInputRecord::new(self.input_intent, mode, text)
            .with_parts(parts);
        self.auto_continue_on_empty_cancel = false;
        Some(FileBackedAction::Submit(record))
    }

    pub(in crate::file_backed) fn begin_resume_write(&mut self, request_id: String) {
        self.response_in_flight = Some(request_id);
        if let Some(conf) = self.confirmation.as_mut() {
            conf.error = None;
            if self
                .notice
                .as_deref()
                .is_some_and(|text| text.starts_with("resume failed:"))
            {
                self.notice = None;
            }
        } else {
            self.notice = Some("sending response…".into());
        }
    }

    pub(super) fn finish_resume_write(
        &mut self,
        request_id: &str,
        retry_input: &str,
        result: Result<(), String>,
    ) {
        if self.response_in_flight.as_deref() != Some(request_id) {
            return;
        }
        self.response_in_flight = None;
        match result {
            Ok(()) => {
                let settled_conf = self.settle_confirmation_on_resume_ok(request_id);
                if self
                    .pending_yield
                    .as_ref()
                    .is_some_and(|pending| pending.request_id == request_id)
                {
                    self.clear_pending_yield();
                }
                if settled_conf {
                    if self.notice.as_deref().is_some_and(|text| {
                        text == "sending response…"
                            || text == "request response is still being sent"
                            || text.starts_with("resume failed:")
                    }) {
                        self.notice = None;
                    }
                } else {
                    self.notice = Some("response sent".into());
                }
            }
            Err(error) => {
                self.staged_confirmation_outcome = None;
                self.notice = Some(Notice::error(format!("resume failed: {error}")));
                if self
                    .pending_yield
                    .as_ref()
                    .is_some_and(|pending| pending.request_id == request_id)
                    && self.form.is_none()
                    && self.confirmation.is_none()
                {
                    self.composer.set_text(retry_input);
                }
            }
        }
    }

    pub(in crate::file_backed) fn accept_input(&mut self) {
        let body = self.composer.text().to_owned();
        self.composer.remember_input(&body, self.input_intent);
        self.composer.set_text("");
        self.input_intent = InputIntent::Agent;
        self.history_draft_intent = None;
        self.last_input_failed = false;
        self.expected_terminal_error = None;
        self.pending_attachments.clear();
        self.attachment_revision += 1;
        // Submission may be queued behind another client. Tape owns turn boundaries.
    }

    pub(in crate::file_backed) fn interrupt_action(&mut self) -> FileBackedAction {
        self.auto_continue_on_empty_cancel = self
            .queue
            .snapshot
            .as_ref()
            .is_none_or(|q| q.pending_submission_ids.is_empty());
        FileBackedAction::Interrupt
    }

    pub(in crate::file_backed) fn take_ready_auto_continue_on_empty_cancel(&mut self) -> bool {
        if !self.auto_continue_on_empty_cancel
            || self.has_pending_response()
            || !self.running_tools.is_empty()
        {
            return false;
        }
        let Some(q) = self.queue.snapshot.as_ref().filter(|q| q.known) else {
            return false;
        };
        if q.paused
            && q.pending_submission_ids.is_empty()
            && q.active_submission_ids.is_empty()
            && !q.deferred
            && q.uncertain_submission_ids.is_empty()
        {
            self.auto_continue_on_empty_cancel = false;
            return true;
        }
        false
    }

    #[cfg(test)]
    pub(in crate::file_backed) fn enter_submits_agent_task(&self) -> bool {
        if self.form.is_some() || self.pending_yield.is_some() || self.completion.is_some() {
            return false;
        }
        let text = self.composer.text().trim();
        let has_agent_attachments =
            self.input_intent != InputIntent::Command && !self.pending_attachments.is_empty();
        (!text.is_empty() || has_agent_attachments)
            && (self.input_intent != InputIntent::Agent || !looks_like_slash_command(text))
    }
}

pub(super) fn looks_like_slash_command(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let first = rest
        .split(|ch: char| ch.is_whitespace())
        .next()
        .unwrap_or("");
    !first.is_empty() && !first.contains('/')
}
