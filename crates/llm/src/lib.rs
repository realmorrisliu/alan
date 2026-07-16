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
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

pub mod anthropic_messages;
pub mod chatgpt_responses;
pub mod google_gemini_generate_content;
mod message;
pub mod openai_chat_completions;
pub mod openai_responses;
pub mod openrouter;
mod sse;
pub(crate) use sse::SseEventParser;

// Re-export clients for convenience
pub use anthropic_messages::AnthropicMessagesClient;
pub use chatgpt_responses::ChatgptResponsesClient;
pub use google_gemini_generate_content::GoogleGeminiGenerateContentClient;
pub use message::{Message, MessageContentPart, MessageRole};
pub use openai_chat_completions::OpenAiChatCompletionsClient;
pub use openai_responses::OpenAiResponsesClient;
pub use openrouter::OpenRouterClient;

// ============================================================================
// Core Types
// ============================================================================

/// Compatibility/support tier for a provider family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityTier {
    TierAFullFidelityStateful,
    TierBFullFidelityStateless,
    TierCBestEffortCompatible,
}

/// Where provider instructions should be projected on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionRole {
    ResponsesInstructions,
    Developer,
    System,
    AnthropicSystem,
}

/// Runtime-visible capability matrix for a provider family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub supports_streaming_text: bool,
    pub supports_streaming_tool_calls: bool,
    pub supports_provider_response_id: bool,
    pub supports_provider_response_status: bool,
    pub supports_reasoning_text: bool,
    pub supports_reasoning_signature: bool,
    pub supports_reasoning_effort_control: bool,
    pub supports_redacted_thinking: bool,
    pub supports_multimodal_input: bool,
    pub supports_document_input: bool,
    pub supports_cached_token_usage: bool,
    pub supports_server_managed_continuation: bool,
    pub supports_background_execution: bool,
    pub supports_retrieve_cancel: bool,
    pub supports_provider_compaction: bool,
    pub instruction_role: InstructionRole,
    pub compatibility_tier: CompatibilityTier,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call requested by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Token usage information
#[derive(Debug, Clone, Copy)]
pub struct TokenUsage {
    pub prompt_tokens: i32,
    pub cached_prompt_tokens: Option<i32>,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub reasoning_tokens: Option<i32>,
}

/// Unified request for generation
#[derive(Debug, Clone)]
pub struct GenerationRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    /// Canonical cross-provider reasoning controls.
    pub reasoning: ReasoningControls,
    /// Provider-specific extra parameters
    pub extra_params: HashMap<String, serde_json::Value>,
}

/// Response from generation
#[derive(Debug, Clone)]
pub struct GenerationResponse {
    pub content: String,
    pub thinking: Option<String>,
    pub thinking_signature: Option<String>,
    pub redacted_thinking: Vec<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
    /// Provider-native response identifier (for example Responses API `response.id`).
    pub provider_response_id: Option<String>,
    /// Provider-native terminal or in-flight status (for example Responses API `status`).
    pub provider_response_status: Option<String>,
    /// Provider/runtime warnings collected while assembling this response.
    pub warnings: Vec<String>,
}

/// A chunk of streaming response
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Text content (incremental)
    pub text: Option<String>,
    /// Thinking content (incremental)
    pub thinking: Option<String>,
    /// Thinking signature content (incremental or final depending on provider)
    pub thinking_signature: Option<String>,
    /// Redacted thinking block data
    pub redacted_thinking: Option<String>,
    /// Token usage (typically emitted near stream completion)
    pub usage: Option<TokenUsage>,
    /// Provider-native response identifier surfaced during streaming completion events.
    pub provider_response_id: Option<String>,
    /// Provider-native status surfaced during streaming completion events.
    pub provider_response_status: Option<String>,
    /// Provider-native stream cursor, for example Responses API `sequence_number`.
    pub sequence_number: Option<u64>,
    /// Tool call delta (for OpenAI-style streaming tool calls)
    pub tool_call_delta: Option<ToolCallDelta>,
    /// Whether this is the final chunk
    pub is_finished: bool,
    /// Finish reason if complete
    pub finish_reason: Option<String>,
}

/// Tool call delta for streaming
#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_delta: Option<String>,
    /// Complete tool-call arguments when the provider yields a finalized item.
    pub arguments: Option<String>,
}

// ============================================================================
// Builder Pattern
// ============================================================================

