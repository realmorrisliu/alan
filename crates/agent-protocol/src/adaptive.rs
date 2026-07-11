use serde::{Deserialize, Serialize};

/// Presentation-level UI hint for adaptive form fields and confirmation surfaces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdaptivePresentationHint {
    Radio,
    Toggle,
    Searchable,
    Multiline,
    Compact,
    Dangerous,
}

/// Option for a structured/adaptive user-input field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredInputOption {
    pub value: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Input control semantics for a structured user-input field.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StructuredInputKind {
    /// Free-form text entry.
    #[default]
    Text,
    /// Boolean choice (typically yes/no).
    Boolean,
    /// Numeric entry that accepts any finite number.
    Number,
    /// Numeric entry that accepts integer values only.
    Integer,
    /// Exactly one value must be selected from a list of options.
    SingleSelect,
    /// Zero or more values may be selected from a list of options.
    MultiSelect,
}

/// Structured question/field shown to the user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredInputQuestion {
    pub id: String,
    pub label: String,
    pub prompt: String,
    #[serde(default)]
    pub kind: StructuredInputKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
    #[serde(default, rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, rename = "defaults", skip_serializing_if = "Vec::is_empty")]
    pub default_values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_selected: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_selected: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<StructuredInputOption>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presentation_hints: Vec<AdaptivePresentationHint>,
}

/// Typed adaptive form contract for custom yields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdaptiveForm {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<StructuredInputQuestion>,
}

/// Payload for `YieldKind::Confirmation`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfirmationYieldPayload {
    pub checkpoint_type: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_option: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presentation_hints: Vec<AdaptivePresentationHint>,
}

/// Payload for `YieldKind::StructuredInput`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredInputYieldPayload {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<StructuredInputQuestion>,
}

/// Payload for `YieldKind::Custom`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomYieldPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<AdaptiveForm>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn structured_input_kind_supports_extended_semantics() {
        let json = serde_json::to_string(&StructuredInputKind::Boolean).unwrap();
        assert_eq!(json, "\"boolean\"");
        assert_eq!(
            serde_json::from_str::<StructuredInputKind>("\"integer\"").unwrap(),
            StructuredInputKind::Integer
        );
    }

    #[test]
    fn confirmation_payload_serialization_skips_empty_optional_fields() {
        let payload = ConfirmationYieldPayload {
            checkpoint_type: "confirmation".to_string(),
            summary: "Approve?".to_string(),
            details: None,
            options: vec![],
            default_option: None,
            presentation_hints: vec![],
        };

        let value = serde_json::to_value(payload).unwrap();
        assert_eq!(value["checkpoint_type"], "confirmation");
        assert_eq!(value["summary"], "Approve?");
        assert!(value.get("details").is_none());
        assert!(value.get("options").is_none());
        assert!(value.get("presentation_hints").is_none());
    }

    #[test]
    fn adaptive_form_serializes_explicit_fields() {
        let form = AdaptiveForm {
            fields: vec![StructuredInputQuestion {
                id: "success".to_string(),
                label: "Success".to_string(),
                prompt: "Did it work?".to_string(),
                kind: StructuredInputKind::Boolean,
                required: true,
                placeholder: None,
                help_text: None,
                default_value: Some("true".to_string()),
                default_values: vec![],
                min_selected: None,
                max_selected: None,
                options: vec![
                    StructuredInputOption {
                        value: "true".to_string(),
                        label: "Yes".to_string(),
                        description: None,
                    },
                    StructuredInputOption {
                        value: "false".to_string(),
                        label: "No".to_string(),
                        description: None,
                    },
                ],
                presentation_hints: vec![AdaptivePresentationHint::Toggle],
            }],
        };

        let value = serde_json::to_value(form).unwrap();
        assert_eq!(value["fields"][0]["kind"], json!("boolean"));
        assert_eq!(value["fields"][0]["presentation_hints"], json!(["toggle"]));
    }
}
