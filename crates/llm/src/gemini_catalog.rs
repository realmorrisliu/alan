//! Shared Google Vertex AI Gemini model baseline, slug validation, and family capability rules.
//!
//! Kept free of HTTP/SDK dependencies so `alan-llmfs`, `alan-agent-engine`, and
//! `alan-service-manager` share one canonical Gemini baseline and capability model
//! even when `alan-llm` is compiled without the `provider-vertex` feature.

use std::cmp::Reverse;
use std::collections::HashSet;

use crate::{ProviderModel, ReasoningEffort};

/// Default Vertex AI Gemini model when no explicit override is configured.
pub const GEMINI_DEFAULT_MODEL: &str = "gemini-2.5-flash";

/// Standard 1M-token context window shared by current Gemini generations.
pub const GEMINI_DEFAULT_CONTEXT_WINDOW: u32 = 1_048_576;

/// Curated baseline Vertex AI Gemini text/multimodal `generateContent` slugs.
///
/// Used as the graceful fallback when live Vertex AI Model Garden discovery is
/// unavailable (for example Express Mode API keys, IAM 403, or offline startup)
/// and merged into live discovery so regional listings never drop baseline models.
pub const VERTEX_BASELINE_MODEL_SLUGS: &[&str] = &[
    "gemini-3.1-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3-pro-preview",
    "gemini-3-flash-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
];

const MAX_PUBLISHER_SLUG_BYTES: usize = 128;

/// Specialized non-text-chat Gemini model tokens skipped during live discovery.
const NON_CHAT_SPECIALIZED_TOKENS: &[&str] = &[
    "aqa",
    "audio",
    "embedding",
    "embeddings",
    "image",
    "imagen",
    "live",
    "music",
    "robotics",
    "speech",
    "transcribe",
    "tts",
    "veo",
    "video",
];

/// Parsed major/minor generation from a `gemini-<major>[.<minor>]-*` model id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GeminiVersion {
    pub major: u32,
    pub minor: u32,
}

/// Strip optional Vertex publisher resource prefixes (`publishers/google/models/`, `models/`)
/// and `@version` suffixes from a model identifier.
pub fn normalize_gemini_slug(raw: &str) -> &str {
    let trimmed = raw.trim();
    let without_prefix = trimmed
        .strip_prefix("publishers/google/models/")
        .or_else(|| trimmed.strip_prefix("models/"))
        .unwrap_or(trimmed);
    without_prefix
        .split_once('@')
        .map_or(without_prefix, |(base, _)| base)
}

/// Validate a safe explicit official `gemini-*` publisher model slug.
///
/// Rejects path traversal, slashes, whitespace, uppercase/control bytes, and
/// non-Gemini identifiers while permitting brand-new official `gemini-*` models.
pub fn is_valid_gemini_publisher_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.len() > MAX_PUBLISHER_SLUG_BYTES {
        return false;
    }
    let Some(rest) = slug.strip_prefix("gemini-") else {
        return false;
    };
    if rest.is_empty() || slug.contains("..") || slug.ends_with('-') || slug.ends_with('.') {
        return false;
    }
    slug.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b'_'))
}

/// Whether `slug` is a general text/multimodal Gemini `generateContent` model
/// suitable for automatic catalog discovery (skipping non-Gemini and specialized
/// embedding/imagen/veo/tts/image/live/audio models).
pub fn is_selectable_gemini_generate_content_slug(slug: &str) -> bool {
    if !is_valid_gemini_publisher_slug(slug) {
        return false;
    }
    !slug
        .split(['-', '_'])
        .any(|token| NON_CHAT_SPECIALIZED_TOKENS.contains(&token))
}

/// Parse `GeminiVersion` from a Gemini model identifier (e.g. `gemini-2.5-pro`,
/// `gemini-3-flash-preview`, `gemini-3.1-pro-preview`, `gemini-4.0-pro`).
pub fn parse_gemini_version(model: &str) -> Option<GeminiVersion> {
    let lower = normalize_gemini_slug(model).to_ascii_lowercase();
    let rest = lower.strip_prefix("gemini-")?;
    let version_token = rest.split('-').next()?;
    if version_token.is_empty() {
        return None;
    }
    match version_token.split_once('.') {
        Some((major_str, minor_str)) => {
            if major_str.is_empty()
                || minor_str.is_empty()
                || !major_str.bytes().all(|b| b.is_ascii_digit())
                || !minor_str.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            Some(GeminiVersion {
                major: major_str.parse().ok()?,
                minor: minor_str.parse().ok()?,
            })
        }
        None => {
            if !version_token.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            Some(GeminiVersion {
                major: version_token.parse().ok()?,
                minor: 0,
            })
        }
    }
}

