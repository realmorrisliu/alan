use crate::approval::{
    RUNTIME_CONFIRMATION_CONTROL_SOURCE, RUNTIME_CONFIRMATION_CONTROL_VERSION,
    is_runtime_confirmation_checkpoint_type, runtime_confirmation_checkpoint_prefix,
    runtime_confirmation_control_kind,
};
use crate::tape::{ContentPart, Message};

/// Structured outcome for an in-memory rollback request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RollbackOutcome {
    /// Number of logical user turns actually removed.
    pub(crate) removed_turns: u32,
    /// Number of tape messages removed by the rollback.
    pub(crate) removed_messages: usize,
}

/// Server-managed continuation state for Responses-compatible providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResponsesContinuationState {
    pub(crate) provider: String,
    pub(crate) last_response_id: String,
    pub(crate) boundary_message_count: usize,
    pub(crate) reference_context_revision: u64,
}

pub(super) const RESPONSES_CONTINUATION_EVENT_TYPE: &str = "responses_continuation";

fn runtime_confirmation_control_checkpoint(payload: &serde_json::Value) -> Option<(&str, &str)> {
    let checkpoint_id = payload
        .get("checkpoint_id")
        .and_then(serde_json::Value::as_str)?;
    let checkpoint_type = payload
        .get("checkpoint_type")
        .and_then(serde_json::Value::as_str)?;
    let choice = payload.get("choice").and_then(serde_json::Value::as_str)?;

    if !is_runtime_confirmation_checkpoint_type(checkpoint_type) {
        return None;
    }
    if !matches!(choice, "approve" | "reject") {
        return None;
    }
    let prefix = runtime_confirmation_checkpoint_prefix(checkpoint_type)?;
    if !checkpoint_id.starts_with(prefix) {
        return None;
    }

    Some((checkpoint_id, checkpoint_type))
}

fn has_runtime_confirmation_control_kind_and_version(
    payload: &serde_json::Value,
    checkpoint_type: &str,
) -> bool {
    let marker = payload.get("__alan_internal_control");
    let marker_kind = marker
        .and_then(|value| value.get("kind"))
        .and_then(serde_json::Value::as_str);
    let marker_version = marker
        .and_then(|value| value.get("version"))
        .and_then(serde_json::Value::as_u64);

    marker_kind == runtime_confirmation_control_kind(checkpoint_type)
        && marker_version == Some(RUNTIME_CONFIRMATION_CONTROL_VERSION)
}

fn runtime_confirmation_control_source(payload: &serde_json::Value) -> Option<&str> {
    payload
        .get("__alan_internal_control")
        .and_then(|marker| marker.get("source"))
        .and_then(serde_json::Value::as_str)
}

fn is_runtime_confirmation_control_payload(payload: &serde_json::Value) -> bool {
    let Some((_, checkpoint_type)) = runtime_confirmation_control_checkpoint(payload) else {
        return false;
    };

    has_runtime_confirmation_control_kind_and_version(payload, checkpoint_type)
        && runtime_confirmation_control_source(payload) == Some(RUNTIME_CONFIRMATION_CONTROL_SOURCE)
}

fn is_runtime_confirmation_control_parts(parts: &[ContentPart]) -> bool {
    parts.iter().any(|part| {
        matches!(
            part,
            ContentPart::Structured { data }
                if is_runtime_confirmation_control_payload(data)
        )
    })
}

pub(super) fn is_runtime_confirmation_control_message(message: &Message) -> bool {
    match message {
        Message::User { parts } => is_runtime_confirmation_control_parts(parts),
        _ => false,
    }
}
