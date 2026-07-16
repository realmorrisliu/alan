use super::*;
use std::path::Path;

#[test]
fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.llm_provider, LlmProvider::OpenAiResponses);
    assert_eq!(
        config.google_gemini_generate_content_location,
        "us-central1"
    );
    assert_eq!(
        config.google_gemini_generate_content_model,
        "gemini-2.0-flash"
    );
    assert_eq!(
        config.openai_responses_base_url,
        "https://api.openai.com/v1"
    );
    assert_eq!(config.openai_responses_model, "gpt-5.4");
    assert_eq!(
        config.openai_chat_completions_base_url,
        "https://api.openai.com/v1"
    );
    assert_eq!(config.openai_chat_completions_model, "gpt-5.4");
    assert_eq!(
        config.openai_chat_completions_compatible_base_url,
        "https://api.openai.com/v1"
    );
    assert_eq!(
        config.openai_chat_completions_compatible_model,
        "qwen3.5-plus"
    );
    assert_eq!(config.openrouter_base_url, "https://openrouter.ai/api/v1");
    assert_eq!(config.openrouter_model, "moonshotai/kimi-k2.6");
    assert!(config.openrouter_api_key.is_none());
    assert!(config.openrouter_app_categories.is_empty());
    assert_eq!(
        config.anthropic_messages_base_url,
        "https://api.anthropic.com/v1"
    );
    assert_eq!(config.anthropic_messages_model, "claude-3-5-sonnet-latest");
    assert_eq!(config.llm_request_timeout_secs, 180);
    assert_eq!(config.tool_timeout_secs, 30);
    assert_eq!(config.tool_repeat_limit, 4);
    assert_eq!(config.context_window_tokens, None);
    assert_eq!(config.compaction_hard_trigger_ratio, None);
    assert_eq!(config.compaction_soft_trigger_ratio, None);
    assert!((config.effective_compaction_hard_trigger_ratio() - 0.8).abs() < f32::EPSILON);
    assert!((config.effective_compaction_soft_trigger_ratio() - 0.72).abs() < f32::EPSILON);
    assert_eq!(config.effective_context_window_tokens(), 1_050_000);
    assert_eq!(config.prompt_snapshot_max_chars, 8000);
    assert!(!config.prompt_snapshot_enabled);
    assert!(config.max_tool_loops.is_none());
    assert_eq!(config.streaming_mode, StreamingMode::Auto);
    assert_eq!(
        config.partial_stream_recovery_mode,
        PartialStreamRecoveryMode::ContinueOnce
    );
    assert!(config.skill_overrides.is_empty());
    // Memory config
    assert!(config.memory.enabled);
    assert!(config.memory.strict_store);
    assert!(config.memory.store_dir.is_none());
    assert!(!config.durability.required);
}

#[test]
fn definition_overlay_content_does_not_require_a_host_file() {
    let config = Config::default()
        .with_definition_overlay_content(
            "tool_repeat_limit = 7\n",
            Path::new("/lib/pkg/example/agents/root/agent.toml"),
        )
        .unwrap();

    assert_eq!(config.tool_repeat_limit, 7);
}

#[test]
fn explicit_config_override_must_be_an_existing_absolute_file() {
    let missing = std::env::temp_dir().join(format!(
        "alan-missing-config-{}.toml",
        uuid::Uuid::new_v4().simple()
    ));
    let error = Config::load_from_override(Some(missing)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("failed to read configuration file"),
        "unexpected missing-file error: {error:#}"
    );

    let error = Config::load_from_override(Some("agent.toml".into())).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ALAN_CONFIG_PATH must be an absolute path"),
        "unexpected relative-path error: {error:#}"
    );
}

