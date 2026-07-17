//! OpenAI Chat Completions API client.
//!
//! Supports the official OpenAI Chat Completions API and generic OpenAI Chat
//! Completions API-compatible endpoints.

use anyhow::{Context, Result};
#[cfg(test)]
use std::collections::HashMap;
use tracing::{debug, instrument, warn};

use crate::{
    GenerationRequest, GenerationResponse, LlmProvider, StreamChunk, TokenUsage,
    ToolCall as LlmToolCall,
};
#[cfg(test)]
use crate::{MessageRole, ReasoningEffort};
use async_trait::async_trait;

mod input_projection;
mod request_projection;
mod streaming;
mod wire;

pub use wire::{
    OpenAiChatCompletionsChoice, OpenAiChatCompletionsChunk, OpenAiChatCompletionsChunkChoice,
    OpenAiChatCompletionsCompletionTokensDetails, OpenAiChatCompletionsDeltaMessage,
    OpenAiChatCompletionsFunctionCall, OpenAiChatCompletionsFunctionDefinition,
    OpenAiChatCompletionsMessage, OpenAiChatCompletionsPromptTokensDetails,
    OpenAiChatCompletionsRequest, OpenAiChatCompletionsResponse,
    OpenAiChatCompletionsStreamFunctionCall, OpenAiChatCompletionsStreamOptions,
    OpenAiChatCompletionsStreamToolCall, OpenAiChatCompletionsToolCall,
    OpenAiChatCompletionsToolDefinition, OpenAiChatCompletionsUsage, OpenAiResponsesCompactRequest,
    OpenAiResponsesCompactResponse, OpenAiResponsesFunctionCallItem,
    OpenAiResponsesFunctionCallOutputItem, OpenAiResponsesInputItem, OpenAiResponsesInputMessage,
    OpenAiResponsesInputTokensDetails, OpenAiResponsesInputTokensRequest,
    OpenAiResponsesInputTokensResponse, OpenAiResponsesOutputTokensDetails,
    OpenAiResponsesReasoning, OpenAiResponsesReasoningInputItem, OpenAiResponsesRequest,
    OpenAiResponsesResponse, OpenAiResponsesToolDefinition, OpenAiResponsesUsage,
};

#[cfg(test)]
pub(crate) use input_projection::convert_messages_for_openai_responses;
use input_projection::is_non_empty;
#[cfg(test)]
use input_projection::openai_chat_completions_message_value;
use request_projection::build_chat_completions_request_for_model;
pub(crate) use request_projection::build_responses_request_for_model;
#[cfg(test)]
use request_projection::{
    build_max_completion_tokens, build_reasoning_effort,
    build_responses_input_tokens_request_for_model, convert_messages_for_openai_chat_completions,
};
#[cfg(test)]
use streaming::{
    allocate_stream_tool_index, responses_stream_text_delta, select_stream_choice_index,
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
        let mut generation_request = GenerationRequest::new()
            .with_user_message(user_message)
            .with_temperature(0.7)
            .with_max_tokens(2048);
        if let Some(system) = system {
            generation_request = generation_request.with_system_prompt(system);
        }
        let request = build_chat_completions_request_for_model(
            self.model.clone(),
            self.instruction_role_name(),
            generation_request,
            false,
        )?;

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

pub(crate) fn extract_responses_output_reasoning_signature(
    output: &[serde_json::Value],
) -> Option<String> {
    output
        .iter()
        .filter(|item| item.get("type").and_then(serde_json::Value::as_str) == Some("reasoning"))
        .filter_map(|item| extract_reasoning_signature(Some(item)))
        .next_back()
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
        let chat_request = build_chat_completions_request_for_model(
            self.model.clone(),
            self.instruction_role_name(),
            request,
            false,
        )?;

        let response = self.openai_chat_completions(chat_request).await?;
        convert_openai_chat_completions_response(response)
    }
}

#[cfg(test)]
mod tests;
