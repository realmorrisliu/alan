use anyhow::Result;
use tokio::sync::mpsc;

use crate::{GenerationRequest, GenerationResponse, StreamChunk};

/// Account-authorized model metadata supplied by a provider adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModel {
    pub slug: String,
    pub context_window_tokens: u32,
    pub supported_reasoning_efforts: Vec<crate::ReasoningEffort>,
    pub default_reasoning_effort: Option<crate::ReasoningEffort>,
}

/// Unified trait for LLM providers.
///
/// This trait abstracts over different LLM backends and API surfaces.
/// providing a consistent interface for generation, streaming, and simple chat.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate a response with tool calling support
    ///
    /// # Arguments
    /// * `request` - The generation request containing messages, tools, and configuration
    ///
    /// # Returns
    /// * `Result<GenerationResponse>` - The generated response or an error
    async fn generate(&mut self, _request: GenerationRequest) -> Result<GenerationResponse> {
        anyhow::bail!("generation unavailable")
    }

    /// Simple chat without tool calling
    ///
    /// This is a convenience method for simple one-turn conversations.
    ///
    /// # Arguments
    /// * `system` - Optional system prompt
    /// * `user` - The user message
    ///
    /// # Returns
    /// * `Result<String>` - The assistant's response text
    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> Result<String> {
        anyhow::bail!("generation unavailable")
    }

    /// Generate with streaming support
    ///
    /// Returns a receiver channel that yields text chunks as they arrive.
    /// Each chunk can be a character, word, or sentence fragment.
    ///
    /// # Arguments
    /// * `request` - The generation request
    ///
    /// # Returns
    /// * `Result<mpsc::Receiver<StreamChunk>>` - Channel receiving stream chunks
    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        anyhow::bail!("generation unavailable")
    }

    /// Whether this callable implements generation (evaluation-only adapters do not).
    fn supports_generation(&self) -> bool {
        true
    }

    /// Whether this actual callable supports finite-choice evaluation.
    /// Generation or tool-calling support alone does not imply this capability.
    fn supports_choice_evaluation(&self) -> bool {
        false
    }

    /// Evaluate a finite set without producing assistant prose or dispatching Tools.
    async fn evaluate_choice(
        &mut self,
        _request: crate::ChoiceEvaluationRequest,
    ) -> Result<crate::ChoiceEvaluationResponse> {
        anyhow::bail!("finite-choice evaluation unavailable")
    }

    /// Non-secret account identity fixed when this callable was constructed.
    fn account_identity(&self) -> Option<&str> {
        None
    }

    /// Fetch account-scoped metadata when supported; never called by status observation.
    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        Ok(None)
    }

    /// Get the provider name (for logging/debugging)
    fn provider_name(&self) -> &'static str;
}
