use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

use crate::ReasoningEffort;
use crate::message::reject_retired_message_overrides;

mod input_projection;
mod streaming;

use input_projection::convert_messages_for_anthropic_messages;

const MIN_THINKING_BUDGET_TOKENS: u32 = 1_024;
const INTERLEAVED_THINKING_BETA: &str = "interleaved-thinking-2025-05-14";
const FILES_API_BETA: &str = "files-api-2025-04-14";

/// Client for the Anthropic Messages API.
pub struct AnthropicMessagesClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    custom_headers: HeaderMap,
}

#[derive(Debug, Serialize)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessagesMessage>,
    pub max_tokens: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicMessagesToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
}

#[derive(Debug, Serialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub config_type: String,
    pub budget_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessagesMessage {
    pub role: String,
    pub content: Vec<ContentBlockInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlockInput {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: serde_json::Value },
    #[serde(rename = "document")]
    Document {
        source: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<serde_json::Value>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: String },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessagesToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicMessagesResponse {
    pub id: String,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    pub usage: Option<Usage>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    pub thinking: Option<String>,
    pub signature: Option<String>,
    pub data: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: i32,
    pub cache_creation_input_tokens: Option<i32>,
    pub cache_read_input_tokens: Option<i32>,
    pub output_tokens: i32,
}

#[derive(Debug, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub index: Option<i32>,
    pub content_block: Option<ContentBlock>,
    pub delta: Option<StreamDelta>,
    pub message: Option<StreamMessage>,
    pub usage: Option<Usage>,
    pub error: Option<StreamError>,
}

#[derive(Debug, Deserialize)]
pub struct StreamDelta {
    #[serde(rename = "type")]
    pub delta_type: Option<String>,
    pub text: Option<String>,
    pub thinking: Option<String>,
    pub signature: Option<String>,
    pub partial_json: Option<String>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamMessage {
    pub id: Option<String>,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct StreamError {
    pub error: Option<serde_json::Value>,
    pub message: Option<String>,
    pub r#type: Option<String>,
}

impl AnthropicMessagesClient {
    pub fn with_params(api_key: &str, base_url: &str, model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            custom_headers: HeaderMap::new(),
        }
    }

    /// Set custom headers to be included in all requests
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        for (key, value) in headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(key.as_bytes()),
                HeaderValue::from_str(&value),
            ) {
                self.custom_headers.insert(name, val);
            }
        }
        self
    }

    /// Set a client name header (for usage tracking)
    pub fn with_client_name(mut self, name: &str) -> Self {
        if let Ok(val) = HeaderValue::from_str(name) {
            self.custom_headers.insert("X-Client-Name", val);
        }
        self
    }

    /// Set User-Agent header
    pub fn with_user_agent(mut self, user_agent: &str) -> Self {
        if let Ok(val) = HeaderValue::from_str(user_agent) {
            self.custom_headers.insert("User-Agent", val);
        }
        self
    }

    pub async fn anthropic_messages(
        &self,
        request: AnthropicMessagesRequest,
    ) -> Result<AnthropicMessagesResponse> {
        self.anthropic_messages_with_headers(request, None).await
    }

    pub async fn anthropic_messages_with_headers(
        &self,
        mut request: AnthropicMessagesRequest,
        extra_headers: Option<&HeaderMap>,
    ) -> Result<AnthropicMessagesResponse> {
        let url = self.anthropic_messages_url();
        if request.model.is_empty() {
            request.model = self.model.clone();
        }

        let mut req_builder = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01");

        // Apply custom headers
        for (name, value) in &self.custom_headers {
            req_builder = req_builder.header(name, value);
        }
        if let Some(headers) = extra_headers {
            for (name, value) in headers {
                req_builder = req_builder.header(name, value);
            }
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .context("Failed to send request to the Anthropic Messages API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic Messages API error ({}): {}", status, error_text);
        }

        let result: AnthropicMessagesResponse = response
            .json()
            .await
            .context("Failed to parse Anthropic Messages API response")?;

        Ok(result)
    }

    pub async fn stream_anthropic_messages(
        &self,
        request: AnthropicMessagesRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        self.stream_anthropic_messages_with_headers(request, tx, None)
            .await
    }

    pub async fn stream_anthropic_messages_with_headers(
        &self,
        mut request: AnthropicMessagesRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
        extra_headers: Option<&HeaderMap>,
    ) -> Result<()> {
        let url = self.anthropic_messages_url();
        if request.model.is_empty() {
            request.model = self.model.clone();
        }
        request.stream = Some(true);

        let mut req_builder = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01");

        // Apply custom headers
        for (name, value) in &self.custom_headers {
            req_builder = req_builder.header(name, value);
        }
        if let Some(headers) = extra_headers {
            for (name, value) in headers {
                req_builder = req_builder.header(name, value);
            }
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to the Anthropic Messages API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Anthropic Messages API streaming error ({}): {}",
                status,
                error_text
            );
        }

        let mut stream = response.bytes_stream();
        let mut parser = SseEventParser::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Failed to read stream chunk")?;
            for data in parser.push(&chunk) {
                if data == "[DONE]" {
                    return Ok(());
                }

                match serde_json::from_str::<StreamEvent>(&data) {
                    Ok(event) => {
                        if tx.send(event).await.is_err() {
                            return Ok(());
                        }
                    }
                    Err(error) => {
                        debug!(?error, data, "Failed to parse stream chunk");
                    }
                }
            }
        }

        for data in parser.finish() {
            if data == "[DONE]" {
                return Ok(());
            }

            match serde_json::from_str::<StreamEvent>(&data) {
                Ok(event) => {
                    if tx.send(event).await.is_err() {
                        return Ok(());
                    }
                }
                Err(error) => {
                    debug!(?error, data, "Failed to parse stream chunk");
                }
            }
        }

        Ok(())
    }

    pub async fn chat(&self, system: Option<&str>, user_message: &str) -> Result<String> {
        let request = AnthropicMessagesRequest {
            model: self.model.clone(),
            system: system.map(ToString::to_string),
            messages: vec![AnthropicMessagesMessage::user_text(user_message)],
            max_tokens: 2048,
            temperature: Some(0.7),
            tools: None,
            stream: None,
            thinking: None,
        };

        let response = self.anthropic_messages(request).await?;
        let text = response
            .content
            .into_iter()
            .filter(|block| block.block_type == "text")
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("");

        Ok(text)
    }

    fn anthropic_messages_url(&self) -> String {
        if self.base_url.ends_with("/v1") {
            format!("{}/messages", self.base_url)
        } else {
            format!("{}/v1/messages", self.base_url)
        }
    }
}

