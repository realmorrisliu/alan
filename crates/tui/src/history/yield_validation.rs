use alan_agent_protocol::{ContentPart, StructuredInputKind, StructuredInputQuestion, YieldKind};
use ratatui::text::{Line, Span};
use serde_json::{Map, Value};
use unicode_width::UnicodeWidthStr;

use super::{PendingYieldCell, action_summary, clean_text, metadata_style};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationOutcome {
    Approved,
    AllowedForSession,
    Rejected,
    Other,
    Closed,
}

impl ConfirmationOutcome {
    pub fn from_choice(choice: &str) -> Self {
        match choice {
            "approve" => Self::Approved,
            "allow_session" => Self::AllowedForSession,
            "reject" => Self::Rejected,
            _ => Self::Other,
        }
    }

    pub fn label(self, choice: &str) -> String {
        match self {
            Self::Approved => "Approved".to_string(),
            Self::AllowedForSession => "Allowed for session".to_string(),
            Self::Rejected => "Rejected".to_string(),
            Self::Other => clean_text(choice),
            Self::Closed => "Approval closed".to_string(),
        }
    }
}

pub(super) fn render_confirmation_receipt_line(
    outcome: ConfirmationOutcome,
    choice: &str,
    summary: &str,
    note: Option<&str>,
    width: usize,
    theme: Theme,
) -> Line<'static> {
    let (icon, label_style) = match outcome {
        ConfirmationOutcome::Approved | ConfirmationOutcome::AllowedForSession => {
            (if theme.is_dumb { "+ " } else { "✓ " }, theme.run_style())
        }
        ConfirmationOutcome::Rejected => {
            (if theme.is_dumb { "x " } else { "✕ " }, theme.block_style())
        }
        ConfirmationOutcome::Other => (
            if theme.is_dumb { "+ " } else { "✓ " },
            theme.accent_style(),
        ),
        ConfirmationOutcome::Closed => ("· ", theme.muted_style()),
    };
    let label = outcome.label(choice);
    let prefix_width = UnicodeWidthStr::width(icon) + UnicodeWidthStr::width(label.as_str()) + 3;
    let clean_summary = clean_text(summary)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let clean_note = note
        .map(|raw| {
            clean_text(raw)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|text| !text.is_empty());

    let detail_budget = width.saturating_sub(prefix_width);
    let (shown_summary, shown_note) = if let Some(note_text) = clean_note {
        let note_width = UnicodeWidthStr::width(note_text.as_str());
        let note_budget = (detail_budget / 2).max(16).min(note_width);
        let clamped_note = action_summary::clamp_text_width(&note_text, note_budget);
        let note_suffix_w = UnicodeWidthStr::width(clamped_note.as_str()) + 3;
        let summary_budget = detail_budget.saturating_sub(note_suffix_w).max(8);
        (
            action_summary::clamp_text_width(&clean_summary, summary_budget),
            Some(clamped_note),
        )
    } else {
        (
            action_summary::clamp_text_width(&clean_summary, detail_budget.max(8)),
            None,
        )
    };

    let mut spans = vec![
        Span::styled(icon, label_style),
        Span::styled(label, label_style),
        Span::styled(format!(" · {shown_summary}"), metadata_style(theme)),
    ];
    if let Some(note_text) = shown_note {
        spans.push(Span::styled(format!(" ({note_text})"), theme.muted_style()));
    }
    action_summary::fit_line_to_width(Line::from(spans), width)
}

impl PendingYieldCell {
    pub fn resume_content(&self, input: &str) -> Result<Vec<ContentPart>, String> {
        match self.kind {
            YieldKind::Confirmation => self.resume_confirmation_with_note(input, None),
            YieldKind::StructuredInput => {
                if self.questions.is_empty() {
                    return Ok(vec![ContentPart::text(input.to_string())]);
                }
                let answers = self.validate_structured_answers(input)?;
                Ok(vec![ContentPart::structured(Value::Object(answers))])
            }
            YieldKind::Custom(_) => Ok(vec![ContentPart::text(input.to_string())]),
        }
    }

    pub fn resume_confirmation_with_note(
        &self,
        choice_input: &str,
        note: Option<&str>,
    ) -> Result<Vec<ContentPart>, String> {
        if !matches!(self.kind, YieldKind::Confirmation) {
            return self.resume_content(choice_input);
        }
        let choice = normalize_confirmation_choice(
            choice_input,
            &self.options,
            self.default_option.as_deref(),
        )?;
        let mut payload = serde_json::json!({ "choice": choice });
        if let Some(trimmed) = note.map(str::trim).filter(|text| !text.is_empty()) {
            payload["modifications"] = Value::String(trimmed.to_string());
        }
        Ok(vec![ContentPart::structured(payload)])
    }

    fn validate_structured_answers(&self, input: &str) -> Result<Map<String, Value>, String> {
        if self.questions.len() == 1 {
            let question = &self.questions[0];
            let trimmed = input.trim();
            // A blank optional single question with no default submits an empty
            // answer map (mirrors the multi-field form omitting blank optional
            // fields), so the user isn't forced to invent a typed value.
            if trimmed.is_empty()
                && !question.required
                && question.default_value.is_none()
                && question.default_values.is_empty()
            {
                return Ok(Map::new());
            }
            let value = validate_question_answer(question, trimmed)?;
            return Ok(Map::from_iter([(question.id.clone(), value)]));
        }

        let raw: Map<String, Value> = serde_json::from_str(input)
            .map_err(|_| "reply with a JSON object keyed by field id")?;
        let mut answers = Map::new();
        for question in &self.questions {
            let Some(value) = raw.get(&question.id) else {
                if question.required {
                    return Err(format!("{} is required", question.id));
                }
                continue;
            };
            answers.insert(
                question.id.clone(),
                validate_question_value(question, value.clone())?,
            );
        }
        Ok(answers)
    }
}

