//! Interactive multi-field form for `StructuredInput` yields, so multi-question
//! requests are answered field-by-field instead of by hand-typed JSON.

use alan_agent_protocol::{StructuredInputKind, StructuredInputQuestion};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub question: StructuredInputQuestion,
    pub value: String,
    pub cursor: usize,
    pub option_index: usize,
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
                let cursor = value.len();
                let option_index = question
                    .options
                    .iter()
                    .position(|opt| opt.value == value)
                    .unwrap_or(0);
                FormField {
                    question,
                    value,
                    cursor,
                    option_index,
                }
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

    pub fn focused_is_select(&self) -> bool {
        self.focused().is_some_and(is_select_field)
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

    pub fn next_option(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus)
            && !field.question.options.is_empty()
        {
            field.option_index = (field.option_index + 1) % field.question.options.len();
        }
    }

    pub fn prev_option(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus)
            && !field.question.options.is_empty()
        {
            let len = field.question.options.len();
            field.option_index = (field.option_index + len - 1) % len;
        }
    }

    pub fn toggle_selected_option(&mut self) -> bool {
        let Some(field) = self.fields.get_mut(self.focus) else {
            return false;
        };
        if !is_select_field(field) {
            return false;
        }
        let Some(opt) = field.question.options.get(field.option_index) else {
            return false;
        };
        let chosen = opt.value.clone();
        match field.question.kind {
            StructuredInputKind::SingleSelect => {
                field.value = chosen;
            }
            StructuredInputKind::MultiSelect => {
                let mut selected: Vec<String> = field
                    .value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                if let Some(pos) = selected.iter().position(|v| v == &chosen) {
                    selected.remove(pos);
                } else {
                    selected.push(chosen);
                }
                field.value = selected.join(",");
            }
            _ => return false,
        }
        field.cursor = field.value.len();
        self.error = None;
        true
    }

    pub fn select_current_option_if_single_empty(&mut self) {
        let Some(field) = self.fields.get_mut(self.focus) else {
            return;
        };
        if field.question.kind == StructuredInputKind::SingleSelect
            && field.value.trim().is_empty()
            && let Some(opt) = field.question.options.get(field.option_index)
        {
            field.value = opt.value.clone();
            field.cursor = field.value.len();
            self.error = None;
        }
    }

    pub fn move_cursor_left(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus)
            && field.cursor > 0
        {
            field.cursor = field.value[..field.cursor.min(field.value.len())]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_cursor_right(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus)
            && field.cursor < field.value.len()
        {
            let next = field.value[field.cursor..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(0);
            field.cursor = (field.cursor + next).min(field.value.len());
        }
    }

    pub fn move_home(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus) {
            field.cursor = 0;
        }
    }

    pub fn move_end(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus) {
            field.cursor = field.value.len();
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        if let Some(field) = self.fields.get_mut(self.focus) {
            let pos = field.cursor.min(field.value.len());
            field.value.insert(pos, ch);
            field.cursor = pos + ch.len_utf8();
            self.error = None;
        }
    }

    pub fn backspace(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus) {
            let pos = field.cursor.min(field.value.len());
            if pos > 0 {
                let prev = field.value[..pos]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                field.value.drain(prev..pos);
                field.cursor = prev;
                self.error = None;
            }
        }
    }

    pub fn delete_forward(&mut self) {
        if let Some(field) = self.fields.get_mut(self.focus) {
            let pos = field.cursor.min(field.value.len());
            if pos < field.value.len() {
                let len = field.value[pos..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(0);
                field.value.drain(pos..pos + len);
                self.error = None;
            }
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

    /// The focused field through its editable value, excluding trailing option help.
    pub(crate) fn focused_input_line(&self) -> Option<String> {
        self.focused().map(|field| field_input_line(field, true))
    }

    /// One display line per field, the focused field marked.
    pub fn render_lines(&self) -> Vec<(String, bool)> {
        let mut lines = Vec::new();
        for (idx, field) in self.fields.iter().enumerate() {
            let focused = idx == self.focus;
            let mut line = field_input_line(field, focused);
            if !field.question.options.is_empty() && !focused {
                let opts = field
                    .question
                    .options
                    .iter()
                    .map(|option| option.value.as_str())
                    .collect::<Vec<_>>()
                    .join("/");
                line.push_str(&format!("  ({opts})"));
            }
            lines.push((line, focused));
            if focused && is_select_field(field) {
                let selected: Vec<&str> = field
                    .value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                for (opt_idx, option) in field.question.options.iter().enumerate() {
                    let cursor = if opt_idx == field.option_index {
                        "▸"
                    } else {
                        " "
                    };
                    let mark = if selected
                        .iter()
                        .any(|v| v.eq_ignore_ascii_case(&option.value))
                    {
                        "✓"
                    } else {
                        " "
                    };
                    lines.push((
                        format!("    {cursor} [{mark}] {} — {}", option.value, option.label),
                        opt_idx == field.option_index,
                    ));
                }
            }
        }
        if let Some(error) = &self.error {
            lines.push((format!("  ! {error}"), false));
        }
        lines.push((
            "  tab fields · ←→ cursor · ↑↓/space select · enter submit".to_string(),
            false,
        ));
        lines
    }
}

fn is_select_field(field: &FormField) -> bool {
    matches!(
        field.question.kind,
        StructuredInputKind::SingleSelect | StructuredInputKind::MultiSelect
    ) && !field.question.options.is_empty()
}

fn field_input_line(field: &FormField, focused: bool) -> String {
    let marker = if focused { "▶" } else { " " };
    format!(
        "{marker} {} [{}]: {}",
        field.question.label,
        kind_label(field.question.kind),
        field.value
    )
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

    #[test]
    fn cursor_navigation_and_select_toggles_work() {
        let mut form = FormState::new(
            "r1".into(),
            vec![
                question("name", "text"),
                question("env", "single_select"),
                question("tags", "multi_select"),
            ],
        );
        form.insert_char('a');
        form.insert_char('c');
        form.move_cursor_left();
        form.insert_char('b');
        assert_eq!(form.fields[0].value, "abc");
        form.move_home();
        form.delete_forward();
        assert_eq!(form.fields[0].value, "bc");
        form.move_end();
        form.backspace();
        assert_eq!(form.fields[0].value, "b");

        form.next_field();
        assert!(form.focused_is_select());
        form.next_option();
        form.toggle_selected_option();
        assert_eq!(form.fields[1].value, "b");

        form.next_field();
        form.toggle_selected_option();
        form.next_option();
        form.toggle_selected_option();
        assert_eq!(form.fields[2].value, "a,b");
        form.toggle_selected_option();
        assert_eq!(form.fields[2].value, "a");
    }
}