impl GenerationRequest {
    /// Create a new empty generation request
    pub fn new() -> Self {
        Self {
            system_prompt: None,
            messages: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            reasoning: ReasoningControls::default(),
            extra_params: HashMap::new(),
        }
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Add a user message
    pub fn with_user_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message {
            role: MessageRole::User,
            content: content.into(),
            content_parts: Vec::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });
        self
    }

    /// Add an assistant message
    pub fn with_assistant_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message {
            role: MessageRole::Assistant,
            content: content.into(),
            content_parts: Vec::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });
        self
    }

    /// Add a message with a specific role
    pub fn with_message(mut self, role: MessageRole, content: impl Into<String>) -> Self {
        self.messages.push(Message {
            role,
            content: content.into(),
            content_parts: Vec::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });
        self
    }

    /// Add a tool definition
    pub fn with_tool(mut self, tool: ToolDefinition) -> Self {
        self.tools.push(tool);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, tokens: i32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set canonical named reasoning effort.
    pub fn with_reasoning_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning.effort = Some(effort);
        self
    }

    /// Set canonical reasoning controls.
    pub fn with_reasoning_controls(mut self, controls: ReasoningControls) -> Self {
        self.reasoning = controls;
        self
    }

    /// Add extra provider-specific parameter
    pub fn with_extra_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra_params.insert(key.into(), value);
        self
    }

    /// Chain this request from a previous Responses API response.
    pub fn with_previous_response_id(mut self, response_id: impl Into<String>) -> Self {
        self.extra_params.insert(
            "previous_response_id".to_string(),
            serde_json::Value::String(response_id.into()),
        );
        self
    }

    /// Control whether the Responses API stores server-side state for this request.
    pub fn with_store(mut self, store: bool) -> Self {
        self.extra_params
            .insert("store".to_string(), serde_json::Value::Bool(store));
        self
    }

    /// Request asynchronous background execution on Responses-compatible providers.
    pub fn with_background(mut self, background: bool) -> Self {
        self.extra_params.insert(
            "background".to_string(),
            serde_json::Value::Bool(background),
        );
        self
    }

    /// Request additional fields in Responses API output items.
    pub fn with_include<I, S>(mut self, include: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_params.insert(
            "include".to_string(),
            serde_json::Value::Array(
                include
                    .into_iter()
                    .map(|value| serde_json::Value::String(value.into()))
                    .collect(),
            ),
        );
        self
    }

    /// Set the raw Responses API `context_management` object.
    pub fn with_context_management(mut self, context_management: serde_json::Value) -> Self {
        self.extra_params
            .insert("context_management".to_string(), context_management);
        self
    }

    /// Enable server-side compaction with a `compact_threshold`.
    pub fn with_context_management_compact_threshold(mut self, compact_threshold: u64) -> Self {
        self.extra_params.insert(
            "context_management".to_string(),
            serde_json::json!({ "compact_threshold": compact_threshold }),
        );
        self
    }
}

impl Default for GenerationRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: serde_json::json!({"type": "object"}),
        }
    }

    /// Set the parameters schema
    pub fn with_parameters(mut self, params: serde_json::Value) -> Self {
        self.parameters = params;
        self
    }

    /// Add a string parameter
    pub fn with_string_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let params = self.parameters.as_object_mut().unwrap();

        if !params.contains_key("properties") {
            params.insert("properties".to_string(), serde_json::json!({}));
        }

        if !params.contains_key("required") {
            params.insert("required".to_string(), serde_json::json!([]));
        }

        params["properties"][&name] = serde_json::json!({
            "type": "string",
            "description": description.into()
        });

        params["required"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!(name));

        self
    }
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            id: None,
            name: name.into(),
            arguments,
        }
    }

    /// Set the tool call ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

// ============================================================================
// LlmProvider Trait
// ============================================================================

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

/// Factory for creating LLM providers from configuration
pub mod factory {
    use super::*;
    use std::path::PathBuf;

    /// Configuration for creating an LLM provider
    #[derive(Debug, Clone)]
    pub struct ProviderConfig {
        pub provider_type: ProviderType,
        pub api_key: Option<String>,
        pub base_url: Option<String>,
        pub model: String,
        pub expected_account_id: Option<String>, // For ChatGPT managed auth
        pub chatgpt_auth_storage_path: Option<PathBuf>, // For ChatGPT managed auth
        pub project_id: Option<String>,          // For Google Gemini GenerateContent
        pub location: Option<String>,            // For Google Gemini GenerateContent
        pub custom_headers: Option<HashMap<String, String>>, // Custom HTTP headers
        pub client_name: Option<String>,         // Client name for usage tracking
        pub user_agent: Option<String>,          // User-Agent header
        pub http_referer: Option<String>,        // OpenRouter HTTP-Referer metadata
        pub x_title: Option<String>,             // OpenRouter X-Title metadata
        pub app_categories: Option<Vec<String>>, // OpenRouter app category metadata
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProviderType {
        GoogleGeminiGenerateContent,
        ChatgptResponses,
        OpenAiResponses,
        OpenAiChatCompletions,
        OpenAiChatCompletionsCompatible,
        OpenRouter,
        AnthropicMessages,
    }

    impl ProviderConfig {
        /// Create a new provider config for the Google Gemini GenerateContent API.
        pub fn google_gemini_generate_content(
            project_id: impl Into<String>,
            model: impl Into<String>,
        ) -> Self {
            Self {
                provider_type: ProviderType::GoogleGeminiGenerateContent,
                api_key: None,
                base_url: None,
                model: model.into(),
                expected_account_id: None,
                chatgpt_auth_storage_path: None,
                project_id: Some(project_id.into()),
                location: Some("us-central1".to_string()),
                custom_headers: None,
                client_name: None,
                user_agent: None,
                http_referer: None,
                x_title: None,
                app_categories: None,
            }
        }

