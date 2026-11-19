//! LLM provider adapters for alan.
//!
//! This crate provides a unified, trait-based interface for different LLM providers
//! (Google Gemini GenerateContent API, OpenAI Responses API, OpenAI Chat Completions API,
//! Anthropic Messages API, and OpenRouter's SDK-backed chat adapter)
//! with support for both sync and streaming generation.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │         LlmProvider (trait)             │
//! │  - generate()    - chat()               │
//! │  - generate_stream() - provider_name()  │
//! └─────────────┬───────────────────────────┘
//!               │ implements
//!     ┌─────────┼─────────┬─────────┐
//!     ▼         ▼         ▼         ▼
//! ┌──────────────┐ ┌──────────────┐ ┌──────────────────┐ ┌──────────────┐
//! │Google Gemini │ │OpenAI        │ │Anthropic         │ │OpenRouter    │
//! │GenerateContent│ │Responses/Chat│ │Messages          │ │(OpenAI Chat  │
//! │Client        │ │Clients       │ │Client            │ │Completions)  │
//! └──────────────┘ └──────────────┘ └──────────────────┘ └──────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use alan_llm::{LlmProvider, GenerationRequest};
//!
//! async fn example(provider: &mut dyn LlmProvider) {
//!     let request = GenerationRequest::new()
//!         .with_system_prompt("You are helpful")
//!         .with_user_message("Hello!");
//!     
//!     let response = provider.generate(request).await.unwrap();
//!     println!("{}", response.content);
//! }
//! ```

pub use alan_agent_protocol::{ReasoningControls, ReasoningEffort};

mod openrouter_url {
    pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
}

#[cfg(feature = "provider-anthropic")]
pub mod anthropic_messages;
#[cfg(feature = "provider-chatgpt")]
pub mod chatgpt_responses;
pub mod factory;
mod failure;
#[cfg(feature = "provider-gemini")]
pub mod google_gemini_generate_content;
mod message;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
mod model;
#[cfg(feature = "provider-openai")]
pub mod openai_chat_completions;
#[cfg(all(feature = "provider-chatgpt", not(feature = "provider-openai")))]
pub(crate) mod openai_chat_completions;
#[cfg(feature = "provider-openai")]
pub mod openai_responses;
#[cfg(feature = "provider-openrouter")]
pub mod openrouter;
#[cfg(not(feature = "provider-openrouter"))]
pub mod openrouter {
    pub use crate::openrouter_url::OPENROUTER_BASE_URL;
}
pub use failure::{safe_failure_reason, safe_finish_reason};
mod provider;
#[cfg(feature = "http")]
mod sse;

#[cfg(feature = "provider-anthropic")]
pub use anthropic_messages::AnthropicMessagesClient;
#[cfg(feature = "provider-chatgpt")]
pub use chatgpt_responses::ChatgptResponsesClient;
#[cfg(feature = "provider-gemini")]
pub use google_gemini_generate_content::GoogleGeminiGenerateContentClient;
pub use message::{Message, MessageContentPart, MessageRole};
#[cfg(any(test, feature = "mock"))]
pub use mock::MockLlmProvider;
pub use model::{
    CompatibilityTier, GenerationRequest, GenerationResponse, InstructionRole,
    ProviderCapabilities, StreamChunk, TokenUsage, ToolCall, ToolCallDelta, ToolDefinition,
};
#[cfg(feature = "provider-openai")]
pub use openai_chat_completions::OpenAiChatCompletionsClient;
#[cfg(feature = "provider-openai")]
pub use openai_responses::OpenAiResponsesClient;
#[cfg(feature = "provider-openrouter")]
pub use openrouter::OpenRouterClient;
pub use provider::{LlmProvider, ProviderModel};
#[cfg(feature = "http")]
pub(crate) use sse::SseEventParser;

#[cfg(test)]
mod safe_failure_tests;

#[cfg(test)]
mod tests;

mod evaluation;
pub use evaluation::{
    ChoiceEvaluationRequest, ChoiceEvaluationResponse, EvaluationCandidate, EvaluationSelection,
    MalformedEvaluationResponse,
};

#[cfg(feature = "provider-typesafe")]
mod typesafe_evaluation;
#[cfg(feature = "provider-typesafe")]
pub use typesafe_evaluation::TypesafeEvaluationClient;

#[cfg(not(feature = "provider-typesafe"))]
pub struct TypesafeEvaluationClient;

#[cfg(not(feature = "provider-typesafe"))]
impl TypesafeEvaluationClient {
    /// Validate a pinned model identifier before saving Connection metadata.
    pub fn validate_model(model: &str) -> anyhow::Result<()> {
        evaluation::validate_typesafe_model(model)
    }
}
