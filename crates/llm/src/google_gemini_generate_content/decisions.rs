//! Native `POST /v1beta/decisions` support for Google Gemini / Vertex AI.
//!
//! Targets dedicated calibrated decision models (such as `gemini-decision-1`)
//! with zero output tokens and [`CalibrationKind::Calibrated`] provenance,
//! reusing Vertex AI Express Mode (`x-goog-api-key`) and OAuth/ADC
//! (`Authorization: Bearer` + `x-goog-user-project` with single 401 refresh)
//! transport from [`super::endpoint`].

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::GoogleGeminiGenerateContentClient;
use crate::decision::{
    MAX_DECISION_MODEL_OVERRIDE_BYTES, deserialize_unique_map, validate_probability_distribution,
    validate_unit_interval,
};
use crate::{
    CalibrationKind, DecisionAnswer, DecisionQuestion, DecisionRequest, DecisionResponse,
    MalformedDecisionResponse, TokenUsage,
};

/// Default dedicated Vertex AI / Gemini calibrated decision model identifier.
pub const DEFAULT_GEMINI_DECISION_MODEL: &str = "gemini-decision-1";

pub(super) const MAX_RESPONSE_BYTES: usize = 1 << 20;

impl GoogleGeminiGenerateContentClient {
    pub(super) fn effective_decision_model<'a>(&'a self, request: &'a DecisionRequest) -> &'a str {
        request
            .model_override
            .as_deref()
            .or(self.decision_model.as_deref())
            .unwrap_or(DEFAULT_GEMINI_DECISION_MODEL)
    }

    pub(super) async fn decide_gemini(&self, request: DecisionRequest) -> Result<DecisionResponse> {
        request.validate()?;
        let effective_model = self.effective_decision_model(&request);
        validate_decision_model(effective_model)?;

        let body = build_gemini_decision_payload(effective_model, &request)?;
        let mut response = self.transport.post_decisions(&body).await?;

        ensure!(
            response
                .content_length()
                .is_none_or(|n| n <= MAX_RESPONSE_BYTES as u64),
            MalformedDecisionResponse
        );

        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Vertex AI decision response read failed"))?
        {
            ensure!(
                chunk.len() <= MAX_RESPONSE_BYTES - bytes.len(),
                MalformedDecisionResponse
            );
            bytes.extend_from_slice(&chunk);
        }

        decode_gemini_decision_response(&bytes, &request)
            .map_err(|error| error.context(MalformedDecisionResponse))
    }
}

pub(super) fn validate_decision_model(model: &str) -> Result<()> {
    ensure!(
        !model.trim().is_empty()
            && model.len() <= MAX_DECISION_MODEL_OVERRIDE_BYTES
            && !model.bytes().any(|b| b.is_ascii_control()),
        "invalid Gemini decision model"
    );
    Ok(())
}

pub(super) fn build_gemini_decision_payload(
    model: &str,
    request: &DecisionRequest,
) -> Result<Value> {
    request.validate()?;
    validate_decision_model(model)?;

    let mut payload = Map::new();
    payload.insert("model".to_string(), Value::String(model.to_string()));
    payload.insert("state".to_string(), request.state.clone());
    payload.insert(
        "questions".to_string(),
        serde_json::to_value(&request.questions)?,
    );
    if let Some(instructions) = &request.instructions {
        payload.insert(
            "instructions".to_string(),
            Value::String(instructions.clone()),
        );
    }
    Ok(Value::Object(payload))
}

#[derive(Deserialize)]
struct RawGeminiDecisionResponse {
    model: String,
    #[serde(default, deserialize_with = "deserialize_non_null_calibration")]
    calibration: Option<CalibrationKind>,
    #[serde(deserialize_with = "deserialize_unique_map")]
    answers: BTreeMap<String, RawGeminiAnswer>,
    #[serde(default, rename = "usageMetadata")]
    usage_metadata: Option<RawGeminiUsageMetadata>,
    #[serde(default)]
    usage: Option<RawGeminiSnakeUsage>,
}

