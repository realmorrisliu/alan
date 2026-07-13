use alan_agent_protocol::ReasoningEffort;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

// The shared OpenAi prefix is intentional here: this enum distinguishes
// OpenAI API families, not unrelated providers.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogProvider {
    OpenAiResponses,
    OpenAiChatCompletions,
    OpenAiChatCompletionsCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub slug: String,
    pub aliases: Vec<String>,
    pub provider: ModelCatalogProvider,
    pub family: String,
    pub context_window_tokens: u32,
    pub supports_reasoning: bool,
    pub supported_reasoning_efforts: Vec<ReasoningEffort>,
    pub default_reasoning_effort: Option<ReasoningEffort>,
    pub effort_budget_tokens: BTreeMap<ReasoningEffort, u32>,
}

#[derive(Debug, Clone)]
pub struct ModelCatalog {
    openai_responses: ProviderCatalog,
    openai_chat_completions: ProviderCatalog,
    openai_chat_completions_compatible: ProviderCatalog,
}

#[derive(Debug, Clone)]
struct ProviderCatalog {
    default_model: String,
    entries: Vec<CatalogEntry>,
}

#[derive(Debug, Clone)]
struct CatalogEntry {
    info: ModelInfo,
    accepts_date_suffixes: bool,
}

#[derive(Debug, Deserialize)]
struct ModelCatalogToml {
    openai_responses: ProviderCatalogToml,
    openai_chat_completions: ProviderCatalogToml,
    openai_chat_completions_compatible: ProviderCatalogToml,
}

#[derive(Debug, Deserialize)]
struct ProviderCatalogToml {
    defaults: ProviderDefaultsToml,
    models: Vec<ModelInfoToml>,
}

#[derive(Debug, Deserialize)]
struct ProviderDefaultsToml {
    model: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelInfoToml {
    slug: String,
    #[serde(default)]
    aliases: Vec<String>,
    family: String,
    context_window_tokens: u32,
    #[serde(default)]
    supports_reasoning: bool,
    #[serde(default)]
    supported_reasoning_efforts: Vec<ReasoningEffort>,
    #[serde(default)]
    default_reasoning_effort: Option<ReasoningEffort>,
    #[serde(default)]
    effort_budget_tokens: BTreeMap<ReasoningEffort, u32>,
    #[serde(default)]
    accepts_date_suffixes: bool,
}

static BASE_MODEL_CATALOG: LazyLock<ModelCatalog> = LazyLock::new(|| {
    let catalog: ModelCatalogToml = toml::from_str(include_str!("../models/catalog.toml"))
        .expect("runtime model catalog TOML should parse");

    ModelCatalog {
        openai_responses: ProviderCatalog::from_toml(
            ModelCatalogProvider::OpenAiResponses,
            catalog.openai_responses,
        ),
        openai_chat_completions: ProviderCatalog::from_toml(
            ModelCatalogProvider::OpenAiChatCompletions,
            catalog.openai_chat_completions,
        ),
        openai_chat_completions_compatible: ProviderCatalog::from_toml(
            ModelCatalogProvider::OpenAiChatCompletionsCompatible,
            catalog.openai_chat_completions_compatible,
        ),
    }
});

pub fn base_catalog() -> &'static ModelCatalog {
    &BASE_MODEL_CATALOG
}

pub fn default_model_slug(provider: ModelCatalogProvider) -> &'static str {
    base_catalog().default_model_slug(provider)
}

impl ModelCatalog {
    pub fn default_model_slug(&self, provider: ModelCatalogProvider) -> &str {
        self.provider_catalog(provider).default_model.as_str()
    }

    pub fn find_model_info(
        &self,
        provider: ModelCatalogProvider,
        model: &str,
    ) -> Option<&ModelInfo> {
        let normalized = normalize_model_id(model);
        self.provider_catalog(provider)
            .entries
            .iter()
            .find(|entry| entry.matches(&normalized))
            .map(|entry| &entry.info)
    }

    pub fn supported_model_slugs(&self, provider: ModelCatalogProvider) -> Vec<&str> {
        self.provider_catalog(provider)
            .entries
            .iter()
            .map(|entry| entry.info.slug.as_str())
            .collect()
    }

