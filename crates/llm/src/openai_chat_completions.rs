//! OpenAI Chat Completions API client.
//!
//! Supports the official OpenAI Chat Completions API and generic OpenAI Chat
//! Completions API-compatible endpoints.

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, instrument, warn};

use crate::message::reject_retired_message_overrides;
use crate::{
    GenerationRequest, GenerationResponse, LlmProvider, Message as LlmMessage, MessageRole,
    ReasoningEffort, SseEventParser, StreamChunk, TokenUsage, ToolCall as LlmToolCall,
    ToolCallDelta, ToolDefinition as LlmToolDefinition,
};
use async_trait::async_trait;

mod input_projection;

use input_projection::{
    convert_messages_for_openai_chat_completions_with_instruction_role, plain_text_content,
    responses_content, responses_output,
};

/// Client for the OpenAI Chat Completions API and compatible endpoints.
pub struct OpenAiChatCompletionsClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    api_flavor: OpenAiChatCompletionsApiFlavor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenAiChatCompletionsApiFlavor {
    Official,
    Compatible,
}

#[derive(Debug, Serialize)]
pub struct OpenAiChatCompletionsRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiChatCompletionsToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<OpenAiChatCompletionsStreamOptions>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChatCompletionsStreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsMessage {
    pub role: String, // system, user, assistant, tool
    pub content: Option<String>,
    /// Provider-specific reasoning/thinking content (e.g. DeepSeek `reasoning_content`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// Provider-specific reasoning metadata payload (e.g. encrypted reasoning state).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiChatCompletionsToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsToolDefinition {
    pub r#type: String,
    pub function: OpenAiChatCompletionsFunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsFunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiResponsesToolDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsToolCall {
    pub id: String,
    pub r#type: String,
    pub function: OpenAiChatCompletionsFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsFunctionCall {
    pub name: String,
    pub arguments: String, // JSON string, needs parsing
}

// Responses API types

#[derive(Debug, Serialize)]
pub struct OpenAiResponsesRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    pub input: Vec<OpenAiResponsesInputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiResponsesToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<OpenAiResponsesReasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiResponsesReasoning {
    pub effort: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OpenAiResponsesInputItem {
    Message(OpenAiResponsesInputMessage),
    Reasoning(OpenAiResponsesReasoningInputItem),
    FunctionCall(OpenAiResponsesFunctionCallItem),
    FunctionCallOutput(OpenAiResponsesFunctionCallOutputItem),
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiResponsesInputMessage {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiResponsesReasoningInputItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub encrypted_content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiResponsesFunctionCallItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiResponsesFunctionCallOutputItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub call_id: String,
    pub output: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesResponse {
    pub id: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub background: Option<bool>,
    #[serde(default)]
    pub output: Vec<serde_json::Value>,
    pub usage: Option<OpenAiResponsesUsage>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiResponsesInputTokensRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub input: Vec<OpenAiResponsesInputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiResponsesToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<OpenAiResponsesReasoning>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesInputTokensResponse {
    pub object: Option<String>,
    pub input_tokens: i32,
}

#[derive(Debug, Serialize)]
pub struct OpenAiResponsesCompactRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<serde_json::Value>>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesCompactResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created_at: Option<i64>,
    #[serde(default)]
    pub output: Vec<serde_json::Value>,
    pub usage: Option<OpenAiResponsesUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesUsage {
    pub input_tokens: i32,
    pub input_tokens_details: Option<OpenAiResponsesInputTokensDetails>,
    pub output_tokens: i32,
    pub total_tokens: i32,
    pub output_tokens_details: Option<OpenAiResponsesOutputTokensDetails>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesInputTokensDetails {
    pub cached_tokens: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesOutputTokensDetails {
    pub reasoning_tokens: Option<i32>,
}

// Response types

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAiChatCompletionsChoice>,
    pub usage: Option<OpenAiChatCompletionsUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsChoice {
    pub index: i32,
    pub message: OpenAiChatCompletionsMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsUsage {
    pub prompt_tokens: i32,
    pub prompt_tokens_details: Option<OpenAiChatCompletionsPromptTokensDetails>,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub completion_tokens_details: Option<OpenAiChatCompletionsCompletionTokensDetails>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsPromptTokensDetails {
    pub cached_tokens: Option<i32>,
    pub audio_tokens: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsCompletionTokensDetails {
    pub reasoning_tokens: Option<i32>,
    pub audio_tokens: Option<i32>,
    pub accepted_prediction_tokens: Option<i32>,
    pub rejected_prediction_tokens: Option<i32>,
}

// Streaming response types

/// Stream chunk from OpenAI streaming API
#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAiChatCompletionsChunkChoice>,
    pub usage: Option<OpenAiChatCompletionsUsage>,
}

/// A choice in streaming response
#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsChunkChoice {
    pub index: i32,
    pub delta: OpenAiChatCompletionsDeltaMessage,
    pub finish_reason: Option<String>,
}

/// Delta message in streaming response (incremental content)
#[derive(Debug, Deserialize, Default)]
pub struct OpenAiChatCompletionsDeltaMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub reasoning: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAiChatCompletionsStreamToolCall>>,
}

/// Tool call in streaming response
#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsStreamToolCall {
    pub index: i32,
    pub id: Option<String>,
    pub r#type: Option<String>,
    pub function: Option<OpenAiChatCompletionsStreamFunctionCall>,
}

/// Function call in streaming response
#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsStreamFunctionCall {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

// ============================================================================
// Client Implementation
// ============================================================================

impl OpenAiChatCompletionsClient {
    fn instruction_role_name(&self) -> &'static str {
        match self.api_flavor {
            OpenAiChatCompletionsApiFlavor::Official => "developer",
            OpenAiChatCompletionsApiFlavor::Compatible => "system",
        }
    }

    fn new(
        api_key: &str,
        base_url: &str,
        model: &str,
        api_flavor: OpenAiChatCompletionsApiFlavor,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.to_string(),
            base_url: base_url.to_string(),
            model: model.to_string(),
            api_flavor,
        }
    }

    /// Create a client for official OpenAI endpoints.
    pub fn official_with_params(api_key: &str, base_url: &str, model: &str) -> Self {
        Self::new(
            api_key,
            base_url,
            model,
            OpenAiChatCompletionsApiFlavor::Official,
        )
    }

    /// Create a client for the OpenAI Chat Completions API-compatible surface.
    pub fn compatible_with_params(api_key: &str, base_url: &str, model: &str) -> Self {
        Self::new(
            api_key,
            base_url,
            model,
            OpenAiChatCompletionsApiFlavor::Compatible,
        )
    }

    pub(crate) fn clone_with_same_config(&self) -> Self {
        Self {
            client: self.client.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            api_flavor: self.api_flavor,
        }
    }

    /// Chat completion (non-streaming)
    #[instrument(skip(self, request))]
    pub async fn openai_chat_completions(
        &self,
        mut request: OpenAiChatCompletionsRequest,
    ) -> Result<OpenAiChatCompletionsResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        // Use the model from the client if not set in the request
        if request.model.is_empty() {
            request.model = self.model.clone();
        }

        debug!(url = %url, model = %request.model, "Sending chat completion request");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error ({}): {}", status, error_text);
        }

        let result: OpenAiChatCompletionsResponse = response
            .json()
            .await
            .context("Failed to parse OpenAI Chat Completions API response")?;

        Ok(result)
    }

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

    #[instrument(skip(self, request))]
    pub async fn openai_responses(
        &self,
        mut request: OpenAiResponsesRequest,
    ) -> Result<OpenAiResponsesResponse> {
        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));

        if request.model.is_empty() {
            request.model = self.model.clone();
        }

        debug!(url = %url, model = %request.model, "Sending responses request");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI Responses API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI Responses API error ({}): {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse OpenAI Responses API response")
    }

    #[instrument(skip(self))]
    pub async fn retrieve_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        let url = format!(
            "{}/responses/{}",
            self.base_url.trim_end_matches('/'),
            response_id
        );
        debug!(url = %url, response_id, "Retrieving Responses API response");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to retrieve OpenAI Responses API response")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI Responses API error ({}): {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse retrieved OpenAI Responses API response")
    }