        /// Create a new provider config for the OpenAI Responses API.
        pub fn openai_responses(api_key: impl Into<String>, model: impl Into<String>) -> Self {
            Self {
                provider_type: ProviderType::OpenAiResponses,
                api_key: Some(api_key.into()),
                base_url: Some("https://api.openai.com/v1".to_string()),
                model: model.into(),
                expected_account_id: None,
                chatgpt_auth_storage_path: None,
                project_id: None,
                location: None,
                custom_headers: None,
                client_name: None,
                user_agent: None,
                http_referer: None,
                x_title: None,
                app_categories: None,
            }
        }

        /// Create a new provider config for the ChatGPT/Codex managed-auth Responses surface.
        pub fn chatgpt(model: impl Into<String>) -> Self {
            Self {
                provider_type: ProviderType::ChatgptResponses,
                api_key: None,
                base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
                model: model.into(),
                expected_account_id: None,
                chatgpt_auth_storage_path: None,
                project_id: None,
                location: None,
                custom_headers: None,
                client_name: None,
                user_agent: None,
                http_referer: None,
                x_title: None,
                app_categories: None,
            }
        }

        /// Create a new provider config for the OpenAI Chat Completions API.
        pub fn openai_chat_completions(
            api_key: impl Into<String>,
            model: impl Into<String>,
        ) -> Self {
            Self {
                provider_type: ProviderType::OpenAiChatCompletions,
                api_key: Some(api_key.into()),
                base_url: Some("https://api.openai.com/v1".to_string()),
                model: model.into(),
                expected_account_id: None,
                chatgpt_auth_storage_path: None,
                project_id: None,
                location: None,
                custom_headers: None,
                client_name: None,
                user_agent: None,
                http_referer: None,
                x_title: None,
                app_categories: None,
            }
        }

        /// Create a new provider config for an OpenAI Chat Completions API-compatible endpoint.
        pub fn openai_chat_completions_compatible(
            api_key: impl Into<String>,
            model: impl Into<String>,
        ) -> Self {
            Self {
                provider_type: ProviderType::OpenAiChatCompletionsCompatible,
                api_key: Some(api_key.into()),
                base_url: Some("https://api.openai.com/v1".to_string()),
                model: model.into(),
                expected_account_id: None,
                chatgpt_auth_storage_path: None,
                project_id: None,
                location: None,
                custom_headers: None,
                client_name: None,
                user_agent: None,
                http_referer: None,
                x_title: None,
                app_categories: None,
            }
        }

        /// Create a new provider config for the Anthropic Messages API.
        pub fn anthropic_messages(api_key: impl Into<String>, model: impl Into<String>) -> Self {
            Self {
                provider_type: ProviderType::AnthropicMessages,
                api_key: Some(api_key.into()),
                base_url: Some("https://api.anthropic.com".to_string()),
                model: model.into(),
                expected_account_id: None,
                chatgpt_auth_storage_path: None,
                project_id: None,
                location: None,
                custom_headers: None,
                client_name: None,
                user_agent: None,
                http_referer: None,
                x_title: None,
                app_categories: None,
            }
        }

        /// Create a new provider config for OpenRouter's SDK-backed chat adapter.
        pub fn openrouter(api_key: impl Into<String>, model: impl Into<String>) -> Self {
            Self {
                provider_type: ProviderType::OpenRouter,
                api_key: Some(api_key.into()),
                base_url: Some(openrouter::OPENROUTER_BASE_URL.to_string()),
                model: model.into(),
                expected_account_id: None,
                chatgpt_auth_storage_path: None,
                project_id: None,
                location: None,
                custom_headers: None,
                client_name: None,
                user_agent: None,
                http_referer: None,
                x_title: None,
                app_categories: None,
            }
        }

        /// Set custom base URL
        pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
            self.base_url = Some(url.into());
            self
        }

        /// Set expected ChatGPT account/workspace binding.
        pub fn with_chatgpt_account_id(mut self, account_id: impl Into<String>) -> Self {
            self.expected_account_id = Some(account_id.into());
            self
        }

        /// Set the managed ChatGPT auth storage path.
        pub fn with_chatgpt_auth_storage_path(mut self, path: impl Into<PathBuf>) -> Self {
            self.chatgpt_auth_storage_path = Some(path.into());
            self
        }

        /// Set location (for Gemini)
        pub fn with_location(mut self, location: impl Into<String>) -> Self {
            self.location = Some(location.into());
            self
        }

        /// Set custom HTTP headers
        pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
            self.custom_headers = Some(headers);
            self
        }

        /// Set client name for usage tracking
        pub fn with_client_name(mut self, name: impl Into<String>) -> Self {
            self.client_name = Some(name.into());
            self
        }

