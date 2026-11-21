//! Connection-owned model catalog discovery, static provider catalogs, and model authorization.

use super::{CallableRegistry, ConnectionService, State, catalog_cache, provider_packages};
use alan_agent_engine::{Config, LlmProvider, ModelCatalog, ModelCatalogProvider};
use alan_llm::{ProviderModel, ReasoningEffort};
use anyhow::{Context, Result, ensure};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Published catalog payload cached per Connection profile revision.
#[derive(Clone)]
pub(crate) struct PublishedCatalog {
    pub models: Vec<ProviderModel>,
    pub engine_catalog: Option<Arc<ModelCatalog>>,
}

#[derive(Clone, Default)]
struct CatalogSlotState {
    published: Option<PublishedCatalog>,
    live_settled: bool,
    live_succeeded: bool,
}

/// Per-profile catalog publication slot supporting warm disk-cache hydration
/// followed by non-blocking background live discovery and pointer-checked replacement.
pub(crate) struct CatalogSlot {
    state: Mutex<CatalogSlotState>,
    settled: tokio::sync::Notify,
}

impl CatalogSlot {
    pub(crate) fn new(initial: Option<PublishedCatalog>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(CatalogSlotState {
                published: initial,
                live_settled: false,
                live_succeeded: false,
            }),
            settled: tokio::sync::Notify::new(),
        })
    }

    pub(crate) fn published(&self) -> Option<PublishedCatalog> {
        self.state.lock().unwrap().published.clone()
    }

    pub(crate) fn is_live_settled(&self) -> bool {
        self.state.lock().unwrap().live_settled
    }

    pub(crate) fn is_live_failed(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.live_settled && !state.live_succeeded
    }

    pub(crate) fn settle_live_ok(&self, catalog: PublishedCatalog) {
        {
            let mut state = self.state.lock().unwrap();
            state.published = Some(catalog);
            state.live_settled = true;
            state.live_succeeded = true;
        }
        self.settled.notify_waiters();
    }

    pub(crate) fn settle_live_failed(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.live_settled = true;
            state.live_succeeded = false;
        }
        self.settled.notify_waiters();
    }

    pub(crate) fn settle_superseded(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.live_settled = true;
            state.live_succeeded = false;
        }
        self.settled.notify_waiters();
    }

    pub(crate) async fn wait_settled(&self) {
        loop {
            let notified = self.settled.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.state.lock().unwrap().live_settled {
                return;
            }
            notified.await;
        }
    }
}

pub(crate) type ModelCatalogPublication = Arc<CatalogSlot>;

/// Detached handle passed to background catalog discovery tasks so they never
/// retain a strong reference to `ConnectionService` (preserving `close_ephemeral`).
#[derive(Clone)]
pub(super) struct CatalogDiscoveryContext {
    pub metadata_path: Option<PathBuf>,
    pub state: Arc<Mutex<State>>,
    pub callables: Arc<tokio::sync::Mutex<Option<CallableRegistry>>>,
    pub package_service: Arc<Mutex<Option<Arc<crate::PackageService>>>>,
    pub catalog_cache_lock: Arc<Mutex<()>>,
}

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
    pub(super) fn discovery_context(&self) -> CatalogDiscoveryContext {
        CatalogDiscoveryContext {
            metadata_path: self.metadata_path.clone(),
            state: self.state.clone(),
            callables: self.callables.clone(),
            package_service: self.package_service.clone(),
            catalog_cache_lock: self.catalog_cache_lock.clone(),
        }
    }

    /// Spawn background live catalog discovery for one profile publication slot.
    pub(super) fn spawn_catalog_discovery(
        &self,
        profile_id: String,
        provider_kind: LlmProvider,
        publication: ModelCatalogPublication,
    ) {
        let ctx = self.discovery_context();
        tokio::spawn(async move {
            discover_catalog_with_context(ctx, &profile_id, provider_kind, &publication).await;
        });
    }

    /// Await settlement of all currently published background catalog discoveries.
    #[cfg(test)]
    pub(crate) async fn wait_for_catalog_discovery(&self) {
        let slots: Vec<ModelCatalogPublication> = {
            let callables = self.callables.lock().await;
            callables
                .as_ref()
                .map(|registry| registry.published_catalogs.values().cloned().collect())
                .unwrap_or_default()
        };
        for slot in slots {
            slot.wait_settled().await;
        }
    }
}

/// Discover a live provider model catalog off-lock and publish it atomically to
/// `published_catalogs`, `llmfs` (`/mnt/llm/connections/<profile>/models`), and disk cache.
pub(super) async fn discover_catalog_with_context(
    ctx: CatalogDiscoveryContext,
    profile_id: &str,
    provider_kind: LlmProvider,
    publication: &ModelCatalogPublication,
) {
    let discovery = async {
        let (client, base_catalog, profile_snapshot, account_identity) = {
            let callables = ctx.callables.lock().await;
            let registry = callables
                .as_ref()
                .context("callable registry unavailable")?;
            let connections = ctx.state.lock().unwrap().connections.clone();
            ensure!(
                registry
                    .published_catalogs
                    .get(profile_id)
                    .is_some_and(|current| Arc::ptr_eq(current, publication)),
                "catalog publication replaced"
            );
            ensure!(
                registry.published_profiles.get(profile_id) == connections.profiles.get(profile_id)
                    && registry.published_profiles.contains_key(profile_id),
                "profile publication changed"
            );
            let profile_snapshot = connections
                .profiles
                .get(profile_id)
                .context("profile unavailable")?
                .clone();
            let packages = ctx
                .package_service
                .lock()
                .expect("package service mutex poisoned")
                .clone();
            let hydrated = provider_packages::hydrate_connections_with_package_defaults(
                packages.as_deref(),
                &connections,
                profile_id,
            );
            let client =
                registry
                    .factory
                    .create(&registry.base_config, Some(profile_id), &hydrated)?;
            let account_identity = registry.published_accounts.get(profile_id).cloned();
            if provider_kind == LlmProvider::Chatgpt {
                let account = account_identity
                    .as_deref()
                    .context("managed catalog account unavailable")?;
                ensure!(
                    client.account_identity() == Some(account),
                    "provider account changed since callable publication"
                );
            } else if let Some(account) = account_identity.as_deref() {
                ensure!(
                    client.account_identity() == Some(account),
                    "provider account changed since callable publication"
                );
            }
            (
                client,
                registry.base_config.resolved_model_catalog().clone(),
                profile_snapshot,
                account_identity,
            )
        };
        // Never hold the global callable lock across provider IO. The publication
        // slot is detached on replacement, so late results cannot authorize it.
        let models = client
            .model_catalog()
            .await?
            .context("model catalog unavailable")?;
        let published = build_published_catalog(provider_kind, &base_catalog, models)
            .context("model catalog unavailable")?;
        Ok::<_, anyhow::Error>((published, profile_snapshot, account_identity))
    }
    .await;

    match discovery {
        Ok((published, profile_snapshot, account_identity)) => {
            let still_current = {
                let callables = ctx.callables.lock().await;
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
                    publication.settle_live_ok(published.clone());
                } else {
                    publication.settle_superseded();
                }
                still_current
            };
            if still_current && let Some(metadata_path) = &ctx.metadata_path {
                catalog_cache::save_cached_models(
                    metadata_path,
                    &ctx.catalog_cache_lock,
                    profile_id,
                    &profile_snapshot,
                    account_identity.as_deref(),
                    &published.models,
                );
            }
        }
        Err(_) => {
            // A settled failure is cached too. Only explicit selection retries it.
            publication.settle_live_failed();
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
