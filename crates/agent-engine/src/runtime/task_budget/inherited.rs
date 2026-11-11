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

/// Build the pending decision this stop publishes.
///
/// It offers no local options: the child cannot grant an allowance its ancestor
/// owns, so the only decisions here are "the ancestor authorized a resume" or
/// "stop". Persisting it lets a relaunched child republish the same identity.
fn checkpoint(
    exhausted: &InheritedExhaustion,
    budget: &crate::agent_machine::task_budget::Snapshot,
) -> PendingConfirmation {
    let dimension = serde_json::to_value(exhausted.dimension)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    PendingConfirmation {
        checkpoint_id: format!(
            "{INHERITED_BUDGET_CHECKPOINT_TYPE}_{}",
            uuid::Uuid::new_v4()
        ),
        checkpoint_type: INHERITED_BUDGET_CHECKPOINT_TYPE.into(),
        summary: format!(
            "Parent task budget reached: ancestor {} exhausted {dimension}. Only that ancestor can extend it.",
            exhausted.ancestor_task_id
        ),
        details: serde_json::json!({
            "kind": INHERITED_BUDGET_CHECKPOINT_TYPE,
            "ancestor_task_id": exhausted.ancestor_task_id,
            "dimension": exhausted.dimension,
            "budget": budget,
        }),
        options: Vec::new(),
    }
}

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
    let snapshot = budget.snapshot().await;
    let wait = InheritedWait {
        budget: snapshot.clone(),
        ancestor_task_id: exhausted.ancestor_task_id.clone(),
        dimension: exhausted.dimension,
        continuation,
        checkpoint: checkpoint(exhausted, &snapshot),
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

/// Republish a recovered inherited stop so a relaunched child exposes the same
/// request identity its ancestor recorded. Without this the ancestor's resume
/// would target a request that does not exist in the new invocation.
pub(crate) async fn publish_inherited_wait(state: &mut RuntimeLoopState) -> Result<Option<String>> {
    let Some(wait) = state.machine.inherited_budget_wait.clone() else {
        return Ok(None);
    };
    if state.machine.has_pending_interaction() {
        return Ok(None);
    }
    let id = super::register_wait(state, wait.checkpoint, wait.continuation).await?;
    Ok(Some(id))
}

/// Lift a recorded inherited stop once its owning ancestor admits again.
///
/// The child grants nothing itself; it only observes that the recorded ancestor
/// extended. Missing ancestry or a changed task identity leaves the wait in place.
pub(crate) async fn lift_inherited(state: &mut RuntimeLoopState) -> Result<bool> {
    let Some(wait) = state.machine.inherited_budget_wait.clone() else {
        return Ok(false);
    };
    let budget = state
        .machine
        .task_budget
        .as_ref()
        .context("inherited wait lacks child budget")?;
    ensure!(
        budget.snapshot().await.task_id == wait.budget.task_id,
        "inherited wait belongs to a different task"
    );
    if !budget
        .ancestor_admits(&wait.ancestor_task_id, wait.dimension)
        .await
        .unwrap_or(false)
    {
        return Ok(false);
    }
    state.machine.inherited_budget_wait = None;
    Ok(true)
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
    let id = super::register_wait(state, wait.checkpoint, wait.continuation.clone()).await?;
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

/// Answer a recorded inherited stop.
///
/// An approval is the ancestor's authorization to continue, not an allowance:
/// the wait is cleared and the preserved continuation replays through the
/// ordinary budget path exactly once. A rejection settles the child subtree.
/// Nothing here can extend the ancestor's ledger.
pub(crate) async fn resolve_inherited(
    machine: &mut crate::agent_machine::AgentMachine,
    pending: &PendingConfirmation,
    approved: bool,
) -> Result<()> {
    let wait = machine
        .inherited_budget_wait
        .as_ref()
        .context("inherited budget resolution lacks its recorded wait")?
        .clone();
    ensure!(
        wait.checkpoint == *pending,
        "inherited budget resolution is not the recorded wait"
    );
    if !approved {
        let budget = machine
            .task_budget
            .as_ref()
            .context("inherited budget resolution lacks its ledger")?;
        budget.settle(true).await?;
    }
    machine.inherited_budget_wait = None;
    Ok(())
}
