//! OpenRouter SDK-backed chat adapter.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures::StreamExt;
#[cfg(test)]
use openrouter_rs::types::UnifiedStreamEvent;
use openrouter_rs::{
    api::chat::{
        ChatCompletionRequest, ChatCompletionRequestBuilder, DebugOptions, Message as OrMessage,
        Modality, Plugin, StopSequence, StreamOptions, TraceOptions,
    },
    types::{
        Effort, FinishReason, ProviderPreferences, ReasoningConfig, ResponseFormat, ResponseUsage,
        Role, Tool as OrTool, ToolCall as OrToolCall, completion::CompletionsResponse,
    },
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::debug;

use crate::{
    GenerationRequest, GenerationResponse, LlmProvider, Message, MessageRole, ReasoningEffort,
    StreamChunk, TokenUsage, ToolCall, ToolCallDelta, ToolDefinition,
};

pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// alan LLM provider backed by the `openrouter-rs` SDK.
#[derive(Clone)]
pub struct OpenRouterClient {
    http_client: reqwest::Client,
    api_key: String,
    base_url: String,
    metadata: OpenRouterClientMetadata,
    model: String,
}

#[derive(Debug, Clone, Default)]
pub struct OpenRouterClientMetadata {
    pub http_referer: Option<String>,
    pub x_title: Option<String>,
    pub app_categories: Option<Vec<String>>,
}

impl OpenRouterClient {
    pub fn with_params(api_key: &str, base_url: &str, model: &str) -> Result<Self> {
        Self::with_metadata(
            api_key,
            base_url,
            model,
            OpenRouterClientMetadata::default(),
        )
    }

    pub fn with_metadata(
        api_key: &str,
        base_url: &str,
        model: &str,
        metadata: OpenRouterClientMetadata,
    ) -> Result<Self> {
        validate_app_categories(metadata.app_categories.as_deref())?;

        Ok(Self {
            http_client: reqwest::Client::new(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            metadata,
            model: model.to_string(),
        })
    }

    async fn generate_chat(&self, request: GenerationRequest) -> Result<GenerationResponse> {
        let payload = build_openrouter_chat_request_payload(&self.model, request)?;
        let response = self
            .request_builder()?
            .json(&payload)
            .send()
            .await
            .context("OpenRouter chat completion request failed")?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .context("failed to read OpenRouter chat completion response")?;
        if !status.is_success() {
            bail!(
                "OpenRouter chat completion request failed with status {status}: {}",
                String::from_utf8_lossy(&bytes)
            );
        }
        let response = serde_json::from_slice(&bytes)
            .context("failed to decode OpenRouter chat completion response")?;
        Ok(convert_openrouter_response(response))
    }

    async fn generate_chat_stream(
        &self,
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        let mut payload = build_openrouter_chat_request_payload(&self.model, request)?;
        payload["stream"] = Value::Bool(true);
        let response = self
            .request_builder()?
            .json(&payload)
            .send()
            .await
            .context("OpenRouter stream request failed")?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<failed to read body>"));
            bail!("OpenRouter stream request failed with status {status}: {body}");
        }

        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            consume_openrouter_raw_stream(response, tx).await;
        });

        Ok(rx)
    }

    fn request_builder(&self) -> Result<reqwest::RequestBuilder> {
        let mut request = self
            .http_client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key);

        let x_title = self.metadata.x_title.as_deref().unwrap_or("openrouter-rs");
        request = request
            .header("X-OpenRouter-Title", x_title)
            .header("X-Title", x_title);

        if let Some(http_referer) = &self.metadata.http_referer {
            request = request.header("HTTP-Referer", http_referer);
        }
        if let Some(app_categories) = &self.metadata.app_categories {
            request = request.header(
                "X-OpenRouter-Categories",
                serialize_app_categories(app_categories)?,
            );
        }

        Ok(request)
    }
}

#[async_trait]
impl LlmProvider for OpenRouterClient {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.generate_chat(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        let mut request = GenerationRequest::new().with_user_message(user);
        if let Some(system) = system {
            request = request.with_system_prompt(system);
        }
        Ok(self.generate_chat(request).await?.content)
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        self.generate_chat_stream(request).await
    }