/// Derive a concise family label for a Gemini model (e.g. `gemini-2.5`, `gemini-3`, `gemini-3.1`).
pub fn gemini_model_family(slug: &str) -> String {
    match parse_gemini_version(slug) {
        Some(GeminiVersion { major, minor: 0 }) => format!("gemini-{major}"),
        Some(GeminiVersion { major, minor }) => format!("gemini-{major}.{minor}"),
        None => "gemini".to_string(),
    }
}

/// Whether `model` is a Flash variant (including `flash-lite`).
pub fn is_gemini_flash(model: &str) -> bool {
    let lower = normalize_gemini_slug(model).to_ascii_lowercase();
    lower.split(['-', '_']).any(|token| token == "flash")
}

/// Whether `model` is a Flash-Lite variant.
pub fn is_gemini_flash_lite(model: &str) -> bool {
    let lower = normalize_gemini_slug(model).to_ascii_lowercase();
    lower.contains("flash-lite") || lower.contains("flash_lite")
}

/// Whether `model` is a Pro variant.
pub fn is_gemini_pro(model: &str) -> bool {
    let lower = normalize_gemini_slug(model).to_ascii_lowercase();
    lower.split(['-', '_']).any(|token| token == "pro")
}

/// Whether Vertex AI supports `streamFunctionCallArguments` for `model` (Gemini 3.x / 4.x+).
pub fn supports_stream_function_call_arguments(model: &str) -> bool {
    parse_gemini_version(model).is_some_and(|version| version.major >= 3)
}

/// Whether Vertex AI validates function-call `thoughtSignature`s (Gemini 3.x / 4.x+).
pub fn model_requires_call_signatures(model: &str) -> bool {
    match parse_gemini_version(model) {
        Some(version) => version.major >= 3,
        None => {
            let lower = normalize_gemini_slug(model).to_ascii_lowercase();
            !(lower.starts_with("gemini-1") || lower.starts_with("gemini-2"))
        }
    }
}

/// Infer [`ProviderModel`] capabilities for any official Gemini publisher slug
/// using family-based generation rules.
pub fn infer_gemini_provider_model(slug: &str) -> ProviderModel {
    let canonical = normalize_gemini_slug(slug).to_string();
    let is_flash = is_gemini_flash(&canonical);
    let is_flash_lite = is_gemini_flash_lite(&canonical);

    let (supported_reasoning_efforts, default_reasoning_effort) =
        match parse_gemini_version(&canonical) {
            Some(version) if version.major >= 3 => {
                if is_flash {
                    let default = if is_flash_lite {
                        ReasoningEffort::Low
                    } else {
                        ReasoningEffort::Medium
                    };
                    (
                        vec![
                            ReasoningEffort::Minimal,
                            ReasoningEffort::Low,
                            ReasoningEffort::Medium,
                            ReasoningEffort::High,
                        ],
                        Some(default),
                    )
                } else if version.major == 3 && version.minor == 0 {
                    // Gemini 3.0 Pro only accepts `low` and `high` thinkingLevel values.
                    (
                        vec![ReasoningEffort::Low, ReasoningEffort::High],
                        Some(ReasoningEffort::High),
                    )
                } else {
                    // Gemini 3.1+ and 4.x+ Pro/general models support low/medium/high thinkingLevel.
                    (
                        vec![
                            ReasoningEffort::Low,
                            ReasoningEffort::Medium,
                            ReasoningEffort::High,
                        ],
                        Some(ReasoningEffort::High),
                    )
                }
            }
            Some(GeminiVersion { major: 2, minor: 5 }) => {
                if is_flash_lite {
                    (
                        vec![
                            ReasoningEffort::None,
                            ReasoningEffort::Minimal,
                            ReasoningEffort::Low,
                            ReasoningEffort::Medium,
                            ReasoningEffort::High,
                            ReasoningEffort::XHigh,
                        ],
                        Some(ReasoningEffort::None),
                    )
                } else if is_flash {
                    (
                        vec![
                            ReasoningEffort::None,
                            ReasoningEffort::Minimal,
                            ReasoningEffort::Low,
                            ReasoningEffort::Medium,
                            ReasoningEffort::High,
                            ReasoningEffort::XHigh,
                        ],
                        Some(ReasoningEffort::Medium),
                    )
                } else {
                    (
                        vec![
                            ReasoningEffort::Minimal,
                            ReasoningEffort::Low,
                            ReasoningEffort::Medium,
                            ReasoningEffort::High,
                            ReasoningEffort::XHigh,
                        ],
                        Some(ReasoningEffort::High),
                    )
                }
            }
            _ => (Vec::new(), None),
        };

    ProviderModel {
        slug: canonical,
        context_window_tokens: GEMINI_DEFAULT_CONTEXT_WINDOW,
        supported_reasoning_efforts,
        default_reasoning_effort,
    }
}

