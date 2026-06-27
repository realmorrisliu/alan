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
    Message, MessageRole, ProviderCapabilities, StreamChunk, TokenUsage, ToolCall, ToolDefinition,
};

pub use alan_llm::factory::{self, ProviderConfig, ProviderType};

// ============================================================================
// LlmProjection — provider-aware tape → LLM message projection
// ============================================================================

/// Provider-aware message projection from rich tape format to LLM wire format.
///
/// Different providers handle thinking/reasoning content differently:
/// - Anthropic Messages / OpenAI Responses / OpenAI Chat Completions: preserves thinking blocks
/// - Google Gemini GenerateContent: drops thinking (not supported in wire format)
pub trait LlmProjection: Send + Sync {
    fn project(&self, messages: &[crate::session::Message]) -> Vec<Message>;
}

/// Projection for providers that preserve thinking content.
struct PreserveThinkingProjection;

/// Projection for providers that drop thinking content.
struct DropThinkingProjection;

impl LlmProjection for PreserveThinkingProjection {
    fn project(&self, messages: &[crate::session::Message]) -> Vec<Message> {
        project_messages_impl(messages, true)
    }
}

impl LlmProjection for DropThinkingProjection {
    fn project(&self, messages: &[crate::session::Message]) -> Vec<Message> {
        project_messages_impl(messages, false)
    }
}

/// Select the appropriate projection for a provider type.
fn projection_for(provider_type: ProviderType) -> Box<dyn LlmProjection> {
    match provider_type {
        ProviderType::AnthropicMessages
        | ProviderType::ChatgptResponses
        | ProviderType::OpenAiResponses
        | ProviderType::OpenAiChatCompletions
        | ProviderType::OpenAiChatCompletionsCompatible
        | ProviderType::OpenRouter => Box::new(PreserveThinkingProjection),
        _ => Box::new(DropThinkingProjection),
    }
}

// ============================================================================
// LlmClient
// ============================================================================

