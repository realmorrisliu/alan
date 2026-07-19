//! Unified LLM client abstraction
//!
//! This module provides a unified, trait-based interface for different LLM providers.
//! The design uses the `LlmProvider` trait from `alan_llm` crate, allowing for
//! easy mocking in tests.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │     LlmClient (wrapper with convenience)    │
//! └─────────────┬───────────────────────────────┘
//!               │ holds
//!               ▼
//! ┌─────────────────────────────────────────────┐
//! │      Box<dyn LlmProvider> (trait object)    │
//! └─────────────┬───────────────────────────────┘
//!               │ implements
//!     ┌─────────┼─────────┬──────────────┐
//!     ▼         ▼         ▼              ▼
//! ┌───────┐ ┌───────┐ ┌──────────┐ ┌─────────┐
//! │Google Gemini │ │OpenAI      │ │Anthropic │ │  Mock   │
//! │GenerateContent│ │Clients     │ │Messages  │ │Provider │
//! └───────┘ └───────┘ └──────────┘ └─────────┘
//! ```

use anyhow::Result;
use std::path::PathBuf;

pub use alan_llm::{
    CompatibilityTier, GenerationRequest, GenerationResponse, InstructionRole, LlmProvider,
    Message, MessageContentPart, MessageRole, ProviderCapabilities, StreamChunk, TokenUsage,
    ToolCall, ToolDefinition,
};

pub use alan_llm::factory::{self, ProviderConfig, ProviderType};

mod input_projection;

pub(crate) use input_projection::project_messages;

fn provider_preserves_thinking(provider_type: ProviderType) -> bool {
    matches!(
        provider_type,
        ProviderType::AnthropicMessages
            | ProviderType::ChatgptResponses
            | ProviderType::OpenAiResponses
            | ProviderType::OpenAiChatCompletions
            | ProviderType::OpenAiChatCompletionsCompatible
            | ProviderType::OpenRouter
    )
}

// ============================================================================
// LlmClient
// ============================================================================

/// Unified LLM client that wraps any provider implementing `LlmProvider`.
pub struct LlmClient {
    provider: Box<dyn LlmProvider>,
    provider_type: ProviderType,
    preserve_thinking: bool,
}

impl LlmClient {
    /// Create a new LLM client from any provider implementing `LlmProvider`.
    pub fn new<P>(provider: P) -> Self
    where
        P: LlmProvider + 'static,
    {
        let provider_type = match provider.provider_name() {
            "google_gemini_generate_content" => ProviderType::GoogleGeminiGenerateContent,
            "chatgpt" => ProviderType::ChatgptResponses,
            "openai_responses" => ProviderType::OpenAiResponses,
            "mock" => ProviderType::OpenAiResponses,
            "openai_chat_completions" => ProviderType::OpenAiChatCompletions,
            "openai_chat_completions_compatible" => ProviderType::OpenAiChatCompletionsCompatible,
            "openrouter" => ProviderType::OpenRouter,
            "anthropic_messages" => ProviderType::AnthropicMessages,
            _ => ProviderType::OpenAiChatCompletionsCompatible, // Default fallback
        };

        Self {
            provider: Box::new(provider),
            provider_type,
            preserve_thinking: provider_preserves_thinking(provider_type),
        }
    }

    /// Create an LLM client from a provider configuration.
    pub fn from_config(config: ProviderConfig) -> Result<Self> {
        let provider_type = config.provider_type;
        let provider = factory::create_provider(config)?;
        Ok(Self {
            provider,
            provider_type,
            preserve_thinking: provider_preserves_thinking(provider_type),
        })
    }

    /// Create a client from core Config
    pub fn from_core_config(config: &crate::config::Config) -> Result<Self> {
        Self::from_core_config_with_chatgpt_auth_storage_path(config, None)
    }

    /// Create a client from core Config with an optional ChatGPT auth storage override.
    pub fn from_core_config_with_chatgpt_auth_storage_path(
        config: &crate::config::Config,
        chatgpt_auth_storage_path: Option<PathBuf>,
    ) -> Result<Self> {
        let mut provider_config = config.to_provider_config()?;
        if let Some(path) = chatgpt_auth_storage_path {
            provider_config = provider_config.with_chatgpt_auth_storage_path(path);
        }
        Self::from_config(provider_config)
    }

