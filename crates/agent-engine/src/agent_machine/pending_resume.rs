//! Durable record of child work paused by an ancestor-owned budget.
//!
//! A delegated child that stops on an inherited budget cannot observe its
//! ancestor across the process boundary, so the parent must remember which
//! request to reconnect. The record is durable because the reconnect may only
//! happen after a later extension, possibly in a new invocation.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::rollout::{EventRecord, RolloutItem};

pub(crate) const PENDING_RESUME_EVENT: &str = "agent_delegated_resume_v1";

/// One reconnect decision, recorded where the authorization was answered.
pub(crate) const INHERITED_RESUME_EVENT: &str = "agent_inherited_resume_v1";

/// Record what a durable reconnect found, so a silent no-op is visible in history.
pub(crate) async fn record_decision(
    machine: &super::AgentMachine,
    checkpoint_id: &str,
    inherited_wait: bool,
    replayed_batch: bool,
) -> Result<()> {
    let Some(recorder) = machine.recorder() else {
        return Ok(());
    };
    recorder
        .persist_batch(vec![RolloutItem::Event(EventRecord {
            event_type: INHERITED_RESUME_EVENT.into(),
            payload: serde_json::json!({
                "checkpoint_id": checkpoint_id,
                "inherited_wait": inherited_wait,
                "replayed_batch": replayed_batch,
            }),
            timestamp: chrono::Utc::now().to_rfc3339(),
        })])
        .await?;
    Ok(())
}

/// One delegated tool call whose child is paused on an ancestor-owned budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingResume {
    /// The parent tool call whose child is paused.
    pub tool_call_id: String,
    /// The child request to resume instead of relaunching the skill.
    pub request_id: String,
    /// The ancestor that owns the exhausted allowance.
    pub ancestor_task_id: String,
    /// Exhausted dimension as reported by the paused child.
    pub dimension: String,
    /// Execution record of the paused child, when it reported one.
    ///
    /// A resume launch recovers this record so the new Process owns the same
    /// Agent Machine state, including the preserved Tool batch.
    #[serde(default)]
    pub rollout_path: String,
}

/// Everything a resume launch needs to reconnect one paused child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResumeTarget {
    /// The checkpoint the ancestor authorized.
    pub request_id: String,
    /// Execution record to recover, when the child reported one.
    pub rollout_path: Option<String>,
}

impl PendingResume {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.tool_call_id.is_empty()
                && !self.request_id.is_empty()
                && !self.ancestor_task_id.is_empty(),
            "paused delegated child lacks its tool call, request or ancestor identity"
        );
        Ok(())
    }
}

/// Keep at most this many paused children reconnects; older entries are dropped.
const MAX_PENDING_RESUMES: usize = 16;

/// Record that a delegated child is waiting on `ancestor_task_id`.
///
/// Re-recording the same tool call replaces its entry so a repeated observation
/// cannot accumulate duplicates.
pub(crate) async fn record(machine: &mut super::AgentMachine, resume: PendingResume) -> Result<()> {
    resume.validate()?;
    if machine.pending_delegated_resumes.last() == Some(&resume) {
        return Ok(());
    }
    machine
        .pending_delegated_resumes
        .retain(|existing| existing.tool_call_id != resume.tool_call_id);
    if machine.pending_delegated_resumes.len() == MAX_PENDING_RESUMES {
        machine.pending_delegated_resumes.remove(0);
    }
    if let Some(recorder) = machine.recorder() {
        recorder
            .persist_batch(vec![RolloutItem::Event(EventRecord {
                event_type: PENDING_RESUME_EVENT.into(),
                payload: serde_json::to_value(&resume)?,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })])
            .await?;
    }
    machine.pending_delegated_resumes.push(resume);
    Ok(())
}

/// The reconnect for `tool_call_id`, when one is recorded.
pub(crate) fn resume_target_for(
    resumes: &[PendingResume],
    tool_call_id: &str,
) -> Option<ResumeTarget> {
    resumes
        .iter()
        .find(|pending| pending.tool_call_id == tool_call_id)
        .map(|pending| ResumeTarget {
            request_id: pending.request_id.clone(),
            rollout_path: (!pending.rollout_path.is_empty()).then(|| pending.rollout_path.clone()),
        })
}

/// Rebuild the recorded paused children from a rollout.
///
/// A record that no longer validates is a corrupt history rather than a reason
/// to reconnect something else, so recovery fails instead of guessing.
pub(crate) fn recover(events: &[EventRecord]) -> Result<Vec<PendingResume>> {
    let mut recovered: Vec<PendingResume> = Vec::new();
    for event in events
        .iter()
        .filter(|event| event.event_type == PENDING_RESUME_EVENT)
    {
        let resume: PendingResume = serde_json::from_value(event.payload.clone())
            .context("invalid paused delegated child record")?;
        resume.validate()?;
        recovered.retain(|existing: &PendingResume| existing.tool_call_id != resume.tool_call_id);
        recovered.push(resume);
    }
    Ok(recovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(payload: serde_json::Value) -> EventRecord {
        EventRecord {
            event_type: PENDING_RESUME_EVENT.into(),
            payload,
            timestamp: String::new(),
        }
    }

    fn sample(tool_call_id: &str) -> PendingResume {
        PendingResume {
            tool_call_id: tool_call_id.into(),
            request_id: format!("inherited_budget-{tool_call_id}"),
            ancestor_task_id: "ancestor-1".into(),
            dimension: "model_calls".into(),
            rollout_path: String::new(),
        }
    }

    #[test]
    fn recovery_keeps_the_latest_record_per_tool_call() {
        let mut first = sample("call-1");
        let stale = event(serde_json::to_value(&first).unwrap());
        first.request_id = "inherited_budget-call-1-retry".into();
        let fresh = event(serde_json::to_value(&first).unwrap());
        let other = event(serde_json::to_value(sample("call-2")).unwrap());

        let recovered = recover(&[stale, fresh, other]).unwrap();

        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0].request_id, "inherited_budget-call-1-retry");
        assert_eq!(recovered[1].tool_call_id, "call-2");
    }

    #[test]
    fn resume_for_only_matches_the_recorded_tool_call() {
        let resumes = vec![sample("call-1")];
        assert_eq!(
            resume_target_for(&resumes, "call-1")
                .as_ref()
                .map(|target| target.request_id.as_str()),
            Some("inherited_budget-call-1")
        );
        assert!(resume_target_for(&resumes, "call-2").is_none());
        assert!(resume_target_for(&[], "call-1").is_none());
    }

    #[test]
    fn recovery_rejects_a_record_without_ancestor_identity() {
        let mut broken = sample("call-1");
        broken.ancestor_task_id = String::new();
        let error = recover(&[event(serde_json::to_value(&broken).unwrap())]).unwrap_err();
        assert!(error.to_string().contains("ancestor identity"), "{error}");
    }
}
