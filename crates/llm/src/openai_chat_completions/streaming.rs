//! Streaming transport and event projection for OpenAI-compatible APIs.

use anyhow::{Context, Result};
use futures::StreamExt;
use std::collections::HashMap;
use tracing::{debug, instrument};

use crate::{GenerationRequest, SseEventParser, StreamChunk, TokenUsage, ToolCallDelta};

use super::{
    OpenAiChatCompletionsChunk, OpenAiChatCompletionsChunkChoice, OpenAiChatCompletionsClient,
    OpenAiChatCompletionsRequest, OpenAiResponsesRequest, OpenAiResponsesResponse,
    build_chat_completions_request_for_model, convert_openai_responses_usage, convert_usage,
    extract_reasoning_fields, extract_responses_output_reasoning_signature, is_non_empty,
    responses_finish_reason,
};

impl OpenAiChatCompletionsClient {
    /// Chat completion with streaming (SSE)
    #[instrument(skip(self, request, tx))]
    pub async fn stream_openai_chat_completions(
        &self,
        mut request: OpenAiChatCompletionsRequest,
        tx: tokio::sync::mpsc::Sender<OpenAiChatCompletionsChunk>,
    ) -> Result<()> {
        let url = format!("{}/chat/completions", self.base_url);

        // Use the model from the client if not set in the request
        if request.model.is_empty() {
            request.model = self.model.clone();
        }

        // Ensure stream is set to true
        request.stream = Some(true);

        debug!(url = %url, model = %request.model, "Sending streaming chat completion request");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to OpenAI API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI streaming API error ({}): {}", status, error_text);
        }

        // Process SSE stream with event-boundary parsing.
        let mut stream = response.bytes_stream();
        let mut parser = SseEventParser::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Failed to read stream chunk")?;
            for data in parser.push(&chunk) {
                if data == "[DONE]" {
                    debug!("Stream completed");
                    return Ok(());
                }

                match serde_json::from_str::<OpenAiChatCompletionsChunk>(&data) {
                    Ok(chunk) => {
                        if tx.send(chunk).await.is_err() {
                            debug!("Receiver dropped, stopping stream");
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        debug!(?e, data, "Failed to parse stream chunk");
                    }
                }
            }
        }

