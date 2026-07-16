//! Provider identity, capability, and model introspection documents.

use alan_llm::{CompatibilityTier, ProviderCapabilities, factory::ProviderType};

use super::render_json_doc;

pub(super) fn known_provider_names() -> Vec<String> {
    let mut names = vec![
        "anthropic_messages".to_string(),
        "chatgpt".to_string(),
        "google_gemini_generate_content".to_string(),
        "openai_chat_completions".to_string(),
        "openai_chat_completions_compatible".to_string(),
        "openai_responses".to_string(),
        "openrouter".to_string(),
    ];
    names.sort();
    names
}

pub(super) fn is_known_provider(name: &str) -> bool {
    provider_type_for_name(name).is_some()
}

fn provider_type_for_name(name: &str) -> Option<ProviderType> {
    match name {
        "google_gemini_generate_content" => Some(ProviderType::GoogleGeminiGenerateContent),
        "chatgpt" => Some(ProviderType::ChatgptResponses),
        // Test providers report `mock`, but the engine-backed smoke path uses
        // them as Responses-compatible connections.
        "mock" => Some(ProviderType::OpenAiResponses),
        "openai_responses" => Some(ProviderType::OpenAiResponses),
        "openai_chat_completions" => Some(ProviderType::OpenAiChatCompletions),
        "openai_chat_completions_compatible" => Some(ProviderType::OpenAiChatCompletionsCompatible),
        "openrouter" => Some(ProviderType::OpenRouter),
        "anthropic_messages" => Some(ProviderType::AnthropicMessages),
        _ => None,
    }
}

pub(super) fn provider_capabilities_for_name(name: &str) -> ProviderCapabilities {
    provider_type_for_name(name)
        .map(ProviderType::capabilities)
        .unwrap_or_else(unknown_provider_capabilities)
}

fn unknown_provider_capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        supports_streaming_text: true,
        supports_streaming_tool_calls: false,
        supports_provider_response_id: false,
        supports_provider_response_status: false,
        supports_reasoning_text: false,
        supports_reasoning_signature: false,
        supports_reasoning_effort_control: false,
        supports_redacted_thinking: false,
        supports_multimodal_input: false,
        supports_document_input: false,
        supports_cached_token_usage: false,
        supports_server_managed_continuation: false,
        supports_background_execution: false,
        supports_retrieve_cancel: false,
        supports_provider_compaction: false,
        instruction_role: alan_llm::InstructionRole::System,
        compatibility_tier: CompatibilityTier::TierCBestEffortCompatible,
    }
}

pub(super) fn provider_capabilities_doc(
    provider: &str,
    capabilities: ProviderCapabilities,
) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "provider": provider,
        "capabilities": capabilities,
    }))
}

pub(super) fn connection_capabilities_doc(
    connection: &str,
    provider: &str,
    capabilities: ProviderCapabilities,
) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "connection": connection,
        "provider": provider,
        "capabilities": capabilities,
    }))
}

pub(super) fn connection_profile_doc(
    connection: &str,
    provider: &str,
    model: Option<&str>,
    credential_ref: Option<&str>,
) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "connection": connection,
        "provider": provider,
        "model": model,
        "credential_ref": credential_ref,
    }))
}

