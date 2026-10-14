use super::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl FileBackedApp {
    pub(in crate::file_backed) fn has_pending_response(&self) -> bool {
        self.pending_yield.is_some()
            || self.form.is_some()
            || !self.activity.waiting_submission_ids.is_empty()
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
                if project.access == ProjectAccess::ReadOnly {
                    format!("{label} · read-only")
                } else {
                    label
                }
            },
        );
        let mut model = self.model.prompt_label(self.prefers_active_model());
        if UnicodeWidthStr::width(model.as_str()) > width
            && let Some(binding) = self.model.prompt_binding(self.prefers_active_model())
        {
            model = super::super::model::safe(&binding.model);
        }
        let model = truncate_middle(&model, width);
        let location = truncate_middle(
            &location,
            width.saturating_sub(UnicodeWidthStr::width(model.as_str()) + 3),
        );
        let mut spans = Vec::new();
        if !location.is_empty() {
            spans.push(Span::styled(location, Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::raw(model));
        Line::from(spans)
    }

    fn prefers_active_model(&self) -> bool {
        self.activity.state != UiActivityState::Idle
            || self.pending_yield.is_some()
            || self.form.is_some()
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
        } else if !self.queue.owner.is_empty() {
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
                Span::styled(prompt, Style::default().fg(Color::Green)),
                Span::raw((*segment).to_string()),
            ]));
        }
        if lines.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                self.input_prompt_prefix(),
                Style::default().fg(Color::Green),
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