    fn provider_name(&self) -> &'static str {
        "openrouter"
    }
}

pub(crate) fn build_openrouter_chat_request(
    model: &str,
    request: GenerationRequest,
) -> Result<ChatCompletionRequest> {
    let GenerationRequest {
        system_prompt,
        messages,
        tools,
        temperature,
        max_tokens,
        reasoning,
        mut extra_params,
    } = request;

    let mut projected_messages = Vec::new();
    if let Some(system_prompt) = system_prompt.filter(|value| !value.is_empty()) {
        projected_messages.push(OrMessage::new(Role::System, system_prompt));
    }
    projected_messages.extend(convert_messages_for_openrouter(messages)?);

    let mut builder = ChatCompletionRequest::builder();
    builder.model(model).messages(projected_messages);

    if let Some(temperature) = temperature {
        builder.temperature(f64::from(temperature));
    }
    if let Some(max_tokens) = max_tokens {
        builder.max_tokens(non_negative_u32("max_tokens", max_tokens)?);
    }
    if let Some(effort) = reasoning.effort {
        extra_params.remove("reasoning_effort");
        builder.reasoning_effort(openrouter_effort(effort));
    }

    let tools = convert_tools_for_openrouter(tools);
    if !tools.is_empty() {
        builder.tools(tools);
        builder.tool_choice_auto();
    }

    apply_openrouter_extra_params(&mut builder, extra_params)?;
    builder
        .build()
        .context("failed to build OpenRouter chat completion request")
}

fn build_openrouter_chat_request_payload(model: &str, request: GenerationRequest) -> Result<Value> {
    let mut alan_messages = Vec::new();
    if let Some(system_prompt) = request
        .system_prompt
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        alan_messages.push(Message::system(system_prompt.clone()));
    }
    alan_messages.extend(request.messages.clone());

    let chat_request = build_openrouter_chat_request(model, request)?;
    let mut payload = serde_json::to_value(chat_request)
        .context("failed to encode OpenRouter chat completion request")?;
    preserve_openrouter_reasoning_fields(&mut payload, &alan_messages)?;
    Ok(payload)
}

fn preserve_openrouter_reasoning_fields(payload: &mut Value, messages: &[Message]) -> Result<()> {
    let Some(projected_messages) = payload.get_mut("messages").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    if projected_messages.len() != messages.len() {
        bail!("OpenRouter message projection lost message alignment");
    }

    for (projected, source) in projected_messages.iter_mut().zip(messages) {
        if source.role != MessageRole::Assistant {
            continue;
        }
        let Some(projected) = projected.as_object_mut() else {
            continue;
        };
        if let Some(thinking) = source
            .thinking
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            projected.insert(
                "reasoning_content".to_string(),
                Value::String(thinking.clone()),
            );
        }
        if let Some(signature) = source
            .thinking_signature
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            projected.insert(
                "reasoning".to_string(),
                serde_json::json!({ "encrypted_content": signature }),
            );
        }
    }

    Ok(())
}

pub(crate) fn convert_messages_for_openrouter(messages: Vec<Message>) -> Result<Vec<OrMessage>> {
    messages
        .into_iter()
        .map(|message| {
            let Message {
                role,
                content,
                thinking: _thinking,
                thinking_signature: _thinking_signature,
                redacted_thinking: _,
                tool_calls,
                tool_call_id,
            } = message;

            Ok(match role {
                MessageRole::System => OrMessage::new(Role::System, content),
                MessageRole::User => OrMessage::new(Role::User, content),
                MessageRole::Assistant => {
                    let tool_calls = tool_calls.map(convert_tool_calls_for_openrouter);
                    match tool_calls {
                        Some(tool_calls) if !tool_calls.is_empty() => {
                            OrMessage::assistant_with_tool_calls(content, tool_calls)
                        }
                        _ => OrMessage::new(Role::Assistant, content),
                    }
                }
                MessageRole::Tool => {
                    let tool_call_id = tool_call_id
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "OpenRouter tool response messages require a non-empty tool_call_id"
                            )
                        })?;
                    OrMessage::tool_response(&tool_call_id, content)
                }
                MessageRole::Context => OrMessage::new(Role::System, content),
            })
        })
        .collect()
}

