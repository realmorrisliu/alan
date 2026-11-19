//! Vertex AI Gemini model catalog and per-model thinking / tool-streaming capabilities.

use anyhow::Result;

use crate::{ProviderModel, ReasoningEffort};

use super::wire::ThinkingConfig;

const ONE_MILLION_CONTEXT: u32 = 1_048_576;

/// Known Vertex AI Gemini models, context windows, and reasoning effort controls.
pub(super) fn vertex_model_catalog() -> Vec<ProviderModel> {
    vec![
        ProviderModel {
            slug: "gemini-2.5-pro".to_string(),
            context_window_tokens: ONE_MILLION_CONTEXT,
            supported_reasoning_efforts: vec![
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
            ],
            default_reasoning_effort: Some(ReasoningEffort::High),
        },
        ProviderModel {
            slug: "gemini-2.5-flash".to_string(),
            context_window_tokens: ONE_MILLION_CONTEXT,
            supported_reasoning_efforts: vec![
                ReasoningEffort::None,
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
            ],
            default_reasoning_effort: Some(ReasoningEffort::Medium),
        },
        ProviderModel {
            slug: "gemini-2.5-flash-lite".to_string(),
            context_window_tokens: ONE_MILLION_CONTEXT,
            supported_reasoning_efforts: vec![
                ReasoningEffort::None,
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
            ],
            default_reasoning_effort: Some(ReasoningEffort::None),
        },
        ProviderModel {
            slug: "gemini-3-pro-preview".to_string(),
            context_window_tokens: ONE_MILLION_CONTEXT,
            supported_reasoning_efforts: vec![ReasoningEffort::Low, ReasoningEffort::High],
            default_reasoning_effort: Some(ReasoningEffort::High),
        },
        ProviderModel {
            slug: "gemini-3-flash-preview".to_string(),
            context_window_tokens: ONE_MILLION_CONTEXT,
            supported_reasoning_efforts: vec![
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
            ],
            default_reasoning_effort: Some(ReasoningEffort::Medium),
        },
        ProviderModel {
            slug: "gemini-3.1-pro-preview".to_string(),
            context_window_tokens: ONE_MILLION_CONTEXT,
            supported_reasoning_efforts: vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
            ],
            default_reasoning_effort: Some(ReasoningEffort::High),
        },
        ProviderModel {
            slug: "gemini-3.1-flash-lite-preview".to_string(),
            context_window_tokens: ONE_MILLION_CONTEXT,
            supported_reasoning_efforts: vec![
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
            ],
            default_reasoning_effort: Some(ReasoningEffort::Low),
        },
    ]
}

/// Whether Vertex AI supports `streamFunctionCallArguments` for `model`.
pub(super) fn supports_stream_function_call_arguments(model: &str) -> bool {
    model.to_ascii_lowercase().contains("gemini-3")
}

/// Map canonical [`ReasoningEffort`] to Vertex AI `thinkingConfig`.
///
/// Sets `includeThoughts: true` whenever thinking is active so thought parts
/// stream back as `thought: true` summaries.
pub(super) fn build_gemini_thinking_config(
    model: &str,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Option<ThinkingConfig>> {
    let model = model.to_ascii_lowercase();
    if model.contains("gemini-3") {
        let Some(effort) = reasoning_effort else {
            return Ok(Some(ThinkingConfig {
                include_thoughts: Some(true),
                thinking_level: None,
                thinking_budget: None,
            }));
        };
        let is_flash = model.contains("flash");
        let is_3_1 = model.contains("gemini-3.1");
        let thinking_level = match effort {
            ReasoningEffort::None => {
                anyhow::bail!("Gemini 3 does not support disabling thinking with effort `none`")
            }
            ReasoningEffort::Minimal if is_flash => "minimal",
            ReasoningEffort::Minimal => {
                anyhow::bail!("Gemini 3 Pro does not support reasoning effort `minimal`")
            }
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium if is_flash || is_3_1 => "medium",
            ReasoningEffort::Medium => {
                anyhow::bail!("Gemini 3 Pro supports reasoning efforts `low` and `high`")
            }
            ReasoningEffort::High => "high",
            ReasoningEffort::XHigh | ReasoningEffort::Max | ReasoningEffort::Ultra => {
                anyhow::bail!("Gemini 3 does not support reasoning effort `{effort}`")
            }
        };
        return Ok(Some(ThinkingConfig {
            include_thoughts: Some(true),
            thinking_level: Some(thinking_level.to_string()),
            thinking_budget: None,
        }));
    }

    if model.contains("gemini-2.5") {
        return match reasoning_effort {
            Some(ReasoningEffort::None) if model.contains("pro") => {
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
            None if model.contains("flash-lite") => Ok(None),
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
            model
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

    #[test]
    fn catalog_includes_gemini_25_and_gemini_3_models_with_valid_efforts() {
        let catalog = vertex_model_catalog();
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
            assert!(model.context_window_tokens >= ONE_MILLION_CONTEXT);
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
}
