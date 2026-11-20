//! Interactive `/login` TUI flow state machine and key/result handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::completion::{self, CompletionCandidate, CompletionKind, CompletionState};

use super::FileBackedApp;
use super::app::{FileBackedAction, Notice};
use super::login::{
    LoginInputField, LoginProgress, LoginProgressSender, LoginProviderChoice, LoginResult,
    LoginSelectOption, LoginSubmission,
};

#[derive(Clone, Default)]
pub(in crate::file_backed) struct LoginFlowState {
    pub generation: u64,
    phase: LoginPhase,
}

#[derive(Clone, Default)]
enum LoginPhase {
    #[default]
    Idle,
    LoadingProviders,
    SelectingProvider {
        choices: Vec<LoginProviderChoice>,
        filtered: Vec<usize>,
        query: String,
        selected: usize,
    },
    WaitingHost {
        status: String,
        browser_url: Option<String>,
        user_code: Option<String>,
        hint: Option<String>,
    },
    SelectingOption {
        title: String,
        prompt: String,
        options: Vec<LoginSelectOption>,
        filtered: Vec<usize>,
        query: String,
        selected: usize,
        submission: LoginSubmission,
    },
    EnteringInput {
        field: LoginInputField,
        title: String,
        prompt: String,
        masked: bool,
        allow_empty: bool,
        buffer: String,
        cursor: usize,
        submission: LoginSubmission,
    },
}

impl LoginFlowState {
    pub(in crate::file_backed) fn is_active(&self) -> bool {
        !matches!(self.phase, LoginPhase::Idle)
    }

    pub(in crate::file_backed) fn is_entering_input(&self) -> bool {
        matches!(self.phase, LoginPhase::EnteringInput { .. })
    }

    pub(in crate::file_backed) fn input_display(&self) -> Option<(&str, usize, bool)> {
        match &self.phase {
            LoginPhase::EnteringInput {
                buffer,
                cursor,
                masked,
                ..
            } => Some((buffer.as_str(), *cursor, *masked)),
            _ => None,
        }
    }

    pub(in crate::file_backed) fn close(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.phase = LoginPhase::Idle;
    }

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    fn push_text_to_input(&mut self, text: &str) {
        if let LoginPhase::EnteringInput { buffer, cursor, .. } = &mut self.phase {
            let clean: String = text.chars().filter(|c| !c.is_control()).collect();
            if clean.is_empty() {
                return;
            }
            let pos = (*cursor).min(buffer.len());
            buffer.insert_str(pos, &clean);
            *cursor = pos + clean.len();
        }
    }
}

impl FileBackedApp {
    /// Entry point for `/login [provider]`.
    pub(in crate::file_backed) fn login_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        if self.turn_active() || self.pending_yield.is_some() || !self.running_tools.is_empty() {
            self.notice = Some(Notice::warning(
                "Wait for or interrupt the active turn before /login.",
            ));
            return None;
        }
        if self.project_selection.is_some() || self.pending_project_control.is_some() {
            self.notice = Some(Notice::warning(
                "Finish or cancel the active project selection before /login.",
            ));
            return None;
        }
        if !self.login_available {
            self.notice = Some(Notice::warning(
                "In-TUI /login is unavailable in this Host; run `alan login` from your terminal.",
            ));
            return None;
        }
        self.modal.active = false;
        self.model_chooser.active = false;
        self.resume_picker.close();
        self.fork_picker.close();
        self.completion = None;

