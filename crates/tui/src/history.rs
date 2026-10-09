mod action_summary;
mod literal;
mod markdown;
pub(crate) use action_summary::action_summary;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use std::time::Instant;

use alan_agent_protocol::{
    ContentPart, DiffLine, PlanItemStatus, StructuredInputKind, StructuredInputQuestion,
    ToolResultPresentation, UiPlanSnapshot, YieldKind,
};
use serde_json::{Map, Value};

use crate::transcript_ui::{
    INLINE_COMMAND_PROMPT_PREFIX, INLINE_PROMPT_CONTINUATION, INLINE_PROMPT_PREFIX,
};

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
    /// Already committed prefix removed; retain semantic spans, not prefix guesses.
    Styled(Vec<Line<'static>>),
    /// Streaming Markdown retains its source and parse context after a partial drain.
    AssistantTail {
        text: String,
        /// Stable source-byte end and expanded-character slot of committed content.
        committed: (usize, usize),
    },
    User(String),
    Command(String),
    /// Literal input retains its role and original source after physical drain.
    InputTail {
        text: String,
        command: bool,
        committed: (usize, usize),
    },
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
    Plan {
        snapshot: UiPlanSnapshot,
        owner: String,
        revision: usize,
    },
    PendingYield(PendingYieldCell),
    /// A fatal (non-recoverable) error.
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Complete,
    Failed,
    Rejected,
    Cancelled,
}

