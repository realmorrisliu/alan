//! Managed model discovery stays behind the same account/auth adapter as generation.
use super::ChatgptResponsesClient;
use crate::{ProviderModel, ReasoningEffort};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};

// The endpoint gates model visibility by Codex protocol compatibility, not Alan's
// release version. Qualified against the managed catalog on 2026-10-07.
pub(super) const CODEX_MODELS_API_VERSION: &str = "0.159.0";
const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct ModelsResponse {
    models: Vec<Model>,
}

#[derive(Deserialize)]
struct Model {
    slug: String,
    visibility: String,
    context_window: u32,
    default_reasoning_level: Option<ReasoningEffort>,
    supported_reasoning_levels: Vec<ReasoningLevel>,
}

#[derive(Deserialize)]
struct ReasoningLevel {
    effort: ReasoningEffort,
}

impl ChatgptResponsesClient {
    pub(super) async fn fetch_model_catalog(&self) -> Result<Vec<ProviderModel>> {
        tokio::time::timeout(Duration::from_secs(15), async {
            let mut response = self.send_models_request(false).await?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                response = self.send_models_request(true).await?;
            }
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

    async fn send_models_request(&self, refresh: bool) -> Result<reqwest::Response> {
        let auth = self.request_auth(refresh).await?;
        let mut url = reqwest::Url::parse(&format!("{}/models", self.base_url))?;
        url.query_pairs_mut()
            .append_pair("client_version", CODEX_MODELS_API_VERSION);
        let request = self.apply_custom_headers(self.client.get(url));
        request
            .header("Authorization", format!("Bearer {}", auth.access_token))
            .header("ChatGPT-Account-ID", auth.account_id)
            .send()
            .await
            .context("Cannot request ChatGPT model catalog")
    }
}

fn decode_models(bytes: &[u8]) -> Result<Vec<ProviderModel>> {
    let response: ModelsResponse = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid ChatGPT model catalog metadata"))?;
    let total_models = response.models.len();
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for model in response
        .models
        .into_iter()
        .filter(|model| model.visibility == "list")
    {
        ensure!(
            !model.slug.is_empty()
                && model.slug.len() <= 128
                && model
                    .slug
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                && model.context_window > 0
                && seen.insert(model.slug.to_ascii_lowercase()),
            "Invalid ChatGPT model identity or context window"
        );
        let efforts: Vec<_> = model
            .supported_reasoning_levels
            .into_iter()
            .map(|level| level.effort)
            .collect();
        let unique: HashSet<_> = efforts.iter().collect();
        ensure!(
            unique.len() == efforts.len()
                && model
                    .default_reasoning_level
                    .is_none_or(|default| efforts.contains(&default)),
            "Invalid ChatGPT model reasoning metadata"
        );
        models.push(ProviderModel {
            slug: model.slug,
            context_window_tokens: model.context_window,
            supported_reasoning_efforts: efforts,
            default_reasoning_effort: model.default_reasoning_level,
        });
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
}