    #[instrument(skip(self, request))]
    pub async fn compact_openai_response(
        &self,
        mut request: OpenAiResponsesCompactRequest,
    ) -> Result<OpenAiResponsesCompactResponse> {
        let url = format!("{}/responses/compact", self.base_url.trim_end_matches('/'));

        if request.model.is_empty() {
            request.model = self.model.clone();
        }

        debug!(url = %url, model = %request.model, "Sending Responses compact request");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI Responses compact API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "OpenAI Responses compact API error ({}): {}",
                status,
                error_text
            );
        }

        response
            .json()
            .await
            .context("Failed to parse OpenAI Responses compact API response")
    }

    #[instrument(skip(self, request))]
    pub async fn count_openai_response_input_tokens(
        &self,
        mut request: OpenAiResponsesInputTokensRequest,
    ) -> Result<OpenAiResponsesInputTokensResponse> {
        let url = format!(
            "{}/responses/input_tokens",
            self.base_url.trim_end_matches('/')
        );

        if request.model.is_empty() {
            request.model = self.model.clone();
        }

        debug!(
            url = %url,
            model = %request.model,
            "Sending Responses input token count request"
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI Responses input token count API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "OpenAI Responses input token count API error ({}): {}",
                status,
                error_text
            );
        }

        response
            .json()
            .await
            .context("Failed to parse OpenAI Responses input token count API response")
    }

    #[instrument(skip(self))]
    pub async fn cancel_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        let url = format!(
            "{}/responses/{}/cancel",
            self.base_url.trim_end_matches('/'),
            response_id
        );
        debug!(url = %url, response_id, "Cancelling Responses API response");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to cancel OpenAI Responses API response")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI Responses API error ({}): {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse cancelled OpenAI Responses API response")
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

    pub(crate) fn build_openai_responses_request(
        &self,
        request: GenerationRequest,
        stream: bool,
    ) -> Result<OpenAiResponsesRequest> {
        build_responses_request_for_model(self.model.clone(), request, stream)
    }

    #[cfg(test)]
    pub(crate) fn build_openai_responses_input_tokens_request(
        &self,
        request: GenerationRequest,
    ) -> Result<OpenAiResponsesInputTokensRequest> {
        build_responses_input_tokens_request_for_model(self.model.clone(), request)
    }

    /// Simple chat helper
    pub async fn chat(&self, system: Option<&str>, user_message: &str) -> Result<String> {
        let mut messages = Vec::new();

        if let Some(sys) = system {
            messages.push(openai_chat_completions_message_value(
                self.instruction_role_name(),
                Some(serde_json::Value::String(sys.to_string())),
                None,
                None,
                None,
                None,
            ));
        }

        messages.push(openai_chat_completions_message_value(
            "user",
            Some(serde_json::Value::String(user_message.to_string())),
            None,
            None,
            None,
            None,
        ));

        let request = OpenAiChatCompletionsRequest {
            model: self.model.clone(),
            messages,
            tools: None,
            tool_choice: None,
            temperature: Some(0.7),
            max_completion_tokens: Some(2048),
            reasoning_effort: None,
            stream: Some(false),
            stream_options: None,
            extra_params: HashMap::new(),
        };

        let response = self.openai_chat_completions(request).await?;

        Ok(response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default())
    }
}

// ============================================================================
// Helper functions
// ============================================================================

impl OpenAiChatCompletionsMessage {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            role: "assistant".to_string(),
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            reasoning_content: None,
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a tool message (response to a tool call)
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            reasoning: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

impl OpenAiChatCompletionsToolDefinition {
    /// Create a tool definition from name, description, and parameters
    pub fn new(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            r#type: "function".to_string(),
            function: OpenAiChatCompletionsFunctionDefinition {
                name: name.to_string(),
                description: description.to_string(),
                parameters,
            },
        }
    }
}

impl OpenAiResponsesToolDefinition {
    /// Create a Responses-native tool definition from name, description, and parameters.
    pub fn new(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            kind: "function".to_string(),
            name: name.to_string(),
            description: description.to_string(),
            parameters,
        }
    }
}

fn openai_chat_completions_message_value(
    role: impl Into<String>,
    content: Option<serde_json::Value>,
    reasoning_content: Option<String>,
    reasoning: Option<serde_json::Value>,
    tool_calls: Option<Vec<OpenAiChatCompletionsToolCall>>,
    tool_call_id: Option<String>,
) -> serde_json::Value {
    let mut message = serde_json::Map::new();
    message.insert("role".to_string(), serde_json::Value::String(role.into()));
    if let Some(content) = content {
        message.insert("content".to_string(), content);
    }
    if let Some(reasoning_content) = reasoning_content {
        message.insert(
            "reasoning_content".to_string(),
            serde_json::Value::String(reasoning_content),
        );
    }
    if let Some(reasoning) = reasoning {
        message.insert("reasoning".to_string(), reasoning);
    }
    if let Some(tool_calls) = tool_calls {
        message.insert(
            "tool_calls".to_string(),
            serde_json::to_value(tool_calls).unwrap_or_else(|_| serde_json::Value::Array(vec![])),
        );
    }
    if let Some(tool_call_id) = tool_call_id {
        message.insert(
            "tool_call_id".to_string(),
            serde_json::Value::String(tool_call_id),
        );
    }
    serde_json::Value::Object(message)
}

#[cfg(test)]
pub(crate) fn convert_messages_for_openai_chat_completions(
    messages: Vec<LlmMessage>,
) -> Vec<serde_json::Value> {
    convert_messages_for_openai_chat_completions_with_instruction_role(messages, "system")
        .expect("test messages should have representable Chat Completions content")
}

