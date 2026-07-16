use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;

use crate::{
    AnthropicMessagesClient, ChatgptResponsesClient, CompatibilityTier,
    GoogleGeminiGenerateContentClient, InstructionRole, LlmProvider, OpenAiChatCompletionsClient,
    OpenAiResponsesClient, OpenRouterClient, ProviderCapabilities, openrouter,
};

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
    pub fn openai_chat_completions(api_key: impl Into<String>, model: impl Into<String>) -> Self {
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
                anyhow::anyhow!("OpenAI Chat Completions API-compatible provider requires api_key")
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

impl ProviderType {
    pub fn capabilities(self) -> ProviderCapabilities {
        match self {
            ProviderType::GoogleGeminiGenerateContent => ProviderCapabilities {
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
            ProviderType::ChatgptResponses => ProviderCapabilities {
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
            ProviderType::OpenAiResponses => ProviderCapabilities {
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
            ProviderType::OpenAiChatCompletions => ProviderCapabilities {
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
            ProviderType::OpenAiChatCompletionsCompatible => ProviderCapabilities {
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
            ProviderType::OpenRouter => ProviderCapabilities {
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
            ProviderType::AnthropicMessages => ProviderCapabilities {
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