#[test]
fn test_config_for_google_gemini_generate_content() {
    let config = Config::for_google_gemini_generate_content(
        "project",
        Some("europe-west1"),
        Some("gemini-2.5-pro"),
    );
    assert_eq!(
        config.llm_provider,
        LlmProvider::GoogleGeminiGenerateContent
    );
    assert_eq!(
        config.google_gemini_generate_content_project_id,
        Some("project".to_string())
    );
    assert_eq!(
        config.google_gemini_generate_content_location,
        "europe-west1"
    );
    assert_eq!(
        config.google_gemini_generate_content_model,
        "gemini-2.5-pro"
    );
    assert!(config.has_google_gemini_generate_content_config());
    assert!(config.has_llm_config());
}

#[test]
fn test_config_for_google_gemini_generate_content_defaults() {
    let config = Config::for_google_gemini_generate_content("project", None, None);
    assert_eq!(
        config.google_gemini_generate_content_location,
        "us-central1"
    );
    assert_eq!(
        config.google_gemini_generate_content_model,
        "gemini-2.0-flash"
    );
}

#[test]
fn test_config_for_openai_responses() {
    let config = Config::for_openai_responses(
        "sk-test",
        Some("https://api.openai.com/v1"),
        Some("gpt-5.4"),
    );
    assert_eq!(config.llm_provider, LlmProvider::OpenAiResponses);
    assert_eq!(config.openai_responses_api_key, Some("sk-test".to_string()));
    assert_eq!(config.openai_responses_model, "gpt-5.4");
    assert!(config.has_openai_responses_config());
    assert!(config.has_llm_config());
}

#[test]
fn test_config_for_openai_responses_defaults() {
    let config = Config::for_openai_responses("sk-test", None, None);
    assert_eq!(
        config.openai_responses_base_url,
        "https://api.openai.com/v1"
    );
    assert_eq!(config.openai_responses_model, "gpt-5.4");
}

#[test]
fn test_config_for_chatgpt() {
    let config = Config::for_chatgpt(
        Some("https://chatgpt.com/backend-api/codex"),
        Some("gpt-5.3-codex"),
    );
    assert_eq!(config.llm_provider, LlmProvider::Chatgpt);
    assert_eq!(
        config.chatgpt_base_url,
        "https://chatgpt.com/backend-api/codex"
    );
    assert_eq!(config.chatgpt_model, "gpt-5.3-codex");
    assert!(config.has_llm_config());
}

#[test]
fn test_config_for_chatgpt_defaults() {
    let config = Config::for_chatgpt(None, None);
    assert_eq!(
        config.chatgpt_base_url,
        "https://chatgpt.com/backend-api/codex"
    );
    assert_eq!(config.chatgpt_model, "gpt-5.3-codex");
}

#[test]
fn test_config_for_openai_chat_completions() {
    let config = Config::for_openai_chat_completions(
        "sk-test",
        Some("https://api.openai.com/v1"),
        Some("gpt-5.4"),
    );
    assert_eq!(config.llm_provider, LlmProvider::OpenAiChatCompletions);
    assert_eq!(
        config.openai_chat_completions_api_key,
        Some("sk-test".to_string())
    );
    assert_eq!(config.openai_chat_completions_model, "gpt-5.4");
    assert!(config.has_openai_chat_completions_config());
    assert!(config.has_llm_config());
}

#[test]
fn test_config_for_openai_chat_completions_defaults() {
    let config = Config::for_openai_chat_completions("sk-test", None, None);
    assert_eq!(
        config.openai_chat_completions_base_url,
        "https://api.openai.com/v1"
    );
    assert_eq!(config.openai_chat_completions_model, "gpt-5.4");
}

#[test]
fn test_config_for_openai_chat_completions_compatible() {
    let config = Config::for_openai_chat_completions_compatible(
        "sk-test",
        Some("https://api.openai.com/v1"),
        Some("qwen3.5-plus"),
    );
    assert_eq!(
        config.llm_provider,
        LlmProvider::OpenAiChatCompletionsCompatible
    );
    assert_eq!(
        config.openai_chat_completions_compatible_api_key,
        Some("sk-test".to_string())
    );
    assert_eq!(
        config.openai_chat_completions_compatible_model,
        "qwen3.5-plus"
    );
    assert!(config.has_openai_chat_completions_compatible_config());
    assert!(config.has_llm_config());
}