impl ToolStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Complete => "completed",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

    fn style(self) -> Style {
        Style::default().fg(match self {
            Self::Complete => Color::DarkGray,
            Self::Failed => Color::Red,
            Self::Rejected | Self::Cancelled => Color::Yellow,
        })
    }
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
    pub(crate) fn input_source(&self) -> Option<(&str, bool, (usize, usize))> {
        match self {
            Self::User(text) => Some((text, false, (0, 0))),
            Self::Command(text) => Some((text, true, (0, 0))),
            Self::InputTail {
                text,
                command,
                committed,
            } => Some((text, *command, *committed)),
            _ => None,
        }
    }

    pub(crate) fn with_input_cut(mut self, committed: (usize, usize)) -> Self {
        if committed != (0, 0)
            && let Some((text, command, _)) = self.input_source()
        {
            self = Self::InputTail {
                text: text.to_owned(),
                command,
                committed,
            };
        }
        self
    }

    pub(crate) fn assistant_source(&self) -> Option<&str> {
        match self {
            Self::Assistant(text) | Self::AssistantTail { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Replace reconciled source without replaying already committed content.
    pub(crate) fn replace_assistant_source(&mut self, source: String) {
        match self {
            Self::Assistant(text) | Self::AssistantTail { text, .. } => *text = source,
            _ => {}
        }
    }

    pub fn render_lines(&self, opts: RenderOpts) -> Vec<String> {
        if matches!(
            self,
            Self::Assistant(_) | Self::Styled(_) | Self::AssistantTail { .. }
        ) {
            return self
                .render_styled_lines(opts)
                .iter()
                .map(ToString::to_string)
                .collect();
        }
        let width = opts.width.max(16);
        if let Some((text, command, committed)) = self.input_source() {
            return literal::project(
                text,
                width,
                if command {
                    INLINE_COMMAND_PROMPT_PREFIX
                } else {
                    INLINE_PROMPT_PREFIX
                },
                committed,
            )
            .into_iter()
            .map(|(line, _)| line.to_string())
            .collect();
        }
        if let Self::Rendered(lines) = self {
            return lines
                .iter()
                .flat_map(|line| textwrap::wrap(line, width))
                .map(Into::into)
                .collect();
        }

        if let Self::Plan {
            snapshot, revision, ..
        } = self
        {
            let completed = snapshot
                .items
                .iter()
                .filter(|item| item.status == PlanItemStatus::Completed)
                .count();
            let current = snapshot
                .items
                .iter()
                .find(|item| item.status == PlanItemStatus::InProgress);
            let step = current.map_or(String::new(), |item| {
                format!(" · {}", action_summary::bounded(&item.content, 256))
            });
            let summary = format!(
                "Plan {revision} · {completed}/{} completed{step}",
                snapshot.items.len()
            );
            return vec![
                action_summary::summary_row(&summary, width, metadata_style()).to_string(),
            ];
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
            return wrap_plain_text(text, width);
        }

        let body = match self {
            Self::Rendered(_)
            | Self::Styled(_)
            | Self::AssistantTail { .. }
            | Self::InputTail { .. }
            | Self::Plan { .. }
            | Self::Thinking { .. } => {
                unreachable!("handled above")
            }
            Self::User(_) | Self::Command(_) => unreachable!("literal inputs handled above"),
            Self::Assistant(text) => return wrap_plain_text(text, width),
            Self::Tool {
                title,
                status,
                preview,
                presentation,
            } => {
                let mut body = format!("{title} · {}", status.label());
                if let Some(presentation) = presentation {
                    for line in presentation_lines(presentation) {
                        body.push_str(&format!("\n  {line}"));
                    }
                } else if let Some(preview) =
                    preview.as_deref().filter(|preview| !preview.is_empty())
                {
                    body.push_str(&format!("\n  {preview}"));
                }
                body
            }
            Self::PendingYield(pending) => format!("Waiting for input\n{}", pending.render_body()),
            Self::Error(message) => format!("Error: {message}"),
        };

        wrap_plain_text(&body, width)
    }

    /// Project typed transcript roles and Markdown directly into Ratatui spans.
    pub fn render_styled_lines(&self, opts: RenderOpts) -> Vec<Line<'static>> {
        let width = opts.width.max(16);
        match self {
            Self::Assistant(text) => markdown::project(text, width, (0, 0))
                .into_iter()
                .map(|(line, _)| line)
                .collect(),
            Self::AssistantTail { text, committed } => markdown::project(text, width, *committed)
                .into_iter()
                .map(|(line, _)| line)
                .collect(),
            Self::Styled(lines) => wrap_styled_lines(lines.clone(), width),
            Self::Tool {
                title,
                status,
                presentation: Some(presentation @ ToolResultPresentation::Diff { .. }),
                ..
            } => {
                let mut lines = vec![Line::styled(
                    format!("{} · {}", clean_text(title), status.label()),
                    status.style().add_modifier(Modifier::BOLD),
                )];
                lines.extend(
                    presentation_rows(presentation)
                        .into_iter()
                        .map(|(text, style)| {
                            Line::styled(format!("  {}", clean_text(&text)), style)
                        }),
                );
                wrap_styled_lines(lines, width)
            }
            _ => {
                let style = match self {
                    Self::User(_)
                    | Self::Command(_)
                    | Self::InputTail { .. }
                    | Self::PendingYield(_) => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                    Self::Tool { status, .. } => status.style(),
                    Self::Plan { .. } => metadata_style(),
                    Self::Thinking { .. } => metadata_style().add_modifier(Modifier::ITALIC),
                    Self::Error(_) => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    _ => Style::default(),
                };
                self.render_lines(opts)
                    .into_iter()
                    .map(|text| Line::styled(clean_text(&text), style))
                    .collect()
            }
        }
    }

    pub fn trim_rendered_prefix(&mut self, opts: RenderOpts, lines_to_trim: usize) -> bool {
        if lines_to_trim == 0 {
            return true;
        }

        if let Some((text, command, cut)) = self.input_source() {
            let prefix = if command {
                INLINE_COMMAND_PROMPT_PREFIX
            } else {
                INLINE_PROMPT_PREFIX
            };
            let rows = literal::project(text, opts.width.max(16), prefix, cut);
            let committed = rows
                .iter()
                .take(lines_to_trim)
                .next_back()
                .map_or(cut, |(_, key)| *key);
            *self = Self::InputTail {
                text: text.to_owned(),
                command,
                committed,
            };
            return true;
        }
        if let Some(text) = self.assistant_source() {
            let cut = match &*self {
                Self::AssistantTail { committed, .. } => *committed,
                _ => (0, 0),
            };
            let rows = markdown::project(text, opts.width.max(16), cut);
            let committed = rows
                .iter()
                .take(lines_to_trim)
                .next_back()
                .map_or(cut, |(_, key)| *key);
            *self = Self::AssistantTail {
                text: text.to_string(),
                committed,
            };
            return true;
        }
        let remaining = self
            .render_styled_lines(opts)
            .into_iter()
            .skip(lines_to_trim)
            .collect();
        *self = Self::Styled(remaining);
        true
    }
}

fn metadata_style() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

// Strip terminal controls and expand tabs before Ratatui filters control graphemes.
// No tab-stop convention exists in this renderer; use four copyable spaces per tab.
pub(crate) fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .flat_map(|c| {
            if c == '\t' {
                "    ".chars().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub(crate) fn wrap_styled_lines(
    lines: impl IntoIterator<Item = Line<'static>>,
    width: usize,
) -> Vec<Line<'static>> {
    let mut output = Vec::new();
    for line in lines {
        let mut row = Vec::new();
        let mut cells = 0;
        for span in &line.spans {
            for grapheme in span.styled_graphemes(line.style) {
                let size = unicode_width::UnicodeWidthStr::width(grapheme.symbol);
                if cells + size > width.max(1) && cells > 0 {
                    output.push(Line::from(std::mem::take(&mut row)));
                    cells = 0;
                }
                row.push(Span::styled(grapheme.symbol.to_string(), grapheme.style));
                cells += size;
            }
        }
        output.push(Line::from(row));
    }
    output
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
    presentation_rows(presentation)
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

fn presentation_rows(presentation: &ToolResultPresentation) -> Vec<(String, Style)> {
    presentation_rows_mode(presentation, false)
}

fn presentation_rows_mode(
    presentation: &ToolResultPresentation,
    detail: bool,
) -> Vec<(String, Style)> {
    if let ToolResultPresentation::Diff { path, hunks } = presentation {
        let mut rows = vec![(path.clone(), metadata_style())];
        for hunk in hunks {
            if let Some(header) = &hunk.header {
                rows.push((header.clone(), metadata_style()));
            }
            for line in &hunk.lines {
                let (marker, text, color) = match line {
                    DiffLine::Added { text } => ("+", text, Color::Green),
                    DiffLine::Removed { text } => ("-", text, Color::Red),
                    DiffLine::Context { text } => (" ", text, Color::Reset),
                };
                rows.push((format!("{marker}{text}"), Style::default().fg(color)));
            }
        }
        if !detail && rows.len() > PRESENTATION_MAX_LINES {
            let hidden = rows.len() - PRESENTATION_MAX_LINES;
            rows.truncate(PRESENTATION_MAX_LINES);
            rows.push((format!("… +{hidden} more lines"), metadata_style()));
        }
        return rows;
    }
    let mut lines = match presentation {
        ToolResultPresentation::Diff { .. } => unreachable!("handled above"),
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
            if detail {
                lines.push("stdout".into());
            }
            lines.extend(stdout.lines().map(str::to_string));
            if detail {
                lines.push("stderr".into());
            }
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

    if !detail && lines.len() > PRESENTATION_MAX_LINES {
        let hidden = lines.len() - PRESENTATION_MAX_LINES;
        lines.truncate(PRESENTATION_MAX_LINES);
        lines.push(format!("… +{hidden} more lines"));
    }
    lines
        .into_iter()
        .map(|text| (text, Style::default()))
        .collect()
}

pub(crate) fn action_detail(cell: &HistoryCell) -> Vec<Line<'static>> {
    if let HistoryCell::Tool {
        title,
        presentation: Some(presentation),
        ..
    } = cell
    {
        let mut rows = vec![Line::styled(
            clean_text(title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )];
        rows.extend(
            presentation_rows_mode(presentation, true)
                .into_iter()
                .map(|(text, style)| Line::styled(clean_text(&text), style)),
        );
        rows
    } else {
        cell.render_styled_lines(RenderOpts::new(16384, false))
    }
}

fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    text.split('\n')
        .flat_map(|segment| {
            let wrapped = textwrap::wrap(segment, width);
            if wrapped.is_empty() {
                vec![String::new()]
            } else {
                wrapped.into_iter().map(Into::into).collect()
            }
        })
        .collect()
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
