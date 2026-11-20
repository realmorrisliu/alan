//! Connection-owned model catalog discovery, static provider catalogs, and model authorization.

use alan_agent_engine::{Config, LlmProvider, ModelCatalog, ModelCatalogProvider};
use alan_llm::{ProviderModel, ReasoningEffort};
use anyhow::{Context, Result, ensure};
use std::sync::{Arc, OnceLock};

/// Published catalog payload cached per Connection profile revision.
#[derive(Clone)]
pub(crate) struct PublishedCatalog {
    pub models: Vec<ProviderModel>,
    pub engine_catalog: Option<Arc<ModelCatalog>>,
}

pub(crate) type ModelCatalogPublication = Arc<OnceLock<Option<PublishedCatalog>>>;

/// Providers that publish live `/models` catalogs from an active client connection.
pub(crate) fn needs_live_catalog_discovery(provider: LlmProvider) -> bool {
    matches!(
        provider,
        LlmProvider::Chatgpt | LlmProvider::Magpie | LlmProvider::OpenRouter
    )
}

/// Convert live discovered models into a Connection `PublishedCatalog`.
pub(crate) fn build_published_catalog(
    provider: LlmProvider,
    base_catalog: &ModelCatalog,
    models: Vec<ProviderModel>,
) -> Option<PublishedCatalog> {
    match provider {
        LlmProvider::Chatgpt => {
            let engine_catalog = base_catalog.with_chatgpt_models(models.clone()).ok()?;
            Some(PublishedCatalog {
                models,
                engine_catalog: Some(Arc::new(engine_catalog)),
            })
        }
        LlmProvider::Magpie | LlmProvider::OpenRouter if !models.is_empty() => {
            Some(PublishedCatalog {
                models,
                engine_catalog: None,
            })
        }
        _ => None,
    }
}

/// Static catalog models for Anthropic Messages API profiles.
pub(crate) fn anthropic_static_models() -> Vec<ProviderModel> {
    let standard_efforts = vec![
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
    ];
    vec![
        ProviderModel {
            slug: "claude-opus-4-6".to_string(),
            context_window_tokens: 200_000,
            supported_reasoning_efforts: standard_efforts.clone(),
            default_reasoning_effort: Some(ReasoningEffort::Medium),
        },
        ProviderModel {
            slug: "claude-sonnet-4-6".to_string(),
            context_window_tokens: 200_000,
            supported_reasoning_efforts: standard_efforts.clone(),
            default_reasoning_effort: Some(ReasoningEffort::Medium),
        },
        ProviderModel {
            slug: "claude-sonnet-4-5".to_string(),
            context_window_tokens: 200_000,
            supported_reasoning_efforts: standard_efforts.clone(),
            default_reasoning_effort: None,
        },
        ProviderModel {
            slug: "claude-haiku-4-5".to_string(),
            context_window_tokens: 200_000,
            supported_reasoning_efforts: standard_efforts,
            default_reasoning_effort: None,
        },
    ]
}

fn static_model_catalog_provider(provider: LlmProvider) -> Option<ModelCatalogProvider> {
    match provider {
        LlmProvider::OpenAiResponses => Some(ModelCatalogProvider::OpenAiResponses),
        LlmProvider::OpenAiChatCompletions => Some(ModelCatalogProvider::OpenAiChatCompletions),
        LlmProvider::OpenAiChatCompletionsCompatible => {
            Some(ModelCatalogProvider::OpenAiChatCompletionsCompatible)
        }
        LlmProvider::GoogleGeminiGenerateContent => {
            Some(ModelCatalogProvider::GoogleGeminiGenerateContent)
        }
        _ => None,
    }
}

/// Serialize UI model catalog entries (`model`, `supported_reasoning_efforts`,
/// `default_reasoning_effort`) from live discovery or static provider catalogs.
pub(crate) fn catalog_entries_for_config(
    config: &Config,
    published: Option<&PublishedCatalog>,
) -> Result<Vec<serde_json::Value>> {
    if needs_live_catalog_discovery(config.llm_provider) {
        let Some(published) = published else {
            anyhow::bail!("model catalog unavailable");
        };
        if let Some(engine_catalog) = &published.engine_catalog {
            let provider = ModelCatalogProvider::Chatgpt;
            let entries: Vec<_> = engine_catalog
                .supported_model_slugs(provider)
                .into_iter()
                .filter_map(|slug| {
                    engine_catalog.find_model_info(provider, slug).map(|entry| {
                        serde_json::json!({
                            "model": entry.slug,
                            "supported_reasoning_efforts": entry.supported_reasoning_efforts,
                            "default_reasoning_effort": entry.default_reasoning_effort,
                        })
                    })
                })
                .collect();
            ensure!(!entries.is_empty(), "model catalog unavailable");
            return Ok(entries);
        }
        let entries: Vec<_> = published
            .models
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "model": entry.slug,
                    "supported_reasoning_efforts": entry.supported_reasoning_efforts,
                    "default_reasoning_effort": entry.default_reasoning_effort,
                })
            })
            .collect();
        ensure!(!entries.is_empty(), "model catalog unavailable");
        return Ok(entries);
    }

    if config.llm_provider == LlmProvider::AnthropicMessages {
        let entries: Vec<_> = anthropic_static_models()
            .into_iter()
            .map(|entry| {
                serde_json::json!({
                    "model": entry.slug,
                    "supported_reasoning_efforts": entry.supported_reasoning_efforts,
                    "default_reasoning_effort": entry.default_reasoning_effort,
                })
            })
            .collect();
        return Ok(entries);
    }

    let provider = config
        .effective_model_info()
        .map(|info| info.provider)
        .or_else(|| static_model_catalog_provider(config.llm_provider))
        .context("model catalog unavailable")?;
    let catalog = config.resolved_model_catalog();
    let entries: Vec<_> = catalog
        .supported_model_slugs(provider)
        .into_iter()
        .filter_map(|slug| {
            catalog.find_model_info(provider, slug).map(|entry| {
                serde_json::json!({
                    "model": entry.slug,
                    "supported_reasoning_efforts": entry.supported_reasoning_efforts,
                    "default_reasoning_effort": entry.default_reasoning_effort,
                })
            })
        })
        .collect();
    ensure!(!entries.is_empty(), "model catalog unavailable");
    Ok(entries)
}

/// Check whether `requested_model` is authorized by either the engine catalog or
/// Connection-owned live/static catalogs.
pub(crate) fn is_model_authorized(
    config: &Config,
    requested_model: &str,
    published: Option<&PublishedCatalog>,
) -> bool {
    if config.effective_model_info().is_some() {
        return true;
    }
    if let Some(published) = published
        && published.models.iter().any(|m| m.slug == requested_model)
    {
        return true;
    }
    if config.llm_provider == LlmProvider::AnthropicMessages {
        return anthropic_static_models()
            .iter()
            .any(|m| m.slug == requested_model);
    }
    false
}
