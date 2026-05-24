use alan_protocol::{
    ContentPart, Event, EventEnvelope, StructuredInputKind, StructuredInputQuestion, YieldKind,
};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryCell {
    User(String),
    Assistant(String),
    Thinking(String),
    Tool {
        id: String,
        name: String,
        status: ToolStatus,
        preview: Option<String>,
    },
    Plan(Vec<String>),
    PendingYield(PendingYieldCell),
    Warning(String),
    Error {
        message: String,
        recoverable: bool,
    },
    Status(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Complete,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingYieldCell {
    pub request_id: String,
    pub kind: YieldKind,
    pub title: String,
    pub prompt: Option<String>,
    pub options: Vec<String>,
    pub questions: Vec<StructuredInputQuestion>,
}

impl HistoryCell {
    pub fn render_lines(&self, width: usize) -> Vec<String> {
        let width = width.max(16);
        let (prefix, body) = match self {
            Self::User(text) => ("you", text.clone()),
            Self::Assistant(text) => ("alan", text.clone()),
            Self::Thinking(text) => ("thinking", text.clone()),
            Self::Tool {
                name,
                status,
                preview,
                ..
            } => {
                let status = match status {
                    ToolStatus::Running => "running",
                    ToolStatus::Complete => "done",
                    ToolStatus::Failed => "failed",
                };
                let preview = preview.as_deref().unwrap_or("");
                ("tool", format!("{name} {status} {preview}"))
            }
            Self::Plan(items) => ("plan", items.join(" / ")),
            Self::PendingYield(pending) => {
                let mut body = format!("{} {}", pending.title, pending.request_id);
                if let Some(prompt) = &pending.prompt {
                    body.push_str(&format!(" - {prompt}"));
                }
                if !pending.options.is_empty() {
                    body.push_str(&format!(" - choices: {}", pending.options.join(", ")));
                }
                for question in &pending.questions {
                    body.push_str(&format!(
                        " - {} [{}]: {}",
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
                ("input", body)
            }
            Self::Warning(message) => ("warning", message.clone()),
            Self::Error {
                message,
                recoverable,
            } => {
                let kind = if *recoverable { "recoverable" } else { "fatal" };
                ("error", format!("{kind}: {message}"))
            }
            Self::Status(message) => ("status", message.clone()),
        };

        textwrap::wrap(&body, width.saturating_sub(prefix.len() + 3))
            .into_iter()
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
}

#[derive(Debug, Default, Clone)]
pub struct SessionReducer {
    pub cells: Vec<HistoryCell>,
    pub pending_yield: Option<PendingYieldCell>,
}

impl SessionReducer {
    pub fn apply_envelope(&mut self, envelope: EventEnvelope) {
        let event = envelope.event;
        if event_advances_past_pending_yield(&event) {
            self.pending_yield = None;
        }

        match event {
            Event::TurnStarted {} => self.cells.push(HistoryCell::Status("turn started".into())),
            Event::TurnCompleted { summary } => {
                self.cells.push(HistoryCell::Status(
                    summary.unwrap_or_else(|| "turn completed".into()),
                ));
            }
            Event::TextDelta { chunk, is_final: _ } => self.append_text(chunk),
            Event::ThinkingDelta { chunk, is_final: _ } => self.append_thinking(chunk),
            Event::ToolCallStarted { id, name, .. } => self.cells.push(HistoryCell::Tool {
                id,
                name,
                status: ToolStatus::Running,
                preview: None,
            }),
            Event::ToolCallCompleted {
                id,
                name,
                success,
                result_preview,
                ..
            } => self.complete_tool(id, name, success, result_preview),
            Event::PlanUpdated { items, .. } => self.cells.push(HistoryCell::Plan(
                items
                    .into_iter()
                    .map(|item| format!("{:?}: {}", item.status, item.content))
                    .collect(),
            )),
            Event::SessionRolledBack { turns, .. } => self
                .cells
                .push(HistoryCell::Status(format!("rolled back {turns} turns"))),
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
                };
                self.pending_yield = Some(cell.clone());
                self.cells.push(HistoryCell::PendingYield(cell));
            }
            Event::CompactionObserved { .. } => {
                self.cells
                    .push(HistoryCell::Status("context compacted".into()));
            }
            Event::MemoryFlushObserved { .. } => {
                self.cells
                    .push(HistoryCell::Status("memory flushed".into()));
            }
            Event::Warning { message } => self.cells.push(HistoryCell::Warning(message)),
            Event::Error {
                message,
                recoverable,
            } => self.cells.push(HistoryCell::Error {
                message,
                recoverable,
            }),
        }
    }

    fn append_text(&mut self, chunk: String) {
        match self.cells.last_mut() {
            Some(HistoryCell::Assistant(text)) => text.push_str(&chunk),
            _ => self.cells.push(HistoryCell::Assistant(chunk)),
        }
    }

    fn append_thinking(&mut self, chunk: String) {
        match self.cells.last_mut() {
            Some(HistoryCell::Thinking(text)) => text.push_str(&chunk),
            _ => self.cells.push(HistoryCell::Thinking(chunk)),
        }
    }

    fn complete_tool(
        &mut self,
        id: String,
        name: Option<String>,
        success: Option<bool>,
        result_preview: Option<String>,
    ) {
        if let Some(HistoryCell::Tool {
            name: tool_name,
            status,
            preview,
            ..
        }) =
            self.cells.iter_mut().rev().find(
                |cell| matches!(cell, HistoryCell::Tool { id: tool_id, .. } if tool_id == &id),
            )
        {
            if let Some(name) = name {
                *tool_name = name;
            }
            *status = if success == Some(false) {
                ToolStatus::Failed
            } else {
                ToolStatus::Complete
            };
            *preview = result_preview;
        }
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
