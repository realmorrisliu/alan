//! Native `POST /v1/decisions` passthrough and opt-in chat fallback for Magpie.

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::{MagpieClient, session_headers};
use crate::decision::{
    MAX_DECISION_MODEL_OVERRIDE_BYTES, deserialize_unique_map, validate_probability_distribution,
    validate_unit_interval,
};
use crate::{
    CalibrationKind, DecisionAnswer, DecisionQuestion, DecisionRequest, DecisionResponse,
    MalformedDecisionResponse, TokenUsage,
};

pub(super) const MAX_RESPONSE_BYTES: usize = 1 << 20;

/// Routing mode used by [`MagpieClient`] for [`crate::LlmProvider::decide`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MagpieDecisionMode {
    /// Route `decide()` to Magpie's native `POST {base_url}/decisions` endpoint
    /// (preserving native [`CalibrationKind::Calibrated`] verdicts).
    #[default]
    Native,
    /// Opt-in structured-generation fallback over `/v1/chat/completions` via
    /// [`crate::decide_via_generation`]. Always reports [`CalibrationKind::Uncalibrated`].
    ChatFallback,
}

impl MagpieClient {
    pub(super) async fn decide_magpie(
        &mut self,
        request: DecisionRequest,
    ) -> Result<DecisionResponse> {
        match self.decision_mode {
            MagpieDecisionMode::Native => self.decide_native(request).await,
            MagpieDecisionMode::ChatFallback => {
                let model = self.inner.model().to_string();
                crate::decide_via_generation(self, &model, request).await
            }
        }
    }

    pub(super) fn decision_url(&self) -> String {
        if let Some(endpoint) = &self.decision_endpoint {
            endpoint.clone()
        } else {
            format!("{}/decisions", self.inner.base_url().trim_end_matches('/'))
        }
    }

    pub(super) fn effective_decision_model<'a>(&'a self, request: &'a DecisionRequest) -> &'a str {
        request
            .model_override
            .as_deref()
            .or(self.decision_model.as_deref())
            .unwrap_or_else(|| self.inner.model())
    }