pub(crate) fn convert_tools_for_openai_chat_completions(
    tools: Vec<LlmToolDefinition>,
) -> (
    Option<Vec<OpenAiChatCompletionsToolDefinition>>,
    Option<String>,
) {
    if tools.is_empty() {
        (None, None)
    } else {
        (
            Some(
                tools
                    .into_iter()
                    .map(|tool| OpenAiChatCompletionsToolDefinition {
                        r#type: "function".to_string(),
                        function: OpenAiChatCompletionsFunctionDefinition {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.parameters,
                        },
                    })
                    .collect(),
            ),
            Some("auto".to_string()),
        )
    }
}

pub(crate) fn convert_tools_for_openai_responses(
    tools: Vec<LlmToolDefinition>,
) -> (Option<Vec<OpenAiResponsesToolDefinition>>, Option<String>) {
    if tools.is_empty() {
        (None, None)
    } else {
        (
            Some(
                tools
                    .into_iter()
                    .map(|tool| {
                        OpenAiResponsesToolDefinition::new(
                            &tool.name,
                            &tool.description,
                            tool.parameters,
                        )
                    })
                    .collect(),
            ),
            Some("auto".to_string()),
        )
    }
}

pub(crate) fn normalize_responses_instructions(system_prompt: Option<String>) -> Option<String> {
    system_prompt.filter(|value| is_non_empty(value))
}

pub(crate) fn build_responses_request_for_model(
    model: String,
    request: GenerationRequest,
    stream: bool,
) -> Result<OpenAiResponsesRequest> {
    reject_retired_message_overrides(&request)?;
    let GenerationRequest {
        system_prompt,
        messages,
        tools,
        temperature,
        max_tokens,
        reasoning,
        mut extra_params,
    } = request;

    let previous_response_id = take_string_extra_param("previous_response_id", &mut extra_params);
    let background = take_bool_extra_param("background", &mut extra_params);
    let mut store = take_bool_extra_param("store", &mut extra_params);
    if (matches!(background, Some(true)) || previous_response_id.is_some()) && store.is_none() {
        store = Some(true);
    }

    let reasoning = build_openai_responses_reasoning(reasoning.effort, &mut extra_params);
    let include = normalize_responses_include(
        take_string_array_extra_param("include", &mut extra_params),
        should_include_reasoning_encrypted_content(&messages, &tools, reasoning.is_some()),
    );
    let (response_tools, tool_choice) = convert_tools_for_openai_responses(tools);
    let input = convert_messages_for_openai_responses(messages)?;

    Ok(OpenAiResponsesRequest {
        model,
        instructions: normalize_responses_instructions(system_prompt),
        previous_response_id,
        store,
        background,
        include,
        input,
        tools: response_tools,
        tool_choice,
        temperature,
        max_output_tokens: build_max_completion_tokens(max_tokens, &mut extra_params),
        reasoning,
        stream: Some(stream),
        extra_params,
    })
}

#[cfg(test)]
pub(crate) fn build_responses_input_tokens_request_for_model(
    model: String,
    request: GenerationRequest,
) -> Result<OpenAiResponsesInputTokensRequest> {
    reject_retired_message_overrides(&request)?;
    let GenerationRequest {
        system_prompt,
        messages,
        tools,
        temperature: _,
        max_tokens: _,
        reasoning,
        mut extra_params,
    } = request;

    let reasoning = build_openai_responses_reasoning(reasoning.effort, &mut extra_params);
    let (response_tools, tool_choice) = convert_tools_for_openai_responses(tools);
    let input = convert_messages_for_openai_responses(messages)?;

    extra_params.remove("previous_response_id");
    extra_params.remove("background");
    extra_params.remove("store");
    extra_params.remove("include");
    extra_params.remove("context_management");
    extra_params.remove("stream");
    extra_params.remove("max_completion_tokens");

    Ok(OpenAiResponsesInputTokensRequest {
        model,
        instructions: normalize_responses_instructions(system_prompt),
        input,
        tools: response_tools,
        tool_choice,
        reasoning,
        extra_params,
    })
}

pub(crate) fn convert_messages_for_openai_responses(
    messages: Vec<LlmMessage>,
) -> Result<Vec<OpenAiResponsesInputItem>> {
    let mut input = Vec::new();

    for message in messages {
        match message.role {
            MessageRole::System | MessageRole::Context | MessageRole::User => {
                if let Some(content) = responses_content(message.content, message.content_parts)? {
                    let role = match message.role {
                        MessageRole::User => "user",
                        _ => "developer",
                    };
                    input.push(OpenAiResponsesInputItem::Message(
                        OpenAiResponsesInputMessage {
                            role: role.to_string(),
                            content,
                        },
                    ));
                }
            }
            MessageRole::Assistant => {
                if let Some(signature) = message
                    .thinking_signature
                    .filter(|value| is_non_empty(value))
                {
                    input.push(OpenAiResponsesInputItem::Reasoning(
                        OpenAiResponsesReasoningInputItem {
                            kind: "reasoning".to_string(),
                            encrypted_content: signature,
                        },
                    ));
                }

                if let Some(content) = responses_output(message.content, message.content_parts)? {
                    input.push(OpenAiResponsesInputItem::Message(
                        OpenAiResponsesInputMessage {
                            role: "assistant".to_string(),
                            content,
                        },
                    ));
                }

                if let Some(tool_calls) = message.tool_calls {
                    for tool_call in tool_calls {
                        let call_id = tool_call.id.unwrap_or_default();
                        if call_id.is_empty() {
                            warn!(
                                tool_name = %tool_call.name,
                                "Skipping assistant tool call without id in Responses API projection"
                            );
                            continue;
                        }

                        input.push(OpenAiResponsesInputItem::FunctionCall(
                            OpenAiResponsesFunctionCallItem {
                                kind: "function_call".to_string(),
                                call_id,
                                name: tool_call.name,
                                arguments: tool_call.arguments.to_string(),
                            },
                        ));
                    }
                }
            }
            MessageRole::Tool => {
                let output = plain_text_content(
                    message.content,
                    message.content_parts,
                    "OpenAI Responses tool output",
                )?;
                let Some(call_id) = message.tool_call_id.filter(|value| is_non_empty(value)) else {
                    warn!("Skipping tool message without tool_call_id in Responses API projection");
                    continue;
                };

                input.push(OpenAiResponsesInputItem::FunctionCallOutput(
                    OpenAiResponsesFunctionCallOutputItem {
                        kind: "function_call_output".to_string(),
                        call_id,
                        output,
                    },
                ));
            }
        }
    }

    Ok(input)
}

