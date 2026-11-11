//! Preserve child work before propagating an ancestor-owned admission stop.
use alan_agent_protocol::{Event, YieldKind};

use super::RuntimeLoopState;
use crate::agent_machine::{
    PendingToolReplayBatch,
    task_budget::{
        InheritedExhaustion,
        waiting::{INHERITED_WAIT_EVENT, InheritedWait},
    },
};
use crate::approval::PendingConfirmation;
use crate::rollout::{EventRecord, RolloutItem};
use anyhow::{Context, Result, ensure};

/// Request kind the delegated supervisor reads back for an inherited stop.
pub(crate) use crate::approval::INHERITED_BUDGET_CHECKPOINT_TYPE;

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

/// Pause the child on an ancestor-owned stop instead of failing it.
///
/// The child cannot grant this allowance itself, so the published decision has no
/// local options; only the recorded ancestor may extend it. Callers translate a
/// `true` return into a paused turn.
pub(crate) async fn pause_inherited<E, F>(
    state: &mut RuntimeLoopState,
    exhausted: &InheritedExhaustion,
    continuation: Option<PendingToolReplayBatch>,
    emit: &mut E,
) -> Result<bool>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if state.machine.has_pending_interaction() {
        return Ok(true);
    }
    preserve_inherited_wait(state, exhausted, continuation).await?;
    let Some(wait) = state.machine.inherited_budget_wait.clone() else {
        return Ok(false);
    };
    let dimension = serde_json::to_value(wait.dimension)?.to_string();
    let pending = PendingConfirmation {
        checkpoint_id: format!(
            "{INHERITED_BUDGET_CHECKPOINT_TYPE}_{}",
            uuid::Uuid::new_v4()
        ),
        checkpoint_type: INHERITED_BUDGET_CHECKPOINT_TYPE.into(),
        summary: format!(
            "Parent task budget reached: ancestor {} exhausted {dimension}. Only that ancestor can extend it.",
            wait.ancestor_task_id
        ),
        details: serde_json::json!({
            "kind": INHERITED_BUDGET_CHECKPOINT_TYPE,
            "ancestor_task_id": wait.ancestor_task_id,
            "dimension": wait.dimension,
            "budget": wait.budget,
        }),
        options: Vec::new(),
    };
    let id = super::register_wait(state, pending, wait.continuation.clone()).await?;
    emit(Event::Yield {
        request_id: id,
        kind: YieldKind::Custom(INHERITED_BUDGET_CHECKPOINT_TYPE.into()),
        payload: serde_json::json!({
            "kind": INHERITED_BUDGET_CHECKPOINT_TYPE,
            "ancestor_task_id": wait.ancestor_task_id,
            "dimension": wait.dimension,
        }),
    })
    .await;
    Ok(true)
}
