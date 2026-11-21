//! LLM provider adapters for alan.
//!
//! This crate provides a unified, trait-based interface for different LLM providers
//! with support for both sync and streaming generation.
//!
//! The first-party built-in providers are exactly: Magpie (local/custom OpenAI
//! Chat Completions-compatible router), OpenAI (Responses API and ChatGPT/Codex
//! OAuth), Google Cloud Vertex AI (Gemini GenerateContent) and OpenRouter. The
//! Anthropic Messages and generic Chat Completions adapters remain available as
//! protocol adapters for Quartermaster (`q`) provider packages.
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

#[cfg(feature = "provider-anthropic-adapter")]
pub mod anthropic_messages;
#[cfg(feature = "provider-openai")]
pub mod chatgpt_responses;
pub mod factory;
mod failure;
pub mod gemini_catalog;
#[cfg(feature = "provider-vertex")]
pub mod google_gemini_generate_content;
#[cfg(feature = "provider-magpie")]
pub mod magpie;
/// Lightweight Magpie configuration defaults when the Magpie client is disabled.
#[cfg(not(feature = "provider-magpie"))]
pub mod magpie {
    pub use crate::provider_meta::{MAGPIE_DEFAULT_BASE_URL, parse_reasoning_effort_map};
}
mod message;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
mod model;
#[cfg(any(feature = "provider-openai", feature = "provider-magpie"))]
pub mod openai_chat_completions;
#[cfg(feature = "provider-openai")]
pub mod openai_responses;
#[cfg(feature = "provider-openrouter")]
pub mod openrouter;
/// Lightweight OpenRouter configuration defaults when the OpenRouter client is disabled.
#[cfg(not(feature = "provider-openrouter"))]
pub mod openrouter {
    pub use crate::provider_meta::OPENROUTER_BASE_URL;
}
pub use failure::{safe_failure_reason, safe_finish_reason};
mod provider;
mod provider_meta;
#[cfg(any(feature = "provider-magpie", feature = "provider-openai"))]
pub mod session_affinity;
#[cfg(any(
    feature = "provider-magpie",
    feature = "provider-openai",
    feature = "provider-vertex",
    feature = "provider-openrouter",
    feature = "provider-anthropic-adapter"
))]
mod sse;

#[cfg(feature = "provider-anthropic-adapter")]
pub use anthropic_messages::AnthropicMessagesClient;
#[cfg(feature = "provider-openai")]
pub use chatgpt_responses::ChatgptResponsesClient;
#[cfg(feature = "provider-vertex")]
pub use google_gemini_generate_content::{
    GoogleGeminiGenerateContentClient, VertexAuth, VertexClientConfig,
};
#[cfg(feature = "provider-magpie")]
pub use magpie::MagpieClient;
pub use message::{Message, MessageContentPart, MessageRole};
#[cfg(any(test, feature = "mock"))]
pub use mock::MockLlmProvider;
pub use model::{
    CompatibilityTier, GenerationRequest, GenerationResponse, InstructionRole,
    ProviderCapabilities, StreamChunk, TokenUsage, ToolCall, ToolCallDelta, ToolDefinition,
};
#[cfg(any(feature = "provider-openai", feature = "provider-magpie"))]
pub use openai_chat_completions::OpenAiChatCompletionsClient;
#[cfg(feature = "provider-openai")]
pub use openai_responses::{OpenAiResponsesClient, OpenAiResponsesTransport};
#[cfg(feature = "provider-openrouter")]
pub use openrouter::OpenRouterClient;
pub use provider::{LlmProvider, ProviderModel, SharedLlmProvider};
pub use provider_meta::ReasoningEffortMap;
#[cfg(any(
    feature = "provider-magpie",
    feature = "provider-openai",
    feature = "provider-vertex",
    feature = "provider-openrouter",
    feature = "provider-anthropic-adapter"
))]
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

mod typesafe_evaluation;
pub use typesafe_evaluation::TypesafeEvaluationClient;
