//! Provider-neutral typed Decisions API.
//!
//! Decisions represent bounded, typed judgments (`Predicate`, `Choice`, `Score`)
//! over untrusted JSON `state` without assistant prose or tool dispatch.
//! Decisions carry explicit [`CalibrationKind`] provenance and never grant
//! execution authority on their own.

use std::collections::{BTreeMap, HashSet};
use std::io::{self, Write};

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::TokenUsage;

/// Maximum number of questions in a single [`DecisionRequest`].
pub const MAX_DECISION_QUESTIONS: usize = 64;
/// Maximum number of options in a [`DecisionQuestion::Choice`] or levels in a
/// [`DecisionQuestion::Score`].
pub const MAX_DECISION_OPTIONS: usize = 128;
/// Maximum byte length of a question, option, or level identifier.
pub const MAX_DECISION_ID_BYTES: usize = 128;
/// Maximum byte length of `model_override`.
pub const MAX_DECISION_MODEL_OVERRIDE_BYTES: usize = 256;
/// Maximum serialized byte size of a [`DecisionRequest`] (1 MiB).
pub const MAX_DECISION_REQUEST_BYTES: usize = 1 << 20;

/// An evaluator returned invalid decision evidence, distinct from provider unavailability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedDecisionResponse;

impl std::fmt::Display for MalformedDecisionResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("malformed decision response")
    }
}

impl std::error::Error for MalformedDecisionResponse {}

/// Provenance of decision probability calibration.
///
/// [`CalibrationKind::Calibrated`] is reserved exclusively for dedicated native
/// calibrated Decisions endpoints (zero output tokens). Generation-based
/// structured fallbacks MUST always report [`CalibrationKind::Uncalibrated`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationKind {
    Calibrated,
    Uncalibrated,
}

/// One finite option offered by [`DecisionQuestion::Choice`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionOption {
    pub id: String,
    pub description: String,
}

/// One discrete scored level offered by [`DecisionQuestion::Score`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionLevel {
    pub id: String,
    pub description: String,
    pub score: f64,
}

/// Typed decision question evaluated over untrusted JSON `state`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionQuestion {
    /// Boolean yes/no judgment over untrusted state.
    Predicate { prompt: String },
    /// Finite selection over unique bounded options.
    Choice {
        prompt: String,
        options: Vec<DecisionOption>,
    },
    /// Discrete expected-value rating over unique finite scored levels.
    Score {
        prompt: String,
        levels: Vec<DecisionLevel>,
    },
}

/// Bounded multi-question decision request; `state` is untrusted JSON data, never instructions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub state: serde_json::Value,
    pub questions: BTreeMap<String, DecisionQuestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
}

/// Typed answer corresponding 1:1 to a submitted [`DecisionQuestion`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionAnswer {
    Predicate {
        yes_probability: f64,
        confidence: f64,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Score {
        expected_score: f64,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
}

/// Validated provider decision output; missing usage stays `None`, never invented zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionResponse {
    pub model: String,
    pub provider: String,
    pub calibration: CalibrationKind,
    pub answers: BTreeMap<String, DecisionAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

impl DecisionRequest {
    /// Create a decision request with the given untrusted JSON `state` and question map.
    pub fn new(state: serde_json::Value, questions: BTreeMap<String, DecisionQuestion>) -> Self {
        Self {
            state,
            questions,
            instructions: None,
            model_override: None,
        }
    }

    /// Attach optional trusted evaluation instructions.
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Request a specific decision model override when supported by the provider.
    pub fn with_model_override(mut self, model: impl Into<String>) -> Self {
        self.model_override = Some(model.into());
        self
    }

    /// Strictly validate request bounds, identifiers, prompts, scores, and 1 MiB size cap
    /// without rewriting `state` or any identifier.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.questions.is_empty() && self.questions.len() <= MAX_DECISION_QUESTIONS,
            "decision request requires 1 to {MAX_DECISION_QUESTIONS} questions"
        );

        if let Some(instructions) = &self.instructions {
            ensure!(
                !instructions.trim().is_empty(),
                "decision instructions cannot be empty when provided"
            );
        }

        if let Some(model_override) = &self.model_override {
            ensure!(
                !model_override.trim().is_empty()
                    && model_override.len() <= MAX_DECISION_MODEL_OVERRIDE_BYTES
                    && !model_override.bytes().any(|b| b.is_ascii_control()),
                "invalid decision model_override"
            );
        }

        for (question_id, question) in &self.questions {
            validate_identifier("question", question_id)?;
            validate_question(question_id, question)?;
        }

        let mut counter = BoundedByteCounter::new(MAX_DECISION_REQUEST_BYTES);
        if serde_json::to_writer(&mut counter, self).is_err() {
            bail!("decision request exceeds 1 MiB");
        }

        Ok(())
    }

    /// Validate that a [`DecisionResponse`] matches this request 1:1 and satisfies
    /// all probability, distribution, option, level, and finite-score invariants.
    pub fn validate_response(&self, response: &DecisionResponse) -> Result<()> {
        self.validate()?;
        response.validate()?;

        ensure!(
            response.answers.len() == self.questions.len(),
            "decision response answer count does not match request questions"
        );

        for (question_id, question) in &self.questions {
            let answer = response
                .answers
                .get(question_id)
                .ok_or_else(|| anyhow::anyhow!("missing answer for question '{question_id}'"))?;
            validate_answer_for_question(question_id, question, answer)?;
        }

        Ok(())
    }
}