    fn provider_catalog(&self, provider: ModelCatalogProvider) -> &ProviderCatalog {
        match provider {
            ModelCatalogProvider::OpenAiResponses => &self.openai_responses,
            ModelCatalogProvider::OpenAiChatCompletions => &self.openai_chat_completions,
            ModelCatalogProvider::OpenAiChatCompletionsCompatible => {
                &self.openai_chat_completions_compatible
            }
        }
    }
}

impl ProviderCatalog {
    fn from_toml(provider: ModelCatalogProvider, raw: ProviderCatalogToml) -> Self {
        let entries = raw
            .models
            .into_iter()
            .map(|model| CatalogEntry::from_toml(provider, model))
            .collect::<Vec<_>>();

        let catalog = Self {
            default_model: raw.defaults.model,
            entries,
        };
        catalog
            .validate()
            .expect("bundled runtime model catalog is valid");
        catalog
    }

    fn validate(&self) -> anyhow::Result<()> {
        if !self
            .entries
            .iter()
            .any(|entry| entry.info.slug == self.default_model)
        {
            anyhow::bail!(
                "default model `{}` must exist in resolved model catalog",
                self.default_model
            );
        }

        let mut seen = HashSet::new();
        for entry in &self.entries {
            entry.validate_reasoning_metadata()?;
            for name in std::iter::once(entry.info.slug.as_str())
                .chain(entry.info.aliases.iter().map(String::as_str))
            {
                let normalized = normalize_model_id(name);
                if !seen.insert(normalized.clone()) {
                    anyhow::bail!(
                        "duplicate model slug or alias `{}` in resolved model catalog",
                        normalized
                    );
                }
            }
        }

        Ok(())
    }
}

impl CatalogEntry {
    fn from_toml(provider: ModelCatalogProvider, raw: ModelInfoToml) -> Self {
        let supported_reasoning_efforts = resolved_supported_reasoning_efforts(
            raw.supports_reasoning,
            raw.supported_reasoning_efforts,
        );
        let supports_reasoning = raw.supports_reasoning || !supported_reasoning_efforts.is_empty();
        let default_reasoning_effort = if supports_reasoning {
            raw.default_reasoning_effort
                .or_else(|| derived_default_reasoning_effort(&supported_reasoning_efforts))
        } else {
            None
        };

        Self {
            accepts_date_suffixes: raw.accepts_date_suffixes,
            info: ModelInfo {
                slug: raw.slug,
                aliases: raw.aliases,
                provider,
                family: raw.family,
                context_window_tokens: raw.context_window_tokens,
                supports_reasoning,
                supported_reasoning_efforts,
                default_reasoning_effort,
                effort_budget_tokens: raw.effort_budget_tokens,
            },
        }
    }

    fn validate_reasoning_metadata(&self) -> anyhow::Result<()> {
        let info = &self.info;
        if !info.supports_reasoning {
            if !info.supported_reasoning_efforts.is_empty() {
                anyhow::bail!(
                    "model `{}` declares supported reasoning efforts while supports_reasoning is false",
                    info.slug
                );
            }
            if info.default_reasoning_effort.is_some() {
                anyhow::bail!(
                    "model `{}` declares default reasoning effort while supports_reasoning is false",
                    info.slug
                );
            }
        }

        let mut seen = HashSet::new();
        for effort in &info.supported_reasoning_efforts {
            if !seen.insert(*effort) {
                anyhow::bail!(
                    "model `{}` declares duplicate reasoning effort `{}`",
                    info.slug,
                    effort
                );
            }
        }

        if let Some(default) = info.default_reasoning_effort
            && !info.supported_reasoning_efforts.contains(&default)
        {
            anyhow::bail!(
                "model `{}` default_reasoning_effort `{}` must appear in supported_reasoning_efforts",
                info.slug,
                default
            );
        }

        for effort in info.effort_budget_tokens.keys() {
            if !info.supported_reasoning_efforts.contains(effort) {
                anyhow::bail!(
                    "model `{}` effort_budget_tokens contains unsupported reasoning effort `{}`",
                    info.slug,
                    effort
                );
            }
        }

        Ok(())
    }