        /// Set User-Agent header
        pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
            self.user_agent = Some(user_agent.into());
            self
        }

        /// Set OpenRouter HTTP-Referer client metadata.
        pub fn with_http_referer(mut self, http_referer: impl Into<String>) -> Self {
            self.http_referer = Some(http_referer.into());
            self
        }

        /// Set OpenRouter X-Title client metadata.
        pub fn with_x_title(mut self, x_title: impl Into<String>) -> Self {
            self.x_title = Some(x_title.into());
            self
        }

        /// Set OpenRouter app category metadata.
        pub fn with_app_categories<I, S>(mut self, app_categories: I) -> Self
        where
            I: IntoIterator<Item = S>,
            S: Into<String>,
        {
            self.app_categories = Some(app_categories.into_iter().map(Into::into).collect());
            self
        }
    }

    /// Create an LLM provider from configuration
    pub fn create_provider(config: ProviderConfig) -> Result<Box<dyn LlmProvider>> {
        match config.provider_type {
            ProviderType::GoogleGeminiGenerateContent => {
                let project_id = config.project_id.ok_or_else(|| {
                    anyhow::anyhow!("Google Gemini GenerateContent provider requires project_id")
                })?;
                let location = config.location.unwrap_or_else(|| "us-central1".to_string());

                Ok(Box::new(GoogleGeminiGenerateContentClient::with_params(
                    &project_id,
                    &location,
                    &config.model,
                )))
            }
            ProviderType::ChatgptResponses => {
                let base_url = config
                    .base_url
                    .unwrap_or_else(|| "https://chatgpt.com/backend-api/codex".to_string());
                let client = ChatgptResponsesClient::with_params(
                    &base_url,
                    &config.model,
                    config.custom_headers.unwrap_or_default(),
                    config.expected_account_id,
                    config.chatgpt_auth_storage_path,
                )?;
                Ok(Box::new(client))
            }
            ProviderType::OpenAiResponses => {
                let api_key = config
                    .api_key
                    .ok_or_else(|| anyhow::anyhow!("OpenAI Responses provider requires api_key"))?;
                let base_url = config
                    .base_url
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

                Ok(Box::new(OpenAiResponsesClient::with_params(
                    &api_key,
                    &base_url,
                    &config.model,
                )))
            }
            ProviderType::OpenAiChatCompletions => {
                let api_key = config.api_key.ok_or_else(|| {
                    anyhow::anyhow!("OpenAI Chat Completions provider requires api_key")
                })?;
                let base_url = config
                    .base_url
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

                Ok(Box::new(OpenAiChatCompletionsClient::official_with_params(
                    &api_key,
                    &base_url,
                    &config.model,
                )))
            }
            ProviderType::OpenAiChatCompletionsCompatible => {
                let api_key = config.api_key.ok_or_else(|| {
                    anyhow::anyhow!(
                        "OpenAI Chat Completions API-compatible provider requires api_key"
                    )
                })?;
                let base_url = config
                    .base_url
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

                Ok(Box::new(
                    OpenAiChatCompletionsClient::compatible_with_params(
                        &api_key,
                        &base_url,
                        &config.model,
                    ),
                ))
            }
            ProviderType::OpenRouter => {
                let api_key = config
                    .api_key
                    .ok_or_else(|| anyhow::anyhow!("OpenRouter provider requires api_key"))?;
                let base_url = config
                    .base_url
                    .unwrap_or_else(|| openrouter::OPENROUTER_BASE_URL.to_string());
                let metadata = openrouter::OpenRouterClientMetadata {
                    http_referer: config.http_referer,
                    x_title: config.x_title,
                    app_categories: config.app_categories,
                };
                Ok(Box::new(OpenRouterClient::with_metadata(
                    &api_key,
                    &base_url,
                    &config.model,
                    metadata,
                )?))
            }
            ProviderType::AnthropicMessages => {
                let api_key = config.api_key.ok_or_else(|| {
                    anyhow::anyhow!("Anthropic Messages API provider requires api_key")
                })?;
                let base_url = config
                    .base_url
                    .unwrap_or_else(|| "https://api.anthropic.com".to_string());

                let mut client =
                    AnthropicMessagesClient::with_params(&api_key, &base_url, &config.model);

                // Apply custom headers if provided
                if let Some(headers) = config.custom_headers {
                    client = client.with_headers(headers);
                }
                if let Some(client_name) = config.client_name {
                    client = client.with_client_name(&client_name);
                }
                if let Some(user_agent) = config.user_agent {
                    client = client.with_user_agent(&user_agent);
                }

                Ok(Box::new(client))
            }
        }
    }
}