fn normalize_confirmation_choice(
    input: &str,
    options: &[String],
    default_option: Option<&str>,
) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        // Blank Enter uses the runtime's default_option (which may not be the
        // first option), falling back to the first option, then "approve".
        return Ok(default_option
            .map(str::to_string)
            .or_else(|| options.first().cloned())
            .unwrap_or_else(|| "approve".to_string()));
    }
    if options.is_empty() {
        return Ok(trimmed.to_string());
    }
    if let Some(exact) = options
        .iter()
        .find(|option| option.eq_ignore_ascii_case(trimmed))
    {
        return Ok(exact.clone());
    }
    if let Ok(index) = trimmed.parse::<usize>()
        && index >= 1
        && let Some(choice) = options.get(index - 1)
    {
        return Ok(choice.clone());
    }
    Err(format!("choose one of: {}", options.join(", ")))
}

fn validate_question_answer(
    question: &StructuredInputQuestion,
    input: &str,
) -> Result<Value, String> {
    if input.is_empty() {
        if let Some(default) = &question.default_value {
            return validate_question_answer(question, default);
        }
        // Multi-select defaults are normalized into `default_values`; honor them
        // on blank input so a defaulted single multi-select submits its default.
        if !question.default_values.is_empty() {
            return validate_question_answer(question, &question.default_values.join(","));
        }
        if question.required {
            return Err(format!("{} is required", question.id));
        }
    }

    match question.kind {
        StructuredInputKind::Text => Ok(Value::String(input.to_string())),
        StructuredInputKind::Boolean => parse_bool_answer(input)
            .map(Value::Bool)
            .ok_or_else(|| format!("{} must be true/false", question.id)),
        StructuredInputKind::Number => {
            let value = input
                .parse::<f64>()
                .map_err(|_| format!("{} must be a number", question.id))?;
            serde_json::Number::from_f64(value)
                .map(Value::Number)
                .ok_or_else(|| format!("{} must be finite", question.id))
        }
        StructuredInputKind::Integer => input
            .parse::<i64>()
            .map(|value| Value::Number(value.into()))
            .map_err(|_| format!("{} must be an integer", question.id)),
        StructuredInputKind::SingleSelect => match_option(question, input)
            .map(Value::String)
            .ok_or_else(|| option_error(question)),
        StructuredInputKind::MultiSelect => {
            let values = input
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(|part| match_option(question, part).ok_or_else(|| option_error(question)))
                .collect::<Result<Vec<_>, _>>()?;
            validate_selection_count(question, values.len())?;
            Ok(Value::Array(
                values.into_iter().map(Value::String).collect(),
            ))
        }
    }
}

fn validate_question_value(
    question: &StructuredInputQuestion,
    value: Value,
) -> Result<Value, String> {
    match (question.kind, value) {
        (StructuredInputKind::Text, Value::String(text)) => {
            validate_question_answer(question, &text)
        }
        (StructuredInputKind::Boolean, Value::Bool(value)) => Ok(Value::Bool(value)),
        (StructuredInputKind::Number, Value::Number(value)) => Ok(Value::Number(value)),
        (StructuredInputKind::Integer, Value::Number(value)) if value.as_i64().is_some() => {
            Ok(Value::Number(value))
        }
        (StructuredInputKind::SingleSelect, Value::String(text)) => {
            validate_question_answer(question, &text)
        }
        (StructuredInputKind::MultiSelect, Value::Array(values)) => {
            let selected = values
                .into_iter()
                .map(|value| {
                    value
                        .as_str()
                        .and_then(|text| match_option(question, text))
                        .ok_or_else(|| option_error(question))
                })
                .collect::<Result<Vec<_>, _>>()?;
            validate_selection_count(question, selected.len())?;
            Ok(Value::Array(
                selected.into_iter().map(Value::String).collect(),
            ))
        }
        (_, value) => validate_question_answer(question, value.as_str().unwrap_or_default()),
    }
}

fn parse_bool_answer(input: &str) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" | "1" => Some(true),
        "false" | "no" | "n" | "0" => Some(false),
        _ => None,
    }
}

fn match_option(question: &StructuredInputQuestion, input: &str) -> Option<String> {
    question
        .options
        .iter()
        .find(|option| {
            option.value.eq_ignore_ascii_case(input) || option.label.eq_ignore_ascii_case(input)
        })
        .map(|option| option.value.clone())
}

fn option_error(question: &StructuredInputQuestion) -> String {
    let values = question
        .options
        .iter()
        .map(|option| option.value.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("{} must be one of: {values}", question.id)
}

fn validate_selection_count(
    question: &StructuredInputQuestion,
    count: usize,
) -> Result<(), String> {
    if let Some(min) = question.min_selected
        && count < min as usize
    {
        return Err(format!("{} needs at least {min} selection(s)", question.id));
    }
    if let Some(max) = question.max_selected
        && count > max as usize
    {
        return Err(format!("{} allows at most {max} selection(s)", question.id));
    }
    if question.required && count == 0 {
        return Err(format!("{} is required", question.id));
    }
    Ok(())
}

pub(super) fn structured_kind_label(kind: StructuredInputKind) -> &'static str {
    match kind {
        StructuredInputKind::Text => "text",
        StructuredInputKind::Boolean => "boolean",
        StructuredInputKind::Number => "number",
        StructuredInputKind::Integer => "integer",
        StructuredInputKind::SingleSelect => "single select",
        StructuredInputKind::MultiSelect => "multi select",
    }
}
