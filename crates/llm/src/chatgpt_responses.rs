//! ChatGPT/Codex managed-auth Responses client.

mod streaming;

use crate::openai_chat_completions::{
    OpenAiResponsesRequest, OpenAiResponsesResponse, build_responses_request_for_model,
    convert_openai_responses_output,
};
use crate::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use alan_auth::{ChatgptAuthConfig, ChatgptAuthError, ChatgptAuthManager};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, instrument};

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
        streaming::consume_openai_responses_stream(response, tx).await
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
        streaming::consume_openai_responses_stream(response, tx).await
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

    async fn generate_via_stream(&self, request: GenerationRequest) -> Result<GenerationResponse> {
        let response_request = self.build_openai_responses_request(request, true)?;
        let background_requested = response_request.background == Some(true);
        let response = self.execute_with_auth_retry(response_request, true).await?;
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
        tokio::spawn(async move {
            let _ = streaming::consume_openai_responses_stream(response, tx).await;
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

#[cfg(test)]
mod tests;
