//! ChatGPT/Codex managed-auth Responses client.

use crate::openai_chat_completions::{
    OpenAiResponsesRequest, OpenAiResponsesResponse, OpenAiResponsesUsage,
    build_responses_request_for_model, convert_openai_responses_output,
    extract_responses_output_reasoning_signature,
};
use crate::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk, ToolCallDelta};
use alan_auth::{ChatgptAuthConfig, ChatgptAuthError, ChatgptAuthManager};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, instrument, warn};

use crate::{SseEventParser, TokenUsage};

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

/// Client for the ChatGPT/Codex managed-auth Responses-compatible surface.
pub struct ChatgptResponsesClient {
    client: reqwest::Client,
    auth_manager: ChatgptAuthManager,
    base_url: String,
    model: String,
    custom_headers: HashMap<String, String>,
    expected_account_id: Option<String>,
}

impl ChatgptResponsesClient {
    const BACKGROUND_POLL_INTERVAL: Duration = Duration::from_secs(2);

    pub fn with_params(
        base_url: &str,
        model: &str,
        custom_headers: HashMap<String, String>,
        expected_account_id: Option<String>,
        auth_storage_path: Option<PathBuf>,
    ) -> Result<Self> {
        let auth_manager = match auth_storage_path {
            Some(path) => ChatgptAuthManager::new(ChatgptAuthConfig::with_storage_path(path))
                .context("Failed to initialize ChatGPT auth manager")?,
            None => {
                ChatgptAuthManager::detect().context("Failed to initialize ChatGPT auth manager")?
            }
        };
        Ok(Self {
            client: reqwest::Client::new(),
            auth_manager,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            custom_headers,
            expected_account_id,
        })
    }

    fn clone_with_same_config(&self) -> Self {
        Self {
            client: self.client.clone(),
            auth_manager: self.auth_manager.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            custom_headers: self.custom_headers.clone(),
            expected_account_id: self.expected_account_id.clone(),
        }
    }

    fn build_openai_responses_request(
        &self,
        request: GenerationRequest,
        stream: bool,
    ) -> Result<OpenAiResponsesRequest> {
        let mut request = build_responses_request_for_model(self.model.clone(), request, stream)?;
        request.instructions = Some(request.instructions.unwrap_or_default());
        // Managed ChatGPT rejects `store=true`, so keep that provider invariant here.
        request.store = Some(false);
        // Managed ChatGPT also rejects the generic Responses `temperature`
        // field; runtime may still set one globally, so strip it here.
        request.temperature = None;
        // Managed ChatGPT currently rejects the official Responses
        // `max_output_tokens` field, so keep the request surface to the
        // subset accepted by the managed endpoint.
        request.max_output_tokens = None;
        Ok(request)
    }

    #[instrument(skip(self, request))]
    pub async fn openai_responses(
        &self,
        request: OpenAiResponsesRequest,
    ) -> Result<OpenAiResponsesResponse> {
        let response = self.execute_with_auth_retry(request, false).await?;
        response
            .json()
            .await
            .context("Failed to parse ChatGPT Responses API response")
    }

    #[instrument(skip(self, request, tx))]
    pub async fn stream_openai_responses(
        &self,
        request: OpenAiResponsesRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        let response = self.execute_with_auth_retry(request, true).await?;
        self.consume_openai_responses_stream(response, tx).await
    }

