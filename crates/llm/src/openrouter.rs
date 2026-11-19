//! OpenRouter SDK-backed chat adapter.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use openrouter_rs::{
    api::chat::{ChatCompletionRequest, Message as OrMessage},
    types::{
        FinishReason, Role, Tool as OrTool, ToolCall as OrToolCall, completion::CompletionsResponse,
    },
};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    GenerationRequest, GenerationResponse, LlmProvider, Message, MessageRole, ProviderModel,
    StreamChunk, ToolCall, ToolDefinition,
};

mod catalog;
#[cfg(test)]
mod completion_tests;
mod extra_params;
mod input_projection;
mod streaming;
mod usage;

use extra_params::{apply_openrouter_extra_params, non_negative_u32, openrouter_effort};
use input_projection::convert_messages_for_openrouter;
use streaming::consume_openrouter_raw_stream;
#[cfg(test)]
use streaming::{consume_openrouter_stream, consume_router_bytes, tool_delta_from_value};
use usage::{convert_usage, request_usage_accounting};

pub use crate::provider_meta::OPENROUTER_BASE_URL;

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
        request_usage_accounting(&mut payload, true);
        let response = self
            .request_builder()?
            .json(&payload)
            .send()
            .await
            .context("OpenRouter stream request failed")?;
        let response = response.error_for_status()?;

        let (tx, rx) = mpsc::channel(100);

        let (status_tx, status_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let outcome = consume_openrouter_raw_stream(response, tx).await;
            let _ = status_tx.send(outcome.err().as_ref().map(crate::safe_failure_reason));
        });

        Ok(crate::failure::guard_stream(rx, status_rx))
    }

    fn request_builder(&self) -> Result<reqwest::RequestBuilder> {
        let request = self
            .http_client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key);
        self.apply_app_headers(request)
    }

    fn apply_app_headers(
        &self,
        mut request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder> {
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

    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        self.fetch_model_catalog().await.map(Some)
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
        session_key: _,
        response_schema: _,
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
        builder.reasoning_effort(openrouter_effort(effort)?);
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
    let response_schema = request.response_schema.clone();
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
    if let Some(schema) = response_schema {
        payload["response_format"] = serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": "response",
                "strict": true,
                "schema": schema,
            }
        });
    }
    preserve_openrouter_reasoning_fields(&mut payload, &alan_messages)?;
    request_usage_accounting(&mut payload, false);
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

#[cfg(test)]
mod tests;
