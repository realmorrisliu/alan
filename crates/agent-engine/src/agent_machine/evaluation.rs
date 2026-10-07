//! Recovery of acknowledged evaluation evidence; never a dispatch path.
use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::rollout::EventRecord;
use crate::runtime::EvaluationSurface as Surface;
use crate::runtime::model_binding::CallableIdentity;

impl super::AgentMachine {
    /// Acknowledge evaluation evidence before model commit or terminal publication.
    /// Returns false for evidence already recorded; this never permits another call.
    /// Errors leave the projection unchanged and must not trigger evaluation retries.
    pub(crate) async fn persist_evaluation_observation(
        &mut self,
        payload: serde_json::Value,
    ) -> Result<bool> {
        let observation: Observation = serde_json::from_value(payload)?;
        observation.validate()?;
        ensure!(
            observation.outcome != Outcome::Interrupted,
            "interrupted evaluation is a recovery projection, not a new outcome"
        );
        let recorder = self
            .recorder
            .as_ref()
            .context("evaluation requires durable rollout storage")?;
        recorder.flush().await?;
        // ponytail: scan the owning rollout for this low-volume shadow slice;
        // use a recovered Machine index only if measured qualification volume needs it.
        let history = crate::rollout::RolloutRecorder::load_history(recorder.path()).await?;
        let mut events: Vec<_> = history
            .into_iter()
            .filter_map(|item| match item {
                crate::rollout::RolloutItem::Event(event) if event.event_type == EVENT_TYPE => {
                    Some(event)
                }
                _ => None,
            })
            .collect();
        let reconciled = recover(&events)?;
        for event in &events {
            let prior: Observation = serde_json::from_value(event.payload.clone())?;
            if prior.identity.submission_id == observation.identity.submission_id
                && (observation.outcome.bypass_reason() != Some(BypassReason::RequestResponse)
                    || prior.identity.source_rollout_id == observation.identity.source_rollout_id)
            {
                ensure!(
                    prior.identity == observation.identity,
                    "evaluation identity changed for an existing submission"
                );
                if prior == observation {
                    // Reconcile acknowledged history without reviving an uncertain
                    // start. A still-owned live start is already acknowledged.
                    let live_start = self.evaluation_observation.as_ref().is_some_and(|live| {
                        live["outcome"]["state"] == "started"
                            && reconciled.as_ref().is_some_and(|latest| {
                                latest["outcome"]["state"] == "interrupted"
                                    && latest["identity"] == live["identity"]
                            })
                    });
                    if !live_start {
                        self.evaluation_observation = reconciled;
                    }
                    return Ok(false);
                }
            }
        }
        ensure!(
            observation.identity.source_rollout_id == recorder.rollout_id(),
            "new evaluation evidence must belong to the current rollout"
        );
        if observation.outcome != Outcome::Started && observation.outcome.bypass_reason().is_none()
        {
            let identity = serde_json::to_value(&observation.identity)?;
            ensure!(
                self.evaluation_observation.as_ref().is_some_and(|live| {
                    live["outcome"]["state"] == "started" && live["identity"] == identity
                }),
                "evaluation terminal requires a live acknowledged start"
            );
        }
        let event = EventRecord {
            event_type: EVENT_TYPE.into(),
            payload: serde_json::to_value(&observation)?,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        events.push(event.clone());
        recover(&events)?;
        recorder
            .persist_batch(vec![crate::rollout::RolloutItem::Event(event)])
            .await?;
        let mut snapshot = serde_json::to_value(observation)?;
        snapshot["cost_microusd"] = serde_json::Value::Null;
        self.evaluation_observation = Some(snapshot);
        Ok(true)
    }
}

pub(crate) const EVENT_TYPE: &str = "machine_evaluation_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Candidate {
    pub id: String,
    pub description: String,
}

/// Identity is unchanged between the pre-dispatch and terminal durability barriers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub source_rollout_id: String,
    pub submission_id: String,
    pub input_sha256: String,
    pub surface: Surface,
    pub operation_id: Option<String>,
    pub callable: CallableIdentity,
    pub schema: String,
    pub deadline_ms: u64,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Outcome {
    Started,
    Selected {
        candidate_id: String,
    },
    NoMatch,
    Unavailable,
    Malformed,
    TimedOut,
    Cancelled,
    Interrupted,
    Bypassed {
        reason: BypassReason,
        evaluator_calls: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BypassReason {
    ExplicitCommand,
    ExplicitAgent,
    RequestResponse,
}

impl Outcome {
    pub(crate) fn bypass_reason(&self) -> Option<BypassReason> {
        match self {
            Self::Bypassed { reason, .. } => Some(*reason),
            _ => None,
        }
    }
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
            !identity.source_rollout_id.is_empty() && !identity.submission_id.is_empty(),
            "evaluation evidence lacks correlation identity"
        );
        if let Outcome::Bypassed {
            evaluator_calls, ..
        } = self.outcome
        {
            ensure!(
                evaluator_calls == 0 && identity.operation_id.is_none() && self.usage.is_none(),
                "bypassed input must not claim a model operation or usage"
            );
        } else {
            ensure!(
                identity
                    .operation_id
                    .as_ref()
                    .is_some_and(|id| !id.is_empty()),
                "evaluation evidence lacks operation identity"
            );
        }
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
                observation.outcome == Outcome::Started
                    || observation.outcome.bypass_reason().is_some(),
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