impl factory::ProviderType {
    pub fn capabilities(self) -> ProviderCapabilities {
        match self {
            factory::ProviderType::GoogleGeminiGenerateContent => ProviderCapabilities {
                supports_streaming_text: true,
                supports_streaming_tool_calls: false,
                supports_provider_response_id: false,
                supports_provider_response_status: false,
                supports_reasoning_text: false,
                supports_reasoning_signature: false,
                supports_reasoning_effort_control: true,
                supports_redacted_thinking: false,
                supports_multimodal_input: false,
                supports_document_input: false,
                supports_cached_token_usage: false,
                supports_server_managed_continuation: false,
                supports_background_execution: false,
                supports_retrieve_cancel: false,
                supports_provider_compaction: false,
                instruction_role: InstructionRole::System,
                compatibility_tier: CompatibilityTier::TierCBestEffortCompatible,
            },
            factory::ProviderType::ChatgptResponses => ProviderCapabilities {
                supports_streaming_text: true,
                supports_streaming_tool_calls: true,
                supports_provider_response_id: true,
                supports_provider_response_status: true,
                supports_reasoning_text: true,
                supports_reasoning_signature: true,
                supports_reasoning_effort_control: true,
                supports_redacted_thinking: false,
                supports_multimodal_input: false,
                supports_document_input: false,
                supports_cached_token_usage: true,
                supports_server_managed_continuation: false,
                supports_background_execution: false,
                supports_retrieve_cancel: false,
                supports_provider_compaction: false,
                instruction_role: InstructionRole::ResponsesInstructions,
                compatibility_tier: CompatibilityTier::TierCBestEffortCompatible,
            },
            factory::ProviderType::OpenAiResponses => ProviderCapabilities {
                supports_streaming_text: true,
                supports_streaming_tool_calls: true,
                supports_provider_response_id: true,
                supports_provider_response_status: true,
                supports_reasoning_text: true,
                supports_reasoning_signature: true,
                supports_reasoning_effort_control: true,
                supports_redacted_thinking: false,
                supports_multimodal_input: true,
                supports_document_input: true,
                supports_cached_token_usage: true,
                supports_server_managed_continuation: true,
                supports_background_execution: true,
                supports_retrieve_cancel: true,
                supports_provider_compaction: true,
                instruction_role: InstructionRole::ResponsesInstructions,
                compatibility_tier: CompatibilityTier::TierAFullFidelityStateful,
            },
            factory::ProviderType::OpenAiChatCompletions => ProviderCapabilities {
                supports_streaming_text: true,
                supports_streaming_tool_calls: true,
                supports_provider_response_id: true,
                supports_provider_response_status: false,
                supports_reasoning_text: true,
                supports_reasoning_signature: false,
                supports_reasoning_effort_control: true,
                supports_redacted_thinking: false,
                supports_multimodal_input: true,
                supports_document_input: true,
                supports_cached_token_usage: true,
                supports_server_managed_continuation: false,
                supports_background_execution: false,
                supports_retrieve_cancel: false,
                supports_provider_compaction: false,
                instruction_role: InstructionRole::Developer,
                compatibility_tier: CompatibilityTier::TierBFullFidelityStateless,
            },
            factory::ProviderType::OpenAiChatCompletionsCompatible => ProviderCapabilities {
                supports_streaming_text: true,
                supports_streaming_tool_calls: true,
                supports_provider_response_id: false,
                supports_provider_response_status: false,
                supports_reasoning_text: false,
                supports_reasoning_signature: false,
                supports_reasoning_effort_control: true,
                supports_redacted_thinking: false,
                supports_multimodal_input: false,
                supports_document_input: false,
                supports_cached_token_usage: false,
                supports_server_managed_continuation: false,
                supports_background_execution: false,
                supports_retrieve_cancel: false,
                supports_provider_compaction: false,
                instruction_role: InstructionRole::System,
                compatibility_tier: CompatibilityTier::TierCBestEffortCompatible,
            },
            factory::ProviderType::OpenRouter => ProviderCapabilities {
                supports_streaming_text: true,
                supports_streaming_tool_calls: true,
                supports_provider_response_id: true,
                supports_provider_response_status: false,
                supports_reasoning_text: true,
                supports_reasoning_signature: true,
                supports_reasoning_effort_control: true,
                supports_redacted_thinking: false,
                supports_multimodal_input: false,
                supports_document_input: false,
                supports_cached_token_usage: false,
                supports_server_managed_continuation: false,
                supports_background_execution: false,
                supports_retrieve_cancel: false,
                supports_provider_compaction: false,
                instruction_role: InstructionRole::System,
                compatibility_tier: CompatibilityTier::TierCBestEffortCompatible,
            },
            factory::ProviderType::AnthropicMessages => ProviderCapabilities {
                supports_streaming_text: true,
                supports_streaming_tool_calls: true,
                supports_provider_response_id: true,
                supports_provider_response_status: false,
                supports_reasoning_text: true,
                supports_reasoning_signature: true,
                supports_reasoning_effort_control: true,
                supports_redacted_thinking: true,
                supports_multimodal_input: true,
                supports_document_input: true,
                supports_cached_token_usage: true,
                supports_server_managed_continuation: false,
                supports_background_execution: false,
                supports_retrieve_cancel: false,
                supports_provider_compaction: false,
                instruction_role: InstructionRole::AnthropicSystem,
                compatibility_tier: CompatibilityTier::TierBFullFidelityStateless,
            },
        }
    }
}