pub(crate) fn convert_tools_for_openrouter(tools: Vec<ToolDefinition>) -> Vec<OrTool> {
    tools
        .into_iter()
        .map(|tool| OrTool::new(&tool.name, &tool.description, tool.parameters))
        .collect()
}

pub(crate) fn convert_openrouter_response(response: CompletionsResponse) -> GenerationResponse {
    let mut content = String::new();
    let mut thinking = String::new();
    let mut tool_calls = Vec::new();
    let mut warnings = Vec::new();
    let mut finish_reason = None;

    for choice in &response.choices {
        if let Some(choice_content) = choice.content() {
            content.push_str(choice_content);
        }
        if let Some(choice_reasoning) = choice.reasoning() {
            thinking.push_str(choice_reasoning);
        } else if let Some(details) = choice.reasoning_details() {
            for detail in details {
                if let Some(part) = detail.content() {
                    thinking.push_str(part);
                }
            }
        }
        if let Some(reason) = choice.finish_reason() {
            finish_reason = Some(finish_reason_to_string(reason).to_string());
        }
        if let Some(calls) = choice.tool_calls() {
            for call in calls {
                match convert_openrouter_tool_call(call) {
                    Ok(call) => tool_calls.push(call),
                    Err(warning) => warnings.push(warning),
                }
            }
        }
    }

    GenerationResponse {
        content,
        thinking: (!thinking.is_empty()).then_some(thinking),
        thinking_signature: first_reasoning_signature(&response),
        redacted_thinking: Vec::new(),
        tool_calls,
        usage: response.usage.map(convert_usage),
        finish_reason,
        provider_response_id: Some(response.id),
        provider_response_status: None,
        warnings,
    }
}

#[cfg(test)]
async fn consume_openrouter_stream(
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
                latest_usage = usage.and_then(usage_from_value);
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
        .send(StreamChunk {
            text: None,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            usage: latest_usage,
            provider_response_id: latest_response_id,
            provider_response_status: None,
            sequence_number: None,
            tool_call_delta: None,
            is_finished: true,
            finish_reason: latest_finish_reason,
        })
        .await;
}

async fn consume_openrouter_raw_stream(response: reqwest::Response, tx: mpsc::Sender<StreamChunk>) {
    let mut emitted_payload = false;
    let mut latest_usage = None;
    let mut latest_response_id = None;
    let mut latest_finish_reason = None;
    let mut parser = crate::SseEventParser::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                debug!(?error, "OpenRouter stream failed while reading bytes");
                if emitted_payload {
                    latest_finish_reason = Some("stream_error".to_string());
                    break;
                }
                return;
            }
        };

        let mut done = false;
        for event in parser.push(&chunk) {
            if event.trim() == "[DONE]" {
                done = true;
                break;
            }
            match serde_json::from_str::<CompletionsResponse>(&event) {
                Ok(response) => {
                    emit_openrouter_stream_response(
                        response,
                        &tx,
                        &mut emitted_payload,
                        &mut latest_response_id,
                        &mut latest_finish_reason,
                        &mut latest_usage,
                    )
                    .await;
                }
                Err(error) => {
                    debug!(?error, event, "failed to decode OpenRouter stream event");
                    if emitted_payload {
                        latest_finish_reason = Some("stream_error".to_string());
                        done = true;
                        break;
                    }
                    return;
                }
            }
        }
        if done {
            break;
        }
    }

    for event in parser.finish() {
        if event.trim() == "[DONE]" {
            break;
        }
        match serde_json::from_str::<CompletionsResponse>(&event) {
            Ok(response) => {
                emit_openrouter_stream_response(
                    response,
                    &tx,
                    &mut emitted_payload,
                    &mut latest_response_id,
                    &mut latest_finish_reason,
                    &mut latest_usage,
                )
                .await;
            }
            Err(error) => {
                debug!(
                    ?error,
                    event, "failed to decode trailing OpenRouter stream event"
                );
                if emitted_payload {
                    latest_finish_reason = Some("stream_error".to_string());
                    break;
                }
                return;
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
            provider_response_id: latest_response_id,
            provider_response_status: None,
            sequence_number: None,
            tool_call_delta: None,
            is_finished: true,
            finish_reason: latest_finish_reason,
        })
        .await;
}