    /// Generate a response using the underlying provider.
    pub async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.provider.generate(request).await
    }

    /// Simple chat interface.
    pub async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.provider.chat(system, user).await
    }

    /// Simple text generation without system prompt.
    /// Used for semantic matching and other internal tasks.
    pub async fn generate_simple(&mut self, prompt: &str) -> Result<String> {
        self.provider.chat(None, prompt).await
    }

    /// Generate with streaming support.
    pub async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.provider.generate_stream(request).await
    }

    /// Get the provider name.
    pub fn provider_name(&self) -> &'static str {
        self.provider.provider_name()
    }

    /// Get the capability matrix for the active provider family.
    pub fn capabilities(&self) -> ProviderCapabilities {
        self.provider_type.capabilities()
    }

    /// Check if this is a Google Gemini GenerateContent client.
    pub fn is_google_gemini_generate_content(&self) -> bool {
        matches!(
            self.provider_type,
            ProviderType::GoogleGeminiGenerateContent
        )
    }

    /// Check if this is the ChatGPT/Codex managed-auth Responses client.
    pub fn is_chatgpt(&self) -> bool {
        matches!(self.provider_type, ProviderType::ChatgptResponses)
    }

    /// Check if this is an OpenAI Responses API client.
    pub fn is_openai_responses(&self) -> bool {
        matches!(self.provider_type, ProviderType::OpenAiResponses)
    }

    /// Check if this is an OpenAI Chat Completions API client.
    pub fn is_openai_chat_completions(&self) -> bool {
        matches!(self.provider_type, ProviderType::OpenAiChatCompletions)
    }

    /// Check if this is an OpenAI Chat Completions API-compatible client.
    pub fn is_openai_chat_completions_compatible(&self) -> bool {
        matches!(
            self.provider_type,
            ProviderType::OpenAiChatCompletionsCompatible
        )
    }

    /// Check if this is the OpenRouter SDK-backed provider.
    pub fn is_openrouter(&self) -> bool {
        matches!(self.provider_type, ProviderType::OpenRouter)
    }

    /// Check if this is an Anthropic Messages API client.
    pub fn is_anthropic_messages(&self) -> bool {
        matches!(self.provider_type, ProviderType::AnthropicMessages)
    }

    /// Project tape messages to LLM wire format using the provider-specific projection.
    pub fn project_messages(&self, messages: &[crate::agent_machine::Message]) -> Vec<Message> {
        project_messages(messages, self.preserve_thinking)
    }
}

impl std::fmt::Debug for LlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmClient")
            .field("provider_name", &self.provider_name())
            .field("provider_type", &self.provider_type)
            .finish()
    }
}