    #[instrument(skip(self))]
    pub async fn retrieve_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        let response = self.retrieve_with_auth_retry(response_id).await?;
        response
            .json()
            .await
            .context("Failed to parse retrieved ChatGPT Responses API response")
    }

    #[instrument(skip(self, tx))]
    pub async fn retrieve_openai_response_stream(
        &self,
        response_id: &str,
        starting_after: Option<u64>,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        let response = self
            .retrieve_stream_with_auth_retry(response_id, starting_after)
            .await?;
        self.consume_openai_responses_stream(response, tx).await
    }

    #[instrument(skip(self))]
    pub async fn cancel_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        let response = self.cancel_with_auth_retry(response_id).await?;
        response
            .json()
            .await
            .context("Failed to parse cancelled ChatGPT Responses API response")
    }

    async fn consume_openai_responses_stream(
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
            let chunk = chunk_result.context("Failed to read ChatGPT Responses stream chunk")?;
            for data in parser.push(&chunk) {
                if self
                    .handle_stream_event(
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
            if self
                .handle_stream_event(
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
        &self,
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
                                *saw_tool_calls =
                                    responses_output_contains_tool_call(&parsed.output);
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

    async fn execute_with_auth_retry(
        &self,
        request: OpenAiResponsesRequest,
        stream: bool,
    ) -> Result<reqwest::Response> {
        let response = self.send_request(&request, stream, false).await?;
        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            return check_chatgpt_response_status(response).await;
        }

        debug!("ChatGPT Responses request returned 401; attempting managed refresh");
        let retry = self.send_request(&request, stream, true).await?;
        check_chatgpt_response_status(retry).await
    }

    async fn retrieve_with_auth_retry(&self, response_id: &str) -> Result<reqwest::Response> {
        let response = self
            .send_retrieve_request(response_id, false, None, false)
            .await?;
        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            return check_chatgpt_response_status(response).await;
        }

        debug!("ChatGPT Responses retrieve returned 401; attempting managed refresh");
        let retry = self
            .send_retrieve_request(response_id, false, None, true)
            .await?;
        check_chatgpt_response_status(retry).await
    }

    async fn retrieve_stream_with_auth_retry(
        &self,
        response_id: &str,
        starting_after: Option<u64>,
    ) -> Result<reqwest::Response> {
        let response = self
            .send_retrieve_request(response_id, true, starting_after, false)
            .await?;
        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            return check_chatgpt_response_status(response).await;
        }

        debug!("ChatGPT Responses stream retrieve returned 401; attempting managed refresh");
        let retry = self
            .send_retrieve_request(response_id, true, starting_after, true)
            .await?;
        check_chatgpt_response_status(retry).await
    }

    async fn cancel_with_auth_retry(&self, response_id: &str) -> Result<reqwest::Response> {
        let response = self.send_cancel_request(response_id, false).await?;
        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            return check_chatgpt_response_status(response).await;
        }

        debug!("ChatGPT Responses cancel returned 401; attempting managed refresh");
        let retry = self.send_cancel_request(response_id, true).await?;
        check_chatgpt_response_status(retry).await
    }

    async fn send_request(
        &self,
        request: &OpenAiResponsesRequest,
        stream: bool,
        force_refresh: bool,
    ) -> Result<reqwest::Response> {
        self.validate_request(request)?;
        let auth = if force_refresh {
            self.auth_manager
                .force_refresh_auth_for_account(self.expected_account_id.as_deref())
                .await?
        } else {
            self.auth_manager
                .request_auth_for_account(self.expected_account_id.as_deref())
                .await?
        };
        let mut builder = self
            .client
            .post(format!("{}/responses", self.base_url))
            .header("Authorization", format!("Bearer {}", auth.access_token))
            .header("ChatGPT-Account-ID", auth.account_id)
            .json(request);
        if stream {
            builder = builder.header(reqwest::header::ACCEPT, "text/event-stream");
        }

        for (name, value) in &self.custom_headers {
            builder = builder.header(name, value);
        }

        let response = builder
            .send()
            .await
            .context("Failed to send request to ChatGPT Responses API")?;

        if stream {
            debug!("Started ChatGPT streaming Responses request");
        }

        Ok(response)
    }

    fn validate_request(&self, request: &OpenAiResponsesRequest) -> Result<()> {
        if request.previous_response_id.is_some() {
            anyhow::bail!(
                "ChatGPT managed Responses does not support previous_response_id continuation"
            );
        }
        if request.background == Some(true) {
            anyhow::bail!("ChatGPT managed Responses does not support background execution");
        }
        Ok(())
    }

    async fn send_retrieve_request(
        &self,
        response_id: &str,
        stream: bool,
        starting_after: Option<u64>,
        force_refresh: bool,
    ) -> Result<reqwest::Response> {
        let auth = if force_refresh {
            self.auth_manager
                .force_refresh_auth_for_account(self.expected_account_id.as_deref())
                .await?
        } else {
            self.auth_manager
                .request_auth_for_account(self.expected_account_id.as_deref())
                .await?
        };

        let mut url = format!("{}/responses/{}", self.base_url, response_id);
        if stream {
            url.push_str("?stream=true");
            if let Some(starting_after) = starting_after {
                url.push_str(&format!("&starting_after={starting_after}"));
            }
        }
        let mut builder = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", auth.access_token))
            .header("ChatGPT-Account-ID", auth.account_id);
        if stream {
            builder = builder.header(reqwest::header::ACCEPT, "text/event-stream");
        }

        for (name, value) in &self.custom_headers {
            builder = builder.header(name, value);
        }

        builder
            .send()
            .await
            .context("Failed to retrieve ChatGPT Responses API response")
    }

    async fn send_cancel_request(
        &self,
        response_id: &str,
        force_refresh: bool,
    ) -> Result<reqwest::Response> {
        let auth = if force_refresh {
            self.auth_manager
                .force_refresh_auth_for_account(self.expected_account_id.as_deref())
                .await?
        } else {
            self.auth_manager
                .request_auth_for_account(self.expected_account_id.as_deref())
                .await?
        };

        let mut builder = self
            .client
            .post(format!(
                "{}/responses/{}/cancel",
                self.base_url, response_id
            ))
            .header("Authorization", format!("Bearer {}", auth.access_token))
            .header("ChatGPT-Account-ID", auth.account_id);

        for (name, value) in &self.custom_headers {
            builder = builder.header(name, value);
        }

        builder
            .send()
            .await
            .context("Failed to cancel ChatGPT Responses API response")
    }

    async fn wait_for_background_response(
        &self,
        mut response: OpenAiResponsesResponse,
    ) -> Result<OpenAiResponsesResponse> {
        while matches!(response.status.as_deref(), Some("queued" | "in_progress")) {
            let response_id = response.id.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "ChatGPT Responses background response is missing an id while status is {:?}",
                    response.status
                )
            })?;
            tokio::time::sleep(Self::BACKGROUND_POLL_INTERVAL).await;
            response = self.retrieve_openai_response(&response_id).await?;
        }
        Ok(response)
    }

    async fn collect_streamed_generation(
        &self,
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

    async fn generate_via_stream(&self, request: GenerationRequest) -> Result<GenerationResponse> {
        let response_request = self.build_openai_responses_request(request, true)?;
        let background_requested = response_request.background == Some(true);
        let response = self.execute_with_auth_retry(response_request, true).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let client = self.clone_with_same_config();
        let stream_task =
            tokio::spawn(async move { client.consume_openai_responses_stream(response, tx).await });
        let collected = self.collect_streamed_generation(rx).await;
        let stream_result = stream_task
            .await
            .context("ChatGPT Responses stream task panicked")?;

        let mut generation = match (collected, stream_result) {
            (Ok(response), Ok(())) => response,
            (Ok(_), Err(error)) => {
                return Err(error).context("Failed to consume ChatGPT Responses stream");
            }
            (Err(error), Ok(())) => return Err(error),
            (Err(_), Err(error)) => {
                return Err(error).context("Failed to consume ChatGPT Responses stream");
            }
        };

        if background_requested
            && matches!(
                generation.provider_response_status.as_deref(),
                Some("queued" | "in_progress")
            )
        {
            let response_id = generation.provider_response_id.clone().ok_or_else(|| {
                anyhow::anyhow!("ChatGPT Responses background stream ended without a response id")
            })?;
            let response = self.retrieve_openai_response(&response_id).await?;
            let response = self.wait_for_background_response(response).await?;
            generation = convert_openai_responses_output(response);
        }

        Ok(generation)
    }
}

