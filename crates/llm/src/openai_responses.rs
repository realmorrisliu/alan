//! OpenAI Responses API client.
//!
//! This module provides the provider surface for the OpenAI Responses API,
//! including stable per-client prompt-cache affinity (`prompt_cache_key` and
//! `prompt_cache_retention`), server-managed continuation, `/responses/compact`,
//! and persistent WebSocket streaming with automatic fallback to HTTP SSE.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use crate::openai_chat_completions::{
    OpenAiChatCompletionsClient, OpenAiResponsesCompactRequest, OpenAiResponsesCompactResponse,
    OpenAiResponsesInputTokensRequest, OpenAiResponsesInputTokensResponse, OpenAiResponsesRequest,
    OpenAiResponsesResponse, build_responses_compact_request_for_model,
    convert_openai_responses_output,
};
use crate::session_affinity::{new_session_id, request_session_id};
use crate::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};

pub(crate) mod stream_events;
mod websocket;

use websocket::{
    DEFAULT_COOLDOWN, ResponsesWebSocket, WebSocketOutcome, is_official_openai_host, websocket_url,
};

pub use crate::openai_chat_completions::{
    OpenAiResponsesCompactRequest as OpenAiResponsesCompactApiRequest,
    OpenAiResponsesCompactResponse as OpenAiResponsesCompactApiResponse,
    OpenAiResponsesFunctionCallItem as OpenAiResponsesFunctionCallEvent,
    OpenAiResponsesFunctionCallOutputItem as OpenAiResponsesFunctionCallOutputEvent,
    OpenAiResponsesInputItem as OpenAiResponsesRequestInputItem,
    OpenAiResponsesInputMessage as OpenAiResponsesRequestInputMessage,
    OpenAiResponsesInputTokensRequest as OpenAiResponsesInputTokensApiRequest,
    OpenAiResponsesInputTokensResponse as OpenAiResponsesInputTokensApiResponse,
    OpenAiResponsesOutputTokensDetails as OpenAiResponsesUsageOutputTokensDetails,
    OpenAiResponsesReasoning as OpenAiResponsesReasoningConfig,
    OpenAiResponsesReasoningInputItem as OpenAiResponsesReasoningRequestInputItem,
    OpenAiResponsesRequest as OpenAiResponsesApiRequest,
    OpenAiResponsesResponse as OpenAiResponsesApiResponse,
    OpenAiResponsesUsage as OpenAiResponsesApiUsage,
};

const PROMPT_CACHE_KEY_PARAM: &str = "prompt_cache_key";
const PROMPT_CACHE_RETENTION_PARAM: &str = "prompt_cache_retention";

/// Streaming transport selection for [`OpenAiResponsesClient`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenAiResponsesTransport {
    /// Use WebSocket for the official `api.openai.com` host (falling back to
    /// HTTP SSE on handshake or pre-output failure) and HTTP SSE elsewhere.
    #[default]
    Auto,
    /// Always stream over HTTP SSE.
    Sse,
    /// Prefer WebSocket regardless of host, falling back to HTTP SSE on
    /// handshake or pre-output failure.
    WebSocket,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum PromptCacheRetention {
    /// Default `"24h"` for `gpt-5*` and `gpt-4.1*` models; omitted otherwise.
    #[default]
    Auto,
    Custom(String),
    Disabled,
}

/// Client for the OpenAI Responses API.
pub struct OpenAiResponsesClient {
    inner: OpenAiChatCompletionsClient,
    prompt_cache_key: String,
    prompt_cache_key_explicit: bool,
    prompt_cache_retention: PromptCacheRetention,
    transport: OpenAiResponsesTransport,
    websocket: Arc<ResponsesWebSocket>,
}

impl OpenAiResponsesClient {
    const BACKGROUND_POLL_INTERVAL: Duration = Duration::from_secs(2);

    /// Construct a client with a freshly minted, stable per-instance
    /// `prompt_cache_key` and [`OpenAiResponsesTransport::Auto`].
    pub fn with_params(api_key: &str, base_url: &str, model: &str) -> Self {
        let inner = OpenAiChatCompletionsClient::official_with_params(api_key, base_url, model);
        let ws_url = websocket_url(inner.base_url());
        Self {
            inner,
            prompt_cache_key: new_session_id(),
            prompt_cache_key_explicit: false,
            prompt_cache_retention: PromptCacheRetention::Auto,
            transport: OpenAiResponsesTransport::Auto,
            websocket: Arc::new(ResponsesWebSocket::new(ws_url, DEFAULT_COOLDOWN)),
        }
    }