#[cfg(test)]
fn build_chat_completions_request_for_model(
    model: String,
    request: GenerationRequest,
) -> OpenAiChatCompletionsRequest {
    let GenerationRequest {
        system_prompt,
        messages: request_messages,
        tools: request_tools,
        temperature,
        max_tokens,
        reasoning,
        mut extra_params,
    } = request;

    let mut messages: Vec<serde_json::Value> = Vec::new();
    if let Some(system) = system_prompt {
        messages.push(openai_chat_completions_message_value(
            "developer",
            Some(serde_json::Value::String(system)),
            None,
            None,
            None,
            None,
        ));
    }
    messages.extend(
        convert_messages_for_openai_chat_completions_with_instruction_role(
            request_messages,
            "developer",
        )
        .expect("test request should have representable Chat Completions content"),
    );

    let (tools, tool_choice) = convert_tools_for_openai_chat_completions(request_tools);
    let reasoning_effort = build_reasoning_effort(reasoning.effort, &mut extra_params);
    let max_completion_tokens = build_max_completion_tokens(max_tokens, &mut extra_params);

    OpenAiChatCompletionsRequest {
        model,
        messages,
        tools,
        tool_choice,
        temperature,
        max_completion_tokens,
        reasoning_effort,
        stream: Some(false),
        stream_options: None,
        extra_params,
    }
}

