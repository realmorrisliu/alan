use anyhow::Result;
use tokio::sync::mpsc;

use crate::{GenerationRequest, GenerationResponse, StreamChunk};

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
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse>;

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
    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String>;

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
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>>;

    /// Get the provider name (for logging/debugging)
    fn provider_name(&self) -> &'static str;
}
