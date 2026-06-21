use std::time::Instant;

use alan_protocol::{
    ContentPart, DiffLine, Event, EventEnvelope, PlanItemStatus, StructuredInputKind,
    StructuredInputQuestion, ToolResultPresentation, YieldKind,
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
/// thinking, transient notices, turn state) lives on [`SessionReducer`] and is
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

fn yield_capability(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("details")
        .and_then(|details| details.get("capability"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn yield_reason(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("details")
        .and_then(|details| details.get("policy"))
        .and_then(|policy| policy.get("reason"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn yield_presentation(payload: &serde_json::Value) -> Option<ToolResultPresentation> {
    payload
        .get("details")
        .and_then(|details| details.get("presentation"))
        .filter(|value| !value.is_null())
        .and_then(|value| serde_json::from_value(value.clone()).ok())
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

#[derive(Debug, Default, Clone)]
pub struct SessionReducer {
    pub cells: Vec<HistoryCell>,
    pub pending_yield: Option<PendingYieldCell>,
    pub turn_active: bool,
    pub turn_started: Option<Instant>,
    pub running_tools: Vec<RunningTool>,
    pub thinking_stream: Option<ThinkingStream>,
    pub transient_notice: Option<String>,
    pub expand_thinking: bool,
}

impl SessionReducer {
    pub fn apply_envelope(&mut self, envelope: EventEnvelope) {
        let event = envelope.event;
        if event_advances_past_pending_yield(&event) {
            self.pending_yield = None;
        }

        match event {
            Event::TurnStarted {} => {
                self.turn_active = true;
                self.turn_started = Some(Instant::now());
                self.transient_notice = None;
            }
            Event::TurnCompleted { summary } => {
                self.turn_active = false;
                self.turn_started = None;
                self.running_tools.clear();
                self.thinking_stream = None;
                if let Some(summary) = summary {
                    tracing::debug!(summary, "turn completed");
                }
            }
            Event::TextDelta { chunk, is_final: _ } => self.append_text(chunk),
            Event::ThinkingDelta { chunk, is_final } => self.append_thinking(chunk, is_final),
            Event::ToolCallStarted {
                id, name, title, ..
            } => {
                self.running_tools.push(RunningTool {
                    id,
                    title: title.unwrap_or(name),
                });
            }
            Event::ToolCallCompleted {
                id,
                name,
                success,
                result_preview,
                presentation,
                ..
            } => self.complete_tool(id, name, success, result_preview, presentation),
            Event::PlanUpdated { items, .. } => self.cells.push(HistoryCell::Plan(
                items
                    .into_iter()
                    .map(|item| PlanLine {
                        status: item.status,
                        content: item.content,
                    })
                    .collect(),
            )),
            Event::SessionRolledBack { turns, .. } => {
                self.transient_notice = Some(format!("rolled back {turns} turns"));
            }
            Event::Yield {
                request_id,
                kind,
                payload,
            } => {
                let title = yield_title(&kind, &payload);
                let prompt = yield_prompt(&kind, &payload);
                let options = yield_options(&kind, &payload);
                let questions = yield_questions(&kind, &payload);
                let cell = PendingYieldCell {
                    request_id,
                    kind,
                    title,
                    prompt,
                    options,
                    questions,
                    capability: yield_capability(&payload),
                    reason: yield_reason(&payload),
                    presentation: yield_presentation(&payload),
                };
                self.pending_yield = Some(cell.clone());
                self.cells.push(HistoryCell::PendingYield(cell));
            }
            Event::CompactionObserved { .. } => {
                self.transient_notice = Some("context compacted".into());
            }
            Event::MemoryFlushObserved { .. } => {
                self.transient_notice = Some("memory flushed".into());
            }
            Event::Warning { message } => {
                self.transient_notice = Some(message);
            }
            Event::Error {
                message,
                recoverable,
            } => {
                if recoverable {
                    self.transient_notice = Some(message);
                } else {
                    self.cells.push(HistoryCell::Error(message));
                }
            }
        }
    }

    /// Label for the live-region activity line, present only while a turn runs.
    pub fn activity_label(&self) -> Option<&str> {
        if !self.turn_active {
            return None;
        }
        if let Some(tool) = self.running_tools.last() {
            return Some(tool.title.as_str());
        }
        if self.thinking_stream.is_some() {
            return Some("thinking");
        }
        Some("working")
    }

    fn append_text(&mut self, chunk: String) {
        match self.cells.last_mut() {
            Some(HistoryCell::Assistant(text)) => text.push_str(&chunk),
            _ => self.cells.push(HistoryCell::Assistant(chunk)),
        }
    }

    fn append_thinking(&mut self, chunk: String, is_final: bool) {
        let stream = self.thinking_stream.get_or_insert_with(|| ThinkingStream {
            text: String::new(),
            started: Instant::now(),
        });
        stream.text.push_str(&chunk);
        if is_final {
            let stream = self.thinking_stream.take().expect("just inserted");
            let duration_secs = stream.started.elapsed().as_secs();
            if !stream.text.trim().is_empty() {
                self.cells.push(HistoryCell::Thinking {
                    text: stream.text,
                    duration_secs,
                });
            }
        }
    }

    fn complete_tool(
        &mut self,
        id: String,
        name: Option<String>,
        success: Option<bool>,
        result_preview: Option<String>,
        presentation: Option<ToolResultPresentation>,
    ) {
        let title = self
            .running_tools
            .iter()
            .position(|tool| tool.id == id)
            .map(|index| self.running_tools.remove(index).title)
            .or(name.clone())
            .unwrap_or_else(|| "tool".to_string());
        let status = if success == Some(false) {
            ToolStatus::Failed
        } else {
            ToolStatus::Complete
        };
        self.cells.push(HistoryCell::Tool {
            title,
            status,
            preview: result_preview,
            presentation,
        });
    }
}

fn event_advances_past_pending_yield(event: &Event) -> bool {
    matches!(
        event,
        Event::TurnStarted { .. }
            | Event::TurnCompleted { .. }
            | Event::TextDelta { .. }
            | Event::ThinkingDelta { .. }
            | Event::ToolCallStarted { .. }
            | Event::ToolCallCompleted { .. }
            | Event::PlanUpdated { .. }
            | Event::SessionRolledBack { .. }
    )
}

impl PendingYieldCell {
    pub fn resume_content(&self, input: &str) -> Result<Vec<ContentPart>, String> {
        match self.kind {
            YieldKind::Confirmation => {
                let choice = normalize_confirmation_choice(input, &self.options)?;
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
            YieldKind::DynamicTool | YieldKind::Custom(_) => {
                Ok(vec![ContentPart::text(input.to_string())])
            }
        }
    }

    fn validate_structured_answers(&self, input: &str) -> Result<Map<String, Value>, String> {
        if self.questions.len() == 1 {
            let question = &self.questions[0];
            let value = validate_question_answer(question, input.trim())?;
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

fn yield_title(kind: &YieldKind, payload: &serde_json::Value) -> String {
    payload
        .get("message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.get("title").and_then(serde_json::Value::as_str))
        .or_else(|| payload.get("summary").and_then(serde_json::Value::as_str))
        .or_else(|| payload.get("prompt").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| match kind {
            YieldKind::Confirmation => "confirmation requested".to_string(),
            YieldKind::StructuredInput => "structured input requested".to_string(),
            YieldKind::DynamicTool => "client tool requested".to_string(),
            YieldKind::Custom(kind) => format!("{kind} requested"),
        })
}

fn yield_prompt(kind: &YieldKind, payload: &serde_json::Value) -> Option<String> {
    match kind {
        YieldKind::StructuredInput | YieldKind::DynamicTool | YieldKind::Custom(_) => payload
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        YieldKind::Confirmation => payload
            .get("details")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    }
}

fn yield_options(kind: &YieldKind, payload: &serde_json::Value) -> Vec<String> {
    if !matches!(kind, YieldKind::Confirmation) {
        return Vec::new();
    }
    payload
        .get("options")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect()
}

fn yield_questions(kind: &YieldKind, payload: &serde_json::Value) -> Vec<StructuredInputQuestion> {
    if !matches!(kind, YieldKind::StructuredInput) {
        return Vec::new();
    }
    payload
        .get("questions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

fn normalize_confirmation_choice(input: &str, options: &[String]) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(options
            .first()
            .cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use alan_protocol::EventEnvelope;

    fn envelope(event: Event) -> EventEnvelope {
        EventEnvelope {
            event_id: "e-1".into(),
            sequence: 1,
            session_id: "s-1".into(),
            submission_id: None,
            turn_id: "t-1".into(),
            item_id: "i-1".into(),
            timestamp_ms: 1,
            event,
        }
    }

    fn opts(width: usize) -> RenderOpts {
        RenderOpts::new(width, false)
    }

    #[test]
    fn text_deltas_accumulate_into_typed_assistant_cell() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::TextDelta {
            chunk: "hel".into(),
            is_final: false,
        }));
        reducer.apply_envelope(envelope(Event::TextDelta {
            chunk: "lo".into(),
            is_final: true,
        }));
        assert_eq!(reducer.cells, vec![HistoryCell::Assistant("hello".into())]);
    }

    #[test]
    fn turn_lifecycle_is_suppressed_from_transcript() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::TurnStarted {}));
        assert!(reducer.turn_active);
        assert_eq!(reducer.activity_label(), Some("working"));
        reducer.apply_envelope(envelope(Event::TurnCompleted {
            summary: Some("done".into()),
        }));
        assert!(!reducer.turn_active);
        assert!(reducer.activity_label().is_none());
        assert!(reducer.cells.is_empty());
    }

    #[test]
    fn thinking_collapses_to_summary_with_full_text_on_expand() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::ThinkingDelta {
            chunk: "deep reasoning here".into(),
            is_final: false,
        }));
        assert!(reducer.thinking_stream.is_some());
        reducer.apply_envelope(envelope(Event::ThinkingDelta {
            chunk: " continues".into(),
            is_final: true,
        }));
        assert!(reducer.thinking_stream.is_none());
        let cell = reducer.cells.last().expect("thinking cell");
        let collapsed = cell.render_lines(RenderOpts::new(80, false)).join("\n");
        assert!(collapsed.contains("thinking ·"));
        assert!(!collapsed.contains("deep reasoning"));
        let expanded = cell.render_lines(RenderOpts::new(80, true)).join("\n");
        assert!(expanded.contains("deep reasoning here continues"));
    }

    #[test]
    fn tool_started_prefers_runtime_title() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::ToolCallStarted {
            id: "t1".into(),
            name: "edit".into(),
            title: Some("Edit src/foo.rs".into()),
            audit: None,
        }));
        assert_eq!(reducer.running_tools[0].title, "Edit src/foo.rs");
    }

    #[test]
    fn completed_tool_renders_diff_presentation() {
        use alan_protocol::{DiffHunk, DiffLine, ToolResultPresentation};
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::ToolCallCompleted {
            id: "t1".into(),
            name: Some("edit".into()),
            success: Some(true),
            result_preview: Some("ignored when presentation present".into()),
            presentation: Some(ToolResultPresentation::Diff {
                path: "src/foo.rs".into(),
                hunks: vec![DiffHunk {
                    header: None,
                    lines: vec![
                        DiffLine::Removed { text: "old".into() },
                        DiffLine::Added { text: "new".into() },
                    ],
                }],
            }),
            audit: None,
        }));
        let rendered = reducer
            .cells
            .last()
            .unwrap()
            .render_lines(opts(80))
            .join("\n");
        assert!(rendered.contains("src/foo.rs"));
        assert!(rendered.contains("-old"));
        assert!(rendered.contains("+new"));
        assert!(!rendered.contains("ignored when presentation"));
    }

    #[test]
    fn completed_tool_becomes_permanent_cell() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::ToolCallStarted {
            id: "t1".into(),
            name: "read_file".into(),
            title: None,
            audit: None,
        }));
        assert_eq!(reducer.running_tools.len(), 1);
        reducer.apply_envelope(envelope(Event::ToolCallCompleted {
            id: "t1".into(),
            name: Some("read_file".into()),
            success: Some(true),
            result_preview: Some("ok".into()),
            presentation: None,
            audit: None,
        }));
        assert!(reducer.running_tools.is_empty());
        assert!(matches!(
            reducer.cells.last(),
            Some(HistoryCell::Tool {
                status: ToolStatus::Complete,
                ..
            })
        ));
    }

    #[test]
    fn plan_renders_checklist_without_debug_format() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::PlanUpdated {
            explanation: None,
            items: vec![
                alan_protocol::PlanItem {
                    id: "1".into(),
                    content: "first".into(),
                    status: PlanItemStatus::Completed,
                },
                alan_protocol::PlanItem {
                    id: "2".into(),
                    content: "second".into(),
                    status: PlanItemStatus::InProgress,
                },
            ],
        }));
        let rendered = reducer.cells[0].render_lines(opts(80)).join("\n");
        assert!(rendered.contains("[x] first"));
        assert!(rendered.contains("[~] second"));
        assert!(!rendered.contains("Completed"));
        assert!(!rendered.contains("InProgress"));
    }

    #[test]
    fn recoverable_error_becomes_transient_not_transcript() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::Error {
            message: "retrying".into(),
            recoverable: true,
        }));
        assert_eq!(reducer.transient_notice.as_deref(), Some("retrying"));
        assert!(reducer.cells.is_empty());
    }

    #[test]
    fn fatal_error_becomes_transcript_cell() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::Error {
            message: "boom".into(),
            recoverable: false,
        }));
        assert!(matches!(reducer.cells.last(), Some(HistoryCell::Error(_))));
    }

    #[test]
    fn pending_yield_render_omits_request_id() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::Yield {
            request_id: "req-secret-123".into(),
            kind: YieldKind::Confirmation,
            payload: serde_json::json!({"message": "Approve write?", "options": ["approve", "reject"]}),
        }));
        let cell = reducer.cells.last().expect("pending yield cell");
        let rendered = cell.render_lines(opts(80)).join("\n");
        assert!(!rendered.contains("req-secret-123"));
        assert!(rendered.contains("Approve write?"));
    }

    #[test]
    fn escalation_yield_renders_capability_reason_and_diff() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::Yield {
            request_id: "ck-1".into(),
            kind: YieldKind::Confirmation,
            payload: serde_json::json!({
                "summary": "Escalate tool call 'bash'?",
                "options": ["approve", "reject"],
                "details": {
                    "kind": "tool_escalation",
                    "capability": "network",
                    "policy": { "reason": "network access needs human judgment" },
                    "presentation": {
                        "form": "command",
                        "cmdline": "curl https://example.com",
                        "stdout": "",
                        "stderr": ""
                    }
                }
            }),
        }));
        let pending = reducer.pending_yield.clone().expect("pending");
        assert_eq!(pending.capability.as_deref(), Some("network"));
        assert_eq!(
            pending.reason.as_deref(),
            Some("network access needs human judgment")
        );
        let rendered = reducer
            .cells
            .last()
            .unwrap()
            .render_lines(opts(80))
            .join("\n");
        assert!(rendered.contains("capability: network"));
        assert!(rendered.contains("why: network access needs human judgment"));
        assert!(rendered.contains("$ curl https://example.com"));
    }

    #[test]
    fn confirmation_yield_becomes_pending_input_cell() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::Yield {
            request_id: "r-1".into(),
            kind: YieldKind::Confirmation,
            payload: serde_json::json!({"message": "Approve write?", "options": ["approve", "reject"]}),
        }));
        let pending = reducer.pending_yield.expect("pending yield");
        assert_eq!(pending.kind, YieldKind::Confirmation);
        assert_eq!(pending.options, vec!["approve", "reject"]);
        let content = pending.resume_content("reject").unwrap();
        assert!(
            matches!(&content[0], ContentPart::Structured { data } if data["choice"] == "reject")
        );
    }

    #[test]
    fn structured_input_yield_validates_single_select_answer() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::Yield {
            request_id: "r-1".into(),
            kind: YieldKind::StructuredInput,
            payload: serde_json::json!({
                "title": "Pick environment",
                "prompt": "Choose deploy target.",
                "questions": [{
                    "id": "env",
                    "label": "Environment",
                    "prompt": "Environment?",
                    "kind": "single_select",
                    "required": true,
                    "options": [
                        {"value": "staging", "label": "Staging"},
                        {"value": "prod", "label": "Production"}
                    ]
                }]
            }),
        }));

        let pending = reducer.pending_yield.expect("pending yield");
        assert_eq!(pending.title, "Pick environment");
        assert_eq!(pending.questions.len(), 1);
        assert!(pending.resume_content("qa").is_err());
        let content = pending.resume_content("Production").unwrap();
        assert!(matches!(&content[0], ContentPart::Structured { data } if data["env"] == "prod"));
    }

    #[test]
    fn runtime_progress_clears_stale_pending_yield() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::Yield {
            request_id: "r-1".into(),
            kind: YieldKind::Confirmation,
            payload: serde_json::json!({"message": "Approve?", "options": ["approve", "reject"]}),
        }));
        assert!(reducer.pending_yield.is_some());

        reducer.apply_envelope(envelope(Event::TurnCompleted {
            summary: Some("done".into()),
        }));

        assert!(reducer.pending_yield.is_none());
    }

    #[test]
    fn recoverable_error_does_not_clear_pending_yield() {
        let mut reducer = SessionReducer::default();
        reducer.apply_envelope(envelope(Event::Yield {
            request_id: "r-1".into(),
            kind: YieldKind::Confirmation,
            payload: serde_json::json!({"message": "Approve?", "options": ["approve", "reject"]}),
        }));
        reducer.apply_envelope(envelope(Event::Error {
            message: "still waiting".into(),
            recoverable: true,
        }));

        assert!(reducer.pending_yield.is_some());
    }
}