// ============================================================================
// Mock Provider for Testing
// ============================================================================

#[cfg(any(test, feature = "mock"))]
pub mod mock {
    use super::*;
    use std::sync::Arc;

    /// A mock LLM provider for testing
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use alan_llm::mock::MockLlmProvider;
    ///
    /// let mut mock = MockLlmProvider::new()
    ///     .with_response(GenerationResponse::new("Hello!"));
    ///
    /// let response = mock.generate(GenerationRequest::new()).await.unwrap();
    /// assert_eq!(response.content, "Hello!");
    /// ```
    #[derive(Debug, Clone)]
    pub struct MockLlmProvider {
        responses: Arc<std::sync::Mutex<Vec<GenerationResponse>>>,
        recorded_requests: Arc<std::sync::Mutex<Vec<GenerationRequest>>>,
        default_response: GenerationResponse,
    }

    impl MockLlmProvider {
        /// Create a new mock provider with a default response
        pub fn new() -> Self {
            Self {
                responses: Arc::new(std::sync::Mutex::new(Vec::new())),
                recorded_requests: Arc::new(std::sync::Mutex::new(Vec::new())),
                default_response: GenerationResponse {
                    content: "Mock response".to_string(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: Vec::new(),
                    usage: Some(TokenUsage {
                        prompt_tokens: 10,
                        cached_prompt_tokens: None,
                        completion_tokens: 5,
                        total_tokens: 15,
                        reasoning_tokens: None,
                    }),
                    finish_reason: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    warnings: Vec::new(),
                },
            }
        }

        /// Add a pre-programmed response
        pub fn with_response(mut self, response: GenerationResponse) -> Self {
            self.default_response = response;
            self
        }

        /// Add multiple responses (will be returned in order)
        pub fn with_responses(self, responses: Vec<GenerationResponse>) -> Self {
            if let Ok(mut guard) = self.responses.lock() {
                *guard = responses;
            }
            self
        }

        /// Get recorded requests for verification
        pub fn recorded_requests(&self) -> Vec<GenerationRequest> {
            self.recorded_requests
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }

        /// Clear recorded requests
        pub fn clear_recorded(&self) {
            self.recorded_requests
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clear();
        }
    }

    impl Default for MockLlmProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
            self.recorded_requests
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(request);

            let mut responses = self.responses.lock().unwrap_or_else(|e| e.into_inner());
            if responses.is_empty() {
                Ok(self.default_response.clone())
            } else {
                Ok(responses.remove(0))
            }
        }

        async fn chat(&mut self, _system: Option<&str>, user: &str) -> Result<String> {
            Ok(format!("Mock response to: {}", user))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> Result<mpsc::Receiver<StreamChunk>> {
            self.recorded_requests
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(request);

            let mut responses = self.responses.lock().unwrap_or_else(|e| e.into_inner());
            let response = if responses.is_empty() {
                self.default_response.clone()
            } else {
                responses.remove(0)
            };

            let (tx, rx) = mpsc::channel(10);

            let content = response.content.clone();
            let tool_calls = response.tool_calls.clone();
            let usage = response.usage;
            let provider_response_id = response.provider_response_id.clone();
            let provider_response_status = response.provider_response_status;
            tokio::spawn(async move {
                if !content.is_empty() {
                    let _ = tx
                        .send(StreamChunk {
                            text: Some(content),
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: false,
                            finish_reason: None,
                        })
                        .await;
                }

                for (index, tool_call) in tool_calls.iter().enumerate() {
                    let arguments =
                        serde_json::to_string(&tool_call.arguments).unwrap_or_else(|_| "{}".into());
                    let _ = tx
                        .send(StreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: Some(ToolCallDelta {
                                index,
                                id: tool_call.id.clone(),
                                name: Some(tool_call.name.clone()),
                                arguments_delta: Some(arguments.clone()),
                                arguments: Some(arguments),
                            }),
                            is_finished: false,
                            finish_reason: None,
                        })
                        .await;
                }

                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage,
                        provider_response_id,
                        provider_response_status,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some(if tool_calls.is_empty() {
                            "stop".to_string()
                        } else {
                            "tool_calls".to_string()
                        }),
                    })
                    .await;
            });

            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "mock"
        }
    }
}