        let provider = argument.trim();
        let generation = self.login.next_generation();
        if provider.is_empty() {
            self.login.phase = LoginPhase::LoadingProviders;
            self.notice = Some("Loading available providers…".into());
            Some(FileBackedAction::LoginListProviders { generation })
        } else {
            let (progress, rx) = LoginProgressSender::channel();
            let submission = LoginSubmission::new(provider.to_string(), None, progress);
            self.pending_login_submission = Some(submission.clone());
            self.login.phase = LoginPhase::WaitingHost {
                status: format!("Connecting {provider}…"),
                browser_url: None,
                user_code: None,
                hint: None,
            };
            self.refresh_login_notice();
            Some(FileBackedAction::LoginSubmit {
                generation,
                submission,
                progress_rx: Some(rx),
            })
        }
    }

    pub(in crate::file_backed) fn apply_login_providers(
        &mut self,
        generation: u64,
        result: Result<LoginResult, String>,
    ) {
        if generation != self.login.generation || !self.login.is_active() {
            return;
        }
        match result {
            Ok(LoginResult::Providers(choices)) => {
                if choices.is_empty() {
                    self.login.close();
                    self.completion = None;
                    self.notice = Some(Notice::warning(
                        "No login providers available; run `alan login` or `alan connection add`.",
                    ));
                    return;
                }
                let filtered = (0..choices.len()).collect();
                self.login.phase = LoginPhase::SelectingProvider {
                    choices,
                    filtered,
                    query: String::new(),
                    selected: 0,
                };
                self.refresh_login_completion();
            }
            Ok(other) => self.apply_login_step_result_inner(generation, Ok(other)),
            Err(error) => {
                self.login.close();
                self.completion = None;
                self.notice = Some(Notice::error(format!("Failed to load providers: {error}")));
            }
        }
    }

    pub(in crate::file_backed) fn apply_login_progress(
        &mut self,
        generation: u64,
        progress: LoginProgress,
    ) {
        if generation != self.login.generation {
            return;
        }
        if let LoginPhase::WaitingHost {
            status,
            browser_url,
            user_code,
            hint,
        } = &mut self.login.phase
        {
            match progress {
                LoginProgress::Status(msg) => *status = msg,
                LoginProgress::BrowserPrompt {
                    url,
                    user_code: code,
                    hint: next_hint,
                } => {
                    *browser_url = Some(url);
                    *user_code = code;
                    *hint = next_hint;
                }
            }
            self.refresh_login_notice();
        }
    }

    /// Apply a completed `LoginControl::Submit` step result; returns `(profile, model)` when `Connected`.
    pub(in crate::file_backed) fn apply_login_step_result(
        &mut self,
        generation: u64,
        result: Result<LoginResult, String>,
    ) -> Option<(String, Option<String>)> {
        if generation != self.login.generation || !self.login.is_active() {
            return None;
        }
        if let Ok(LoginResult::Connected {
            ref profile,
            ref provider,
            ref model,
            ref message,
        }) = result
        {
            let connected_profile = profile.clone();
            let connected_model = model.clone().filter(|m| !m.trim().is_empty());
            self.login.close();
            self.completion = None;
            self.connection_configured = Some(true);
            if let Some(ref m) = connected_model {
                self.set_effective_model(Some(m.clone()));
            }
            self.last_input_failed = false;
            self.expected_terminal_error = None;
            self.refresh_welcome_header();
            let summary = message.clone().unwrap_or_else(|| {
                let model_suffix = connected_model
                    .as_deref()
                    .map_or_else(String::new, |m| format!(" · model {m}"));
                format!("Connected {provider} ({profile}){model_suffix}")
            });
            self.notice = Some(summary.into());
            return Some((connected_profile, connected_model));
        }
        self.apply_login_step_result_inner(generation, result);
        None
    }

    fn apply_login_step_result_inner(
        &mut self,
        generation: u64,
        result: Result<LoginResult, String>,
    ) {
        if generation != self.login.generation {
            return;
        }
        match result {
            Ok(LoginResult::Providers(choices)) => {
                let filtered = (0..choices.len()).collect();
                self.login.phase = LoginPhase::SelectingProvider {
                    choices,
                    filtered,
                    query: String::new(),
                    selected: 0,
                };
                self.refresh_login_completion();
            }
            Ok(LoginResult::NeedsSelection {
                title,
                prompt,
                options,
            }) => {
                if options.is_empty() {
                    self.login.close();
                    self.completion = None;
                    self.notice = Some(Notice::warning(format!(
                        "{title}: no selectable options returned."
                    )));
                    return;
                }
                let prior = self.take_pending_submission().unwrap_or_else(|| {
                    let (progress, _) = LoginProgressSender::channel();
                    LoginSubmission::new(String::new(), None, progress)
                });
                let filtered = (0..options.len()).collect();
                self.login.phase = LoginPhase::SelectingOption {
                    title,
                    prompt,
                    options,
                    filtered,
                    query: String::new(),
                    selected: 0,
                    submission: prior,
                };
                self.refresh_login_completion();
            }
            Ok(LoginResult::NeedsInput {
                field,
                title,
                prompt,
                masked,
                allow_empty,
                default_value,
            }) => {
                let prior = self.take_pending_submission().unwrap_or_else(|| {
                    let (progress, _) = LoginProgressSender::channel();
                    LoginSubmission::new(String::new(), None, progress)
                });
                let buffer = if masked {
                    String::new()
                } else {
                    default_value.unwrap_or_default()
                };
                let cursor = buffer.len();
                self.completion = None;
                self.login.phase = LoginPhase::EnteringInput {
                    field,
                    title,
                    prompt,
                    masked,
                    allow_empty,
                    buffer,
                    cursor,
                    submission: prior,
                };
                self.refresh_login_notice();
            }
            Ok(LoginResult::Failed { message }) => {
                self.login.close();
                self.completion = None;
                self.notice = Some(Notice::warning(message));
            }
            Ok(LoginResult::Connected { .. }) => {}
            Err(error) => {
                self.login.close();
                self.completion = None;
                self.notice = Some(Notice::error(format!("Login failed: {error}")));
            }
        }
    }

    fn take_pending_submission(&mut self) -> Option<LoginSubmission> {
        match std::mem::replace(&mut self.login.phase, LoginPhase::Idle) {
            LoginPhase::SelectingOption { submission, .. }
            | LoginPhase::EnteringInput { submission, .. } => Some(submission),
            LoginPhase::WaitingHost { .. } => self.pending_login_submission.take(),
            other => {
                self.login.phase = other;
                None
            }
        }
    }

    pub(in crate::file_backed) fn login_paste(&mut self, text: &str) {
        self.login.push_text_to_input(text);
    }

    pub(in crate::file_backed) fn login_key(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        if !self.login.is_active() {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('c')) {
            self.login.close();
            self.pending_login_submission = None;
            self.completion = None;
            self.notice = Some("Login cancelled.".into());
            return Some(FileBackedAction::CancelLogin);
        }
        match &mut self.login.phase {
            LoginPhase::Idle => None,
            LoginPhase::LoadingProviders | LoginPhase::WaitingHost { .. } => None,
            LoginPhase::SelectingProvider {
                choices,
                filtered,
                query,
                selected,
            } => {
                match key.code {
                    KeyCode::Up if !filtered.is_empty() => {
                        *selected = if *selected == 0 {
                            filtered.len() - 1
                        } else {
                            *selected - 1
                        };
                        self.refresh_login_completion();
                    }
                    KeyCode::Down if !filtered.is_empty() => {
                        *selected = (*selected + 1) % filtered.len();
                        self.refresh_login_completion();
                    }
                    KeyCode::Backspace if !query.is_empty() => {
                        query.pop();
                        *selected = 0;
                        self.refresh_login_completion();
                    }
                    KeyCode::Char('u') if ctrl && !query.is_empty() => {
                        query.clear();
                        *selected = 0;
                        self.refresh_login_completion();
                    }
                    KeyCode::Char(ch) if !ctrl && !alt && !ch.is_control() => {
                        query.push(ch);
                        *selected = 0;
                        self.refresh_login_completion();
                    }
                    KeyCode::Enter => {
                        let choice_idx = *filtered.get(*selected)?;
                        let choice = choices.get(choice_idx)?.clone();
                        let (progress, rx) = LoginProgressSender::channel();
                        let submission = LoginSubmission::new(
                            choice.id.clone(),
                            Some(choice.default_profile.clone()),
                            progress,
                        );
                        let generation = self.login.next_generation();
                        self.pending_login_submission = Some(submission.clone());
                        self.completion = None;
                        self.login.phase = LoginPhase::WaitingHost {
                            status: format!("Connecting {}…", choice.label),
                            browser_url: None,
                            user_code: None,
                            hint: None,
                        };
                        self.refresh_login_notice();
                        return Some(FileBackedAction::LoginSubmit {
                            generation,
                            submission,
                            progress_rx: Some(rx),
                        });
                    }
                    _ => {}
                }
                None
            }
            LoginPhase::SelectingOption {
                options,
                filtered,
                query,
                selected,
                submission,
                ..
            } => {
                match key.code {
                    KeyCode::Up if !filtered.is_empty() => {
                        *selected = if *selected == 0 {
                            filtered.len() - 1
                        } else {
                            *selected - 1
                        };
                        self.refresh_login_completion();
                    }
                    KeyCode::Down if !filtered.is_empty() => {
                        *selected = (*selected + 1) % filtered.len();
                        self.refresh_login_completion();
                    }
                    KeyCode::Backspace if !query.is_empty() => {
                        query.pop();
                        *selected = 0;
                        self.refresh_login_completion();
                    }
                    KeyCode::Char('u') if ctrl && !query.is_empty() => {
                        query.clear();
                        *selected = 0;
                        self.refresh_login_completion();
                    }
                    KeyCode::Char(ch) if !ctrl && !alt && !ch.is_control() => {
                        query.push(ch);
                        *selected = 0;
                        self.refresh_login_completion();
                    }
                    KeyCode::Enter => {
                        let option_idx = *filtered.get(*selected)?;
                        let opt = options.get(option_idx)?.clone();
                        let mut next_submission = submission.clone();
                        let (progress, rx) = LoginProgressSender::channel();
                        next_submission.progress = progress;
                        next_submission.selection = Some(opt.id.clone());
                        next_submission.selections.push(opt.id.clone());
                        let generation = self.login.next_generation();
                        self.pending_login_submission = Some(next_submission.clone());
                        self.completion = None;
                        self.login.phase = LoginPhase::WaitingHost {
                            status: format!("Completing login with {}…", opt.label),
                            browser_url: None,
                            user_code: None,
                            hint: None,
                        };
                        self.refresh_login_notice();
                        return Some(FileBackedAction::LoginSubmit {
                            generation,
                            submission: next_submission,
                            progress_rx: Some(rx),
                        });
                    }
                    _ => {}
                }
                None
            }
            LoginPhase::EnteringInput {
                field,
                allow_empty,
                buffer,
                cursor,
                submission,
                ..
            } => {
                match key.code {
                    KeyCode::Left if !ctrl && !alt => {
                        if *cursor > 0 {
                            let prev = buffer[..*cursor]
                                .char_indices()
                                .last()
                                .map_or(0, |(idx, _)| idx);
                            *cursor = prev;
                        }
                    }
                    KeyCode::Right if !ctrl && !alt => {
                        if *cursor < buffer.len() {
                            let next = buffer[*cursor..]
                                .char_indices()
                                .nth(1)
                                .map_or(buffer.len(), |(idx, _)| *cursor + idx);
                            *cursor = next;
                        }
                    }
                    KeyCode::Home | KeyCode::Char('a') if key.code == KeyCode::Home || ctrl => {
                        *cursor = 0;
                    }
                    KeyCode::End | KeyCode::Char('e') if key.code == KeyCode::End || ctrl => {
                        *cursor = buffer.len();
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            let prev = buffer[..*cursor]
                                .char_indices()
                                .last()
                                .map_or(0, |(idx, _)| idx);
                            buffer.drain(prev..*cursor);
                            *cursor = prev;
                        }
                    }
                    KeyCode::Delete => {
                        if *cursor < buffer.len() {
                            let next = buffer[*cursor..]
                                .char_indices()
                                .nth(1)
                                .map_or(buffer.len(), |(idx, _)| *cursor + idx);
                            buffer.drain(*cursor..next);
                        }
                    }
                    KeyCode::Char('u') if ctrl => {
                        buffer.clear();
                        *cursor = 0;
                    }
                    KeyCode::Char(ch) if !ctrl && !alt && !ch.is_control() => {
                        let pos = (*cursor).min(buffer.len());
                        buffer.insert(pos, ch);
                        *cursor = pos + ch.len_utf8();
                    }
                    KeyCode::Enter => {
                        let trimmed = buffer.trim().to_string();
                        if !*allow_empty && trimmed.is_empty() {
                            self.notice =
                                Some(Notice::warning("Input cannot be empty (Esc to cancel)."));
                            return None;
                        }
                        let mut next_submission = submission.clone();
                        let (progress, rx) = LoginProgressSender::channel();
                        next_submission.progress = progress;
                        match *field {
                            LoginInputField::Secret => next_submission.secret = Some(trimmed),
                            LoginInputField::BaseUrl => next_submission.base_url = Some(trimmed),
                            LoginInputField::ProjectId => {
                                next_submission.project_id = Some(trimmed)
                            }
                            LoginInputField::AuthCode => next_submission.auth_code = Some(trimmed),
                            LoginInputField::ProfileId => {
                                next_submission.profile_id = Some(trimmed)
                            }
                        }
                        buffer.clear();
                        *cursor = 0;
                        let generation = self.login.next_generation();
                        self.pending_login_submission = Some(next_submission.clone());
                        self.login.phase = LoginPhase::WaitingHost {
                            status: "Verifying credentials…".into(),
                            browser_url: None,
                            user_code: None,
                            hint: None,
                        };
                        self.refresh_login_notice();
                        return Some(FileBackedAction::LoginSubmit {
                            generation,
                            submission: next_submission,
                            progress_rx: Some(rx),
                        });
                    }
                    _ => {}
                }
                None
            }
        }
    }

    fn refresh_login_completion(&mut self) {
        match &mut self.login.phase {
            LoginPhase::SelectingProvider {
                choices,
                filtered,
                query,
                selected,
            } => {
                let mut scored: Vec<(i32, usize)> = choices
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, choice)| {
                        let haystack = format!("{} {}", choice.id, choice.label);
                        completion::fuzzy_score(&haystack, query).map(|s| (s, idx))
                    })
                    .collect();
                if !query.is_empty() {
                    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                }
                *filtered = scored.into_iter().map(|(_, idx)| idx).collect();
                if filtered.is_empty() {
                    *selected = 0;
                } else {
                    *selected = (*selected).min(filtered.len() - 1);
                }
                let matches = filtered
                    .iter()
                    .map(|&idx| {
                        let c = &choices[idx];
                        CompletionCandidate {
                            value: c.id.clone(),
                            label: c.label.clone(),
                            detail: Some(c.detail.clone()),
                        }
                    })
                    .collect();
                self.completion = Some(CompletionState {
                    kind: CompletionKind::Command,
                    token_start: 0,
                    query: query.clone(),
                    matches,
                    selected: *selected,
                });
                let filter_part = if query.is_empty() {
                    String::new()
                } else {
                    format!(" · filter: \"{query}\"")
                };
                self.notice = Some(
                    format!("Select provider{filter_part} · ↑↓ move · type to filter · Enter connect · Esc cancel")
                        .into(),
                );
            }
            LoginPhase::SelectingOption {
                title,
                prompt,
                options,
                filtered,
                query,
                selected,
                ..
            } => {
                let mut scored: Vec<(i32, usize)> = options
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, opt)| {
                        let haystack = format!(
                            "{} {} {}",
                            opt.label,
                            opt.id,
                            opt.detail.as_deref().unwrap_or("")
                        );
                        completion::fuzzy_score(&haystack, query).map(|s| (s, idx))
                    })
                    .collect();
                if !query.is_empty() {
                    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                }
                *filtered = scored.into_iter().map(|(_, idx)| idx).collect();
                if filtered.is_empty() {
                    *selected = 0;
                } else {
                    *selected = (*selected).min(filtered.len() - 1);
                }
                let matches = filtered
                    .iter()
                    .map(|&idx| {
                        let o = &options[idx];
                        CompletionCandidate {
                            value: o.id.clone(),
                            label: o.label.clone(),
                            detail: o.detail.clone(),
                        }
                    })
                    .collect();
                self.completion = Some(CompletionState {
                    kind: CompletionKind::Command,
                    token_start: 0,
                    query: query.clone(),
                    matches,
                    selected: *selected,
                });
                let filter_part = if query.is_empty() {
                    String::new()
                } else {
                    format!(" · filter: \"{query}\"")
                };
                self.notice = Some(format!("{title}{filter_part}\n{prompt}").into());
            }
            _ => {}
        }
    }

    fn refresh_login_notice(&mut self) {
        match &self.login.phase {
            LoginPhase::WaitingHost {
                status,
                browser_url,
                user_code,
                hint,
            } => {
                let mut lines = vec![status.clone()];
                if let Some(url) = browser_url {
                    lines.push(format!("Open URL: {url}"));
                }
                if let Some(code) = user_code {
                    lines.push(format!("Verification code: {code}"));
                }
                if let Some(hint) = hint {
                    lines.push(format!("{hint} (Esc to cancel)"));
                } else {
                    lines.push("Waiting for completion… (Esc to cancel)".to_string());
                }
                self.notice = Some(lines.join("\n").into());
            }
            LoginPhase::EnteringInput { title, prompt, .. } => {
                self.notice =
                    Some(format!("{title}\n{prompt} (Enter confirm · Esc cancel)").into());
            }
            _ => {}
        }
    }

    pub(in crate::file_backed) fn refresh_welcome_header(&mut self) {
        if matches!(
            self.transcript.first(),
            Some(crate::history::HistoryCell::Welcome(_))
        ) {
            let updated = super::welcome::from_app(self);
            if let Some(crate::history::HistoryCell::Welcome(banner)) = self.transcript.first_mut()
            {
                *banner = updated;
            }
        }
    }

    pub(in crate::file_backed) fn is_connection_unconfigured(&self) -> bool {
        self.connection_configured == Some(false)
            && self.model.prompt_binding(false).is_none()
            && self.effective_model.is_none()
    }
}