    /// Override the stable per-client `prompt_cache_key`.
    ///
    /// Caller-supplied `extra_params["prompt_cache_key"]` still takes precedence
    /// on individual requests.
    pub fn with_prompt_cache_key(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            self.prompt_cache_key = trimmed.to_string();
            self.prompt_cache_key_explicit = true;
        }
        self
    }

    /// Override `prompt_cache_retention` sent on Responses requests (for
    /// example `"24h"` or `"in_memory"`).
    ///
    /// Passing an empty/whitespace string disables the default retention field
    /// even for `gpt-5*` / `gpt-4.1*` models. Caller-supplied
    /// `extra_params["prompt_cache_retention"]` still takes precedence.
    pub fn with_prompt_cache_retention(mut self, retention: impl Into<String>) -> Self {
        let retention = retention.into();
        let trimmed = retention.trim();
        self.prompt_cache_retention = if trimmed.is_empty() {
            PromptCacheRetention::Disabled
        } else {
            PromptCacheRetention::Custom(trimmed.to_string())
        };
        self
    }

    /// Configure the streaming transport mode (`Auto`, `Sse`, or `WebSocket`).
    pub fn with_transport(mut self, transport: OpenAiResponsesTransport) -> Self {
        self.transport = transport;
        self
    }

    /// Return the client's stable default `prompt_cache_key`.
    pub fn prompt_cache_key(&self) -> &str {
        &self.prompt_cache_key
    }

    /// Return the configured streaming transport mode.
    pub fn transport(&self) -> OpenAiResponsesTransport {
        self.transport
    }

    #[cfg(test)]
    pub(crate) fn with_ws_cooldown(mut self, cooldown: Duration) -> Self {
        let ws_url = websocket_url(self.inner.base_url());
        self.websocket = Arc::new(ResponsesWebSocket::new(ws_url, cooldown));
        self
    }

    #[cfg(test)]
    pub(crate) fn ws_connections_opened(&self) -> usize {
        self.websocket.connections_opened()
    }

    #[cfg(test)]
    pub(crate) fn ws_is_cooling_down(&self) -> bool {
        self.websocket.is_cooling_down()
    }

    fn clone_shared(&self) -> Self {
        Self {
            inner: self.inner.clone_with_same_config(),
            prompt_cache_key: self.prompt_cache_key.clone(),
            prompt_cache_key_explicit: self.prompt_cache_key_explicit,
            prompt_cache_retention: self.prompt_cache_retention.clone(),
            transport: self.transport,
            websocket: Arc::clone(&self.websocket),
        }
    }

    fn resolve_prompt_cache_retention(&self, model: &str) -> Option<String> {
        match &self.prompt_cache_retention {
            PromptCacheRetention::Auto => default_prompt_cache_retention(model).map(str::to_string),
            PromptCacheRetention::Custom(value) => Some(value.clone()),
            PromptCacheRetention::Disabled => None,
        }
    }

    fn prepare_responses_request(
        &self,
        request: &mut OpenAiResponsesRequest,
        session_key: Option<&str>,
    ) {
        if request.model.is_empty() {
            request.model = self.inner.model().to_string();
        }
        // Chaining with `previous_response_id` requires server-side storage.
        if request.previous_response_id.is_some() {
            request.store = Some(true);
        }

        let has_explicit_cache_key = request
            .extra_params
            .get(PROMPT_CACHE_KEY_PARAM)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        if !has_explicit_cache_key {
            let key = if self.prompt_cache_key_explicit {
                self.prompt_cache_key.clone()
            } else {
                request_session_id(session_key, &self.prompt_cache_key)
            };
            request.extra_params.insert(
                PROMPT_CACHE_KEY_PARAM.to_string(),
                serde_json::Value::String(key),
            );
        }

        match request.extra_params.get(PROMPT_CACHE_RETENTION_PARAM) {
            Some(serde_json::Value::String(value)) if !value.trim().is_empty() => {}
            Some(serde_json::Value::Null) | Some(serde_json::Value::String(_)) => {
                request.extra_params.remove(PROMPT_CACHE_RETENTION_PARAM);
            }
            Some(_) => {}
            None => {
                if let Some(retention) = self.resolve_prompt_cache_retention(&request.model) {
                    request.extra_params.insert(
                        PROMPT_CACHE_RETENTION_PARAM.to_string(),
                        serde_json::Value::String(retention),
                    );
                }
            }
        }
    }

    fn should_try_websocket(&self, request: &OpenAiResponsesRequest) -> bool {
        if request.background == Some(true) {
            return false;
        }
        match self.transport {
            OpenAiResponsesTransport::Sse => false,
            OpenAiResponsesTransport::Auto => {
                is_official_openai_host(self.inner.base_url()) && self.websocket.available()
            }
            OpenAiResponsesTransport::WebSocket => self.websocket.available(),
        }
    }

    async fn dispatch_openai_responses(
        &self,
        request: OpenAiResponsesRequest,
    ) -> Result<OpenAiResponsesResponse> {
        self.inner.openai_responses(request).await
    }

    async fn dispatch_stream_openai_responses(
        &self,
        request: OpenAiResponsesRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        if self.should_try_websocket(&request) {
            match self
                .websocket
                .stream(self.inner.api_key(), &request, &tx)
                .await?
            {
                WebSocketOutcome::Done => return Ok(()),
                WebSocketOutcome::Fallback => {}
            }
        }
        self.inner.stream_openai_responses(request, tx).await
    }

    pub async fn openai_responses(
        &self,
        mut request: OpenAiResponsesRequest,
    ) -> Result<OpenAiResponsesResponse> {
        self.prepare_responses_request(&mut request, None);
        self.dispatch_openai_responses(request).await
    }

    pub async fn stream_openai_responses(
        &self,
        mut request: OpenAiResponsesRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        self.prepare_responses_request(&mut request, None);
        self.dispatch_stream_openai_responses(request, tx).await
    }

    pub async fn retrieve_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        self.inner.retrieve_openai_response(response_id).await
    }

    pub async fn compact_openai_response(
        &self,
        request: OpenAiResponsesCompactRequest,
    ) -> Result<OpenAiResponsesCompactResponse> {
        self.inner.compact_openai_response(request).await
    }

    pub async fn count_openai_response_input_tokens(
        &self,
        request: OpenAiResponsesInputTokensRequest,
    ) -> Result<OpenAiResponsesInputTokensResponse> {
        self.inner.count_openai_response_input_tokens(request).await
    }

    pub async fn retrieve_openai_response_stream(
        &self,
        response_id: &str,
        starting_after: Option<u64>,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        self.inner
            .retrieve_openai_response_stream(response_id, starting_after, tx)
            .await
    }

    pub async fn cancel_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        self.inner.cancel_openai_response(response_id).await
    }

    /// Build a `/v1/responses/compact` request from a provider-neutral request,
    /// preserving `instructions`, `previous_response_id`, and `input` while
    /// omitting parameters rejected by the compact endpoint.
    pub fn build_openai_responses_compact_request(
        &self,
        request: GenerationRequest,
    ) -> Result<OpenAiResponsesCompactRequest> {
        build_responses_compact_request_for_model(self.inner.model().to_string(), request)
    }

    fn build_openai_responses_request(
        &self,
        request: GenerationRequest,
        stream: bool,
    ) -> Result<OpenAiResponsesRequest> {
        let session_key = request.session_key.clone();
        let mut built = self.inner.build_openai_responses_request(request, stream)?;
        self.prepare_responses_request(&mut built, session_key.as_deref());
        Ok(built)
    }

    #[cfg(test)]
    fn build_openai_responses_input_tokens_request(
        &self,
        request: GenerationRequest,
    ) -> Result<OpenAiResponsesInputTokensRequest> {
        self.inner
            .build_openai_responses_input_tokens_request(request)
    }

    async fn wait_for_background_response(
        &self,
        mut response: OpenAiResponsesResponse,
    ) -> Result<OpenAiResponsesResponse> {
        while matches!(response.status.as_deref(), Some("queued" | "in_progress")) {
            let response_id = response.id.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "OpenAI Responses background response is missing an id while status is {:?}",
                    response.status
                )
            })?;
            tokio::time::sleep(Self::BACKGROUND_POLL_INTERVAL).await;
            response = self.retrieve_openai_response(&response_id).await?;
        }
        Ok(response)
    }
}

