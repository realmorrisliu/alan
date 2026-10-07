use super::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl FileBackedApp {
    pub(in crate::file_backed) fn activity_label(&self) -> Option<&str> {
        if self.pending_yield.is_some() || self.form.is_some() {
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
        let status = if self.project_selection.is_some() {
            "selecting project"
        } else if self
            .pending_yield
            .as_ref()
            .is_some_and(|pending| matches!(pending.kind, YieldKind::Confirmation))
        {
            "waiting for approval"
        } else {
            match self.activity.state {
                UiActivityState::Running => "working",
                UiActivityState::Paused => "paused",
                UiActivityState::Idle if self.last_input_failed => "failed",
                UiActivityState::Idle => "ready",
            }
        };
        let queue_label = self.queue.label();
        let status = if self.model_chooser.uncertain.is_some() {
            "model outcome uncertain"
        } else if self.model_chooser.pending.is_some() {
            "model pending"
        } else {
            status
        };
        let status_color = match status {
            "ready" => Color::Green,
            "working" | "selecting project" | "model pending" => Color::Cyan,
            "waiting for approval" | "paused" => Color::Yellow,
            _ => Color::Red,
        };
        let queue_label = if status == "paused" {
            queue_label
                .strip_prefix("paused · ")
                .unwrap_or(&queue_label)
        } else {
            &queue_label
        };
        let status = if width < 48 {
            match status {
                "selecting project" => "selecting",
                "waiting for approval" => "approval",
                "model outcome uncertain" => "model uncertain",
                other => other,
            }
        } else {
            status
        };
        let queue_label = if width < 64 {
            queue_label
                .replace("queued ", "q ")
                .replace("queue unknown", "q ?")
        } else {
            queue_label.to_string()
        };
        let core_status = status;
        let status = if self.queue.owner.is_empty() {
            status.to_string()
        } else {
            format!("{status} · {queue_label}")
        };
        let status = status.as_str();
        if width < 16 {
            return Line::styled(
                truncate_middle(status, width),
                Style::default().fg(status_color),
            );
        }
        let location = self.project.as_ref().map_or_else(
            || "no project".to_string(),
            |project| {
                if self
                    .pending_project_control
                    .as_ref()
                    .is_some_and(|control| !control.fenced)
                {
                    return format!("/{} · opening", project.label);
                }
                let location = match self.namespace_cwd.strip_prefix(&project.namespace_path) {
                    Ok(relative) if relative.as_os_str().is_empty() => {
                        format!("/{}", project.label)
                    }
                    Ok(relative) => format!("/{}/{}", project.label, relative.display()),
                    Err(_) => format!("/{} · cwd unavailable", project.label),
                };
                if project.access == ProjectAccess::ReadOnly {
                    format!("{location} · read-only")
                } else {
                    location
                }
            },
        );
        let model = if self.model.known().is_some() {
            self.model.header()
        } else {
            "model unknown".into()
        };
        let model = model.trim();
        let model_width = width.saturating_sub(
            UnicodeWidthStr::width(" ·  · ")
                + UnicodeWidthStr::width(status)
                + UnicodeWidthStr::width(location.as_str()).min(20),
        );
        let compact_model;
        let model = if UnicodeWidthStr::width(model) > model_width {
            compact_model = self.model.header_controls(false);
            compact_model.as_str()
        } else {
            model
        };
        let mut model = model.to_string();
        let mut status = status.to_string();
        if UnicodeWidthStr::width(model.as_str()) + UnicodeWidthStr::width(status.as_str()) + 3
            > width
        {
            // Preserve an urgent queue cue; counts and simultaneous details remain in /queue.
            let cue = if self.queue.owner.is_empty() || core_status.starts_with("model ") {
                None
            } else {
                match self.queue.snapshot.as_ref().filter(|q| q.known) {
                    None => Some("q ?"),
                    Some(q) if !q.uncertain_submission_ids.is_empty() => Some("uncertain"),
                    Some(q) if q.deferred => Some("deferred"),
                    Some(q) if q.paused && core_status != "paused" => Some("paused"),
                    Some(q) if !q.active_submission_ids.is_empty() => Some("active"),
                    Some(_) => None,
                }
            };
            status = cue.map_or_else(
                || core_status.to_string(),
                |cue| format!("{core_status} · {cue}"),
            );
            // Keep characters on both sides of a shortened model, not a bare ellipsis.
            if UnicodeWidthStr::width(status.as_str()) + 6 > width {
                status = cue
                    .unwrap_or(core_status)
                    .strip_prefix("model ")
                    .unwrap_or(cue.unwrap_or(core_status))
                    .to_string();
            }
            if UnicodeWidthStr::width(model.as_str()) + UnicodeWidthStr::width(status.as_str()) + 3
                > width
            {
                model = self
                    .model
                    .known()
                    .and_then(|s| s.active.as_ref().or(s.selected_next.as_ref()))
                    .map(|binding| super::super::model::safe(&binding.model))
                    .unwrap_or_else(|| "model unknown".into());
            }
        }
        // Location yields first; never reserve a project slot at the model's expense.
        let status = status.as_str();
        let model_budget = width.saturating_sub(UnicodeWidthStr::width(status) + 3);
        let model = truncate_middle(&model, model_budget);
        let available = width.saturating_sub(
            UnicodeWidthStr::width(model.as_str()) + UnicodeWidthStr::width(status) + 6,
        );
        let location = truncate_middle(&location, available);
        let mut spans = Vec::new();
        if !location.is_empty() {
            spans.push(Span::styled(location, Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" · "));
        }
        if !model.is_empty() {
            spans.push(Span::raw(model));
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::raw(truncate_middle(status, width)));
        let line = Line::from(spans);
        if width < 32 {
            line.style(Style::default().fg(status_color))
        } else {
            line
        }
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