    fn matches(&self, candidate: &str) -> bool {
        self.matches_alias(candidate, &self.info.slug)
            || self
                .info
                .aliases
                .iter()
                .any(|alias| self.matches_alias(candidate, alias))
    }

    fn matches_alias(&self, candidate: &str, alias: &str) -> bool {
        let normalized_alias = normalize_model_id(alias);
        candidate == normalized_alias
            || (self.accepts_date_suffixes
                && candidate
                    .strip_prefix(normalized_alias.as_str())
                    .is_some_and(is_supported_snapshot_suffix))
    }
}

fn resolved_supported_reasoning_efforts(
    supports_reasoning: bool,
    configured: Vec<ReasoningEffort>,
) -> Vec<ReasoningEffort> {
    if !configured.is_empty() || !supports_reasoning {
        return configured;
    }

    vec![
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
    ]
}

fn derived_default_reasoning_effort(
    supported_reasoning_efforts: &[ReasoningEffort],
) -> Option<ReasoningEffort> {
    if supported_reasoning_efforts.contains(&ReasoningEffort::Medium) {
        Some(ReasoningEffort::Medium)
    } else {
        supported_reasoning_efforts.first().copied()
    }
}

fn normalize_model_id(model: &str) -> String {
    model.trim().to_ascii_lowercase()
}

fn is_supported_snapshot_suffix(suffix: &str) -> bool {
    let Some(snapshot) = suffix.strip_prefix('-') else {
        return false;
    };

    is_compact_date_snapshot(snapshot) || is_iso_date_snapshot(snapshot)
}

fn is_compact_date_snapshot(snapshot: &str) -> bool {
    snapshot.len() == 8 && snapshot.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_iso_date_snapshot(snapshot: &str) -> bool {
    let mut parts = snapshot.split('-');
    let Some(year) = parts.next() else {
        return false;
    };
    let Some(month) = parts.next() else {
        return false;
    };
    let Some(day) = parts.next() else {
        return false;
    };

    parts.next().is_none()
        && year.len() == 4
        && month.len() == 2
        && day.len() == 2
        && [year, month, day]
            .into_iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_exposes_default_models() {
        assert_eq!(
            default_model_slug(ModelCatalogProvider::OpenAiResponses),
            "gpt-5.4"
        );
        assert_eq!(
            default_model_slug(ModelCatalogProvider::OpenAiChatCompletions),
            "gpt-5.4"
        );
        assert_eq!(
            default_model_slug(ModelCatalogProvider::OpenAiChatCompletionsCompatible),
            "qwen3.5-plus"
        );
    }

    #[test]
    fn finds_exact_canonical_slug() {
        let kimi = base_catalog()
            .find_model_info(
                ModelCatalogProvider::OpenAiChatCompletionsCompatible,
                "kimi-k2.5",
            )
            .unwrap();
        assert_eq!(kimi.slug, "kimi-k2.5");
        assert_eq!(kimi.context_window_tokens, 250_000);
    }

    #[test]
    fn finds_date_snapshot_aliases() {
        let qwen = base_catalog()
            .find_model_info(
                ModelCatalogProvider::OpenAiChatCompletionsCompatible,
                "bailian/qwen3.5-plus-2026-02-15",
            )
            .unwrap();
        assert_eq!(qwen.slug, "qwen3.5-plus");
        assert_eq!(qwen.family, "qwen3.5");
    }

    #[test]
    fn rejects_non_snapshot_suffix_variants() {
        assert!(
            base_catalog()
                .find_model_info(
                    ModelCatalogProvider::OpenAiChatCompletionsCompatible,
                    "kimi-k2.5-thinking",
                )
                .is_none()
        );
    }

    #[test]
    fn supports_reasoning_derives_compatible_reasoning_effort_metadata() {
        let gpt = base_catalog()
            .find_model_info(ModelCatalogProvider::OpenAiResponses, "gpt-5.4")
            .unwrap();

        assert!(gpt.supports_reasoning);
        assert_eq!(
            gpt.supported_reasoning_efforts,
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High
            ]
        );
        assert_eq!(gpt.default_reasoning_effort, Some(ReasoningEffort::Medium));
    }
}
