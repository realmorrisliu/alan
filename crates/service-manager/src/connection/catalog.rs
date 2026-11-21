//! Connection-owned model catalog discovery, static provider catalogs, and model authorization.

use super::{ConnectionService, provider_packages};
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
        LlmProvider::Chatgpt
            | LlmProvider::Magpie
            | LlmProvider::OpenRouter
            | LlmProvider::GoogleGeminiGenerateContent
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
        LlmProvider::GoogleGeminiGenerateContent if !models.is_empty() => {
            let engine_catalog = base_catalog
                .with_google_gemini_models(models.clone())
                .ok()?;
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

impl ConnectionService {
    /// Discover a live provider model catalog off-lock and publish it atomically to
    /// both `published_catalogs` and `llmfs` (`/mnt/llm/connections/<profile>/models`).
    pub(super) async fn discover_catalog(
        &self,
        profile_id: &str,
        provider_kind: LlmProvider,
        publication: &ModelCatalogPublication,
    ) {
        let discovery = async {
            let (client, base_catalog) = {
                let callables = self.callables.lock().await;
                let registry = callables
                    .as_ref()
                    .context("callable registry unavailable")?;
                let connections = self.metadata();
                ensure!(
                    registry
                        .published_catalogs
                        .get(profile_id)
                        .is_some_and(|current| Arc::ptr_eq(current, publication)),
                    "catalog publication replaced"
                );
                ensure!(
                    registry.published_profiles.get(profile_id)
                        == connections.profiles.get(profile_id)
                        && registry.published_profiles.contains_key(profile_id),
                    "profile publication changed"
                );
                let packages = self.package_service();
                let hydrated = provider_packages::hydrate_connections_with_package_defaults(
                    packages.as_deref(),
                    &connections,
                    profile_id,
                );
                let client =
                    registry
                        .factory
                        .create(&registry.base_config, Some(profile_id), &hydrated)?;
                if provider_kind == LlmProvider::Chatgpt {
                    let account = registry
                        .published_accounts
                        .get(profile_id)
                        .context("managed catalog account unavailable")?;
                    ensure!(
                        client.account_identity() == Some(account.as_str()),
                        "provider account changed since callable publication"
                    );
                } else if let Some(account) = registry.published_accounts.get(profile_id) {
                    ensure!(
                        client.account_identity() == Some(account.as_str()),
                        "provider account changed since callable publication"
                    );
                }
                (
                    client,
                    registry.base_config.resolved_model_catalog().clone(),
                )
            };
            // Never hold the global callable lock across provider IO. The publication
            // slot is detached on replacement, so late results cannot authorize it.
            let models = client
                .model_catalog()
                .await?
                .context("model catalog unavailable")?;
            build_published_catalog(provider_kind, &base_catalog, models)
                .context("model catalog unavailable")
        }
        .await;

        match discovery {
            Ok(published) => {
                let callables = self.callables.lock().await;
                let still_current = callables.as_ref().is_some_and(|registry| {
                    registry
                        .published_catalogs
                        .get(profile_id)
                        .is_some_and(|current| Arc::ptr_eq(current, publication))
                });
                if still_current {
                    if let Some(registry) = callables.as_ref() {
                        registry
                            .llmfs
                            .set_connection_models(profile_id, published.models.clone());
                    }
                    let _ = publication.set(Some(published));
                } else {
                    let _ = publication.set(None);
                }
            }
            Err(_) => {
                // A settled failure is cached too. Only explicit selection retries it.
                let _ = publication.set(None);
            }
        }
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

fn published_engine_catalog_provider(provider: LlmProvider) -> Option<ModelCatalogProvider> {
    match provider {
        LlmProvider::Chatgpt => Some(ModelCatalogProvider::Chatgpt),
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
        if let Some(published) = published {
            if let Some(engine_catalog) = &published.engine_catalog
                && let Some(provider) = published_engine_catalog_provider(config.llm_provider)
            {
                let entries = serialize_engine_catalog_entries(engine_catalog, provider);
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
        // Vertex AI falls back gracefully to the curated baseline Gemini catalog when
        // live discovery is unavailable (Express Mode, IAM 403, or offline startup).
        if config.llm_provider != LlmProvider::GoogleGeminiGenerateContent {
            anyhow::bail!("model catalog unavailable");
        }
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
    let entries = serialize_engine_catalog_entries(catalog, provider);
    ensure!(!entries.is_empty(), "model catalog unavailable");
    Ok(entries)
}

fn serialize_engine_catalog_entries(
    catalog: &ModelCatalog,
    provider: ModelCatalogProvider,
) -> Vec<serde_json::Value> {
    catalog
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
        .collect()
}

/// Check whether `requested_model` is authorized by either the engine catalog,
/// Connection-owned live/static catalogs, or explicit official `gemini-*` Vertex slugs.
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
    if config.llm_provider == LlmProvider::GoogleGeminiGenerateContent
        && alan_llm::gemini_catalog::is_valid_gemini_publisher_slug(requested_model)
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
