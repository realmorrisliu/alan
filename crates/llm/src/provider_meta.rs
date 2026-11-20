//! Lightweight shared provider constants and metadata parsers.
//!
//! Kept free of heavy HTTP/SDK dependencies so crates depending on `alan-llm`
//! with minimal features can still inspect provider configuration defaults.

use alan_agent_protocol::ReasoningEffort;
use serde_json::Value;

/// Canonical reasoning-effort name to provider wire value; `None` omits the field.
pub type ReasoningEffortMap = std::collections::BTreeMap<String, Option<String>>;

/// Default Magpie router endpoint.
pub const MAGPIE_DEFAULT_BASE_URL: &str = "http://127.0.0.1:3425/v1";

/// Default OpenRouter API base URL.
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Parse a `thinkingLevelMap` object: canonical effort name -> router level.
///
/// String values map onto that wire level; `null`/`false` means "omit the
/// reasoning field" for that effort. Unknown effort names are ignored so a
/// router may advertise extra levels without breaking Alan.
pub fn parse_reasoning_effort_map(value: &Value) -> Option<ReasoningEffortMap> {
    let object = value.as_object()?;
    let map: ReasoningEffortMap = object
        .iter()
        .filter_map(|(key, value)| {
            let effort: ReasoningEffort =
                serde_json::from_value(Value::String(key.trim().to_ascii_lowercase())).ok()?;
            let wire = match value {
                Value::String(level) if !level.trim().is_empty() => Some(level.trim().to_string()),
                Value::Null | Value::Bool(false) => None,
                _ => return None,
            };
            Some((effort.as_str().to_string(), wire))
        })
        .collect();
    (!map.is_empty()).then_some(map)
}
