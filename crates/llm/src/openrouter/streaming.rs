//! OpenRouter Server-Sent Events stream projection into Alan stream chunks.

use anyhow::Result;
use futures::StreamExt;
#[cfg(test)]
use openrouter_rs::types::UnifiedStreamEvent;
use openrouter_rs::types::completion::CompletionsResponse;
use serde_json::Value;
use tokio::sync::mpsc;
#[cfg(test)]
use tracing::debug;

use super::{finish_reason_to_string, usage::convert_usage};
use crate::{StreamChunk, TokenUsage, ToolCallDelta};

#[cfg(test)]
pub(super) async fn consume_openrouter_stream(
    mut stream: openrouter_rs::types::UnifiedStream,
    tx: mpsc::Sender<StreamChunk>,
) {
    let mut emitted_payload = false;
    let mut latest_usage = None;
    let mut latest_response_id = None;
    let mut latest_finish_reason = None;

    while let Some(event) = stream.next().await {
        match event {
            UnifiedStreamEvent::ContentDelta(text) if !text.is_empty() => {
                emitted_payload = true;
                let _ = tx.send(stream_text_chunk(text)).await;
            }
            UnifiedStreamEvent::ContentDelta(_) => {}
            UnifiedStreamEvent::ReasoningDelta(thinking) if !thinking.is_empty() => {
                emitted_payload = true;
                let _ = tx.send(stream_thinking_chunk(thinking)).await;
            }
            UnifiedStreamEvent::ReasoningDelta(_) => {}
            UnifiedStreamEvent::ReasoningDetailsDelta(details) => {
                for detail in details {
                    if let Some(thinking) = detail.content().filter(|value| !value.is_empty()) {
                        emitted_payload = true;
                        let _ = tx.send(stream_thinking_chunk(thinking.to_string())).await;
                    }
                    if let Some(signature) = detail.signature.filter(|value| !value.is_empty()) {
                        emitted_payload = true;
                        let _ = tx.send(stream_thinking_signature_chunk(signature)).await;
                    }
                }
            }
            UnifiedStreamEvent::ToolDelta(value) => {
                if let Some(delta) = tool_delta_from_value(value) {
                    emitted_payload = true;
                    let _ = tx.send(stream_tool_chunk(delta)).await;
                }
            }
            UnifiedStreamEvent::Done {
                id,
                finish_reason,
                usage,
                ..
            } => {
                latest_response_id = id;
                latest_finish_reason = finish_reason;
                latest_usage = usage.map(convert_usage);
                break;
            }
            UnifiedStreamEvent::Error(error) => {
                debug!(?error, "OpenRouter stream failed");
                if emitted_payload {
                    latest_finish_reason = Some("stream_error".to_string());
                    break;
                }
                return;
            }
            UnifiedStreamEvent::Raw { .. } => {}
            _ => {}
        }
    }

    let _ = tx
        .send(terminal_chunk(
            latest_usage,
            latest_response_id,
            latest_finish_reason,
        ))
        .await;
}