/// Curated baseline Vertex AI Gemini catalog derived from the single family ruleset.
pub fn vertex_baseline_catalog() -> Vec<ProviderModel> {
    let mut models: Vec<ProviderModel> = VERTEX_BASELINE_MODEL_SLUGS
        .iter()
        .map(|slug| infer_gemini_provider_model(slug))
        .collect();
    sort_gemini_models(&mut models);
    models
}

/// Merge discovered Vertex AI Gemini slugs with the curated baseline, deduplicating
/// case-insensitively and sorting newest-generation first.
pub fn merge_and_sort_vertex_catalog<I>(discovered_slugs: I) -> Vec<ProviderModel>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = HashSet::new();
    let mut models = Vec::new();

    for raw in discovered_slugs {
        let slug = normalize_gemini_slug(&raw).to_ascii_lowercase();
        if is_selectable_gemini_generate_content_slug(&slug) && seen.insert(slug.clone()) {
            models.push(infer_gemini_provider_model(&slug));
        }
    }

    for baseline in VERTEX_BASELINE_MODEL_SLUGS {
        if seen.insert((*baseline).to_string()) {
            models.push(infer_gemini_provider_model(baseline));
        }
    }

    sort_gemini_models(&mut models);
    models
}

/// Sort Gemini models newest-generation first, then Pro -> Flash -> Flash-Lite -> other,
/// then GA before preview/experimental, then lexicographically by slug.
pub fn sort_gemini_models(models: &mut [ProviderModel]) {
    models.sort_by_key(|model| sort_key(&model.slug));
}