// Re-export mock when running tests
#[cfg(any(test, feature = "mock"))]
pub use mock::MockLlmProvider;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_request_builder() {
        let request = GenerationRequest::new()
            .with_system_prompt("You are helpful")
            .with_user_message("Hello")
            .with_temperature(0.7)
            .with_max_tokens(100);

        assert_eq!(request.system_prompt, Some("You are helpful".to_string()));
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].content, "Hello");
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(100));
    }

    #[test]
    fn test_generation_request_builder_responses_helpers() {
        let request = GenerationRequest::new()
            .with_previous_response_id("resp_prev")
            .with_store(true)
            .with_background(true)
            .with_include(["reasoning.encrypted_content"])
            .with_context_management_compact_threshold(8192);

        assert_eq!(
            request.extra_params.get("previous_response_id"),
            Some(&serde_json::json!("resp_prev"))
        );
        assert_eq!(
            request.extra_params.get("store"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            request.extra_params.get("background"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            request.extra_params.get("include"),
            Some(&serde_json::json!(["reasoning.encrypted_content"]))
        );
        assert_eq!(
            request.extra_params.get("context_management"),
            Some(&serde_json::json!({ "compact_threshold": 8192 }))
        );
    }

    #[test]
    fn test_generation_request_reasoning_controls_set_canonical_controls() {
        let cleared =
            GenerationRequest::new().with_reasoning_controls(ReasoningControls::default());

        assert_eq!(cleared.reasoning, ReasoningControls::default());

        let efforted = GenerationRequest::new().with_reasoning_controls(ReasoningControls {
            effort: Some(ReasoningEffort::High),
        });

        assert_eq!(efforted.reasoning.effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn test_message_helpers() {
        let sys = Message::system("System prompt");
        assert_eq!(sys.role, MessageRole::System);
        assert_eq!(sys.content, "System prompt");

        let user = Message::user("User message");
        assert_eq!(user.role, MessageRole::User);

        let assistant = Message::assistant("Assistant reply");
        assert_eq!(assistant.role, MessageRole::Assistant);

        let tool = Message::tool("call-123", "Tool result");
        assert_eq!(tool.role, MessageRole::Tool);
        assert_eq!(tool.tool_call_id, Some("call-123".to_string()));
    }

    #[test]
    fn test_tool_definition_builder() {
        let tool = ToolDefinition::new("search", "Search the web")
            .with_string_param("query", "The search query");

        assert_eq!(tool.name, "search");
        assert_eq!(tool.description, "Search the web");
        assert!(tool.parameters["properties"].get("query").is_some());
        assert!(
            tool.parameters["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("query"))
        );
    }

    #[test]
    fn test_tool_call_builder() {
        let call =
            ToolCall::new("my_tool", serde_json::json!({"arg": "value"})).with_id("call-123");

        assert_eq!(call.name, "my_tool");
        assert_eq!(call.id, Some("call-123".to_string()));
        assert_eq!(call.arguments["arg"], "value");
    }

    #[test]
    fn test_factory_config() {
        let gemini =
            factory::ProviderConfig::google_gemini_generate_content("my-project", "gemini-pro");
        assert_eq!(
            gemini.provider_type,
            factory::ProviderType::GoogleGeminiGenerateContent
        );
        assert_eq!(gemini.project_id, Some("my-project".to_string()));

        let chatgpt = factory::ProviderConfig::chatgpt("gpt-5.3-codex");
        assert_eq!(
            chatgpt.provider_type,
            factory::ProviderType::ChatgptResponses
        );
        assert_eq!(
            chatgpt.base_url,
            Some("https://chatgpt.com/backend-api/codex".to_string())
        );
        assert_eq!(chatgpt.api_key, None);

        let openai_responses = factory::ProviderConfig::openai_responses("sk-xxx", "gpt-5.4");
        assert_eq!(
            openai_responses.provider_type,
            factory::ProviderType::OpenAiResponses
        );
        assert_eq!(openai_responses.api_key, Some("sk-xxx".to_string()));

        let openai_chat_completions =
            factory::ProviderConfig::openai_chat_completions("sk-openai-chat", "gpt-4.1");
        assert_eq!(
            openai_chat_completions.provider_type,
            factory::ProviderType::OpenAiChatCompletions
        );
        assert_eq!(
            openai_chat_completions.api_key,
            Some("sk-openai-chat".to_string())
        );

        let openai_chat_completions_compatible =
            factory::ProviderConfig::openai_chat_completions_compatible(
                "sk-compat",
                "qwen3.5-plus",
            );
        assert_eq!(
            openai_chat_completions_compatible.provider_type,
            factory::ProviderType::OpenAiChatCompletionsCompatible
        );
        assert_eq!(
            openai_chat_completions_compatible.api_key,
            Some("sk-compat".to_string())
        );

        let anthropic_messages =
            factory::ProviderConfig::anthropic_messages("sk-ant", "claude-3-5-sonnet");
        assert_eq!(
            anthropic_messages.provider_type,
            factory::ProviderType::AnthropicMessages
        );
        assert_eq!(anthropic_messages.api_key, Some("sk-ant".to_string()));

        let openrouter =
            factory::ProviderConfig::openrouter("sk-or-xxx", "anthropic/claude-3-opus")
                .with_http_referer("https://alan.local")
                .with_x_title("alan")
                .with_app_categories(["cli-agent"]);
        assert_eq!(openrouter.provider_type, factory::ProviderType::OpenRouter);
        assert_eq!(openrouter.api_key, Some("sk-or-xxx".to_string()));
        assert_eq!(
            openrouter.base_url,
            Some("https://openrouter.ai/api/v1".to_string())
        );
        assert_eq!(
            openrouter.http_referer.as_deref(),
            Some("https://alan.local")
        );
        assert_eq!(openrouter.x_title.as_deref(), Some("alan"));
        assert_eq!(
            openrouter.app_categories,
            Some(vec!["cli-agent".to_string()])
        );
    }

    #[test]
    fn test_provider_capabilities_distinguish_provider_families() {
        let chatgpt = factory::ProviderType::ChatgptResponses.capabilities();
        let openai_responses = factory::ProviderType::OpenAiResponses.capabilities();
        let openai_chat = factory::ProviderType::OpenAiChatCompletions.capabilities();
        let openrouter = factory::ProviderType::OpenRouter.capabilities();
        let anthropic = factory::ProviderType::AnthropicMessages.capabilities();

        assert_eq!(
            chatgpt.compatibility_tier,
            CompatibilityTier::TierCBestEffortCompatible
        );
        assert!(!chatgpt.supports_server_managed_continuation);
        assert!(!chatgpt.supports_provider_compaction);
        assert_eq!(
            chatgpt.instruction_role,
            InstructionRole::ResponsesInstructions
        );

        assert!(openai_responses.supports_server_managed_continuation);
        assert!(openai_responses.supports_background_execution);
        assert!(openai_responses.supports_retrieve_cancel);
        assert!(openai_responses.supports_provider_compaction);
        assert_eq!(
            openai_responses.instruction_role,
            InstructionRole::ResponsesInstructions
        );

        assert_eq!(
            openai_chat.compatibility_tier,
            CompatibilityTier::TierBFullFidelityStateless
        );
        assert_eq!(openai_chat.instruction_role, InstructionRole::Developer);
        assert!(openai_chat.supports_multimodal_input);
        assert!(!openai_chat.supports_server_managed_continuation);

        assert_eq!(
            openrouter.compatibility_tier,
            CompatibilityTier::TierCBestEffortCompatible
        );
        assert!(openrouter.supports_streaming_text);
        assert!(openrouter.supports_streaming_tool_calls);
        assert!(openrouter.supports_provider_response_id);
        assert!(openrouter.supports_reasoning_text);
        assert!(!openrouter.supports_server_managed_continuation);
        assert!(!openrouter.supports_provider_compaction);

        assert_eq!(
            anthropic.compatibility_tier,
            CompatibilityTier::TierBFullFidelityStateless
        );
        assert_eq!(anthropic.instruction_role, InstructionRole::AnthropicSystem);
        assert!(anthropic.supports_multimodal_input);
        assert!(anthropic.supports_document_input);
        assert!(anthropic.supports_redacted_thinking);
    }

    #[tokio::test]
    async fn test_mock_provider() {
        use crate::LlmProvider;

        let mut mock = MockLlmProvider::new().with_response(GenerationResponse {
            content: "Test response".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        });

        let request = GenerationRequest::new().with_user_message("Hello");
        let response: GenerationResponse = LlmProvider::generate(&mut mock, request).await.unwrap();

        assert_eq!(response.content, "Test response");

        // Verify request was recorded
        let recorded: Vec<GenerationRequest> = mock.recorded_requests();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].messages[0].content, "Hello");
    }

    #[tokio::test]
    async fn test_mock_provider_multiple_responses() {
        use crate::LlmProvider;

        let mut mock = MockLlmProvider::new().with_responses(vec![
            GenerationResponse {
                content: "First".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            },
            GenerationResponse {
                content: "Second".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            },
        ]);

        let r1: GenerationResponse = LlmProvider::generate(&mut mock, GenerationRequest::new())
            .await
            .unwrap();
        let r2: GenerationResponse = LlmProvider::generate(&mut mock, GenerationRequest::new())
            .await
            .unwrap();

        assert_eq!(r1.content, "First");
        assert_eq!(r2.content, "Second");
    }

    #[tokio::test]
    async fn test_mock_provider_chat() {
        use crate::LlmProvider;

        let mut mock = MockLlmProvider::new();
        let response: String = LlmProvider::chat(&mut mock, Some("System"), "Hello")
            .await
            .unwrap();

        assert!(response.contains("Mock response to:"));
        assert!(response.contains("Hello"));
    }

    #[tokio::test]
    async fn test_mock_provider_stream() {
        use crate::LlmProvider;

        let mut mock = MockLlmProvider::new().with_response(GenerationResponse {
            content: "Streamed".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        });

        let mut rx: tokio::sync::mpsc::Receiver<StreamChunk> =
            LlmProvider::generate_stream(&mut mock, GenerationRequest::new())
                .await
                .unwrap();
        let first_chunk: StreamChunk = rx.recv().await.unwrap();
        let final_chunk: StreamChunk = rx.recv().await.unwrap();

        assert_eq!(first_chunk.text, Some("Streamed".to_string()));
        assert!(!first_chunk.is_finished);
        assert!(final_chunk.is_finished);
        assert_eq!(final_chunk.finish_reason.as_deref(), Some("stop"));
    }
}
