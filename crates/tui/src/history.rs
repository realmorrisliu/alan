use std::time::Instant;

use alan_agent_protocol::{
    ContentPart, DiffLine, PlanItemStatus, StructuredInputKind, StructuredInputQuestion,
    ToolResultPresentation, YieldKind,
};
use serde_json::{Map, Value};

/// Options controlling how transcript cells render.
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    pub width: usize,
    pub expand_thinking: bool,
}

impl RenderOpts {
    pub fn new(width: usize, expand_thinking: bool) -> Self {
        Self {
            width,
            expand_thinking,
        }
    }
}

/// Permanent transcript content. Ephemeral activity (running tools, streaming
/// thinking, transient notices, turn state) is projected by the file-backed transcript owner and is
/// rendered in the live region rather than committed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryCell {
    Rendered(Vec<String>),
    User(String),
    Assistant(String),
    /// Completed thinking, collapsed to a one-line summary by default.
    Thinking {
        text: String,
        duration_secs: u64,
    },
    /// A completed tool call.
    Tool {
        title: String,
        status: ToolStatus,
        preview: Option<String>,
        presentation: Option<ToolResultPresentation>,
    },
    Plan(Vec<PlanLine>),
    PendingYield(PendingYieldCell),
    /// A fatal (non-recoverable) error.
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanLine {
    pub status: PlanItemStatus,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Complete,
    Failed,
}

/// A tool call that is still running; shown in the live region only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningTool {
    pub id: String,
    pub title: String,
}

/// Thinking that is currently streaming; shown dimmed in the live region.
#[derive(Debug, Clone)]
pub struct ThinkingStream {
    pub text: String,
    pub started: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingYieldCell {
    pub request_id: String,
    pub kind: YieldKind,
    pub title: String,
    pub prompt: Option<String>,
    pub options: Vec<String>,
    /// The option submitted on a blank confirmation reply (the runtime's
    /// `default_option`), which may not be the first option.
    pub default_option: Option<String>,
    pub questions: Vec<StructuredInputQuestion>,
    /// Policy capability for an escalation (e.g. `network`, `write`).
    pub capability: Option<String>,
    /// Human-readable policy reason for the escalation.
    pub reason: Option<String>,
    /// Structured preview of the operation being approved (diff/command).
    pub presentation: Option<ToolResultPresentation>,
}

impl HistoryCell {
    pub fn render_lines(&self, opts: RenderOpts) -> Vec<String> {
        let width = opts.width.max(16);
        if let Self::Rendered(lines) = self {
            return lines
                .iter()
                .flat_map(|line| textwrap::wrap(line, width))
                .map(Into::into)
                .collect();
        }

        if let Self::Plan(items) = self {
            return render_plan(items, width);
        }

        if let Self::Thinking {
            text,
            duration_secs,
        } = self
        {
            if !opts.expand_thinking {
                let summary = format!("thinking · {duration_secs}s (ctrl+r to expand)");
                return textwrap::wrap(&summary, width)
                    .into_iter()
                    .map(Into::into)
                    .collect();
            }
            return wrap_with_prefix("thinking", text, width);
        }

        let (prefix, body) = match self {
            Self::Rendered(_) | Self::Plan(_) | Self::Thinking { .. } => {
                unreachable!("handled above")
            }
            Self::User(text) => ("you", text.clone()),
            Self::Assistant(text) => ("alan", text.clone()),
            Self::Tool {
                title,
                status,
                preview,
                presentation,
            } => {
                let glyph = match status {
                    ToolStatus::Complete => "✓",
                    ToolStatus::Failed => "✗",
                };
                let mut body = format!("{glyph} {title}");
                if let Some(presentation) = presentation {
                    for line in presentation_lines(presentation) {
                        body.push_str(&format!("\n  {line}"));
                    }
                } else if let Some(preview) =
                    preview.as_deref().filter(|preview| !preview.is_empty())
                {
                    body.push_str(&format!("\n  {preview}"));
                }
                ("tool", body)
            }
            Self::PendingYield(pending) => ("input", pending.render_body()),
            Self::Error(message) => ("error", message.clone()),
        };

        wrap_with_prefix(prefix, &body, width)
    }

