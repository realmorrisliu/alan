//! Vertex AI Gemini live model discovery, curated fallback, and per-model thinking config.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use tracing::warn;

use crate::gemini_catalog::{
    GeminiVersion, is_gemini_flash, is_gemini_flash_lite, is_gemini_pro,
    is_selectable_gemini_generate_content_slug, merge_and_sort_vertex_catalog,
    normalize_gemini_slug, parse_gemini_version,
};
use crate::{ProviderModel, ReasoningEffort};

use super::endpoint::Transport;
use super::wire::ThinkingConfig;

pub(super) use crate::gemini_catalog::supports_stream_function_call_arguments;

const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_CATALOG_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CATALOG_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_CATALOG_PAGES: usize = 10;
const DEFAULT_CATALOG_PAGE_SIZE: &str = "100";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublisherModelsPage {
    #[serde(default)]
    publisher_models: Vec<Value>,
    #[serde(default)]
    next_page_token: Option<String>,
}

/// Discover live Vertex AI Gemini models in OAuth/ADC mode.
///
/// Express Mode API keys cannot list Model Garden publisher models and return
/// `Ok(None)` without issuing HTTP requests. ADC discovery errors are logged
/// safely (without auth material or raw response bodies) and propagated so
/// `ConnectionService` and `llmfs` serve the curated `bundled-gemini` baseline
/// while preserving failed-discovery state for explicit-selection retries.
pub(super) async fn discover_vertex_model_catalog(
    transport: &Transport,
) -> Result<Option<Vec<ProviderModel>>> {
    if transport.is_express_mode() {
        return Ok(None);
    }
    match fetch_live_vertex_model_catalog(transport).await {
        Ok(models) => Ok(Some(models)),
        Err(err) => {
            warn!(
                reason = %err,
                "Vertex AI live model catalog unavailable; falling back to curated Gemini baseline"
            );
            Err(err)
        }
    }
}

/// Strictly fetch and parse `GET {catalog_host}/v1beta1/publishers/google/models`.
///
/// Enforces total timeout, per-page and total byte caps, and pagination page cap.
/// Never leaks auth material or raw provider response bodies in error diagnostics.
pub(super) async fn fetch_live_vertex_model_catalog(
    transport: &Transport,
) -> Result<Vec<ProviderModel>> {
    fetch_live_vertex_model_catalog_with_limits(
        transport,
        CATALOG_TIMEOUT,
        MAX_CATALOG_PAGE_BYTES,
        MAX_CATALOG_TOTAL_BYTES,
        MAX_CATALOG_PAGES,
    )
    .await
}

pub(super) async fn fetch_live_vertex_model_catalog_with_limits(
    transport: &Transport,
    timeout: Duration,
    max_page_bytes: usize,
    max_total_bytes: usize,
    max_pages: usize,
) -> Result<Vec<ProviderModel>> {
    tokio::time::timeout(timeout, async {
        let mut discovered_slugs = Vec::new();
        let mut seen_tokens = HashSet::new();
        let mut next_page_token: Option<String> = None;
        let mut total_bytes: usize = 0;

        for _ in 0..max_pages.max(1) {
            let mut url = reqwest::Url::parse(transport.endpoint.catalog_url())
                .map_err(|_| anyhow::anyhow!("Invalid Vertex AI model catalog URL"))?;
            {
                let mut pairs = url.query_pairs_mut();
                pairs.append_pair("pageSize", DEFAULT_CATALOG_PAGE_SIZE);
                if let Some(token) = next_page_token.as_deref() {
                    pairs.append_pair("pageToken", token);
                }
            }

            let mut response = transport.get_catalog_page(&url).await?;
            let remaining_total = max_total_bytes.saturating_sub(total_bytes);
            let page_cap = max_page_bytes.min(remaining_total);
            ensure!(
                response
                    .content_length()
                    .is_none_or(|size| size <= page_cap as u64),
                "Vertex AI model catalog exceeds size limit"
            );

            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| anyhow::anyhow!("Cannot read Vertex AI model catalog"))?
            {
                ensure!(
                    chunk.len() <= page_cap.saturating_sub(bytes.len()),
                    "Vertex AI model catalog exceeds size limit"
                );
                bytes.extend_from_slice(&chunk);
            }
            total_bytes = total_bytes.saturating_add(bytes.len());

            let (page_slugs, next_token) = decode_publisher_models_page(&bytes)?;
            discovered_slugs.extend(page_slugs);

            match next_token.filter(|token| !token.trim().is_empty()) {
                Some(token) if seen_tokens.insert(token.clone()) => {
                    next_page_token = Some(token);
                }
                _ => break,
            }
        }

        ensure!(
            !discovered_slugs.is_empty(),
            "Vertex AI model catalog has no selectable Gemini models"
        );
        Ok(merge_and_sort_vertex_catalog(discovered_slugs))
    })
    .await
    .context("Vertex AI model catalog timed out")?
}

