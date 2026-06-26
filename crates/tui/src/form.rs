//! Interactive multi-field form for `StructuredInput` yields, so multi-question
//! requests are answered field-by-field instead of by hand-typed JSON.

use alan_agent_protocol::{StructuredInputKind, StructuredInputQuestion};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub question: StructuredInputQuestion,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormState {
    pub request_id: String,
    pub fields: Vec<FormField>,
    pub focus: usize,
    pub error: Option<String>,
}

impl FormState {
    pub fn new(request_id: String, questions: Vec<StructuredInputQuestion>) -> Self {
        let fields = questions
            .into_iter()
            .map(|question| {
                // Multi-select defaults are normalized into `default_values`
                // (comma-joined to match multi-select answer parsing); other
                // kinds use the single `default_value`.
                let value = if !question.default_values.is_empty() {
                    question.default_values.join(",")
                } else {
                    question.default_value.clone().unwrap_or_default()
                };
                FormField { question, value }
            })
            .collect();
        Self {
            request_id,
            fields,
            focus: 0,
            error: None,
        }
    }

    pub fn focused(&self) -> Option<&FormField> {
        self.fields.get(self.focus)
    }

    pub fn next_field(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + 1) % self.fields.len();
        }
    }

    pub fn prev_field(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + self.fields.len() - 1) % self.fields.len();
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        if let Some(field) = self.fields.get_mut(self.focus) {
            field.value.push(ch);
            self.error = None;
        }
    }

    pub fn backspace(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus) {
            field.value.pop();
            self.error = None;
        }
    }

    /// Build the JSON object (`{id: value}`) consumed by `resume_content`.
    pub fn answers_json(&self) -> String {
        let map: Map<String, Value> = self
            .fields
            .iter()
            // Omit blank optional fields: the resume validator treats a field as
            // omitted only when its key is absent, so sending "" would fail
            // type-specific validation (bool/number/select) for optional fields.
            // A blank required field is kept so the validator reports it.
            .filter(|field| !field.value.trim().is_empty() || field.question.required)
            .map(|field| {
                (
                    field.question.id.clone(),
                    Value::String(field.value.clone()),
                )
            })
            .collect();
        Value::Object(map).to_string()
    }

    /// One display line per field, the focused field marked.
    pub fn render_lines(&self) -> Vec<(String, bool)> {
        let mut lines: Vec<(String, bool)> = self
            .fields
            .iter()
            .enumerate()
            .map(|(idx, field)| {
                let marker = if idx == self.focus { "▶" } else { " " };
                let mut line = format!(
                    "{marker} {} [{}]: {}",
                    field.question.label,
                    kind_label(field.question.kind),
                    field.value
                );
                if !field.question.options.is_empty() {
                    let opts = field
                        .question
                        .options
                        .iter()
                        .map(|option| option.value.as_str())
                        .collect::<Vec<_>>()
                        .join("/");
                    line.push_str(&format!("  ({opts})"));
                }
                (line, idx == self.focus)
            })
            .collect();
        if let Some(error) = &self.error {
            lines.push((format!("  ! {error}"), false));
        }
        lines.push((
            "  tab/↑↓ move · type to edit · enter submit".to_string(),
            false,
        ));
        lines
    }
}

fn kind_label(kind: StructuredInputKind) -> &'static str {
    match kind {
        StructuredInputKind::Text => "text",
        StructuredInputKind::Boolean => "bool",
        StructuredInputKind::Number => "number",
        StructuredInputKind::Integer => "integer",
        StructuredInputKind::SingleSelect => "select",
        StructuredInputKind::MultiSelect => "multi",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(id: &str, kind: &str) -> StructuredInputQuestion {
        let options = if kind == "single_select" || kind == "multi_select" {
            serde_json::json!([
                {"value": "a", "label": "Apple"},
                {"value": "b", "label": "Banana"}
            ])
        } else {
            serde_json::json!([])
        };
        serde_json::from_value(serde_json::json!({
            "id": id,
            "label": id,
            "prompt": format!("{id}?"),
            "kind": kind,
            "required": true,
            "options": options,
        }))
        .unwrap()
    }

    #[test]
    fn typing_edits_focused_field_and_nav_moves_focus() {
        let mut form = FormState::new(
            "r1".into(),
            vec![question("name", "text"), question("env", "single_select")],
        );
        form.insert_char('h');
        form.insert_char('i');
        assert_eq!(form.fields[0].value, "hi");
        form.next_field();
        assert_eq!(form.focus, 1);
        form.insert_char('a');
        assert_eq!(form.fields[1].value, "a");
        form.prev_field();
        assert_eq!(form.focus, 0);
    }

    #[test]
    fn answers_json_omits_blank_optional_fields() {
        let optional = serde_json::from_value::<StructuredInputQuestion>(serde_json::json!({
            "id": "note", "label": "note", "prompt": "note?", "kind": "text", "required": false
        }))
        .unwrap();
        let form = FormState::new("r1".into(), vec![question("name", "text"), optional]);
        // Both blank: required "name" is kept (validator will flag it), optional
        // "note" is omitted entirely.
        let json: Value = serde_json::from_str(&form.answers_json()).unwrap();
        assert!(json.get("name").is_some());
        assert!(json.get("note").is_none());
    }

    #[test]
    fn multi_select_defaults_seed_the_field() {
        let q = serde_json::from_value::<StructuredInputQuestion>(serde_json::json!({
            "id": "env", "label": "env", "prompt": "env?", "kind": "multi_select",
            "required": true, "defaults": ["staging", "prod"],
            "options": [{"value":"staging","label":"S"},{"value":"prod","label":"P"}]
        }))
        .unwrap();
        let form = FormState::new("r1".into(), vec![q]);
        assert_eq!(form.fields[0].value, "staging,prod");
        // Pressing Enter without editing submits the defaulted selection.
        let json: Value = serde_json::from_str(&form.answers_json()).unwrap();
        assert_eq!(json["env"], "staging,prod");
    }

    #[test]
    fn answers_json_is_keyed_by_id() {
        let mut form = FormState::new("r1".into(), vec![question("name", "text")]);
        form.insert_char('x');
        let json: Value = serde_json::from_str(&form.answers_json()).unwrap();
        assert_eq!(json["name"], "x");
    }
}
