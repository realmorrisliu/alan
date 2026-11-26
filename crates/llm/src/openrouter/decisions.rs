//! OpenRouter native calibrated Decisions adapter (`POST /api/alpha/decisions`).

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use openrouter_rs::{
    api::decisions::{DecisionsRequest, DecisionsResponse, DecisionsUsage},
    error::OpenRouterError,
};
use serde_json::{Map, Value, json};

use super::OpenRouterClient;
use crate::decision::{validate_probability_distribution, validate_unit_interval};
use crate::{
    CalibrationKind, DecisionAnswer, DecisionLevel, DecisionOption, DecisionQuestion,
    DecisionRequest, DecisionResponse, MalformedDecisionResponse, TokenUsage,
};

impl OpenRouterClient {
    pub(super) async fn decide_openrouter(
        &self,
        request: DecisionRequest,
    ) -> Result<DecisionResponse> {
        let sdk_request = build_openrouter_decisions_request(&self.model, &request)?;
        let sdk_client = self.build_sdk_client()?;
        let sdk_response = sdk_client
            .decisions()
            .create(&sdk_request)
            .await
            .map_err(map_openrouter_decisions_error)?;
        decode_openrouter_decisions_response(&request, sdk_response)
    }

    fn build_sdk_client(&self) -> Result<openrouter_rs::OpenRouterClient> {
        ensure!(
            !self.api_key.trim().is_empty(),
            "OpenRouter credential unavailable"
        );
        let mut builder = openrouter_rs::OpenRouterClient::builder();
        builder
            .base_url(&self.base_url)
            .api_key(&self.api_key)
            .http_client(self.http_client.clone());
        if let Some(x_title) = &self.metadata.x_title {
            builder.x_title(x_title);
        }
        if let Some(http_referer) = &self.metadata.http_referer {
            builder.http_referer(http_referer);
        }
        if let Some(app_categories) = &self.metadata.app_categories {
            builder.app_categories(app_categories.clone());
        }
        builder
            .build()
            .map_err(|_| anyhow::anyhow!("failed to configure OpenRouter SDK client"))
    }
}

fn map_openrouter_decisions_error(error: OpenRouterError) -> anyhow::Error {
    match error {
        OpenRouterError::Unknown(message)
            if message.starts_with("Failed to deserialize decisions response") =>
        {
            anyhow::anyhow!("invalid OpenRouter decisions response JSON")
                .context(MalformedDecisionResponse)
        }
        OpenRouterError::Api(context) => {
            anyhow::anyhow!("OpenRouter decisions HTTP {}", context.status.as_u16())
        }
        _ => anyhow::anyhow!("OpenRouter decisions transport failed"),
    }
}

pub(super) fn build_openrouter_decisions_request(
    default_model: &str,
    request: &DecisionRequest,
) -> Result<DecisionsRequest> {
    request.validate()?;
    let model = request
        .model_override
        .as_deref()
        .unwrap_or(default_model)
        .trim();
    ensure!(
        !model.is_empty(),
        "OpenRouter decisions requires a non-empty model"
    );

    let questions: Vec<(String, Value)> = request
        .questions
        .iter()
        .map(|(question_id, question)| {
            (
                question_id.clone(),
                encode_openrouter_question(request.instructions.as_deref(), question),
            )
        })
        .collect();

    DecisionsRequest::builder()
        .model(model.to_string())
        .state(request.state.clone())
        .questions(questions)
        .build()
        .map_err(|_| anyhow::anyhow!("failed to build OpenRouter decisions request"))
}

fn compose_instructions(global_instructions: Option<&str>, prompt: &str) -> String {
    match global_instructions.map(str::trim).filter(|s| !s.is_empty()) {
        Some(global) => format!("{global}\n\n{prompt}"),
        None => prompt.to_string(),
    }
}

fn encode_openrouter_question(
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
            "criteria": encode_choice_criteria(options)
        }),
        DecisionQuestion::Score { prompt, levels } => json!({
            "type": "score",
            "instructions": compose_instructions(global_instructions, prompt),
            "criteria": encode_score_criteria(levels)
        }),
    }
}

