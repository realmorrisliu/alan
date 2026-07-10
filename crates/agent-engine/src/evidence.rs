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
const MAX_EVIDENCE_METADATA_VALUE_BYTES: usize = 512;
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

pub(crate) fn redact_evidence_payload(payload: &Value) -> Value {
    let mut markers = Vec::new();
    redact_json_value(payload, &mut markers)
}

/// Returns the preview from a structurally valid, already-bounded evidence
/// projection. Rollout persistence uses this to avoid applying the generic
/// 512-character tool-value cap to the runtime's bounded evidence preview.
pub(crate) fn bounded_projection_preview(payload: &Value) -> Option<&str> {
    let projection = serde_json::from_value::<EvidenceProjection>(payload.clone()).ok()?;
    if projection.record_type != "evidence_projection"
        || projection.preview.len() > MAX_EVIDENCE_PREVIEW_BYTES
        || projection.truncation.preview_bytes != projection.preview.len()
    {
        return None;
    }
    payload.get("preview")?.as_str()
}

pub(crate) fn project_evidence_payload(
    payload: &Value,
    reference: Option<NamespaceEvidenceReference>,
    mut redactions: Vec<EvidenceRedactionMarker>,
    fallback_reason: Option<String>,
) -> Value {
    let mut projection_redactions = Vec::new();
    let redacted_payload = redact_json_value(payload, &mut projection_redactions);
    for redaction in projection_redactions {
        push_marker(&mut redactions, redaction.marker, &redaction.reason_class);
    }
    let serialized =
        serde_json::to_string(&redacted_payload).unwrap_or_else(|_| redacted_payload.to_string());
    let preview = utf8_prefix_with_marker(&serialized, MAX_EVIDENCE_PREVIEW_BYTES);
    let recoverable = reference.is_some();
    let mut metadata = Map::new();
    if let Some(object) = redacted_payload.as_object() {
        for key in [
            "success",
            "exit_code",
            "process",
            "action_id",
            "status",
            "summary",
        ] {
            if let Some(value) = object.get(key) {
                metadata.insert(key.to_string(), bounded_metadata_value(value));
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

fn bounded_metadata_value(value: &Value) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::String(text) => Value::String(utf8_prefix_with_marker(
            text,
            MAX_EVIDENCE_METADATA_VALUE_BYTES,
        )),
        Value::Array(_) | Value::Object(_) => {
            let serialized = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
            Value::String(utf8_prefix_with_marker(
                &serialized,
                MAX_EVIDENCE_METADATA_VALUE_BYTES,
            ))
        }
    }
}

pub(crate) fn redact_durable_evidence_text(text: &str) -> RedactedEvidence {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        let mut markers = Vec::new();
        let value = redact_json_value(&value, &mut markers);
        if markers.is_empty() {
            return RedactedEvidence {
                text: text.to_string(),
                markers,
            };
        }
        return RedactedEvidence {
            text: serde_json::to_string(&value).unwrap_or_else(|_| text.to_string()),
            markers,
        };
    }

    let mut markers = Vec::new();
    RedactedEvidence {
        text: redact_line_oriented_text(text, &mut markers),
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
        Value::String(value) => Value::String(redact_line_oriented_text(value, markers)),
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
    let line = redact_url_query_secrets(line, markers);
    if lower.contains("bearer ") {
        return redact_bearer(&line, markers);
    }
    line
}

fn redact_url_query_secrets(text: &str, markers: &mut Vec<EvidenceRedactionMarker>) -> String {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    let mut copied_through = 0;
    let mut redacted = String::with_capacity(text.len());
    let mut found = false;

    while cursor < bytes.len() {
        if !matches!(bytes[cursor], b'?' | b'&') {
            cursor += 1;
            continue;
        }

        let key_start = cursor + 1;
        let mut separator = key_start;
        while separator < bytes.len()
            && !matches!(
                bytes[separator],
                b'=' | b'&' | b'#' | b' ' | b'\t' | b'\r' | b'\n'
            )
        {
            separator += 1;
        }
        if separator >= bytes.len()
            || bytes[separator] != b'='
            || !sensitive_key(&text[key_start..separator])
        {
            cursor = separator.max(cursor + 1);
            continue;
        }

        let value_start = separator + 1;
        let mut value_end = value_start;
        while value_end < bytes.len()
            && !matches!(
                bytes[value_end],
                b'&' | b'#' | b' ' | b'\t' | b'\r' | b'\n' | b'\'' | b'"' | b'<' | b'>'
            )
        {
            value_end += 1;
        }
        redacted.push_str(&text[copied_through..value_start]);
        redacted.push_str("[REDACTED reason=secret_key]");
        copied_through = value_end;
        cursor = value_end;
        found = true;
    }

    if !found {
        return text.to_string();
    }
    redacted.push_str(&text[copied_through..]);
    push_marker(markers, marker("secret_key"), "secret_key");
    redacted
}

fn redact_line_oriented_text(text: &str, markers: &mut Vec<EvidenceRedactionMarker>) -> String {
    let redacted_lines = text
        .lines()
        .map(|line| redact_sensitive_line(line, markers))
        .collect::<Vec<_>>()
        .join("\n");
    if text.ends_with('\n') {
        format!("{redacted_lines}\n")
    } else {
        redacted_lines
    }
}

fn redact_bearer(text: &str, markers: &mut Vec<EvidenceRedactionMarker>) -> String {
    let marker = marker("credential_token");
    let mut remaining = text;
    let mut redacted = String::with_capacity(text.len());
    let mut found = false;

    loop {
        let lower = remaining.to_ascii_lowercase();
        let Some(start) = lower.find("bearer ") else {
            redacted.push_str(remaining);
            break;
        };
        let token_start = start + "bearer ".len();
        let token_end = remaining[token_start..]
            .find(char::is_whitespace)
            .map(|offset| token_start + offset)
            .unwrap_or(remaining.len());
        redacted.push_str(&remaining[..token_start]);
        redacted.push_str(&marker);
        remaining = &remaining[token_end..];
        found = true;
    }

    if found {
        push_marker(markers, marker, "credential_token");
        redacted
    } else {
        text.to_string()
    }
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
            | "password"
            | "passwd"
            | "passphrase"
            | "token"
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
    fn json_evidence_without_redactions_preserves_original_bytes() {
        let original = "{\n  \"duplicate\": 1,\n  \"duplicate\": 2,\n  \"safe\": true\n}\n";

        let redacted = redact_durable_evidence_text(original);

        assert_eq!(redacted.text, original);
        assert!(redacted.markers.is_empty());
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

    #[test]
    fn projection_preview_redacts_sensitive_payload_fields() {
        let projection = project_evidence_payload(
            &json!({
                "authorization": "Bearer preview-secret",
                "output": "x".repeat(MAX_INLINE_EVIDENCE_BYTES + 1)
            }),
            None,
            Vec::new(),
            Some("reference_unresolvable".to_string()),
        );

        let preview = projection["preview"].as_str().unwrap();
        assert!(preview.contains("[REDACTED reason=secret_key]"));
        assert!(!preview.contains("preview-secret"));
        assert_eq!(projection["redactions"][0]["reason_class"], "secret_key");
    }

    #[test]
    fn projection_metadata_is_bounded_and_scalar_normalized() {
        let projection = project_evidence_payload(
            &json!({
                "output": "x".repeat(MAX_INLINE_EVIDENCE_BYTES + 1),
                "summary": "s".repeat(100_000),
                "process": {"nested": "p".repeat(100_000)}
            }),
            Some(NamespaceEvidenceReference {
                path: "/agent/1/actions/a0/output".to_string(),
                offset: Some(0),
                length: None,
            }),
            Vec::new(),
            None,
        );

        let summary = projection["metadata"]["summary"].as_str().unwrap();
        let process = projection["metadata"]["process"].as_str().unwrap();
        assert!(summary.len() <= MAX_EVIDENCE_METADATA_VALUE_BYTES);
        assert!(process.len() <= MAX_EVIDENCE_METADATA_VALUE_BYTES);
        assert!(summary.contains("...[truncated; inspect reference]"));
        assert!(process.contains("...[truncated; inspect reference]"));
        assert!(serde_json::to_vec(&projection).unwrap().len() < 12_000);
    }

    #[test]
    fn bounded_projection_preview_accepts_runtime_projection_and_rejects_oversized_preview() {
        let projection = project_evidence_payload(
            &json!({"output": "x".repeat(MAX_INLINE_EVIDENCE_BYTES + 1)}),
            None,
            Vec::new(),
            Some("reference_unresolvable".to_string()),
        );
        assert_eq!(
            bounded_projection_preview(&projection),
            projection["preview"].as_str()
        );

        let mut oversized = projection;
        oversized["preview"] = json!("x".repeat(MAX_EVIDENCE_PREVIEW_BYTES + 1));
        oversized["truncation"]["preview_bytes"] = json!(MAX_EVIDENCE_PREVIEW_BYTES + 1);
        assert!(bounded_projection_preview(&oversized).is_none());
    }

    #[test]
    fn projection_preview_redacts_line_oriented_secrets_inside_output_strings() {
        let projection = project_evidence_payload(
            &json!({
                "output": format!(
                    "api_key=preview-secret\nSECRET: another-secret\n{}",
                    "x".repeat(MAX_INLINE_EVIDENCE_BYTES)
                )
            }),
            Some(NamespaceEvidenceReference {
                path: "/agent/1/actions/a0/output".to_string(),
                offset: Some(0),
                length: None,
            }),
            Vec::new(),
            None,
        );

        let serialized = projection.to_string();
        assert!(!serialized.contains("preview-secret"));
        assert!(!serialized.contains("another-secret"));
        assert!(serialized.contains("[REDACTED reason=secret_key]"));
    }

    #[test]
    fn redacts_every_bearer_token_in_one_value() {
        let redacted =
            redact_durable_evidence_text(r#"{"message":"Bearer first token-gap Bearer second"}"#);

        assert!(!redacted.text.contains("first"));
        assert!(!redacted.text.contains("second"));
        assert_eq!(
            redacted
                .text
                .matches("[REDACTED reason=credential_token]")
                .count(),
            2
        );
    }

    #[test]
    fn redacts_sensitive_url_query_parameters_anywhere_in_a_line() {
        let redacted = redact_durable_evidence_text(
            "download: https://host/cb?api_key=first&safe=ok\nurl=https://host/file?access_token=second#fragment",
        );

        assert!(!redacted.text.contains("first"));
        assert!(!redacted.text.contains("second"));
        assert!(redacted.text.contains("safe=ok"));
        assert_eq!(
            redacted
                .text
                .matches("[REDACTED reason=secret_key]")
                .count(),
            2
        );
    }

    #[test]
    fn redacts_password_passphrase_and_plain_token_credentials() {
        let json = redact_durable_evidence_text(
            r#"{"password":"json-password","passphrase":"json-passphrase"}"#,
        );
        let text = redact_durable_evidence_text(
            "password=line-password\ndownload: https://host/file?token=query-token",
        );

        for secret in [
            "json-password",
            "json-passphrase",
            "line-password",
            "query-token",
        ] {
            assert!(!json.text.contains(secret));
            assert!(!text.text.contains(secret));
        }
        assert!(json.text.contains("[REDACTED reason=secret_key]"));
        assert_eq!(text.text.matches("[REDACTED reason=secret_key]").count(), 2);
    }
}
