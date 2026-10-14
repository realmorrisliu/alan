//! Task budget setup, visible pause and explicit allowance extension.

use alan_agent_protocol::{Event, YieldKind};
use anyhow::{Context, Result};

use crate::agent_machine::task_budget::{Dimension, Exhausted, Extension, Phase, TaskBudget};
use crate::approval::{PendingConfirmation, TASK_BUDGET_CHECKPOINT_TYPE};

use super::transition::RuntimeLoopState;
use crate::agent_machine::PendingToolReplayBatch;
use crate::agent_machine::task_budget::waiting::{WAIT_EVENT, Wait};
use crate::rollout::{EventRecord, RolloutItem};

#[cfg(test)]
mod tests;

pub(crate) async fn begin(state: &mut RuntimeLoopState) -> Result<()> {
    anyhow::ensure!(
        state.machine.task_budget_wait.is_none(),
        "task is waiting for explicit budget extension"
    );
    let starts_new = match &state.machine.task_budget {
        Some(budget) => {
            let snapshot = budget.snapshot().await;
            snapshot.foreground_complete || snapshot.phase == Phase::Completed
        }
        None => true,
    };
    if starts_new {
        let id = state
            .machine
            .current_submission_id()
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        state.machine.task_budget = Some(
            TaskBudget::start(
                id,
                state.core_config.task_budget.clone(),
                state.machine.recorder(),
            )
            .await?,
        );
    } else if let Some(budget) = &state.machine.task_budget {
        budget.resume().await?;
    }
    Ok(())
}

pub(crate) async fn settle_foreground(state: &RuntimeLoopState) -> Result<()> {
    if let Some(budget) = &state.machine.task_budget {
        if state.machine.has_pending_interaction() {
            budget.settle(false).await?;
        } else {
            budget.finish_foreground().await?;
        }
    }
    Ok(())
}

pub(crate) async fn pause_tool_batch<E, F>(
    state: &mut RuntimeLoopState,
    exhausted: &Exhausted,
    calls: &[crate::agent_machine::NormalizedToolCall],
    resume_with_generation: bool,
    emit: &mut E,
) -> Result<bool>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if state.machine.has_pending_interaction() {
        return Ok(true);
    }
    let tool_calls = calls
        .iter()
        .filter(|call| state.machine.tool_payload_by_call_id(&call.id).is_none())
        .cloned()
        .collect::<Vec<_>>();
    // Cancellation cleanup may already have recorded the explicit command's
    // terminal result. There is no remaining work to authorize in that case.
    if tool_calls.is_empty() && !resume_with_generation {
        return Ok(false);
    }
    pause(
        state,
        exhausted,
        Some(PendingToolReplayBatch {
            tool_calls,
            resume_with_generation,
        }),
        emit,
    )
    .await?;
    Ok(true)
}

pub(crate) async fn pause<E, F>(
    state: &mut RuntimeLoopState,
    exhausted: &Exhausted,
    continuation: Option<PendingToolReplayBatch>,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    anyhow::ensure!(
        state.machine.task_budget_wait.is_none(),
        "task already has a budget wait"
    );
    let budget = state
        .machine
        .task_budget
        .as_ref()
        .context("missing task budget")?;
    budget.settle(false).await?;
    let snapshot = budget.snapshot().await;
    let limits = &state.core_config.task_budget;
    let additional = match exhausted.dimension {
        Dimension::Time => Extension {
            execution_seconds: limits.execution_seconds.get(),
            ..Default::default()
        },
        Dimension::ToolCalls => Extension {
            tool_calls: limits.tool_calls.get(),
            ..Default::default()
        },
        Dimension::ModelCalls => Extension {
            model_calls: limits.model_calls.get(),
            ..Default::default()
        },
    };
    let summary = format!(
        "{}; add {}s execution, {} Tool attempts and {} model requests?",
        exhausted, additional.execution_seconds, additional.tool_calls, additional.model_calls,
    );
    let pending = PendingConfirmation {
        checkpoint_id: format!("task_budget_{}", uuid::Uuid::new_v4()),
        checkpoint_type: TASK_BUDGET_CHECKPOINT_TYPE.into(),
        summary: summary.clone(),
        details: serde_json::json!({
            "kind": TASK_BUDGET_CHECKPOINT_TYPE,
            "budget": snapshot,
            "additional": additional,
        }),
        options: vec!["reject".into(), "approve".into()],
    };
    let wait = Wait {
        checkpoint: pending,
        continuation,
        resume_only: false,
    };
    wait.budget()?;
    if let Some(recorder) = state.machine.recorder() {
        recorder
            .persist_batch(vec![RolloutItem::Event(EventRecord {
                event_type: WAIT_EVENT.into(),
                payload: serde_json::to_value(&wait)?,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })])
            .await?;
    }
    state.machine.task_budget_wait = Some(wait);
    let id = publish_wait(state)
        .await?
        .context("budget wait was not published")?;
    let pending = &state
        .machine
        .task_budget_wait
        .as_ref()
        .expect("owned wait")
        .checkpoint;
    let payload = crate::approval::runtime_confirmation_yield_payload(pending);
    emit(Event::Yield {
        request_id: id,
        kind: YieldKind::Confirmation,
        payload: serde_json::to_value(payload)?,
    })
    .await;
    Ok(())
}

