//! Managed model discovery stays behind the same account/auth adapter as generation.
use super::ChatgptResponsesClient;
use crate::{ProviderModel, ReasoningEffort};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};

const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    models: Option<Vec<CatalogModel>>,
    #[serde(default)]
    data: Option<Vec<OpenAiModel>>,
}

#[derive(Deserialize)]
struct CatalogModel {
    slug: String,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    context_window: Option<u32>,
    #[serde(default)]
    default_reasoning_level: Option<ReasoningEffort>,
    #[serde(default)]
    supported_reasoning_levels: Vec<ReasoningLevel>,
}

#[derive(Deserialize)]
struct OpenAiModel {
    id: String,
    #[serde(default)]
    context_window: Option<u32>,
    #[serde(default)]
    default_reasoning_level: Option<ReasoningEffort>,
    #[serde(default)]
    supported_reasoning_levels: Vec<ReasoningLevel>,
}

#[derive(Deserialize)]
struct ReasoningLevel {
    effort: ReasoningEffort,
}

impl ChatgptResponsesClient {
    pub(super) async fn fetch_model_catalog(&self) -> Result<Vec<ProviderModel>> {
        tokio::time::timeout(Duration::from_secs(15), async {
            let url = reqwest::Url::parse(&format!("{}/models", self.base_url))?;
            let mut response = self
                .send_with_auth_retry("request ChatGPT model catalog", || {
                    self.client.get(url.clone())
                })
                .await?;
            // Do not include a provider response body or auth material in diagnostics.
            ensure!(
                response.status().is_success(),
                "ChatGPT model catalog HTTP {}",
                response.status()
            );
            ensure!(
                response
                    .content_length()
                    .is_none_or(|size| size <= MAX_CATALOG_BYTES as u64),
                "ChatGPT model catalog exceeds size limit"
            );
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .context("Cannot read ChatGPT model catalog")?
            {
                ensure!(
                    chunk.len() <= MAX_CATALOG_BYTES - bytes.len(),
                    "ChatGPT model catalog exceeds size limit"
                );
                bytes.extend_from_slice(&chunk);
            }
            decode_models(&bytes)
        })
        .await
        .context("ChatGPT model catalog timed out")?
    }
}

fn default_context_window_for_slug(slug: &str) -> u32 {
    let lower = slug.to_ascii_lowercase();
    if lower.starts_with("gpt-5.4") {
        1_050_000
    } else if lower.starts_with("gpt-5") {
        400_000
    } else if lower.starts_with("gpt-4.1") {
        1_047_576
    } else {
        200_000
    }
}

fn is_non_chat_openai_model(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    [
        "dall-e-",
        "whisper-",
        "tts-",
        "text-embedding-",
        "omni-moderation-",
        "text-moderation-",
        "babbage-",
        "davinci-",
        "gpt-image-",
        "sora-",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn validate_and_push_model(
    slug: String,
    context_window: Option<u32>,
    default_reasoning_level: Option<ReasoningEffort>,
    supported_reasoning_levels: Vec<ReasoningLevel>,
    seen: &mut HashSet<String>,
    models: &mut Vec<ProviderModel>,
) -> Result<()> {
    let resolved_context_window = match context_window {
        Some(0) => anyhow::bail!("Invalid ChatGPT model identity or context window"),
        Some(window) => window,
        None => default_context_window_for_slug(&slug),
    };
    ensure!(
        !slug.is_empty()
            && slug.len() <= 128
            && slug
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            && seen.insert(slug.to_ascii_lowercase()),
        "Invalid ChatGPT model identity or context window"
    );
    let efforts: Vec<_> = supported_reasoning_levels
        .into_iter()
        .map(|level| level.effort)
        .collect();
    let unique: HashSet<_> = efforts.iter().collect();
    ensure!(
        unique.len() == efforts.len()
            && default_reasoning_level.is_none_or(|default| efforts.contains(&default)),
        "Invalid ChatGPT model reasoning metadata"
    );
    models.push(ProviderModel {
        slug,
        context_window_tokens: resolved_context_window,
        supported_reasoning_efforts: efforts,
        default_reasoning_effort: default_reasoning_level,
    });
    Ok(())
}

fn decode_models(bytes: &[u8]) -> Result<Vec<ProviderModel>> {
    let response: ModelsResponse = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid ChatGPT model catalog metadata"))?;
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let total_models: usize;

    if let Some(catalog_models) = response.models {
        total_models = catalog_models.len();
        for model in catalog_models
            .into_iter()
            .filter(|model| model.visibility.as_deref().is_none_or(|v| v == "list"))
        {
            validate_and_push_model(
                model.slug,
                model.context_window,
                model.default_reasoning_level,
                model.supported_reasoning_levels,
                &mut seen,
                &mut models,
            )?;
        }
    } else if let Some(data_models) = response.data {
        total_models = data_models.len();
        for model in data_models
            .into_iter()
            .filter(|model| !is_non_chat_openai_model(&model.id))
        {
            validate_and_push_model(
                model.id,
                model.context_window,
                model.default_reasoning_level,
                model.supported_reasoning_levels,
                &mut seen,
                &mut models,
            )?;
        }
    } else {
        anyhow::bail!("Invalid ChatGPT model catalog metadata");
    }

    ensure!(
        !models.is_empty(),
        "ChatGPT model catalog has no selectable models ({total_models} returned)"
    );
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_preserves_account_metadata_and_rejects_invalid_entries() {
        let model = serde_json::json!({"slug":"test-model", "visibility":"list",
            "context_window":100000, "default_reasoning_level":"medium",
            "supported_reasoning_levels":[{"effort":"medium"},{"effort":"high"},{"effort":"max"},{"effort":"ultra"}]});
        let decode = |models| {
            decode_models(&serde_json::to_vec(&serde_json::json!({"models":models})).unwrap())
        };
        let parsed = decode(vec![model.clone()]).unwrap();
        assert_eq!(parsed[0].context_window_tokens, 100000);
        assert_eq!(
            parsed[0].supported_reasoning_efforts,
            vec![
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Max,
                ReasoningEffort::Ultra
            ]
        );
        assert!(decode(vec![model.clone(), model.clone()]).is_err());
        for (field, value) in [
            ("slug", serde_json::json!("bad\nmodel")),
            ("context_window", serde_json::json!(0)),
            ("default_reasoning_level", serde_json::json!("low")),
            ("visibility", serde_json::json!("hide")),
        ] {
            let mut invalid = model.clone();
            invalid[field] = value;
            assert!(decode(vec![invalid]).is_err(), "{field}");
        }
        assert!(
            decode_models(b"secret malformed body")
                .unwrap_err()
                .to_string()
                .find("secret")
                .is_none()
        );
    }

    #[test]
    fn catalog_decodes_openai_data_shape_with_fallback_context_windows() {
        let raw = serde_json::to_vec(&serde_json::json!({
            "object": "list",
            "data": [
                {"id": "gpt-5.4"},
                {"id": "gpt-5-mini"},
                {"id": "dall-e-3"},
                {"id": "text-embedding-3-large"}
            ]
        }))
        .unwrap();
        let parsed = decode_models(&raw).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].slug, "gpt-5.4");
        assert_eq!(parsed[0].context_window_tokens, 1_050_000);
        assert_eq!(parsed[1].slug, "gpt-5-mini");
        assert_eq!(parsed[1].context_window_tokens, 400_000);
    }
}
