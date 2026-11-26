use super::{ProviderConfig, ProviderType, create_provider};
use crate::CalibrationKind;

#[test]
fn provider_type_native_decision_metadata_matches_created_providers() {
    assert!(ProviderType::Magpie.supports_native_decisions());
    assert!(ProviderType::GoogleGeminiGenerateContent.supports_native_decisions());
    assert!(ProviderType::OpenAiResponses.supports_native_decisions());
    assert!(ProviderType::OpenRouter.supports_native_decisions());
    assert!(ProviderType::TypesafeEvaluation.supports_native_decisions());

    assert!(!ProviderType::ChatgptResponses.supports_native_decisions());
    assert!(!ProviderType::OpenAiChatCompletions.supports_native_decisions());
    assert!(!ProviderType::OpenAiChatCompletionsCompatible.supports_native_decisions());
    assert!(!ProviderType::AnthropicMessages.supports_native_decisions());

    for provider_type in [
        ProviderType::Magpie,
        ProviderType::GoogleGeminiGenerateContent,
        ProviderType::OpenAiResponses,
        ProviderType::OpenRouter,
        ProviderType::TypesafeEvaluation,
    ] {
        assert_eq!(
            provider_type.default_decision_calibration(),
            Some(CalibrationKind::Calibrated)
        );
    }

    for provider_type in [
        ProviderType::ChatgptResponses,
        ProviderType::OpenAiChatCompletions,
        ProviderType::OpenAiChatCompletionsCompatible,
        ProviderType::AnthropicMessages,
    ] {
        assert_eq!(provider_type.default_decision_calibration(), None);
    }
}

#[test]
fn factory_decision_capabilities_and_fallback_wiring_stay_consistent() {
    let dir = tempfile::TempDir::new().unwrap();
    let chatgpt_auth_path = dir.path().join("auth.json");

    let cases: Vec<(&str, ProviderConfig)> = vec![
        ("magpie_default", ProviderConfig::magpie("auto/qwen3-coder")),
        (
            "magpie_fallback",
            ProviderConfig::magpie("auto/qwen3-coder").with_decision_fallback(true),
        ),
        (
            "gemini_adc",
            ProviderConfig::google_gemini_generate_content("my-proj", "gemini-decision-1"),
        ),
        (
            "gemini_express",
            ProviderConfig::google_gemini_express("express-key", "gemini-decision-1"),
        ),
        (
            "openai_responses",
            ProviderConfig::openai_responses("sk-test", "gpt-5.4"),
        ),
        (
            "openrouter",
            ProviderConfig::openrouter("or-test", "openai/gpt-5"),
        ),
        (
            "typesafe",
            ProviderConfig::typesafe_evaluation("ts-test", "jev-1.13.0"),
        ),
        (
            "openai_chat_default",
            ProviderConfig::openai_chat_completions("sk-test", "gpt-5"),
        ),
        (
            "openai_chat_fallback",
            ProviderConfig::openai_chat_completions("sk-test", "gpt-5")
                .with_decision_fallback(true),
        ),
        (
            "openai_compat_default",
            ProviderConfig::openai_chat_completions_compatible("sk-test", "local-model"),
        ),
        (
            "openai_compat_fallback",
            ProviderConfig::openai_chat_completions_compatible("sk-test", "local-model")
                .with_decision_fallback(true),
        ),
        (
            "anthropic_default",
            ProviderConfig::anthropic_messages("sk-ant", "claude-sonnet-4-5"),
        ),
        (
            "anthropic_fallback",
            ProviderConfig::anthropic_messages("sk-ant", "claude-sonnet-4-5")
                .with_decision_fallback(true),
        ),
        (
            "chatgpt_default",
            ProviderConfig::chatgpt("gpt-5.4").with_chatgpt_auth_storage_path(&chatgpt_auth_path),
        ),
        (
            "chatgpt_fallback",
            ProviderConfig::chatgpt("gpt-5.4")
                .with_chatgpt_auth_storage_path(&chatgpt_auth_path)
                .with_decision_fallback(true),
        ),
    ];

    for (label, config) in cases {
        let expected_supports = config.supports_decisions();
        let expected_calibration = config.decision_calibration();
        let provider = create_provider(config).expect(label);
        assert_eq!(
            provider.supports_decisions(),
            expected_supports,
            "supports_decisions mismatch for {label}"
        );
        assert_eq!(
            provider.decision_calibration(),
            expected_calibration,
            "decision_calibration mismatch for {label}"
        );
        assert_eq!(
            provider.supports_choice_evaluation(),
            expected_supports,
            "supports_choice_evaluation mismatch for {label}"
        );
    }
}