impl DecisionAnswer {
    /// Return the reported confidence in `[0.0, 1.0]`.
    pub fn confidence(&self) -> f64 {
        match self {
            Self::Predicate { confidence, .. }
            | Self::Choice { confidence, .. }
            | Self::Score { confidence, .. } => *confidence,
        }
    }

    /// For a predicate answer, return whether `yes_probability >= 0.5`.
    pub fn predicate_yes(&self) -> Option<bool> {
        match self {
            Self::Predicate {
                yes_probability, ..
            } => Some(*yes_probability >= 0.5),
            _ => None,
        }
    }

    /// Validate self-contained numeric and probability invariants (finite, non-NaN,
    /// `[0.0, 1.0]` bounds, simplex mass, and argmax consistency for `Choice`).
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Predicate {
                yes_probability,
                confidence,
            } => {
                validate_unit_interval("yes_probability", *yes_probability)?;
                validate_unit_interval("confidence", *confidence)?;
            }
            Self::Choice {
                choice,
                probabilities,
                confidence,
            } => {
                validate_unit_interval("confidence", *confidence)?;
                ensure!(!choice.is_empty(), "choice answer cannot be empty");
                validate_probability_distribution(probabilities)?;
                let selected = probabilities.get(choice).ok_or_else(|| {
                    anyhow::anyhow!("choice '{choice}' missing from probability distribution")
                })?;
                ensure!(
                    probabilities.values().all(|p| *p <= *selected + 1e-6),
                    "choice '{choice}' disagrees with probability distribution"
                );
            }
            Self::Score {
                expected_score,
                probabilities,
                confidence,
            } => {
                validate_unit_interval("confidence", *confidence)?;
                ensure!(expected_score.is_finite(), "expected_score must be finite");
                validate_probability_distribution(probabilities)?;
            }
        }
        Ok(())
    }
}

impl DecisionResponse {
    /// Validate response-level metadata and per-answer invariants.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.model.trim().is_empty(),
            "decision response model cannot be empty"
        );
        ensure!(
            !self.provider.trim().is_empty(),
            "decision response provider cannot be empty"
        );
        ensure!(
            !self.answers.is_empty(),
            "decision response must contain at least one answer"
        );
        for (question_id, answer) in &self.answers {
            validate_identifier("answer", question_id)?;
            answer.validate()?;
        }
        Ok(())
    }
}

pub(crate) fn validate_identifier(kind: &str, id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= MAX_DECISION_ID_BYTES
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "invalid {kind} ID"
    );
    Ok(())
}

fn validate_question(question_id: &str, question: &DecisionQuestion) -> Result<()> {
    match question {
        DecisionQuestion::Predicate { prompt } => {
            ensure!(
                !prompt.trim().is_empty(),
                "predicate question '{question_id}' requires a non-empty prompt"
            );
        }
        DecisionQuestion::Choice { prompt, options } => {
            ensure!(
                !prompt.trim().is_empty(),
                "choice question '{question_id}' requires a non-empty prompt"
            );
            ensure!(
                !options.is_empty() && options.len() <= MAX_DECISION_OPTIONS,
                "choice question '{question_id}' requires 1 to {MAX_DECISION_OPTIONS} options"
            );
            let mut seen = HashSet::with_capacity(options.len());
            for option in options {
                validate_identifier("option", &option.id)?;
                ensure!(
                    seen.insert(option.id.as_str()),
                    "duplicate option ID '{}' in choice question '{question_id}'",
                    option.id
                );
            }
        }
        DecisionQuestion::Score { prompt, levels } => {
            ensure!(
                !prompt.trim().is_empty(),
                "score question '{question_id}' requires a non-empty prompt"
            );
            ensure!(
                !levels.is_empty() && levels.len() <= MAX_DECISION_OPTIONS,
                "score question '{question_id}' requires 1 to {MAX_DECISION_OPTIONS} levels"
            );
            let mut seen = HashSet::with_capacity(levels.len());
            for level in levels {
                validate_identifier("level", &level.id)?;
                ensure!(
                    seen.insert(level.id.as_str()),
                    "duplicate level ID '{}' in score question '{question_id}'",
                    level.id
                );
                ensure!(
                    level.score.is_finite(),
                    "level '{}' in score question '{question_id}' must have a finite score",
                    level.id
                );
            }
        }
    }
    Ok(())
}