#[test]
fn test_config_for_openai_chat_completions_compatible_defaults() {
    let config = Config::for_openai_chat_completions_compatible("sk-test", None, None);
    assert_eq!(
        config.openai_chat_completions_compatible_base_url,
        "https://api.openai.com/v1"
    );
    assert_eq!(
        config.openai_chat_completions_compatible_model,
        "qwen3.5-plus"
    );
}

#[test]
fn test_config_for_openrouter() {
    let config = Config::for_openrouter(
        "sk-or",
        None,
        Some("anthropic/claude-sonnet-4"),
        Some("https://alan.local"),
        Some("alan"),
        vec!["cli-agent".to_string()],
    );
    assert_eq!(config.llm_provider, LlmProvider::OpenRouter);
    assert_eq!(config.openrouter_api_key.as_deref(), Some("sk-or"));
    assert_eq!(config.openrouter_base_url, "https://openrouter.ai/api/v1");
    assert_eq!(config.openrouter_model, "anthropic/claude-sonnet-4");
    assert_eq!(
        config.openrouter_http_referer.as_deref(),
        Some("https://alan.local")
    );
    assert_eq!(config.openrouter_x_title.as_deref(), Some("alan"));
    assert_eq!(config.openrouter_app_categories, vec!["cli-agent"]);
    assert!(config.has_openrouter_config());
    assert!(config.has_llm_config());
}

#[test]
fn test_config_for_anthropic_messages() {
    let config = Config::for_anthropic_messages(
        "ak-test",
        Some("https://api.anthropic.com/v1"),
        Some("claude-sonnet-4-5"),
    );
    assert_eq!(config.llm_provider, LlmProvider::AnthropicMessages);
    assert_eq!(
        config.anthropic_messages_api_key,
        Some("ak-test".to_string())
    );
    assert_eq!(config.anthropic_messages_model, "claude-sonnet-4-5");
    assert!(config.has_anthropic_messages_config());
    assert!(config.has_llm_config());
}

#[test]
fn test_config_for_anthropic_messages_with_options() {
    let config = Config {
        llm_provider: LlmProvider::AnthropicMessages,
        anthropic_messages_api_key: Some("key".to_string()),
        anthropic_messages_base_url: "https://api.anthropic.com/v1".to_string(),
        anthropic_messages_model: "claude-3".to_string(),
        anthropic_messages_client_name: Some("test-client".to_string()),
        anthropic_messages_user_agent: Some("test-agent/1.0".to_string()),
        ..Config::default()
    };
    assert_eq!(
        config.anthropic_messages_client_name,
        Some("test-client".to_string())
    );
    assert_eq!(
        config.anthropic_messages_user_agent,
        Some("test-agent/1.0".to_string())
    );
}

#[test]
fn test_config_for_anthropic_messages_defaults() {
    let config = Config::for_anthropic_messages("ak-test", None, None);
    assert_eq!(
        config.anthropic_messages_base_url,
        "https://api.anthropic.com/v1"
    );
    assert_eq!(config.anthropic_messages_model, "claude-3-5-sonnet-latest");
}

#[test]
fn test_effective_model() {
    let gemini =
        Config::for_google_gemini_generate_content("project", None, Some("gemini-2.5-pro"));
    assert_eq!(gemini.effective_model(), "gemini-2.5-pro");

    let chatgpt = Config::for_chatgpt(None, Some("gpt-5.3-codex"));
    assert_eq!(chatgpt.effective_model(), "gpt-5.3-codex");

    let openai_responses = Config::for_openai_responses("k", None, Some("gpt-5.4"));
    assert_eq!(openai_responses.effective_model(), "gpt-5.4");

    let openai_chat_completions = Config::for_openai_chat_completions("k", None, Some("gpt-5.4"));
    assert_eq!(openai_chat_completions.effective_model(), "gpt-5.4");

    let openai_chat_completions_compatible =
        Config::for_openai_chat_completions_compatible("k", None, Some("qwen3.5-plus"));
    assert_eq!(
        openai_chat_completions_compatible.effective_model(),
        "qwen3.5-plus"
    );

    let anthropic = Config::for_anthropic_messages("k", None, Some("claude-3-5-sonnet"));
    assert_eq!(anthropic.effective_model(), "claude-3-5-sonnet");

    let openrouter = Config::for_openrouter(
        "sk-or",
        None,
        Some("openai/gpt-5.4"),
        None,
        None,
        Vec::new(),
    );
    assert_eq!(openrouter.effective_model(), "openai/gpt-5.4");
}