#[derive(Deserialize)]
struct RawGeminiAnswer {
    #[serde(rename = "type")]
    kind: String,
    #[serde(
        default,
        alias = "yesProbability",
        deserialize_with = "deserialize_non_null_f64"
    )]
    yes_probability: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_non_null_string")]
    choice: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    score: Option<f64>,
    #[serde(
        default,
        alias = "expectedScore",
        deserialize_with = "deserialize_non_null_f64"
    )]
    expected_score: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional_unique_map")]
    probabilities: Option<BTreeMap<String, f64>>,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    confidence: Option<f64>,
}

#[derive(Deserialize)]
struct RawGeminiUsageMetadata {
    #[serde(
        default,
        rename = "promptTokenCount",
        deserialize_with = "deserialize_non_null_i32"
    )]
    prompt_token_count: Option<i32>,
    #[serde(
        default,
        rename = "candidatesTokenCount",
        deserialize_with = "deserialize_non_null_i32"
    )]
    candidates_token_count: Option<i32>,
    #[serde(
        default,
        rename = "totalTokenCount",
        deserialize_with = "deserialize_non_null_i32"
    )]
    total_token_count: Option<i32>,
    #[serde(
        default,
        rename = "cachedContentTokenCount",
        deserialize_with = "deserialize_non_null_i32"
    )]
    cached_content_token_count: Option<i32>,
    #[serde(
        default,
        rename = "thoughtsTokenCount",
        deserialize_with = "deserialize_non_null_i32"
    )]
    thoughts_token_count: Option<i32>,
}

#[derive(Deserialize)]
struct RawGeminiSnakeUsage {
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    prompt_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    input_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    completion_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    output_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    total_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    cached_prompt_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    reasoning_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    cost_usd: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    cost: Option<f64>,
}

fn deserialize_non_null_calibration<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<CalibrationKind>, D::Error>
where
    D: Deserializer<'de>,
{
    CalibrationKind::deserialize(deserializer).map(Some)
}

fn deserialize_non_null_f64<'de, D>(deserializer: D) -> std::result::Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    f64::deserialize(deserializer).map(Some)
}

fn deserialize_non_null_i32<'de, D>(deserializer: D) -> std::result::Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    i32::deserialize(deserializer).map(Some)
}

fn deserialize_non_null_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer).map(Some)
}

fn deserialize_optional_unique_map<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<BTreeMap<String, f64>>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_unique_map(deserializer).map(Some)
}

pub(super) fn decode_gemini_decision_response(
    bytes: &[u8],
    request: &DecisionRequest,
) -> Result<DecisionResponse> {
    let mut raw: RawGeminiDecisionResponse = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("invalid Gemini decision response"))?;
    validate_decision_model(&raw.model)?;

    let calibration = raw.calibration.unwrap_or(CalibrationKind::Calibrated);
    ensure!(
        calibration == CalibrationKind::Calibrated,
        "native Gemini decisions endpoint must report calibrated provenance"
    );

    ensure!(
        raw.answers.len() == request.questions.len(),
        "Gemini decision answer count mismatch"
    );

    let mut answers = BTreeMap::new();
    for (question_id, question) in &request.questions {
        let raw_answer = raw
            .answers
            .remove(question_id)
            .ok_or_else(|| anyhow::anyhow!("missing Gemini answer for '{question_id}'"))?;
        let decoded = decode_gemini_answer(question_id, question, raw_answer)?;
        answers.insert(question_id.clone(), decoded);
    }

    let usage = match (raw.usage_metadata, raw.usage) {
        (Some(_), Some(_)) => {
            bail!("Gemini decision response must not include both `usageMetadata` and `usage`")
        }
        (Some(meta), None) => Some(decode_gemini_usage_metadata(meta)?),
        (None, Some(usage)) => Some(decode_gemini_snake_usage(usage)?),
        (None, None) => None,
    };

    let response = DecisionResponse {
        model: raw.model,
        provider: "google_gemini_generate_content".to_string(),
        calibration: CalibrationKind::Calibrated,
        answers,
        usage,
    };

    request.validate_response(&response)?;
    Ok(response)
}

