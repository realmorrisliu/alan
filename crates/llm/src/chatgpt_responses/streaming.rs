use std::collections::HashMap;

use anyhow::{Context, Result};
use futures::StreamExt;
use tracing::{debug, warn};

use crate::openai_chat_completions::{
    OpenAiResponsesResponse, OpenAiResponsesUsage, extract_responses_output_reasoning_signature,
};
use crate::{GenerationResponse, SseEventParser, StreamChunk, TokenUsage, ToolCallDelta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamEventAction {
    Continue,
    Finish,
}

async fn emit_terminal_stream_chunk(
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
    latest_usage: Option<TokenUsage>,
    finish_reason: &str,
    provider_response_id: Option<String>,
    provider_response_status: Option<String>,
) {
    let _ = tx
        .send(StreamChunk {
            text: None,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            usage: latest_usage,
            provider_response_id,
            provider_response_status,
            sequence_number: None,
            tool_call_delta: None,
            is_finished: true,
            finish_reason: Some(finish_reason.to_string()),
        })
        .await;
}

pub(super) async fn consume_openai_responses_stream(
    response: reqwest::Response,
    tx: tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<()> {
    let mut stream = response.bytes_stream();
    let mut parser = SseEventParser::new();
    let mut latest_usage: Option<TokenUsage> = None;
    let mut emitted_payload = false;
    let mut saw_tool_calls = false;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("Failed to read ChatGPT Responses stream chunk")?;
        for data in parser.push(&chunk) {
            if handle_stream_event(
                &tx,
                &data,
                &mut latest_usage,
                &mut emitted_payload,
                &mut saw_tool_calls,
            )
            .await?
                == StreamEventAction::Finish
            {
                return Ok(());
            }
        }
    }

    for data in parser.finish() {
        if handle_stream_event(
            &tx,
            &data,
            &mut latest_usage,
            &mut emitted_payload,
            &mut saw_tool_calls,
        )
        .await?
            == StreamEventAction::Finish
        {
            return Ok(());
        }
    }

    if emitted_payload {
        emit_terminal_stream_chunk(
            &tx,
            latest_usage,
            responses_finish_reason(saw_tool_calls),
            None,
            None,
        )
        .await;
    }

    Ok(())
}

async fn handle_stream_event(
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
    data: &str,
    latest_usage: &mut Option<TokenUsage>,
    emitted_payload: &mut bool,
    saw_tool_calls: &mut bool,
) -> Result<StreamEventAction> {
    if data == "[DONE]" {
        if *emitted_payload {
            emit_terminal_stream_chunk(
                tx,
                *latest_usage,
                responses_finish_reason(*saw_tool_calls),
                None,
                None,
            )
            .await;
        }
        return Ok(StreamEventAction::Finish);
    }

    let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
        debug!(data, "Failed to parse ChatGPT Responses stream event");
        return Ok(StreamEventAction::Continue);
    };

    let Some(event_type) = event.get("type").and_then(serde_json::Value::as_str) else {
        return Ok(StreamEventAction::Continue);
    };

    match event_type {
        "response.output_text.delta" | "response.refusal.delta" => {
            if let Some(text) = responses_stream_text_delta(&event) {
                *emitted_payload = true;
                if tx
                    .send(StreamChunk {
                        text: Some(text.to_string()),
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: event
                            .get("sequence_number")
                            .and_then(serde_json::Value::as_u64),
                        tool_call_delta: None,
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await
                    .is_err()
                {
                    return Ok(StreamEventAction::Finish);
                }
            }
        }
        "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
            if let Some(thinking) = event
                .get("delta")
                .and_then(serde_json::Value::as_str)
                .filter(|value| is_non_empty(value))
            {
                *emitted_payload = true;
                if tx
                    .send(StreamChunk {
                        text: None,
                        thinking: Some(thinking.to_string()),
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: event
                            .get("sequence_number")
                            .and_then(serde_json::Value::as_u64),
                        tool_call_delta: None,
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await
                    .is_err()
                {
                    return Ok(StreamEventAction::Finish);
                }
            }
        }
        "response.function_call_arguments.delta" => {
            let delta = event
                .get("delta")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if !delta.is_empty() {
                *emitted_payload = true;
                if tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: event
                            .get("sequence_number")
                            .and_then(serde_json::Value::as_u64),
                        tool_call_delta: Some(ToolCallDelta {
                            index: responses_stream_index(&event),
                            id: responses_stream_tool_id(event.get("item"), &event),
                            name: responses_stream_tool_name(event.get("item"), &event),
                            arguments_delta: Some(delta.to_string()),
                            arguments: None,
                        }),
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await
                    .is_err()
                {
                    return Ok(StreamEventAction::Finish);
                }
            }
        }
        "response.output_item.done" => {
            let Some(item) = event.get("item") else {
                return Ok(StreamEventAction::Continue);
            };
            if item.get("type").and_then(serde_json::Value::as_str) != Some("function_call") {
                return Ok(StreamEventAction::Continue);
            }

            let arguments = item
                .get("arguments")
                .and_then(serde_json::Value::as_str)
                .filter(|value| is_non_empty(value));
            let name = responses_stream_tool_name(Some(item), &event);

            if let (Some(arguments), Some(name)) = (arguments, name) {
                *emitted_payload = true;
                *saw_tool_calls = true;
                if tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: event
                            .get("sequence_number")
                            .and_then(serde_json::Value::as_u64),
                        tool_call_delta: Some(ToolCallDelta {
                            index: responses_stream_index(&event),
                            id: responses_stream_tool_id(Some(item), &event),
                            name: Some(name),
                            arguments_delta: None,
                            arguments: Some(arguments.to_string()),
                        }),
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await
                    .is_err()
                {
                    return Ok(StreamEventAction::Finish);
                }
            }
        }
        "response.completed" => {
            let mut completed_response_id: Option<String> = None;
            let mut completed_response_status: Option<String> = None;
            if let Some(response) = event.get("response").cloned() {
                match serde_json::from_value::<OpenAiResponsesResponse>(response) {
                    Ok(parsed) => {
                        completed_response_id = parsed.id.clone();
                        completed_response_status = parsed.status.clone();
                        *latest_usage = parsed.usage.map(convert_openai_responses_usage);
                        if !*saw_tool_calls {
                            *saw_tool_calls = responses_output_contains_tool_call(&parsed.output);
                        }
                        if let Some(signature) =
                            extract_responses_output_reasoning_signature(&parsed.output)
                            && tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: Some(signature),
                                    redacted_thinking: None,
                                    usage: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                    sequence_number: event
                                        .get("sequence_number")
                                        .and_then(serde_json::Value::as_u64),
                                    tool_call_delta: None,
                                    is_finished: false,
                                    finish_reason: None,
                                })
                                .await
                                .is_err()
                        {
                            return Ok(StreamEventAction::Finish);
                        }
                    }
                    Err(error) => {
                        debug!(?error, "Failed to parse ChatGPT response.completed payload");
                    }
                }
            }

            emit_terminal_stream_chunk(
                tx,
                *latest_usage,
                responses_finish_reason(*saw_tool_calls),
                completed_response_id,
                completed_response_status,
            )
            .await;
            return Ok(StreamEventAction::Finish);
        }
        "response.incomplete" | "response.cancelled" => {
            let (response_id, response_status) = event
                .get("response")
                .cloned()
                .and_then(|response| {
                    serde_json::from_value::<OpenAiResponsesResponse>(response).ok()
                })
                .map(|response| (response.id, response.status))
                .unwrap_or((None, None));

            if *emitted_payload {
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: *latest_usage,
                        provider_response_id: response_id,
                        provider_response_status: response_status,
                        sequence_number: event
                            .get("sequence_number")
                            .and_then(serde_json::Value::as_u64),
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some("stream_error".to_string()),
                    })
                    .await;
            }
            return Ok(StreamEventAction::Finish);
        }
        "response.failed" | "error" => {
            if *emitted_payload {
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: *latest_usage,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: event
                            .get("sequence_number")
                            .and_then(serde_json::Value::as_u64),
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some("stream_error".to_string()),
                    })
                    .await;
            }
            return Ok(StreamEventAction::Finish);
        }
        _ => {}
    }

    Ok(StreamEventAction::Continue)
}

