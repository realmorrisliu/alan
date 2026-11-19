//! Projection of Alan extra parameters onto the OpenRouter chat request builder.

use anyhow::{Context, Result, bail};
use openrouter_rs::{
    api::chat::{
        ChatCompletionRequestBuilder, DebugOptions, Modality, Plugin, StopSequence, StreamOptions,
        TraceOptions,
    },
    types::{Effort, ProviderPreferences, ReasoningConfig, ResponseFormat},
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;

use crate::ReasoningEffort;

pub(super) fn apply_openrouter_extra_params(
    builder: &mut ChatCompletionRequestBuilder,
    extra_params: HashMap<String, Value>,
) -> Result<()> {
    for (key, value) in extra_params {
        match key.as_str() {
            "max_completion_tokens" => builder.max_completion_tokens(value_as_u32(&key, value)?),
            "seed" => builder.seed(value_as_u32(&key, value)?),
            "top_p" => builder.top_p(value_as_f64(&key, value)?),
            "top_k" => builder.top_k(value_as_u32(&key, value)?),
            "frequency_penalty" => builder.frequency_penalty(value_as_f64(&key, value)?),
            "presence_penalty" => builder.presence_penalty(value_as_f64(&key, value)?),
            "repetition_penalty" => builder.repetition_penalty(value_as_f64(&key, value)?),
            "logit_bias" => builder.logit_bias(value_as::<HashMap<String, f64>>(&key, value)?),
            "logprobs" => builder.logprobs(value_as_bool(&key, value)?),
            "top_logprobs" => builder.top_logprobs(value_as_u32(&key, value)?),
            "min_p" => builder.min_p(value_as_f64(&key, value)?),
            "top_a" => builder.top_a(value_as_f64(&key, value)?),
            "transforms" => builder.transforms(value_as::<Vec<String>>(&key, value)?),
            "models" => builder.models(value_as::<Vec<String>>(&key, value)?),
            "route" => builder.route(value_as_string(&key, value)?),
            "user" => builder.user(value_as_string(&key, value)?),
            "session_id" => builder.session_id(value_as_string(&key, value)?),
            "trace" => builder.trace(value_as::<TraceOptions>(&key, value)?),
            "provider" => builder.provider(value_as::<ProviderPreferences>(&key, value)?),
            "metadata" => builder.metadata(value_as::<HashMap<String, String>>(&key, value)?),
            "plugins" => builder.plugins(value_as::<Vec<Plugin>>(&key, value)?),
            "modalities" => builder.modalities(value_as::<Vec<Modality>>(&key, value)?),
            "image_config" => {
                builder.image_config(value_as::<HashMap<String, Value>>(&key, value)?)
            }
            "response_format" => builder.response_format(value_as::<ResponseFormat>(&key, value)?),
            "reasoning" => builder.reasoning(value_as::<ReasoningConfig>(&key, value)?),
            "reasoning_effort" => builder.reasoning_effort(effort_from_value(&key, value)?),
            "reasoning_max_tokens" => builder.reasoning_max_tokens(value_as_u32(&key, value)?),
            "include_reasoning" => builder.include_reasoning(value_as_bool(&key, value)?),
            "stop" => builder.stop(value_as::<StopSequence>(&key, value)?),
            "stream_options" => builder.stream_options(value_as::<StreamOptions>(&key, value)?),
            "debug" => builder.debug(value_as::<DebugOptions>(&key, value)?),
            "parallel_tool_calls" => builder.parallel_tool_calls(value_as_bool(&key, value)?),
            unsupported => {
                bail!("OpenRouter provider does not support extra parameter `{unsupported}`");
            }
        };
    }
    Ok(())
}

pub(super) fn non_negative_u32(label: &str, value: i32) -> Result<u32> {
    u32::try_from(value).with_context(|| format!("`{label}` must be a non-negative integer"))
}

fn value_as<T>(key: &str, value: Value) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).with_context(|| {
        format!("OpenRouter extra parameter `{key}` has an unsupported value shape")
    })
}

fn value_as_string(key: &str, value: Value) -> Result<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("OpenRouter extra parameter `{key}` must be a string"))
}

fn value_as_bool(key: &str, value: Value) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter extra parameter `{key}` must be a boolean"))
}

fn value_as_u32(key: &str, value: Value) -> Result<u32> {
    match value.as_u64().and_then(|value| u32::try_from(value).ok()) {
        Some(value) => Ok(value),
        None => bail!("OpenRouter extra parameter `{key}` must be an unsigned 32-bit integer"),
    }
}

fn value_as_f64(key: &str, value: Value) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter extra parameter `{key}` must be a number"))
}

fn effort_from_value(key: &str, value: Value) -> Result<Effort> {
    let raw = value_as_string(key, value)?;
    match raw.as_str() {
        "xhigh" => Ok(Effort::Xhigh),
        "high" => Ok(Effort::High),
        "medium" => Ok(Effort::Medium),
        "low" => Ok(Effort::Low),
        "minimal" => Ok(Effort::Minimal),
        "none" => Ok(Effort::None),
        _ => bail!("OpenRouter extra parameter `{key}` has unsupported reasoning effort `{raw}`"),
    }
}

pub(super) fn openrouter_effort(effort: ReasoningEffort) -> Result<Effort> {
    Ok(match effort {
        ReasoningEffort::None => Effort::None,
        ReasoningEffort::Minimal => Effort::Minimal,
        ReasoningEffort::Low => Effort::Low,
        ReasoningEffort::Medium => Effort::Medium,
        ReasoningEffort::High => Effort::High,
        ReasoningEffort::XHigh => Effort::Xhigh,
        ReasoningEffort::Max | ReasoningEffort::Ultra => {
            anyhow::bail!("Provider does not support reasoning effort `{effort}`")
        }
    })
}