        for data in parser.finish() {
            if data == "[DONE]" {
                debug!("Stream completed");
                return Ok(());
            }

            match serde_json::from_str::<OpenAiChatCompletionsChunk>(&data) {
                Ok(chunk) => {
                    if tx.send(chunk).await.is_err() {
                        debug!("Receiver dropped, stopping stream");
                        return Ok(());
                    }
                }
                Err(e) => {
                    debug!(?e, data, "Failed to parse stream chunk");
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self, request, tx))]
    pub async fn stream_openai_responses(
        &self,
        mut request: OpenAiResponsesRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));

        if request.model.is_empty() {
            request.model = self.model.clone();
        }
        request.stream = Some(true);

        debug!(url = %url, model = %request.model, "Sending streaming responses request");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to OpenAI Responses API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "OpenAI Responses streaming API error ({}): {}",
                status,
                error_text
            );
        }

        self.consume_openai_responses_stream_response(response, tx)
            .await
    }

    #[instrument(skip(self, tx))]
    pub async fn retrieve_openai_response_stream(
        &self,
        response_id: &str,
        starting_after: Option<u64>,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        let mut url = format!(
            "{}/responses/{}",
            self.base_url.trim_end_matches('/'),
            response_id
        );
        url.push_str("?stream=true");
        if let Some(starting_after) = starting_after {
            url.push_str(&format!("&starting_after={starting_after}"));
        }
        debug!(
            url = %url,
            response_id,
            starting_after,
            "Retrieving Responses API stream"
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to retrieve OpenAI Responses API stream")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "OpenAI Responses streaming API error ({}): {}",
                status,
                error_text
            );
        }

        self.consume_openai_responses_stream_response(response, tx)
            .await
    }

    async fn consume_openai_responses_stream_response(
        &self,
        response: reqwest::Response,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        let mut stream = response.bytes_stream();
        let mut parser = SseEventParser::new();
        let mut latest_usage: Option<TokenUsage> = None;
        let mut emitted_payload = false;
        let mut saw_tool_calls = false;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Failed to read Responses stream chunk")?;
            for data in parser.push(&chunk) {
                if data == "[DONE]" {
                    if emitted_payload {
                        let _ = tx
                            .send(StreamChunk {
                                text: None,
                                thinking: None,
                                thinking_signature: None,
                                redacted_thinking: None,
                                usage: latest_usage,
                                provider_response_id: None,
                                provider_response_status: None,
                                sequence_number: None,
                                tool_call_delta: None,
                                is_finished: true,
                                finish_reason: Some(
                                    responses_finish_reason(saw_tool_calls).to_string(),
                                ),
                            })
                            .await;
                    }
                    return Ok(());
                }

                let Ok(event) = serde_json::from_str::<serde_json::Value>(&data) else {
                    debug!(data, "Failed to parse Responses stream event");
                    continue;
                };

                let Some(event_type) = event.get("type").and_then(serde_json::Value::as_str) else {
                    continue;
                };

                match event_type {
                    "response.output_text.delta" | "response.refusal.delta" => {
                        if let Some(text) = responses_stream_text_delta(&event) {
                            emitted_payload = true;
                            if tx
                                .send(StreamChunk {
                                    text: Some(text.to_string()),
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                    sequence_number: responses_stream_sequence_number(&event),
                                    tool_call_delta: None,
                                    is_finished: false,
                                    finish_reason: None,
                                })
                                .await
                                .is_err()
                            {
                                debug!("Receiver dropped, stopping Responses stream");
                                return Ok(());
                            }
                        }
                    }
                    "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
                        if let Some(thinking) = event
                            .get("delta")
                            .and_then(serde_json::Value::as_str)
                            .filter(|value| is_non_empty(value))
                        {
                            emitted_payload = true;
                            if tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: Some(thinking.to_string()),
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                    sequence_number: responses_stream_sequence_number(&event),
                                    tool_call_delta: None,
                                    is_finished: false,
                                    finish_reason: None,
                                })
                                .await
                                .is_err()
                            {
                                debug!("Receiver dropped, stopping Responses stream");
                                return Ok(());
                            }
                        }
                    }
                    "response.function_call_arguments.delta" => {
                        let delta = event
                            .get("delta")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default();
                        if !delta.is_empty() {
                            emitted_payload = true;
                            if tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                    sequence_number: responses_stream_sequence_number(&event),
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
                                debug!("Receiver dropped, stopping Responses stream");
                                return Ok(());
                            }
                        }
                    }
                    "response.output_item.done" => {
                        let Some(item) = event.get("item") else {
                            continue;
                        };
                        if item.get("type").and_then(serde_json::Value::as_str)
                            != Some("function_call")
                        {
                            continue;
                        }

                        let arguments = item
                            .get("arguments")
                            .and_then(serde_json::Value::as_str)
                            .filter(|value| is_non_empty(value));
                        let name = responses_stream_tool_name(Some(item), &event);

                        if let (Some(arguments), Some(name)) = (arguments, name) {
                            emitted_payload = true;
                            saw_tool_calls = true;
                            if tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                    sequence_number: responses_stream_sequence_number(&event),
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
                                debug!("Receiver dropped, stopping Responses stream");
                                return Ok(());
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
                                    latest_usage = parsed.usage.map(convert_openai_responses_usage);
                                    if !saw_tool_calls {
                                        saw_tool_calls =
                                            responses_output_contains_tool_call(&parsed.output);
                                    }
                                    if let Some(signature) =
                                        extract_responses_output_reasoning_signature(&parsed.output)
                                    {
                                        let _ = tx
                                            .send(StreamChunk {
                                                text: None,
                                                thinking: None,
                                                thinking_signature: Some(signature),
                                                redacted_thinking: None,
                                                usage: None,
                                                provider_response_id: None,
                                                provider_response_status: None,
                                                sequence_number: responses_stream_sequence_number(
                                                    &event,
                                                ),
                                                tool_call_delta: None,
                                                is_finished: false,
                                                finish_reason: None,
                                            })
                                            .await;
                                    }
                                }
                                Err(error) => {
                                    debug!(?error, "Failed to parse response.completed payload");
                                }
                            }
                        }

                        let _ = tx
                            .send(StreamChunk {
                                text: None,
                                thinking: None,
                                thinking_signature: None,
                                redacted_thinking: None,
                                usage: latest_usage,
                                provider_response_id: completed_response_id,
                                provider_response_status: completed_response_status,
                                sequence_number: responses_stream_sequence_number(&event),
                                tool_call_delta: None,
                                is_finished: true,
                                finish_reason: Some(
                                    responses_finish_reason(saw_tool_calls).to_string(),
                                ),
                            })
                            .await;
                        return Ok(());
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

                        if emitted_payload {
                            let _ = tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: latest_usage,
                                    provider_response_id: response_id,
                                    provider_response_status: response_status,
                                    sequence_number: responses_stream_sequence_number(&event),
                                    tool_call_delta: None,
                                    is_finished: true,
                                    finish_reason: Some("stream_error".to_string()),
                                })
                                .await;
                        }
                        return Ok(());
                    }
                    "response.failed" | "error" => {
                        if emitted_payload {
                            let _ = tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: latest_usage,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                    sequence_number: responses_stream_sequence_number(&event),
                                    tool_call_delta: None,
                                    is_finished: true,
                                    finish_reason: Some("stream_error".to_string()),
                                })
                                .await;
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        for data in parser.finish() {
            if data == "[DONE]" {
                if emitted_payload {
                    let _ = tx
                        .send(StreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: latest_usage,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: true,
                            finish_reason: Some(
                                responses_finish_reason(saw_tool_calls).to_string(),
                            ),
                        })
                        .await;
                }
                return Ok(());
            }
        }

        if emitted_payload {
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: latest_usage,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some(responses_finish_reason(saw_tool_calls).to_string()),
                })
                .await;
        }

        Ok(())
    }

    pub(super) async fn generate_stream_via_openai_chat_completions(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        let chat_request = build_chat_completions_request_for_model(
            self.model.clone(),
            self.instruction_role_name(),
            request,
            true,
        )?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel(100);
        let (stream_status_tx, stream_status_rx) =
            tokio::sync::oneshot::channel::<Option<String>>();

        let client = self.clone_with_same_config();
        tokio::spawn(async move {
            let outcome = match client
                .stream_openai_chat_completions(chat_request, chunk_tx)
                .await
            {
                Ok(()) => None,
                Err(e) => {
                    debug!(error = ?e, "OpenAI Chat Completions API stream failed");
                    Some(e.to_string())
                }
            };
            let _ = stream_status_tx.send(outcome);
        });

        tokio::spawn(async move {
            let mut latest_finish_reason: Option<String> = None;
            let mut latest_usage: Option<TokenUsage> = None;
            let mut latest_response_id: Option<String> = None;
            let mut emitted_payload = false;
            let mut selected_choice_index: Option<i32> = None;
            let mut tool_index_map: HashMap<(i32, i32), usize> = HashMap::new();
            let mut next_tool_index: usize = 0;
            while let Some(chunk) = chunk_rx.recv().await {
                latest_response_id = Some(chunk.id.clone());
                if let Some(usage) = chunk.usage {
                    latest_usage = Some(convert_usage(usage));
                }

                selected_choice_index = select_stream_choice_index(
                    selected_choice_index,
                    emitted_payload,
                    &chunk.choices,
                );
                let Some(active_choice_index) = selected_choice_index else {
                    continue;
                };

                for choice in &chunk.choices {
                    if choice.index != active_choice_index {
                        continue;
                    }
                    let delta = &choice.delta;

                    if let Some(ref reason) = choice.finish_reason {
                        latest_finish_reason = Some(reason.clone());
                    }

                    let (thinking, thinking_signature) = extract_reasoning_fields(
                        delta.reasoning_content.as_deref(),
                        delta.reasoning.as_ref(),
                    );

                    if let Some(reasoning_content) = thinking {
                        emitted_payload = true;
                        let _ = tx
                            .send(StreamChunk {
                                text: None,
                                thinking: Some(reasoning_content),
                                thinking_signature: None,
                                redacted_thinking: None,
                                usage: None,
                                sequence_number: None,
                                tool_call_delta: None,
                                is_finished: false,
                                finish_reason: None,
                                provider_response_id: None,
                                provider_response_status: None,
                            })
                            .await;
                    }
                    if let Some(signature) = thinking_signature {
                        emitted_payload = true;
                        let _ = tx
                            .send(StreamChunk {
                                text: None,
                                thinking: None,
                                thinking_signature: Some(signature),
                                redacted_thinking: None,
                                usage: None,
                                sequence_number: None,
                                tool_call_delta: None,
                                is_finished: false,
                                finish_reason: None,
                                provider_response_id: None,
                                provider_response_status: None,
                            })
                            .await;
                    }

                    if let Some(content) = &delta.content {
                        emitted_payload = true;
                        let _ = tx
                            .send(StreamChunk {
                                text: Some(content.clone()),
                                thinking: None,
                                thinking_signature: None,
                                redacted_thinking: None,
                                usage: None,
                                sequence_number: None,
                                tool_call_delta: None,
                                is_finished: false,
                                finish_reason: None,
                                provider_response_id: None,
                                provider_response_status: None,
                            })
                            .await;
                    }

                    if let Some(tool_calls) = &delta.tool_calls {
                        for tool_call in tool_calls {
                            emitted_payload = true;
                            let stream_tool_index = allocate_stream_tool_index(
                                &mut tool_index_map,
                                &mut next_tool_index,
                                choice.index,
                                tool_call.index,
                            );
                            let tool_delta = ToolCallDelta {
                                index: stream_tool_index,
                                id: tool_call.id.clone(),
                                name: tool_call.function.as_ref().and_then(|f| f.name.clone()),
                                arguments_delta: tool_call
                                    .function
                                    .as_ref()
                                    .and_then(|f| f.arguments.clone()),
                                arguments: None,
                            };

                            let _ = tx
                                .send(StreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    sequence_number: None,
                                    tool_call_delta: Some(tool_delta),
                                    is_finished: false,
                                    finish_reason: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                })
                                .await;
                        }
                    }
                }
            }

            let upstream_error = stream_status_rx.await.ok().flatten();
            if upstream_error.is_some() && !emitted_payload {
                return;
            }

            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: latest_usage,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: latest_finish_reason
                        .or_else(|| upstream_error.map(|_| "stream_error".to_string())),
                    provider_response_id: latest_response_id,
                    provider_response_status: None,
                })
                .await;
        });

        Ok(rx)
    }
}

