//! Task budget setup, visible pause and explicit allowance extension.

use alan_agent_protocol::{Event, YieldKind};
use anyhow::{Context, Result};

use crate::agent_machine::task_budget::{Exhausted, Extension, Phase, TaskBudget};
use crate::approval::{PendingConfirmation, TASK_BUDGET_CHECKPOINT_TYPE};

use super::transition::RuntimeLoopState;
use crate::agent_machine::PendingToolReplayBatch;
use crate::agent_machine::task_budget::waiting::{WAIT_EVENT, Wait};
use crate::rollout::{EventRecord, RolloutItem};

#[cfg(test)]
mod tests;

fn extension(
    spent: &crate::agent_machine::task_budget::Snapshot,
    limits: &crate::TaskBudgetLimits,
    owner: Option<&crate::agent_machine::owner_work::Snapshot>,
) -> Result<Extension> {
    let time_limit = spent.limits.execution_seconds.get().saturating_mul(1000);
    let mut additional = Extension {
        execution_seconds: if spent.elapsed_millis >= time_limit {
            spent
                .elapsed_millis
                .saturating_sub(time_limit)
                .div_ceil(1000)
                .checked_add(limits.execution_seconds.get())
                .context("task budget time extension overflow")?
        } else {
            0
        },
        tool_calls: if spent.tool_calls >= spent.limits.tool_calls.get() {
            limits.tool_calls.get()
        } else {
            0
        },
        model_calls: if spent.model_calls >= spent.limits.model_calls.get() {
            limits.model_calls.get()
        } else {
            0
        },
    };
    if let Some(work) = owner
        && let Some(selected) = &work.selected_owner
    {
        let sources = work
            .request
            .candidates
            .iter()
            .find(|candidate| &candidate.id == selected)
            .context("budget continuation selected an unknown owner")?
            .sources
            .len();
        let remaining = spent
            .limits
            .tool_calls
            .get()
            .saturating_sub(spent.tool_calls);
        additional.tool_calls = additional.tool_calls.max(
            u64::try_from(sources)
                .context("source count exceeds Tool allowance")?
                .saturating_sub(remaining),
        );
    }
    Ok(additional)
}

fn confirmation_summary(
    title: &str,
    spent: &crate::agent_machine::task_budget::Snapshot,
    additional: &Extension,
) -> String {
    let additions = [
        (additional.execution_seconds, "second"),
        (additional.tool_calls, "Tool call"),
        (additional.model_calls, "model call"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, unit)| format!("{count} {unit}{}", if count == 1 { "" } else { "s" }))
    .collect::<Vec<_>>()
    .join(", ");
    let proposal = if additions.is_empty() {
        "Resume using remaining allowance; no additional budget".into()
    } else {
        format!("Additional allowance: {additions}")
    };
    let time_note = if spent.recovered_time_upper_bound {
        " (recovered upper bound)"
    } else {
        ""
    };
    format!(
        "{title}\nUsed: time {}.{:03}/{}s{time_note}; tools {}/{}; models {}/{}\n{proposal}",
        spent.elapsed_millis / 1000,
        spent.elapsed_millis % 1000,
        spent.limits.execution_seconds,
        spent.tool_calls,
        spent.limits.tool_calls,
        spent.model_calls,
        spent.limits.model_calls,
    )
}

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
            TaskBudget::start_with_parent(
                id,
                state.core_config.task_budget.clone(),
                state.machine.recorder(),
                state.environment.task_budget_binding.parent(),
            )
            .await?,
        );
    } else if let Some(budget) = &state.machine.task_budget {
        budget.resume().await?;
    }
    if let Some(budget) = &mut state.machine.task_budget {
        state.environment.task_budget_binding.bind(budget).await?;
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
    let owner = state
        .machine
        .owner_work
        .as_ref()
        .filter(|work| work.outcome == crate::agent_machine::owner_work::Outcome::BudgetPaused);
    let additional = extension(&snapshot, limits, owner)?;
    let summary = confirmation_summary(
        &format!("Task budget reached: {}", exhausted.dimension),
        &snapshot,
        &additional,
    );
    let generation_binding = state
        .environment
        .active_binding
        .read()
        .expect("active binding snapshot")
        .as_ref()
        .map(|(binding, _)| binding.clone());
    let pending = PendingConfirmation {
        checkpoint_id: format!("task_budget_{}", uuid::Uuid::new_v4()),
        checkpoint_type: TASK_BUDGET_CHECKPOINT_TYPE.into(),
        summary: summary.clone(),
        details: serde_json::json!({
            "kind": TASK_BUDGET_CHECKPOINT_TYPE,
            "budget": snapshot,
            "additional": additional,
            "generation_binding": generation_binding,
            "owner_work": state.machine.owner_work.as_ref().filter(|work|
                work.outcome == crate::agent_machine::owner_work::Outcome::BudgetPaused),
        }),
        options: vec!["reject".into(), "approve".into()],
    };
    let wait = Wait {
        checkpoint: pending,
        continuation,
        resume_only: false,
    };
    wait.budget()?;
    crate::agent_machine::owner_work::checkpoint_work(&wait.checkpoint)?;
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
    let owner_work = state.machine.budget_owner_work(&wait.checkpoint)?;
    anyhow::ensure!(
        wait.budget()?.task_id == actual.task_id,
        "budget wait belongs to a different task"
    );
    let value = wait
        .checkpoint
        .details
        .get("generation_binding")
        .context("budget wait lacks admitted generation binding evidence")?;
    if !value.is_null() {
        let binding: super::model_binding::InputBinding = serde_json::from_value(value.clone())?;
        let active = state
            .environment
            .active_binding
            .read()
            .expect("active binding snapshot")
            .as_ref()
            .map(|(binding, _)| binding.clone());
        if active.as_ref() != Some(&binding) {
            let callable = state
                .environment
                .model_bindings
                .lock()
                .await
                .restore(&wait.budget()?.task_id, &binding)
                .await?;
            *state
                .environment
                .active_binding
                .write()
                .expect("active binding snapshot") = Some((binding, callable));
        }
    }
    if wait.budget()? != actual {
        let limits = &state.core_config.task_budget;
        let additional = extension(&actual, limits, owner_work.as_ref())?;
        wait.resume_only = additional == Extension::default();
        wait.checkpoint.checkpoint_id = format!("task_budget_{}", uuid::Uuid::new_v4());
        wait.checkpoint.details["budget"] = serde_json::to_value(&actual)?;
        wait.checkpoint.details["additional"] = serde_json::to_value(&additional)?;
        wait.checkpoint.summary =
            confirmation_summary("Resume retained task", &actual, &additional);
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
    if let Some(work) = owner_work {
        state
            .environment
            .publish_work(Some(work.projection()?))
            .await?;
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
    machine.budget_owner_work(pending)?;
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
