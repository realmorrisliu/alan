//! OpenRouter usage accounting projection.
//!
//! Requests opt into OpenRouter usage accounting (`usage: {include: true}`), which
//! reports cache reads/writes, reasoning tokens and the billed request cost.

use openrouter_rs::types::ResponseUsage;
use serde_json::{Value, json};

use crate::TokenUsage;

/// Request OpenRouter usage accounting (and the final streaming usage chunk) unless the
/// caller already configured those fields through extra parameters.
pub(super) fn request_usage_accounting(payload: &mut Value, stream: bool) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    object
        .entry("usage")
        .or_insert_with(|| json!({ "include": true }));
    if stream {
        object
            .entry("stream_options")
            .or_insert_with(|| json!({ "include_usage": true }));
    }
}

pub(super) fn convert_usage(usage: ResponseUsage) -> TokenUsage {
    let prompt_details = usage.prompt_tokens_details.as_ref();
    TokenUsage {
        prompt_tokens: saturating_i32(usage.prompt_tokens),
        cached_prompt_tokens: prompt_details
            .and_then(|details| details.cached_tokens)
            .map(saturating_i32),
        completion_tokens: saturating_i32(usage.completion_tokens),
        total_tokens: saturating_i32(usage.total_tokens),
        reasoning_tokens: usage
            .completion_tokens_details
            .as_ref()
            .and_then(|details| details.reasoning_tokens)
            .map(saturating_i32),
        cache_creation_tokens: prompt_details
            .and_then(|details| details.cache_write_tokens)
            .map(saturating_i32),
        cost_usd: request_cost_usd(&usage),
    }
}

/// OpenRouter credits are denominated in USD. For BYOK requests `cost` is only the
/// OpenRouter fee, so the upstream inference cost billed to the user's key is added.
fn request_cost_usd(usage: &ResponseUsage) -> Option<f64> {
    let upstream = (usage.is_byok == Some(true))
        .then(|| {
            usage
                .cost_details
                .as_ref()
                .and_then(|details| details.upstream_inference_cost)
        })
        .flatten();
    match (usage.cost, upstream) {
        (None, None) => None,
        (cost, upstream) => Some(cost.unwrap_or_default() + upstream.unwrap_or_default()),
    }
    .filter(|cost| cost.is_finite() && *cost >= 0.0)
}

fn saturating_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(value: Value) -> ResponseUsage {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn maps_cost_cache_reads_writes_and_reasoning_tokens() {
        let converted = convert_usage(usage(json!({
            "prompt_tokens": 1200,
            "completion_tokens": 300,
            "total_tokens": 1500,
            "prompt_tokens_details": {"cached_tokens": 1000, "cache_write_tokens": 150},
            "completion_tokens_details": {"reasoning_tokens": 120},
            "cost": 0.00123,
            "is_byok": false,
            "cost_details": {"upstream_inference_cost": 0.5}
        })));

        assert_eq!(
            converted,
            TokenUsage {
                prompt_tokens: 1200,
                cached_prompt_tokens: Some(1000),
                completion_tokens: 300,
                total_tokens: 1500,
                reasoning_tokens: Some(120),
                cache_creation_tokens: Some(150),
                cost_usd: Some(0.00123),
            }
        );
    }

    #[test]
    fn byok_cost_includes_upstream_inference_and_absent_fields_stay_unknown() {
        let byok = convert_usage(usage(json!({
            "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2,
            "cost": 0.25, "is_byok": true,
            "cost_details": {"upstream_inference_cost": 0.5}
        })));
        assert_eq!(byok.cost_usd, Some(0.75));

        let bare = convert_usage(usage(json!({
            "prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7
        })));
        assert_eq!(bare.cost_usd, None);
        assert_eq!(bare.cached_prompt_tokens, None);
        assert_eq!(bare.cache_creation_tokens, None);
        assert_eq!(bare.reasoning_tokens, None);
    }

    #[test]
    fn usage_accounting_is_requested_without_overriding_caller_choices() {
        let mut payload = json!({"model": "m"});
        request_usage_accounting(&mut payload, true);
        assert_eq!(payload["usage"], json!({"include": true}));
        assert_eq!(payload["stream_options"], json!({"include_usage": true}));

        let mut payload = json!({"usage": {"include": false}});
        request_usage_accounting(&mut payload, false);
        assert_eq!(payload["usage"], json!({"include": false}));
        assert!(payload.get("stream_options").is_none());
    }
}
