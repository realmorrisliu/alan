//! Batch calibrated Decisions support (`Predicate`, `Choice`, `Score`) for TypeSafe `/v1/systemone`.

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};

use super::{MAX_RESPONSE_BYTES, TypesafeEvaluationClient};
use crate::decision::{
    deserialize_unique_map, validate_probability_distribution, validate_unit_interval,
};
use crate::{
    CalibrationKind, DecisionAnswer, DecisionLevel, DecisionOption, DecisionQuestion,
    DecisionRequest, DecisionResponse, MalformedDecisionResponse, TokenUsage,
};

impl TypesafeEvaluationClient {
    pub(super) async fn decide_typesafe(
        &self,
        request: DecisionRequest,
    ) -> Result<DecisionResponse> {
        request.validate()?;
        let effective_model = match &request.model_override {
            Some(override_model) => {
                Self::validate_model(override_model)?;
                override_model.as_str()
            }
            None => self.model.as_str(),
        };

        let body = build_typesafe_decision_payload(effective_model, &request)?;
        let mut response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("TypeSafe decision transport failed"))?;
        ensure!(
            response.status().is_success(),
            "TypeSafe decision HTTP {}",
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
            .map_err(|_| anyhow::anyhow!("TypeSafe decision response read failed"))?
        {
            ensure!(
                chunk.len() <= MAX_RESPONSE_BYTES - bytes.len(),
                MalformedDecisionResponse
            );
            bytes.extend_from_slice(&chunk);
        }

        decode_typesafe_decision_response(&bytes, effective_model, &request)
            .map_err(|error| error.context(MalformedDecisionResponse))
    }
}

pub(super) fn build_typesafe_decision_payload(
    model: &str,
    request: &DecisionRequest,
) -> Result<Value> {
    request.validate()?;
    TypesafeEvaluationClient::validate_model(model)?;

    let mut questions = Map::with_capacity(request.questions.len());
    for (question_id, question) in &request.questions {
        questions.insert(
            question_id.clone(),
            encode_typesafe_question(request.instructions.as_deref(), question),
        );
    }

    Ok(json!({
        "model": model,
        "state": &request.state,
        "questions": questions,
    }))
}

fn compose_instructions(global_instructions: Option<&str>, prompt: &str) -> String {
    match global_instructions.map(str::trim).filter(|s| !s.is_empty()) {
        Some(global) => format!("{global}\n\n{prompt}"),
        None => prompt.to_string(),
    }
}

fn encode_typesafe_question(
    global_instructions: Option<&str>,
    question: &DecisionQuestion,
) -> Value {
    match question {
        DecisionQuestion::Predicate { prompt } => json!({
            "type": "noul",
            "instructions": compose_instructions(global_instructions, prompt),
            "criteria": {
                "true": "The predicate holds for the state.",
                "false": "The predicate does not hold for the state."
            }
        }),
        DecisionQuestion::Choice { prompt, options } => json!({
            "type": "choice",
            "instructions": compose_instructions(global_instructions, prompt),
            "criteria": encode_choice_criteria(options),
        }),
        DecisionQuestion::Score { prompt, levels } => json!({
            "type": "score",
            "instructions": compose_instructions(global_instructions, prompt),
            "criteria": encode_score_criteria(levels),
        }),
    }
}

fn encode_choice_criteria(options: &[DecisionOption]) -> Value {
    let mut map = Map::with_capacity(options.len());
    for opt in options {
        let desc = if opt.description.trim().is_empty() {
            opt.id.clone()
        } else {
            opt.description.clone()
        };
        map.insert(opt.id.clone(), Value::String(desc));
    }
    Value::Object(map)
}

fn encode_score_criteria(levels: &[DecisionLevel]) -> Value {
    let mut map = Map::with_capacity(levels.len());
    for level in levels {
        let desc = if level.description.trim().is_empty() {
            level.id.as_str()
        } else {
            level.description.as_str()
        };
        map.insert(
            level.id.clone(),
            json!({
                "description": desc,
                "score": level.score,
            }),
        );
    }
    Value::Object(map)
}

#[derive(Deserialize)]
struct RawSystemOneResponse {
    model: String,
    #[serde(deserialize_with = "deserialize_unique_map")]
    answers: BTreeMap<String, RawSystemOneAnswer>,
    usage: RawSystemOneUsage,
}

#[derive(Deserialize)]
struct RawSystemOneAnswer {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    noul: Option<f64>,
    #[serde(default)]
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
struct RawSystemOneUsage {
    input_tokens: i32,
    output_tokens: i32,
    #[serde(default, deserialize_with = "deserialize_non_null_f64")]
    cost: Option<f64>,
}

fn deserialize_non_null_f64<'de, D>(deserializer: D) -> std::result::Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    f64::deserialize(deserializer).map(Some)
}

fn deserialize_optional_unique_map<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<BTreeMap<String, f64>>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_unique_map(deserializer).map(Some)
}

