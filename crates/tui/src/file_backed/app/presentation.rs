use super::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl FileBackedApp {
    pub(in crate::file_backed) fn has_pending_response(&self) -> bool {
        self.pending_yield.is_some()
            || self.form.is_some()
            || !self.activity.waiting_submission_ids.is_empty()
    }

    pub(in crate::file_backed) fn window_title(&self) -> String {
        let project = self
            .project
            .as_ref()
            .map(|p| super::super::model::safe(&p.label))
            .or_else(|| {
                self.project_candidate
                    .as_ref()
                    .and_then(|p| p.file_name().and_then(|n| n.to_str()))
                    .map(str::to_string)
            });
        let state = self.activity_label().unwrap_or("ready");
        crate::terminal::format_osc_title(project.as_deref(), state)
    }

    pub(in crate::file_backed) fn activity_label(&self) -> Option<&str> {
        if self.has_pending_response() {
            return Some("waiting for input");
        }
        match self.activity.state {
            UiActivityState::Idle => None,
            UiActivityState::Paused
                if self.activity.waiting_submission_ids.is_empty()
                    && self.pending_yield.is_none()
                    && self.form.is_none()
                    && self.running_tools.is_empty()
                    && !self
                        .queue
                        .snapshot
                        .as_ref()
                        .is_some_and(|q| !q.active_submission_ids.is_empty()) =>
            {
                None
            }
            UiActivityState::Paused => Some("waiting for input"),
            UiActivityState::Running
                if matches!(self.thinking.state, UiThinkingState::Streaming) =>
            {
                Some("thinking")
            }
            UiActivityState::Running => Some("working"),
        }
    }

    pub(in crate::file_backed) fn context_line(&self, width: usize) -> Line<'static> {
        let location = self.project.as_ref().map_or_else(
            || {
                if self.namespace_cwd == std::path::Path::new("/") {
                    "Select project".to_string()
                } else {
                    self.namespace_cwd.display().to_string()
                }
            },
            |project| {
                let label = super::super::model::safe(&project.label);
                if project.access == ProjectAccess::ReadOnly && self.sandbox_backend.is_none() {
                    format!("{label} · read-only")
                } else {
                    label
                }
            },
        );
        let git_chip = self.git_status.as_ref().map(|git| {
            (
                git.label(),
                if git.dirty {
                    self.theme.wait_style()
                } else {
                    self.theme.run_style()
                },
            )
        });
        let dot_state = if self.expected_terminal_error.is_some() || self.last_input_failed {
            crate::theme::DotState::Block
        } else if self.pending_yield.is_some() || self.form.is_some() {
            crate::theme::DotState::Wait
        } else if self.turn_active() || self.project.is_some() {
            crate::theme::DotState::Run
        } else {
            crate::theme::DotState::Muted
        };
        let (dot, dot_style) = self.theme.status_dot(dot_state);
        let dot_width = UnicodeWidthStr::width(dot) + 1;
        let full_model = self.model.prompt_label(self.prefers_active_model());
        let short_model = self
            .model
            .prompt_binding(self.prefers_active_model())
            .map(|b| super::super::model::safe(&b.model))
            .unwrap_or_else(|| full_model.clone());
        let full_usage = self.usage.prompt_label();
        let short_usage = self.usage.short_prompt_label();
        let sandbox_chip = self.sandbox_trust_chip();

        // Priority degradation: keep model/effort while it fits width, then shrink
        // optional chips (sandbox -> usage details -> git -> location).
        let mut model = full_model;
        if UnicodeWidthStr::width(model.as_str()) + dot_width > width {
            model = short_model;
        }
        let mut usage = full_usage;
        let mut include_git = git_chip.is_some();
        let mut include_sandbox = sandbox_chip.is_some();

        let measure = |loc: &str, m: &str, g: bool, u: Option<&str>, s: bool| -> usize {
            let mut total = dot_width + UnicodeWidthStr::width(m);
            if !loc.is_empty() {
                total += UnicodeWidthStr::width(loc) + 3;
            }
            if g && let Some((ref gt, _)) = git_chip {
                total += UnicodeWidthStr::width(gt.as_str()) + 3;
            }
            if let Some(ut) = u {
                total += UnicodeWidthStr::width(ut) + 3;
            }
            if s && let Some(ref st) = sandbox_chip {
                total += UnicodeWidthStr::width(st.as_str()) + 3;
            }
            total
        };

        if measure(
            &location,
            &model,
            include_git,
            usage.as_deref(),
            include_sandbox,
        ) > width
        {
            include_sandbox = false;
        }
        if measure(
            &location,
            &model,
            include_git,
            usage.as_deref(),
            include_sandbox,
        ) > width
        {
            usage = short_usage;
        }
        if measure(
            &location,
            &model,
            include_git,
            usage.as_deref(),
            include_sandbox,
        ) > width
        {
            include_git = false;
        }
        let mut reserved = dot_width + UnicodeWidthStr::width(model.as_str()) + 3;
        if include_git && let Some((ref gt, _)) = git_chip {
            reserved += UnicodeWidthStr::width(gt.as_str()) + 3;
        }
        let show_usage = usage.as_ref().is_some_and(|ut| {
            let u_len = UnicodeWidthStr::width(ut.as_str()) + 3;
            if reserved + u_len + 6 <= width {
                reserved += u_len;
                true
            } else {
                false
            }
        });
        if !show_usage {
            usage = None;
        }
        if include_sandbox && let Some(ref st) = sandbox_chip {
            let s_len = UnicodeWidthStr::width(st.as_str()) + 3;
            if reserved + s_len + 6 <= width {
                reserved += s_len;
            } else {
                include_sandbox = false;
            }
        }
        let model = truncate_middle(&model, width.saturating_sub(dot_width));
        let location = truncate_middle(&location, width.saturating_sub(reserved));

        let mut spans = Vec::new();
        if width > dot_width {
            spans.push(Span::styled(format!("{dot} "), dot_style));
        }
        if !location.is_empty() {
            spans.push(Span::styled(location, self.theme.accent_style()));
            spans.push(Span::raw(" · "));
        }
        if include_git && let Some((gt, style)) = git_chip {
            spans.push(Span::styled(gt, style));
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::raw(model));
        if let Some(ut) = usage {
            spans.push(Span::styled(format!(" · {ut}"), self.theme.muted_style()));
        }
        if include_sandbox && let Some(st) = sandbox_chip {
            spans.push(Span::styled(format!(" · {st}"), self.theme.muted_style()));
        }
        Line::from(spans)
    }

    fn sandbox_trust_chip(&self) -> Option<String> {
        let trust = self.project.as_ref().map(|p| p.access.label());
        match (self.sandbox_backend.as_deref(), trust) {
            (Some(sb), Some(tr)) => Some(format!("{sb}/{tr}")),
            (Some(sb), None) => Some(sb.to_string()),
            (None, _) => None,
        }
    }

    pub(in crate::file_backed) fn interruptible_work_active(
        &self,
        has_pending_submission: bool,
    ) -> bool {
        has_pending_submission
            || self.turn_active()
            || self.pending_yield.is_some()
            || self.form.is_some()
            || self.has_pending_response()
            || self
                .queue
                .snapshot
                .as_ref()
                .is_some_and(|queue| queue.known && !queue.active_submission_ids.is_empty())
    }

    fn active_command_only(&self) -> bool {
        if self.has_pending_response()
            || self
                .model
                .known()
                .is_some_and(|snapshot| snapshot.active.is_some())
        {
            return false;
        }
        if let Some(queue) = self.queue.snapshot.as_ref().filter(|q| q.known)
            && !queue.active_submission_ids.is_empty()
        {
            return queue.active_submission_ids.iter().all(|id| {
                self.local_inputs
                    .get(id)
                    .is_some_and(|input| input.intent == InputIntent::Command)
            });
        }
        if self.activity.state == UiActivityState::Running
            && self
                .model
                .known()
                .is_some_and(|snapshot| snapshot.admitted.is_empty())
        {
            let mut active_local = self
                .local_inputs
                .values()
                .filter(|input| !input.terminal)
                .peekable();
            if active_local.peek().is_some() {
                return active_local.all(|input| input.intent == InputIntent::Command);
            }
            if matches!(
                self.transcript.last(),
                Some(HistoryCell::Command(_)) | Some(HistoryCell::InputTail { command: true, .. })
            ) {
                return true;
            }
        }
        false
    }

    fn prefers_active_model(&self) -> bool {
        if self.active_command_only() {
            return false;
        }
        self.activity.state != UiActivityState::Idle
            || self.has_pending_response()
            || self
                .queue
                .snapshot
                .as_ref()
                .is_some_and(|queue| queue.known && !queue.active_submission_ids.is_empty())
    }

    pub(in crate::file_backed) fn attention_labels(&self) -> Vec<String> {
        let mut labels = Vec::new();
        if self.project_selection.is_some() {
            labels.push("Selecting project".into());
        }
        if self
            .pending_project_control
            .as_ref()
            .is_some_and(|control| !control.fenced)
        {
            labels.push("Project opening".into());
        } else if let Some(project) = &self.project
            && self
                .namespace_cwd
                .strip_prefix(&project.namespace_path)
                .is_err()
        {
            labels.push("Project cwd unavailable".into());
        }
        if self.model_chooser.uncertain.is_some() {
            labels.push("Model outcome uncertain".into());
        } else if self.model_chooser.pending.is_some() {
            labels.push("Selecting model".into());
        }
        if self.activity.state == UiActivityState::Idle && self.last_input_failed {
            labels.push("Failed".into());
        }
        let queue = self.queue.snapshot.as_ref().filter(|q| q.known);
        if !self.has_pending_response()
            && !self.auto_continue_on_empty_cancel
            && (self.activity.state == UiActivityState::Paused || queue.is_some_and(|q| q.paused))
        {
            labels.push("Paused · /continue".into());
        }
        if let Some(q) = queue {
            if !q.pending_submission_ids.is_empty() {
                labels.push(format!("Queued {}", q.pending_submission_ids.len()));
            }
            if !q.uncertain_submission_ids.is_empty() {
                labels.push("Input outcome uncertain · /status".into());
            }
            if q.deferred {
                labels.push("Input deferred · /status".into());
            }
            if !q.active_submission_ids.is_empty()
                && self.activity.state != UiActivityState::Running
            {
                labels.push("Input active".into());
            }
        } else if !self.queue.owner.is_empty() && !self.queue.is_fresh_unknown() {
            labels.push("Queue unavailable · /status".into());
        }
        if self.prefers_active_model()
            && let Some(s) = self.model.known()
            && let (Some(active), Some(next)) = (&s.active, &s.selected_next)
            && active != next
        {
            labels.push(format!(
                "Next: {} · /status",
                super::super::model::safe(&next.model)
            ));
        }
        labels
    }

    pub(in crate::file_backed) fn composer_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let segments = self.composer.text().split('\n').collect::<Vec<_>>();
        for (idx, segment) in segments.iter().enumerate() {
            let prompt = if idx == 0 {
                self.input_prompt_prefix()
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                Span::styled(prompt, self.theme.run_style()),
                Span::raw((*segment).to_string()),
            ]));
        }
        if lines.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                self.input_prompt_prefix(),
                self.theme.run_style(),
            )]));
        }
        lines
    }

    pub(in crate::file_backed) fn input_prompt_prefix(&self) -> &'static str {
        if self.project_selection.is_some() {
            INLINE_COMMAND_PROMPT_PREFIX
        } else if self.pending_yield.is_some() {
            INLINE_WAITING_PROMPT_PREFIX
        } else if self.input_intent == InputIntent::Command {
            INLINE_COMMAND_PROMPT_PREFIX
        } else {
            INLINE_PROMPT_PREFIX
        }
    }
}

fn truncate_middle(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let left_limit = (max_width - 1) / 2;
    let right_limit = max_width - 1 - left_limit;
    let mut left = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let cells = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cells > left_limit {
            break;
        }
        left.push(ch);
        used += cells;
    }
    let mut right = String::new();
    used = 0;
    for ch in value.chars().rev() {
        let cells = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cells > right_limit {
            break;
        }
        right.insert(0, ch);
        used += cells;
    }
    format!("{left}…{right}")
}