/// Leniently parse one `v1beta1/publishers/google/models` response page.
pub(super) fn decode_publisher_models_page(bytes: &[u8]) -> Result<(Vec<String>, Option<String>)> {
    let page: PublisherModelsPage = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("Invalid Vertex AI model catalog metadata"))?;
    let mut slugs = Vec::new();
    for entry in page.publisher_models {
        if let Some(slug) = extract_selectable_publisher_slug(&entry) {
            slugs.push(slug);
        }
    }
    Ok((slugs, page.next_page_token))
}

fn extract_selectable_publisher_slug(entry: &Value) -> Option<String> {
    let object = entry.as_object()?;
    let raw_name = object.get("name")?.as_str()?;
    let _version_id = object.get("versionId").and_then(Value::as_str);

    if let Some(stage) = object.get("launchStage").and_then(Value::as_str) {
        let normalized_stage = stage.trim().to_ascii_uppercase();
        if matches!(
            normalized_stage.as_str(),
            "DEPRECATED" | "SUNSET" | "RETIRED"
        ) {
            return None;
        }
    }

    if let Some(actions) = object.get("supportedActions")
        && !supported_actions_allow_generate_content(actions)
    {
        return None;
    }

    let slug = normalize_gemini_slug(raw_name).to_ascii_lowercase();
    is_selectable_gemini_generate_content_slug(&slug).then_some(slug)
}

fn supported_actions_allow_generate_content(actions: &Value) -> bool {
    match actions {
        Value::Array(items) => {
            let names: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            if names.is_empty() {
                return true;
            }
            names.iter().any(|action| {
                action.eq_ignore_ascii_case("generateContent")
                    || action.eq_ignore_ascii_case("streamGenerateContent")
            })
        }
        Value::Object(map) => {
            let explicit_generate = map.get("generateContent").and_then(Value::as_bool);
            let explicit_stream = map.get("streamGenerateContent").and_then(Value::as_bool);
            if explicit_generate == Some(false) && explicit_stream != Some(true) {
                return false;
            }
            true
        }
        _ => true,
    }
}

/// Build `thinkingConfig` for a `GenerateContent` request.
///
/// When `reasoning_effort` is omitted on a schema-constrained structured-output
/// request (such as Tier 2 fallback decisions), bound thinking to the lowest
/// model-supported setting and omit thought summaries so thinking tokens do not
/// exhaust `maxOutputTokens` before JSON output is emitted.
pub(super) fn build_gemini_request_thinking_config(
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
    has_response_schema: bool,
) -> Result<Option<ThinkingConfig>> {
    if reasoning_effort.is_some() || !has_response_schema {
        return build_gemini_thinking_config(model, reasoning_effort);
    }

    let version = parse_gemini_version(model);
    if let Some(version) = version
        && version.major >= 3
    {
        let level = if is_gemini_flash(model) {
            "minimal"
        } else {
            "low"
        };
        return Ok(Some(ThinkingConfig {
            include_thoughts: None,
            thinking_level: Some(level.to_string()),
            thinking_budget: None,
        }));
    }

    if version == Some(GeminiVersion { major: 2, minor: 5 }) {
        // Gemini 2.5 Pro rejects budget=0; 128 is its documented minimum budget.
        let budget = if is_gemini_pro(model) { 128 } else { 0 };
        return Ok(Some(ThinkingConfig {
            include_thoughts: None,
            thinking_level: None,
            thinking_budget: Some(budget),
        }));
    }

    Ok(None)
}

