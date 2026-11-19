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

pub mod anthropic_messages;
pub mod chatgpt_responses;
pub mod factory;
mod failure;
pub mod google_gemini_generate_content;
pub mod magpie;
mod message;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
mod model;
pub mod openai_chat_completions;
pub mod openai_responses;
pub mod openrouter;
pub use failure::{safe_failure_reason, safe_finish_reason};
mod provider;
pub mod session_affinity;
mod sse;

pub use anthropic_messages::AnthropicMessagesClient;
pub use chatgpt_responses::ChatgptResponsesClient;
pub use google_gemini_generate_content::{
    GoogleGeminiGenerateContentClient, VertexAuth, VertexClientConfig,
};
pub use magpie::MagpieClient;
pub use message::{Message, MessageContentPart, MessageRole};
#[cfg(any(test, feature = "mock"))]
pub use mock::MockLlmProvider;
pub use model::{
    CompatibilityTier, GenerationRequest, GenerationResponse, InstructionRole,
    ProviderCapabilities, StreamChunk, TokenUsage, ToolCall, ToolCallDelta, ToolDefinition,
};
pub use openai_chat_completions::{OpenAiChatCompletionsClient, ReasoningEffortMap};
pub use openai_responses::{OpenAiResponsesClient, OpenAiResponsesTransport};
pub use openrouter::OpenRouterClient;
pub use provider::{LlmProvider, ProviderModel};
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