/// Unified LLM client that wraps any provider implementing `LlmProvider`.
pub struct LlmClient {
    provider: Box<dyn LlmProvider>,
    provider_type: ProviderType,
    projection: Box<dyn LlmProjection>,
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
            "openai_chat_completions" => ProviderType::OpenAiChatCompletions,
            "openai_chat_completions_compatible" => ProviderType::OpenAiChatCompletionsCompatible,
            "openrouter" => ProviderType::OpenRouter,
            "anthropic_messages" => ProviderType::AnthropicMessages,
            _ => ProviderType::OpenAiChatCompletionsCompatible, // Default fallback
        };

        let projection = projection_for(provider_type);
        Self {
            provider: Box::new(provider),
            provider_type,
            projection,
        }
    }

    /// Create an LLM client from a provider configuration.
    pub fn from_config(config: ProviderConfig) -> Result<Self> {
        let provider_type = config.provider_type;
        let provider = factory::create_provider(config)?;
        let projection = projection_for(provider_type);
        Ok(Self {
            provider,
            provider_type,
            projection,
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
    pub fn project_messages(&self, messages: &[crate::session::Message]) -> Vec<Message> {
        self.projection.project(messages)
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

// ============================================================================
// Conversion Helpers
// ============================================================================

/// Convert session messages to LLM messages (preserves thinking).
///
/// This is the legacy free-function entry point. Prefer `LlmClient::project_messages()`
/// which automatically selects the right projection for the provider.
#[cfg(test)]
pub fn convert_session_messages(messages: &[crate::session::Message]) -> Vec<Message> {
    project_messages_impl(messages, true)
}

/// Core projection implementation.
///
/// `preserve_thinking`: if true, thinking content is forwarded to the LLM message;
/// if false, thinking is stripped (for providers that don't support it).
fn project_messages_impl(
    messages: &[crate::session::Message],
    preserve_thinking: bool,
) -> Vec<Message> {
    use crate::tape;

    messages
        .iter()
        .flat_map(|m| match m {
            tape::Message::Tool { responses } => responses
                .iter()
                .map(|r| {
                    let content = project_tool_response_for_prompt(&r.content);

                    let tool_call_id = {
                        let trimmed = r.id.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        }
                    };

                    Message {
                        role: MessageRole::Tool,
                        content,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        tool_calls: None,
                        tool_call_id,
                    }
                })
                .collect::<Vec<_>>(),
            _ => {
                let role = match m.role() {
                    tape::MessageRole::System => MessageRole::System,
                    tape::MessageRole::Context => MessageRole::Context,
                    tape::MessageRole::User => MessageRole::User,
                    tape::MessageRole::Assistant => MessageRole::Assistant,
                    tape::MessageRole::Tool => MessageRole::Tool,
                };

                let content = m.non_thinking_text_content();
                let thinking = if preserve_thinking {
                    m.thinking_content()
                } else {
                    None
                };
                let thinking_signature = if preserve_thinking {
                    m.thinking_signature()
                } else {
                    None
                };
                let redacted_thinking = if preserve_thinking {
                    let blocks = m.redacted_thinking_blocks();
                    if blocks.is_empty() {
                        None
                    } else {
                        Some(blocks)
                    }
                } else {
                    None
                };

                let tool_calls = if !m.tool_requests().is_empty() {
                    Some(
                        m.tool_requests()
                            .iter()
                            .map(|tc| ToolCall {
                                id: {
                                    let trimmed = tc.id.trim();
                                    if trimmed.is_empty() {
                                        None
                                    } else {
                                        Some(trimmed.to_string())
                                    }
                                },
                                name: tc.name.clone(),
                                arguments: tc.arguments.clone(),
                            })
                            .collect(),
                    )
                } else {
                    None
                };

                vec![Message {
                    role,
                    content,
                    thinking,
                    thinking_signature,
                    redacted_thinking,
                    tool_calls,
                    tool_call_id: None,
                }]
            }
        })
        .collect()
}

const MAX_PROJECTED_TOOL_PAYLOAD_SIZE: usize = 30_000;

pub(crate) fn project_tool_response_for_prompt(parts: &[crate::tape::ContentPart]) -> String {
    project_tool_response_content(parts, MAX_PROJECTED_TOOL_PAYLOAD_SIZE)
}

fn project_tool_response_content(parts: &[crate::tape::ContentPart], max_size: usize) -> String {
    let mut content = String::new();

    for (idx, part) in parts.iter().enumerate() {
        let remaining = max_size.saturating_sub(content.len());
        if remaining == 0 {
            break;
        }

        let remaining_parts = parts.len() - idx;
        let part_budget = remaining.div_ceil(remaining_parts);
        content.push_str(&project_tool_content_part(part, part_budget));
    }

    truncate_text_for_projection(&content, max_size)
}

fn project_tool_content_part(part: &crate::tape::ContentPart, max_size: usize) -> String {
    let raw = match part {
        crate::tape::ContentPart::Structured { data } => {
            let truncated = truncate_payload_for_projection(data.clone(), max_size);
            serde_json::to_string(&truncated).unwrap_or_else(|_| "{}".to_string())
        }
        _ => part.to_text_lossy(),
    };

    truncate_text_for_projection(&raw, max_size)
}

fn truncate_payload_for_projection(
    payload: serde_json::Value,
    max_size: usize,
) -> serde_json::Value {
    let payload_str = payload.to_string();
    if payload_str.len() <= max_size {
        return payload;
    }

    match payload {
        serde_json::Value::Object(map) => {
            let mut truncated = serde_json::Map::new();
            let mut current_size = 0;

            for (key, value) in map {
                let is_critical = matches!(key.as_str(), "success" | "error" | "url" | "title");
                if is_critical {
                    truncated.insert(key, value);
                    continue;
                }

                let processed_value = if key == "content" || key == "aggregated_content" {
                    if let serde_json::Value::String(s) = &value {
                        serde_json::Value::String(truncate_text_for_projection(s, max_size / 4))
                    } else {
                        value
                    }
                } else {
                    truncate_payload_for_projection(value, max_size / 2)
                };

                let value_str = processed_value.to_string();
                if current_size + value_str.len() < max_size * 3 / 4 {
                    truncated.insert(key, processed_value);
                    current_size += value_str.len();
                } else {
                    truncated.insert(
                        "_truncated".to_string(),
                        serde_json::Value::String("Additional fields omitted".to_string()),
                    );
                    break;
                }
            }

            serde_json::Value::Object(truncated)
        }
        serde_json::Value::Array(arr) => {
            let arr_len = arr.len();
            let mut truncated = Vec::new();
            let mut current_size = 0;

            for item in arr {
                let processed = truncate_payload_for_projection(item, max_size / arr_len.max(1));
                let item_str = processed.to_string();

                if current_size + item_str.len() < max_size * 3 / 4 {
                    truncated.push(processed);
                    current_size += item_str.len();
                } else {
                    truncated.push(serde_json::json!({
                        "_note": "Additional array items omitted"
                    }));
                    break;
                }
            }

            serde_json::Value::Array(truncated)
        }
        serde_json::Value::String(s) => {
            if s.len() > max_size / 10 {
                serde_json::Value::String(truncate_text_for_projection(&s, max_size / 10))
            } else {
                serde_json::Value::String(s)
            }
        }
        other => other,
    }
}

const PROJECTION_TRUNCATION_MARKER: &str = "...[truncated]";

fn truncate_text_for_projection(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    if max_len == 0 {
        return String::new();
    }

    if max_len <= PROJECTION_TRUNCATION_MARKER.len() {
        return utf8_prefix(PROJECTION_TRUNCATION_MARKER, max_len);
    }

    let prefix_len = max_len - PROJECTION_TRUNCATION_MARKER.len();
    let truncated = utf8_prefix(text, prefix_len);
    format!("{truncated}{PROJECTION_TRUNCATION_MARKER}")
}

fn utf8_prefix(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    let mut end = 0;
    for (idx, ch) in text.char_indices() {
        let next = idx + ch.len_utf8();
        if next > max_len {
            break;
        }
        end = next;
    }

    text[..end].to_string()
}

/// Build a generation request from session context.
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
        // Note: Mock provider is treated as OpenAI Chat Completions API-compatible by default
        // since it doesn't match known providers.
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

        let mut session = crate::session::Session::new();
        session.add_assistant_message("hi", Some("openrouter thinking"));
        let messages = session.tape.messages();
        let projected = client.project_messages(messages);
        assert_eq!(
            projected[0].thinking.as_deref(),
            Some("openrouter thinking")
        );
    }

    #[test]
    fn test_convert_session_messages() {
        use crate::session::Message as SessionMessage;

        let session_messages = vec![
            SessionMessage::user("Hello"),
            SessionMessage::assistant("Hi there"),
        ];

        let llm_messages = convert_session_messages(&session_messages);

        assert_eq!(llm_messages.len(), 2);
        assert_eq!(llm_messages[0].role, MessageRole::User);
        assert_eq!(llm_messages[1].role, MessageRole::Assistant);
        assert_eq!(llm_messages[0].content, "Hello");
    }

    #[test]
    fn test_convert_session_messages_ignores_blank_tool_ids() {
        use crate::session::Message as SessionMessage;
        use crate::tape::ToolRequest;

        let session_messages = vec![
            SessionMessage::assistant_with_tools(
                "",
                vec![ToolRequest {
                    id: "   ".to_string(),
                    name: "web_search".to_string(),
                    arguments: serde_json::json!({"query": "test"}),
                }],
            ),
            SessionMessage::tool_text("   ", "{}"),
        ];

        let llm_messages = convert_session_messages(&session_messages);
        assert_eq!(llm_messages.len(), 2);
        assert_eq!(llm_messages[0].tool_calls.as_ref().unwrap()[0].id, None);
        assert_eq!(llm_messages[1].tool_call_id, None);
    }

    #[test]
    fn test_convert_session_messages_uses_tool_payload_for_tool_content() {
        use crate::session::Message as SessionMessage;

        let payload = serde_json::json!({
            "success": true,
            "company": "y-warm.com"
        });
        let session_messages = vec![SessionMessage::tool_structured(
            "tool_call_123",
            payload.clone(),
        )];

        let llm_messages = convert_session_messages(&session_messages);
        assert_eq!(llm_messages.len(), 1);
        assert_eq!(llm_messages[0].role, MessageRole::Tool);
        assert_eq!(
            llm_messages[0].tool_call_id.as_deref(),
            Some("tool_call_123")
        );
        assert_eq!(llm_messages[0].content, payload.to_string());
    }

    #[test]
    fn test_convert_session_messages_tool_without_payload_uses_content() {
        use crate::session::Message as SessionMessage;

        let session_messages = vec![SessionMessage::tool_text("tool_call_123", "{\"ok\":true}")];

        let llm_messages = convert_session_messages(&session_messages);
        assert_eq!(llm_messages.len(), 1);
        assert_eq!(llm_messages[0].content, "{\"ok\":true}");
    }

    #[test]
    fn test_convert_session_messages_truncates_large_tool_payload_for_projection() {
        use crate::session::Message as SessionMessage;

        let large_content = "x".repeat(50_000);
        let payload = serde_json::json!({
            "success": true,
            "content": large_content
        });
        let session_messages = vec![SessionMessage::tool_structured("tool_call_123", payload)];

        let llm_messages = convert_session_messages(&session_messages);
        assert_eq!(llm_messages.len(), 1);
        assert!(llm_messages[0].content.len() <= 30_000);
        assert!(
            llm_messages[0]
                .content
                .contains(PROJECTION_TRUNCATION_MARKER)
        );
    }

    #[test]
    fn test_convert_session_messages_truncates_large_tool_text_for_projection() {
        use crate::session::Message as SessionMessage;

        let large_content = "x".repeat(50_000);
        let session_messages = vec![SessionMessage::tool_text("tool_call_123", large_content)];

        let llm_messages = convert_session_messages(&session_messages);
        assert_eq!(llm_messages.len(), 1);
        assert!(llm_messages[0].content.len() <= 30_000);
        assert!(
            llm_messages[0]
                .content
                .contains(PROJECTION_TRUNCATION_MARKER)
        );
    }

    #[test]
    fn test_convert_session_messages_preserves_single_part_tool_text_within_projection_budget() {
        use crate::session::Message as SessionMessage;

        let content = "x".repeat(20_000);
        let session_messages = vec![SessionMessage::tool_text("tool_call_123", content.clone())];

        let llm_messages = convert_session_messages(&session_messages);
        assert_eq!(llm_messages.len(), 1);
        assert_eq!(llm_messages[0].content, content);
    }

    #[test]
    fn test_convert_session_messages_caps_tool_text_projection_by_bytes() {
        use crate::session::Message as SessionMessage;

        let large_content = "你".repeat(20_000);
        let session_messages = vec![SessionMessage::tool_text("tool_call_123", large_content)];

        let llm_messages = convert_session_messages(&session_messages);
        assert_eq!(llm_messages.len(), 1);
        assert!(llm_messages[0].content.len() <= 30_000);
        assert!(
            llm_messages[0]
                .content
                .contains(PROJECTION_TRUNCATION_MARKER)
        );
    }

    #[test]
    fn test_truncate_text_for_projection_uses_byte_limit_and_preserves_utf8() {
        let text = "你好世界好";
        assert_eq!(truncate_text_for_projection(text, text.len()), text);
        let truncated = truncate_text_for_projection(text, 14);
        assert!(truncated.len() <= 14);
        assert_eq!(truncated, PROJECTION_TRUNCATION_MARKER);
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
    fn test_anthropic_projection_preserves_thinking() {
        use crate::session::Session;

        let mut session = Session::new();
        session.add_assistant_message("hello", Some("my reasoning"));

        let messages = session.tape.messages();
        let projection = PreserveThinkingProjection;
        let llm_messages = projection.project(messages);

        assert_eq!(llm_messages.len(), 1);
        assert_eq!(llm_messages[0].content, "hello");
        assert_eq!(llm_messages[0].thinking, Some("my reasoning".to_string()));
    }

    #[test]
    fn test_anthropic_projection_preserves_thinking_metadata() {
        use crate::session::Session;

        let mut session = Session::new();
        let redacted = vec!["ciphertext".to_string()];
        session.add_assistant_message_with_reasoning(
            "hello",
            Some("my reasoning"),
            Some("sig_123"),
            &redacted,
        );

        let messages = session.tape.messages();
        let projection = PreserveThinkingProjection;
        let llm_messages = projection.project(messages);

        assert_eq!(llm_messages.len(), 1);
        assert_eq!(llm_messages[0].thinking, Some("my reasoning".to_string()));
        assert_eq!(
            llm_messages[0].thinking_signature.as_deref(),
            Some("sig_123")
        );
        assert_eq!(
            llm_messages[0].redacted_thinking,
            Some(vec!["ciphertext".to_string()])
        );
    }

    #[test]
    fn test_drop_thinking_projection_strips_thinking() {
        use crate::session::Session;

        let mut session = Session::new();
        session.add_assistant_message("hello", Some("my reasoning"));

        let messages = session.tape.messages();
        let projection = DropThinkingProjection;
        let llm_messages = projection.project(messages);

        assert_eq!(llm_messages.len(), 1);
        assert_eq!(llm_messages[0].content, "hello");
        assert_eq!(llm_messages[0].thinking, None);
    }

    #[test]
    fn test_llm_client_selects_correct_projection() {
        use alan_llm::MockLlmProvider;

        // Mock defaults to OpenAI Chat Completions API-compatible fallback.
        let client = LlmClient::new(MockLlmProvider::new());
        assert!(client.is_openai_chat_completions_compatible());

        // The compatible chat-completions path preserves thinking metadata when available.
        let mut session = crate::session::Session::new();
        session.add_assistant_message("hi", Some("thinking..."));
        let messages = session.tape.messages();
        let projected = client.project_messages(messages);
        assert_eq!(projected[0].thinking.as_deref(), Some("thinking..."));
    }
}
