//! Correlated budget wait evidence. Invocation-local AgentFS request IDs are not durable.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{Extension, Phase, Snapshot};
use crate::agent_machine::PendingToolReplayBatch;
use crate::approval::{PendingConfirmation, TASK_BUDGET_CHECKPOINT_TYPE};
use crate::rollout::EventRecord;

pub(crate) const WAIT_EVENT: &str = "agent_task_budget_wait_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Wait {
    pub checkpoint: PendingConfirmation,
    pub continuation: Option<PendingToolReplayBatch>,
    #[serde(default)]
    pub resume_only: bool,
}

impl Wait {
    pub(crate) fn budget(&self) -> Result<Snapshot> {
        let budget: Snapshot = serde_json::from_value(self.checkpoint.details["budget"].clone())
            .context("budget wait lacks its correlated accounting snapshot")?;
        budget.validate()?;
        let additional: Extension =
            serde_json::from_value(self.checkpoint.details["additional"].clone())
                .context("budget wait lacks its explicit extension")?;
        ensure!(
            self.checkpoint.checkpoint_type == TASK_BUDGET_CHECKPOINT_TYPE
                && self.checkpoint.checkpoint_id.starts_with("task_budget_")
                && budget.phase == Phase::Paused
                && !budget.foreground_complete
                && self.checkpoint.options == ["reject", "approve"]
                && (self.resume_only == (additional == Extension::default())),
            "invalid durable task budget wait"
        );
        Ok(budget)
    }
}

/// Retain ordered work across extension and execution until foreground settlement.
/// A new invocation must bind its fresh confirmation to current accounting.
pub(crate) fn recover(events: &[EventRecord], latest: Option<&Snapshot>) -> Result<Option<Wait>> {
    let Some(latest) = latest.filter(|snapshot| !snapshot.foreground_complete) else {
        return Ok(None);
    };
    let mut wait = None;
    for event in events.iter().filter(|event| event.event_type == WAIT_EVENT) {
        let candidate: Wait = serde_json::from_value(event.payload.clone())
            .context("invalid durable task budget continuation")?;
        candidate.budget()?;
        wait = Some(candidate);
    }
    if let Some(wait) = &wait {
        let original = wait.budget()?;
        if original.task_id != latest.task_id {
            return Ok(None);
        }
        ensure!(
            original.model_calls <= latest.model_calls && original.tool_calls <= latest.tool_calls,
            "budget wait consumption exceeds current accounting"
        );
    }
    Ok(wait)
}
