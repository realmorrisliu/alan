//! Google Gemini GenerateContent API client.
//!
//! This module provides a minimal client for the Google Gemini GenerateContent API via Vertex AI.
//! Authentication is handled via `gcloud auth print-access-token`.

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::{debug, error, instrument, warn};

use crate::ReasoningEffort;

mod input_projection;

use input_projection::project_request_input;

/// Client for the Google Gemini GenerateContent API.
pub struct GoogleGeminiGenerateContentClient {
    /// HTTP client
    client: reqwest::Client,
    /// GCP Project ID
    project_id: String,
    /// GCP Location (e.g., us-central1)
    location: String,
    /// Model name (e.g., gemini-2.0-flash)
    model: String,
    /// Cached access token
    access_token: Option<String>,
}

// ============================================================================
// Request Types
// ============================================================================

/// Request body for the Google Gemini GenerateContent API.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleGeminiGenerateContentRequest {
    /// Conversation contents
    pub contents: Vec<Content>,
    /// System instruction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
    /// Tools for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Generation configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
}

/// Content represents a message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    /// Role: "user", "model", or "function"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Parts of the content (may be missing in some responses)
    #[serde(default)]
    pub parts: Vec<Part>,
}

/// Part of content - can be text, function call, or function response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// Text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Function call from model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    /// Function response to model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
}

/// Function call requested by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Arguments as JSON object
    pub args: serde_json::Value,
}

/// Response to a function call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    /// Function name
    pub name: String,
    /// Response data
    pub response: serde_json::Value,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// Function declarations
    pub function_declarations: Vec<FunctionDeclaration>,
}

/// Function declaration schema
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDeclaration {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Parameters JSON schema
    pub parameters: serde_json::Value,
}

/// Generation configuration
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// Temperature (0.0-2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Max output tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    /// Top-P sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-K sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    /// Gemini thinking configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
}

/// Gemini thinking controls.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i32>,
}

// ============================================================================
// Response Types
// ============================================================================

/// Response from the Google Gemini GenerateContent API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleGeminiGenerateContentResponse {
    /// Generated candidates
    #[serde(default)]
    pub candidates: Vec<Candidate>,
    /// Usage metadata
    pub usage_metadata: Option<UsageMetadata>,
    /// Model version
    pub model_version: Option<String>,
    /// Prompt feedback (e.g., when blocked by safety filters)
    pub prompt_feedback: Option<PromptFeedback>,
}

/// A generated candidate response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// Content of the response
    pub content: Option<Content>,
    /// Why generation stopped
    pub finish_reason: Option<String>,
    /// Index of this candidate
    pub index: Option<i32>,
    /// Safety ratings for the generated content
    #[serde(default)]
    pub safety_ratings: Vec<SafetyRating>,
}

/// Prompt feedback when content is blocked
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    /// Why the prompt was blocked
    pub block_reason: Option<String>,
    /// Safety ratings for the prompt
    #[serde(default)]
    pub safety_ratings: Vec<SafetyRating>,
}

/// Safety rating for content
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    /// Safety category
    pub category: String,
    /// Probability of harm
    pub probability: String,
}

/// Token usage metadata
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    pub prompt_token_count: Option<i32>,
    pub candidates_token_count: Option<i32>,
    pub total_token_count: Option<i32>,
}

// ============================================================================
// Client Implementation
// ============================================================================

