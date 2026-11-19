//! Magpie: Alan's first-party local/custom router provider.
//!
//! Magpie speaks the OpenAI Chat Completions wire protocol against a local or
//! custom router (default `http://127.0.0.1:3425/v1`). Compared with the generic
//! Chat Completions-compatible adapter it adds:
//!
//! - an optional API key (local routers commonly run unauthenticated);
//! - stable per-client session-affinity headers so the upstream KV cache stays
//!   pinned to one backend for the whole session;
//! - live `/v1/models` discovery, including per-model `thinkingLevelMap`
//!   metadata that maps Alan's canonical [`crate::ReasoningEffort`] onto router levels;
//! - streaming reasoning text and multimodal image input via the shared Chat
//!   Completions projection.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::openai_chat_completions::{
    OpenAiChatCompletionsApiFlavor, OpenAiChatCompletionsClient, ReasoningEffortMap,
};
use crate::{GenerationRequest, GenerationResponse, LlmProvider, ProviderModel, StreamChunk};

mod catalog;
pub use catalog::parse_reasoning_effort_map;
use catalog::{MagpieModel, parse_models_response};

#[cfg(test)]
mod tests;

/// Default Magpie router endpoint.
pub const MAGPIE_DEFAULT_BASE_URL: &str = "http://127.0.0.1:3425/v1";
/// Header carrying the stable session-affinity key used for upstream routing.
pub const MAGPIE_SESSION_AFFINITY_HEADER: &str = "x-session-affinity";
/// Conventional session header mirrored for routers that key on it instead.
pub const MAGPIE_SESSION_ID_HEADER: &str = "x-session-id";

const MODEL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

/// First-party Magpie provider client.
pub struct MagpieClient {
    inner: OpenAiChatCompletionsClient,
    session_id: String,
    effort_map_resolved: bool,
}

impl MagpieClient {
    /// Create a client. `base_url` defaults to [`MAGPIE_DEFAULT_BASE_URL`]; an
    /// absent or blank `api_key` sends no `Authorization` header.
    pub fn new(base_url: Option<&str>, api_key: Option<&str>, model: &str) -> Self {
        Self::with_session_id(
            base_url,
            api_key,
            model,
            crate::session_affinity::new_session_id(),
        )
    }

    /// Create a client with a caller-chosen stable session-affinity key.
    pub fn with_session_id(
        base_url: Option<&str>,
        api_key: Option<&str>,
        model: &str,
        session_id: impl Into<String>,
    ) -> Self {
        let base_url = base_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(MAGPIE_DEFAULT_BASE_URL);
        let session_id = session_id.into();
        let inner = OpenAiChatCompletionsClient::new(
            api_key.unwrap_or_default().trim(),
            base_url,
            model,
            OpenAiChatCompletionsApiFlavor::Magpie,
        )
        .with_extra_headers(session_headers(&session_id))
        .with_reasoning_effort_map(Some(default_reasoning_effort_map()));
        Self {
            inner,
            session_id,
            effort_map_resolved: false,
        }
    }

    /// Use an explicit canonical-effort → router-level map, skipping discovery.
    pub fn with_reasoning_effort_map(mut self, map: ReasoningEffortMap) -> Self {
        self.inner = self.inner.with_reasoning_effort_map(Some(map));
        self.effort_map_resolved = true;
        self
    }

    /// Stable session-affinity key sent on every request from this client.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    fn client_for_request(&self, session_key: Option<&str>) -> OpenAiChatCompletionsClient {
        let session_id = crate::session_affinity::request_session_id(session_key, &self.session_id);
        if session_id == self.session_id {
            self.inner.clone_with_same_config()
        } else {
            self.inner
                .clone_with_same_config()
                .with_extra_headers(session_headers(&session_id))
        }
    }

    async fn fetch_models(&self) -> Result<Vec<MagpieModel>> {
        let url = format!("{}/models", self.inner.base_url());
        let response = self
            .inner
            .authorize(self.inner.http().get(&url))
            .timeout(MODEL_DISCOVERY_TIMEOUT)
            .send()
            .await
            .context("Failed to reach the Magpie router model list")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Magpie model list error ({status}): {body}");
        }
        let value: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse the Magpie model list")?;
        parse_models_response(&value)
    }

    /// Adopt the router-advertised `thinkingLevelMap` for this model once, and
    /// only when a request actually asks for reasoning effort.
    async fn resolve_effort_map(&mut self, request: &GenerationRequest) {
        if self.effort_map_resolved || request.reasoning.effort.is_none() {
            return;
        }
        self.effort_map_resolved = true;
        match self.fetch_models().await {
            Ok(models) => {
                if let Some(map) = models
                    .into_iter()
                    .find(|model| model.slug == self.inner.model())
                    .and_then(|model| model.reasoning_effort_map)
                {
                    self.inner = self
                        .inner
                        .clone_with_same_config()
                        .with_reasoning_effort_map(Some(merge_with_default(map)));
                }
            }
            Err(error) => {
                tracing::debug!(error = %error, "Magpie thinkingLevelMap discovery skipped");
            }
        }
    }
}

fn session_headers(session_id: &str) -> Vec<(String, String)> {
    vec![
        (
            MAGPIE_SESSION_AFFINITY_HEADER.to_string(),
            session_id.to_string(),
        ),
        (MAGPIE_SESSION_ID_HEADER.to_string(), session_id.to_string()),
    ]
}

/// Default mapping: canonical names pass through; `max`/`ultra` clamp to the
/// highest OpenAI-compatible level so strict routers never see unknown values.
pub(crate) fn default_reasoning_effort_map() -> ReasoningEffortMap {
    crate::ReasoningEffort::VALUES
        .iter()
        .map(|effort| {
            let wire = match effort {
                crate::ReasoningEffort::Max | crate::ReasoningEffort::Ultra => "xhigh",
                other => other.as_str(),
            };
            (effort.as_str().to_string(), Some(wire.to_string()))
        })
        .collect()
}

fn merge_with_default(map: ReasoningEffortMap) -> ReasoningEffortMap {
    let mut merged = default_reasoning_effort_map();
    merged.extend(map);
    merged
}

#[async_trait]
impl LlmProvider for MagpieClient {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.resolve_effort_map(&request).await;
        let mut client = self.client_for_request(request.session_key.as_deref());
        LlmProvider::generate(&mut client, request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        LlmProvider::chat(&mut self.inner, system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        self.resolve_effort_map(&request).await;
        let mut client = self.client_for_request(request.session_key.as_deref());
        LlmProvider::generate_stream(&mut client, request).await
    }

    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        let models = self.fetch_models().await?;
        Ok(Some(
            models
                .into_iter()
                .map(MagpieModel::into_provider_model)
                .collect(),
        ))
    }

    fn provider_name(&self) -> &'static str {
        "magpie"
    }
}
