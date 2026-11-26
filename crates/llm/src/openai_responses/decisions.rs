//! Native `POST /v1/decisions` support for OpenAI Responses clients.
//!
//! Targets OpenAI's dedicated calibrated Decisions endpoint with zero output
//! tokens and [`CalibrationKind::Calibrated`] provenance, reusing the OpenAI
//! client's Bearer auth, custom headers, HTTP pool, and endpoint configuration.

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::OpenAiResponsesClient;
use crate::decision::{
    MAX_DECISION_MODEL_OVERRIDE_BYTES, deserialize_unique_map, validate_probability_distribution,
    validate_unit_interval,
};
use crate::{
    CalibrationKind, DecisionAnswer, DecisionQuestion, DecisionRequest, DecisionResponse,
    MalformedDecisionResponse, TokenUsage,
};

pub(super) const MAX_RESPONSE_BYTES: usize = 1 << 20;

impl OpenAiResponsesClient {
    pub(super) fn effective_decision_model<'a>(&'a self, request: &'a DecisionRequest) -> &'a str {
        request
            .model_override
            .as_deref()
            .or(self.decision_model.as_deref())
            .unwrap_or_else(|| self.inner.model())
    }

    pub(super) async fn decide_openai(&self, request: DecisionRequest) -> Result<DecisionResponse> {
        request.validate()?;
        let effective_model = self.effective_decision_model(&request);
        validate_decision_model(effective_model)?;

        let body = build_openai_decision_payload(effective_model, &request)?;
        let url = self.decisions_endpoint_url();
        let req_builder = self.inner.authorize(self.inner.http().post(&url));

        let mut response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("OpenAI decision transport failed"))?;
        ensure!(
            response.status().is_success(),
            "OpenAI decision HTTP {}",
            response.status().as_u16()
        );
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
            .map_err(|_| anyhow::anyhow!("OpenAI decision response read failed"))?
        {
            ensure!(
                chunk.len() <= MAX_RESPONSE_BYTES - bytes.len(),
                MalformedDecisionResponse
            );
            bytes.extend_from_slice(&chunk);
        }

        decode_openai_decision_response(&bytes, &request)
            .map_err(|error| error.context(MalformedDecisionResponse))
    }
}

pub(super) fn validate_decision_model(model: &str) -> Result<()> {
    ensure!(
        !model.trim().is_empty()
            && model.len() <= MAX_DECISION_MODEL_OVERRIDE_BYTES
            && !model.bytes().any(|b| b.is_ascii_control()),
        "invalid OpenAI decision model"
    );
    Ok(())
}