#[test]
fn test_has_llm_config_without_api_key() {
    let mut config = Config {
        llm_provider: LlmProvider::OpenAiResponses,
        openai_responses_api_key: None,
        openai_chat_completions_api_key: None,
        openai_chat_completions_compatible_api_key: None,
        ..Config::default()
    };
    assert!(!config.has_openai_responses_config());
    assert!(!config.has_llm_config());

    config.llm_provider = LlmProvider::OpenAiChatCompletions;
    assert!(!config.has_openai_chat_completions_config());
    assert!(!config.has_llm_config());

    config.llm_provider = LlmProvider::OpenAiChatCompletionsCompatible;
    assert!(!config.has_openai_chat_completions_compatible_config());
    assert!(!config.has_llm_config());

    config.llm_provider = LlmProvider::OpenRouter;
    config.openrouter_api_key = Some("sk-or".to_string());
    config.openrouter_model.clear();
    assert!(!config.has_openrouter_config());
    assert!(!config.has_llm_config());

    config.llm_provider = LlmProvider::AnthropicMessages;
    config.anthropic_messages_api_key = None;
    assert!(!config.has_anthropic_messages_config());
    assert!(!config.has_llm_config());

    config.llm_provider = LlmProvider::GoogleGeminiGenerateContent;
    config.google_gemini_generate_content_project_id = None;
    assert!(!config.has_google_gemini_generate_content_config());
}

#[test]
fn test_openai_provider_does_not_treat_compat_key_as_valid_config() {
    let config = Config {
        llm_provider: LlmProvider::OpenAiResponses,
        openai_responses_api_key: None,
        openai_chat_completions_compatible_api_key: Some("sk-legacy".to_string()),
        ..Config::default()
    };

    assert!(!config.has_openai_responses_config());
    assert!(!config.has_llm_config());
}

#[test]
fn test_llm_provider_deserialization() {
    let gemini: LlmProvider = serde_json::from_str("\"google_gemini_generate_content\"").unwrap();
    assert_eq!(gemini, LlmProvider::GoogleGeminiGenerateContent);

    let chatgpt: LlmProvider = serde_json::from_str("\"chatgpt\"").unwrap();
    assert_eq!(chatgpt, LlmProvider::Chatgpt);

    let openai_responses: LlmProvider = serde_json::from_str("\"openai_responses\"").unwrap();
    assert_eq!(openai_responses, LlmProvider::OpenAiResponses);

    let openai_chat_completions: LlmProvider =
        serde_json::from_str("\"openai_chat_completions\"").unwrap();
    assert_eq!(openai_chat_completions, LlmProvider::OpenAiChatCompletions);

    let openai_chat_completions_compatible: LlmProvider =
        serde_json::from_str("\"openai_chat_completions_compatible\"").unwrap();
    assert_eq!(
        openai_chat_completions_compatible,
        LlmProvider::OpenAiChatCompletionsCompatible
    );

    let openrouter: LlmProvider = serde_json::from_str("\"openrouter\"").unwrap();
    assert_eq!(openrouter, LlmProvider::OpenRouter);

    let anthropic: LlmProvider = serde_json::from_str("\"anthropic_messages\"").unwrap();
    assert_eq!(anthropic, LlmProvider::AnthropicMessages);
}
