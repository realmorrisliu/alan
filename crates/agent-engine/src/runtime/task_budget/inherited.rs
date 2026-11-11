//! Preserve child work before propagating an ancestor-owned admission stop.
use super::RuntimeLoopState;
use crate::agent_machine::{
    PendingToolReplayBatch,
    task_budget::{
        InheritedExhaustion,
        waiting::{INHERITED_WAIT_EVENT, InheritedWait},
    },
};
use crate::rollout::{EventRecord, RolloutItem};
use anyhow::{Context, Result, ensure};

pub(crate) async fn preserve_inherited_wait(
    state: &mut RuntimeLoopState,
    exhausted: &InheritedExhaustion,
    continuation: Option<PendingToolReplayBatch>,
) -> Result<()> {
    state.environment.operation_cleanup.check_admission()?;
    let budget = state
        .machine
        .task_budget
        .as_ref()
        .context("inherited wait lacks child budget")?;
    if let Some(wait) = &state.machine.inherited_budget_wait {
        ensure!(
            wait.budget.task_id == budget.snapshot().await.task_id
                && wait.ancestor_task_id == exhausted.ancestor_task_id,
            "inherited wait belongs to different task ancestry"
        );
        return Ok(());
    }
    budget.settle(false).await?;
    let wait = InheritedWait {
        budget: budget.snapshot().await,
        ancestor_task_id: exhausted.ancestor_task_id.clone(),
        dimension: exhausted.dimension,
        continuation,
    };
    wait.validate()?;
    if let Some(recorder) = state.machine.recorder() {
        recorder
            .persist_batch(vec![RolloutItem::Event(EventRecord {
                event_type: INHERITED_WAIT_EVENT.into(),
                payload: serde_json::to_value(&wait)?,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })])
            .await?;
    }
    state.machine.inherited_budget_wait = Some(wait);
    Ok(())
}