pub(super) async fn collect_streamed_generation(
    mut rx: tokio::sync::mpsc::Receiver<StreamChunk>,
) -> Result<GenerationResponse> {
    let mut content = String::new();
    let mut thinking = String::new();
    let mut thinking_signature: Option<String> = None;
    let mut redacted_thinking = Vec::new();
    let mut usage = None;
    let mut finish_reason = None;
    let mut provider_response_id = None;
    let mut provider_response_status = None;
    let mut tool_call_buffers: HashMap<usize, StreamedToolCallBuffer> = HashMap::new();
    let mut saw_terminal_chunk = false;

    while let Some(chunk) = rx.recv().await {
        if let Some(delta) = chunk.text
            && !delta.is_empty()
        {
            content.push_str(&delta);
        }
        if let Some(delta) = chunk.thinking
            && !delta.is_empty()
        {
            thinking.push_str(&delta);
        }
        if let Some(signature) = chunk.thinking_signature
            && !signature.is_empty()
        {
            match &mut thinking_signature {
                Some(existing) => existing.push_str(&signature),
                None => thinking_signature = Some(signature),
            }
        }
        if let Some(redacted) = chunk.redacted_thinking
            && !redacted.is_empty()
        {
            redacted_thinking.push(redacted);
        }
        if let Some(usage_update) = chunk.usage {
            usage = Some(usage_update);
        }
        if let Some(response_id) = chunk.provider_response_id
            && !response_id.is_empty()
        {
            provider_response_id = Some(response_id);
        }
        if let Some(status) = chunk.provider_response_status
            && !status.is_empty()
        {
            provider_response_status = Some(status);
        }
        if let Some(delta) = chunk.tool_call_delta {
            let entry = tool_call_buffers.entry(delta.index).or_default();
            if let Some(id) = delta.id {
                entry.id = Some(id);
            }
            if let Some(name) = delta.name {
                entry.name = Some(name);
            }
            if let Some(arguments_delta) = delta.arguments_delta {
                entry.arguments_delta.push_str(&arguments_delta);
            }
            if let Some(arguments) = delta.arguments {
                entry.final_arguments = Some(arguments);
            }
        }
        if chunk.is_finished {
            saw_terminal_chunk = true;
            finish_reason = chunk.finish_reason;
            break;
        }
    }

    if !saw_terminal_chunk {
        anyhow::bail!("ChatGPT Responses stream ended before a terminal chunk");
    }

    let (tool_calls, warnings) = assemble_streamed_tool_calls(tool_call_buffers);

    Ok(GenerationResponse {
        content,
        thinking: if thinking.is_empty() {
            None
        } else {
            Some(thinking)
        },
        thinking_signature,
        redacted_thinking,
        tool_calls,
        usage,
        finish_reason,
        provider_response_id,
        provider_response_status,
        warnings,
    })
}