fn build_openai_responses_reasoning(
    reasoning_effort: Option<ReasoningEffort>,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<OpenAiResponsesReasoning> {
    build_reasoning_effort(reasoning_effort, extra_params)
        .map(|effort| OpenAiResponsesReasoning { effort })
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

pub(crate) fn convert_openai_responses_output(
    response: OpenAiResponsesResponse,
) -> GenerationResponse {
    let OpenAiResponsesResponse {
        id,
        status,
        background: _background,
        output,
        usage,
    } = response;
    let mut content = String::new();
    let mut thinking_parts = Vec::new();
    let mut thinking_signature = None;
    let mut tool_calls = Vec::new();
    let mut warnings = Vec::new();

    for item in output {
        match item.get("type").and_then(serde_json::Value::as_str) {
            Some("message") => {
                if let Some(parts) = item.get("content").and_then(serde_json::Value::as_array) {
                    for part in parts {
                        match part.get("type").and_then(serde_json::Value::as_str) {
                            Some("output_text") => {
                                if let Some(text) =
                                    part.get("text").and_then(serde_json::Value::as_str)
                                {
                                    content.push_str(text);
                                }
                            }
                            Some("refusal") => {
                                if let Some(text) = part
                                    .get("refusal")
                                    .and_then(serde_json::Value::as_str)
                                    .or_else(|| {
                                        part.get("text").and_then(serde_json::Value::as_str)
                                    })
                                {
                                    content.push_str(text);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("reasoning") => {
                if let Some(reasoning) = extract_reasoning_text_from_value(&item)
                    && !reasoning.is_empty()
                {
                    thinking_parts.push(reasoning);
                }
                if let Some(signature) = extract_reasoning_signature(Some(&item)) {
                    thinking_signature = Some(signature);
                }
            }
            Some("function_call") => {
                let Some(name) = item
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| is_non_empty(value))
                else {
                    continue;
                };
                let arguments_raw = item
                    .get("arguments")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("{}");

                match serde_json::from_str::<serde_json::Value>(arguments_raw) {
                    Ok(arguments) => tool_calls.push(LlmToolCall {
                        id: item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned),
                        name: name.to_string(),
                        arguments,
                    }),
                    Err(err) => {
                        warn!(
                            tool_name = %name,
                            error = %err,
                            "Dropping malformed Responses API tool call arguments"
                        );
                        warnings.push(format!(
                            "Dropped malformed Responses API tool call `{name}` arguments."
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    GenerationResponse {
        content,
        thinking: if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join("\n"))
        },
        thinking_signature,
        redacted_thinking: Vec::new(),
        finish_reason: Some(responses_finish_reason(!tool_calls.is_empty()).to_string()),
        tool_calls,
        usage: usage.map(convert_openai_responses_usage),
        provider_response_id: id,
        provider_response_status: status,
        warnings,
    }
}

fn convert_openai_chat_completions_response(
    response: OpenAiChatCompletionsResponse,
) -> Result<GenerationResponse> {
    let choice = select_primary_choice(&response.choices).context("No choices in response")?;
    let message = &choice.message;

    let mut response_warnings: Vec<String> = Vec::new();
    let tool_calls: Vec<LlmToolCall> = message
        .tool_calls
        .as_ref()
        .map(|calls| {
            let mut parsed_calls = Vec::new();
            for call in calls {
                match serde_json::from_str::<serde_json::Value>(&call.function.arguments) {
                    Ok(args) => parsed_calls.push(LlmToolCall {
                        id: Some(call.id.clone()),
                        name: call.function.name.clone(),
                        arguments: args,
                    }),
                    Err(err) => {
                        warn!(
                            tool_name = %call.function.name,
                            error = %err,
                            "Dropping malformed non-streaming tool call arguments"
                        );
                        response_warnings.push(format!(
                            "Dropped malformed non-streaming tool call `{}` arguments.",
                            call.function.name
                        ));
                    }
                }
            }
            parsed_calls
        })
        .unwrap_or_default();

    let usage = response.usage.map(convert_usage);
    let (thinking, thinking_signature) = extract_reasoning_fields(
        message.reasoning_content.as_deref(),
        message.reasoning.as_ref(),
    );

    Ok(GenerationResponse {
        content: message.content.clone().unwrap_or_default(),
        thinking,
        thinking_signature,
        redacted_thinking: Vec::new(),
        tool_calls,
        usage,
        finish_reason: choice.finish_reason.clone(),
        warnings: response_warnings,
        provider_response_id: Some(response.id.clone()),
        provider_response_status: None,
    })
}

fn responses_finish_reason(saw_tool_calls: bool) -> &'static str {
    if saw_tool_calls { "tool_calls" } else { "stop" }
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

fn is_valid_reasoning_effort(effort: &str) -> bool {
    matches!(
        effort,
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
    )
}

fn take_string_extra_param(
    key: &str,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<String> {
    let value = extra_params.remove(key)?;
    match value {
        serde_json::Value::String(value) if is_non_empty(&value) => Some(value),
        other => {
            debug!(key, value = %other, "Ignoring non-string or empty Responses extra_param");
            None
        }
    }
}

fn take_bool_extra_param(
    key: &str,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<bool> {
    let value = extra_params.remove(key)?;
    match value {
        serde_json::Value::Bool(value) => Some(value),
        other => {
            debug!(key, value = %other, "Ignoring non-boolean Responses extra_param");
            None
        }
    }
}

fn take_string_array_extra_param(
    key: &str,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<Vec<String>> {
    let value = extra_params.remove(key)?;
    match value {
        serde_json::Value::Array(values) => {
            let collected: Vec<String> = values
                .into_iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .filter(|value| is_non_empty(value))
                .collect();
            if collected.is_empty() {
                None
            } else {
                Some(collected)
            }
        }
        other => {
            debug!(key, value = %other, "Ignoring non-array Responses extra_param");
            None
        }
    }
}

fn should_include_reasoning_encrypted_content(
    messages: &[LlmMessage],
    tools: &[LlmToolDefinition],
    reasoning_requested: bool,
) -> bool {
    reasoning_requested
        || !tools.is_empty()
        || messages.iter().any(|message| {
            message
                .thinking_signature
                .as_deref()
                .is_some_and(is_non_empty)
        })
}

fn normalize_responses_include(
    include: Option<Vec<String>>,
    require_reasoning_encrypted_content: bool,
) -> Option<Vec<String>> {
    let mut include = include.unwrap_or_default();
    if require_reasoning_encrypted_content
        && !include
            .iter()
            .any(|value| value == "reasoning.encrypted_content")
    {
        include.push("reasoning.encrypted_content".to_string());
    }
    if include.is_empty() {
        None
    } else {
        Some(include)
    }
}

pub(crate) fn extract_responses_output_reasoning_signature(
    output: &[serde_json::Value],
) -> Option<String> {
    output
        .iter()
        .filter(|item| item.get("type").and_then(serde_json::Value::as_str) == Some("reasoning"))
        .filter_map(|item| extract_reasoning_signature(Some(item)))
        .next_back()
}

pub(crate) fn is_non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

// Streamed text can arrive as standalone spaces/newlines that preserve formatting.
fn responses_stream_text_delta(event: &serde_json::Value) -> Option<&str> {
    event
        .get("delta")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
}

fn extract_reasoning_text_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) if is_non_empty(text) => Some(text.clone()),
        serde_json::Value::Object(map) => {
            for key in ["content", "text"] {
                if let Some(text) = map
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .filter(|text| is_non_empty(text))
                {
                    return Some(text.to_string());
                }
            }

            for key in ["content", "summary"] {
                if let Some(serde_json::Value::Array(items)) = map.get(key) {
                    let mut joined = String::new();
                    for item in items {
                        if let Some(text) = item.as_str().filter(|text| is_non_empty(text)) {
                            joined.push_str(text);
                        } else if let Some(text) = item
                            .get("text")
                            .and_then(serde_json::Value::as_str)
                            .filter(|text| is_non_empty(text))
                        {
                            joined.push_str(text);
                        } else if let Some(text) = item
                            .get("content")
                            .and_then(serde_json::Value::as_str)
                            .filter(|text| is_non_empty(text))
                        {
                            joined.push_str(text);
                        }
                    }
                    if !joined.is_empty() {
                        return Some(joined);
                    }
                }
            }

            None
        }
        _ => None,
    }
}

fn extract_reasoning_signature(reasoning: Option<&serde_json::Value>) -> Option<String> {
    reasoning.and_then(|value| match value {
        serde_json::Value::Object(map) => map
            .get("encrypted_content")
            .and_then(serde_json::Value::as_str)
            .filter(|value| is_non_empty(value))
            .map(ToString::to_string),
        _ => None,
    })
}

fn extract_reasoning_fields(
    reasoning_content: Option<&str>,
    reasoning: Option<&serde_json::Value>,
) -> (Option<String>, Option<String>) {
    let thinking = reasoning_content
        .filter(|value| is_non_empty(value))
        .map(ToString::to_string)
        .or_else(|| reasoning.and_then(extract_reasoning_text_from_value));

    let thinking_signature = extract_reasoning_signature(reasoning);

    (thinking, thinking_signature)
}

pub(crate) fn build_reasoning_effort(
    reasoning_effort: Option<ReasoningEffort>,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<String> {
    if let Some(effort) = reasoning_effort {
        extra_params.remove("reasoning_effort");
        return Some(effort.as_str().to_string());
    }

    if let Some(value) = extra_params.remove("reasoning_effort") {
        if let Some(effort) = value.as_str() {
            if is_valid_reasoning_effort(effort) {
                return Some(effort.to_string());
            }
            debug!(
                effort,
                "Ignoring invalid `reasoning_effort`; expected one of: none, minimal, low, medium, high, xhigh"
            );
        } else {
            debug!(
                value = %value,
                "Ignoring non-string `reasoning_effort` in extra_params"
            );
        }
    }

    None
}

pub(crate) fn build_max_completion_tokens(
    max_tokens: Option<i32>,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<i32> {
    if let Some(value) = extra_params.remove("max_completion_tokens") {
        if let Some(tokens) = value.as_i64() {
            return i32::try_from(tokens).ok();
        }
        debug!(
            value = %value,
            "Ignoring non-integer `max_completion_tokens` in extra_params"
        );
    }
    max_tokens
}

fn convert_usage(usage: OpenAiChatCompletionsUsage) -> TokenUsage {
    TokenUsage {
        prompt_tokens: usage.prompt_tokens,
        cached_prompt_tokens: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens),
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        reasoning_tokens: usage
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens),
    }
}

fn allocate_stream_tool_index(
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

fn select_stream_choice_index(
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

fn select_primary_choice(
    choices: &[OpenAiChatCompletionsChoice],
) -> Option<&OpenAiChatCompletionsChoice> {
    choices
        .iter()
        .find(|choice| choice.index == 0)
        .or_else(|| choices.first())
}

#[async_trait]
impl LlmProvider for OpenAiChatCompletionsClient {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        self.generate_via_openai_chat_completions(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> anyhow::Result<String> {
        // Directly use the existing chat method
        self.chat(system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.generate_stream_via_openai_chat_completions(request)
            .await
    }

    fn provider_name(&self) -> &'static str {
        match self.api_flavor {
            OpenAiChatCompletionsApiFlavor::Official => "openai_chat_completions",
            OpenAiChatCompletionsApiFlavor::Compatible => "openai_chat_completions_compatible",
        }
    }
}

impl OpenAiChatCompletionsClient {
    async fn generate_via_openai_chat_completions(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<GenerationResponse> {
        reject_retired_message_overrides(&request)?;
        let GenerationRequest {
            system_prompt,
            messages: request_messages,
            tools: request_tools,
            temperature,
            max_tokens,
            reasoning,
            mut extra_params,
        } = request;

        let mut messages: Vec<serde_json::Value> = Vec::new();
        if let Some(system) = system_prompt {
            messages.push(openai_chat_completions_message_value(
                self.instruction_role_name(),
                Some(serde_json::Value::String(system)),
                None,
                None,
                None,
                None,
            ));
        }
        messages.extend(
            convert_messages_for_openai_chat_completions_with_instruction_role(
                request_messages,
                self.instruction_role_name(),
            )?,
        );

        let (tools, tool_choice) = convert_tools_for_openai_chat_completions(request_tools);
        let reasoning_effort = build_reasoning_effort(reasoning.effort, &mut extra_params);
        let max_completion_tokens = build_max_completion_tokens(max_tokens, &mut extra_params);

        let chat_request = OpenAiChatCompletionsRequest {
            model: self.model.clone(),
            messages,
            tools,
            tool_choice,
            temperature,
            max_completion_tokens,
            reasoning_effort,
            stream: Some(false),
            stream_options: None,
            extra_params,
        };

        let response = self.openai_chat_completions(chat_request).await?;
        convert_openai_chat_completions_response(response)
    }

    async fn generate_stream_via_openai_chat_completions(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        reject_retired_message_overrides(&request)?;
        let GenerationRequest {
            system_prompt,
            messages: request_messages,
            tools: request_tools,
            temperature,
            max_tokens,
            reasoning,
            mut extra_params,
        } = request;

        let mut messages: Vec<serde_json::Value> = Vec::new();
        if let Some(system) = system_prompt {
            messages.push(openai_chat_completions_message_value(
                self.instruction_role_name(),
                Some(serde_json::Value::String(system)),
                None,
                None,
                None,
                None,
            ));
        }
        messages.extend(
            convert_messages_for_openai_chat_completions_with_instruction_role(
                request_messages,
                self.instruction_role_name(),
            )?,
        );

        let (tools, tool_choice) = convert_tools_for_openai_chat_completions(request_tools);
        let reasoning_effort = build_reasoning_effort(reasoning.effort, &mut extra_params);
        let max_completion_tokens = build_max_completion_tokens(max_tokens, &mut extra_params);

        let chat_request = OpenAiChatCompletionsRequest {
            model: self.model.clone(),
            messages,
            tools,
            tool_choice,
            temperature,
            max_completion_tokens,
            reasoning_effort,
            stream: Some(true),
            stream_options: Some(OpenAiChatCompletionsStreamOptions {
                include_usage: true,
            }),
            extra_params,
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_client_with_params() {
        let client = OpenAiChatCompletionsClient::official_with_params(
            "test-key",
            "https://api.openai.com/v1",
            "gpt-5.4",
        );
        assert_eq!(client.provider_name(), "openai_chat_completions");
        drop(client);
    }

    #[test]
    fn test_openai_chat_completions_compatible_client_with_params() {
        let client = OpenAiChatCompletionsClient::compatible_with_params(
            "test-key",
            "https://proxy.example/v1",
            "qwen3.5-plus",
        );
        assert_eq!(client.provider_name(), "openai_chat_completions_compatible");
        drop(client);
    }

    #[test]
    fn test_chat_message_system() {
        let msg = OpenAiChatCompletionsMessage::system("You are a helpful assistant");
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, Some("You are a helpful assistant".to_string()));
    }

    #[test]
    fn test_chat_message_user() {
        let msg = OpenAiChatCompletionsMessage::user("Hello!");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, Some("Hello!".to_string()));
    }

    #[test]
    fn test_chat_message_assistant() {
        let msg = OpenAiChatCompletionsMessage::assistant("Hi there!");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, Some("Hi there!".to_string()));
    }

    #[test]
    fn test_chat_message_tool() {
        let msg = OpenAiChatCompletionsMessage::tool("call-123", "Tool result");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.content, Some("Tool result".to_string()));
        assert_eq!(msg.tool_call_id, Some("call-123".to_string()));
    }

    #[test]
    fn test_tool_definition_new() {
        let params = serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"]
        });

        let tool = OpenAiChatCompletionsToolDefinition::new(
            "web_search",
            "Search the web",
            params.clone(),
        );
        assert_eq!(tool.r#type, "function");
        assert_eq!(tool.function.name, "web_search");
        assert_eq!(tool.function.description, "Search the web");
        assert_eq!(tool.function.parameters, params);
    }

    #[test]
    fn test_chat_completion_request_serialization() {
        let request = OpenAiChatCompletionsRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                openai_chat_completions_message_value(
                    "system",
                    Some(serde_json::Value::String("Be helpful".to_string())),
                    None,
                    None,
                    None,
                    None,
                ),
                openai_chat_completions_message_value(
                    "user",
                    Some(serde_json::Value::String("Hello".to_string())),
                    None,
                    None,
                    None,
                    None,
                ),
            ],
            tools: None,
            tool_choice: None,
            temperature: Some(0.7),
            max_completion_tokens: Some(100),
            reasoning_effort: None,
            stream: Some(false),
            stream_options: None,
            extra_params: HashMap::new(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("model"));
        assert!(json.contains("gpt-4"));
        assert!(json.contains("messages"));
        assert!(json.contains("temperature"));
    }

    #[test]
    fn test_chat_completion_request_with_tools() {
        let tool = OpenAiChatCompletionsToolDefinition::new(
            "search",
            "Search",
            serde_json::json!({"type": "object"}),
        );

        let request = OpenAiChatCompletionsRequest {
            model: "gpt-4".to_string(),
            messages: vec![openai_chat_completions_message_value(
                "user",
                Some(serde_json::Value::String(
                    "Search for something".to_string(),
                )),
                None,
                None,
                None,
                None,
            )],
            tools: Some(vec![tool]),
            tool_choice: Some("auto".to_string()),
            temperature: None,
            max_completion_tokens: None,
            reasoning_effort: None,
            stream: None,
            stream_options: None,
            extra_params: HashMap::new(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("tools"));
        assert!(json.contains("tool_choice"));
        assert!(json.contains("auto"));
    }

    #[test]
    fn test_chat_completion_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        }"#;

        let response: OpenAiChatCompletionsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "gpt-4");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.usage.as_ref().unwrap().total_tokens, 30);
    }

    #[test]
    fn test_responses_stream_text_delta_preserves_whitespace_only_chunks() {
        let event = serde_json::json!({
            "type": "response.output_text.delta",
            "delta": " ",
        });
        assert_eq!(responses_stream_text_delta(&event), Some(" "));

        let empty_event = serde_json::json!({
            "type": "response.output_text.delta",
            "delta": "",
        });
        assert_eq!(responses_stream_text_delta(&empty_event), None);
    }

    #[test]
    fn test_chat_completion_response_with_tool_calls() {
        let json = r#"{
            "id": "chatcmpl-456",
            "object": "chat.completion",
            "created": 1677652289,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call-123",
                        "type": "function",
                        "function": {
                            "name": "web_search",
                            "arguments": "{\"query\": \"test\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": null
        }"#;

        let response: OpenAiChatCompletionsResponse = serde_json::from_str(json).unwrap();
        let message = &response.choices[0].message;
        assert!(message.content.is_none());
        assert!(message.tool_calls.is_some());
        let tool_calls = message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].id, "call-123");
        assert_eq!(tool_calls[0].function.name, "web_search");
    }

    #[test]
    fn test_chat_completion_response_with_reasoning_tokens() {
        let json = r#"{
            "id": "chatcmpl-rsn",
            "object": "chat.completion",
            "created": 1677652289,
            "model": "deepseek-reasoner",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Final answer"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 11,
                "completion_tokens": 22,
                "total_tokens": 33,
                "completion_tokens_details": {
                    "reasoning_tokens": 7
                }
            }
        }"#;

        let response: OpenAiChatCompletionsResponse = serde_json::from_str(json).unwrap();
        let usage = response.usage.unwrap();
        assert_eq!(
            usage.completion_tokens_details.unwrap().reasoning_tokens,
            Some(7)
        );
    }

    #[test]
    fn test_convert_openai_chat_completions_response_propagates_id_and_finish_reason() {
        let response = OpenAiChatCompletionsResponse {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1,
            model: "gpt-5.4".to_string(),
            choices: vec![OpenAiChatCompletionsChoice {
                index: 0,
                message: OpenAiChatCompletionsMessage::assistant("Hello!"),
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };

        let converted = convert_openai_chat_completions_response(response).unwrap();
        assert_eq!(converted.content, "Hello!");
        assert_eq!(converted.finish_reason.as_deref(), Some("stop"));
        assert_eq!(
            converted.provider_response_id.as_deref(),
            Some("chatcmpl-123")
        );
    }

    #[test]
    fn test_chat_completion_response_deserialization_with_reasoning_content() {
        let json = r#"{
            "id": "chatcmpl-rsn-content",
            "object": "chat.completion",
            "created": 1677652290,
            "model": "deepseek-reasoner",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Final answer",
                    "reasoning_content": "Internal reasoning trail"
                },
                "finish_reason": "stop"
            }],
            "usage": null
        }"#;

        let response: OpenAiChatCompletionsResponse = serde_json::from_str(json).unwrap();
        let message = &response.choices[0].message;
        assert_eq!(
            message.reasoning_content.as_deref(),
            Some("Internal reasoning trail")
        );
    }

    #[test]
    fn test_chat_completion_chunk_deserialization() {
        let json = r#"{
            "id": "chatcmpl-789",
            "object": "chat.completion.chunk",
            "created": 1677652290,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "content": "Hello"
                },
                "finish_reason": null
            }]
        }"#;

        let chunk: OpenAiChatCompletionsChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.id, "chatcmpl-789");
        assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_chat_completion_chunk_deserialization_with_reasoning_content() {
        let json = r#"{
            "id": "chatcmpl-791",
            "object": "chat.completion.chunk",
            "created": 1677652292,
            "model": "deepseek-reasoner",
            "choices": [{
                "index": 0,
                "delta": {
                    "reasoning_content": "Thinking..."
                },
                "finish_reason": null
            }]
        }"#;

        let chunk: OpenAiChatCompletionsChunk = serde_json::from_str(json).unwrap();
        assert_eq!(
            chunk.choices[0].delta.reasoning_content.as_deref(),
            Some("Thinking...")
        );
    }

    #[test]
    fn test_chat_completion_chunk_deserialization_with_usage() {
        let json = r#"{
            "id": "chatcmpl-790",
            "object": "chat.completion.chunk",
            "created": 1677652291,
            "model": "deepseek-reasoner",
            "choices": [],
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 2,
                "total_tokens": 3,
                "completion_tokens_details": {
                    "reasoning_tokens": 1
                }
            }
        }"#;

        let chunk: OpenAiChatCompletionsChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 0);
        assert_eq!(
            chunk
                .usage
                .and_then(|u| u.completion_tokens_details)
                .and_then(|d| d.reasoning_tokens),
            Some(1)
        );
    }

    #[test]
    fn test_usage_deserialization() {
        let json = r#"{
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        }"#;

        let usage: OpenAiChatCompletionsUsage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_function_definition() {
        let func = OpenAiChatCompletionsFunctionDefinition {
            name: "test_func".to_string(),
            description: "Test function".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };

        assert_eq!(func.name, "test_func");
        assert_eq!(func.description, "Test function");
    }

    #[test]
    fn test_function_call() {
        let fc = OpenAiChatCompletionsFunctionCall {
            name: "my_func".to_string(),
            arguments: "{\"arg\": 123}".to_string(),
        };

        assert_eq!(fc.name, "my_func");
        assert_eq!(fc.arguments, "{\"arg\": 123}");
    }

    #[test]
    fn test_delta_message_default() {
        let delta: OpenAiChatCompletionsDeltaMessage = Default::default();
        assert!(delta.role.is_none());
        assert!(delta.content.is_none());
        assert!(delta.reasoning_content.is_none());
        assert!(delta.reasoning.is_none());
        assert!(delta.tool_calls.is_none());
    }

    #[test]
    fn test_build_reasoning_effort_prefers_canonical_effort() {
        let mut extra_params = HashMap::from([(
            "reasoning_effort".to_string(),
            serde_json::Value::String("high".to_string()),
        )]);

        let effort = build_reasoning_effort(Some(ReasoningEffort::Low), &mut extra_params);
        assert_eq!(effort.as_deref(), Some("low"));
        assert!(!extra_params.contains_key("reasoning_effort"));
    }

    #[test]
    fn test_build_reasoning_effort_accepts_compat_extra_params() {
        let mut extra_params = HashMap::from([(
            "reasoning_effort".to_string(),
            serde_json::Value::String("high".to_string()),
        )]);

        let effort = build_reasoning_effort(None, &mut extra_params);
        assert_eq!(effort.as_deref(), Some("high"));
        assert!(!extra_params.contains_key("reasoning_effort"));
    }

    #[test]
    fn test_build_reasoning_effort_accepts_extended_values() {
        let mut extra_params = HashMap::from([(
            "reasoning_effort".to_string(),
            serde_json::Value::String("xhigh".to_string()),
        )]);
        let effort = build_reasoning_effort(None, &mut extra_params);
        assert_eq!(effort.as_deref(), Some("xhigh"));
        assert!(!extra_params.contains_key("reasoning_effort"));
    }

    #[test]
    fn test_build_responses_request_uses_canonical_reasoning_effort() {
        let request = GenerationRequest::new()
            .with_user_message("Hello")
            .with_extra_param("reasoning_effort", serde_json::json!("high"))
            .with_reasoning_effort(ReasoningEffort::Low);

        let built =
            build_responses_request_for_model("gpt-5.4".to_string(), request, false).unwrap();

        assert_eq!(
            built.reasoning.map(|reasoning| reasoning.effort),
            Some("low".to_string())
        );
        assert!(!built.extra_params.contains_key("reasoning_effort"));
    }

    #[test]
    fn test_build_chat_completions_request_uses_canonical_reasoning_effort() {
        let request = GenerationRequest::new()
            .with_user_message("Hello")
            .with_extra_param("reasoning_effort", serde_json::json!("high"))
            .with_reasoning_effort(ReasoningEffort::Low);

        let built = build_chat_completions_request_for_model("gpt-5.4".to_string(), request);

        assert_eq!(built.reasoning_effort.as_deref(), Some("low"));
        assert!(!built.extra_params.contains_key("reasoning_effort"));
    }

    #[test]
    fn test_instruction_role_name_differs_by_api_flavor() {
        let official = OpenAiChatCompletionsClient::official_with_params(
            "sk-test",
            "https://api.openai.com/v1",
            "gpt-5.4",
        );
        let compatible = OpenAiChatCompletionsClient::compatible_with_params(
            "sk-test",
            "https://proxy.example/v1",
            "qwen3.5-plus",
        );

        assert_eq!(official.instruction_role_name(), "developer");
        assert_eq!(compatible.instruction_role_name(), "system");
    }

    #[test]
    fn test_convert_messages_for_openai_preserves_assistant_reasoning_content() {
        let messages = vec![crate::Message {
            role: MessageRole::Assistant,
            content: "Done".to_string(),
            content_parts: Vec::new(),
            thinking: Some("step by step".to_string()),
            thinking_signature: Some("encrypted_state".to_string()),
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        }];

        let converted = convert_messages_for_openai_chat_completions(messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(
            converted[0]
                .get("reasoning_content")
                .and_then(serde_json::Value::as_str),
            Some("step by step")
        );
        assert_eq!(
            converted[0]
                .get("reasoning")
                .and_then(|value| value.get("encrypted_content"))
                .and_then(serde_json::Value::as_str),
            Some("encrypted_state")
        );
    }

    #[test]
    fn test_build_max_completion_tokens_prefers_extra_params() {
        let mut extra_params = HashMap::from([(
            "max_completion_tokens".to_string(),
            serde_json::Value::Number(serde_json::Number::from(1234)),
        )]);

        let max_tokens = build_max_completion_tokens(Some(100), &mut extra_params);
        assert_eq!(max_tokens, Some(1234));
        assert!(!extra_params.contains_key("max_completion_tokens"));
    }

    #[test]
    fn test_convert_usage_extracts_reasoning_tokens() {
        let usage = OpenAiChatCompletionsUsage {
            prompt_tokens: 10,
            prompt_tokens_details: None,
            completion_tokens: 20,
            total_tokens: 30,
            completion_tokens_details: Some(OpenAiChatCompletionsCompletionTokensDetails {
                reasoning_tokens: Some(7),
                audio_tokens: None,
                accepted_prediction_tokens: None,
                rejected_prediction_tokens: None,
            }),
        };

        let token_usage = convert_usage(usage);
        assert_eq!(token_usage.prompt_tokens, 10);
        assert_eq!(token_usage.completion_tokens, 20);
        assert_eq!(token_usage.total_tokens, 30);
        assert_eq!(token_usage.reasoning_tokens, Some(7));
    }

    #[test]
    fn test_convert_usage_extracts_cached_prompt_tokens() {
        let usage = OpenAiChatCompletionsUsage {
            prompt_tokens: 10,
            prompt_tokens_details: Some(OpenAiChatCompletionsPromptTokensDetails {
                cached_tokens: Some(6),
                audio_tokens: None,
            }),
            completion_tokens: 20,
            total_tokens: 30,
            completion_tokens_details: None,
        };

        let token_usage = convert_usage(usage);
        assert_eq!(token_usage.cached_prompt_tokens, Some(6));
    }

    #[test]
    fn test_allocate_stream_tool_index_is_stable_per_choice_and_tool_index() {
        let mut tool_index_map = HashMap::new();
        let mut next_tool_index = 0usize;

        let first = allocate_stream_tool_index(&mut tool_index_map, &mut next_tool_index, 0, 0);
        let first_repeat =
            allocate_stream_tool_index(&mut tool_index_map, &mut next_tool_index, 0, 0);
        let second = allocate_stream_tool_index(&mut tool_index_map, &mut next_tool_index, 1, 0);
        let third = allocate_stream_tool_index(&mut tool_index_map, &mut next_tool_index, 1, 1);

        assert_eq!(first, first_repeat);
        assert_ne!(first, second);
        assert_ne!(second, third);
        assert_eq!(next_tool_index, 3);
    }

    #[test]
    fn test_select_stream_choice_index_prefers_zero_then_falls_back() {
        let choices_non_zero = vec![
            OpenAiChatCompletionsChunkChoice {
                index: 2,
                delta: OpenAiChatCompletionsDeltaMessage::default(),
                finish_reason: None,
            },
            OpenAiChatCompletionsChunkChoice {
                index: 3,
                delta: OpenAiChatCompletionsDeltaMessage::default(),
                finish_reason: None,
            },
        ];
        assert_eq!(
            select_stream_choice_index(None, false, &choices_non_zero),
            Some(2)
        );

        let choices_zero = vec![
            OpenAiChatCompletionsChunkChoice {
                index: 5,
                delta: OpenAiChatCompletionsDeltaMessage::default(),
                finish_reason: None,
            },
            OpenAiChatCompletionsChunkChoice {
                index: 0,
                delta: OpenAiChatCompletionsDeltaMessage::default(),
                finish_reason: None,
            },
        ];
        assert_eq!(
            select_stream_choice_index(None, false, &choices_zero),
            Some(0)
        );

        // If no payload has been emitted yet, switch to index=0 when it appears.
        assert_eq!(
            select_stream_choice_index(Some(2), false, &choices_zero),
            Some(0)
        );
        // Once payload has been emitted, keep stable selection to avoid mixed output.
        assert_eq!(
            select_stream_choice_index(Some(2), true, &choices_zero),
            Some(2)
        );
    }

    #[test]
    fn test_select_primary_choice_prefers_index_zero() {
        let choices = vec![
            OpenAiChatCompletionsChoice {
                index: 1,
                message: OpenAiChatCompletionsMessage::assistant("secondary"),
                finish_reason: Some("stop".to_string()),
            },
            OpenAiChatCompletionsChoice {
                index: 0,
                message: OpenAiChatCompletionsMessage::assistant("primary"),
                finish_reason: Some("stop".to_string()),
            },
        ];
        let selected = select_primary_choice(&choices).expect("expected choice");
        assert_eq!(selected.index, 0);
        assert_eq!(selected.message.content.as_deref(), Some("primary"));
    }

    #[test]
    fn test_stream_tool_call_deserialization() {
        let json = r#"{
            "index": 0,
            "id": "call-123",
            "type": "function",
            "function": {
                "name": "web_search",
                "arguments": "{\"query\": \"test\"}"
            }
        }"#;

        let call: OpenAiChatCompletionsStreamToolCall = serde_json::from_str(json).unwrap();
        assert_eq!(call.index, 0);
        assert_eq!(call.id, Some("call-123".to_string()));
        assert_eq!(call.r#type, Some("function".to_string()));
        assert!(call.function.is_some());
    }

    #[test]
    fn test_stream_function_call_deserialization() {
        let json = r#"{
            "name": "my_func",
            "arguments": "{\"key\": \"value\"}"
        }"#;

        let func: OpenAiChatCompletionsStreamFunctionCall = serde_json::from_str(json).unwrap();
        assert_eq!(func.name, Some("my_func".to_string()));
        assert_eq!(func.arguments, Some("{\"key\": \"value\"}".to_string()));
    }
}
