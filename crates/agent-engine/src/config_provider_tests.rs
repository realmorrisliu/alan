use super::*;

#[test]
fn test_to_provider_config_gemini() {
    let config =
        Config::for_google_gemini_generate_content("my-project", None, Some("gemini-2.0-flash"));
    let provider_config = config.to_provider_config().unwrap();
    // Verify it creates the right config type
    assert_eq!(
        provider_config.provider_type,
        alan_llm::factory::ProviderType::GoogleGeminiGenerateContent
    );
    assert_eq!(provider_config.project_id, Some("my-project".to_string()));
    assert_eq!(provider_config.model, "gemini-2.0-flash");
}

#[test]
fn test_to_provider_config_google_gemini_generate_content_missing_project() {
    let config = Config {
        llm_provider: LlmProvider::GoogleGeminiGenerateContent,
        google_gemini_generate_content_project_id: None,
        ..Config::default()
    };
    let result = config.to_provider_config();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("google_gemini_generate_content_project_id")
    );
}

#[test]
fn test_to_provider_config_openai_responses() {
    let config = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.provider_type,
        alan_llm::factory::ProviderType::OpenAiResponses
    );
    assert_eq!(provider_config.api_key, Some("sk-test".to_string()));
    assert_eq!(provider_config.model, "gpt-5.4");
}

#[test]
fn test_to_provider_config_chatgpt() {
    let config = Config::for_chatgpt(
        Some("https://chatgpt.com/backend-api/codex"),
        Some("gpt-5.3-codex"),
    );
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.provider_type,
        alan_llm::factory::ProviderType::ChatgptResponses
    );
    assert_eq!(provider_config.api_key, None);
    assert_eq!(
        provider_config.base_url,
        Some("https://chatgpt.com/backend-api/codex".to_string())
    );
    assert_eq!(provider_config.model, "gpt-5.3-codex");
    assert_eq!(provider_config.expected_account_id, None);
}

#[test]
fn test_to_provider_config_chatgpt_with_account_binding() {
    let mut config = Config::for_chatgpt(
        Some("https://chatgpt.com/backend-api/codex"),
        Some("gpt-5.3-codex"),
    );
    config.chatgpt_account_id = Some("acct_123".to_string());
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.expected_account_id.as_deref(),
        Some("acct_123")
    );
}

#[test]
fn test_to_provider_config_openai_responses_missing_key() {
    let config = Config {
        llm_provider: LlmProvider::OpenAiResponses,
        openai_responses_api_key: None,
        openai_chat_completions_api_key: None,
        openai_chat_completions_compatible_api_key: None,
        ..Config::default()
    };
    let result = config.to_provider_config();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("openai_responses_api_key")
    );
}

#[test]
fn test_to_provider_config_openai_chat_completions() {
    let config = Config::for_openai_chat_completions("sk-test", None, Some("gpt-5.4"));
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.provider_type,
        alan_llm::factory::ProviderType::OpenAiChatCompletions
    );
    assert_eq!(provider_config.api_key, Some("sk-test".to_string()));
    assert_eq!(provider_config.model, "gpt-5.4");
}

#[test]
fn test_to_provider_config_openai_chat_completions_compatible() {
    let config =
        Config::for_openai_chat_completions_compatible("sk-test", None, Some("qwen3.5-plus"));
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.provider_type,
        alan_llm::factory::ProviderType::OpenAiChatCompletionsCompatible
    );
    assert_eq!(provider_config.api_key, Some("sk-test".to_string()));
    assert_eq!(provider_config.model, "qwen3.5-plus");
}

#[test]
fn test_to_provider_config_openrouter() {
    let config = Config::for_openrouter(
        "sk-or",
        Some("https://openrouter.example/api/v1"),
        Some("anthropic/claude-sonnet-4"),
        Some("https://alan.local"),
        Some("alan"),
        vec!["cli-agent".to_string()],
    );
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.provider_type,
        alan_llm::factory::ProviderType::OpenRouter
    );
    assert_eq!(provider_config.api_key.as_deref(), Some("sk-or"));
    assert_eq!(
        provider_config.base_url.as_deref(),
        Some("https://openrouter.example/api/v1")
    );
    assert_eq!(provider_config.model, "anthropic/claude-sonnet-4");
    assert_eq!(
        provider_config.http_referer.as_deref(),
        Some("https://alan.local")
    );
    assert_eq!(provider_config.x_title.as_deref(), Some("alan"));
    assert_eq!(
        provider_config.app_categories,
        Some(vec!["cli-agent".to_string()])
    );
}

#[test]
fn test_to_provider_config_openrouter_missing_model() {
    let config = Config {
        llm_provider: LlmProvider::OpenRouter,
        openrouter_api_key: Some("sk-or".to_string()),
        openrouter_model: String::new(),
        ..Config::default()
    };
    let result = config.to_provider_config();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("openrouter_model"));
}