impl AnthropicMessagesMessage {
    pub fn user_text(text: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: vec![ContentBlockInput::Text {
                text: text.to_string(),
            }],
        }
    }

    pub fn assistant_text(text: &str) -> Self {
        Self {
            role: "assistant".to_string(),
            content: vec![ContentBlockInput::Text {
                text: text.to_string(),
            }],
        }
    }

    pub fn user_tool_result(tool_use_id: &str, content: String) -> Self {
        Self {
            role: "user".to_string(),
            content: vec![ContentBlockInput::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content,
                is_error: None,
            }],
        }
    }
}

impl AnthropicMessagesToolDefinition {
    pub fn new(name: &str, description: &str, input_schema: serde_json::Value) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
        }
    }
}

use crate::{
    GenerationRequest, GenerationResponse, LlmProvider, SseEventParser, StreamChunk, TokenUsage,
    ToolCall as LlmToolCall, ToolDefinition as LlmToolDefinition,
};
use async_trait::async_trait;

fn is_non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn convert_tools_for_anthropic_messages(
    tools: Vec<LlmToolDefinition>,
) -> Option<Vec<AnthropicMessagesToolDefinition>> {
    if tools.is_empty() {
        None
    } else {
        Some(
            tools
                .into_iter()
                .map(|tool| AnthropicMessagesToolDefinition {
                    name: tool.name,
                    description: tool.description,
                    input_schema: tool.parameters,
                })
                .collect(),
        )
    }
}