fn encode_choice_criteria(options: &[DecisionOption]) -> Value {
    let mut map = Map::with_capacity(options.len());
    for option in options {
        let desc = if option.description.trim().is_empty() {
            option.id.clone()
        } else {
            option.description.clone()
        };
        map.insert(option.id.clone(), Value::String(desc));
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

pub(super) fn decode_openrouter_decisions_response(
    request: &DecisionRequest,
    response: DecisionsResponse,
) -> Result<DecisionResponse> {
    request.validate()?;
    decode_openrouter_decisions_response_inner(request, response)
        .map_err(|err| err.context(MalformedDecisionResponse))
}

fn decode_openrouter_decisions_response_inner(
    request: &DecisionRequest,
    response: DecisionsResponse,
) -> Result<DecisionResponse> {
    ensure!(
        !response.model.trim().is_empty(),
        "OpenRouter decisions response model cannot be empty"
    );
    ensure!(
        response.answers.len() == request.questions.len(),
        "OpenRouter decisions response answer count mismatch"
    );

    let mut answers = BTreeMap::new();
    for (question_id, question) in &request.questions {
        let raw_answer = response
            .answers
            .get(question_id)
            .ok_or_else(|| anyhow::anyhow!("missing OpenRouter answer for '{question_id}'"))?;
        let decoded = decode_openrouter_answer(question_id, question, raw_answer)?;
        answers.insert(question_id.clone(), decoded);
    }

    let usage = convert_decisions_usage(response.usage)?;
    let provider = response
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("openrouter")
        .to_string();

    let decision_response = DecisionResponse {
        model: response.model,
        provider,
        calibration: CalibrationKind::Calibrated,
        answers,
        usage: Some(usage),
    };
    request.validate_response(&decision_response)?;
    Ok(decision_response)
}

fn decode_openrouter_answer(
    question_id: &str,
    question: &DecisionQuestion,
    raw: &Value,
) -> Result<DecisionAnswer> {
    let map = raw
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("answer for '{question_id}' must be a JSON object"))?;
    let kind = map
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("answer for '{question_id}' missing string `type`"))?;

    match question {
        DecisionQuestion::Predicate { .. } => {
            ensure!(
                kind == "noul",
                "expected `noul` answer for predicate '{question_id}', got '{kind}'"
            );
            let noul_value = parse_optional_f64_field(map, "noul")?;
            let probs = parse_optional_probabilities(map)?;
            if let Some(ref p) = probs {
                ensure!(
                    p.len() == 2 && p.contains_key("true") && p.contains_key("false"),
                    "noul probabilities for '{question_id}' must contain exact keys `true` and `false`"
                );
                validate_probability_distribution(p)?;
            }
            let yes_probability = match (noul_value, probs.as_ref()) {
                (Some(noul), Some(p)) => {
                    validate_unit_interval("noul", noul)?;
                    ensure!(
                        (noul - p["true"]).abs() <= 0.01,
                        "noul value disagrees with `true` probability for '{question_id}'"
                    );
                    noul
                }
                (Some(noul), None) => {
                    validate_unit_interval("noul", noul)?;
                    noul
                }
                (None, Some(p)) => p["true"],
                (None, None) => {
                    bail!("noul answer for '{question_id}' requires `noul` or `probabilities`")
                }
            };
            let confidence = match parse_optional_f64_field(map, "confidence")? {
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
                kind == "choice",
                "expected `choice` answer for '{question_id}', got '{kind}'"
            );
            let choice = map
                .get("choice")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    anyhow::anyhow!("choice answer for '{question_id}' missing `choice`")
                })?
                .to_string();
            let probabilities = parse_required_probabilities(map, question_id)?;
            let confidence = match parse_optional_f64_field(map, "confidence")? {
                Some(conf) => {
                    validate_unit_interval("confidence", conf)?;
                    conf
                }
                None => *probabilities.get(&choice).ok_or_else(|| {
                    anyhow::anyhow!(
                        "choice '{choice}' missing from probabilities for '{question_id}'"
                    )
                })?,
            };
            Ok(DecisionAnswer::Choice {
                choice,
                probabilities,
                confidence,
            })
        }
        DecisionQuestion::Score { levels, .. } => {
            ensure!(
                kind == "score",
                "expected `score` answer for '{question_id}', got '{kind}'"
            );
            let probabilities = parse_required_probabilities(map, question_id)?;
            let score_field = parse_optional_f64_field(map, "score")?;
            let expected_field = parse_optional_f64_field(map, "expected_score")?;
            let expected_score = match (score_field, expected_field) {
                (Some(s), Some(es)) => {
                    ensure!(
                        s.is_finite() && es.is_finite() && (s - es).abs() <= 1e-6,
                        "contradictory score fields for '{question_id}'"
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
            let confidence = match parse_optional_f64_field(map, "confidence")? {
                Some(conf) => {
                    validate_unit_interval("confidence", conf)?;
                    conf
                }
                None => probabilities.values().copied().fold(0.0_f64, f64::max),
            };
            Ok(DecisionAnswer::Score {
                expected_score,
                probabilities,
                confidence,
            })
        }
    }
}

fn parse_optional_f64_field(map: &Map<String, Value>, field: &str) -> Result<Option<f64>> {
    let Some(raw) = map.get(field) else {
        return Ok(None);
    };
    let val = raw
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("field `{field}` must be a finite JSON number"))?;
    ensure!(val.is_finite(), "field `{field}` must be finite");
    Ok(Some(val))
}

fn parse_optional_probabilities(map: &Map<String, Value>) -> Result<Option<BTreeMap<String, f64>>> {
    let Some(raw) = map.get("probabilities") else {
        return Ok(None);
    };
    let obj = raw
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("`probabilities` must be a JSON object"))?;
    let mut probs = BTreeMap::new();
    for (k, v) in obj {
        let num = v
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("probability for '{k}' must be a finite number"))?;
        probs.insert(k.clone(), num);
    }
    Ok(Some(probs))
}

fn parse_required_probabilities(
    map: &Map<String, Value>,
    question_id: &str,
) -> Result<BTreeMap<String, f64>> {
    parse_optional_probabilities(map)?
        .ok_or_else(|| anyhow::anyhow!("answer for '{question_id}' missing `probabilities` object"))
}

fn convert_decisions_usage(usage: DecisionsUsage) -> Result<TokenUsage> {
    let prompt_tokens = i32::try_from(usage.input_tokens)
        .map_err(|_| anyhow::anyhow!("OpenRouter decisions input_tokens overflow"))?;
    let completion_tokens = i32::try_from(usage.output_tokens)
        .map_err(|_| anyhow::anyhow!("OpenRouter decisions output_tokens overflow"))?;
    let total_tokens = prompt_tokens
        .checked_add(completion_tokens)
        .ok_or_else(|| anyhow::anyhow!("OpenRouter decisions total_tokens overflow"))?;
    let cost_usd = match usage.cost {
        Some(cost) => {
            ensure!(
                cost.is_finite() && cost >= 0.0,
                "OpenRouter decisions usage cost must be finite and non-negative"
            );
            Some(cost)
        }
        None => None,
    };
    Ok(TokenUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        cost_usd,
        ..Default::default()
    })
}

#[cfg(test)]
#[path = "decisions_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "decisions_http_tests.rs"]
mod http_tests;