async fn emit_openrouter_stream_response(
    response: CompletionsResponse,
    tx: &mpsc::Sender<StreamChunk>,
    emitted_payload: &mut bool,
    latest_response_id: &mut Option<String>,
    latest_finish_reason: &mut Option<String>,
    latest_usage: &mut Option<TokenUsage>,
) {
    *latest_response_id = Some(response.id.clone());
    if let Some(usage) = response.usage {
        *latest_usage = Some(convert_usage(usage));
    }

    for choice in &response.choices {
        if let Some(content) = choice.content().filter(|value| !value.is_empty()) {
            *emitted_payload = true;
            let _ = tx.send(stream_text_chunk(content.to_string())).await;
        }
        if let Some(reasoning) = choice.reasoning().filter(|value| !value.is_empty()) {
            *emitted_payload = true;
            let _ = tx.send(stream_thinking_chunk(reasoning.to_string())).await;
        }
        if let Some(details) = choice.reasoning_details() {
            for detail in details {
                if let Some(thinking) = detail.content().filter(|value| !value.is_empty()) {
                    *emitted_payload = true;
                    let _ = tx.send(stream_thinking_chunk(thinking.to_string())).await;
                }
                if let Some(signature) = detail.signature.as_ref().filter(|value| !value.is_empty())
                {
                    *emitted_payload = true;
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
                    *emitted_payload = true;
                    let _ = tx.send(stream_tool_chunk(delta)).await;
                }
            }
        }
        if let Some(reason) = choice.finish_reason() {
            *latest_finish_reason = Some(finish_reason_to_string(reason).to_string());
        }
    }
}

fn apply_openrouter_extra_params(
    builder: &mut ChatCompletionRequestBuilder,
    extra_params: HashMap<String, Value>,
) -> Result<()> {
    for (key, value) in extra_params {
        match key.as_str() {
            "max_completion_tokens" => builder.max_completion_tokens(value_as_u32(&key, value)?),
            "seed" => builder.seed(value_as_u32(&key, value)?),
            "top_p" => builder.top_p(value_as_f64(&key, value)?),
            "top_k" => builder.top_k(value_as_u32(&key, value)?),
            "frequency_penalty" => builder.frequency_penalty(value_as_f64(&key, value)?),
            "presence_penalty" => builder.presence_penalty(value_as_f64(&key, value)?),
            "repetition_penalty" => builder.repetition_penalty(value_as_f64(&key, value)?),
            "logit_bias" => builder.logit_bias(value_as::<HashMap<String, f64>>(&key, value)?),
            "logprobs" => builder.logprobs(value_as_bool(&key, value)?),
            "top_logprobs" => builder.top_logprobs(value_as_u32(&key, value)?),
            "min_p" => builder.min_p(value_as_f64(&key, value)?),
            "top_a" => builder.top_a(value_as_f64(&key, value)?),
            "transforms" => builder.transforms(value_as::<Vec<String>>(&key, value)?),
            "models" => builder.models(value_as::<Vec<String>>(&key, value)?),
            "route" => builder.route(value_as_string(&key, value)?),
            "user" => builder.user(value_as_string(&key, value)?),
            "session_id" => builder.session_id(value_as_string(&key, value)?),
            "trace" => builder.trace(value_as::<TraceOptions>(&key, value)?),
            "provider" => builder.provider(value_as::<ProviderPreferences>(&key, value)?),
            "metadata" => builder.metadata(value_as::<HashMap<String, String>>(&key, value)?),
            "plugins" => builder.plugins(value_as::<Vec<Plugin>>(&key, value)?),
            "modalities" => builder.modalities(value_as::<Vec<Modality>>(&key, value)?),
            "image_config" => {
                builder.image_config(value_as::<HashMap<String, Value>>(&key, value)?)
            }
            "response_format" => builder.response_format(value_as::<ResponseFormat>(&key, value)?),
            "reasoning" => builder.reasoning(value_as::<ReasoningConfig>(&key, value)?),
            "reasoning_effort" => builder.reasoning_effort(effort_from_value(&key, value)?),
            "reasoning_max_tokens" => builder.reasoning_max_tokens(value_as_u32(&key, value)?),
            "include_reasoning" => builder.include_reasoning(value_as_bool(&key, value)?),
            "stop" => builder.stop(value_as::<StopSequence>(&key, value)?),
            "stream_options" => builder.stream_options(value_as::<StreamOptions>(&key, value)?),
            "debug" => builder.debug(value_as::<DebugOptions>(&key, value)?),
            "parallel_tool_calls" => builder.parallel_tool_calls(value_as_bool(&key, value)?),
            unsupported => {
                bail!("OpenRouter provider does not support extra parameter `{unsupported}`");
            }
        };
    }
    Ok(())
}

