use alan_agent_protocol::Event;
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use uuid::Uuid;

use crate::llm::LlmClient;

use super::agent_loop::{NormalizedToolCall, RuntimeLoopState};

pub(super) async fn cancel_current_task<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    warn!("Cancelling current task");
    // Clear turn-scoped pending state, but preserve session history so the user can
    // continue the same conversation after an interrupt/cancel.
    state.turn_state.clear();
    state.turn_state.clear_plan_snapshot();
    state.session.has_active_task = false;
    emit(Event::TurnCompleted {
        summary: Some("Task cancelled by user".to_string()),
    })
    .await;
    Ok(())
}

pub(super) async fn emit_task_completed_success<E, F>(emit: &mut E, summary: impl Into<String>)
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let summary = summary.into();
    emit(Event::TurnCompleted {
        summary: Some(summary),
    })
    .await;
}

pub(super) fn normalize_tool_calls(
    tool_calls: Vec<crate::llm::ToolCall>,
) -> Vec<NormalizedToolCall> {
    let fallback_prefix = format!("tool_call_{}", Uuid::new_v4().simple());

    tool_calls
        .into_iter()
        .enumerate()
        .map(|(index, tc)| {
            let id = tc
                .id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{fallback_prefix}_{index}"));

            NormalizedToolCall {
                id,
                name: tc.name,
                arguments: tc.arguments,
            }
        })
        .collect()
}

pub(super) fn detect_provider(llm_client: &LlmClient) -> &'static str {
    if llm_client.is_google_gemini_generate_content() {
        "google_gemini_generate_content"
    } else if llm_client.is_chatgpt() {
        "chatgpt"
    } else if llm_client.is_anthropic_messages() {
        "anthropic_messages"
    } else if llm_client.is_openai_responses() {
        "openai_responses"
    } else if llm_client.is_openai_chat_completions() {
        "openai_chat_completions"
    } else if llm_client.is_openai_chat_completions_compatible() {
        "openai_chat_completions_compatible"
    } else {
        "unknown"
    }
}

pub(super) fn tool_result_preview(value: &serde_json::Value) -> Option<String> {
    const MAX_PREVIEW_CHARS: usize = 160;

    let mut preview = match value {
        serde_json::Value::Null => return None,
        serde_json::Value::String(text) => text.trim().to_string(),
        serde_json::Value::Object(map) => {
            if let Some(error) = map.get("error").and_then(|v| v.as_str()) {
                format!("error: {}", error.trim())
            } else if let Some(status) = map.get("status").and_then(|v| v.as_str()) {
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

    if preview.chars().count() > MAX_PREVIEW_CHARS {
        preview = preview.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        preview.push_str("...");
    }

    Some(preview)
}

pub(super) fn split_text_for_typing(text: &str) -> Vec<String> {
    const TARGET_CHUNK_CHARS: usize = 32;

    if text.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for ch in text.chars() {
        current.push(ch);
        current_len += 1;

        let boundary = ch.is_whitespace() || [',', '.', '!', '?', ';', ':'].contains(&ch);
        if current_len >= TARGET_CHUNK_CHARS && boundary {
            chunks.push(std::mem::take(&mut current));
            current_len = 0;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

pub(super) async fn emit_streaming_chunks<E, F>(emit: &mut E, content: &str)
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let chunks = split_text_for_typing(content);
    for chunk in &chunks {
        emit(Event::TextDelta {
            chunk: chunk.clone(),
            is_final: false,
        })
        .await;
    }
    emit(Event::TextDelta {
        chunk: String::new(),
        is_final: true,
    })
    .await;
}

pub(super) async fn emit_thinking_chunks<E, F>(emit: &mut E, thinking: &str)
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let chunks = split_text_for_typing(thinking);
    for chunk in &chunks {
        emit(Event::ThinkingDelta {
            chunk: chunk.clone(),
            is_final: false,
        })
        .await;
    }
    emit(Event::ThinkingDelta {
        chunk: String::new(),
        is_final: true,
    })
    .await;
}

pub(super) async fn check_turn_cancelled<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<bool>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if !cancel.is_cancelled() {
        return Ok(false);
    }
    if !state.turn_state.is_turn_active() && !state.turn_state.has_pending_interaction() {
        emit(Event::Error {
            message: "No active turn to cancel.".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(false);
    }
    cancel_current_task(state, emit).await?;
    Ok(true)
}
