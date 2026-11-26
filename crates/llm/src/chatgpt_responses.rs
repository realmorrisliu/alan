//! ChatGPT/Codex managed-auth Responses client.

mod catalog;
mod streaming;

use crate::openai_chat_completions::{
    OpenAiResponsesRequest, OpenAiResponsesResponse, build_responses_request_for_model,
    convert_openai_responses_output,
};
use crate::{
    CalibrationKind, ChatgptSubscriptionLimitError, ChatgptSubscriptionLimitKind, DecisionRequest,
    DecisionResponse, GenerationRequest, GenerationResponse, LlmProvider, StreamChunk,
};
use alan_auth::{ChatgptAuthConfig, ChatgptAuthError, ChatgptAuthManager, new_chatgpt_session_id};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, instrument};

/// Body field the backend uses to route requests to a warm prompt cache.
const PROMPT_CACHE_KEY: &str = "prompt_cache_key";

/// Client for the Sign in with ChatGPT managed-auth OpenAI Responses surface.
///
/// Each client instance owns one stable session key, sent as the body `prompt_cache_key`
/// on every request so consecutive turns hit the same backend prompt cache.
pub struct ChatgptResponsesClient {
    client: reqwest::Client,
    auth_manager: ChatgptAuthManager,
    base_url: String,
    model: String,
    custom_headers: HashMap<String, String>,
    expected_account_id: Option<String>,
    prompt_cache_key: String,
    decision_fallback: bool,
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
        let path =
            auth_storage_path.context("ChatGPT requires an explicit Host auth storage binding")?;
        let auth_manager = ChatgptAuthManager::new(ChatgptAuthConfig::with_storage_path(path))
            .context("Failed to initialize ChatGPT auth manager")?;
        let expected_account_id = auth_manager.bound_account_id(expected_account_id.as_deref())?;
        Ok(Self {
            client: reqwest::Client::new(),
            auth_manager,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            custom_headers,
            expected_account_id,
            prompt_cache_key: new_chatgpt_session_id(),
            decision_fallback: false,
        })
    }

    /// Opt in to uncalibrated structured-generation decision fallback via
    /// [`crate::decide_via_generation`].
    ///
    /// Defaults to `false`. When enabled, [`LlmProvider::supports_decisions`]
    /// returns `true` and [`LlmProvider::decision_calibration`] reports
    /// [`CalibrationKind::Uncalibrated`] (never [`CalibrationKind::Calibrated`]).
    pub fn with_decision_fallback(mut self, enabled: bool) -> Self {
        self.decision_fallback = enabled;
        self
    }

    /// Pin the request-affinity cache key, for a host that keeps one routing key across
    /// client reconstruction (for example one per Agent Process).
    pub fn with_prompt_cache_key(mut self, key: impl Into<String>) -> Self {
        self.prompt_cache_key = key.into();
        self
    }

    /// The stable request-affinity key sent with every request from this client.
    pub fn prompt_cache_key(&self) -> &str {
        &self.prompt_cache_key
    }

    /// Credentials for one request. `rejected` names the access token a backend `401`
    /// just rejected; it forces a refresh unless another request already rotated it.
    async fn request_auth(&self, rejected: Option<&str>) -> Result<alan_auth::ChatgptRequestAuth> {
        // Re-read ChatgptAuthManager on every request so a client constructed while logged
        // out picks up a later live `/login` on the very next turn without restarting Alan,
        // while clients already bound to a workspace account keep rejecting account swaps.
        let account = self.expected_account_id.as_deref();
        Ok(match rejected {
            Some(token) => {
                self.auth_manager
                    .refresh_rejected_auth_for_account(account, token)
                    .await?
            }
            None => self.auth_manager.request_auth_for_account(account).await?,
        })
    }

    fn apply_custom_headers(
        &self,
        mut builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        for (name, value) in &self.custom_headers {
            // Managed Bearer authorization has a single owner.
            if !name.eq_ignore_ascii_case("authorization") {
                builder = builder.header(name, value);
            }
        }
        builder
    }

    fn authorize(
        &self,
        builder: reqwest::RequestBuilder,
        auth: &alan_auth::ChatgptRequestAuth,
    ) -> reqwest::RequestBuilder {
        self.apply_custom_headers(builder).header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", auth.access_token),
        )
    }

    /// Send a managed request, refreshing credentials before dispatch when they are
    /// near expiry and retrying exactly once after a `401`. The caller interprets the
    /// final status, including a second `401`.
    async fn send_with_auth_retry(
        &self,
        operation: &'static str,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let auth = self.request_auth(None).await?;
        let response = self
            .authorize(build(), &auth)
            .send()
            .await
            .with_context(|| format!("Failed to {operation}"))?;
        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        debug!(
            operation,
            "ChatGPT request returned 401; refreshing managed auth once"
        );
        drop(response);
        let auth = self.request_auth(Some(&auth.access_token)).await?;
        self.authorize(build(), &auth)
            .send()
            .await
            .with_context(|| format!("Failed to {operation}"))
    }

    fn build_openai_responses_request(
        &self,
        request: GenerationRequest,
        stream: bool,
    ) -> Result<OpenAiResponsesRequest> {
        let mut request = build_responses_request_for_model(self.model.clone(), request, stream)?;
        request.instructions = Some(request.instructions.unwrap_or_default());
        // Sign in with ChatGPT requires `store=false` and rejects temperature,
        // max_output_tokens, prompt_cache_retention, and prompt_cache_options.
        request.store = Some(false);
        request.temperature = None;
        request.max_output_tokens = None;
        request.extra_params.remove("prompt_cache_retention");
        request.extra_params.remove("prompt_cache_options");
        Ok(request)
    }

    /// Route the request to `affinity_key`'s prompt cache unless the caller chose a key.
    fn bind_prompt_cache_key(&self, request: &mut OpenAiResponsesRequest, affinity_key: &str) {
        let explicit = request
            .extra_params
            .get(PROMPT_CACHE_KEY)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|key| !key.is_empty());
        if !explicit {
            request.extra_params.insert(
                PROMPT_CACHE_KEY.to_string(),
                serde_json::Value::String(affinity_key.to_string()),
            );
        }
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
        streaming::consume_openai_responses_stream(response, tx).await
    }

    #[instrument(skip(self))]
    pub async fn retrieve_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        let url = format!("{}/responses/{}", self.base_url, response_id);
        let response = self
            .send_with_auth_retry("retrieve ChatGPT Responses API response", || {
                self.client.get(&url)
            })
            .await?;
        check_chatgpt_response_status(response)
            .await?
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
        let mut url = format!("{}/responses/{}?stream=true", self.base_url, response_id);
        if let Some(starting_after) = starting_after {
            url.push_str(&format!("&starting_after={starting_after}"));
        }
        let response = self
            .send_with_auth_retry("retrieve ChatGPT Responses API response", || {
                self.client
                    .get(&url)
                    .header(reqwest::header::ACCEPT, "text/event-stream")
            })
            .await?;
        let response = check_chatgpt_response_status(response).await?;
        streaming::consume_openai_responses_stream(response, tx).await
    }

    #[instrument(skip(self))]
    pub async fn cancel_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        let url = format!("{}/responses/{}/cancel", self.base_url, response_id);
        let response = self
            .send_with_auth_retry("cancel ChatGPT Responses API response", || {
                self.client.post(&url)
            })
            .await?;
        check_chatgpt_response_status(response)
            .await?
            .json()
            .await
            .context("Failed to parse cancelled ChatGPT Responses API response")
    }

    async fn execute_with_auth_retry(
        &self,
        request: OpenAiResponsesRequest,
        stream: bool,
    ) -> Result<reqwest::Response> {
        self.execute_with_auth_retry_for_session(request, stream, &self.prompt_cache_key)
            .await
    }

    async fn execute_with_auth_retry_for_session(
        &self,
        mut request: OpenAiResponsesRequest,
        stream: bool,
        affinity_key: &str,
    ) -> Result<reqwest::Response> {
        self.validate_request(&request)?;
        self.bind_prompt_cache_key(&mut request, affinity_key);
        let url = format!("{}/responses", self.base_url);
        let response = self
            .send_with_auth_retry("send request to ChatGPT Responses API", || {
                let builder = self.client.post(&url).json(&request);
                if stream {
                    builder.header(reqwest::header::ACCEPT, "text/event-stream")
                } else {
                    builder
                }
            })
            .await?;
        if stream {
            debug!("Started ChatGPT streaming Responses request");
        }
        check_chatgpt_response_status(response).await
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

    async fn generate_via_stream(&self, request: GenerationRequest) -> Result<GenerationResponse> {
        let affinity_key = crate::session_affinity::request_session_id(
            request.session_key.as_deref(),
            &self.prompt_cache_key,
        );
        let response_request = self.build_openai_responses_request(request, true)?;
        let background_requested = response_request.background == Some(true);
        let response = self
            .execute_with_auth_retry_for_session(response_request, true, &affinity_key)
            .await?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let stream_task =
            tokio::spawn(
                async move { streaming::consume_openai_responses_stream(response, tx).await },
            );
        let collected = streaming::collect_streamed_generation(rx).await;
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
    fn account_identity(&self) -> Option<&str> {
        self.expected_account_id.as_deref()
    }

    async fn model_catalog(&self) -> Result<Option<Vec<crate::ProviderModel>>> {
        self.fetch_model_catalog().await.map(Some)
    }

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
        let affinity_key = crate::session_affinity::request_session_id(
            request.session_key.as_deref(),
            &self.prompt_cache_key,
        );
        let response_request = self.build_openai_responses_request(request, true)?;
        let response = self
            .execute_with_auth_retry_for_session(response_request, true, &affinity_key)
            .await?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let (status_tx, status_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let outcome = streaming::consume_openai_responses_stream(response, tx).await;
            let _ = status_tx.send(outcome.err().as_ref().map(crate::safe_failure_reason));
        });
        Ok(crate::failure::guard_stream(rx, status_rx))
    }

    fn supports_decisions(&self) -> bool {
        self.decision_fallback
    }

    fn decision_calibration(&self) -> Option<CalibrationKind> {
        self.decision_fallback
            .then_some(CalibrationKind::Uncalibrated)
    }

    async fn decide(&mut self, request: DecisionRequest) -> Result<DecisionResponse> {
        anyhow::ensure!(
            self.decision_fallback,
            "Provider '{}' does not support typed decisions unless decision fallback is enabled",
            self.provider_name()
        );
        let model = self.model.clone();
        crate::decide_via_generation(self, &model, request).await
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
    if let Err(status_err) = response.error_for_status_ref() {
        let body = response.text().await.unwrap_or_default();
        if body.contains("subscription_sharing_workplace_individual_usage_limit_exceeded") {
            return Err(ChatgptSubscriptionLimitError {
                kind: ChatgptSubscriptionLimitKind::WorkplaceIndividualUsageLimitExceeded,
            }
            .into());
        }
        if body.contains("subscription_sharing_usage_limit_exceeded") {
            return Err(ChatgptSubscriptionLimitError {
                kind: ChatgptSubscriptionLimitKind::SharedUsageLimitExceeded,
            }
            .into());
        }
        if body.trim().is_empty() {
            return Err(status_err.into());
        }
        return Err(anyhow::Error::new(status_err).context(body));
    }
    Ok(response)
}

#[cfg(test)]
mod decisions_tests;
#[cfg(test)]
mod tests;