impl GoogleGeminiGenerateContentClient {
    /// Create a client with explicit parameters
    pub fn with_params(project_id: &str, location: &str, model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            project_id: project_id.to_string(),
            location: location.to_string(),
            model: model.to_string(),
            access_token: None,
        }
    }

    /// Get access token via gcloud CLI
    fn get_access_token(&mut self) -> Result<String> {
        // Return cached token if available
        if let Some(ref token) = self.access_token {
            return Ok(token.clone());
        }

        debug!("Fetching access token via gcloud");

        let output = Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .output()
            .context("Failed to run gcloud command. Is gcloud CLI installed and authenticated?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("gcloud auth failed: {}", stderr);
        }

        let token = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in access token")?
            .trim()
            .to_string();

        self.access_token = Some(token.clone());
        Ok(token)
    }

    /// Build the API endpoint URL
    fn endpoint(&self) -> String {
        format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:generateContent",
            self.location, self.project_id, self.location, self.model
        )
    }

    /// Build the streaming API endpoint URL
    fn stream_endpoint(&self) -> String {
        format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:streamGenerateContent",
            self.location, self.project_id, self.location, self.model
        )
    }

    /// Generate content (non-streaming)
    #[instrument(skip(self, request))]
    pub async fn generate_content(
        &mut self,
        request: GoogleGeminiGenerateContentRequest,
    ) -> Result<GoogleGeminiGenerateContentResponse> {
        let token = self.get_access_token()?;
        let endpoint = self.endpoint();

        debug!(%endpoint, "Sending generateContent request");

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&token)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Gemini API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            // Clear token on auth error to force refresh
            if status.as_u16() == 401 {
                warn!("Auth error, clearing cached token");
                self.access_token = None;
            }
            anyhow::bail!("Gemini API error ({}): {}", status, error_text);
        }

        // Get response text first for better error diagnostics
        let response_text = response
            .text()
            .await
            .context("Failed to read Gemini response body")?;

        // Try to parse as JSON
        let result: GoogleGeminiGenerateContentResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                error!(
                    "Failed to parse Gemini response: {}\nResponse body: {}",
                    e,
                    &response_text[..response_text.len().min(2000)]
                );
                anyhow::anyhow!("Failed to parse Gemini response: {}", e)
            })?;

        Ok(result)
    }

    /// Generate content with streaming (SSE)
    #[instrument(skip(self, request, tx))]
    pub async fn stream_generate_content(
        &mut self,
        request: GoogleGeminiGenerateContentRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        let token = self.get_access_token()?;
        let endpoint = self.stream_endpoint();

        debug!(%endpoint, "Sending streamGenerateContent request");

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&token)
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to Gemini API")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if status.as_u16() == 401 {
                warn!("Auth error, clearing cached token");
                self.access_token = None;
            }
            anyhow::bail!("Gemini streaming API error ({}): {}", status, error_text);
        }

        // Process SSE stream with event-boundary parsing.
        let mut stream = response.bytes_stream();
        let mut parser = SseEventParser::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Failed to read stream chunk")?;
            for data in parser.push(&chunk) {
                if data == "[DONE]" {
                    debug!("Stream completed");
                    return Ok(());
                }

                match serde_json::from_str::<StreamChunk>(&data) {
                    Ok(stream_chunk) => {
                        if tx.send(stream_chunk).await.is_err() {
                            debug!("Receiver dropped, stopping stream");
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        debug!(?e, data, "Failed to parse stream chunk");
                    }
                }
            }
        }

        for data in parser.finish() {
            if data == "[DONE]" {
                debug!("Stream completed");
                return Ok(());
            }

            match serde_json::from_str::<StreamChunk>(&data) {
                Ok(stream_chunk) => {
                    if tx.send(stream_chunk).await.is_err() {
                        debug!("Receiver dropped, stopping stream");
                        return Ok(());
                    }
                }
                Err(e) => {
                    debug!(?e, data, "Failed to parse stream chunk");
                }
            }
        }

        Ok(())
    }

    /// Simple chat helper - send message and get text response
    pub async fn chat(&mut self, user_message: &str) -> Result<String> {
        let request = GoogleGeminiGenerateContentRequest {
            contents: vec![Content {
                role: Some("user".to_string()),
                parts: vec![Part {
                    text: Some(user_message.to_string()),
                    function_call: None,
                    function_response: None,
                }],
            }],
            system_instruction: None,
            tools: None,
            generation_config: None,
        };

        let response = self.generate_content(request).await?;

        // Extract text from first candidate
        let text = response
            .candidates
            .first()
            .and_then(|c| c.content.as_ref())
            .and_then(|c| c.parts.first())
            .and_then(|p| p.text.clone())
            .unwrap_or_default();

        Ok(text)
    }

    /// Chat with system instruction
    pub async fn chat_with_system(&mut self, system: &str, user_message: &str) -> Result<String> {
        let request = GoogleGeminiGenerateContentRequest {
            contents: vec![Content {
                role: Some("user".to_string()),
                parts: vec![Part {
                    text: Some(user_message.to_string()),
                    function_call: None,
                    function_response: None,
                }],
            }],
            system_instruction: Some(Content {
                role: None,
                parts: vec![Part {
                    text: Some(system.to_string()),
                    function_call: None,
                    function_response: None,
                }],
            }),
            tools: None,
            generation_config: None,
        };

        let response = self.generate_content(request).await?;

        let text = response
            .candidates
            .first()
            .and_then(|c| c.content.as_ref())
            .and_then(|c| c.parts.first())
            .and_then(|p| p.text.clone())
            .unwrap_or_default();

        Ok(text)
    }
}