pub(super) fn provider_models_doc(provider: &str) -> String {
    let catalog = provider_model_catalog(provider);
    let models = catalog
        .map(|catalog| {
            catalog
                .models
                .iter()
                .map(|model| {
                    serde_json::json!({
                        "slug": model.slug,
                        "family": model.family,
                        "context_window_tokens": model.context_window_tokens,
                        "supports_reasoning": model.supports_reasoning,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    render_json_doc(serde_json::json!({
        "version": 1,
        "provider": provider,
        "source": catalog.map(|catalog| catalog.source).unwrap_or("unknown"),
        "default_model": catalog.map(|catalog| catalog.default_model),
        "models": models,
    }))
}

pub(super) fn provider_status_doc(provider: &str) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "provider": provider,
        "status": "available",
        "callable": false,
        "has_model_catalog": provider_model_catalog(provider).is_some(),
    }))
}

#[derive(Clone, Copy)]
struct ProviderModel {
    slug: &'static str,
    family: &'static str,
    context_window_tokens: u32,
    supports_reasoning: bool,
}

#[derive(Clone, Copy)]
struct ProviderModelCatalog {
    default_model: &'static str,
    source: &'static str,
    models: &'static [ProviderModel],
}

const OPENAI_GPT_MODELS: &[ProviderModel] = &[
    ProviderModel {
        slug: "gpt-5.4",
        family: "gpt-5",
        context_window_tokens: 1_050_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5.4-pro",
        family: "gpt-5",
        context_window_tokens: 1_050_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5.2",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5.2-pro",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5.1",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5-mini",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5-nano",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-oss-120b",
        family: "gpt-oss",
        context_window_tokens: 131_072,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-oss-20b",
        family: "gpt-oss",
        context_window_tokens: 131_072,
        supports_reasoning: true,
    },
];

const OPENAI_COMPATIBLE_MODELS: &[ProviderModel] = &[
    ProviderModel {
        slug: "qwen3.5-plus",
        family: "qwen3.5",
        context_window_tokens: 1_000_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "minimax-m2.5",
        family: "minimax-m2.5",
        context_window_tokens: 204_800,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "minimax-m2.5-highspeed",
        family: "minimax-m2.5",
        context_window_tokens: 204_800,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "glm-5",
        family: "glm-5",
        context_window_tokens: 200_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "kimi-k2.5",
        family: "kimi-k2.5",
        context_window_tokens: 250_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "deepseek-chat",
        family: "deepseek-v3",
        context_window_tokens: 128_000,
        supports_reasoning: false,
    },
    ProviderModel {
        slug: "deepseek-reasoner",
        family: "deepseek-r1",
        context_window_tokens: 128_000,
        supports_reasoning: true,
    },
];

const CHATGPT_MODELS: &[ProviderModel] = &[ProviderModel {
    slug: "gpt-5.3-codex",
    family: "gpt-5-codex",
    context_window_tokens: 400_000,
    supports_reasoning: true,
}];

const GEMINI_MODELS: &[ProviderModel] = &[ProviderModel {
    slug: "gemini-2.0-flash",
    family: "gemini-2.0",
    context_window_tokens: 1_048_576,
    supports_reasoning: false,
}];

const ANTHROPIC_MODELS: &[ProviderModel] = &[ProviderModel {
    slug: "claude-3-5-sonnet-latest",
    family: "claude-3.5-sonnet",
    context_window_tokens: 200_000,
    supports_reasoning: false,
}];

const OPENROUTER_MODELS: &[ProviderModel] = &[ProviderModel {
    slug: "moonshotai/kimi-k2.6",
    family: "kimi-k2.6",
    context_window_tokens: 256_000,
    supports_reasoning: true,
}];

fn provider_model_catalog(provider: &str) -> Option<ProviderModelCatalog> {
    match provider {
        "openai_responses" => Some(ProviderModelCatalog {
            default_model: "gpt-5.4",
            source: "bundled-openai-responses",
            models: OPENAI_GPT_MODELS,
        }),
        "openai_chat_completions" => Some(ProviderModelCatalog {
            default_model: "gpt-5.4",
            source: "bundled-openai-chat-completions",
            models: OPENAI_GPT_MODELS,
        }),
        "openai_chat_completions_compatible" => Some(ProviderModelCatalog {
            default_model: "qwen3.5-plus",
            source: "bundled-openai-compatible",
            models: OPENAI_COMPATIBLE_MODELS,
        }),
        "chatgpt" => Some(ProviderModelCatalog {
            default_model: "gpt-5.3-codex",
            source: "bundled-chatgpt",
            models: CHATGPT_MODELS,
        }),
        "google_gemini_generate_content" => Some(ProviderModelCatalog {
            default_model: "gemini-2.0-flash",
            source: "bundled-gemini",
            models: GEMINI_MODELS,
        }),
        "anthropic_messages" => Some(ProviderModelCatalog {
            default_model: "claude-3-5-sonnet-latest",
            source: "bundled-anthropic",
            models: ANTHROPIC_MODELS,
        }),
        "openrouter" => Some(ProviderModelCatalog {
            default_model: "moonshotai/kimi-k2.6",
            source: "bundled-openrouter",
            models: OPENROUTER_MODELS,
        }),
        _ => None,
    }
}
