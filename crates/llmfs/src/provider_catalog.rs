//! Provider identity, capability, and model introspection documents.

use alan_llm::{
    CompatibilityTier, ProviderCapabilities, ProviderModel as LiveProviderModel,
    factory::ProviderType, gemini_catalog,
};

use super::render_json_doc;

pub(super) fn known_provider_names() -> Vec<String> {
    let mut names = vec![
        "anthropic_messages".to_string(),
        "chatgpt".to_string(),
        "google_gemini_generate_content".to_string(),
        "magpie".to_string(),
        "openai_chat_completions".to_string(),
        "openai_chat_completions_compatible".to_string(),
        "openai_responses".to_string(),
        "openrouter".to_string(),
        "typesafe".to_string(),
    ];
    names.sort();
    names
}

pub(super) fn is_known_provider(name: &str) -> bool {
    provider_type_for_name(name).is_some()
}

fn provider_type_for_name(name: &str) -> Option<ProviderType> {
    match name {
        "magpie" => Some(ProviderType::Magpie),
        "google_gemini_generate_content" => Some(ProviderType::GoogleGeminiGenerateContent),
        "chatgpt" => Some(ProviderType::ChatgptResponses),
        // Test providers report `mock`, but the engine-backed smoke path uses
        // them as Responses-compatible connections.
        "mock" => Some(ProviderType::OpenAiResponses),
        "openai_responses" => Some(ProviderType::OpenAiResponses),
        "openai_chat_completions" => Some(ProviderType::OpenAiChatCompletions),
        "openai_chat_completions_compatible" => Some(ProviderType::OpenAiChatCompletionsCompatible),
        "openrouter" => Some(ProviderType::OpenRouter),
        "typesafe" => Some(ProviderType::TypesafeEvaluation),
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
        "capabilities": (provider != "typesafe").then_some(capabilities),
        "generation": provider != "typesafe",
        "evaluation": {"choice_v1": provider == "typesafe"},
    }))
}

pub(super) fn connection_capabilities_doc(
    connection: &str,
    provider: &str,
    capabilities: ProviderCapabilities,
    choice_evaluation: bool,
    generation: bool,
) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "connection": connection,
        "provider": provider,
        // These are generation features; evaluation-only callables have none.
        "capabilities": generation.then_some(capabilities),
        "evaluation": { "choice_v1": choice_evaluation },
        "generation": generation,
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

pub(super) fn connection_models_doc(
    connection: &str,
    provider: &str,
    live_models: Option<&[LiveProviderModel]>,
) -> String {
    let (source, models) = if let Some(live_models) = live_models {
        (
            "live",
            live_models.iter().map(live_model_json).collect::<Vec<_>>(),
        )
    } else if provider == "google_gemini_generate_content" {
        ("bundled-gemini", gemini_baseline_models_json())
    } else if let Some(catalog) = provider_model_catalog(provider) {
        (catalog.source, static_models_json(catalog.models))
    } else {
        ("unavailable", Vec::new())
    };
    render_json_doc(serde_json::json!({
        "version": 1,
        "connection": connection,
        "provider": provider,
        "source": source,
        "models": models,
    }))
}

pub(super) fn provider_models_doc(provider: &str) -> String {
    if provider == "google_gemini_generate_content" {
        return render_json_doc(serde_json::json!({
            "version": 1,
            "provider": provider,
            "source": "bundled-gemini",
            "default_model": gemini_catalog::GEMINI_DEFAULT_MODEL,
            "models": gemini_baseline_models_json(),
        }));
    }
    let catalog = provider_model_catalog(provider);
    let models = catalog
        .map(|catalog| static_models_json(catalog.models))
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
        "has_model_catalog": provider == "google_gemini_generate_content"
            || provider_model_catalog(provider).is_some(),
    }))
}

fn live_model_json(model: &LiveProviderModel) -> serde_json::Value {
    serde_json::json!({
        "slug": model.slug,
        "context_window_tokens": model.context_window_tokens,
        "supports_reasoning": !model.supported_reasoning_efforts.is_empty(),
        "supported_reasoning_efforts": model.supported_reasoning_efforts,
        "default_reasoning_effort": model.default_reasoning_effort,
    })
}