pub(super) fn build_openai_decision_payload(
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
struct RawOpenAiDecisionResponse {
    model: String,
    #[serde(default, deserialize_with = "deserialize_non_null_calibration")]
    calibration: Option<CalibrationKind>,
    #[serde(deserialize_with = "deserialize_unique_map")]
    answers: BTreeMap<String, RawOpenAiAnswer>,
    #[serde(default)]
    usage: Option<RawOpenAiUsage>,
}

#[derive(Deserialize)]
struct RawOpenAiAnswer {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    yes_probability: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_non_null_string")]
    choice: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    score: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    expected_score: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional_unique_map")]
    probabilities: Option<BTreeMap<String, f64>>,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    confidence: Option<f64>,
}

#[derive(Deserialize)]
struct RawOpenAiInputTokensDetails {
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    cached_tokens: Option<i32>,
}

#[derive(Deserialize)]
struct RawOpenAiOutputTokensDetails {
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    reasoning_tokens: Option<i32>,
}

#[derive(Deserialize)]
struct RawOpenAiUsage {
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    input_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    prompt_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    output_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    completion_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    total_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    cached_prompt_tokens: Option<i32>,
    #[serde(default)]
    input_tokens_details: Option<RawOpenAiInputTokensDetails>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    reasoning_tokens: Option<i32>,
    #[serde(default)]
    output_tokens_details: Option<RawOpenAiOutputTokensDetails>,
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

pub(super) fn decode_openai_decision_response(
    bytes: &[u8],
    request: &DecisionRequest,
) -> Result<DecisionResponse> {
    let mut raw: RawOpenAiDecisionResponse = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("invalid OpenAI decision response"))?;
    validate_decision_model(&raw.model)?;

    let calibration = raw.calibration.unwrap_or(CalibrationKind::Calibrated);
    ensure!(
        calibration == CalibrationKind::Calibrated,
        "native OpenAI decisions endpoint must report calibrated provenance"
    );

    ensure!(
        raw.answers.len() == request.questions.len(),
        "OpenAI decision answer count mismatch"
    );

    let mut answers = BTreeMap::new();
    for (question_id, question) in &request.questions {
        let raw_answer = raw
            .answers
            .remove(question_id)
            .ok_or_else(|| anyhow::anyhow!("missing OpenAI answer for '{question_id}'"))?;
        let decoded = decode_openai_answer(question_id, question, raw_answer)?;
        answers.insert(question_id.clone(), decoded);
    }

    let usage = raw.usage.map(decode_openai_usage).transpose()?;

    let response = DecisionResponse {
        model: raw.model,
        provider: "openai_responses".to_string(),
        calibration: CalibrationKind::Calibrated,
        answers,
        usage,
    };

    request.validate_response(&response)?;
    Ok(response)
}

fn decode_openai_usage(raw: RawOpenAiUsage) -> Result<TokenUsage> {
    let prompt_tokens = match (raw.input_tokens, raw.prompt_tokens) {
        (Some(i), Some(p)) => {
            ensure!(
                i == p,
                "contradictory OpenAI input_tokens and prompt_tokens"
            );
            i
        }
        (Some(i), None) => i,
        (None, Some(p)) => p,
        (None, None) => bail!("OpenAI decision usage missing input/prompt tokens"),
    };
    let completion_tokens = match (raw.output_tokens, raw.completion_tokens) {
        (Some(o), Some(c)) => {
            ensure!(
                o == c,
                "contradictory OpenAI output_tokens and completion_tokens"
            );
            o
        }
        (Some(o), None) => o,
        (None, Some(c)) => c,
        (None, None) => 0,
    };
    ensure!(prompt_tokens >= 0, "invalid OpenAI input token usage");
    ensure!(
        completion_tokens == 0,
        "native OpenAI decisions must report zero output tokens"
    );
    if let Some(total) = raw.total_tokens {
        ensure!(
            total == prompt_tokens,
            "OpenAI total_tokens disagrees with input_tokens + 0 output tokens"
        );
    }

    let detailed_cached = raw
        .input_tokens_details
        .and_then(|details| details.cached_tokens);
    let cached_prompt_tokens = match (raw.cached_prompt_tokens, detailed_cached) {
        (Some(a), Some(b)) => {
            ensure!(
                a == b,
                "contradictory OpenAI cached_prompt_tokens and input_tokens_details.cached_tokens"
            );
            Some(a)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let Some(cached) = cached_prompt_tokens {
        ensure!(cached >= 0, "invalid OpenAI cached token usage");
    }

    let detailed_reasoning = raw
        .output_tokens_details
        .and_then(|details| details.reasoning_tokens);
    let reasoning_tokens = match (raw.reasoning_tokens, detailed_reasoning) {
        (Some(a), Some(b)) => {
            ensure!(
                a == b,
                "contradictory OpenAI reasoning_tokens and output_tokens_details.reasoning_tokens"
            );
            Some(a)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let Some(reasoning) = reasoning_tokens {
        ensure!(
            reasoning == 0,
            "native OpenAI decisions must report zero reasoning_tokens"
        );
    }

    let cost_usd = match (raw.cost_usd, raw.cost) {
        (Some(a), Some(b)) => {
            ensure!(
                a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-12,
                "contradictory OpenAI usage cost fields"
            );
            Some(a)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let Some(cost) = cost_usd {
        ensure!(cost.is_finite() && cost >= 0.0, "invalid OpenAI usage cost");
    }

    Ok(TokenUsage {
        prompt_tokens,
        completion_tokens: 0,
        total_tokens: prompt_tokens,
        cached_prompt_tokens,
        reasoning_tokens,
        cost_usd,
        ..Default::default()
    })
}

fn decode_openai_answer(
    question_id: &str,
    question: &DecisionQuestion,
    raw: RawOpenAiAnswer,
) -> Result<DecisionAnswer> {
    match question {
        DecisionQuestion::Predicate { .. } => {
            ensure!(
                raw.kind == "predicate",
                "OpenAI predicate '{question_id}' requires `predicate` answer"
            );
            ensure!(
                raw.choice.is_none() && raw.score.is_none() && raw.expected_score.is_none(),
                "OpenAI predicate '{question_id}' contains non-predicate fields"
            );
            if let Some(ref probs) = raw.probabilities {
                ensure!(
                    probs.len() == 2 && probs.contains_key("true") && probs.contains_key("false"),
                    "OpenAI predicate probabilities for '{question_id}' must match `true`/`false`"
                );
                validate_probability_distribution(probs)?;
            }
            let yes_probability = match (raw.yes_probability, raw.probabilities.as_ref()) {
                (Some(prob), Some(probs)) => {
                    validate_unit_interval("yes_probability", prob)?;
                    ensure!(
                        (prob - probs["true"]).abs() <= 0.01,
                        "OpenAI predicate '{question_id}' disagrees with `true` probability"
                    );
                    prob
                }
                (Some(prob), None) => {
                    validate_unit_interval("yes_probability", prob)?;
                    prob
                }
                (None, Some(probs)) => probs["true"],
                (None, None) => bail!("OpenAI predicate '{question_id}' missing probability"),
            };
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("OpenAI predicate '{question_id}' missing confidence")
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
                "OpenAI choice '{question_id}' requires `choice` answer"
            );
            ensure!(
                raw.yes_probability.is_none()
                    && raw.score.is_none()
                    && raw.expected_score.is_none(),
                "OpenAI choice '{question_id}' contains non-choice fields"
            );
            let choice = raw
                .choice
                .ok_or_else(|| anyhow::anyhow!("OpenAI choice missing for '{question_id}'"))?;
            let probabilities = raw.probabilities.ok_or_else(|| {
                anyhow::anyhow!("OpenAI choice probabilities missing for '{question_id}'")
            })?;
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("OpenAI choice confidence missing for '{question_id}'")
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
                "OpenAI score '{question_id}' requires `score` answer"
            );
            ensure!(
                raw.yes_probability.is_none() && raw.choice.is_none(),
                "OpenAI score '{question_id}' contains non-score fields"
            );
            let probabilities = raw.probabilities.ok_or_else(|| {
                anyhow::anyhow!("OpenAI score probabilities missing for '{question_id}'")
            })?;
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("OpenAI score confidence missing for '{question_id}'")
            })?;
            validate_unit_interval("confidence", confidence)?;
            let expected_score = match (raw.expected_score, raw.score) {
                (Some(es), Some(s)) => {
                    ensure!(
                        es.is_finite() && s.is_finite() && (es - s).abs() <= 1e-6,
                        "OpenAI contradictory score values for '{question_id}'"
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