fn convert_tool_calls_for_openrouter(tool_calls: Vec<ToolCall>) -> Vec<OrToolCall> {
    tool_calls
        .into_iter()
        .map(|call| {
            OrToolCall::new(
                call.id.unwrap_or_default(),
                call.name,
                call.arguments.to_string(),
            )
        })
        .collect()
}

fn convert_openrouter_tool_call(call: &OrToolCall) -> std::result::Result<ToolCall, String> {
    let arguments = match serde_json::from_str::<Value>(call.arguments_json()) {
        Ok(arguments) => arguments,
        Err(_) => {
            return Err(format!(
                "Dropped malformed OpenRouter tool call `{}` arguments.",
                call.name()
            ));
        }
    };

    Ok(ToolCall {
        id: Some(call.id().to_string()),
        name: call.name().to_string(),
        arguments,
    })
}

fn tool_delta_from_value(value: Value) -> Option<ToolCallDelta> {
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

fn validate_app_categories(app_categories: Option<&[String]>) -> Result<()> {
    if let Some(app_categories) = app_categories {
        let _ = serialize_app_categories(app_categories)?;
    }
    Ok(())
}

fn serialize_app_categories(app_categories: &[String]) -> Result<String> {
    if app_categories.is_empty() {
        bail!("OpenRouter app_categories cannot be empty when provided");
    }
    if app_categories.len() > 2 {
        bail!("OpenRouter app_categories supports at most 2 categories per request");
    }

    let mut serialized = Vec::with_capacity(app_categories.len());
    for category in app_categories {
        let trimmed = category.trim();
        if trimmed.is_empty() {
            bail!("OpenRouter app_categories cannot contain empty values");
        }
        if trimmed.len() > 30 {
            bail!("OpenRouter app category `{trimmed}` exceeds the 30 character limit");
        }
        if !trimmed
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            bail!("OpenRouter app category `{trimmed}` must be lowercase and hyphen-separated");
        }
        serialized.push(trimmed.to_string());
    }

    Ok(serialized.join(","))
}

fn first_reasoning_signature(response: &CompletionsResponse) -> Option<String> {
    response
        .choices
        .iter()
        .filter_map(|choice| choice.reasoning_details())
        .flat_map(|details| details.iter())
        .filter_map(|detail| detail.signature.clone())
        .find(|value| !value.is_empty())
}

fn finish_reason_to_string(reason: &FinishReason) -> &'static str {
    match reason {
        FinishReason::ToolCalls => "tool_calls",
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ContentFilter => "content_filter",
        FinishReason::Error => "error",
        _ => "unknown",
    }
}

fn convert_usage(usage: ResponseUsage) -> TokenUsage {
    TokenUsage {
        prompt_tokens: usage.prompt_tokens.min(i32::MAX as u32) as i32,
        cached_prompt_tokens: None,
        completion_tokens: usage.completion_tokens.min(i32::MAX as u32) as i32,
        total_tokens: usage.total_tokens.min(i32::MAX as u32) as i32,
        reasoning_tokens: None,
    }
}

#[cfg(test)]
fn usage_from_value(value: Value) -> Option<TokenUsage> {
    serde_json::from_value::<ResponseUsage>(value)
        .ok()
        .map(convert_usage)
}

