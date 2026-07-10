//! Bounded prompt projections for durable namespace evidence.
//!
//! Full evidence remains a file. Tape records carry only a preview, a walkable
//! namespace reference, and explicit truncation/redaction metadata.

use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use serde_json::{Map, Value};

pub(crate) const MAX_INLINE_EVIDENCE_BYTES: usize = 30_000;
const MAX_EVIDENCE_PREVIEW_BYTES: usize = 8_000;
pub(crate) const RETENTION_EXPIRED_RECORD_TYPE: &str = "evidence_retention_expired";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceEvidenceReference {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceTruncation {
    pub original_bytes: usize,
    pub preview_bytes: usize,
    pub full_content_recoverable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRedactionMarker {
    pub marker: String,
    pub reason_class: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceProjection {
    #[serde(rename = "type")]
    pub record_type: String,
    pub preview: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<NamespaceEvidenceReference>,
    pub truncation: EvidenceTruncation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redactions: Vec<EvidenceRedactionMarker>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceResolutionErrorCode {
    Missing,
    RetentionExpired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceResolutionError {
    pub code: EvidenceResolutionErrorCode,
    pub reference: NamespaceEvidenceReference,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_run: Option<Value>,
}

impl std::fmt::Display for EvidenceResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.reference.path, self.message)
    }
}

impl std::error::Error for EvidenceResolutionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RedactedEvidence {
    pub text: String,
    pub markers: Vec<EvidenceRedactionMarker>,
}

pub(crate) fn payload_needs_projection(payload: &Value) -> bool {
    serde_json::to_vec(payload)
        .map(|bytes| bytes.len() > MAX_INLINE_EVIDENCE_BYTES)
        .unwrap_or(true)
}

pub(crate) fn project_evidence_payload(
    payload: &Value,
    reference: Option<NamespaceEvidenceReference>,
    redactions: Vec<EvidenceRedactionMarker>,
    fallback_reason: Option<String>,
) -> Value {
    let serialized = serde_json::to_string(payload).unwrap_or_else(|_| payload.to_string());
    let preview = utf8_prefix_with_marker(&serialized, MAX_EVIDENCE_PREVIEW_BYTES);
    let recoverable = reference.is_some();
    let mut metadata = Map::new();
    if let Some(object) = payload.as_object() {
        for key in [
            "success",
            "exit_code",
            "process",
            "action_id",
            "status",
            "summary",
        ] {
            if let Some(value) = object.get(key) {
                metadata.insert(key.to_string(), value.clone());
            }
        }
    }
    serde_json::to_value(EvidenceProjection {
        record_type: "evidence_projection".to_string(),
        preview: preview.clone(),
        reference,
        truncation: EvidenceTruncation {
            original_bytes: serialized.len(),
            preview_bytes: preview.len(),
            full_content_recoverable: recoverable,
            fallback_reason,
        },
        redactions,
        metadata,
    })
    .expect("evidence projection is serializable")
}

pub(crate) fn redact_durable_evidence_text(text: &str) -> RedactedEvidence {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        let mut markers = Vec::new();
        let value = redact_json_value(&value, &mut markers);
        return RedactedEvidence {
            text: serde_json::to_string(&value).unwrap_or_else(|_| text.to_string()),
            markers,
        };
    }

    let mut markers = Vec::new();
    let redacted_lines = text
        .lines()
        .map(|line| redact_sensitive_line(line, &mut markers))
        .collect::<Vec<_>>()
        .join("\n");
    RedactedEvidence {
        text: if text.ends_with('\n') {
            format!("{redacted_lines}\n")
        } else {
            redacted_lines
        },
        markers,
    }
}

pub(crate) fn redaction_markers_in_text(text: &str) -> Vec<EvidenceRedactionMarker> {
    let mut markers = Vec::new();
    for reason in ["secret_key", "credential_token"] {
        let marker = format!("[REDACTED reason={reason}]");
        if text.contains(&marker) {
            markers.push(EvidenceRedactionMarker {
                marker,
                reason_class: reason.to_string(),
            });
        }
    }
    markers
}

pub(crate) fn is_retention_expired_record(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .is_some_and(|record_type| record_type == RETENTION_EXPIRED_RECORD_TYPE)
}

fn redact_json_value(value: &Value, markers: &mut Vec<EvidenceRedactionMarker>) -> Value {
    match value {
        Value::Object(object) => {
            let mut redacted = Map::new();
            for (key, value) in object {
                if sensitive_key(key) {
                    let marker = marker("secret_key");
                    push_marker(markers, marker.clone(), "secret_key");
                    redacted.insert(key.clone(), Value::String(marker));
                } else {
                    redacted.insert(key.clone(), redact_json_value(value, markers));
                }
            }
            Value::Object(redacted)
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| redact_json_value(value, markers))
                .collect(),
        ),
        Value::String(value) => Value::String(redact_bearer(value, markers)),
        _ => value.clone(),
    }
}