fn sort_key(slug: &str) -> (Reverse<(u32, u32)>, u8, u8, String) {
    let lower = normalize_gemini_slug(slug).to_ascii_lowercase();
    let version = parse_gemini_version(&lower).map_or((0, 0), |v| (v.major, v.minor));
    let tier = if is_gemini_flash_lite(&lower) {
        2
    } else if is_gemini_flash(&lower) {
        1
    } else if is_gemini_pro(&lower) || lower.split(['-', '_']).any(|t| t == "ultra") {
        0
    } else {
        3
    };
    let stability = if lower
        .split(['-', '_'])
        .any(|t| t == "exp" || t == "experimental")
    {
        2
    } else if lower.split(['-', '_']).any(|t| t == "preview") {
        1
    } else {
        0
    };
    (Reverse(version), tier, stability, lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_and_future_gemini_versions() {
        assert_eq!(
            parse_gemini_version("gemini-1.5-pro"),
            Some(GeminiVersion { major: 1, minor: 5 })
        );
        assert_eq!(
            parse_gemini_version("gemini-2.5-flash"),
            Some(GeminiVersion { major: 2, minor: 5 })
        );
        assert_eq!(
            parse_gemini_version("gemini-3-pro-preview"),
            Some(GeminiVersion { major: 3, minor: 0 })
        );
        assert_eq!(
            parse_gemini_version("gemini-3.1-flash-lite-preview"),
            Some(GeminiVersion { major: 3, minor: 1 })
        );
        assert_eq!(
            parse_gemini_version("gemini-4-pro"),
            Some(GeminiVersion { major: 4, minor: 0 })
        );
        assert_eq!(
            parse_gemini_version("publishers/google/models/gemini-10.2-flash@001"),
            Some(GeminiVersion {
                major: 10,
                minor: 2
            })
        );
        assert_eq!(parse_gemini_version("gemini-exp-1206"), None);
        assert_eq!(parse_gemini_version("claude-sonnet-4-6"), None);
    }

    #[test]
    fn validates_safe_slugs_and_filters_specialized_models() {
        for valid in [
            "gemini-2.5-pro",
            "gemini-3-flash-preview",
            "gemini-3.1-pro-preview",
            "gemini-4.0-pro-preview",
            "gemini-4.5-flash-001",
            "gemini-exp-1206",
        ] {
            assert!(is_valid_gemini_publisher_slug(valid), "{valid}");
            assert!(is_selectable_gemini_generate_content_slug(valid), "{valid}");
        }

        for specialized in [
            "gemini-embedding-001",
            "gemini-2.5-flash-preview-tts",
            "gemini-2.5-flash-image",
            "gemini-2.0-flash-live-001",
            "gemini-2.5-flash-native-audio-preview",
            "gemini-robotics-er-1.5-preview",
        ] {
            assert!(is_valid_gemini_publisher_slug(specialized), "{specialized}");
            assert!(
                !is_selectable_gemini_generate_content_slug(specialized),
                "{specialized}"
            );
        }

        for invalid in [
            "",
            "gemini-",
            "gemini-3-pro/",
            "gemini-3/../evil",
            "gemini-3-pro?x=1",
            "gemini-3 pro",
            "GEMINI-3-PRO",
            "imagen-3.0-generate-002",
            "veo-2.0-generate-001",
            "arbitrary-new-model",
            "claude-sonnet-4-6",
        ] {
            assert!(!is_valid_gemini_publisher_slug(invalid), "{invalid}");
            assert!(
                !is_selectable_gemini_generate_content_slug(invalid),
                "{invalid}"
            );
        }
    }

    #[test]
    fn future_generations_enable_streaming_tool_args_and_call_signatures() {
        assert!(!supports_stream_function_call_arguments("gemini-2.5-pro"));
        assert!(!model_requires_call_signatures("gemini-2.5-pro"));

        for future in [
            "gemini-3-pro-preview",
            "gemini-3.1-pro-preview",
            "gemini-3.5-flash",
            "gemini-4-pro",
            "gemini-4.2-flash-lite",
            "gemini-10-pro",
        ] {
            assert!(supports_stream_function_call_arguments(future), "{future}");
            assert!(model_requires_call_signatures(future), "{future}");
        }
    }

    #[test]
    fn infers_family_capabilities_and_sorts_newest_first() {
        let g4_pro = infer_gemini_provider_model("gemini-4-pro");
        assert_eq!(g4_pro.context_window_tokens, GEMINI_DEFAULT_CONTEXT_WINDOW);
        assert_eq!(
            g4_pro.supported_reasoning_efforts,
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High
            ]
        );
        assert_eq!(g4_pro.default_reasoning_effort, Some(ReasoningEffort::High));

        let g4_flash = infer_gemini_provider_model("gemini-4-flash");
        assert_eq!(
            g4_flash.supported_reasoning_efforts,
            vec![
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High
            ]
        );
        assert_eq!(
            g4_flash.default_reasoning_effort,
            Some(ReasoningEffort::Medium)
        );

        let merged = merge_and_sort_vertex_catalog(vec![
            "gemini-2.0-flash".to_string(),
            "gemini-4-flash".to_string(),
            "gemini-4-pro-preview".to_string(),
            "gemini-4-pro".to_string(),
            "gemini-2.5-pro".to_string(),
            "gemini-embedding-001".to_string(),
            "imagen-3.0-generate-002".to_string(),
        ]);
        let slugs: Vec<&str> = merged.iter().map(|m| m.slug.as_str()).collect();
        assert_eq!(slugs[0], "gemini-4-pro");
        assert_eq!(slugs[1], "gemini-4-pro-preview");
        assert_eq!(slugs[2], "gemini-4-flash");
        assert!(slugs.contains(&"gemini-3.1-pro-preview"));
        assert!(slugs.contains(&"gemini-2.5-flash"));
        assert!(slugs.contains(&"gemini-2.0-flash"));
        assert!(!slugs.contains(&"gemini-embedding-001"));
        assert!(!slugs.contains(&"imagen-3.0-generate-002"));
    }
}
