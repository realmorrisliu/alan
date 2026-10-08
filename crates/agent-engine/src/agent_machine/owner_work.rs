//! Durable state for the bounded source-owner Machine program, never a dispatch owner.
use alan_agent_protocol::{OwnerSourceRange, OwnerWorkRequest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::rollout::{EventRecord, RolloutItem, RolloutRecorder};

pub(crate) const EVENT_TYPE: &str = "machine_owner_work_v1";
const WAIT_ACK_TYPE: &str = "machine_owner_wait_acknowledged_v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitAcknowledged {
    work_id: String,
    request_id: String,
    request_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub owner: String,
    pub source: OwnerSourceRange,
    /// Exact canonical UTF-8 range bytes returned by read_file, without a terminal newline.
    pub content: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Outcome {
    Started,
    Completed { owner: String },
    Waiting { request_id: String, reason: String },
    Failed { reason: String },
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FallbackEvidence {
    pub callable: crate::runtime::model_binding::CallableIdentity,
    pub request_sha256: String,
    pub quote: crate::runtime::model_binding::GenerationCostBound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub version: u8,
    pub work_id: String,
    pub source_rollout_id: String,
    pub request_sha256: String,
    pub request: OwnerWorkRequest,
    pub evidence: Vec<Evidence>,
    pub evaluator_calls: u8,
    pub generation_calls: u8,
    pub fallback: Option<FallbackEvidence>,
    pub owned_request: Option<String>,
    pub outcome: Outcome,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

impl Snapshot {
    pub(crate) fn waiting_request(&self) -> crate::approval::PendingStructuredInputRequest {
        let options = self
            .request
            .candidates
            .iter()
            .map(|c| serde_json::json!({"value":c.id,"label":c.id}))
            .collect::<Vec<_>>();
        crate::approval::PendingStructuredInputRequest {
            request_id:format!("work:{}", self.work_id), title:"Choose source owner".into(), prompt:self.request.question.clone(),
            questions:serde_json::from_value(serde_json::json!([{"id":"owner","label":"Owner",
                "prompt":"Choose a captured owner candidate", "kind":"single_select","required":true,"options":options}])).expect("validated owner candidates form one question"),
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.request.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            self.version == 1 && uuid::Uuid::parse_str(&self.work_id).is_ok(),
            "invalid work identity"
        );
        ensure!(
            !self.source_rollout_id.is_empty(),
            "missing source rollout identity"
        );
        ensure!(
            self.request_sha256 == digest(&serde_json::to_vec(&self.request)?),
            "work request digest changed"
        );
        ensure!(
            self.evaluator_calls <= 1 && self.generation_calls <= 1,
            "work model budget exceeded"
        );
        ensure!(
            (self.generation_calls == 1) == self.fallback.is_some(),
            "fallback lacks billing evidence"
        );
        if let Some(fallback) = &self.fallback {
            ensure!(
                fallback.quote.cost_microusd <= 1000
                    && !fallback.quote.provenance.is_empty()
                    && fallback.quote.provenance.len() <= 1024
                    && fallback.request_sha256.len() == 64
                    && fallback
                        .request_sha256
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit())
                    && !fallback.callable.profile.is_empty()
                    && !fallback.callable.model.is_empty(),
                "invalid fallback billing provenance"
            );
        }
        for evidence in &self.evidence {
            ensure!(
                self.request
                    .candidates
                    .iter()
                    .any(|c| c.id == evidence.owner && c.sources.contains(&evidence.source)),
                "work evidence outside captured candidates"
            );
            ensure!(
                evidence.sha256 == digest(evidence.content.as_bytes()),
                "work evidence digest changed"
            );
        }
        if let Outcome::Completed { owner } = &self.outcome {
            let expected = self
                .request
                .candidates
                .iter()
                .find(|c| &c.id == owner)
                .context("work owner outside candidates")?;
            ensure!(
                expected.sources.iter().all(|source| self
                    .evidence
                    .iter()
                    .any(|e| &e.owner == owner && &e.source == source)),
                "work completion lacks source evidence"
            );
        }
        if let Outcome::Waiting { request_id, reason } = &self.outcome {
            ensure!(
                !request_id.is_empty()
                    && !reason.is_empty()
                    && self.owned_request.as_ref() == Some(request_id),
                "work wait lacks request identity"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 1 << 20,
            "work snapshot exceeds document bound"
        );
        Ok(())
    }

    pub(crate) fn projection(&self) -> Result<serde_json::Value> {
        let mut value = serde_json::json!({
            "version":self.version,"work_id":self.work_id,"source_rollout_id":self.source_rollout_id,
            "request_sha256":self.request_sha256,"evaluator_calls":self.evaluator_calls,
            "generation_calls":self.generation_calls,
        });
        if let Some(fallback) = &self.fallback {
            value["fallback_quote"] = serde_json::to_value(&fallback.quote)?;
        }
        let outcome = serde_json::to_value(&self.outcome)?;
        value
            .as_object_mut()
            .unwrap()
            .extend(outcome.as_object().unwrap().clone());
        if let Outcome::Completed { owner } = &self.outcome {
            value["citations"] = serde_json::json!(self.evidence.iter().filter(|e| &e.owner == owner).map(|e| serde_json::json!({
                "path":e.source.path,"start_line":e.source.start_line,"end_line":e.source.end_line,"sha256":e.sha256,
            })).collect::<Vec<_>>());
            ensure!(
                serde_json::to_vec(&value)?.len() <= 8192,
                "completed work exceeds result bound"
            );
        }
        if let Outcome::Waiting { request_id, .. } = &self.outcome {
            let pending = self.waiting_request();
            value["pending_request"] = serde_json::json!({"id":request_id,"prompt":pending.prompt,
                "options":{"request_id":pending.request_id,"title":pending.title,"questions":pending.questions}});
        }
        Ok(value)
    }
}

fn next(previous: &Snapshot, snapshot: &Snapshot) -> Result<()> {
    ensure!(
        previous.work_id == snapshot.work_id
            && previous.source_rollout_id == snapshot.source_rollout_id
            && previous.request == snapshot.request
            && previous.request_sha256 == snapshot.request_sha256,
        "work identity changed after admission"
    );
    ensure!(
        matches!(previous.outcome, Outcome::Started | Outcome::Waiting { .. }),
        "work terminal evidence conflicts"
    );
    ensure!(
        snapshot.evaluator_calls >= previous.evaluator_calls
            && snapshot.generation_calls >= previous.generation_calls,
        "work replenished spent attempts"
    );
    ensure!(
        previous.evidence.is_empty() || previous.evidence == snapshot.evidence,
        "work evidence changed after capture"
    );
    ensure!(
        previous.fallback.is_none() || previous.fallback == snapshot.fallback,
        "fallback billing provenance changed"
    );
    ensure!(
        previous.owned_request.is_none() || previous.owned_request == snapshot.owned_request,
        "work request identity changed"
    );
    if matches!(previous.outcome, Outcome::Waiting { .. }) {
        ensure!(
            !matches!(snapshot.outcome, Outcome::Started),
            "waiting work cannot restart"
        );
        ensure!(
            previous.evaluator_calls == snapshot.evaluator_calls
                && previous.generation_calls == snapshot.generation_calls,
            "wait response cannot dispatch another model"
        );
    }
    Ok(())
}

pub(super) fn recover(events: &[EventRecord]) -> Result<Option<Snapshot>> {
    let mut works = std::collections::HashMap::<String, Snapshot>::new();
    let mut acknowledged_waits = std::collections::HashSet::new();
    let mut latest = None;
    for event in events {
        if event.event_type == WAIT_ACK_TYPE {
            let ack: WaitAcknowledged = serde_json::from_value(event.payload.clone())?;
            let work = works
                .get(&ack.work_id)
                .context("wait acknowledgement lacks work")?;
            ensure!(
                matches!(&work.outcome, Outcome::Waiting { request_id, .. } if request_id == &ack.request_id)
                    && work.request_sha256 == ack.request_sha256,
                "wait acknowledgement changed request ownership"
            );
            acknowledged_waits.insert(ack.work_id);
            continue;
        }
        if event.event_type != EVENT_TYPE {
            continue;
        }
        let snapshot: Snapshot = serde_json::from_value(event.payload.clone())?;
        snapshot.validate()?;
        ensure!(
            snapshot.outcome != Outcome::Interrupted,
            "interrupted work is a recovery projection"
        );
        if let Some(previous) = works.get(&snapshot.work_id) {
            if previous == &snapshot {
                continue;
            }
            next(previous, &snapshot)?;
        } else {
            ensure!(
                snapshot.outcome == Outcome::Started
                    && snapshot.evidence.is_empty()
                    && snapshot.evaluator_calls == 0
                    && snapshot.generation_calls == 0,
                "work evidence lacks an acknowledged initial start"
            );
        }
        latest = Some(snapshot.work_id.clone());
        works.insert(snapshot.work_id.clone(), snapshot);
    }
    let mut snapshot = latest.and_then(|id| works.remove(&id));
    if let Some(work) = &mut snapshot
        && (work.outcome == Outcome::Started
            || (matches!(work.outcome, Outcome::Waiting { .. })
                && !acknowledged_waits.contains(&work.work_id)))
    {
        work.outcome = Outcome::Interrupted;
    }
    Ok(snapshot)
}

impl super::AgentMachine {
    pub(crate) async fn persist_owner_wait(&mut self, snapshot: Snapshot) -> Result<()> {
        let Outcome::Waiting { request_id, .. } = &snapshot.outcome else {
            anyhow::bail!("wait acknowledgement requires Waiting work");
        };
        let ack = WaitAcknowledged {
            work_id: snapshot.work_id.clone(),
            request_id: request_id.clone(),
            request_sha256: snapshot.request_sha256.clone(),
        };
        self.persist_owner_work(snapshot).await?;
        // Only enqueue after the wait flush acknowledged; missing witness recovers Interrupted.
        self.recorder
            .as_ref()
            .unwrap()
            .record_nowait(RolloutItem::Event(EventRecord {
                event_type: WAIT_ACK_TYPE.into(),
                payload: serde_json::to_value(ack)?,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }))
    }

    pub(crate) async fn persist_owner_work(&mut self, snapshot: Snapshot) -> Result<()> {
        snapshot.validate()?;
        ensure!(
            snapshot.outcome != Outcome::Interrupted,
            "cannot dispatch a recovered work start"
        );
        let recorder = self
            .recorder
            .as_ref()
            .context("owner work requires durable rollout storage")?;
        recorder.flush().await?;
        let mut events: Vec<_> = RolloutRecorder::load_history(recorder.path())
            .await?
            .into_iter()
            .filter_map(|item| {
                if let RolloutItem::Event(e) = item {
                    Some(e)
                } else {
                    None
                }
            })
            .collect();
        for event in events.iter().filter(|e| e.event_type == EVENT_TYPE) {
            let previous: Snapshot = serde_json::from_value(event.payload.clone())?;
            if previous == snapshot {
                ensure!(
                    self.owner_work.as_ref() == Some(&snapshot),
                    "acknowledged work must be reconciled, never redispatched"
                );
                return Ok(());
            }
        }
        ensure!(snapshot.source_rollout_id == recorder.rollout_id()
            || events.iter().any(|e| e.event_type == EVENT_TYPE && e.payload["work_id"] == snapshot.work_id),
            "new work evidence belongs to another rollout");
        let event = EventRecord {
            event_type: EVENT_TYPE.into(),
            payload: serde_json::to_value(&snapshot)?,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        events.push(event.clone());
        recover(&events)?;
        recorder
            .persist_batch(vec![RolloutItem::Event(event)])
            .await?;
        if !matches!(snapshot.outcome, Outcome::Started | Outcome::Waiting { .. }) {
            if let Some(request_id) = &snapshot.owned_request {
                self.take_pending(request_id);
            }
            self.set_turn_activity(super::TurnActivityState::Idle);
        }
        self.owner_work = Some(snapshot);
        Ok(())
    }
}

#[cfg(test)]
#[path = "owner_work/tests.rs"]
mod tests;
