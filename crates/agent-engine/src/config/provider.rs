use super::{
    Config, LlmProvider, default_anthropic_messages_base_url, default_anthropic_messages_model,
    default_chatgpt_base_url, default_chatgpt_model,
    default_google_gemini_generate_content_location, default_google_gemini_generate_content_model,
    default_openai_chat_completions_base_url, default_openai_chat_completions_compatible_base_url,
    default_openai_chat_completions_compatible_model, default_openai_chat_completions_model,
    default_openai_responses_base_url, default_openai_responses_model, default_openrouter_base_url,
    default_openrouter_model,
};
use crate::models::{self, ModelCatalogProvider, ModelInfo};
use std::sync::Arc;

impl Config {
    pub fn for_google_gemini_generate_content(
        project_id: &str,
        location: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::GoogleGeminiGenerateContent,
            google_gemini_generate_content_project_id: Some(project_id.to_string()),
            google_gemini_generate_content_location: location
                .map(ToString::to_string)
                .unwrap_or_else(default_google_gemini_generate_content_location),
            google_gemini_generate_content_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_google_gemini_generate_content_model),
            ..Self::default()
        }
    }

    pub fn for_openai_responses(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::OpenAiResponses,
            openai_responses_api_key: Some(api_key.to_string()),
            openai_responses_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_responses_base_url),
            openai_responses_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_responses_model),
            ..Self::default()
        }
    }

    pub fn for_chatgpt(base_url: Option<&str>, model: Option<&str>) -> Self {
        Self {
            llm_provider: LlmProvider::Chatgpt,
            chatgpt_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_chatgpt_base_url),
            chatgpt_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_chatgpt_model),
            chatgpt_account_id: None,
            ..Self::default()
        }
    }

    pub fn for_openai_chat_completions(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::OpenAiChatCompletions,
            openai_chat_completions_api_key: Some(api_key.to_string()),
            openai_chat_completions_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_chat_completions_base_url),
            openai_chat_completions_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_chat_completions_model),
            ..Self::default()
        }
    }

    pub fn for_openai_chat_completions_compatible(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::OpenAiChatCompletionsCompatible,
            openai_chat_completions_compatible_api_key: Some(api_key.to_string()),
            openai_chat_completions_compatible_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_chat_completions_compatible_base_url),
            openai_chat_completions_compatible_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_chat_completions_compatible_model),
            ..Self::default()
        }
    }

    pub fn for_openrouter(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
        http_referer: Option<&str>,
        x_title: Option<&str>,
        app_categories: Vec<String>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::OpenRouter,
            openrouter_api_key: Some(api_key.to_string()),
            openrouter_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_openrouter_base_url),
            openrouter_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_openrouter_model),
            openrouter_http_referer: http_referer.map(ToString::to_string),
            openrouter_x_title: x_title.map(ToString::to_string),
            openrouter_app_categories: app_categories,
            ..Self::default()
        }
    }

    pub fn for_anthropic_messages(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::AnthropicMessages,
            anthropic_messages_api_key: Some(api_key.to_string()),
            anthropic_messages_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_anthropic_messages_base_url),
            anthropic_messages_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_anthropic_messages_model),
            ..Self::default()
        }
    }

    pub fn has_google_gemini_generate_content_config(&self) -> bool {
        self.google_gemini_generate_content_project_id.is_some()
    }

    pub fn has_openai_responses_config(&self) -> bool {
        self.openai_responses_api_key.is_some()
    }

    pub fn has_openai_chat_completions_config(&self) -> bool {
        self.openai_chat_completions_api_key.is_some()
    }

    pub fn has_openai_chat_completions_compatible_config(&self) -> bool {
        self.openai_chat_completions_compatible_api_key.is_some()
    }

    pub fn has_openrouter_config(&self) -> bool {
        self.openrouter_api_key.is_some() && !self.openrouter_model.trim().is_empty()
    }

    pub fn has_anthropic_messages_config(&self) -> bool {
        self.anthropic_messages_api_key.is_some()
    }

    pub fn has_llm_config(&self) -> bool {
        match self.llm_provider {
            LlmProvider::GoogleGeminiGenerateContent => {
                self.has_google_gemini_generate_content_config()
            }
            LlmProvider::Chatgpt => true,
            LlmProvider::OpenAiResponses => self.has_openai_responses_config(),
            LlmProvider::OpenAiChatCompletions => self.has_openai_chat_completions_config(),
            LlmProvider::OpenAiChatCompletionsCompatible => {
                self.has_openai_chat_completions_compatible_config()
            }
            LlmProvider::OpenRouter => self.has_openrouter_config(),
            LlmProvider::AnthropicMessages => self.has_anthropic_messages_config(),
        }
    }

    pub fn effective_model(&self) -> &str {
        match self.llm_provider {
            LlmProvider::GoogleGeminiGenerateContent => &self.google_gemini_generate_content_model,
            LlmProvider::Chatgpt => &self.chatgpt_model,
            LlmProvider::OpenAiResponses => self.resolved_openai_responses_model(),
            LlmProvider::OpenAiChatCompletions => self.resolved_openai_chat_completions_model(),
            LlmProvider::OpenAiChatCompletionsCompatible => {
                &self.openai_chat_completions_compatible_model
            }
            LlmProvider::OpenRouter => &self.openrouter_model,
            LlmProvider::AnthropicMessages => &self.anthropic_messages_model,
        }
    }

    pub fn set_effective_model(&mut self, model: impl Into<String>) {
        let model = model.into();
        match self.llm_provider {
            LlmProvider::GoogleGeminiGenerateContent => {
                self.google_gemini_generate_content_model = model;
            }
            LlmProvider::Chatgpt => {
                self.chatgpt_model = model;
            }
            LlmProvider::OpenAiResponses => {
                self.openai_responses_model = model;
            }
            LlmProvider::OpenAiChatCompletions => {
                self.openai_chat_completions_model = model;
            }
            LlmProvider::OpenAiChatCompletionsCompatible => {
                self.openai_chat_completions_compatible_model = model;
            }
            LlmProvider::OpenRouter => {
                self.openrouter_model = model;
            }
            LlmProvider::AnthropicMessages => {
                self.anthropic_messages_model = model;
            }
        }
    }

    pub fn set_model_catalog(&mut self, model_catalog: Arc<crate::ModelCatalog>) {
        self.model_catalog = Some(model_catalog);
    }

    pub fn effective_model_info(&self) -> Option<&ModelInfo> {
        match self.llm_provider {
            LlmProvider::Chatgpt => None,
            LlmProvider::OpenAiResponses => self.resolved_model_catalog().find_model_info(
                ModelCatalogProvider::OpenAiResponses,
                self.resolved_openai_responses_model(),
            ),
            LlmProvider::OpenAiChatCompletions => self.resolved_model_catalog().find_model_info(
                ModelCatalogProvider::OpenAiChatCompletions,
                self.resolved_openai_chat_completions_model(),
            ),
            LlmProvider::OpenAiChatCompletionsCompatible => {
                self.resolved_model_catalog().find_model_info(
                    ModelCatalogProvider::OpenAiChatCompletionsCompatible,
                    &self.openai_chat_completions_compatible_model,
                )
            }
            LlmProvider::OpenRouter
            | LlmProvider::GoogleGeminiGenerateContent
            | LlmProvider::AnthropicMessages => None,
        }
    }

    pub fn effective_context_window_tokens(&self) -> u32 {
        self.context_window_tokens
            .or_else(|| {
                self.effective_model_info()
                    .map(|model_info| model_info.context_window_tokens)
            })
            .unwrap_or_else(|| inferred_context_window_tokens(self.llm_provider))
    }

    fn resolved_openai_responses_model(&self) -> &str {
        &self.openai_responses_model
    }

    fn resolved_openai_chat_completions_model(&self) -> &str {
        &self.openai_chat_completions_model
    }

    /// Convert to LLM provider configuration
    pub fn to_provider_config(&self) -> anyhow::Result<crate::llm::ProviderConfig> {
        use crate::llm::factory::ProviderConfig;

        match self.llm_provider {
            LlmProvider::GoogleGeminiGenerateContent => {
                let project_id = self
                    .google_gemini_generate_content_project_id
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Google Gemini GenerateContent API provider requires google_gemini_generate_content_project_id"
                        )
                    })?;
                Ok(ProviderConfig::google_gemini_generate_content(
                    project_id,
                    &self.google_gemini_generate_content_model,
                )
                .with_location(&self.google_gemini_generate_content_location))
            }
            LlmProvider::Chatgpt => {
                let mut provider_config = ProviderConfig::chatgpt(&self.chatgpt_model)
                    .with_base_url(&self.chatgpt_base_url);
                if let Some(account_id) = &self.chatgpt_account_id {
                    provider_config = provider_config.with_chatgpt_account_id(account_id);
                }
                Ok(provider_config)
            }
            LlmProvider::OpenAiResponses => {
                let api_key = self.openai_responses_api_key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "OpenAI Responses API provider requires openai_responses_api_key"
                    )
                })?;
                validate_supported_model(
                    self.resolved_model_catalog(),
                    "OpenAI Responses API",
                    ModelCatalogProvider::OpenAiResponses,
                    self.resolved_openai_responses_model(),
                )?;
                Ok(ProviderConfig::openai_responses(
                    api_key,
                    self.resolved_openai_responses_model(),
                )
                .with_base_url(&self.openai_responses_base_url))
            }
            LlmProvider::OpenAiChatCompletions => {
                let api_key = self
                    .openai_chat_completions_api_key
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "OpenAI Chat Completions API provider requires openai_chat_completions_api_key"
                        )
                    })?;
                validate_supported_model(
                    self.resolved_model_catalog(),
                    "OpenAI Chat Completions API",
                    ModelCatalogProvider::OpenAiChatCompletions,
                    self.resolved_openai_chat_completions_model(),
                )?;
                Ok(ProviderConfig::openai_chat_completions(
                    api_key,
                    self.resolved_openai_chat_completions_model(),
                )
                .with_base_url(&self.openai_chat_completions_base_url))
            }
            LlmProvider::OpenAiChatCompletionsCompatible => {
                let api_key = self
                    .openai_chat_completions_compatible_api_key
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "OpenAI Chat Completions API-compatible provider requires openai_chat_completions_compatible_api_key"
                        )
                    })?;
                validate_supported_model(
                    self.resolved_model_catalog(),
                    "OpenAI Chat Completions API-compatible",
                    ModelCatalogProvider::OpenAiChatCompletionsCompatible,
                    &self.openai_chat_completions_compatible_model,
                )?;
                Ok(ProviderConfig::openai_chat_completions_compatible(
                    api_key,
                    &self.openai_chat_completions_compatible_model,
                )
                .with_base_url(&self.openai_chat_completions_compatible_base_url))
            }
            LlmProvider::OpenRouter => {
                let api_key = self.openrouter_api_key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("OpenRouter provider requires openrouter_api_key")
                })?;
                if self.openrouter_model.trim().is_empty() {
                    anyhow::bail!("OpenRouter provider requires openrouter_model");
                }
                let mut config = ProviderConfig::openrouter(api_key, &self.openrouter_model)
                    .with_base_url(&self.openrouter_base_url);
                if let Some(http_referer) = &self.openrouter_http_referer {
                    config = config.with_http_referer(http_referer);
                }
                if let Some(x_title) = &self.openrouter_x_title {
                    config = config.with_x_title(x_title);
                }
                if !self.openrouter_app_categories.is_empty() {
                    config = config.with_app_categories(self.openrouter_app_categories.clone());
                }
                Ok(config)
            }
            LlmProvider::AnthropicMessages => {
                let api_key = self.anthropic_messages_api_key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Anthropic Messages API provider requires anthropic_messages_api_key"
                    )
                })?;
                let mut config =
                    ProviderConfig::anthropic_messages(api_key, &self.anthropic_messages_model)
                        .with_base_url(&self.anthropic_messages_base_url);

                if let Some(client_name) = &self.anthropic_messages_client_name {
                    config = config.with_client_name(client_name);
                }
                if let Some(user_agent) = &self.anthropic_messages_user_agent {
                    config = config.with_user_agent(user_agent);
                }

                Ok(config)
            }
        }
    }

    fn resolved_model_catalog(&self) -> &crate::ModelCatalog {
        if let Some(model_catalog) = self.model_catalog.as_deref() {
            model_catalog
        } else {
            models::base_catalog()
        }
    }
}

fn inferred_context_window_tokens(provider: LlmProvider) -> u32 {
    match provider {
        LlmProvider::GoogleGeminiGenerateContent => 1_048_576,
        LlmProvider::AnthropicMessages => 200_000,
        LlmProvider::Chatgpt => 400_000,
        LlmProvider::OpenAiResponses
        | LlmProvider::OpenAiChatCompletions
        | LlmProvider::OpenAiChatCompletionsCompatible
        | LlmProvider::OpenRouter => 32_768,
    }
}

fn validate_supported_model(
    catalog: &crate::ModelCatalog,
    provider_name: &str,
    provider: ModelCatalogProvider,
    model: &str,
) -> anyhow::Result<()> {
    if catalog.find_model_info(provider, model).is_some() {
        return Ok(());
    }

    let supported = catalog.supported_model_slugs(provider).join(", ");
    anyhow::bail!(
        "{provider_name} model `{model}` is not in alan's curated catalog. Supported models: {supported}"
    );
}