fn stream_text_chunk(text: String) -> StreamChunk {
    StreamChunk {
        text: Some(text),
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

fn stream_thinking_chunk(thinking: String) -> StreamChunk {
    StreamChunk {
        text: None,
        thinking: Some(thinking),
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

fn stream_thinking_signature_chunk(thinking_signature: String) -> StreamChunk {
    StreamChunk {
        text: None,
        thinking: None,
        thinking_signature: Some(thinking_signature),
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

fn stream_tool_chunk(tool_call_delta: ToolCallDelta) -> StreamChunk {
    StreamChunk {
        text: None,
        thinking: None,
        thinking_signature: None,
        redacted_thinking: None,
        usage: None,
        provider_response_id: None,
        provider_response_status: None,
        sequence_number: None,
        tool_call_delta: Some(tool_call_delta),
        is_finished: false,
        finish_reason: None,
    }
}

fn non_negative_u32(label: &str, value: i32) -> Result<u32> {
    u32::try_from(value).with_context(|| format!("`{label}` must be a non-negative integer"))
}

fn value_as<T>(key: &str, value: Value) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).with_context(|| {
        format!("OpenRouter extra parameter `{key}` has an unsupported value shape")
    })
}

fn value_as_string(key: &str, value: Value) -> Result<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("OpenRouter extra parameter `{key}` must be a string"))
}

fn value_as_bool(key: &str, value: Value) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter extra parameter `{key}` must be a boolean"))
}

fn value_as_u32(key: &str, value: Value) -> Result<u32> {
    match value.as_u64().and_then(|value| u32::try_from(value).ok()) {
        Some(value) => Ok(value),
        None => bail!("OpenRouter extra parameter `{key}` must be an unsigned 32-bit integer"),
    }
}

fn value_as_f64(key: &str, value: Value) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter extra parameter `{key}` must be a number"))
}

fn effort_from_value(key: &str, value: Value) -> Result<Effort> {
    let raw = value_as_string(key, value)?;
    match raw.as_str() {
        "xhigh" => Ok(Effort::Xhigh),
        "high" => Ok(Effort::High),
        "medium" => Ok(Effort::Medium),
        "low" => Ok(Effort::Low),
        "minimal" => Ok(Effort::Minimal),
        "none" => Ok(Effort::None),
        _ => bail!("OpenRouter extra parameter `{key}` has unsupported reasoning effort `{raw}`"),
    }
}

