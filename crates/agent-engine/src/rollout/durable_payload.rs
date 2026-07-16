use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub(super) const DURABLE_PAYLOAD_MAX_STRING_CHARS: usize = 512;
const DURABLE_PAYLOAD_MAX_ARRAY_ITEMS: usize = 32;
const DURABLE_PAYLOAD_MAX_OBJECT_FIELDS: usize = 64;
const DURABLE_PREVIEW_MAX_CHARS: usize = 160;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ToolPayloadRedactionSummary {
    #[serde(default)]
    pub redacted_fields: usize,
    #[serde(default)]
    pub truncated_values: usize,
}

#[derive(Debug, Clone)]
pub struct DurableToolPayload {
    pub payload: Value,
    pub digest: String,
    pub preview: Option<String>,
    pub redaction: Option<ToolPayloadRedactionSummary>,
}

pub fn build_durable_tool_payload(payload: &Value) -> DurableToolPayload {
    let mut summary = ToolPayloadRedactionSummary::default();
    let redacted_payload = crate::evidence::redact_evidence_payload(payload);
    let projection_preview = crate::evidence::bounded_projection_preview(&redacted_payload);
    let mut durable_payload = sanitize_payload_for_rollout(&redacted_payload, &mut summary);
    if let (Some(preview), Some(object)) = (projection_preview, durable_payload.as_object_mut()) {
        object.insert("preview".to_string(), Value::String(preview.to_string()));
        if preview
            .chars()
            .nth(DURABLE_PAYLOAD_MAX_STRING_CHARS)
            .is_some()
        {
            summary.truncated_values = summary.truncated_values.saturating_sub(1);
        }
    }
    let digest = sha256_hex(&canonicalize_json(&durable_payload).to_string());
    let preview = payload_preview(&durable_payload);
    let redaction =
        (summary.redacted_fields > 0 || summary.truncated_values > 0).then_some(summary);

    DurableToolPayload {
        payload: durable_payload,
        digest,
        preview,
        redaction,
    }
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                if let Some(entry) = map.get(key) {
                    sorted.insert(key.clone(), canonicalize_json(entry));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json).collect()),
        _ => value.clone(),
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn normalize_sensitive_key(key: &str) -> String {
    key.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = normalize_sensitive_key(key);
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
        || ["token", "secret", "password", "passwd", "passphrase"]
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
}

fn truncate_string_for_rollout(text: &str, summary: &mut ToolPayloadRedactionSummary) -> String {
    let mut chars = text.chars();
    let preview: String = chars
        .by_ref()
        .take(DURABLE_PAYLOAD_MAX_STRING_CHARS)
        .collect();
    if chars.next().is_none() {
        return text.to_string();
    }

    summary.truncated_values += 1;
    format!("{preview}...[truncated]")
}

fn sanitize_payload_for_rollout(
    payload: &Value,
    summary: &mut ToolPayloadRedactionSummary,
) -> Value {
    match payload {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();

            let omitted = keys.len().saturating_sub(DURABLE_PAYLOAD_MAX_OBJECT_FIELDS);
            let mut sanitized = Map::new();
            for key in keys.into_iter().take(DURABLE_PAYLOAD_MAX_OBJECT_FIELDS) {
                let value = map.get(key).expect("key from map iteration must exist");
                if is_sensitive_key(key) {
                    summary.redacted_fields += 1;
                    sanitized.insert(
                        key.clone(),
                        Value::String("[REDACTED reason=secret_key]".to_string()),
                    );
                } else {
                    sanitized.insert(key.clone(), sanitize_payload_for_rollout(value, summary));
                }
            }

            if omitted > 0 {
                summary.truncated_values += omitted;
                sanitized.insert(
                    "_truncated".to_string(),
                    Value::String(format!("{omitted} additional field(s) omitted")),
                );
            }

            Value::Object(sanitized)
        }
        Value::Array(items) => {
            let omitted = items.len().saturating_sub(DURABLE_PAYLOAD_MAX_ARRAY_ITEMS);
            let mut sanitized: Vec<Value> = items
                .iter()
                .take(DURABLE_PAYLOAD_MAX_ARRAY_ITEMS)
                .map(|item| sanitize_payload_for_rollout(item, summary))
                .collect();

            if omitted > 0 {
                summary.truncated_values += omitted;
                sanitized.push(serde_json::json!({
                    "_truncated": format!("{omitted} additional item(s) omitted")
                }));
            }

            Value::Array(sanitized)
        }
        Value::String(text) => Value::String(truncate_string_for_rollout(text, summary)),
        _ => payload.clone(),
    }
}

fn payload_preview(value: &Value) -> Option<String> {
    let mut preview = match value {
        Value::Null => return None,
        Value::String(text) => text.trim().to_string(),
        Value::Object(map) => {
            if let Some(error) = map.get("error").and_then(Value::as_str) {
                format!("error: {}", error.trim())
            } else if let Some(status) = map.get("status").and_then(Value::as_str) {
                status.trim().to_string()
            } else {
                value.to_string()
            }
        }
        _ => value.to_string(),
    };

    if preview.is_empty() {
        return None;
    }

    if preview.chars().count() > DURABLE_PREVIEW_MAX_CHARS {
        preview = preview
            .chars()
            .take(DURABLE_PREVIEW_MAX_CHARS)
            .collect::<String>();
        preview.push_str("...");
    }

    Some(preview)
}