pub(super) async fn consume_openrouter_raw_stream(
    response: reqwest::Response,
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()> {
    consume_router_bytes(
        response.bytes_stream().map(|chunk| {
            chunk
                .map(|bytes| bytes.to_vec())
                .map_err(anyhow::Error::from)
        }),
        tx,
    )
    .await
}

/// Running state of one OpenRouter stream between SSE events.
#[derive(Default)]
struct StreamState {
    latest_response_id: Option<String>,
    latest_finish_reason: Option<String>,
    latest_usage: Option<TokenUsage>,
}

pub(super) async fn consume_router_bytes<S>(
    mut stream: S,
    tx: mpsc::Sender<StreamChunk>,
) -> Result<()>
where
    S: futures::Stream<Item = Result<Vec<u8>>> + Unpin,
{
    let mut state = StreamState::default();
    let mut parser = crate::SseEventParser::new();

    let mut completed = false;
    loop {
        let chunk = tokio::select! {
            _ = tx.closed() => return Ok(()),
            chunk = stream.next() => match chunk { Some(chunk) => chunk, None => break },
        };
        for event in parser.push(&chunk?) {
            if event.trim() == "[DONE]" {
                completed = true;
                break;
            }
            let response = serde_json::from_str::<CompletionsResponse>(&event)?;
            emit_openrouter_stream_response(response, &tx, &mut state).await;
        }
        if completed {
            break;
        }
    }

    if !completed {
        for event in parser.finish() {
            if event.trim() == "[DONE]" {
                completed = true;
                break;
            }
            let response = serde_json::from_str::<CompletionsResponse>(&event)?;
            emit_openrouter_stream_response(response, &tx, &mut state).await;
        }
    }
    if tx.is_closed() {
        return Ok(());
    }
    if !completed {
        return Err(crate::failure::StreamClosed.into());
    }

    let _ = tx
        .send(terminal_chunk(
            state.latest_usage,
            state.latest_response_id,
            Some(state.latest_finish_reason.unwrap_or_else(|| "stop".into())),
        ))
        .await;
    Ok(())
}

async fn emit_openrouter_stream_response(
    response: CompletionsResponse,
    tx: &mpsc::Sender<StreamChunk>,
    state: &mut StreamState,
) {
    state.latest_response_id = Some(response.id.clone());
    if let Some(usage) = response.usage {
        state.latest_usage = Some(convert_usage(usage));
    }

    for choice in &response.choices {
        if let Some(content) = choice.content().filter(|value| !value.is_empty()) {
            let _ = tx.send(stream_text_chunk(content.to_string())).await;
        }
        if let Some(reasoning) = choice.reasoning().filter(|value| !value.is_empty()) {
            let _ = tx.send(stream_thinking_chunk(reasoning.to_string())).await;
        }
        if let Some(details) = choice.reasoning_details() {
            for detail in details {
                if let Some(thinking) = detail.content().filter(|value| !value.is_empty()) {
                    let _ = tx.send(stream_thinking_chunk(thinking.to_string())).await;
                }
                if let Some(signature) = detail.signature.as_ref().filter(|value| !value.is_empty())
                {
                    let _ = tx
                        .send(stream_thinking_signature_chunk(signature.clone()))
                        .await;
                }
            }
        }
        if let Some(partials) = choice.partial_tool_calls() {
            for partial in partials {
                if let Ok(value) = serde_json::to_value(partial)
                    && let Some(delta) = tool_delta_from_value(value)
                {
                    let _ = tx.send(stream_tool_chunk(delta)).await;
                }
            }
        }
        if let Some(reason) = choice.finish_reason() {
            state.latest_finish_reason = Some(finish_reason_to_string(reason).to_string());
        }
    }
}

pub(super) fn tool_delta_from_value(value: Value) -> Option<ToolCallDelta> {
    let partial: openrouter_rs::types::PartialToolCall = serde_json::from_value(value).ok()?;
    let function = partial.function;
    Some(ToolCallDelta {
        index: partial.index.unwrap_or(0) as usize,
        id: partial.id,
        name: function.as_ref().and_then(|function| function.name.clone()),
        arguments_delta: function.and_then(|function| function.arguments),
        arguments: None,
    })
}

fn empty_chunk() -> StreamChunk {
    StreamChunk {
        text: None,
        thinking: None,
        thinking_signature: None,
        redacted_thinking: None,
        usage: None,
        provider_response_id: None,
        provider_response_status: None,
        sequence_number: None,
        tool_call_delta: None,
        is_finished: false,
        finish_reason: None,
    }
}

fn terminal_chunk(
    usage: Option<TokenUsage>,
    provider_response_id: Option<String>,
    finish_reason: Option<String>,
) -> StreamChunk {
    StreamChunk {
        usage,
        provider_response_id,
        is_finished: true,
        finish_reason,
        ..empty_chunk()
    }
}

fn stream_text_chunk(text: String) -> StreamChunk {
    StreamChunk {
        text: Some(text),
        ..empty_chunk()
    }
}

fn stream_thinking_chunk(thinking: String) -> StreamChunk {
    StreamChunk {
        thinking: Some(thinking),
        ..empty_chunk()
    }
}

fn stream_thinking_signature_chunk(thinking_signature: String) -> StreamChunk {
    StreamChunk {
        thinking_signature: Some(thinking_signature),
        ..empty_chunk()
    }
}

fn stream_tool_chunk(tool_call_delta: ToolCallDelta) -> StreamChunk {
    StreamChunk {
        tool_call_delta: Some(tool_call_delta),
        ..empty_chunk()
    }
}