    pub fn trim_rendered_prefix(&mut self, opts: RenderOpts, lines_to_trim: usize) -> bool {
        if lines_to_trim == 0 {
            return true;
        }

        if let Self::Assistant(text) = self {
            *text = trim_wrapped_body("alan", text, opts.width, lines_to_trim);
            return true;
        }

        let remaining = self
            .render_lines(opts)
            .into_iter()
            .skip(lines_to_trim)
            .collect::<Vec<_>>();
        *self = Self::Rendered(remaining);
        true
    }
}

impl PendingYieldCell {
    fn render_body(&self) -> String {
        let mut body = self.title.clone();
        if let Some(prompt) = &self.prompt {
            body.push_str(&format!(" - {prompt}"));
        }
        if !self.options.is_empty() {
            body.push_str(&format!(" - choices: {}", self.options.join(", ")));
        }
        if let Some(capability) = &self.capability {
            body.push_str(&format!("\n  capability: {capability}"));
        }
        if let Some(reason) = &self.reason {
            body.push_str(&format!("\n  why: {reason}"));
        }
        if let Some(presentation) = &self.presentation {
            for line in presentation_lines(presentation) {
                body.push_str(&format!("\n  {line}"));
            }
        }
        for question in &self.questions {
            body.push_str(&format!(
                "\n  {} [{}]: {}",
                question.id,
                structured_kind_label(question.kind),
                question.prompt
            ));
            if !question.options.is_empty() {
                let labels = question
                    .options
                    .iter()
                    .map(|option| format!("{}={}", option.value, option.label))
                    .collect::<Vec<_>>()
                    .join(", ");
                body.push_str(&format!(" ({labels})"));
            }
        }
        body
    }
}

/// Maximum lines shown for a tool presentation before collapsing.
const PRESENTATION_MAX_LINES: usize = 40;

/// Render a presentation primitive into transcript lines (one renderer per form).
fn presentation_lines(presentation: &ToolResultPresentation) -> Vec<String> {
    let mut lines = match presentation {
        ToolResultPresentation::Diff { path, hunks } => {
            let mut lines = vec![path.clone()];
            for hunk in hunks {
                if let Some(header) = &hunk.header {
                    lines.push(header.clone());
                }
                for line in &hunk.lines {
                    lines.push(match line {
                        DiffLine::Added { text } => format!("+{text}"),
                        DiffLine::Removed { text } => format!("-{text}"),
                        DiffLine::Context { text } => format!(" {text}"),
                    });
                }
            }
            lines
        }
        ToolResultPresentation::FileContent {
            path,
            lines,
            truncated,
        } => {
            let suffix = if *truncated { ", truncated" } else { "" };
            vec![format!("{path} ({lines} lines{suffix})")]
        }
        ToolResultPresentation::Command {
            cmdline,
            exit_code,
            stdout,
            stderr,
            truncated,
        } => {
            let mut lines = vec![format!("$ {cmdline}")];
            lines.extend(stdout.lines().map(str::to_string));
            lines.extend(stderr.lines().map(str::to_string));
            if let Some(code) = exit_code {
                lines.push(format!("exit {code}"));
            }
            if *truncated {
                lines.push("(output truncated)".to_string());
            }
            lines
        }
        ToolResultPresentation::Listing { rows } => rows.clone(),
        ToolResultPresentation::PlainText { body } => body.lines().map(str::to_string).collect(),
    };

    if lines.len() > PRESENTATION_MAX_LINES {
        let hidden = lines.len() - PRESENTATION_MAX_LINES;
        lines.truncate(PRESENTATION_MAX_LINES);
        lines.push(format!("… +{hidden} more lines"));
    }
    lines
}

fn render_plan(items: &[PlanLine], width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for item in items {
        let marker = match item.status {
            PlanItemStatus::Completed => "[x]",
            PlanItemStatus::InProgress => "[~]",
            PlanItemStatus::Pending => "[ ]",
        };
        let body = format!("{marker} {}", item.content);
        let wrapped = textwrap::wrap(&body, width.saturating_sub(6).max(8));
        for (idx, line) in wrapped.into_iter().enumerate() {
            if idx == 0 {
                lines.push(format!("plan> {line}"));
            } else {
                lines.push(format!("      {line}"));
            }
        }
    }
    lines
}

fn wrap_with_prefix(prefix: &str, body: &str, width: usize) -> Vec<String> {
    let body_width = width.saturating_sub(prefix.len() + 3).max(8);
    body.split('\n')
        .flat_map(|segment| {
            let wrapped = textwrap::wrap(segment, body_width);
            if wrapped.is_empty() {
                vec![String::new()]
            } else {
                wrapped.into_iter().map(|line| line.into_owned()).collect()
            }
        })
        .enumerate()
        .map(|(idx, line)| {
            if idx == 0 {
                format!("{prefix}> {line}")
            } else {
                format!("{:width$}  {line}", "", width = prefix.len())
            }
        })
        .collect()
}

fn trim_wrapped_body(prefix: &str, text: &str, width: usize, lines_to_trim: usize) -> String {
    let body_width = width.max(16).saturating_sub(prefix.len() + 3);
    textwrap::wrap(text, body_width)
        .into_iter()
        .skip(lines_to_trim)
        .map(|line| line.into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

impl PendingYieldCell {
    pub fn resume_content(&self, input: &str) -> Result<Vec<ContentPart>, String> {
        match self.kind {
            YieldKind::Confirmation => {
                let choice = normalize_confirmation_choice(
                    input,
                    &self.options,
                    self.default_option.as_deref(),
                )?;
                Ok(vec![ContentPart::structured(serde_json::json!({
                    "choice": choice
                }))])
            }
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
    options
        .iter()
        .find(|option| option.eq_ignore_ascii_case(trimmed))
        .cloned()
        .ok_or_else(|| format!("choose one of: {}", options.join(", ")))
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

fn structured_kind_label(kind: StructuredInputKind) -> &'static str {
    match kind {
        StructuredInputKind::Text => "text",
        StructuredInputKind::Boolean => "boolean",
        StructuredInputKind::Number => "number",
        StructuredInputKind::Integer => "integer",
        StructuredInputKind::SingleSelect => "single select",
        StructuredInputKind::MultiSelect => "multi select",
    }
}
