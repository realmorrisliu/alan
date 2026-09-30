use super::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl FileBackedApp {
    pub(in crate::file_backed) fn activity_label(&self) -> Option<&str> {
        match self.activity.state {
            UiActivityState::Idle => None,
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
        if width < 32 {
            return Line::styled(
                status.to_string(),
                Style::default().fg(match status {
                    "ready" => Color::Green,
                    "working" | "selecting project" => Color::Cyan,
                    "waiting for approval" | "paused" => Color::Yellow,
                    _ => Color::Red,
                }),
            );
        }
        let location = self.project.as_ref().map_or_else(
            || "no project".to_string(),
            |project| {
                if self.pending_project_cwd.is_some() {
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
        let status = if width < 48 {
            match status {
                "selecting project" => "selecting",
                "waiting for approval" => "approval",
                other => other,
            }
        } else {
            status
        };
        let model = self
            .effective_model
            .as_deref()
            .unwrap_or("unknown")
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>();
        let model = model.trim();
        let model_width = width.saturating_sub(
            UnicodeWidthStr::width("alan  · model  · ") + UnicodeWidthStr::width(status) + 1,
        );
        let model = truncate_middle(model, model_width);
        let suffix = format!(" · model {model} · {status}");
        let available = width.saturating_sub(
            UnicodeWidthStr::width("alan ") + UnicodeWidthStr::width(suffix.as_str()),
        );
        let location = truncate_middle(&location, available);
        Line::from(vec![
            Span::styled("alan ", Style::default().fg(Color::DarkGray)),
            Span::styled(location, Style::default().fg(Color::Cyan)),
            Span::styled(" · model ", Style::default().fg(Color::DarkGray)),
            Span::styled(model, Style::default().fg(Color::DarkGray)),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                status.to_string(),
                Style::default().fg(match status {
                    "ready" => Color::Green,
                    "working" | "selecting" | "selecting project" => Color::Cyan,
                    "approval" | "waiting for approval" | "paused" => Color::Yellow,
                    _ => Color::Red,
                }),
            ),
        ])
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
