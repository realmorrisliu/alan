use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

use crate::ReasoningEffort;
use crate::message::reject_retired_message_overrides;

mod input_projection;

use input_projection::content_blocks;

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
    GenerationRequest, GenerationResponse, LlmProvider, Message as LlmMessage, MessageRole,
    SseEventParser, StreamChunk, TokenUsage, ToolCall as LlmToolCall, ToolCallDelta,
    ToolDefinition as LlmToolDefinition,
};
use async_trait::async_trait;

fn is_non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn convert_messages_for_anthropic_messages(
    messages: Vec<LlmMessage>,
) -> Vec<AnthropicMessagesMessage> {
    let mut converted = Vec::new();
    let mut known_tool_use_ids = std::collections::HashSet::new();

    for msg in messages {
        let LlmMessage {
            role,
            content,
            content_parts,
            thinking,
            thinking_signature,
            redacted_thinking,
            tool_calls,
            tool_call_id,
        } = msg;

        let content_blocks = match role {
            MessageRole::User => content_blocks(content, content_parts),
            MessageRole::Assistant => {
                let mut blocks = Vec::new();

                if let Some(thinking) = thinking
                    && !thinking.is_empty()
                {
                    blocks.push(ContentBlockInput::Thinking {
                        thinking,
                        signature: thinking_signature.filter(|sig| is_non_empty(sig)),
                    });
                }

                if let Some(redacted_blocks) = redacted_thinking {
                    for data in redacted_blocks {
                        if is_non_empty(&data) {
                            blocks.push(ContentBlockInput::RedactedThinking { data });
                        }
                    }
                }

                blocks.extend(content_blocks(content, content_parts));

                if let Some(calls) = tool_calls {
                    for call in calls {
                        if let Some(id) = call.id.filter(|id| is_non_empty(id)) {
                            known_tool_use_ids.insert(id.clone());
                            blocks.push(ContentBlockInput::ToolUse {
                                id,
                                name: call.name,
                                input: call.arguments,
                            });
                        }
                    }
                }

                blocks
            }
            MessageRole::Tool => {
                if let Some(tool_use_id) = tool_call_id.filter(|id| is_non_empty(id)) {
                    if known_tool_use_ids.contains(&tool_use_id) {
                        vec![ContentBlockInput::ToolResult {
                            tool_use_id,
                            content,
                            is_error: None,
                        }]
                    } else if !content.is_empty() {
                        vec![ContentBlockInput::Text { text: content }]
                    } else {
                        Vec::new()
                    }
                } else if !content.is_empty() {
                    vec![ContentBlockInput::Text { text: content }]
                } else {
                    Vec::new()
                }
            }
            MessageRole::System | MessageRole::Context => Vec::new(),
        };

        if content_blocks.is_empty() {
            continue;
        }

        let role = match role {
            MessageRole::User | MessageRole::Tool => "user".to_string(),
            MessageRole::Assistant => "assistant".to_string(),
            MessageRole::System | MessageRole::Context => continue,
        };

        converted.push(AnthropicMessagesMessage {
            role,
            content: content_blocks,
        });
    }

    converted
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

fn merge_initial_tool_arguments_delta(
    start_input: &mut Option<String>,
    saw_partial_json: &mut bool,
    partial_json: String,
) -> String {
    let merged = if !*saw_partial_json {
        start_input
            .take()
            .map(|prefix| format!("{prefix}{partial_json}"))
    } else {
        None
    };
    *saw_partial_json = true;
    merged.unwrap_or(partial_json)
}

#[derive(Debug, Clone)]
struct StreamedToolUseState {
    id: Option<String>,
    name: Option<String>,
    preview_start_input: Option<String>,
    accumulated_arguments: String,
    saw_partial_json: bool,
}

impl StreamedToolUseState {
    fn new(id: Option<String>, name: Option<String>, start_input: Option<String>) -> Self {
        Self {
            id,
            name,
            preview_start_input: start_input,
            accumulated_arguments: String::new(),
            saw_partial_json: false,
        }
    }

    fn merge_partial_json_for_preview(&mut self, partial_json: String) -> String {
        self.accumulated_arguments.push_str(&partial_json);
        merge_initial_tool_arguments_delta(
            &mut self.preview_start_input,
            &mut self.saw_partial_json,
            partial_json,
        )
    }

    fn finalized_arguments(&self) -> Option<String> {
        if self.saw_partial_json {
            (!self.accumulated_arguments.is_empty()).then(|| self.accumulated_arguments.clone())
        } else {
            self.preview_start_input
                .clone()
                .filter(|value| is_non_empty(value))
        }
    }
}

fn finalize_tool_use_chunks(index: usize, state: StreamedToolUseState) -> Vec<StreamChunk> {
    let StreamedToolUseState {
        id,
        name,
        preview_start_input,
        accumulated_arguments: _,
        saw_partial_json,
    } = state.clone();
    let mut chunks = Vec::new();

    if !saw_partial_json && let Some(arguments_delta) = preview_start_input {
        chunks.push(StreamChunk {
            text: None,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            usage: None,
            sequence_number: None,
            tool_call_delta: Some(ToolCallDelta {
                index,
                id: id.clone(),
                name: name.clone(),
                arguments_delta: Some(arguments_delta),
                arguments: None,
            }),
            is_finished: false,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
        });
    }

    if let Some(arguments) = state.finalized_arguments() {
        chunks.push(StreamChunk {
            text: None,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            usage: None,
            sequence_number: None,
            tool_call_delta: Some(ToolCallDelta {
                index,
                id,
                name,
                arguments_delta: None,
                arguments: Some(arguments),
            }),
            is_finished: false,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
        });
    }

    chunks
}

async fn send_stream_chunk(
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
    chunk: StreamChunk,
) -> bool {
    tx.send(chunk).await.is_ok()
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

        let messages = convert_messages_for_anthropic_messages(request_messages);
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

        let messages = convert_messages_for_anthropic_messages(request_messages);
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

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

        // Spawn streaming task
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

        // Transform events to StreamChunk
        tokio::spawn(async move {
            let mut latest_usage: Option<TokenUsage> = None;
            let mut latest_response_id: Option<String> = None;
            let mut tool_call_meta: std::collections::HashMap<usize, StreamedToolUseState> =
                std::collections::HashMap::new();
            while let Some(event) = event_rx.recv().await {
                if let Some(message) = event.message.as_ref()
                    && let Some(id) = message.id.clone()
                {
                    latest_response_id = Some(id);
                }
                let usage_from_event = event
                    .usage
                    .clone()
                    .or_else(|| event.message.as_ref().and_then(|m| m.usage.clone()));
                if let Some(usage) = usage_from_event {
                    latest_usage = Some(convert_usage(usage));
                }
                match event.event_type.as_str() {
                    "content_block_start" => {
                        if let Some(content_block) = event.content_block {
                            if content_block.block_type == "redacted_thinking"
                                && let Some(data) = content_block.data
                            {
                                let _ = tx
                                    .send(StreamChunk {
                                        text: None,
                                        thinking: None,
                                        thinking_signature: None,
                                        redacted_thinking: Some(data),
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
                            if content_block.block_type == "tool_use" {
                                let index = event
                                    .index
                                    .unwrap_or_default()
                                    .max(0)
                                    .try_into()
                                    .unwrap_or(0usize);
                                let id = content_block.id.clone();
                                let name = content_block.name.clone();
                                let start_input =
                                    content_block.input.map(|input| input.to_string());
                                tool_call_meta.insert(
                                    index,
                                    StreamedToolUseState::new(
                                        id.clone(),
                                        name.clone(),
                                        start_input,
                                    ),
                                );
                                if !send_stream_chunk(
                                    &tx,
                                    StreamChunk {
                                        text: None,
                                        thinking: None,
                                        thinking_signature: None,
                                        redacted_thinking: None,
                                        usage: None,
                                        sequence_number: None,
                                        tool_call_delta: Some(ToolCallDelta {
                                            index,
                                            id,
                                            name,
                                            arguments_delta: None,
                                            arguments: None,
                                        }),
                                        is_finished: false,
                                        finish_reason: None,
                                        provider_response_id: None,
                                        provider_response_status: None,
                                    },
                                )
                                .await
                                {
                                    return;
                                }
                            }
                        }
                    }
                    "content_block_delta" => {
                        if let Some(delta) = event.delta {
                            if let Some(thinking) = delta.thinking {
                                let _ = tx
                                    .send(StreamChunk {
                                        text: None,
                                        thinking: Some(thinking),
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
                            if let Some(signature) = delta.signature {
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
                            if let Some(text) = delta.text {
                                let _ = tx
                                    .send(StreamChunk {
                                        text: Some(text),
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
                            if let (Some(partial_json), Some(index)) =
                                (delta.partial_json, event.index)
                            {
                                let index = index.max(0).try_into().unwrap_or(0usize);
                                let state = tool_call_meta
                                    .entry(index)
                                    .or_insert_with(|| StreamedToolUseState::new(None, None, None));
                                let arguments_delta =
                                    state.merge_partial_json_for_preview(partial_json);
                                if !send_stream_chunk(
                                    &tx,
                                    StreamChunk {
                                        text: None,
                                        thinking: None,
                                        thinking_signature: None,
                                        redacted_thinking: None,
                                        usage: None,
                                        sequence_number: None,
                                        tool_call_delta: Some(ToolCallDelta {
                                            index,
                                            id: state.id.clone(),
                                            name: state.name.clone(),
                                            arguments_delta: Some(arguments_delta),
                                            arguments: None,
                                        }),
                                        is_finished: false,
                                        finish_reason: None,
                                        provider_response_id: None,
                                        provider_response_status: None,
                                    },
                                )
                                .await
                                {
                                    return;
                                }
                            }
                        }
                    }
                    "content_block_stop" => {
                        if let Some(index) = event.index {
                            let index = index.max(0).try_into().unwrap_or(0usize);
                            if let Some(state) = tool_call_meta.remove(&index) {
                                for chunk in finalize_tool_use_chunks(index, state) {
                                    if !send_stream_chunk(&tx, chunk).await {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    "message_stop" => {
                        let mut indices: Vec<usize> = tool_call_meta.keys().copied().collect();
                        indices.sort_unstable();
                        for index in indices {
                            if let Some(state) = tool_call_meta.remove(&index) {
                                for chunk in finalize_tool_use_chunks(index, state) {
                                    if !send_stream_chunk(&tx, chunk).await {
                                        return;
                                    }
                                }
                            }
                        }
                        let _ = send_stream_chunk(
                            &tx,
                            StreamChunk {
                                text: None,
                                thinking: None,
                                thinking_signature: None,
                                redacted_thinking: None,
                                usage: latest_usage,
                                sequence_number: None,
                                tool_call_delta: None,
                                is_finished: true,
                                finish_reason: event.message.and_then(|m| m.stop_reason),
                                provider_response_id: latest_response_id.clone(),
                                provider_response_status: None,
                            },
                        )
                        .await;
                    }
                    _ => {}
                }
            }
        });

        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "anthropic_messages"
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_messages_url_appends_v1_when_missing() {
        let client =
            AnthropicMessagesClient::with_params("k", "https://api.kimi.com/coding", "k2p5");
        assert_eq!(
            client.anthropic_messages_url(),
            "https://api.kimi.com/coding/v1/messages"
        );
    }

    #[test]
    fn anthropic_messages_url_preserves_existing_v1() {
        let client =
            AnthropicMessagesClient::with_params("k", "https://api.anthropic.com/v1", "claude");
        assert_eq!(
            client.anthropic_messages_url(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn test_anthropic_messages_client_with_params() {
        let client = AnthropicMessagesClient::with_params(
            "test-key",
            "https://api.anthropic.com/v1",
            "claude-3-opus",
        );
        // Just verify client creation works
        drop(client);
    }

    #[test]
    fn test_message_request_serialization() {
        let request = AnthropicMessagesRequest {
            model: "claude-3-opus".to_string(),
            messages: vec![AnthropicMessagesMessage {
                role: "user".to_string(),
                content: vec![ContentBlockInput::Text {
                    text: "Hello".to_string(),
                }],
            }],
            max_tokens: 1024,
            system: None,
            temperature: Some(0.7),
            tools: None,
            stream: None,
            thinking: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("claude-3-opus"));
        assert!(json.contains("messages"));
        assert!(json.contains("max_tokens"));
    }

    #[test]
    fn test_message_response_deserialization() {
        let json = r#"{
            "id": "msg_123",
            "type": "message",
            "content": [
                {"type": "text", "text": "Hello!"}
            ],
            "model": "claude-3-opus",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20
            }
        }"#;

        let response: AnthropicMessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "msg_123");
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.usage.as_ref().unwrap().input_tokens, 10);
    }

    #[test]
    fn test_convert_anthropic_response_propagates_id_and_finish_reason() {
        let response = AnthropicMessagesResponse {
            id: "msg_123".to_string(),
            content: vec![
                ContentBlock {
                    block_type: "thinking".to_string(),
                    text: None,
                    thinking: Some("step".to_string()),
                    signature: Some("sig_123".to_string()),
                    data: None,
                    id: None,
                    name: None,
                    input: None,
                },
                ContentBlock {
                    block_type: "text".to_string(),
                    text: Some("Done".to_string()),
                    thinking: None,
                    signature: None,
                    data: None,
                    id: None,
                    name: None,
                    input: None,
                },
            ],
            usage: None,
            stop_reason: Some("end_turn".to_string()),
        };

        let converted = convert_anthropic_response(response);
        assert_eq!(converted.content, "Done");
        assert_eq!(converted.thinking.as_deref(), Some("step"));
        assert_eq!(converted.thinking_signature.as_deref(), Some("sig_123"));
        assert_eq!(converted.finish_reason.as_deref(), Some("end_turn"));
        assert_eq!(converted.provider_response_id.as_deref(), Some("msg_123"));
        assert_eq!(
            converted.provider_response_status.as_deref(),
            Some("end_turn")
        );
    }

    #[test]
    fn test_content_block() {
        let block = ContentBlock {
            block_type: "text".to_string(),
            text: Some("Hello".to_string()),
            thinking: None,
            signature: None,
            data: None,
            id: None,
            name: None,
            input: None,
        };

        assert_eq!(block.block_type, "text");
        assert_eq!(block.text, Some("Hello".to_string()));
    }

    #[test]
    fn test_message_user_text() {
        let msg = AnthropicMessagesMessage::user_text("Hello");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            ContentBlockInput::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_message_user_tool_result() {
        let msg =
            AnthropicMessagesMessage::user_tool_result("tool-call-123", "Tool output".to_string());
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            ContentBlockInput::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(tool_use_id, "tool-call-123");
                assert_eq!(content, "Tool output");
            }
            _ => panic!("Expected ToolResult variant"),
        }
    }

    #[test]
    fn test_tool_definition_new() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            }
        });

        let tool = AnthropicMessagesToolDefinition::new("search", "Search tool", schema.clone());
        assert_eq!(tool.name, "search");
        assert_eq!(tool.description, "Search tool");
        assert_eq!(tool.input_schema, schema);
    }

    #[test]
    fn test_content_block_input_text() {
        let block = ContentBlockInput::Text {
            text: "Hello".to_string(),
        };

        match block {
            ContentBlockInput::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_content_block_input_tool_use() {
        let block = ContentBlockInput::ToolUse {
            id: "call-123".to_string(),
            name: "my_tool".to_string(),
            input: serde_json::json!({"arg": "value"}),
        };

        match block {
            ContentBlockInput::ToolUse { id, name, input } => {
                assert_eq!(id, "call-123");
                assert_eq!(name, "my_tool");
                assert_eq!(input["arg"], "value");
            }
            _ => panic!("Expected ToolUse variant"),
        }
    }

    #[test]
    fn test_content_block_input_tool_result() {
        let block = ContentBlockInput::ToolResult {
            tool_use_id: "call-456".to_string(),
            content: "Result".to_string(),
            is_error: Some(false),
        };

        match block {
            ContentBlockInput::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "call-456");
                assert_eq!(content, "Result");
                assert_eq!(is_error, Some(false));
            }
            _ => panic!("Expected ToolResult variant"),
        }
    }

    #[test]
    fn test_stream_event_deserialization() {
        let json = r#"{
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": "Hello"
            }
        }"#;

        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "content_block_delta");
        assert_eq!(event.index, Some(0));
        assert!(event.delta.is_some());
    }

    #[test]
    fn test_message_serialization() {
        let msg = AnthropicMessagesMessage {
            role: "assistant".to_string(),
            content: vec![ContentBlockInput::Text {
                text: "Hi!".to_string(),
            }],
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("assistant"));
        assert!(json.contains("Hi!"));
    }

    #[test]
    fn test_usage() {
        let usage = Usage {
            input_tokens: 100,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            output_tokens: 50,
        };

        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn test_stream_delta() {
        let delta = StreamDelta {
            delta_type: Some("text_delta".to_string()),
            text: Some("Hello".to_string()),
            thinking: None,
            signature: None,
            partial_json: None,
            stop_reason: None,
        };

        assert_eq!(delta.delta_type, Some("text_delta".to_string()));
        assert_eq!(delta.text, Some("Hello".to_string()));
    }

    #[test]
    fn test_stream_message() {
        let msg = StreamMessage {
            id: Some("msg_123".to_string()),
            stop_reason: Some("end_turn".to_string()),
            usage: None,
        };
        assert_eq!(msg.id.as_deref(), Some("msg_123"));
        assert_eq!(msg.stop_reason, Some("end_turn".to_string()));
    }

    #[test]
    fn test_stream_error() {
        let err = StreamError {
            error: Some(serde_json::json!({"type": "error"})),
            message: Some("Something went wrong".to_string()),
            r#type: Some("api_error".to_string()),
        };
        assert_eq!(err.message, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_merge_initial_tool_arguments_delta_prefixes_first_partial_json() {
        let mut start_input = Some("{\"a\":".to_string());
        let mut saw_partial_json = false;

        let merged = merge_initial_tool_arguments_delta(
            &mut start_input,
            &mut saw_partial_json,
            "1}".to_string(),
        );

        assert_eq!(merged, "{\"a\":1}");
        assert!(saw_partial_json);
        assert!(start_input.is_none());
    }

    #[test]
    fn test_merge_initial_tool_arguments_delta_does_not_repeat_prefix() {
        let mut start_input = Some("{\"a\":".to_string());
        let mut saw_partial_json = false;

        let first = merge_initial_tool_arguments_delta(
            &mut start_input,
            &mut saw_partial_json,
            "1".to_string(),
        );
        let second = merge_initial_tool_arguments_delta(
            &mut start_input,
            &mut saw_partial_json,
            "}".to_string(),
        );

        assert_eq!(first, "{\"a\":1");
        assert_eq!(second, "}");
    }

    #[test]
    fn test_streamed_tool_use_state_finalizes_start_input_without_partial_json() {
        let state = StreamedToolUseState::new(
            Some("tool-1".to_string()),
            Some("bash".to_string()),
            Some("{\"command\":\"pwd\"}".to_string()),
        );

        let chunks = finalize_tool_use_chunks(0, state);
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            chunks[0]
                .tool_call_delta
                .as_ref()
                .and_then(|delta| delta.arguments_delta.as_deref()),
            Some("{\"command\":\"pwd\"}")
        );
        assert_eq!(
            chunks[1]
                .tool_call_delta
                .as_ref()
                .and_then(|delta| delta.arguments.as_deref()),
            Some("{\"command\":\"pwd\"}")
        );
    }

    #[test]
    fn test_streamed_tool_use_state_finalizes_accumulated_partial_json() {
        let mut state =
            StreamedToolUseState::new(Some("tool-2".to_string()), Some("bash".to_string()), None);

        let first_preview =
            state.merge_partial_json_for_preview("{\"command\":\"echo ".to_string());
        let second_preview = state.merge_partial_json_for_preview("hello\"}".to_string());
        let chunks = finalize_tool_use_chunks(0, state);

        assert_eq!(first_preview, "{\"command\":\"echo ");
        assert_eq!(second_preview, "hello\"}");
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0]
                .tool_call_delta
                .as_ref()
                .and_then(|delta| delta.arguments.as_deref()),
            Some("{\"command\":\"echo hello\"}")
        );
    }

    #[test]
    fn test_streamed_tool_use_state_ignores_start_input_when_partial_json_arrives() {
        let mut state = StreamedToolUseState::new(
            Some("tool-3".to_string()),
            Some("bash".to_string()),
            Some("{\"command\":\"pwd\"}".to_string()),
        );

        let preview = state.merge_partial_json_for_preview("{\"command\":\"pwd\"}".to_string());
        let chunks = finalize_tool_use_chunks(0, state);

        assert_eq!(preview, "{\"command\":\"pwd\"}{\"command\":\"pwd\"}");
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0]
                .tool_call_delta
                .as_ref()
                .and_then(|delta| delta.arguments.as_deref()),
            Some("{\"command\":\"pwd\"}")
        );
    }

    #[test]
    fn test_message_assistant() {
        let msg = AnthropicMessagesMessage::assistant_text("Hello from assistant");
        assert_eq!(msg.role, "assistant");
        match &msg.content[0] {
            ContentBlockInput::Text { text } => assert_eq!(text, "Hello from assistant"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_convert_messages_for_anthropic_keeps_assistant_tool_use() {
        let assistant = crate::Message::assistant_with_tools(
            "",
            vec![
                crate::ToolCall::new("web_search", serde_json::json!({"query": "laptop"}))
                    .with_id("toolu_123"),
            ],
        );
        let tool = crate::Message::tool("toolu_123", "{\"ok\":true}");

        let converted = convert_messages_for_anthropic_messages(vec![assistant, tool]);
        assert_eq!(converted.len(), 2);

        assert_eq!(converted[0].role, "assistant");
        assert_eq!(converted[0].content.len(), 1);
        match &converted[0].content[0] {
            ContentBlockInput::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_123");
                assert_eq!(name, "web_search");
                assert_eq!(input["query"], "laptop");
            }
            _ => panic!("Expected ToolUse variant"),
        }

        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[1].content.len(), 1);
        match &converted[1].content[0] {
            ContentBlockInput::ToolResult { tool_use_id, .. } => {
                assert_eq!(tool_use_id, "toolu_123");
            }
            _ => panic!("Expected ToolResult variant"),
        }
    }

    #[test]
    fn test_convert_messages_for_anthropic_empty_tool_call_id_falls_back_to_text() {
        let tool_msg = crate::Message {
            role: MessageRole::Tool,
            content: "tool output".to_string(),
            content_parts: Vec::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: Some("   ".to_string()),
        };

        let converted = convert_messages_for_anthropic_messages(vec![tool_msg]);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[0].content.len(), 1);
        match &converted[0].content[0] {
            ContentBlockInput::Text { text } => assert_eq!(text, "tool output"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_convert_messages_for_anthropic_unknown_tool_call_id_falls_back_to_text() {
        let assistant = crate::Message::assistant_with_tools(
            "",
            vec![
                crate::ToolCall::new("web_search", serde_json::json!({"q": "rust"}))
                    .with_id("toolu_known"),
            ],
        );
        let unmatched_tool_result = crate::Message::tool("toolu_unknown", "{\"ok\":true}");

        let converted =
            convert_messages_for_anthropic_messages(vec![assistant, unmatched_tool_result]);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[1].content.len(), 1);
        match &converted[1].content[0] {
            ContentBlockInput::Text { text } => assert_eq!(text, "{\"ok\":true}"),
            _ => panic!("Expected Text fallback for unknown tool_use_id"),
        }
    }

    #[test]
    fn test_build_thinking_params_omits_thinking_when_effort_unset() {
        let (thinking, temperature, max_tokens) =
            build_thinking_params(None, Some(0.7), 2048).unwrap();
        assert!(thinking.is_none());
        assert_eq!(temperature, Some(0.7));
        assert_eq!(max_tokens, 2048);
    }

    #[test]
    fn test_build_thinking_params_adjusts_max_tokens_and_temperature() {
        let (thinking, temperature, max_tokens) =
            build_thinking_params(Some(ReasoningEffort::Low), Some(0.2), 1000).unwrap();

        assert!(thinking.is_some());
        assert_eq!(temperature, Some(1.0));
        assert_eq!(max_tokens, 1025);
    }

    #[test]
    fn test_build_thinking_params_maps_reasoning_effort_to_budget() {
        let (thinking, temperature, max_tokens) =
            build_thinking_params(Some(ReasoningEffort::High), Some(0.2), 4096).unwrap();

        assert_eq!(thinking.unwrap().budget_tokens, 8192);
        assert_eq!(temperature, Some(1.0));
        assert_eq!(max_tokens, 8193);
    }

    #[test]
    fn test_build_request_headers_supports_beta_and_interleaved() {
        let mut extra_params = HashMap::from([
            (
                "anthropic_beta".to_string(),
                serde_json::json!(["tools-2024-05-16"]),
            ),
            ("interleaved_thinking".to_string(), serde_json::json!(true)),
        ]);

        let headers = build_request_headers(&[], &mut extra_params).unwrap();
        assert!(extra_params.is_empty());
        let value = headers.get("anthropic-beta").unwrap().to_str().unwrap();
        assert!(value.contains("tools-2024-05-16"));
        assert!(value.contains(INTERLEAVED_THINKING_BETA));
    }

    #[test]
    fn test_build_request_headers_adds_files_beta_for_file_sources() {
        let messages = vec![AnthropicMessagesMessage {
            role: "user".to_string(),
            content: vec![ContentBlockInput::Document {
                source: serde_json::json!({
                    "type": "file",
                    "file_id": "file_123"
                }),
                title: Some("Spec".to_string()),
                citations: None,
            }],
        }];
        let mut extra_params = HashMap::new();

        let headers = build_request_headers(&messages, &mut extra_params).unwrap();

        let value = headers.get("anthropic-beta").unwrap().to_str().unwrap();
        assert!(value.contains(FILES_API_BETA));
    }

    #[test]
    fn test_convert_usage_extracts_cached_prompt_tokens() {
        let usage = Usage {
            input_tokens: 100,
            cache_creation_input_tokens: Some(20),
            cache_read_input_tokens: Some(30),
            output_tokens: 50,
        };

        let token_usage = convert_usage(usage);
        assert_eq!(token_usage.prompt_tokens, 150);
        assert_eq!(token_usage.cached_prompt_tokens, Some(30));
    }

    #[test]
    fn test_convert_messages_for_anthropic_preserves_thinking_signature_and_redacted() {
        let message = crate::Message {
            role: MessageRole::Assistant,
            content: "done".to_string(),
            content_parts: Vec::new(),
            thinking: Some("step by step".to_string()),
            thinking_signature: Some("sig_123".to_string()),
            redacted_thinking: Some(vec!["ciphertext".to_string()]),
            tool_calls: None,
            tool_call_id: None,
        };

        let converted = convert_messages_for_anthropic_messages(vec![message]);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "assistant");
        assert_eq!(converted[0].content.len(), 3);

        match &converted[0].content[0] {
            ContentBlockInput::Thinking {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "step by step");
                assert_eq!(signature.as_deref(), Some("sig_123"));
            }
            _ => panic!("Expected Thinking block"),
        }
        match &converted[0].content[1] {
            ContentBlockInput::RedactedThinking { data } => {
                assert_eq!(data, "ciphertext");
            }
            _ => panic!("Expected RedactedThinking block"),
        }
        match &converted[0].content[2] {
            ContentBlockInput::Text { text } => {
                assert_eq!(text, "done");
            }
            _ => panic!("Expected Text block"),
        }
    }
}