fn openrouter_effort(effort: ReasoningEffort) -> Effort {
    match effort {
        ReasoningEffort::None => Effort::None,
        ReasoningEffort::Minimal => Effort::Minimal,
        ReasoningEffort::Low => Effort::Low,
        ReasoningEffort::Medium => Effort::Medium,
        ReasoningEffort::High => Effort::High,
        ReasoningEffort::XHigh => Effort::Xhigh,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projects_messages_tools_and_reasoning_effort() {
        let mut request = GenerationRequest::new()
            .with_system_prompt("system")
            .with_user_message("hello")
            .with_assistant_message("thinking done")
            .with_tool(ToolDefinition::new("lookup", "Lookup data"))
            .with_reasoning_effort(ReasoningEffort::Low);
        request.messages.push(Message {
            role: MessageRole::Assistant,
            content: String::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: Some(vec![
                ToolCall::new("lookup", json!({"q":"rust"})).with_id("call-1"),
            ]),
            tool_call_id: None,
        });
        request
            .messages
            .push(Message::tool("call-1", "tool result"));

        let projected = build_openrouter_chat_request("openrouter/model", request).unwrap();
        let value = serde_json::to_value(projected).unwrap();

        assert_eq!(value["model"], "openrouter/model");
        assert_eq!(value["messages"][0]["role"], "system");
        assert_eq!(value["messages"][1]["role"], "user");
        assert_eq!(value["messages"][3]["tool_calls"][0]["id"], "call-1");
        assert_eq!(value["messages"][4]["role"], "tool");
        assert_eq!(value["messages"][4]["tool_call_id"], "call-1");
        assert_eq!(value["tools"][0]["function"]["name"], "lookup");
        assert_eq!(value["tool_choice"], "auto");
        assert_eq!(value["reasoning"]["effort"], "low");
    }

    #[test]
    fn request_payload_preserves_assistant_reasoning_fields() {
        let mut request = GenerationRequest::new().with_user_message("hello");
        request.messages.push(Message {
            role: MessageRole::Assistant,
            content: "answer".to_string(),
            thinking: Some("step by step".to_string()),
            thinking_signature: Some("encrypted_state".to_string()),
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });

        let value = build_openrouter_chat_request_payload("openrouter/model", request).unwrap();

        assert_eq!(value["messages"][1]["role"], "assistant");
        assert_eq!(value["messages"][1]["reasoning_content"], "step by step");
        assert_eq!(
            value["messages"][1]["reasoning"]["encrypted_content"],
            "encrypted_state"
        );
    }

    #[test]
    fn missing_tool_call_id_fails_projection_before_dispatch() {
        let mut request = GenerationRequest::new().with_user_message("hello");
        request.messages.push(Message {
            role: MessageRole::Tool,
            content: "tool result".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });

        let error = build_openrouter_chat_request("openrouter/model", request).unwrap_err();

        assert!(error.to_string().contains("tool_call_id"));
    }

    #[test]
    fn unsupported_extra_parameter_fails_projection() {
        let mut request = GenerationRequest::new().with_user_message("hello");
        request
            .extra_params
            .insert("unsupported".to_string(), json!(true));

        let error = build_openrouter_chat_request("openrouter/model", request).unwrap_err();
        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn supported_extra_parameters_are_projected() {
        let mut request = GenerationRequest::new().with_user_message("hello");
        request
            .extra_params
            .insert("route".to_string(), json!("fallback"));
        request.extra_params.insert(
            "provider".to_string(),
            json!({ "allow_fallbacks": false, "require_parameters": true }),
        );
        request
            .extra_params
            .insert("transforms".to_string(), json!(["middle-out"]));
        request
            .extra_params
            .insert("reasoning_effort".to_string(), json!("high"));

        let projected = build_openrouter_chat_request("openrouter/model", request).unwrap();
        let value = serde_json::to_value(projected).unwrap();

        assert_eq!(value["route"], "fallback");
        assert_eq!(value["provider"]["allow_fallbacks"], false);
        assert_eq!(value["transforms"][0], "middle-out");
        assert_eq!(value["reasoning"]["effort"], "high");
    }

    #[test]
    fn canonical_effort_overrides_reasoning_effort_extra_parameter() {
        let request = GenerationRequest::new()
            .with_user_message("hello")
            .with_reasoning_effort(ReasoningEffort::Low)
            .with_extra_param("reasoning_effort", json!("high"));

        let projected = build_openrouter_chat_request("openrouter/model", request).unwrap();
        let value = serde_json::to_value(projected).unwrap();

        assert_eq!(value["reasoning"]["effort"], "low");
        assert!(value["reasoning"].get("max_tokens").is_none());
    }

    #[test]
    fn maps_content_reasoning_usage_finish_and_response_id() {
        let response = response_with_choice(json!({
            "finish_reason": "stop",
            "native_finish_reason": null,
            "message": {
                "content": "answer",
                "role": "assistant",
                "reasoning": "because"
            },
            "error": null,
            "index": 0,
            "logprobs": null
        }));

        let converted = convert_openrouter_response(response);
        assert_eq!(converted.content, "answer");
        assert_eq!(converted.thinking.as_deref(), Some("because"));
        assert_eq!(converted.finish_reason.as_deref(), Some("stop"));
        assert_eq!(converted.provider_response_id.as_deref(), Some("resp-1"));
        assert_eq!(converted.usage.unwrap().total_tokens, 7);
    }

    #[test]
    fn maps_tool_call_and_drops_malformed_arguments() {
        let response = response_with_choice(json!({
            "finish_reason": "tool_calls",
            "native_finish_reason": null,
            "message": {
                "content": "",
                "role": "assistant",
                "tool_calls": [
                    {
                        "id": "call-ok",
                        "type": "function",
                        "function": {
                            "name": "lookup",
                            "arguments": "{\"q\":\"rust\"}"
                        },
                        "index": 0
                    },
                    {
                        "id": "call-bad",
                        "type": "function",
                        "function": {
                            "name": "broken",
                            "arguments": "{bad"
                        },
                        "index": 1
                    }
                ]
            },
            "error": null,
            "index": 0,
            "logprobs": null
        }));

        let converted = convert_openrouter_response(response);
        assert_eq!(converted.tool_calls.len(), 1);
        assert_eq!(converted.tool_calls[0].id.as_deref(), Some("call-ok"));
        assert_eq!(converted.tool_calls[0].arguments["q"], "rust");
        assert_eq!(converted.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(converted.warnings.len(), 1);
        assert!(converted.warnings[0].contains("broken"));
    }

    #[test]
    fn maps_reasoning_detail_signature() {
        let response = response_with_choice(json!({
            "finish_reason": "stop",
            "native_finish_reason": null,
            "message": {
                "content": "answer",
                "role": "assistant",
                "reasoning_details": [
                    {
                        "type": "reasoning.text",
                        "text": "detail",
                        "signature": "sig"
                    }
                ]
            },
            "error": null,
            "index": 0,
            "logprobs": null
        }));

        let converted = convert_openrouter_response(response);
        assert_eq!(converted.thinking.as_deref(), Some("detail"));
        assert_eq!(converted.thinking_signature.as_deref(), Some("sig"));
    }

    #[test]
    fn maps_stream_tool_delta() {
        let delta = tool_delta_from_value(json!({
            "index": 2,
            "id": "call-1",
            "type": "function",
            "function": {
                "name": "lookup",
                "arguments": "{\"q\""
            }
        }))
        .unwrap();

        assert_eq!(delta.index, 2);
        assert_eq!(delta.id.as_deref(), Some("call-1"));
        assert_eq!(delta.name.as_deref(), Some("lookup"));
        assert_eq!(delta.arguments_delta.as_deref(), Some("{\"q\""));
    }

    #[tokio::test]
    async fn maps_stream_text_reasoning_completion_and_errors() {
        let events = futures::stream::iter(vec![
            UnifiedStreamEvent::ContentDelta("hel".to_string()),
            UnifiedStreamEvent::ReasoningDelta("why".to_string()),
            UnifiedStreamEvent::ToolDelta(json!({
                "index": 0,
                "id": "call-1",
                "type": "function",
                "function": { "name": "lookup", "arguments": "{}" }
            })),
            UnifiedStreamEvent::Done {
                source: openrouter_rs::types::UnifiedStreamSource::Chat,
                id: Some("resp-stream".to_string()),
                model: Some("model".to_string()),
                finish_reason: Some("tool_calls".to_string()),
                usage: Some(json!({
                    "prompt_tokens": 2,
                    "completion_tokens": 3,
                    "total_tokens": 5
                })),
            },
        ])
        .boxed();
        let (tx, mut rx) = mpsc::channel(10);
        consume_openrouter_stream(events, tx).await;

        assert_eq!(rx.recv().await.unwrap().text.as_deref(), Some("hel"));
        assert_eq!(rx.recv().await.unwrap().thinking.as_deref(), Some("why"));
        assert_eq!(
            rx.recv()
                .await
                .unwrap()
                .tool_call_delta
                .unwrap()
                .name
                .as_deref(),
            Some("lookup")
        );
        let done = rx.recv().await.unwrap();
        assert!(done.is_finished);
        assert_eq!(done.provider_response_id.as_deref(), Some("resp-stream"));
        assert_eq!(done.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(done.usage.unwrap().total_tokens, 5);
    }

    #[tokio::test]
    async fn stream_error_after_partial_output_emits_terminal_error_chunk() {
        let events = futures::stream::iter(vec![
            UnifiedStreamEvent::ContentDelta("partial".to_string()),
            UnifiedStreamEvent::Error(openrouter_rs::error::OpenRouterError::Unknown(
                "boom".to_string(),
            )),
        ])
        .boxed();
        let (tx, mut rx) = mpsc::channel(10);
        consume_openrouter_stream(events, tx).await;

        assert_eq!(rx.recv().await.unwrap().text.as_deref(), Some("partial"));
        let done = rx.recv().await.unwrap();
        assert!(done.is_finished);
        assert_eq!(done.finish_reason.as_deref(), Some("stream_error"));
    }

    fn response_with_choice(choice: Value) -> CompletionsResponse {
        serde_json::from_value(json!({
            "id": "resp-1",
            "choices": [choice],
            "created": 1,
            "model": "openrouter/model",
            "object": "chat.completion",
            "provider": "openrouter",
            "system_fingerprint": null,
            "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 4,
                "total_tokens": 7
            }
        }))
        .unwrap()
    }
}