fn convert_openai_responses_usage(usage: OpenAiResponsesUsage) -> TokenUsage {
    TokenUsage {
        prompt_tokens: usage.input_tokens,
        cached_prompt_tokens: usage
            .input_tokens_details
            .and_then(|details| details.cached_tokens),
        completion_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        reasoning_tokens: usage
            .output_tokens_details
            .and_then(|details| details.reasoning_tokens),
    }
}

fn responses_finish_reason(saw_tool_calls: bool) -> &'static str {
    if saw_tool_calls { "tool_calls" } else { "stop" }
}

fn responses_output_contains_tool_call(output: &[serde_json::Value]) -> bool {
    output
        .iter()
        .any(|item| item.get("type").and_then(serde_json::Value::as_str) == Some("function_call"))
}

fn responses_stream_index(event: &serde_json::Value) -> usize {
    event
        .get("output_index")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default() as usize
}

fn responses_stream_tool_id(
    item: Option<&serde_json::Value>,
    event: &serde_json::Value,
) -> Option<String> {
    item.and_then(|item| item.get("call_id"))
        .or_else(|| event.get("call_id"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn responses_stream_tool_name(
    item: Option<&serde_json::Value>,
    event: &serde_json::Value,
) -> Option<String> {
    item.and_then(|item| item.get("name"))
        .or_else(|| event.get("name"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

// Streamed text can arrive as standalone spaces/newlines that preserve formatting.
fn responses_stream_text_delta(event: &serde_json::Value) -> Option<&str> {
    event
        .get("delta")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
}

fn is_non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

#[derive(Default)]
struct StreamedToolCallBuffer {
    id: Option<String>,
    name: Option<String>,
    arguments_delta: String,
    final_arguments: Option<String>,
}

fn assemble_streamed_tool_calls(
    mut tool_call_buffers: HashMap<usize, StreamedToolCallBuffer>,
) -> (Vec<crate::ToolCall>, Vec<String>) {
    let mut tool_calls = Vec::new();
    let mut warnings = Vec::new();
    let mut indices: Vec<usize> = tool_call_buffers.keys().copied().collect();
    indices.sort();

    for index in indices {
        let Some(StreamedToolCallBuffer {
            id,
            name: Some(name),
            arguments_delta,
            final_arguments,
        }) = tool_call_buffers.remove(&index)
        else {
            continue;
        };

        let arguments_json = final_arguments.unwrap_or(arguments_delta);
        match serde_json::from_str(&arguments_json) {
            Ok(arguments) => tool_calls.push(crate::ToolCall {
                id,
                name,
                arguments,
            }),
            Err(error) => {
                warn!(
                    tool_name = %name,
                    error = %error,
                    "Dropping malformed streamed ChatGPT tool call arguments"
                );
                warnings.push(format!(
                    "Dropped malformed streamed ChatGPT tool call `{name}` arguments."
                ));
            }
        }
    }

    (tool_calls, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stream_finish_flushes_trailing_completed_event() {
        let mut parser = SseEventParser::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(4);
        let mut latest_usage = None;
        let mut emitted_payload = true;
        let mut saw_tool_calls = false;

        let completed_event = r#"data: {"type":"response.completed","response":{"id":"resp_123","output":[],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}}"#;
        assert!(parser.push(completed_event.as_bytes()).is_empty());

        for data in parser.finish() {
            let action = handle_stream_event(
                &tx,
                &data,
                &mut latest_usage,
                &mut emitted_payload,
                &mut saw_tool_calls,
            )
            .await
            .expect("event");
            assert_eq!(action, StreamEventAction::Finish);
        }

        let final_chunk = rx.recv().await.expect("final chunk");
        assert!(final_chunk.is_finished);
        assert_eq!(final_chunk.finish_reason.as_deref(), Some("stop"));
        assert_eq!(final_chunk.usage.map(|usage| usage.total_tokens), Some(3));
    }

    #[tokio::test]
    async fn emit_terminal_stream_chunk_marks_stream_finished() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(4);
        emit_terminal_stream_chunk(
            &tx,
            Some(TokenUsage {
                prompt_tokens: 1,
                cached_prompt_tokens: None,
                completion_tokens: 2,
                total_tokens: 3,
                reasoning_tokens: None,
            }),
            responses_finish_reason(false),
            None,
            None,
        )
        .await;
        let terminal = rx.recv().await.expect("terminal chunk");
        assert!(terminal.is_finished);
        assert_eq!(terminal.finish_reason.as_deref(), Some("stop"));
        assert_eq!(terminal.usage.map(|usage| usage.total_tokens), Some(3));
    }

    #[tokio::test]
    async fn handle_stream_event_preserves_whitespace_only_output_text_delta() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(4);
        let mut latest_usage = None;
        let mut emitted_payload = false;
        let mut saw_tool_calls = false;

        let action = handle_stream_event(
            &tx,
            r#"{"type":"response.output_text.delta","delta":" ","sequence_number":0}"#,
            &mut latest_usage,
            &mut emitted_payload,
            &mut saw_tool_calls,
        )
        .await
        .expect("event");

        assert_eq!(action, StreamEventAction::Continue);
        assert!(emitted_payload);
        let chunk = rx.recv().await.expect("text chunk");
        assert_eq!(chunk.text.as_deref(), Some(" "));
        assert!(!chunk.is_finished);
    }
}
