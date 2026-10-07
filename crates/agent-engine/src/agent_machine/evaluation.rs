//! Recovery of acknowledged evaluation evidence; never a dispatch path.
use std::collections::HashMap;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::rollout::EventRecord;
use crate::runtime::model_binding::CallableIdentity;

pub(crate) const EVENT_TYPE: &str = "machine_evaluation_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Candidate {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Surface {
    Interactive,
    Redirected,
}

/// Identity is unchanged between the pre-dispatch and terminal durability barriers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub source_rollout_id: String,
    pub submission_id: String,
    pub input_sha256: String,
    pub surface: Surface,
    pub operation_id: String,
    pub callable: CallableIdentity,
    pub schema: String,
    pub deadline_ms: u64,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Outcome {
    Started,
    Selected { candidate_id: String },
    NoMatch,
    Unavailable,
    Malformed,
    TimedOut,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub identity: Identity,
    pub outcome: Outcome,
    pub elapsed_ms: Option<u64>,
    pub usage: Option<Usage>,
}

impl Observation {
    fn validate(&self) -> Result<()> {
        let identity = &self.identity;
        ensure!(
            !identity.source_rollout_id.is_empty()
                && !identity.submission_id.is_empty()
                && !identity.operation_id.is_empty(),
            "evaluation evidence lacks correlation identity"
        );
        ensure!(
            identity.input_sha256.len() == 64
                && identity.input_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "evaluation evidence has invalid input digest"
        );
        ensure!(
            identity.schema == "choice.v1" && (1..=30_000).contains(&identity.deadline_ms),
            "evaluation evidence has unsupported operation controls"
        );
        ensure!(
            !identity.callable.profile.is_empty()
                && !identity.callable.provider.is_empty()
                && !identity.callable.model.is_empty()
                && !identity.callable.revision.is_empty(),
            "evaluation evidence lacks captured Connection provenance"
        );
        let request = alan_llm::ChoiceEvaluationRequest {
            input: String::new(),
            candidates: identity
                .candidates
                .iter()
                .map(|candidate| alan_llm::EvaluationCandidate {
                    id: candidate.id.clone(),
                    description: candidate.description.clone(),
                })
                .collect(),
        };
        request.validate()?;
        if let Outcome::Selected { candidate_id } = &self.outcome {
            request.validate_selection(&alan_llm::EvaluationSelection::Selected(
                candidate_id.clone(),
            ))?;
        }
        if self.outcome == Outcome::Started {
            ensure!(
                self.elapsed_ms.is_none() && self.usage.is_none(),
                "started evaluation contains terminal measurements"
            );
        } else if self.outcome != Outcome::Interrupted {
            ensure!(
                self.elapsed_ms.is_some(),
                "terminal evaluation lacks elapsed time"
            );
        }
        Ok(())
    }
}

/// Replay evidence only. Unsettled requests become interrupted, never queued work.
/// Conflicting records fail recovery rather than choosing a convenient outcome.
pub(super) fn recover(events: &[EventRecord]) -> Result<Option<serde_json::Value>> {
    let mut observations: HashMap<(String, String), Observation> = HashMap::new();
    let mut latest = None;
    for event in events.iter().filter(|event| event.event_type == EVENT_TYPE) {
        let observation: Observation = serde_json::from_value(event.payload.clone())?;
        observation.validate()?;
        let key = (
            observation.identity.source_rollout_id.clone(),
            observation.identity.submission_id.clone(),
        );
        match observations.get(&key) {
            None => ensure!(
                observation.outcome == Outcome::Started,
                "evaluation terminal evidence lacks an acknowledged start"
            ),
            Some(previous) => {
                if previous == &observation {
                    continue;
                }
                ensure!(
                    previous.identity == observation.identity,
                    "evaluation identity changed after admission"
                );
                ensure!(
                    previous.outcome == Outcome::Started,
                    "evaluation terminal evidence conflicts"
                );
            }
        }
        observations.insert(key.clone(), observation);
        latest = Some(key);
    }
    let Some(mut observation) = latest.and_then(|key| observations.remove(&key)) else {
        return Ok(None);
    };
    if observation.outcome == Outcome::Started {
        observation.outcome = Outcome::Interrupted;
    }
    // Billing is intentionally not inferred from token usage or missing evidence.
    let mut snapshot = serde_json::to_value(observation)?;
    snapshot["cost_microusd"] = serde_json::Value::Null;
    Ok(Some(snapshot))
}

#[cfg(test)]
mod tests;