// ============================================================================
// Streaming Response Types
// ============================================================================

/// Stream chunk from Gemini streaming API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunk {
    /// Generated candidates
    #[serde(default)]
    pub candidates: Vec<StreamCandidate>,
    /// Usage metadata (only present in final chunk)
    pub usage_metadata: Option<UsageMetadata>,
    /// Prompt feedback (e.g., blocked prompt) in some streamed responses.
    pub prompt_feedback: Option<PromptFeedback>,
}

/// A candidate in streaming response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamCandidate {
    /// Content of the response
    pub content: Option<Content>,
    /// Why generation stopped (only in final chunk)
    pub finish_reason: Option<String>,
    /// Index of this candidate
    pub index: Option<i32>,
}

// ============================================================================
// Helper functions
// ============================================================================

impl Part {
    /// Create a text part
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            function_call: None,
            function_response: None,
        }
    }

    /// Create a function call part
    pub fn function_call(name: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            text: None,
            function_call: Some(FunctionCall {
                name: name.into(),
                args,
            }),
            function_response: None,
        }
    }

    /// Create a function response part
    pub fn function_response(name: impl Into<String>, response: serde_json::Value) -> Self {
        Self {
            text: None,
            function_call: None,
            function_response: Some(FunctionResponse {
                name: name.into(),
                response,
            }),
        }
    }
}

impl Content {
    /// Create user content
    pub fn user(parts: Vec<Part>) -> Self {
        Self {
            role: Some("user".to_string()),
            parts,
        }
    }

    /// Create model content
    pub fn model(parts: Vec<Part>) -> Self {
        Self {
            role: Some("model".to_string()),
            parts,
        }
    }

    /// Create function response content
    pub fn function(parts: Vec<Part>) -> Self {
        Self {
            role: Some("function".to_string()),
            parts,
        }
    }
}

// ============================================================================
// LlmProvider Trait Implementation
// ============================================================================

use crate::{
    GenerationRequest, GenerationResponse, LlmProvider, SseEventParser,
    StreamChunk as UnifiedStreamChunk, TokenUsage, ToolCall as LlmToolCall,
};

fn select_stream_candidate_index(
    selected_candidate_index: Option<i32>,
    emitted_payload: bool,
    candidates: &[StreamCandidate],
) -> Option<i32> {
    if candidates.is_empty() {
        return selected_candidate_index;
    }

    let has_index_zero = candidates
        .iter()
        .any(|candidate| candidate.index == Some(0));
    match selected_candidate_index {
        Some(0) => Some(0),
        Some(_current) if has_index_zero && !emitted_payload => Some(0),
        Some(current) => Some(current),
        None if has_index_zero => Some(0),
        None => candidates.first().and_then(|candidate| candidate.index),
    }
}

