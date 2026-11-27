//! Google Gemini GenerateContent API client via Vertex AI.
//!
//! Supports both Application Default Credentials (via `gcloud auth print-access-token`
//! with TTL caching and single 401 refresh-retry) and Vertex AI Express Mode (`x-goog-api-key`).

use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::{error, instrument};

use crate::{
    CalibrationKind, DecisionRequest, DecisionResponse, GenerationRequest, GenerationResponse,
    LlmProvider, ProviderModel, StreamChunk as UnifiedStreamChunk, ToolCall as LlmToolCall,
};

mod auth;
mod catalog;
mod decisions;
mod endpoint;
mod input_projection;
mod signatures;
mod streaming;
mod wire;

pub use auth::{GcloudAccessTokenSource, VertexAccessTokenSource, VertexAuth};
pub use decisions::DEFAULT_GEMINI_DECISION_MODEL;
pub use endpoint::VertexClientConfig;
pub use streaming::{StreamCandidate, StreamChunk};
pub use wire::{
    Candidate, Content, FileData, FunctionCall, FunctionCallingConfig, FunctionDeclaration,
    FunctionResponse, GenerationConfig, GoogleGeminiGenerateContentRequest,
    GoogleGeminiGenerateContentResponse, InlineData, Part, PartialArg, PromptFeedback,
    SafetyRating, ThinkingConfig, Tool, ToolConfig, UsageMetadata,
};

use auth::{ADC_TOKEN_TTL, AdcTokenCache, AuthState};
use catalog::{build_gemini_thinking_config, supports_stream_function_call_arguments};
use endpoint::{Endpoint, Method, Transport};
use input_projection::project_request_input;
use signatures::TurnSignatures;

/// Client for the Google Gemini GenerateContent API on Vertex AI.
pub struct GoogleGeminiGenerateContentClient {
    model: String,
    decision_model: Option<String>,
    transport: Transport,
}

impl GoogleGeminiGenerateContentClient {
    /// Construct a client from an explicit Vertex AI configuration (ADC or Express Mode).
    pub fn from_vertex_config(cfg: VertexClientConfig) -> Result<Self> {
        Self::build(cfg, None, None)
    }

    /// Construct a client from a Vertex AI configuration with an explicit Host `auth.json` path.
    pub fn from_vertex_config_with_auth_storage(
        cfg: VertexClientConfig,
        auth_storage_path: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        Self::build(cfg, None, auth_storage_path)
    }

    /// Construct a client with an injected ADC token source (for tests or custom credentials).
    pub fn from_vertex_config_with_token_source(
        cfg: VertexClientConfig,
        token_source: Arc<dyn VertexAccessTokenSource>,
    ) -> Result<Self> {
        Self::build(cfg, Some(token_source), None)
    }

    /// Create an ADC-authenticated client with explicit project, location, and model.
    pub fn with_params(project_id: &str, location: &str, model: &str) -> Self {
        Self::from_vertex_config(VertexClientConfig {
            project_id: Some(project_id.to_string()),
            location: location.to_string(),
            model: model.to_string(),
            auth: VertexAuth::Adc,
            base_url: None,
        })
        .expect("valid Vertex AI client parameters")
    }

    fn build(
        cfg: VertexClientConfig,
        token_source: Option<Arc<dyn VertexAccessTokenSource>>,
        auth_storage_path: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        let (api_key_mode, auth_state) = match cfg.auth {
            VertexAuth::Adc => {
                let auth = match token_source {
                    Some(source) => {
                        AuthState::Adc(Arc::new(AdcTokenCache::new(source, ADC_TOKEN_TTL)))
                    }
                    None => {
                        AuthState::from_auth_with_storage_path(VertexAuth::Adc, auth_storage_path)
                    }
                };
                (false, auth)
            }
            VertexAuth::ApiKey(key) => {
                let trimmed = key.trim();
                anyhow::ensure!(
                    !trimmed.is_empty(),
                    "Vertex AI Express Mode requires a non-empty API key"
                );
                (
                    true,
                    AuthState::from_auth(VertexAuth::ApiKey(trimmed.to_string())),
                )
            }
        };

        let endpoint = Endpoint::resolve(
            cfg.project_id.as_deref(),
            &cfg.location,
            &cfg.model,
            api_key_mode,
            cfg.base_url.as_deref(),
        )?;

        Ok(Self {
            model: cfg.model.trim().to_string(),
            decision_model: None,
            transport: Transport {
                http: reqwest::Client::new(),
                endpoint,
                auth: auth_state,
            },
        })
    }

    /// Override the default decision model used for native `POST /v1beta/decisions`
    /// requests when `DecisionRequest::model_override` is not supplied.
    pub fn with_decision_model(mut self, model: impl Into<String>) -> Self {
        let model = model.into();
        let trimmed = model.trim();
        self.decision_model = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self
    }

    /// Resolved non-streaming endpoint URL.
    pub fn endpoint_url(&self) -> String {
        self.transport.endpoint.url(Method::Generate)
    }