/// Republish only into a newly selected invocation; live waits retain their ID.
pub(crate) async fn publish_wait(state: &mut RuntimeLoopState) -> Result<Option<String>> {
    let Some(mut wait) = state.machine.task_budget_wait.clone() else {
        return Ok(None);
    };
    if state
        .machine
        .pending_confirmation()
        .is_some_and(|pending| pending.checkpoint_id == wait.checkpoint.checkpoint_id)
    {
        return Ok(None);
    }
    if state.machine.has_pending_interaction() {
        state.machine.task_budget_wait = None;
        return Ok(None);
    }
    let actual = state
        .machine
        .task_budget
        .as_ref()
        .context("budget wait lacks its ledger")?
        .snapshot()
        .await;
    if wait.budget()? != actual {
        let limits = &state.core_config.task_budget;
        let additional = Extension {
            execution_seconds: if actual.elapsed_millis
                >= actual.limits.execution_seconds.get().saturating_mul(1000)
            {
                limits.execution_seconds.get()
            } else {
                0
            },
            tool_calls: if actual.tool_calls >= actual.limits.tool_calls.get() {
                limits.tool_calls.get()
            } else {
                0
            },
            model_calls: if actual.model_calls >= actual.limits.model_calls.get() {
                limits.model_calls.get()
            } else {
                0
            },
        };
        wait.resume_only = additional == Extension::default();
        wait.checkpoint.checkpoint_id = format!("task_budget_{}", uuid::Uuid::new_v4());
        wait.checkpoint.details["budget"] = serde_json::to_value(actual)?;
        wait.checkpoint.details["additional"] = serde_json::to_value(&additional)?;
        wait.checkpoint.summary = if wait.resume_only {
            "Resume retained work using the previously approved remaining allowance?".into()
        } else {
            format!(
                "Recovered task; retain spent counts and add {}s execution, {} Tool attempts and {} model requests? Unsettled execution time is conservatively accounted.",
                additional.execution_seconds, additional.tool_calls, additional.model_calls
            )
        };
        if let Some(batch) = &mut wait.continuation {
            batch
                .tool_calls
                .retain(|call| state.machine.tool_payload_by_call_id(&call.id).is_none());
        }
        if let Some(recorder) = state.machine.recorder() {
            recorder
                .persist_batch(vec![RolloutItem::Event(EventRecord {
                    event_type: WAIT_EVENT.into(),
                    payload: serde_json::to_value(&wait)?,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                })])
                .await?;
        }
        state.machine.task_budget_wait = Some(wait.clone());
    }
    let pending = wait.checkpoint;
    let id = state
        .agent_files()
        .write_confirmation_request(&pending)
        .await?;
    state
        .machine
        .set_confirmation_for_request(id.clone(), pending.clone());
    if let Some(batch) = wait.continuation {
        state.machine.set_tool_replay_batch(
            pending.checkpoint_id,
            batch.tool_calls,
            batch.resume_with_generation,
        );
    }
    state
        .machine
        .set_turn_activity(crate::agent_machine::TurnActivityState::Paused);
    super::ui_surfaces::paused(&state.agent_files(), Some(&state.machine)).await?;
    Ok(Some(id))
}

pub(crate) async fn resolve(
    machine: &mut crate::agent_machine::AgentMachine,
    pending: &PendingConfirmation,
    approved: bool,
) -> Result<()> {
    anyhow::ensure!(
        machine
            .task_budget_wait
            .as_ref()
            .is_some_and(|wait| wait.checkpoint == *pending),
        "budget extension is not the Machine-owned pending decision"
    );
    let budget = machine
        .task_budget
        .as_ref()
        .context("missing pending task budget")?;
    let expected: crate::agent_machine::task_budget::Snapshot =
        serde_json::from_value(pending.details["budget"].clone())?;
    let actual = budget.snapshot().await;
    anyhow::ensure!(
        expected == actual,
        "task budget changed after the extension request"
    );
    if approved {
        let additional: Extension = serde_json::from_value(pending.details["additional"].clone())?;
        if !machine
            .task_budget_wait
            .as_ref()
            .expect("owned wait")
            .resume_only
        {
            budget.extend(&additional).await?;
        } else {
            anyhow::ensure!(
                additional == Extension::default(),
                "resume-only decision cannot add allowance"
            );
        }
        budget.resume().await?;
    } else {
        budget.settle(true).await?;
    }
    machine.task_budget_wait = None;
    Ok(())
}