fn decode_gemini_usage_metadata(raw: RawGeminiUsageMetadata) -> Result<TokenUsage> {
    let prompt_tokens = raw
        .prompt_token_count
        .ok_or_else(|| anyhow::anyhow!("Gemini usageMetadata missing promptTokenCount"))?;
    let completion_tokens = raw.candidates_token_count.unwrap_or(0);
    ensure!(prompt_tokens >= 0, "invalid Gemini promptTokenCount");
    ensure!(
        completion_tokens == 0,
        "native Gemini decisions must report zero output tokens"
    );
    if let Some(total) = raw.total_token_count {
        ensure!(
            total == prompt_tokens,
            "Gemini totalTokenCount disagrees with promptTokenCount + 0 output tokens"
        );
    }
    if let Some(cached) = raw.cached_content_token_count {
        ensure!(cached >= 0, "invalid Gemini cachedContentTokenCount");
    }
    if let Some(thoughts) = raw.thoughts_token_count {
        ensure!(
            thoughts == 0,
            "native Gemini decisions must report zero thoughtsTokenCount"
        );
    }

    Ok(TokenUsage {
        prompt_tokens,
        completion_tokens: 0,
        total_tokens: prompt_tokens,
        cached_prompt_tokens: raw.cached_content_token_count,
        reasoning_tokens: raw.thoughts_token_count,
        ..Default::default()
    })
}