    /// Resolved SSE streaming endpoint URL.
    pub fn stream_endpoint_url(&self) -> String {
        self.transport.endpoint.url(Method::StreamSse)
    }

    /// Resolved native `/v1beta/decisions` endpoint URL.
    pub fn decisions_endpoint_url(&self) -> String {
        self.transport.endpoint.decisions_url().to_string()
    }

    /// Generate content (non-streaming).
    #[instrument(skip(self, request))]
    pub async fn generate_content(
        &mut self,
        request: GoogleGeminiGenerateContentRequest,
    ) -> Result<GoogleGeminiGenerateContentResponse> {
        let response = self.transport.post(Method::Generate, &request).await?;
        let response_text = response
            .text()
            .await
            .context("Failed to read Gemini response body")?;

        serde_json::from_str(&response_text).map_err(|e| {
            error!(
                "Failed to parse Gemini response: {}\nResponse body: {}",
                e,
                &response_text[..response_text.len().min(2000)]
            );
            anyhow::anyhow!("Failed to parse Gemini response: {}", e)
        })
    }

    /// Generate content with streaming (SSE).
    #[instrument(skip(self, request, tx))]
    pub async fn stream_generate_content(
        &mut self,
        request: GoogleGeminiGenerateContentRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        let response = self.transport.post(Method::StreamSse, &request).await?;
        streaming::consume_response_stream(response, tx).await
    }

    /// Simple chat helper - send message and get text response.
    pub async fn chat(&mut self, user_message: &str) -> Result<String> {
        let request = GoogleGeminiGenerateContentRequest {
            contents: vec![Content::user(vec![Part::text(user_message)])],
            system_instruction: None,
            tools: None,
            tool_config: None,
            generation_config: None,
        };

        let response = self.generate_content(request).await?;
        Ok(extract_primary_text(&response))
    }

    /// Chat with system instruction.
    pub async fn chat_with_system(&mut self, system: &str, user_message: &str) -> Result<String> {
        let request = GoogleGeminiGenerateContentRequest {
            contents: vec![Content::user(vec![Part::text(user_message)])],
            system_instruction: Some(Content {
                role: None,
                parts: vec![Part::text(system)],
            }),
            tools: None,
            tool_config: None,
            generation_config: None,
        };

        let response = self.generate_content(request).await?;
        Ok(extract_primary_text(&response))
    }

    fn build_request(
        &self,
        request: GenerationRequest,
        streaming: bool,
    ) -> Result<GoogleGeminiGenerateContentRequest> {
        crate::message::reject_retired_message_overrides(&request)?;
        let thinking_config = build_gemini_thinking_config(&self.model, request.reasoning.effort)?;
        let input = project_request_input(
            &self.model,
            request.system_prompt.as_deref(),
            &request.messages,
        )?;

        let has_tools = !request.tools.is_empty();
        let tools = if has_tools {
            let declarations = request
                .tools
                .into_iter()
                .map(|tool| FunctionDeclaration {
                    name: tool.name,
                    description: tool.description,
                    parameters: tool.parameters,
                })
                .collect();
            Some(vec![Tool {
                function_declarations: declarations,
            }])
        } else {
            None
        };

        let tool_config = (streaming
            && has_tools
            && supports_stream_function_call_arguments(&self.model))
        .then(|| ToolConfig {
            function_calling_config: Some(FunctionCallingConfig {
                mode: Some("AUTO".to_string()),
                stream_function_call_arguments: Some(true),
            }),
        });

        Ok(GoogleGeminiGenerateContentRequest {
            contents: input.contents,
            system_instruction: input.system_instruction,
            tools,
            tool_config,
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
                top_p: None,
                top_k: None,
                thinking_config,
                response_mime_type: request
                    .response_schema
                    .as_ref()
                    .map(|_| "application/json".to_string()),
                response_schema: request.response_schema,
            }),
        })
    }
}