fn redact_sensitive_line(line: &str, markers: &mut Vec<EvidenceRedactionMarker>) -> String {
    let lower = line.to_ascii_lowercase();
    if let Some(separator) = line.find([':', '=']) {
        let key = &line[..separator];
        if sensitive_key(key.trim()) {
            let marker = marker("secret_key");
            push_marker(markers, marker.clone(), "secret_key");
            return format!("{}{} {}", key, &line[separator..=separator], marker);
        }
    }
    if lower.contains("bearer ") {
        return redact_bearer(line, markers);
    }
    line.to_string()
}

fn redact_bearer(text: &str, markers: &mut Vec<EvidenceRedactionMarker>) -> String {
    let lower = text.to_ascii_lowercase();
    let Some(start) = lower.find("bearer ") else {
        return text.to_string();
    };
    let token_start = start + "bearer ".len();
    let token_end = text[token_start..]
        .find(char::is_whitespace)
        .map(|offset| token_start + offset)
        .unwrap_or(text.len());
    let marker = marker("credential_token");
    push_marker(markers, marker.clone(), "credential_token");
    format!("{}{}{}", &text[..token_start], marker, &text[token_end..])
}

fn sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "setcookie"
            | "apikey"
            | "xapikey"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "bearertoken"
            | "clientsecret"
            | "secret"
    ) || normalized.contains("apikey")
}

fn marker(reason: &str) -> String {
    format!("[REDACTED reason={reason}]")
}

fn push_marker(markers: &mut Vec<EvidenceRedactionMarker>, marker: String, reason_class: &str) {
    if markers
        .iter()
        .any(|existing| existing.reason_class == reason_class)
    {
        return;
    }
    markers.push(EvidenceRedactionMarker {
        marker,
        reason_class: reason_class.to_string(),
    });
}

fn utf8_prefix_with_marker(text: &str, max_bytes: usize) -> String {
    const MARKER: &str = "...[truncated; inspect reference]";
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let prefix_budget = max_bytes.saturating_sub(MARKER.len());
    let mut end = prefix_budget.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{MARKER}", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_markers_are_distinct_from_truncation_metadata() {
        let redacted =
            redact_durable_evidence_text(r#"{"authorization":"Bearer top-secret","body":"safe"}"#);
        let projection = project_evidence_payload(
            &json!({"output": "x".repeat(MAX_INLINE_EVIDENCE_BYTES + 1)}),
            Some(NamespaceEvidenceReference {
                path: "/agent/1/actions/a0/output".to_string(),
                offset: Some(0),
                length: Some(42),
            }),
            redacted.markers.clone(),
            None,
        );

        assert!(redacted.text.contains("[REDACTED reason=secret_key]"));
        assert_eq!(projection["redactions"][0]["reason_class"], "secret_key");
        assert_eq!(projection["truncation"]["full_content_recoverable"], true);
    }

    #[test]
    fn unresolvable_projection_marks_inline_fallback() {
        let payload = json!({"output": "x".repeat(MAX_INLINE_EVIDENCE_BYTES + 1)});
        let projection = project_evidence_payload(
            &payload,
            None,
            Vec::new(),
            Some("reference_unresolvable".to_string()),
        );

        assert!(projection.get("reference").is_none());
        assert_eq!(projection["truncation"]["full_content_recoverable"], false);
        assert_eq!(
            projection["truncation"]["fallback_reason"],
            "reference_unresolvable"
        );
    }
}