fn responses_stream_index(event: &serde_json::Value) -> usize {
    event
        .get("output_index")
        .or_else(|| event.get("item_index"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default() as usize
}

fn responses_stream_tool_id(
    item: Option<&serde_json::Value>,
    event: &serde_json::Value,
) -> Option<String> {
    item.and_then(|value| {
        value
            .get("call_id")
            .or_else(|| value.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    })
    .or_else(|| {
        event
            .get("call_id")
            .or_else(|| event.get("item_id"))
            .or_else(|| event.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    })
}

fn responses_stream_tool_name(
    item: Option<&serde_json::Value>,
    event: &serde_json::Value,
) -> Option<String> {
    item.and_then(|value| {
        value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|value| is_non_empty(value))
            .map(str::to_owned)
    })
    .or_else(|| {
        event
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|value| is_non_empty(value))
            .map(str::to_owned)
    })
}

fn responses_output_contains_tool_call(output: &[serde_json::Value]) -> bool {
    output
        .iter()
        .any(|item| item.get("type").and_then(serde_json::Value::as_str) == Some("function_call"))
}

fn responses_stream_sequence_number(event: &serde_json::Value) -> Option<u64> {
    event
        .get("sequence_number")
        .and_then(serde_json::Value::as_u64)
}

// Streamed text can arrive as standalone spaces/newlines that preserve formatting.
pub(super) fn responses_stream_text_delta(event: &serde_json::Value) -> Option<&str> {
    event
        .get("delta")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
}

pub(super) fn allocate_stream_tool_index(
    tool_index_map: &mut HashMap<(i32, i32), usize>,
    next_tool_index: &mut usize,
    choice_index: i32,
    tool_call_index: i32,
) -> usize {
    *tool_index_map
        .entry((choice_index, tool_call_index))
        .or_insert_with(|| {
            let assigned = *next_tool_index;
            *next_tool_index = next_tool_index.saturating_add(1);
            assigned
        })
}

pub(super) fn select_stream_choice_index(
    selected_choice_index: Option<i32>,
    emitted_payload: bool,
    choices: &[OpenAiChatCompletionsChunkChoice],
) -> Option<i32> {
    if choices.is_empty() {
        return selected_choice_index;
    }

    let has_index_zero = choices.iter().any(|choice| choice.index == 0);
    match selected_choice_index {
        Some(0) => Some(0),
        Some(_current) if has_index_zero && !emitted_payload => Some(0),
        Some(current) => Some(current),
        None if has_index_zero => Some(0),
        None => Some(choices[0].index),
    }
}
