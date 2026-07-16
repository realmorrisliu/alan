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

pub mod anthropic_messages;
pub mod chatgpt_responses;
pub mod factory;
pub mod google_gemini_generate_content;
mod message;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
mod model;
pub mod openai_chat_completions;
pub mod openai_responses;
pub mod openrouter;
mod provider;
mod sse;

pub use anthropic_messages::AnthropicMessagesClient;
pub use chatgpt_responses::ChatgptResponsesClient;
pub use google_gemini_generate_content::GoogleGeminiGenerateContentClient;
pub use message::{Message, MessageContentPart, MessageRole};
#[cfg(any(test, feature = "mock"))]
pub use mock::MockLlmProvider;
pub use model::{
    CompatibilityTier, GenerationRequest, GenerationResponse, InstructionRole,
    ProviderCapabilities, StreamChunk, TokenUsage, ToolCall, ToolCallDelta, ToolDefinition,
};
pub use openai_chat_completions::OpenAiChatCompletionsClient;
pub use openai_responses::OpenAiResponsesClient;
pub use openrouter::OpenRouterClient;
pub use provider::LlmProvider;
pub(crate) use sse::SseEventParser;

#[cfg(test)]
mod tests;