#[async_trait]
impl LlmProvider for ChatgptResponsesClient {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.generate_via_stream(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        let request = match system {
            Some(system) => GenerationRequest::new()
                .with_system_prompt(system)
                .with_user_message(user),
            None => GenerationRequest::new().with_user_message(user),
        };
        let response = self.generate(request).await?;
        Ok(response.content)
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        let response_request = self.build_openai_responses_request(request, true)?;
        let response = self.execute_with_auth_retry(response_request, true).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let client = self.clone_with_same_config();
        tokio::spawn(async move {
            let _ = client.consume_openai_responses_stream(response, tx).await;
        });
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "chatgpt"
    }
}

async fn check_chatgpt_response_status(response: reqwest::Response) -> Result<reqwest::Response> {
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let body = response.text().await.unwrap_or_default();
        return Err(ChatgptAuthError::UnauthorizedAfterRefresh(body).into());
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("ChatGPT Responses API error ({}): {}", status, body);
    }
    Ok(response)
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
    use super::{
        ChatgptResponsesClient, StreamEventAction, emit_terminal_stream_chunk,
        responses_finish_reason,
    };
    use crate::factory::{ProviderConfig, ProviderType};
    use crate::{GenerationRequest, LlmProvider, SseEventParser, StreamChunk, TokenUsage};
    use alan_auth::{
        AuthStorage, AuthStore, ChatgptAuthConfig, ChatgptAuthError, ChatgptAuthManager,
        ChatgptIdTokenInfo, ChatgptTokenData, StoredChatgptAuth,
    };
    use axum::{
        Json, Router, extract::State, http::HeaderMap, response::IntoResponse, routing::post,
    };
    use base64::Engine;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct TestServerState {
        response_count: Arc<AtomicUsize>,
        refresh_count: Arc<AtomicUsize>,
        authorizations: Arc<Mutex<Vec<String>>>,
        account_ids: Arc<Mutex<Vec<String>>>,
        accept_headers: Arc<Mutex<Vec<String>>>,
        request_bodies: Arc<Mutex<Vec<serde_json::Value>>>,
        response_mode: TestResponseMode,
    }

    #[derive(Clone, Copy)]
    enum TestResponseMode {
        AlwaysOk,
        RequireStream,
        UnauthorizedThenOk,
        AlwaysUnauthorized,
    }

    fn build_jwt(payload: serde_json::Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{header}.{payload}.sig")
    }

    fn seed_chatgpt_auth(
        storage_path: PathBuf,
        access_token: String,
        refresh_token: &str,
    ) -> PathBuf {
        let storage = AuthStorage::new(storage_path.clone()).expect("storage");
        let id_token = build_jwt(serde_json::json!({
            "email": "user@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "pro",
                "chatgpt_user_id": "user_123",
                "chatgpt_account_id": "acct_123"
            }
        }));
        storage
            .save(&AuthStore {
                version: 1,
                chatgpt: Some(
                    StoredChatgptAuth::from_tokens(ChatgptTokenData {
                        id_token: ChatgptIdTokenInfo {
                            email: Some("user@example.com".to_string()),
                            plan_type: Some("pro".to_string()),
                            user_id: Some("user_123".to_string()),
                            account_id: Some("acct_123".to_string()),
                            raw_jwt: id_token,
                        },
                        access_token,
                        refresh_token: refresh_token.to_string(),
                    })
                    .expect("stored auth"),
                ),
            })
            .expect("save auth");
        storage_path
    }

    fn test_client(base_url: &str, storage_path: PathBuf) -> ChatgptResponsesClient {
        ChatgptResponsesClient {
            client: reqwest::Client::new(),
            auth_manager: ChatgptAuthManager::new(ChatgptAuthConfig {
                storage_path,
                issuer: base_url.to_string(),
                client_id: "client".to_string(),
                browser_callback_port: 1455,
            })
            .expect("auth manager"),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: "gpt-5.3-codex".to_string(),
            custom_headers: HashMap::new(),
            expected_account_id: Some("acct_123".to_string()),
        }
    }

    async fn spawn_chatgpt_test_server(
        response_mode: TestResponseMode,
    ) -> (String, TestServerState, tokio::task::JoinHandle<()>) {
        fn ok_response_body(text: &str) -> serde_json::Value {
            serde_json::json!({
                "id": "resp_123",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{"type": "output_text", "text": text}]
                }],
                "usage": {
                    "input_tokens": 1,
                    "output_tokens": 1,
                    "total_tokens": 2,
                    "output_tokens_details": {"reasoning_tokens": 0}
                }
            })
        }

        fn streaming_response(text: &str) -> axum::response::Response {
            let delta = serde_json::json!({
                "type": "response.output_text.delta",
                "delta": text,
                "sequence_number": 0
            });
            let completed = serde_json::json!({
                "type": "response.completed",
                "sequence_number": 1,
                "response": ok_response_body(text)
            });
            let body = format!("data: {delta}\n\ndata: {completed}\n\n");
            (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                body,
            )
                .into_response()
        }

        fn json_response(
            status: axum::http::StatusCode,
            body: serde_json::Value,
        ) -> axum::response::Response {
            (status, Json(body)).into_response()
        }

        async fn refresh_token(State(state): State<TestServerState>) -> Json<serde_json::Value> {
            state.refresh_count.fetch_add(1, Ordering::SeqCst);
            Json(serde_json::json!({
                "id_token": build_jwt(serde_json::json!({
                    "email": "user@example.com",
                    "https://api.openai.com/auth": {
                        "chatgpt_plan_type": "pro",
                        "chatgpt_user_id": "user_123",
                        "chatgpt_account_id": "acct_123"
                    }
                })),
                "access_token": build_jwt(serde_json::json!({"exp": 4_102_444_800_i64, "token": "refreshed"})),
                "refresh_token": "refresh_token_rotated"
            }))
        }

        async fn responses(
            State(state): State<TestServerState>,
            headers: HeaderMap,
            axum::Json(request_body): axum::Json<serde_json::Value>,
        ) -> axum::response::Response {
            let count = state.response_count.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(auth) = headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
            {
                state
                    .authorizations
                    .lock()
                    .expect("authorizations")
                    .push(auth.to_string());
            }
            if let Some(account_id) = headers
                .get("chatgpt-account-id")
                .and_then(|value| value.to_str().ok())
            {
                state
                    .account_ids
                    .lock()
                    .expect("account ids")
                    .push(account_id.to_string());
            }
            if let Some(accept) = headers.get("accept").and_then(|value| value.to_str().ok()) {
                state
                    .accept_headers
                    .lock()
                    .expect("accept headers")
                    .push(accept.to_string());
            }
            state
                .request_bodies
                .lock()
                .expect("request bodies")
                .push(request_body.clone());

            let stream_requested = request_body
                .get("stream")
                .and_then(serde_json::Value::as_bool)
                == Some(true);

            match state.response_mode {
                TestResponseMode::AlwaysOk => {
                    if stream_requested {
                        streaming_response("ok")
                    } else {
                        json_response(axum::http::StatusCode::OK, ok_response_body("ok"))
                    }
                }
                TestResponseMode::RequireStream if !stream_requested => json_response(
                    axum::http::StatusCode::BAD_REQUEST,
                    serde_json::json!({"detail": "Stream must be set to true"}),
                ),
                TestResponseMode::RequireStream => streaming_response("ok"),
                TestResponseMode::UnauthorizedThenOk if count == 1 => json_response(
                    axum::http::StatusCode::UNAUTHORIZED,
                    serde_json::json!({"error": "expired"}),
                ),
                TestResponseMode::UnauthorizedThenOk => {
                    if stream_requested {
                        streaming_response("retried")
                    } else {
                        json_response(axum::http::StatusCode::OK, ok_response_body("retried"))
                    }
                }
                TestResponseMode::AlwaysUnauthorized => json_response(
                    axum::http::StatusCode::UNAUTHORIZED,
                    serde_json::json!({"error": "still unauthorized"}),
                ),
            }
        }

        let state = TestServerState {
            response_count: Arc::new(AtomicUsize::new(0)),
            refresh_count: Arc::new(AtomicUsize::new(0)),
            authorizations: Arc::new(Mutex::new(Vec::new())),
            account_ids: Arc::new(Mutex::new(Vec::new())),
            accept_headers: Arc::new(Mutex::new(Vec::new())),
            request_bodies: Arc::new(Mutex::new(Vec::new())),
            response_mode,
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        let app = Router::new()
            .route("/oauth/token", post(refresh_token))
            .route("/responses", post(responses))
            .with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        (format!("http://{}", address), state, server)
    }

    fn expired_access_token() -> String {
        build_jwt(serde_json::json!({"exp": 1_i64, "token": "expired"}))
    }

    fn valid_access_token() -> String {
        build_jwt(serde_json::json!({"exp": 4_102_444_800_i64, "token": "initial"}))
    }

    fn refreshed_access_token() -> String {
        build_jwt(serde_json::json!({"exp": 4_102_444_800_i64, "token": "refreshed"}))
    }

    #[test]
    fn provider_config_builds_chatgpt_client() {
        let config = ProviderConfig::chatgpt("gpt-5.3-codex")
            .with_base_url("https://chatgpt.com/backend-api/codex")
            .with_chatgpt_account_id("acct_123");
        assert_eq!(config.provider_type, ProviderType::ChatgptResponses);
        assert_eq!(config.expected_account_id.as_deref(), Some("acct_123"));
    }

    #[test]
    fn client_requires_auth_manager_paths() {
        let client = ChatgptResponsesClient::with_params(
            "https://chatgpt.com/backend-api/codex",
            "gpt-5.3-codex",
            HashMap::new(),
            None,
            None,
        );
        assert!(client.is_ok());
    }

    #[test]
    fn client_uses_custom_auth_storage_path_when_provided() {
        let storage_path = std::env::temp_dir().join(format!(
            "alan-llm-chatgpt-auth-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let storage_path = storage_path.join("auth.json");
        let client = ChatgptResponsesClient::with_params(
            "https://chatgpt.com/backend-api/codex",
            "gpt-5.3-codex",
            HashMap::new(),
            None,
            Some(storage_path.clone()),
        )
        .expect("client");

        assert_eq!(client.auth_manager.storage_path(), storage_path.as_path());
    }

    #[tokio::test]
    async fn proactive_refresh_happens_before_dispatch() {
        let temp_dir = TempDir::new().expect("temp dir");
        let storage_path = seed_chatgpt_auth(
            temp_dir.path().join("auth.json"),
            expired_access_token(),
            "refresh",
        );
        let (base_url, state, server) = spawn_chatgpt_test_server(TestResponseMode::AlwaysOk).await;
        let mut client = test_client(&base_url, storage_path);

        let result = client.chat(None, "hello").await.expect("chat");
        assert_eq!(result, "ok");
        assert_eq!(state.refresh_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.response_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            state.authorizations.lock().expect("authorizations").clone(),
            vec![format!("Bearer {}", refreshed_access_token())]
        );
        assert_eq!(
            state.account_ids.lock().expect("account ids").clone(),
            vec!["acct_123".to_string()]
        );
        assert_eq!(
            state.accept_headers.lock().expect("accept headers").clone(),
            vec!["text/event-stream".to_string()]
        );
        let request_bodies = state.request_bodies.lock().expect("request bodies").clone();
        assert_eq!(request_bodies.len(), 1);
        assert_eq!(request_bodies[0]["instructions"], "");
        assert_eq!(request_bodies[0]["stream"], true);
        assert_eq!(request_bodies[0]["input"][0]["role"], "user");

        server.abort();
    }

    #[tokio::test]
    async fn unauthorized_response_triggers_single_refresh_and_retry() {
        let temp_dir = TempDir::new().expect("temp dir");
        let storage_path = seed_chatgpt_auth(
            temp_dir.path().join("auth.json"),
            valid_access_token(),
            "refresh",
        );
        let (base_url, state, server) =
            spawn_chatgpt_test_server(TestResponseMode::UnauthorizedThenOk).await;
        let mut client = test_client(&base_url, storage_path);

        let result = client.chat(None, "hello").await.expect("chat");
        assert_eq!(result, "retried");
        assert_eq!(state.refresh_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.response_count.load(Ordering::SeqCst), 2);
        let authorizations = state.authorizations.lock().expect("authorizations").clone();
        assert_eq!(authorizations.len(), 2);
        assert_eq!(
            authorizations[0],
            format!("Bearer {}", valid_access_token())
        );
        assert_eq!(
            authorizations[1],
            format!("Bearer {}", refreshed_access_token())
        );
        assert_eq!(
            state.account_ids.lock().expect("account ids").clone(),
            vec!["acct_123".to_string(), "acct_123".to_string()]
        );
        let request_bodies = state.request_bodies.lock().expect("request bodies").clone();
        assert_eq!(request_bodies.len(), 2);
        assert_eq!(request_bodies[0]["stream"], true);
        assert_eq!(request_bodies[1]["stream"], true);
        assert_eq!(
            state.accept_headers.lock().expect("accept headers").clone(),
            vec![
                "text/event-stream".to_string(),
                "text/event-stream".to_string()
            ]
        );

        server.abort();
    }

    #[tokio::test]
    async fn repeated_unauthorized_surfaces_first_class_auth_error() {
        let temp_dir = TempDir::new().expect("temp dir");
        let storage_path = seed_chatgpt_auth(
            temp_dir.path().join("auth.json"),
            valid_access_token(),
            "refresh",
        );
        let (base_url, state, server) =
            spawn_chatgpt_test_server(TestResponseMode::AlwaysUnauthorized).await;
        let mut client = test_client(&base_url, storage_path);

        let error = client.chat(None, "hello").await.expect_err("auth failure");
        let auth_error = error
            .downcast_ref::<ChatgptAuthError>()
            .expect("ChatGPT auth error");
        assert!(matches!(
            auth_error,
            ChatgptAuthError::UnauthorizedAfterRefresh(message)
                if message.contains("still unauthorized")
        ));
        assert_eq!(state.refresh_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.response_count.load(Ordering::SeqCst), 2);

        server.abort();
    }

    #[tokio::test]
    async fn chatgpt_requests_send_instructions_separately_from_input() {
        let temp_dir = TempDir::new().expect("temp dir");
        let storage_path = seed_chatgpt_auth(
            temp_dir.path().join("auth.json"),
            valid_access_token(),
            "refresh",
        );
        let (base_url, state, server) = spawn_chatgpt_test_server(TestResponseMode::AlwaysOk).await;
        let mut client = test_client(&base_url, storage_path);

        let result = client
            .chat(Some("Follow the system prompt"), "hello")
            .await
            .expect("chat");
        assert_eq!(result, "ok");
        let request_bodies = state.request_bodies.lock().expect("request bodies").clone();
        assert_eq!(request_bodies.len(), 1);
        assert_eq!(
            request_bodies[0]["instructions"],
            serde_json::Value::String("Follow the system prompt".to_string())
        );
        assert_eq!(request_bodies[0]["stream"], true);
        assert_eq!(request_bodies[0]["input"].as_array().map(Vec::len), Some(1));
        assert_eq!(request_bodies[0]["input"][0]["role"], "user");

        server.abort();
    }

    #[tokio::test]
    async fn generate_uses_streaming_contract_when_server_requires_stream_true() {
        let temp_dir = TempDir::new().expect("temp dir");
        let storage_path = seed_chatgpt_auth(
            temp_dir.path().join("auth.json"),
            valid_access_token(),
            "refresh",
        );
        let (base_url, state, server) =
            spawn_chatgpt_test_server(TestResponseMode::RequireStream).await;
        let mut client = test_client(&base_url, storage_path);

        let response = client
            .generate(GenerationRequest::new().with_user_message("hello"))
            .await
            .expect("generate");

        assert_eq!(response.content, "ok");
        assert_eq!(response.finish_reason.as_deref(), Some("stop"));
        assert_eq!(response.provider_response_id.as_deref(), Some("resp_123"));
        assert_eq!(
            response.provider_response_status.as_deref(),
            Some("completed")
        );
        assert_eq!(response.usage.map(|usage| usage.total_tokens), Some(2));
        assert!(response.warnings.is_empty());

        let request_bodies = state.request_bodies.lock().expect("request bodies").clone();
        assert_eq!(request_bodies.len(), 1);
        assert_eq!(request_bodies[0]["stream"], true);
        assert_eq!(
            state.accept_headers.lock().expect("accept headers").clone(),
            vec!["text/event-stream".to_string()]
        );

        server.abort();
    }

    #[tokio::test]
    async fn generate_rejects_previous_response_id_for_managed_chatgpt() {
        let storage_path = std::env::temp_dir().join(format!(
            "alan-llm-chatgpt-auth-continuation-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let storage_path = storage_path.join("auth.json");
        let mut client = ChatgptResponsesClient::with_params(
            "https://chatgpt.com/backend-api/codex",
            "gpt-5.3-codex",
            HashMap::new(),
            None,
            Some(storage_path),
        )
        .expect("client");

        let error = client
            .generate(
                GenerationRequest::new()
                    .with_user_message("hello")
                    .with_previous_response_id("resp_prev"),
            )
            .await
            .expect_err("continuation should be rejected before dispatch");
        assert!(
            error
                .to_string()
                .contains("does not support previous_response_id continuation")
        );
    }

    #[test]
    fn chatgpt_build_request_normalizes_managed_request_fields() {
        let client = ChatgptResponsesClient::with_params(
            "https://chatgpt.com/backend-api/codex",
            "gpt-5.3-codex",
            HashMap::new(),
            Some("acct_123".to_string()),
            None,
        )
        .expect("client");

        let request = client
            .build_openai_responses_request(
                crate::GenerationRequest::new()
                    .with_system_prompt("system")
                    .with_user_message("hello")
                    .with_previous_response_id("resp_prev")
                    .with_temperature(0.7)
                    .with_store(true),
                false,
            )
            .unwrap();

        assert_eq!(request.previous_response_id.as_deref(), Some("resp_prev"));
        assert_eq!(request.store, Some(false));
        assert_eq!(request.instructions.as_deref(), Some("system"));
        assert_eq!(request.temperature, None);
        assert_eq!(request.max_output_tokens, None);
    }

    #[tokio::test]
    async fn stream_finish_flushes_trailing_completed_event() {
        let client = ChatgptResponsesClient::with_params(
            "https://chatgpt.com/backend-api/codex",
            "gpt-5.3-codex",
            HashMap::new(),
            Some("acct_123".to_string()),
            None,
        )
        .expect("client");
        assert_eq!(client.expected_account_id.as_deref(), Some("acct_123"));
        let mut parser = SseEventParser::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(4);
        let mut latest_usage = None;
        let mut emitted_payload = true;
        let mut saw_tool_calls = false;

        let completed_event = r#"data: {"type":"response.completed","response":{"id":"resp_123","output":[],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}}"#;
        assert!(parser.push(completed_event.as_bytes()).is_empty());

        for data in parser.finish() {
            let action = client
                .handle_stream_event(
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
        let client = ChatgptResponsesClient::with_params(
            "https://chatgpt.com/backend-api/codex",
            "gpt-5.3-codex",
            HashMap::new(),
            Some("acct_123".to_string()),
            None,
        )
        .expect("client");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(4);
        let mut latest_usage = None;
        let mut emitted_payload = false;
        let mut saw_tool_calls = false;

        let action = client
            .handle_stream_event(
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

    #[tokio::test]
    async fn generate_stream_surfaces_auth_errors_before_returning_receiver() {
        let storage_path = std::env::temp_dir().join(format!(
            "alan-llm-chatgpt-auth-stream-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let storage_path = storage_path.join("auth.json");
        let mut client = ChatgptResponsesClient::with_params(
            "https://chatgpt.com/backend-api/codex",
            "gpt-5.3-codex",
            HashMap::new(),
            None,
            Some(storage_path),
        )
        .expect("client");

        let error = client
            .generate_stream(crate::GenerationRequest::new().with_user_message("hi"))
            .await
            .expect_err("missing auth should fail before returning a receiver");
        let auth_error = error
            .downcast_ref::<alan_auth::ChatgptAuthError>()
            .expect("chatgpt auth error");
        assert!(matches!(
            auth_error,
            alan_auth::ChatgptAuthError::NotLoggedIn
        ));
    }
}