/// Build thinking-related parameters for Anthropic API.
/// When thinking is enabled: temperature must be 1.0, max_tokens must > budget_tokens.
fn build_thinking_params(
    reasoning_effort: Option<ReasoningEffort>,
    temperature: Option<f32>,
    max_tokens: i32,
) -> Result<(Option<ThinkingConfig>, Option<f32>, i32)> {
    let resolved_budget = match reasoning_effort {
        Some(ReasoningEffort::None) => None,
        Some(effort) => Some(anthropic_budget_for_effort(effort)),
        None => None,
    };

    match resolved_budget {
        Some(budget) => {
            if budget < MIN_THINKING_BUDGET_TOKENS {
                anyhow::bail!(
                    "Anthropic thinking requires budget_tokens >= {} (got {})",
                    MIN_THINKING_BUDGET_TOKENS,
                    budget
                );
            }
            let budget_i32 =
                i32::try_from(budget).context("Anthropic budget_tokens exceeds supported range")?;

            // Anthropic requires max_tokens > budget_tokens.
            let min_max_tokens = budget_i32
                .checked_add(1)
                .context("Anthropic budget_tokens is too large")?;
            let adjusted_max = max_tokens.max(min_max_tokens);
            if let Some(temp) = temperature
                && (temp - 1.0).abs() > f32::EPSILON
            {
                debug!(
                    provided_temperature = temp,
                    "Anthropic thinking requires temperature=1.0; overriding request temperature"
                );
            }

            Ok((
                Some(ThinkingConfig {
                    config_type: "enabled".to_string(),
                    budget_tokens: budget,
                }),
                // Anthropic requires temperature = 1.0 when thinking is enabled
                Some(1.0),
                adjusted_max,
            ))
        }
        None => Ok((None, temperature, max_tokens)),
    }
}

fn anthropic_budget_for_effort(effort: ReasoningEffort) -> u32 {
    match effort {
        ReasoningEffort::None => 0,
        ReasoningEffort::Minimal | ReasoningEffort::Low => MIN_THINKING_BUDGET_TOKENS,
        ReasoningEffort::Medium => 4_096,
        ReasoningEffort::High => 8_192,
        ReasoningEffort::XHigh => 16_384,
    }
}