/// Default `prompt_cache_retention` for OpenAI Responses models.
///
/// OpenAI extended prompt cache retention (`"24h"`) is supported on `gpt-5*`
/// and `gpt-4.1*` model families; other models omit the field by default.
pub(crate) fn default_prompt_cache_retention(model: &str) -> Option<&'static str> {
    let lower = model.trim().to_ascii_lowercase();
    if lower.starts_with("gpt-5") || lower.starts_with("gpt-4.1") {
        Some("24h")
    } else {
        None
    }
}

#[async_trait]
impl LlmProvider for OpenAiResponsesClient {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        let response_request = self.build_openai_responses_request(request, false)?;
        let background_requested = response_request.background == Some(true);
        let mut response = self.dispatch_openai_responses(response_request).await?;
        if background_requested {
            response = self.wait_for_background_response(response).await?;
        }
        Ok(convert_openai_responses_output(response))
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> anyhow::Result<String> {
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
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        let response_request = self.build_openai_responses_request(request, true)?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let client = self.clone_shared();
        let (status_tx, status_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let outcome = client
                .dispatch_stream_openai_responses(response_request, tx)
                .await;
            let _ = status_tx.send(outcome.err().as_ref().map(crate::safe_failure_reason));
        });
        Ok(crate::failure::guard_stream(rx, status_rx))
    }

    fn provider_name(&self) -> &'static str {
        "openai_responses"
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod websocket_tests;