/// Build a generation request from machine context.
pub fn build_generation_request(
    system_prompt: Option<String>,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,
    temperature: Option<f32>,
    max_tokens: Option<i32>,
) -> GenerationRequest {
    GenerationRequest {
        system_prompt,
        messages,
        tools,
        temperature,
        max_tokens,
        reasoning: alan_llm::ReasoningControls::default(),
        extra_params: std::collections::HashMap::new(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use alan_llm::{
        AnthropicMessagesClient, ChatgptResponsesClient, MockLlmProvider,
        OpenAiChatCompletionsClient, OpenAiResponsesClient, OpenRouterClient,
    };
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_llm_client_with_mock() {
        let mock = MockLlmProvider::new().with_response(GenerationResponse {
            content: "Hello from mock".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: Some(TokenUsage {
                prompt_tokens: 10,
                cached_prompt_tokens: None,
                completion_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        });

        let mut client = LlmClient::new(mock);

        assert_eq!(client.provider_name(), "mock");
        // Mock provider uses the Responses projection/capability shape because
        // runtime smoke tests default to the OpenAI Responses configuration.
        assert!(client.is_openai_responses());
        assert!(!client.is_google_gemini_generate_content());
        assert!(!client.is_anthropic_messages());

        let request = GenerationRequest::new().with_user_message("Hi");

        let response = client.generate(request).await.unwrap();
        assert_eq!(response.content, "Hello from mock");
    }

    #[tokio::test]
    async fn test_llm_client_chat() {
        let mock = MockLlmProvider::new();
        let mut client = LlmClient::new(mock);

        let response = client.chat(Some("System"), "Hello").await.unwrap();
        assert!(response.contains("Mock response to:"));
    }

    #[tokio::test]
    async fn test_llm_client_stream() {
        let mock = MockLlmProvider::new().with_response(GenerationResponse {
            content: "Streamed content".to_string(),
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

        let mut client = LlmClient::new(mock);
        let mut rx = client
            .generate_stream(GenerationRequest::new())
            .await
            .unwrap();

        let first_chunk = rx.recv().await.unwrap();
        let final_chunk = rx.recv().await.unwrap();
        assert_eq!(first_chunk.text, Some("Streamed content".to_string()));
        assert!(!first_chunk.is_finished);
        assert!(final_chunk.is_finished);
        assert_eq!(final_chunk.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_llm_client_exposes_provider_capabilities() {
        let openai_responses = LlmClient::new(OpenAiResponsesClient::with_params(
            "sk-test",
            "https://api.openai.com/v1",
            "gpt-5.4",
        ));
        let openai_responses_caps = openai_responses.capabilities();
        assert!(openai_responses_caps.supports_server_managed_continuation);
        assert!(openai_responses_caps.supports_provider_compaction);
        assert_eq!(
            openai_responses_caps.instruction_role,
            InstructionRole::ResponsesInstructions
        );

        let auth_storage = tempfile::tempdir().unwrap();
        let chatgpt = LlmClient::new(
            ChatgptResponsesClient::with_params(
                "https://chatgpt.com/backend-api/codex",
                "gpt-5.3-codex",
                HashMap::new(),
                None,
                Some(auth_storage.path().join("auth.json")),
            )
            .unwrap(),
        );
        let chatgpt_caps = chatgpt.capabilities();
        assert!(!chatgpt_caps.supports_server_managed_continuation);
        assert!(!chatgpt_caps.supports_provider_compaction);
        assert!(!chatgpt_caps.supports_background_execution);

        let openai_chat = LlmClient::new(OpenAiChatCompletionsClient::official_with_params(
            "sk-test",
            "https://api.openai.com/v1",
            "gpt-5.4",
        ));
        let openai_chat_caps = openai_chat.capabilities();
        assert_eq!(
            openai_chat_caps.instruction_role,
            InstructionRole::Developer
        );
        assert!(openai_chat_caps.supports_multimodal_input);
        assert!(!openai_chat_caps.supports_server_managed_continuation);

        let anthropic = LlmClient::new(AnthropicMessagesClient::with_params(
            "sk-ant-test",
            "https://api.anthropic.com/v1",
            "claude-3-5-sonnet-latest",
        ));
        let anthropic_caps = anthropic.capabilities();
        assert_eq!(
            anthropic_caps.instruction_role,
            InstructionRole::AnthropicSystem
        );
        assert!(anthropic_caps.supports_document_input);
        assert!(anthropic_caps.supports_redacted_thinking);
    }

    #[test]
    fn test_llm_client_detects_openrouter_provider() {
        let client = LlmClient::new(
            OpenRouterClient::with_params(
                "sk-or-test",
                alan_llm::openrouter::OPENROUTER_BASE_URL,
                "moonshotai/kimi-k2.6",
            )
            .unwrap(),
        );

        assert!(client.is_openrouter());
        assert!(!client.is_openai_chat_completions_compatible());
        assert!(client.capabilities().supports_reasoning_text);

        let mut machine = crate::agent_machine::AgentMachine::new();
        machine.add_assistant_message("hi", Some("openrouter thinking"));
        let messages = machine.messages();
        let projected = client.project_messages(messages);
        assert_eq!(
            projected[0].thinking.as_deref(),
            Some("openrouter thinking")
        );
    }

    #[test]
    fn test_build_generation_request() {
        let messages = vec![Message::user("Hello"), Message::assistant("Hi")];

        let request = build_generation_request(
            Some("System".to_string()),
            messages,
            vec![],
            Some(0.7),
            Some(1000),
        );

        assert_eq!(request.system_prompt, Some("System".to_string()));
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(1000));
    }

    #[test]
    fn test_llm_client_selects_correct_projection() {
        use alan_llm::MockLlmProvider;

        // Mock defaults to OpenAI Responses for the runtime smoke path.
        let client = LlmClient::new(MockLlmProvider::new());
        assert!(client.is_openai_responses());

        // The Responses path preserves thinking metadata when available.
        let mut machine = crate::agent_machine::AgentMachine::new();
        machine.add_assistant_message("hi", Some("thinking..."));
        let messages = machine.messages();
        let projected = client.project_messages(messages);
        assert_eq!(projected[0].thinking.as_deref(), Some("thinking..."));
    }
}