fn build_request_headers(
    messages: &[AnthropicMessagesMessage],
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Result<HeaderMap> {
    let mut beta_values: Vec<String> = Vec::new();

    if let Some(value) = extra_params.remove("anthropic_beta") {
        match value {
            serde_json::Value::String(s) => {
                if is_non_empty(&s) {
                    beta_values.push(s);
                }
            }
            serde_json::Value::Array(values) => {
                for v in values {
                    if let Some(s) = v.as_str()
                        && is_non_empty(s)
                    {
                        beta_values.push(s.to_string());
                    }
                }
            }
            other => {
                debug!(
                    value = %other,
                    "Ignoring non-string/array `anthropic_beta` in extra_params"
                );
            }
        }
    }

    if let Some(value) = extra_params.remove("interleaved_thinking") {
        match value {
            serde_json::Value::Bool(true) => {
                beta_values.push(INTERLEAVED_THINKING_BETA.to_string());
            }
            serde_json::Value::Bool(false) => {}
            other => {
                debug!(
                    value = %other,
                    "Ignoring non-boolean `interleaved_thinking` in extra_params"
                );
            }
        }
    }

    beta_values.retain(|v| is_non_empty(v));
    if messages.iter().any(message_uses_anthropic_file_source) {
        beta_values.push(FILES_API_BETA.to_string());
    }
    beta_values.sort();
    beta_values.dedup();

    let mut headers = HeaderMap::new();
    if !beta_values.is_empty() {
        let joined = beta_values.join(",");
        let header_value = HeaderValue::from_str(&joined)
            .context("Invalid anthropic-beta header value in extra_params")?;
        headers.insert("anthropic-beta", header_value);
    }

    Ok(headers)
}

fn convert_usage(u: Usage) -> TokenUsage {
    let cache_creation = u.cache_creation_input_tokens.unwrap_or_default();
    let cache_read = u.cache_read_input_tokens.unwrap_or_default();
    let prompt_tokens = u
        .input_tokens
        .saturating_add(cache_creation)
        .saturating_add(cache_read);
    TokenUsage {
        prompt_tokens,
        cached_prompt_tokens: u.cache_read_input_tokens,
        completion_tokens: u.output_tokens,
        total_tokens: prompt_tokens.saturating_add(u.output_tokens),
        reasoning_tokens: None,
    }
}

fn convert_anthropic_response(response: AnthropicMessagesResponse) -> GenerationResponse {
    let mut text_parts = Vec::new();
    let mut thinking_parts = Vec::new();
    let mut thinking_signature: Option<String> = None;
    let mut redacted_thinking = Vec::new();
    let mut tool_calls = Vec::new();

    for block in response.content {
        match block.block_type.as_str() {
            "thinking" => {
                if let Some(t) = block.thinking {
                    thinking_parts.push(t);
                }
                if let Some(sig) = block.signature.filter(|s| is_non_empty(s)) {
                    thinking_signature = Some(sig);
                }
            }
            "redacted_thinking" => {
                if let Some(data) = block.data {
                    redacted_thinking.push(data);
                }
            }
            "text" => {
                if let Some(t) = block.text {
                    text_parts.push(t);
                }
            }
            "tool_use" => {
                if let (Some(name), Some(input)) = (block.name, block.input) {
                    tool_calls.push(LlmToolCall {
                        id: block.id,
                        name,
                        arguments: input,
                    });
                }
            }
            _ => {}
        }
    }

    let usage = response.usage.map(convert_usage);

    let thinking = if thinking_parts.is_empty() {
        None
    } else {
        Some(thinking_parts.join(""))
    };

    GenerationResponse {
        content: text_parts.join(""),
        thinking,
        thinking_signature,
        redacted_thinking,
        tool_calls,
        usage,
        finish_reason: response.stop_reason.clone(),
        warnings: Vec::new(),
        provider_response_id: Some(response.id),
        provider_response_status: response.stop_reason,
    }
}

fn message_uses_anthropic_file_source(message: &AnthropicMessagesMessage) -> bool {
    message.content.iter().any(|block| match block {
        ContentBlockInput::Image { source } | ContentBlockInput::Document { source, .. } => {
            source.get("type").and_then(serde_json::Value::as_str) == Some("file")
        }
        _ => false,
    })
}

#[async_trait]
impl LlmProvider for AnthropicMessagesClient {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
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

        let (messages, system_prompt) =
            convert_messages_for_anthropic_messages(request_messages, system_prompt)?;
        let tools = convert_tools_for_anthropic_messages(request_tools);
        let request_headers = build_request_headers(&messages, &mut extra_params)?;
        if !extra_params.is_empty() {
            debug!(
                keys = ?extra_params.keys().collect::<Vec<_>>(),
                "Ignoring unsupported Anthropic extra_params keys"
            );
        }

        let (thinking, temperature, max_tokens) =
            build_thinking_params(reasoning.effort, temperature, max_tokens.unwrap_or(4096))?;

        let anthropic_request = AnthropicMessagesRequest {
            model: self.model.clone(),
            messages,
            max_tokens,
            system: system_prompt,
            temperature,
            tools,
            stream: Some(false),
            thinking,
        };

        let response = self
            .anthropic_messages_with_headers(anthropic_request, Some(&request_headers))
            .await?;
        Ok(convert_anthropic_response(response))
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> anyhow::Result<String> {
        self.chat(system, user).await
    }

    async fn generate_stream(
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

        let (messages, system_prompt) =
            convert_messages_for_anthropic_messages(request_messages, system_prompt)?;
        let tools = convert_tools_for_anthropic_messages(request_tools);
        let request_headers = build_request_headers(&messages, &mut extra_params)?;
        if !extra_params.is_empty() {
            debug!(
                keys = ?extra_params.keys().collect::<Vec<_>>(),
                "Ignoring unsupported Anthropic extra_params keys"
            );
        }

        let (thinking, temperature, max_tokens) =
            build_thinking_params(reasoning.effort, temperature, max_tokens.unwrap_or(4096))?;

        let anthropic_request = AnthropicMessagesRequest {
            model: self.model.clone(),
            messages,
            max_tokens,
            system: system_prompt,
            temperature,
            tools,
            stream: Some(true),
            thinking,
        };

        let (event_tx, event_rx) = tokio::sync::mpsc::channel(100);

        let client =
            AnthropicMessagesClient::with_params(&self.api_key, &self.base_url, &self.model);
        let request_headers_for_task = request_headers;
        tokio::spawn(async move {
            if let Err(e) = client
                .stream_anthropic_messages_with_headers(
                    anthropic_request,
                    event_tx,
                    Some(&request_headers_for_task),
                )
                .await
            {
                tracing::debug!(error = ?e, "Anthropic Messages API stream failed");
            }
        });

        Ok(streaming::project_events(event_rx))
    }

    fn provider_name(&self) -> &'static str {
        "anthropic_messages"
    }
}
#[cfg(test)]
mod tests;
