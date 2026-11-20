//! JSON Schema compilation and structured output validation for `response_schema`.

use alan_agent_protocol::Event;
use anyhow::Result;
use serde_json::Value;
use tracing::warn;

use super::{accepted_output, live_stream::LiveTurnStream};

/// Validate that a caller-supplied `response_schema` is a well-formed JSON Schema.
pub fn validate_response_schema(schema: &Value) -> Result<()> {
    jsonschema::validator_for(schema)
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("invalid response_schema: {err}"))
}

/// Validate one final assistant output against an active `response_schema`.
pub(super) fn validate_output_against_schema(
    schema: &Value,
    content: &str,
) -> std::result::Result<(), String> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|err| format!("invalid response_schema: {err}"))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("assistant output was empty when a JSON response_schema was required".into());
    }
    let parsed: Value = serde_json::from_str(trimmed).map_err(|err| {
        format!("assistant output is not valid JSON matching response_schema: {err}")
    })?;
    if let Err(err) = validator.validate(&parsed) {
        return Err(format!(
            "assistant output failed response_schema validation: {err}"
        ));
    }
    Ok(())
}

/// Enforce `response_schema` on final assistant text, recovering once before visible output.
pub(super) async fn enforce_or_recover<E, F>(
    active_schema: Option<&Value>,
    has_tool_calls: bool,
    content: &str,
    live: &LiveTurnStream,
    pending_guardrail_instruction: &mut Option<String>,
    schema_retried: &mut bool,
    emit: &mut E,
) -> Result<bool>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if has_tool_calls {
        return Ok(false);
    }
    let Some(schema) = active_schema else {
        return Ok(false);
    };
    let Err(reason) = validate_output_against_schema(schema, content) else {
        return Ok(false);
    };
    if !live.text_visible() && !*schema_retried {
        *schema_retried = true;
        warn!(reason = %reason, "Structured output failed response_schema; retrying once");
        accepted_output::abandon(live, emit).await;
        *pending_guardrail_instruction = Some(format!(
            "Your previous response did not satisfy response_schema ({reason}). Reply with ONLY valid JSON matching the required schema."
        ));
        return Ok(true);
    }
    emit(Event::Error {
        message: reason.clone(),
        recoverable: false,
    })
    .await;
    Err(
        anyhow::Error::new(std::io::Error::new(std::io::ErrorKind::InvalidData, reason))
            .context("structured response_schema validation failed"),
    )
}
