//! Acknowledged, no-effect reviewer evidence; never permission or replay authority.

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{rollout::EventRecord, runtime::model_binding::CallableIdentity};

const EVENT_TYPE: &str = "guardian_shadow_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub(crate) enum Assessment {
    Allow,
    Deny { rationale: String },
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub task_id: String,
    pub tool_call_id: String,
    pub action_digest: String,
    pub request_digest: String,
    pub policy_digest: String,
    pub baseline_request_digest: String,
    pub baseline_binding: Option<CallableIdentity>,
    pub reviewer: CallableIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Stage {
    Started,
    Finished {
        candidate: Assessment,
        elapsed_millis: u64,
        final_response_usage: Option<serde_json::Value>,
        cost_microusd: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub identity: Identity,
    pub baseline: Assessment,
    pub stage: Stage,
}

impl Observation {
    fn key(&self) -> (String, String) {
        (
            self.identity.task_id.clone(),
            self.identity.tool_call_id.clone(),
        )
    }

    fn validate(&self) -> Result<()> {
        for field in [&self.identity.task_id, &self.identity.tool_call_id] {
            ensure!(!field.trim().is_empty(), "empty Guardian shadow identity");
        }
        for callable in
            std::iter::once(&self.identity.reviewer).chain(self.identity.baseline_binding.iter())
        {
            for field in [
                &callable.profile,
                &callable.provider,
                &callable.model,
                &callable.revision,
            ] {
                ensure!(
                    !field.trim().is_empty(),
                    "empty Guardian shadow callable identity"
                );
            }
        }
        for digest in [
            &self.identity.action_digest,
            &self.identity.request_digest,
            &self.identity.policy_digest,
            &self.identity.baseline_request_digest,
        ] {
            ensure!(
                digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid Guardian shadow digest"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct State {
    observations: HashMap<(String, String), Observation>,
    uncertain: bool,
}

fn validate_transition(prior: Option<&Observation>, next: &Observation) -> Result<()> {
    next.validate()?;
    if let Some(prior) = prior {
        ensure!(
            prior.identity == next.identity && prior.baseline == next.baseline,
            "Guardian shadow action, policy, profile or baseline changed"
        );
        ensure!(
            prior == next
                || matches!(prior.stage, Stage::Started)
                    && matches!(next.stage, Stage::Finished { .. }),
            "Guardian shadow outcome cannot be replaced"
        );
    } else {
        ensure!(
            matches!(next.stage, Stage::Started),
            "Guardian shadow finish lacks start"
        );
    }
    Ok(())
}

impl super::AgentMachine {
    /// Persist start before any candidate admission; recovery never retries an old start.
    pub(crate) async fn begin_guardian_shadow(&mut self, next: Observation) -> Result<bool> {
        ensure!(
            matches!(next.stage, Stage::Started),
            "expected Guardian shadow start"
        );
        ensure!(
            !self.guardian_shadow.uncertain,
            "Guardian shadow evidence is uncertain"
        );
        next.validate()?;
        if let Some(prior) = self.guardian_shadow.observations.get(&next.key()) {
            ensure!(
                prior.identity == next.identity,
                "Guardian shadow identity changed for an existing action"
            );
            return Ok(false);
        }
        self.persist_guardian_shadow(next).await?;
        Ok(true)
    }

    pub(crate) async fn persist_guardian_shadow(&mut self, next: Observation) -> Result<()> {
        ensure!(
            !self.guardian_shadow.uncertain,
            "Guardian shadow evidence is uncertain"
        );
        validate_transition(self.guardian_shadow.observations.get(&next.key()), &next)?;
        let recorder = self
            .recorder
            .as_ref()
            .context("Guardian shadow requires durable storage")?;
        let event = EventRecord {
            event_type: EVENT_TYPE.into(),
            payload: serde_json::to_value(&next)?,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.guardian_shadow.uncertain = true;
        recorder
            .persist_batch(vec![crate::rollout::RolloutItem::Event(event)])
            .await?;
        self.guardian_shadow.observations.insert(next.key(), next);
        self.guardian_shadow.uncertain = false;
        Ok(())
    }
}

pub(super) fn recover(events: &[EventRecord]) -> Result<State> {
    let mut state = State::default();
    for event in events.iter().filter(|event| event.event_type == EVENT_TYPE) {
        let observation: Observation = serde_json::from_value(event.payload.clone())?;
        validate_transition(state.observations.get(&observation.key()), &observation)?;
        state.observations.insert(observation.key(), observation);
    }
    Ok(state)
}

#[cfg(test)]
mod tests;