    async fn decide_native(&self, request: DecisionRequest) -> Result<DecisionResponse> {
        request.validate()?;
        let effective_model = self.effective_decision_model(&request);
        validate_decision_model(effective_model)?;

        let body = build_magpie_decision_payload(effective_model, &request)?;
        let url = self.decision_url();
        let mut req_builder = self.inner.authorize(self.decision_http.post(&url));
        for (name, value) in session_headers(&self.session_id) {
            req_builder = req_builder.header(name, value);
        }

        let mut response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Magpie decision transport failed"))?;
        ensure!(
            response.status().is_success(),
            "Magpie decision HTTP {}",
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
            .map_err(|_| anyhow::anyhow!("Magpie decision response read failed"))?
        {
            ensure!(
                chunk.len() <= MAX_RESPONSE_BYTES - bytes.len(),
                MalformedDecisionResponse
            );
            bytes.extend_from_slice(&chunk);
        }

        decode_magpie_decision_response(&bytes, &request)
            .map_err(|error| error.context(MalformedDecisionResponse))
    }
}

pub(super) fn validate_decision_model(model: &str) -> Result<()> {
    ensure!(
        !model.trim().is_empty()
            && model.len() <= MAX_DECISION_MODEL_OVERRIDE_BYTES
            && !model.bytes().any(|b| b.is_ascii_control()),
        "invalid Magpie decision model"
    );
    Ok(())
}

pub(super) fn build_magpie_decision_payload(
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
struct RawMagpieDecisionResponse {
    model: String,
    #[serde(default, deserialize_with = "deserialize_non_null_calibration")]
    calibration: Option<CalibrationKind>,
    #[serde(deserialize_with = "deserialize_unique_map")]
    answers: BTreeMap<String, RawMagpieAnswer>,
    #[serde(default)]
    usage: Option<RawMagpieUsage>,
}

#[derive(Deserialize)]
struct RawMagpieAnswer {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    yes_probability: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    noul: Option<f64>,
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
struct RawMagpieUsage {
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    prompt_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    completion_tokens: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_non_null_i32")]
    input_tokens: Option<i32>,
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

pub(super) fn decode_magpie_decision_response(
    bytes: &[u8],
    request: &DecisionRequest,
) -> Result<DecisionResponse> {
    let mut raw: RawMagpieDecisionResponse = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("invalid Magpie decision response"))?;
    validate_decision_model(&raw.model)?;
    ensure!(
        raw.answers.len() == request.questions.len(),
        "Magpie decision answer count mismatch"
    );

    let mut answers = BTreeMap::new();
    for (question_id, question) in &request.questions {
        let raw_answer = raw
            .answers
            .remove(question_id)
            .ok_or_else(|| anyhow::anyhow!("missing Magpie answer for '{question_id}'"))?;
        let decoded = decode_magpie_answer(question_id, question, raw_answer)?;
        answers.insert(question_id.clone(), decoded);
    }

    let usage = raw.usage.map(decode_magpie_usage).transpose()?;
    let calibration = raw.calibration.unwrap_or(CalibrationKind::Calibrated);

    let response = DecisionResponse {
        model: raw.model,
        provider: "magpie".to_string(),
        calibration,
        answers,
        usage,
    };

    request.validate_response(&response)?;
    Ok(response)
}

fn decode_magpie_usage(raw: RawMagpieUsage) -> Result<TokenUsage> {
    let prompt_tokens = match (raw.prompt_tokens, raw.input_tokens) {
        (Some(p), Some(i)) => {
            ensure!(
                p == i,
                "contradictory Magpie prompt_tokens and input_tokens"
            );
            p
        }
        (Some(p), None) => p,
        (None, Some(i)) => i,
        (None, None) => bail!("Magpie usage missing prompt/input tokens"),
    };
    let completion_tokens = match (raw.completion_tokens, raw.output_tokens) {
        (Some(c), Some(o)) => {
            ensure!(
                c == o,
                "contradictory Magpie completion_tokens and output_tokens"
            );
            c
        }
        (Some(c), None) => c,
        (None, Some(o)) => o,
        (None, None) => bail!("Magpie usage missing completion/output tokens"),
    };
    ensure!(
        prompt_tokens >= 0 && completion_tokens >= 0,
        "invalid Magpie token usage"
    );
    let computed_total = prompt_tokens
        .checked_add(completion_tokens)
        .ok_or_else(|| anyhow::anyhow!("Magpie token usage overflow"))?;
    if let Some(total) = raw.total_tokens {
        ensure!(
            total == computed_total,
            "Magpie total_tokens disagrees with prompt + completion"
        );
    }
    if let Some(cached) = raw.cached_prompt_tokens {
        ensure!(cached >= 0, "invalid Magpie cached_prompt_tokens");
    }
    if let Some(reasoning) = raw.reasoning_tokens {
        ensure!(reasoning >= 0, "invalid Magpie reasoning_tokens");
    }
    let cost_usd = match (raw.cost_usd, raw.cost) {
        (Some(a), Some(b)) => {
            ensure!(
                a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-12,
                "contradictory Magpie usage cost fields"
            );
            Some(a)
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let Some(cost) = cost_usd {
        ensure!(cost.is_finite() && cost >= 0.0, "invalid Magpie usage cost");
    }

    Ok(TokenUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens: computed_total,
        cached_prompt_tokens: raw.cached_prompt_tokens,
        reasoning_tokens: raw.reasoning_tokens,
        cost_usd,
        ..Default::default()
    })
}

fn decode_magpie_answer(
    question_id: &str,
    question: &DecisionQuestion,
    raw: RawMagpieAnswer,
) -> Result<DecisionAnswer> {
    match question {
        DecisionQuestion::Predicate { .. } => {
            let is_noul = raw.kind == "noul";
            ensure!(
                raw.kind == "predicate" || is_noul,
                "Magpie predicate '{question_id}' requires `predicate` or `noul` answer"
            );
            ensure!(
                raw.choice.is_none() && raw.score.is_none() && raw.expected_score.is_none(),
                "Magpie predicate '{question_id}' contains non-predicate fields"
            );
            if let Some(ref probs) = raw.probabilities {
                ensure!(
                    probs.len() == 2 && probs.contains_key("true") && probs.contains_key("false"),
                    "Magpie predicate probabilities for '{question_id}' must match `true`/`false`"
                );
                validate_probability_distribution(probs)?;
            }
            let direct_prob = match (raw.yes_probability, raw.noul) {
                (Some(y), Some(n)) => {
                    validate_unit_interval("yes_probability", y)?;
                    validate_unit_interval("noul", n)?;
                    ensure!(
                        (y - n).abs() <= 1e-6,
                        "Magpie predicate '{question_id}' has contradictory probability fields"
                    );
                    Some(y)
                }
                (Some(y), None) => {
                    validate_unit_interval("yes_probability", y)?;
                    Some(y)
                }
                (None, Some(n)) => {
                    validate_unit_interval("noul", n)?;
                    Some(n)
                }
                (None, None) => None,
            };
            let yes_probability = match (direct_prob, raw.probabilities.as_ref()) {
                (Some(prob), Some(probs)) => {
                    ensure!(
                        (prob - probs["true"]).abs() <= 0.01,
                        "Magpie predicate '{question_id}' disagrees with `true` probability"
                    );
                    prob
                }
                (Some(prob), None) => prob,
                (None, Some(probs)) => probs["true"],
                (None, None) => bail!("Magpie predicate '{question_id}' missing probability"),
            };
            let confidence = match raw.confidence {
                Some(conf) => {
                    validate_unit_interval("confidence", conf)?;
                    conf
                }
                None if is_noul => yes_probability.max(1.0 - yes_probability),
                None => bail!("Magpie predicate '{question_id}' missing confidence"),
            };
            Ok(DecisionAnswer::Predicate {
                yes_probability,
                confidence,
            })
        }
        DecisionQuestion::Choice { .. } => {
            ensure!(
                raw.kind == "choice",
                "Magpie choice '{question_id}' requires `choice` answer"
            );
            ensure!(
                raw.yes_probability.is_none()
                    && raw.noul.is_none()
                    && raw.score.is_none()
                    && raw.expected_score.is_none(),
                "Magpie choice '{question_id}' contains non-choice fields"
            );
            let choice = raw
                .choice
                .ok_or_else(|| anyhow::anyhow!("Magpie choice missing for '{question_id}'"))?;
            let probabilities = raw.probabilities.ok_or_else(|| {
                anyhow::anyhow!("Magpie choice probabilities missing for '{question_id}'")
            })?;
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("Magpie choice confidence missing for '{question_id}'")
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
                "Magpie score '{question_id}' requires `score` answer"
            );
            ensure!(
                raw.yes_probability.is_none() && raw.noul.is_none() && raw.choice.is_none(),
                "Magpie score '{question_id}' contains non-score fields"
            );
            let probabilities = raw.probabilities.ok_or_else(|| {
                anyhow::anyhow!("Magpie score probabilities missing for '{question_id}'")
            })?;
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("Magpie score confidence missing for '{question_id}'")
            })?;
            validate_unit_interval("confidence", confidence)?;
            let expected_score = match (raw.expected_score, raw.score) {
                (Some(es), Some(s)) => {
                    ensure!(
                        es.is_finite() && s.is_finite() && (es - s).abs() <= 1e-6,
                        "Magpie contradictory score values for '{question_id}'"
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