fn should_consume_stream_candidate(
    selected_candidate_index: Option<i32>,
    candidate_position: usize,
    candidate_index: Option<i32>,
) -> bool {
    match selected_candidate_index {
        Some(index) => candidate_index == Some(index),
        None => candidate_position == 0,
    }
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
        || finish_reason.eq_ignore_ascii_case("OTHER")
}

fn normalize_stream_finish_reason(finish_reason: String) -> String {
    if is_blocking_finish_reason(&finish_reason) {
        format!("stream_error:{}", finish_reason.to_ascii_lowercase())
    } else {
        finish_reason
    }
}

fn build_gemini_thinking_config(
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Option<ThinkingConfig>> {
    let model = model.to_ascii_lowercase();
    if model.contains("gemini-3") {
        let Some(effort) = reasoning_effort else {
            return Ok(None);
        };
        let thinking_level = match effort {
            ReasoningEffort::None => {
                anyhow::bail!("Gemini 3 does not support disabling thinking with effort `none`")
            }
            ReasoningEffort::Minimal if model.contains("flash") => "minimal",
            ReasoningEffort::Minimal => {
                anyhow::bail!("Gemini 3 Pro supports reasoning efforts `low` and `high`")
            }
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium if model.contains("flash") => "medium",
            ReasoningEffort::Medium => {
                anyhow::bail!("Gemini 3 Pro supports reasoning efforts `low` and `high`")
            }
            ReasoningEffort::High => "high",
            ReasoningEffort::XHigh => {
                anyhow::bail!("Gemini 3 does not support reasoning effort `xhigh`")
            }
        };
        return Ok(Some(ThinkingConfig {
            thinking_level: Some(thinking_level.to_string()),
            thinking_budget: None,
        }));
    }

    if model.contains("gemini-2.5") {
        let budget = match reasoning_effort {
            Some(ReasoningEffort::None) if model.contains("pro") => {
                anyhow::bail!("Gemini 2.5 Pro does not support disabling thinking")
            }
            Some(ReasoningEffort::None) => Some(0),
            Some(effort) => Some(gemini_budget_for_effort(effort)),
            None => None,
        };
        return Ok(budget.map(|thinking_budget| ThinkingConfig {
            thinking_level: None,
            thinking_budget: Some(thinking_budget),
        }));
    }

    if reasoning_effort.is_some() {
        anyhow::bail!(
            "Gemini model `{}` does not declare reasoning effort support",
            model
        );
    }

    Ok(None)
}

fn gemini_budget_for_effort(effort: ReasoningEffort) -> i32 {
    match effort {
        ReasoningEffort::None => 0,
        ReasoningEffort::Minimal => 512,
        ReasoningEffort::Low => 1_024,
        ReasoningEffort::Medium => 4_096,
        ReasoningEffort::High => 8_192,
        ReasoningEffort::XHigh => 16_384,
    }
}

#[async_trait::async_trait]
impl LlmProvider for GoogleGeminiGenerateContentClient {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        let thinking_config = build_gemini_thinking_config(&self.model, request.reasoning.effort)?;
        let input = project_request_input(request.system_prompt.as_deref(), &request.messages)?;

        // Build tools
        let tools_payload = if request.tools.is_empty() {
            None
        } else {
            let declarations: Vec<FunctionDeclaration> = request
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
        };

        let gemini_request = GoogleGeminiGenerateContentRequest {
            contents: input.contents,
            system_instruction: input.system_instruction,
            tools: tools_payload,
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
                top_p: None,
                top_k: None,
                thinking_config,
            }),
        };

        let response = self.generate_content(gemini_request).await?;

        // Check if prompt was blocked
        if let Some(feedback) = response.prompt_feedback
            && let Some(block_reason) = feedback.block_reason
        {
            anyhow::bail!("Content blocked by safety filter: {}", block_reason);
        }

        // Get first candidate
        let candidate = match select_primary_candidate(&response.candidates) {
            Some(c) => c,
            None => {
                return Ok(GenerationResponse {
                    content: String::new(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                });
            }
        };

        // Check finish reason
        if let Some(finish_reason) = &candidate.finish_reason
            && is_blocking_finish_reason(finish_reason)
        {
            if finish_reason.eq_ignore_ascii_case("SAFETY") {
                anyhow::bail!("Response blocked by safety filter");
            }
            if finish_reason.eq_ignore_ascii_case("RECITATION") {
                anyhow::bail!("Response blocked due to recitation");
            }
            anyhow::bail!("Response blocked for unknown reason");
        }

        // Extract content
        let content = candidate
            .content
            .clone()
            .unwrap_or_else(|| Content::model(vec![]));

        let text = content
            .parts
            .iter()
            .filter_map(|p| p.text.clone())
            .collect::<Vec<_>>()
            .join("");

        let tool_calls: Vec<LlmToolCall> = content
            .parts
            .iter()
            .filter_map(|p| {
                p.function_call.as_ref().map(|fc| LlmToolCall {
                    id: None,
                    name: fc.name.clone(),
                    arguments: fc.args.clone(),
                })
            })
            .collect();

        let usage = response.usage_metadata.map(|u| TokenUsage {
            prompt_tokens: u.prompt_token_count.unwrap_or(0),
            cached_prompt_tokens: None,
            completion_tokens: u.candidates_token_count.unwrap_or(0),
            total_tokens: u.total_token_count.unwrap_or(0),
            reasoning_tokens: None,
        });

        Ok(GenerationResponse {
            content: text,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls,
            usage,
            finish_reason: candidate.finish_reason.clone(),
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        })
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> anyhow::Result<String> {
        if let Some(sys) = system {
            self.chat_with_system(sys, user).await
        } else {
            self.chat(user).await
        }
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<UnifiedStreamChunk>> {
        let thinking_config = build_gemini_thinking_config(&self.model, request.reasoning.effort)?;
        let input = project_request_input(request.system_prompt.as_deref(), &request.messages)?;

        // Build tools
        let tools_payload = if request.tools.is_empty() {
            None
        } else {
            let declarations: Vec<FunctionDeclaration> = request
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
        };

        let gemini_request = GoogleGeminiGenerateContentRequest {
            contents: input.contents,
            system_instruction: input.system_instruction,
            tools: tools_payload,
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
                top_p: None,
                top_k: None,
                thinking_config,
            }),
        };

        let (gemini_tx, mut gemini_rx) = tokio::sync::mpsc::channel::<StreamChunk>(128);
        let (tx, rx) = tokio::sync::mpsc::channel(256);

        // Convert Gemini chunks to unified chunks
        let convert_tx = tx;
        tokio::spawn(async move {
            let mut latest_usage: Option<TokenUsage> = None;
            let mut emitted_final = false;
            let mut emitted_payload = false;
            let mut selected_candidate_index: Option<i32> = None;
            let mut next_tool_call_index: usize = 0;

            while let Some(gemini_chunk) = gemini_rx.recv().await {
                if let Some(usage) = gemini_chunk.usage_metadata {
                    latest_usage = Some(TokenUsage {
                        prompt_tokens: usage.prompt_token_count.unwrap_or(0),
                        cached_prompt_tokens: None,
                        completion_tokens: usage.candidates_token_count.unwrap_or(0),
                        total_tokens: usage.total_token_count.unwrap_or(0),
                        reasoning_tokens: None,
                    });
                }

                if let Some(feedback) = gemini_chunk.prompt_feedback
                    && let Some(block_reason) = feedback.block_reason
                {
                    let _ = convert_tx
                        .send(UnifiedStreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: latest_usage,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: true,
                            finish_reason: Some(format!(
                                "stream_error:prompt_blocked:{}",
                                block_reason.to_ascii_lowercase()
                            )),
                            provider_response_id: None,
                            provider_response_status: None,
                        })
                        .await;
                    emitted_final = true;
                    break;
                }

                selected_candidate_index = select_stream_candidate_index(
                    selected_candidate_index,
                    emitted_payload,
                    &gemini_chunk.candidates,
                );

                for (candidate_position, candidate) in
                    gemini_chunk.candidates.into_iter().enumerate()
                {
                    if !should_consume_stream_candidate(
                        selected_candidate_index,
                        candidate_position,
                        candidate.index,
                    ) {
                        continue;
                    }
                    let finish_reason = candidate.finish_reason.clone();
                    if let Some(content) = candidate.content {
                        for part in content.parts {
                            if let Some(text) = part.text {
                                emitted_payload = true;
                                let stream_chunk = UnifiedStreamChunk {
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
                                };
                                if convert_tx.send(stream_chunk).await.is_err() {
                                    return;
                                }
                            }

                            if let Some(fc) = part.function_call {
                                let tool_call_index = next_tool_call_index;
                                next_tool_call_index = next_tool_call_index.saturating_add(1);
                                emitted_payload = true;
                                let stream_chunk = UnifiedStreamChunk {
                                    text: None,
                                    thinking: None,
                                    thinking_signature: None,
                                    redacted_thinking: None,
                                    usage: None,
                                    sequence_number: None,
                                    tool_call_delta: Some(crate::ToolCallDelta {
                                        index: tool_call_index,
                                        id: None,
                                        name: Some(fc.name),
                                        arguments_delta: Some(fc.args.to_string()),
                                        arguments: None,
                                    }),
                                    is_finished: false,
                                    finish_reason: None,
                                    provider_response_id: None,
                                    provider_response_status: None,
                                };
                                if convert_tx.send(stream_chunk).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }

                    if finish_reason.is_some() {
                        let final_chunk = UnifiedStreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: latest_usage,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: true,
                            finish_reason: finish_reason.map(normalize_stream_finish_reason),
                            provider_response_id: None,
                            provider_response_status: None,
                        };
                        emitted_final = true;
                        let _ = convert_tx.send(final_chunk).await;
                    }
                }
            }

            // If stream ended without finish event:
            // - emit final only when we already saw payload, so runtime can finalize streamed output.
            // - emit nothing on totally empty stream, so runtime can fallback to non-streaming generate().
            if !emitted_final && emitted_payload {
                let _ = convert_tx
                    .send(UnifiedStreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: latest_usage,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some("stream_closed".to_string()),
                        provider_response_id: None,
                        provider_response_status: None,
                    })
                    .await;
            }
        });

        let mut stream_client = GoogleGeminiGenerateContentClient::with_params(
            &self.project_id,
            &self.location,
            &self.model,
        );
        tokio::spawn(async move {
            if let Err(e) = stream_client
                .stream_generate_content(gemini_request, gemini_tx)
                .await
            {
                warn!(error = %e, "Gemini streaming failed");
            }
        });

        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "google_gemini_generate_content"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_client_with_params() {
        let client = GoogleGeminiGenerateContentClient::with_params(
            "test-project",
            "us-central1",
            "gemini-2.0-flash",
        );
        // Just verify it compiles and creates
        drop(client);
    }

    #[test]
    fn test_part_text() {
        let part = Part::text("Hello, world!");
        assert_eq!(part.text, Some("Hello, world!".to_string()));
        assert!(part.function_call.is_none());
        assert!(part.function_response.is_none());
    }

    #[test]
    fn test_part_function_call() {
        let args = serde_json::json!({"query": "test"});
        let part = Part::function_call("web_search", args.clone());
        assert!(part.text.is_none());
        assert!(part.function_call.is_some());
        let fc = part.function_call.unwrap();
        assert_eq!(fc.name, "web_search");
        assert_eq!(fc.args, args);
    }

    #[test]
    fn test_part_function_response() {
        let response = serde_json::json!({"result": "success"});
        let part = Part::function_response("my_tool", response.clone());
        assert!(part.text.is_none());
        assert!(part.function_response.is_some());
        let fr = part.function_response.unwrap();
        assert_eq!(fr.name, "my_tool");
        assert_eq!(fr.response, response);
    }

    #[test]
    fn test_content_user() {
        let parts = vec![Part::text("Hello")];
        let content = Content::user(parts);
        assert_eq!(content.role, Some("user".to_string()));
        assert_eq!(content.parts.len(), 1);
    }

    #[test]
    fn test_content_model() {
        let parts = vec![Part::text("Response")];
        let content = Content::model(parts);
        assert_eq!(content.role, Some("model".to_string()));
        assert_eq!(content.parts.len(), 1);
    }

    #[test]
    fn test_content_function() {
        let parts = vec![Part::function_response("tool", serde_json::json!({}))];
        let content = Content::function(parts);
        assert_eq!(content.role, Some("function".to_string()));
        assert_eq!(content.parts.len(), 1);
    }

    #[test]
    fn test_generation_config_default() {
        let config = GenerationConfig::default();
        assert!(config.temperature.is_none());
        assert!(config.max_output_tokens.is_none());
        assert!(config.top_p.is_none());
        assert!(config.top_k.is_none());
        assert!(config.thinking_config.is_none());
    }

    #[test]
    fn test_build_gemini_thinking_config_maps_gemini_3_effort_to_level() {
        let config =
            build_gemini_thinking_config("gemini-3-flash-preview", Some(ReasoningEffort::Medium))
                .unwrap()
                .unwrap();

        assert_eq!(config.thinking_level.as_deref(), Some("medium"));
        assert_eq!(config.thinking_budget, None);
    }

    #[test]
    fn test_build_gemini_thinking_config_maps_gemini_25_effort_to_budget() {
        let config = build_gemini_thinking_config("gemini-2.5-flash", Some(ReasoningEffort::High))
            .unwrap()
            .unwrap();

        assert_eq!(config.thinking_level, None);
        assert_eq!(config.thinking_budget, Some(8192));
    }

    #[test]
    fn test_build_gemini_thinking_config_omits_budget_when_effort_unset() {
        let config = build_gemini_thinking_config("gemini-2.5-flash", None).unwrap();

        assert!(config.is_none());
    }

    #[test]
    fn test_select_stream_candidate_index_prefers_zero_then_falls_back() {
        let non_zero_candidates = vec![
            StreamCandidate {
                content: None,
                finish_reason: None,
                index: Some(3),
            },
            StreamCandidate {
                content: None,
                finish_reason: None,
                index: Some(4),
            },
        ];
        assert_eq!(
            select_stream_candidate_index(None, false, &non_zero_candidates),
            Some(3)
        );

        let with_zero = vec![
            StreamCandidate {
                content: None,
                finish_reason: None,
                index: Some(2),
            },
            StreamCandidate {
                content: None,
                finish_reason: None,
                index: Some(0),
            },
        ];
        assert_eq!(
            select_stream_candidate_index(None, false, &with_zero),
            Some(0)
        );
        assert_eq!(
            select_stream_candidate_index(Some(3), false, &with_zero),
            Some(0)
        );
        assert_eq!(
            select_stream_candidate_index(Some(3), true, &with_zero),
            Some(3)
        );
    }

    #[test]
    fn test_should_consume_stream_candidate_uses_selected_index_or_position() {
        assert!(should_consume_stream_candidate(Some(0), 1, Some(0)));
        assert!(!should_consume_stream_candidate(Some(0), 0, Some(1)));
        assert!(should_consume_stream_candidate(None, 0, None));
        assert!(!should_consume_stream_candidate(None, 1, None));
    }

    #[test]
    fn test_normalize_stream_finish_reason_maps_blocking_reasons_to_stream_error() {
        assert_eq!(
            normalize_stream_finish_reason("SAFETY".to_string()),
            "stream_error:safety"
        );
        assert_eq!(
            normalize_stream_finish_reason("RECITATION".to_string()),
            "stream_error:recitation"
        );
        assert_eq!(normalize_stream_finish_reason("STOP".to_string()), "STOP");
    }

    #[test]
    fn test_select_primary_candidate_prefers_index_zero() {
        let candidates = vec![
            Candidate {
                content: Some(Content::model(vec![Part::text("secondary")])),
                finish_reason: Some("STOP".to_string()),
                index: Some(2),
                safety_ratings: vec![],
            },
            Candidate {
                content: Some(Content::model(vec![Part::text("primary")])),
                finish_reason: Some("STOP".to_string()),
                index: Some(0),
                safety_ratings: vec![],
            },
        ];

        let selected = select_primary_candidate(&candidates).expect("expected candidate");
        assert_eq!(selected.index, Some(0));
        assert_eq!(
            selected
                .content
                .as_ref()
                .and_then(|c| c.parts.first())
                .and_then(|p| p.text.as_deref()),
            Some("primary")
        );
    }

    #[test]
    fn test_generate_content_request_serialization() {
        let request = GoogleGeminiGenerateContentRequest {
            contents: vec![Content::user(vec![Part::text("Hello")])],
            system_instruction: None,
            tools: None,
            generation_config: Some(GenerationConfig {
                temperature: Some(0.7),
                max_output_tokens: Some(100),
                top_p: None,
                top_k: None,
                thinking_config: Some(ThinkingConfig {
                    thinking_level: Some("low".to_string()),
                    thinking_budget: None,
                }),
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("contents"));
        assert!(json.contains("generationConfig"));
        assert!(json.contains("thinkingConfig"));
        assert!(json.contains("thinkingLevel"));
        assert!(json.contains("0.7"));
    }

    #[test]
    fn test_generate_content_response_deserialization() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello!"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        }"#;

        let response: GoogleGeminiGenerateContentResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        assert!(response.usage_metadata.is_some());
        let usage = response.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, Some(10));
        assert_eq!(usage.total_token_count, Some(15));
    }

    #[test]
    fn test_safety_rating() {
        let json = r#"{
            "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT",
            "probability": "NEGLIGIBLE"
        }"#;

        let rating: SafetyRating = serde_json::from_str(json).unwrap();
        assert_eq!(rating.category, "HARM_CATEGORY_SEXUALLY_EXPLICIT");
        assert_eq!(rating.probability, "NEGLIGIBLE");
    }

    #[test]
    fn test_prompt_feedback_deserialization() {
        let json = r#"{
            "blockReason": "SAFETY",
            "safetyRatings": [
                {"category": "HARM_CATEGORY_HARASSMENT", "probability": "HIGH"}
            ]
        }"#;

        let feedback: PromptFeedback = serde_json::from_str(json).unwrap();
        assert_eq!(feedback.block_reason, Some("SAFETY".to_string()));
        assert_eq!(feedback.safety_ratings.len(), 1);
    }

    #[test]
    fn test_function_declaration() {
        let decl = FunctionDeclaration {
            name: "test_function".to_string(),
            description: "A test function".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "arg1": {"type": "string"}
                }
            }),
        };

        assert_eq!(decl.name, "test_function");
        assert_eq!(decl.description, "A test function");
    }

    #[test]
    fn test_tool_serialization() {
        let tool = Tool {
            function_declarations: vec![FunctionDeclaration {
                name: "func1".to_string(),
                description: "Function 1".to_string(),
                parameters: serde_json::json!({}),
            }],
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("functionDeclarations"));
    }

    #[test]
    fn test_stream_chunk_deserialization() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello"}]
                },
                "finishReason": null,
                "index": 0
            }],
            "usageMetadata": null
        }"#;

        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.candidates.len(), 1);
        assert!(chunk.usage_metadata.is_none());
    }
}