fn decode_gemini_snake_usage(raw: RawGeminiSnakeUsage) -> Result<TokenUsage> {
    let prompt_tokens = match (raw.prompt_tokens, raw.input_tokens) {
        (Some(p), Some(i)) => {
            ensure!(
                p == i,
                "contradictory Gemini prompt_tokens and input_tokens"
            );
            p
        }
        (Some(p), None) => p,
        (None, Some(i)) => i,
        (None, None) => bail!("Gemini usage missing prompt/input tokens"),
    };
    let completion_tokens = match (raw.completion_tokens, raw.output_tokens) {
        (Some(c), Some(o)) => {
            ensure!(
                c == o,
                "contradictory Gemini completion_tokens and output_tokens"
            );
            c
        }
        (Some(c), None) => c,
        (None, Some(o)) => o,
        (None, None) => 0,
    };
    ensure!(prompt_tokens >= 0, "invalid Gemini prompt token usage");
    ensure!(
        completion_tokens == 0,
        "native Gemini decisions must report zero output tokens"
    );
    if let Some(total) = raw.total_tokens {
        ensure!(
            total == prompt_tokens,
            "Gemini total_tokens disagrees with prompt_tokens + 0 output tokens"
        );
    }
    if let Some(cached) = raw.cached_prompt_tokens {
        ensure!(cached >= 0, "invalid Gemini cached_prompt_tokens");
    }
    if let Some(reasoning) = raw.reasoning_tokens {
        ensure!(
            reasoning == 0,
            "native Gemini decisions must report zero reasoning_tokens"
        );
    }
    let cost_usd = match (raw.cost_usd, raw.cost) {
        (Some(a), Some(b)) => {
            ensure!(
                a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-12,
                "contradictory Gemini usage cost fields"
            );
            Some(a)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let Some(cost) = cost_usd {
        ensure!(cost.is_finite() && cost >= 0.0, "invalid Gemini usage cost");
    }

    Ok(TokenUsage {
        prompt_tokens,
        completion_tokens: 0,
        total_tokens: prompt_tokens,
        cached_prompt_tokens: raw.cached_prompt_tokens,
        reasoning_tokens: raw.reasoning_tokens,
        cost_usd,
        ..Default::default()
    })
}

fn decode_gemini_answer(
    question_id: &str,
    question: &DecisionQuestion,
    raw: RawGeminiAnswer,
) -> Result<DecisionAnswer> {
    match question {
        DecisionQuestion::Predicate { .. } => {
            ensure!(
                raw.kind == "predicate",
                "Gemini predicate '{question_id}' requires `predicate` answer"
            );
            ensure!(
                raw.choice.is_none() && raw.score.is_none() && raw.expected_score.is_none(),
                "Gemini predicate '{question_id}' contains non-predicate fields"
            );
            if let Some(ref probs) = raw.probabilities {
                ensure!(
                    probs.len() == 2 && probs.contains_key("true") && probs.contains_key("false"),
                    "Gemini predicate probabilities for '{question_id}' must match `true`/`false`"
                );
                validate_probability_distribution(probs)?;
            }
            let yes_probability = match (raw.yes_probability, raw.probabilities.as_ref()) {
                (Some(prob), Some(probs)) => {
                    validate_unit_interval("yes_probability", prob)?;
                    ensure!(
                        (prob - probs["true"]).abs() <= 0.01,
                        "Gemini predicate '{question_id}' disagrees with `true` probability"
                    );
                    prob
                }
                (Some(prob), None) => {
                    validate_unit_interval("yes_probability", prob)?;
                    prob
                }
                (None, Some(probs)) => probs["true"],
                (None, None) => bail!("Gemini predicate '{question_id}' missing probability"),
            };
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("Gemini predicate '{question_id}' missing confidence")
            })?;
            validate_unit_interval("confidence", confidence)?;
            Ok(DecisionAnswer::Predicate {
                yes_probability,
                confidence,
            })
        }
        DecisionQuestion::Choice { .. } => {
            ensure!(
                raw.kind == "choice",
                "Gemini choice '{question_id}' requires `choice` answer"
            );
            ensure!(
                raw.yes_probability.is_none()
                    && raw.score.is_none()
                    && raw.expected_score.is_none(),
                "Gemini choice '{question_id}' contains non-choice fields"
            );
            let choice = raw
                .choice
                .ok_or_else(|| anyhow::anyhow!("Gemini choice missing for '{question_id}'"))?;
            let probabilities = raw.probabilities.ok_or_else(|| {
                anyhow::anyhow!("Gemini choice probabilities missing for '{question_id}'")
            })?;
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("Gemini choice confidence missing for '{question_id}'")
            })?;
            validate_unit_interval("confidence", confidence)?;
            Ok(DecisionAnswer::Choice {
                choice,
                probabilities,
                confidence,
            })
        }
        DecisionQuestion::Score { levels, .. } => {
            ensure!(
                raw.kind == "score",
                "Gemini score '{question_id}' requires `score` answer"
            );
            ensure!(
                raw.yes_probability.is_none() && raw.choice.is_none(),
                "Gemini score '{question_id}' contains non-score fields"
            );
            let probabilities = raw.probabilities.ok_or_else(|| {
                anyhow::anyhow!("Gemini score probabilities missing for '{question_id}'")
            })?;
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("Gemini score confidence missing for '{question_id}'")
            })?;
            validate_unit_interval("confidence", confidence)?;
            let expected_score = match (raw.expected_score, raw.score) {
                (Some(es), Some(s)) => {
                    ensure!(
                        es.is_finite() && s.is_finite() && (es - s).abs() <= 1e-6,
                        "Gemini contradictory score values for '{question_id}'"
                    );
                    es
                }
                (Some(es), None) => es,
                (None, Some(s)) => s,
                (None, None) => levels
                    .iter()
                    .filter_map(|level| probabilities.get(&level.id).map(|p| *p * level.score))
                    .sum(),
            };
            Ok(DecisionAnswer::Score {
                expected_score,
                probabilities,
                confidence,
            })
        }
    }
}

#[cfg(test)]
#[path = "decisions_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "decisions_http_tests.rs"]
mod http_tests;