/// Map canonical [`ReasoningEffort`] to Vertex AI `thinkingConfig`.
///
/// Sets `includeThoughts: true` whenever thinking is active so thought parts
/// stream back as `thought: true` summaries. Supports Gemini 2.5 budget-based
/// thinking and Gemini 3.x / 4.x+ level-based thinking cleanly.
pub(super) fn build_gemini_thinking_config(
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Option<ThinkingConfig>> {
    let version = parse_gemini_version(model);
    if let Some(version) = version
        && version.major >= 3
    {
        let Some(effort) = reasoning_effort else {
            return Ok(Some(ThinkingConfig {
                include_thoughts: Some(true),
                thinking_level: None,
                thinking_budget: None,
            }));
        };
        let is_flash = is_gemini_flash(model);
        let is_3_0_pro = version.major == 3 && version.minor == 0 && !is_flash;
        let thinking_level = match effort {
            ReasoningEffort::None => {
                anyhow::bail!(
                    "Gemini {} does not support disabling thinking with effort `none`",
                    version.major
                )
            }
            ReasoningEffort::Minimal if is_flash => "minimal",
            ReasoningEffort::Minimal => {
                anyhow::bail!(
                    "Gemini {} Pro does not support reasoning effort `minimal`",
                    version.major
                )
            }
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium if !is_3_0_pro => "medium",
            ReasoningEffort::Medium => {
                anyhow::bail!("Gemini 3 Pro supports reasoning efforts `low` and `high`")
            }
            ReasoningEffort::High => "high",
            ReasoningEffort::XHigh | ReasoningEffort::Max | ReasoningEffort::Ultra => {
                anyhow::bail!(
                    "Gemini {} does not support reasoning effort `{effort}`",
                    version.major
                )
            }
        };
        return Ok(Some(ThinkingConfig {
            include_thoughts: Some(true),
            thinking_level: Some(thinking_level.to_string()),
            thinking_budget: None,
        }));
    }

    if version == Some(GeminiVersion { major: 2, minor: 5 }) {
        return match reasoning_effort {
            Some(ReasoningEffort::None) if is_gemini_pro(model) => {
                anyhow::bail!("Gemini 2.5 Pro does not support disabling thinking")
            }
            Some(ReasoningEffort::None) => Ok(Some(ThinkingConfig {
                include_thoughts: None,
                thinking_level: None,
                thinking_budget: Some(0),
            })),
            Some(effort) => Ok(Some(ThinkingConfig {
                include_thoughts: Some(true),
                thinking_level: None,
                thinking_budget: Some(gemini_budget_for_effort(effort)?),
            })),
            None if is_gemini_flash_lite(model) => Ok(None),
            None => Ok(Some(ThinkingConfig {
                include_thoughts: Some(true),
                thinking_level: None,
                thinking_budget: None,
            })),
        };
    }

    if reasoning_effort.is_some() {
        anyhow::bail!(
            "Gemini model `{}` does not declare reasoning effort support",
            model.to_ascii_lowercase()
        );
    }

    Ok(None)
}

fn gemini_budget_for_effort(effort: ReasoningEffort) -> Result<i32> {
    Ok(match effort {
        ReasoningEffort::None => 0,
        ReasoningEffort::Minimal => 512,
        ReasoningEffort::Low => 1_024,
        ReasoningEffort::Medium => 4_096,
        ReasoningEffort::High => 8_192,
        ReasoningEffort::XHigh => 16_384,
        ReasoningEffort::Max | ReasoningEffort::Ultra => {
            anyhow::bail!("Provider does not support reasoning effort `{effort}`")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemini_catalog::{
        GEMINI_DEFAULT_CONTEXT_WINDOW, infer_gemini_provider_model, vertex_baseline_catalog,
    };

    #[test]
    fn catalog_includes_gemini_25_and_gemini_3_models_with_valid_efforts() {
        let catalog = vertex_baseline_catalog();
        for required in [
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
            "gemini-3-pro-preview",
            "gemini-3-flash-preview",
            "gemini-3.1-pro-preview",
            "gemini-3.1-flash-lite-preview",
        ] {
            let model = catalog
                .iter()
                .find(|entry| entry.slug == required)
                .unwrap_or_else(|| panic!("missing catalog model {required}"));
            assert!(model.context_window_tokens >= GEMINI_DEFAULT_CONTEXT_WINDOW);
            for effort in &model.supported_reasoning_efforts {
                build_gemini_thinking_config(&model.slug, Some(*effort))
                    .unwrap_or_else(|err| panic!("{} failed effort {effort}: {err}", model.slug));
            }
            if let Some(default) = model.default_reasoning_effort {
                assert!(model.supported_reasoning_efforts.contains(&default));
            }
        }
    }

    #[test]
    fn future_generation_thinking_config_matches_inferred_efforts() {
        for slug in [
            "gemini-3.5-pro-preview",
            "gemini-3.5-flash",
            "gemini-4-pro",
            "gemini-4.0-flash-lite",
            "gemini-4.2-pro-preview",
        ] {
            let inferred = infer_gemini_provider_model(slug);
            assert!(!inferred.supported_reasoning_efforts.is_empty(), "{slug}");
            for effort in &inferred.supported_reasoning_efforts {
                let config = build_gemini_thinking_config(slug, Some(*effort))
                    .unwrap_or_else(|err| panic!("{slug} failed effort {effort}: {err}"))
                    .expect("thinking config");
                assert_eq!(config.include_thoughts, Some(true));
                assert!(config.thinking_level.is_some());
                assert_eq!(config.thinking_budget, None);
            }
            assert!(
                build_gemini_thinking_config(slug, Some(ReasoningEffort::None)).is_err(),
                "{slug} should reject effort `none`"
            );
            assert!(
                build_gemini_thinking_config(slug, Some(ReasoningEffort::XHigh)).is_err(),
                "{slug} should reject effort `xhigh`"
            );
        }
    }

    #[test]
    fn thinking_config_sets_include_thoughts_for_active_efforts() {
        let g3 = build_gemini_thinking_config("gemini-3-flash-preview", Some(ReasoningEffort::Low))
            .unwrap()
            .unwrap();
        assert_eq!(g3.include_thoughts, Some(true));
        assert_eq!(g3.thinking_level.as_deref(), Some("low"));
        assert_eq!(g3.thinking_budget, None);

        let g25 = build_gemini_thinking_config("gemini-2.5-pro", Some(ReasoningEffort::High))
            .unwrap()
            .unwrap();
        assert_eq!(g25.include_thoughts, Some(true));
        assert_eq!(g25.thinking_budget, Some(8_192));

        let disabled =
            build_gemini_thinking_config("gemini-2.5-flash", Some(ReasoningEffort::None))
                .unwrap()
                .unwrap();
        assert_eq!(disabled.include_thoughts, None);
        assert_eq!(disabled.thinking_budget, Some(0));

        assert!(
            build_gemini_thinking_config("gemini-2.0-flash", None)
                .unwrap()
                .is_none()
        );
        assert!(
            build_gemini_thinking_config("gemini-2.5-flash-lite", None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn decoder_sanitizes_malformed_json_errors() {
        let err = decode_publisher_models_page(b"secret-token-raw-body").unwrap_err();
        let message = err.to_string();
        assert!(!message.contains("secret-token"));
        assert!(!message.contains("raw-body"));
    }
}