fn extract_primary_text(response: &GoogleGeminiGenerateContentResponse) -> String {
    select_primary_candidate(&response.candidates)
        .and_then(|c| c.content.as_ref())
        .map(|c| {
            c.parts
                .iter()
                .filter(|p| !p.is_thought())
                .filter_map(|p| p.text.as_deref())
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn select_primary_candidate(candidates: &[Candidate]) -> Option<&Candidate> {
    candidates
        .iter()
        .find(|candidate| candidate.index == Some(0))
        .or_else(|| candidates.first())
}

fn is_blocking_finish_reason(finish_reason: &str) -> bool {
    finish_reason.eq_ignore_ascii_case("SAFETY")
        || finish_reason.eq_ignore_ascii_case("RECITATION")
        || finish_reason.eq_ignore_ascii_case("MALFORMED_FUNCTION_CALL")
        || finish_reason.eq_ignore_ascii_case("UNEXPECTED_TOOL_CALL")
        || finish_reason.eq_ignore_ascii_case("BLOCKLIST")
        || finish_reason.eq_ignore_ascii_case("PROHIBITED_CONTENT")
        || finish_reason.eq_ignore_ascii_case("SPII")
        || finish_reason.eq_ignore_ascii_case("IMAGE_SAFETY")
        || finish_reason.eq_ignore_ascii_case("OTHER")
}

fn blocking_finish_reason_message(finish_reason: &str) -> &'static str {
    if finish_reason.eq_ignore_ascii_case("SAFETY") {
        "Response blocked by safety filter"
    } else if finish_reason.eq_ignore_ascii_case("RECITATION") {
        "Response blocked due to recitation"
    } else if finish_reason.eq_ignore_ascii_case("MALFORMED_FUNCTION_CALL") {
        "Response stopped due to malformed function call"
    } else if finish_reason.eq_ignore_ascii_case("UNEXPECTED_TOOL_CALL") {
        "Response stopped due to unexpected tool call"
    } else if finish_reason.eq_ignore_ascii_case("BLOCKLIST") {
        "Response blocked due to blocklist"
    } else if finish_reason.eq_ignore_ascii_case("PROHIBITED_CONTENT") {
        "Response blocked due to prohibited content"
    } else if finish_reason.eq_ignore_ascii_case("SPII") {
        "Response blocked due to sensitive personal information (SPII)"
    } else if finish_reason.eq_ignore_ascii_case("IMAGE_SAFETY") {
        "Response blocked by image safety filter"
    } else {
        "Response stopped with finish reason OTHER"
    }
}

#[async_trait::async_trait]
impl LlmProvider for GoogleGeminiGenerateContentClient {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        let gemini_request = self.build_request(request, false)?;
        let response = self.generate_content(gemini_request).await?;

        if let Some(feedback) = response.prompt_feedback
            && let Some(block_reason) = feedback.block_reason
        {
            anyhow::bail!("Content blocked by safety filter: {}", block_reason);
        }

        let usage = response.usage_metadata.map(|u| u.to_token_usage());
        let provider_response_id = response.response_id.filter(|id| !id.trim().is_empty());

        let Some(candidate) = select_primary_candidate(&response.candidates) else {
            return Ok(GenerationResponse {
                content: String::new(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id,
                provider_response_status: None,
            });
        };

        if let Some(finish_reason) = &candidate.finish_reason
            && is_blocking_finish_reason(finish_reason)
        {
            anyhow::bail!("{}", blocking_finish_reason_message(finish_reason));
        }

        let content = candidate
            .content
            .clone()
            .unwrap_or_else(|| Content::model(vec![]));

        let mut text = String::new();
        let mut thinking = String::new();
        let mut tool_calls = Vec::new();
        let mut signatures = TurnSignatures::default();

        for part in content.parts {
            if part.is_thought() {
                signatures.record_text(part.thought_signature.as_deref());
                if let Some(thought_text) = part.text {
                    thinking.push_str(&thought_text);
                }
                continue;
            }
            if let Some(part_text) = part.text {
                signatures.record_text(part.thought_signature.as_deref());
                text.push_str(&part_text);
                continue;
            }
            if let Some(fc) = part.function_call {
                let ordinal = tool_calls.len();
                signatures.record_call(ordinal, part.thought_signature.as_deref());
                tool_calls.push(LlmToolCall {
                    id: fc.id,
                    name: fc.name,
                    arguments: fc.args,
                });
                continue;
            }
            if part.thought_signature.is_some() {
                signatures.record_text(part.thought_signature.as_deref());
            }
        }

        Ok(GenerationResponse {
            content: text,
            thinking: (!thinking.is_empty()).then_some(thinking),
            thinking_signature: signatures.encode(),
            redacted_thinking: Vec::new(),
            tool_calls,
            usage,
            finish_reason: candidate.finish_reason.clone(),
            warnings: Vec::new(),
            provider_response_id,
            provider_response_status: None,
        })
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        if let Some(sys) = system {
            self.chat_with_system(sys, user).await
        } else {
            self.chat(user).await
        }
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<UnifiedStreamChunk>> {
        let gemini_request = self.build_request(request, true)?;

        let (gemini_tx, gemini_rx) = tokio::sync::mpsc::channel::<StreamChunk>(128);
        let (tx, rx) = tokio::sync::mpsc::channel(256);
        tokio::spawn(streaming::project_chunks(gemini_rx, tx));

        let transport = self.transport.clone();
        let (status_tx, status_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let outcome = async {
                let response = transport.post(Method::StreamSse, &gemini_request).await?;
                streaming::consume_response_stream(response, gemini_tx).await
            }
            .await;
            let _ = status_tx.send(outcome.err().as_ref().map(crate::safe_failure_reason));
        });

        Ok(crate::failure::guard_stream(rx, status_rx))
    }

    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        catalog::discover_vertex_model_catalog(&self.transport).await
    }

    fn supports_decisions(&self) -> bool {
        true
    }

    fn decision_calibration(&self) -> Option<CalibrationKind> {
        Some(CalibrationKind::Calibrated)
    }

    async fn decide(&mut self, request: DecisionRequest) -> Result<DecisionResponse> {
        self.decide_gemini(request).await
    }

    fn provider_name(&self) -> &'static str {
        "google_gemini_generate_content"
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod http_tests;

#[cfg(test)]
mod catalog_http_tests;