fn validate_answer_for_question(
    question_id: &str,
    question: &DecisionQuestion,
    answer: &DecisionAnswer,
) -> Result<()> {
    match (question, answer) {
        (DecisionQuestion::Predicate { .. }, DecisionAnswer::Predicate { .. }) => Ok(()),
        (
            DecisionQuestion::Choice { options, .. },
            DecisionAnswer::Choice {
                choice,
                probabilities,
                ..
            },
        ) => {
            ensure!(
                probabilities.len() == options.len()
                    && options
                        .iter()
                        .all(|opt| probabilities.contains_key(&opt.id)),
                "choice probabilities for '{question_id}' must match submitted option IDs"
            );
            ensure!(
                options.iter().any(|opt| opt.id == *choice),
                "choice '{choice}' for '{question_id}' is outside the submitted options"
            );
            Ok(())
        }
        (
            DecisionQuestion::Score { levels, .. },
            DecisionAnswer::Score {
                expected_score,
                probabilities,
                ..
            },
        ) => {
            ensure!(
                probabilities.len() == levels.len()
                    && levels
                        .iter()
                        .all(|level| probabilities.contains_key(&level.id)),
                "score probabilities for '{question_id}' must match submitted level IDs"
            );
            let mut min_score = f64::INFINITY;
            let mut max_score = f64::NEG_INFINITY;
            let mut computed_expected = 0.0_f64;
            for level in levels {
                min_score = min_score.min(level.score);
                max_score = max_score.max(level.score);
                let prob = probabilities[&level.id];
                computed_expected += prob * level.score;
            }
            ensure!(
                *expected_score >= min_score - 1e-6 && *expected_score <= max_score + 1e-6,
                "score expected_score for '{question_id}' is outside submitted level bounds"
            );
            let span = (max_score - min_score).abs().max(1.0);
            let exact_simplex = (probabilities.values().sum::<f64>() - 1.0).abs() <= 1e-3;
            let tolerance = if exact_simplex {
                0.01 * span + 1e-4
            } else {
                0.005 * (levels.len() as f64) * span + 1e-4
            };
            ensure!(
                (*expected_score - computed_expected).abs() <= tolerance,
                "score expected_score for '{question_id}' disagrees with level probabilities"
            );
            Ok(())
        }
        _ => bail!("decision answer variant mismatch for question '{question_id}'"),
    }
}

pub(crate) fn validate_unit_interval(name: &str, value: f64) -> Result<()> {
    ensure!(
        value.is_finite() && (0.0..=1.0).contains(&value),
        "{name} must be finite and in [0.0, 1.0]"
    );
    Ok(())
}

/// Validate that a probability distribution is non-empty, finite in `[0.0, 1.0]`,
/// and sums to `1.0` within strict precision or independent hundredths rounding.
pub(crate) fn validate_probability_distribution(
    probabilities: &BTreeMap<String, f64>,
) -> Result<()> {
    ensure!(
        !probabilities.is_empty(),
        "probability distribution cannot be empty"
    );
    let mut sum = 0.0_f64;
    for p in probabilities.values() {
        validate_unit_interval("probability", *p)?;
        sum += *p;
    }
    let rounded_mass = probabilities
        .values()
        .all(|p| (p * 100.0 - (p * 100.0).round()).abs() <= 1e-9)
        && probabilities
            .values()
            .map(|p| (p - 0.005).max(0.0))
            .sum::<f64>()
            <= 1.0 + 1e-9
        && probabilities
            .values()
            .map(|p| (p + 0.005).min(1.0))
            .sum::<f64>()
            >= 1.0 - 1e-9;
    ensure!(
        (sum - 1.0).abs() <= 1e-3 || rounded_mass,
        "probability distribution must sum to 1.0"
    );
    Ok(())
}

/// Deserialize a JSON object into a `BTreeMap` while rejecting duplicate keys.
pub(crate) fn deserialize_unique_map<'de, D, V>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct MapVisitor<V>(std::marker::PhantomData<V>);

    impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for MapVisitor<V> {
        type Value = BTreeMap<String, V>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map with unique keys")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, V>()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate decision key"));
                }
            }
            Ok(result)
        }
    }

    deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
}

struct BoundedByteCounter {
    count: usize,
    limit: usize,
}

impl BoundedByteCounter {
    fn new(limit: usize) -> Self {
        Self { count: 0, limit }
    }
}

impl Write for BoundedByteCounter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.count = self.count.saturating_add(buf.len());
        if self.count > self.limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "payload exceeds size limit",
            ));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "decision_tests.rs"]
mod tests;