#[test]
fn test_to_provider_config_openai_chat_completions_compatible_accepts_snapshot_and_vendor_prefix() {
    let config = Config::for_openai_chat_completions_compatible(
        "sk-test",
        None,
        Some("bailian/qwen3.5-plus-2026-02-15"),
    );
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.provider_type,
        alan_llm::factory::ProviderType::OpenAiChatCompletionsCompatible
    );
    assert_eq!(provider_config.model, "bailian/qwen3.5-plus-2026-02-15");
}

#[test]
fn test_to_provider_config_openai_chat_completions_compatible_rejects_non_snapshot_variant_suffix()
{
    let config =
        Config::for_openai_chat_completions_compatible("sk-test", None, Some("kimi-k2.5-thinking"));
    let result = config.to_provider_config();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("curated catalog"));
}

#[test]
fn test_to_provider_config_openai_does_not_fall_back_to_compat_settings() {
    let config = Config {
        llm_provider: LlmProvider::OpenAiResponses,
        openai_responses_api_key: None,
        openai_chat_completions_compatible_api_key: Some("sk-legacy".to_string()),
        openai_chat_completions_compatible_base_url: "https://proxy.example/v1".to_string(),
        openai_chat_completions_compatible_model: "qwen3.5-plus".to_string(),
        ..Config::default()
    };

    let result = config.to_provider_config();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("openai_responses_api_key")
    );
}

#[test]
fn test_to_provider_config_openai_rejects_unsupported_model() {
    let config = Config::for_openai_responses("sk-test", None, Some("gpt-4o"));
    let result = config.to_provider_config();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("curated catalog"));
}

#[test]
fn test_to_provider_config_openai_chat_completions_compatible_rejects_outdated_model_family() {
    let config = Config::for_openai_chat_completions_compatible("sk-test", None, Some("kimi-k2"));
    let result = config.to_provider_config();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("curated catalog"));
}

#[test]
fn test_to_provider_config_anthropic_messages() {
    let config = Config::for_anthropic_messages("sk-test", None, Some("claude-3"));
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.provider_type,
        alan_llm::factory::ProviderType::AnthropicMessages
    );
    assert_eq!(provider_config.api_key, Some("sk-test".to_string()));
    assert_eq!(provider_config.model, "claude-3");
}

#[test]
fn test_to_provider_config_anthropic_messages_with_options() {
    let config = Config {
        llm_provider: LlmProvider::AnthropicMessages,
        anthropic_messages_api_key: Some("key".to_string()),
        anthropic_messages_base_url: "https://custom.api.com".to_string(),
        anthropic_messages_model: "claude-3".to_string(),
        anthropic_messages_client_name: Some("test-client".to_string()),
        anthropic_messages_user_agent: Some("test-agent/1.0".to_string()),
        ..Config::default()
    };
    let provider_config = config.to_provider_config().unwrap();
    assert_eq!(
        provider_config.base_url,
        Some("https://custom.api.com".to_string())
    );
    assert_eq!(provider_config.client_name, Some("test-client".to_string()));
    assert_eq!(
        provider_config.user_agent,
        Some("test-agent/1.0".to_string())
    );
}

#[test]
fn test_to_provider_config_anthropic_messages_missing_key() {
    let config = Config {
        llm_provider: LlmProvider::AnthropicMessages,
        anthropic_messages_api_key: None,
        ..Config::default()
    };
    let result = config.to_provider_config();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("anthropic_messages_api_key")
    );
}

#[test]
fn test_default_functions() {
    assert_eq!(default_llm_provider(), LlmProvider::OpenAiResponses);
    assert_eq!(
        default_google_gemini_generate_content_location(),
        "us-central1"
    );
    assert_eq!(
        default_google_gemini_generate_content_model(),
        "gemini-2.0-flash"
    );
    assert_eq!(
        default_openai_responses_base_url(),
        "https://api.openai.com/v1"
    );
    assert_eq!(default_openai_responses_model(), "gpt-5.4");
    assert_eq!(
        default_openai_chat_completions_base_url(),
        "https://api.openai.com/v1"
    );
    assert_eq!(default_openai_chat_completions_model(), "gpt-5.4");
    assert_eq!(
        default_openai_chat_completions_compatible_base_url(),
        "https://api.openai.com/v1"
    );
    assert_eq!(
        default_openai_chat_completions_compatible_model(),
        "qwen3.5-plus"
    );
    assert_eq!(
        default_openrouter_base_url(),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(default_openrouter_model(), "moonshotai/kimi-k2.6");
    assert_eq!(
        default_anthropic_messages_base_url(),
        "https://api.anthropic.com/v1"
    );
    assert_eq!(
        default_anthropic_messages_model(),
        "claude-3-5-sonnet-latest"
    );
    assert_eq!(default_llm_timeout_secs(), 180);
    assert_eq!(default_tool_timeout_secs(), 30);
    assert_eq!(default_prompt_snapshot_max_chars(), 8000);
    assert_eq!(default_tool_repeat_limit(), 4);
    assert_eq!(default_streaming_mode(), StreamingMode::Auto);
    assert_eq!(
        default_partial_stream_recovery_mode(),
        PartialStreamRecoveryMode::ContinueOnce
    );
}