fn gemini_baseline_models_json() -> Vec<serde_json::Value> {
    gemini_catalog::vertex_baseline_catalog()
        .into_iter()
        .map(|model| {
            serde_json::json!({
                "slug": model.slug,
                "family": gemini_catalog::gemini_model_family(&model.slug),
                "context_window_tokens": model.context_window_tokens,
                "supports_reasoning": !model.supported_reasoning_efforts.is_empty(),
                "supported_reasoning_efforts": model.supported_reasoning_efforts,
                "default_reasoning_effort": model.default_reasoning_effort,
            })
        })
        .collect()
}

fn static_models_json(models: &[StaticProviderModel]) -> Vec<serde_json::Value> {
    models
        .iter()
        .map(|model| {
            serde_json::json!({
                "slug": model.slug,
                "family": model.family,
                "context_window_tokens": model.context_window_tokens,
                "supports_reasoning": model.supports_reasoning,
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
struct StaticProviderModel {
    slug: &'static str,
    family: &'static str,
    context_window_tokens: u32,
    supports_reasoning: bool,
}

#[derive(Clone, Copy)]
struct ProviderModelCatalog {
    default_model: &'static str,
    source: &'static str,
    models: &'static [StaticProviderModel],
}

const OPENAI_GPT_MODELS: &[StaticProviderModel] = &[
    StaticProviderModel {
        slug: "gpt-5.4",
        family: "gpt-5",
        context_window_tokens: 1_050_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-5.4-pro",
        family: "gpt-5",
        context_window_tokens: 1_050_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-5.2",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-5.2-pro",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-5.1",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-5-mini",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-5-nano",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-oss-120b",
        family: "gpt-oss",
        context_window_tokens: 131_072,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-oss-20b",
        family: "gpt-oss",
        context_window_tokens: 131_072,
        supports_reasoning: true,
    },
];

const OPENAI_COMPATIBLE_MODELS: &[StaticProviderModel] = &[
    StaticProviderModel {
        slug: "qwen3.5-plus",
        family: "qwen3.5",
        context_window_tokens: 1_000_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "minimax-m2.5",
        family: "minimax-m2.5",
        context_window_tokens: 204_800,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "minimax-m2.5-highspeed",
        family: "minimax-m2.5",
        context_window_tokens: 204_800,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "glm-5",
        family: "glm-5",
        context_window_tokens: 200_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "kimi-k2.5",
        family: "kimi-k2.5",
        context_window_tokens: 250_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "deepseek-chat",
        family: "deepseek-v3",
        context_window_tokens: 128_000,
        supports_reasoning: false,
    },
    StaticProviderModel {
        slug: "deepseek-reasoner",
        family: "deepseek-r1",
        context_window_tokens: 128_000,
        supports_reasoning: true,
    },
];

const CHATGPT_MODELS: &[StaticProviderModel] = &[
    StaticProviderModel {
        slug: "gpt-5.4",
        family: "gpt-5",
        context_window_tokens: 1_050_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-5.4-pro",
        family: "gpt-5",
        context_window_tokens: 1_050_000,
        supports_reasoning: true,
    },
    StaticProviderModel {
        slug: "gpt-5-mini",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
];

const ANTHROPIC_MODELS: &[StaticProviderModel] = &[StaticProviderModel {
    slug: "claude-sonnet-4-5",
    family: "claude-4.5",
    context_window_tokens: 200_000,
    supports_reasoning: true,
}];

const MAGPIE_MODELS: &[StaticProviderModel] = &[StaticProviderModel {
    slug: "default",
    family: "magpie",
    context_window_tokens: 128_000,
    supports_reasoning: true,
}];

const OPENROUTER_MODELS: &[StaticProviderModel] = &[StaticProviderModel {
    slug: "moonshotai/kimi-k2.6",
    family: "kimi-k2.6",
    context_window_tokens: 256_000,
    supports_reasoning: true,
}];

fn provider_model_catalog(provider: &str) -> Option<ProviderModelCatalog> {
    match provider {
        "magpie" => Some(ProviderModelCatalog {
            default_model: "default",
            source: "bundled-magpie",
            models: MAGPIE_MODELS,
        }),
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
            default_model: "gpt-5.4",
            source: "bundled-chatgpt",
            models: CHATGPT_MODELS,
        }),
        "anthropic_messages" => Some(ProviderModelCatalog {
            default_model: "claude-sonnet-4-5",
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
