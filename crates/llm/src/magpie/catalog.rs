//! Magpie `/v1/models` discovery and `thinkingLevelMap` parsing.

use anyhow::Result;
use serde_json::Value;

use crate::openai_chat_completions::ReasoningEffortMap;
use crate::{ProviderModel, ReasoningEffort};

/// Conservative window used when a router does not advertise one.
pub(super) const DEFAULT_CONTEXT_WINDOW_TOKENS: u32 = 128_000;

const CONTEXT_WINDOW_KEYS: [&str; 6] = [
    "context_window",
    "contextWindow",
    "context_length",
    "max_context_length",
    "max_model_len",
    "n_ctx",
];

/// One model advertised by a Magpie router.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MagpieModel {
    pub slug: String,
    pub context_window_tokens: u32,
    pub reasoning_effort_map: Option<ReasoningEffortMap>,
    pub default_reasoning_effort: Option<ReasoningEffort>,
}

impl MagpieModel {
    pub(super) fn into_provider_model(self) -> ProviderModel {
        let supported_reasoning_efforts = self
            .reasoning_effort_map
            .as_ref()
            .map(|map| {
                ReasoningEffort::VALUES
                    .into_iter()
                    .filter(|effort| map.get(effort.as_str()).is_some_and(Option::is_some))
                    .collect()
            })
            .unwrap_or_default();
        ProviderModel {
            slug: self.slug,
            context_window_tokens: self.context_window_tokens,
            supported_reasoning_efforts,
            default_reasoning_effort: self.default_reasoning_effort,
        }
    }
}

/// Parse an OpenAI-style `{"data": [...]}` model list (a bare array is accepted).
pub(super) fn parse_models_response(value: &Value) -> Result<Vec<MagpieModel>> {
    let entries = value
        .get("data")
        .or_else(|| value.get("models"))
        .unwrap_or(value)
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Magpie model list is not an array"))?;
    Ok(entries.iter().filter_map(parse_model).collect())
}

fn parse_model(entry: &Value) -> Option<MagpieModel> {
    let slug = entry
        .get("id")
        .or_else(|| entry.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|slug| !slug.is_empty())?
        .to_string();
    let context_window_tokens = context_window(entry)
        .or_else(|| entry.get("meta").and_then(context_window))
        .unwrap_or(DEFAULT_CONTEXT_WINDOW_TOKENS);
    let reasoning_effort_map = entry
        .get("thinkingLevelMap")
        .or_else(|| entry.get("thinking_level_map"))
        .and_then(parse_reasoning_effort_map);
    let default_reasoning_effort = entry
        .get("defaultThinkingLevel")
        .or_else(|| entry.get("default_reasoning_effort"))
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    Some(MagpieModel {
        slug,
        context_window_tokens,
        reasoning_effort_map,
        default_reasoning_effort,
    })
}

fn context_window(value: &Value) -> Option<u32> {
    CONTEXT_WINDOW_KEYS.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_u64)
            .filter(|tokens| *tokens > 0)
            .map(|tokens| u32::try_from(tokens).unwrap_or(u32::MAX))
    })
}

pub(crate) use crate::provider_meta::parse_reasoning_effort_map;