pub(super) fn decode_typesafe_decision_response(
    bytes: &[u8],
    expected_model: &str,
    request: &DecisionRequest,
) -> Result<DecisionResponse> {
    let mut raw: RawSystemOneResponse = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("invalid TypeSafe decision response"))?;
    ensure!(
        raw.model == expected_model && raw.answers.len() == request.questions.len(),
        "TypeSafe decision provenance mismatch"
    );

    let mut answers = BTreeMap::new();
    for (question_id, question) in &request.questions {
        let raw_answer = raw
            .answers
            .remove(question_id)
            .ok_or_else(|| anyhow::anyhow!("missing TypeSafe answer for '{question_id}'"))?;
        let decoded = decode_typesafe_answer(question_id, question, raw_answer)?;
        answers.insert(question_id.clone(), decoded);
    }

    ensure!(
        raw.usage.input_tokens >= 0 && raw.usage.output_tokens >= 0,
        "invalid TypeSafe token usage"
    );
    let total_tokens = raw
        .usage
        .input_tokens
        .checked_add(raw.usage.output_tokens)
        .ok_or_else(|| anyhow::anyhow!("invalid TypeSafe token usage"))?;
    if let Some(cost) = raw.usage.cost {
        ensure!(
            cost.is_finite() && cost >= 0.0,
            "invalid TypeSafe usage cost"
        );
    }

    let response = DecisionResponse {
        model: raw.model,
        provider: "typesafe".to_string(),
        calibration: CalibrationKind::Calibrated,
        answers,
        usage: Some(TokenUsage {
            prompt_tokens: raw.usage.input_tokens,
            completion_tokens: raw.usage.output_tokens,
            total_tokens,
            cost_usd: raw.usage.cost,
            ..Default::default()
        }),
    };

    request.validate_response(&response)?;
    Ok(response)
}

fn decode_typesafe_answer(
    question_id: &str,
    question: &DecisionQuestion,
    raw: RawSystemOneAnswer,
) -> Result<DecisionAnswer> {
    match question {
        DecisionQuestion::Predicate { .. } => {
            ensure!(
                raw.kind == "noul",
                "TypeSafe predicate '{question_id}' requires `noul` answer"
            );
            if let Some(ref probs) = raw.probabilities {
                ensure!(
                    probs.len() == 2 && probs.contains_key("true") && probs.contains_key("false"),
                    "TypeSafe noul probabilities for '{question_id}' must match `true`/`false`"
                );
                validate_probability_distribution(probs)?;
            }
            let yes_probability = match (raw.noul, raw.probabilities.as_ref()) {
                (Some(noul), Some(probs)) => {
                    validate_unit_interval("noul", noul)?;
                    ensure!(
                        (noul - probs["true"]).abs() <= 0.01,
                        "TypeSafe noul disagrees with `true` probability for '{question_id}'"
                    );
                    noul
                }
                (Some(noul), None) => {
                    validate_unit_interval("noul", noul)?;
                    noul
                }
                (None, Some(probs)) => probs["true"],
                (None, None) => {
                    bail!("TypeSafe noul answer missing probability for '{question_id}'")
                }
            };
            let confidence = match raw.confidence {
                Some(conf) => {
                    validate_unit_interval("confidence", conf)?;
                    conf
                }
                None => yes_probability.max(1.0 - yes_probability),
            };
            Ok(DecisionAnswer::Predicate {
                yes_probability,
                confidence,
            })
        }
        DecisionQuestion::Choice { .. } => {
            ensure!(
                raw.kind == "choice",
                "TypeSafe choice '{question_id}' requires `choice` answer"
            );
            let choice = raw
                .choice
                .ok_or_else(|| anyhow::anyhow!("TypeSafe choice missing for '{question_id}'"))?;
            let probabilities = raw.probabilities.ok_or_else(|| {
                anyhow::anyhow!("TypeSafe choice probabilities missing for '{question_id}'")
            })?;
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("TypeSafe choice confidence missing for '{question_id}'")
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
                "TypeSafe score '{question_id}' requires `score` answer"
            );
            let probabilities = raw.probabilities.ok_or_else(|| {
                anyhow::anyhow!("TypeSafe score probabilities missing for '{question_id}'")
            })?;
            let confidence = raw.confidence.ok_or_else(|| {
                anyhow::anyhow!("TypeSafe score confidence missing for '{question_id}'")
            })?;
            validate_unit_interval("confidence", confidence)?;
            let expected_score = match (raw.score, raw.expected_score) {
                (Some(s), Some(es)) => {
                    ensure!(
                        s.is_finite() && es.is_finite() && (s - es).abs() <= 1e-6,
                        "TypeSafe contradictory score values for '{question_id}'"
                    );
                    s
                }
                (Some(s), None) => s,
                (None, Some(es)) => es,
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
