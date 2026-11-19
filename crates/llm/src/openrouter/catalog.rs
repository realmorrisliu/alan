//! Live OpenRouter model catalog discovery (`GET {base_url}/models`).

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};

use super::OpenRouterClient;
use crate::{ProviderModel, ReasoningEffort};

const MAX_CATALOG_BYTES: usize = 32 * 1024 * 1024;
const CATALOG_TIMEOUT: Duration = Duration::from_secs(20);
const CHAT_COMPLETIONS_ENDPOINT: &str = "/api/v1/chat/completions";
/// Efforts OpenRouter normalizes for reasoning models that do not list their own.
const DEFAULT_REASONING_EFFORTS: [ReasoningEffort; 3] = [
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<CatalogModel>,
}

/// Lenient subset of the OpenRouter model schema: unknown or malformed optional
/// metadata must not hide the rest of the catalog.
#[derive(Deserialize)]
struct CatalogModel {
    id: String,
    #[serde(default)]
    context_length: Option<f64>,
    #[serde(default)]
    top_provider: Option<TopProvider>,
    #[serde(default)]
    architecture: Option<Architecture>,
    #[serde(default)]
    supported_parameters: Vec<String>,
    #[serde(default)]
    endpoints: Vec<String>,
    #[serde(default)]
    reasoning: Option<ModelReasoning>,
}

#[derive(Deserialize)]
struct TopProvider {
    #[serde(default)]
    context_length: Option<f64>,
}

#[derive(Deserialize)]
struct Architecture {
    #[serde(default)]
    output_modalities: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ModelReasoning {
    #[serde(default)]
    default_effort: Option<String>,
    #[serde(default)]
    supported_efforts: Option<Vec<String>>,
}

impl OpenRouterClient {
    pub(super) async fn fetch_model_catalog(&self) -> Result<Vec<ProviderModel>> {
        tokio::time::timeout(CATALOG_TIMEOUT, async {
            let request = self
                .http_client
                .get(format!("{}/models", self.base_url))
                .bearer_auth(&self.api_key);
            let mut response = self
                .apply_app_headers(request)?
                .send()
                .await
                .context("Cannot request OpenRouter model catalog")?;
            // Do not include a provider response body or auth material in diagnostics.
            ensure!(
                response.status().is_success(),
                "OpenRouter model catalog HTTP {}",
                response.status()
            );
            ensure!(
                response
                    .content_length()
                    .is_none_or(|size| size <= MAX_CATALOG_BYTES as u64),
                "OpenRouter model catalog exceeds size limit"
            );
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .context("Cannot read OpenRouter model catalog")?
            {
                ensure!(
                    chunk.len() <= MAX_CATALOG_BYTES - bytes.len(),
                    "OpenRouter model catalog exceeds size limit"
                );
                bytes.extend_from_slice(&chunk);
            }
            decode_models(&bytes)
        })
        .await
        .context("OpenRouter model catalog timed out")?
    }
}

pub(super) fn decode_models(bytes: &[u8]) -> Result<Vec<ProviderModel>> {
    let response: ModelsResponse = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid OpenRouter model catalog metadata"))?;
    let total_models = response.data.len();
    let mut seen = HashSet::new();
    let models: Vec<_> = response
        .data
        .into_iter()
        .filter_map(project_model)
        .filter(|model| seen.insert(model.slug.to_ascii_lowercase()))
        .collect();
    ensure!(
        !models.is_empty(),
        "OpenRouter model catalog has no selectable models ({total_models} returned)"
    );
    Ok(models)
}

fn project_model(model: CatalogModel) -> Option<ProviderModel> {
    if !valid_slug(&model.id) || !serves_text_chat(&model) {
        return None;
    }
    let context_window_tokens = model
        .context_length
        .or_else(|| model.top_provider.as_ref()?.context_length)
        .filter(|tokens| tokens.is_finite() && *tokens >= 1.0)
        .map(|tokens| tokens.min(f64::from(u32::MAX)) as u32)?;

    let supports_reasoning = model
        .supported_parameters
        .iter()
        .any(|parameter| parameter == "reasoning" || parameter == "reasoning_effort");
    let (supported_reasoning_efforts, default_reasoning_effort) = if supports_reasoning {
        reasoning_efforts(model.reasoning.as_ref())
    } else {
        (Vec::new(), None)
    };

    Some(ProviderModel {
        slug: model.id,
        context_window_tokens,
        supported_reasoning_efforts,
        default_reasoning_effort,
    })
}

fn reasoning_efforts(
    reasoning: Option<&ModelReasoning>,
) -> (Vec<ReasoningEffort>, Option<ReasoningEffort>) {
    let mut efforts = Vec::new();
    for effort in reasoning
        .and_then(|reasoning| reasoning.supported_efforts.as_deref())
        .unwrap_or_default()
        .iter()
        .filter_map(|raw| parse_effort(raw))
    {
        if !efforts.contains(&effort) {
            efforts.push(effort);
        }
    }
    if efforts.is_empty() {
        efforts = DEFAULT_REASONING_EFFORTS.to_vec();
    }
    let default = reasoning
        .and_then(|reasoning| reasoning.default_effort.as_deref())
        .and_then(parse_effort)
        .filter(|effort| efforts.contains(effort));
    (efforts, default)
}

/// Only efforts the OpenRouter request projection can send are advertised.
fn parse_effort(raw: &str) -> Option<ReasoningEffort> {
    Some(match raw {
        "none" => ReasoningEffort::None,
        "minimal" => ReasoningEffort::Minimal,
        "low" => ReasoningEffort::Low,
        "medium" => ReasoningEffort::Medium,
        "high" => ReasoningEffort::High,
        "xhigh" => ReasoningEffort::XHigh,
        _ => return None,
    })
}

fn serves_text_chat(model: &CatalogModel) -> bool {
    let chat_endpoint = model.endpoints.is_empty()
        || model
            .endpoints
            .iter()
            .any(|endpoint| endpoint == CHAT_COMPLETIONS_ENDPOINT);
    let text_output = model
        .architecture
        .as_ref()
        .and_then(|architecture| architecture.output_modalities.as_ref())
        .is_none_or(|modalities| modalities.iter().any(|modality| modality == "text"));
    chat_endpoint && text_output
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 256
        && slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/~@".contains(&b))
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
