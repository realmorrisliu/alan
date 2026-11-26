//! Task budget setup, visible pause and explicit allowance extension.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alan_agent_protocol::{Event, UiBudgetSnapshot, YieldKind};
use anyhow::{Context, Result};

use crate::agent_machine::task_budget::{
    Exhausted, Extension, Phase, TaskBudget, TaskBudgetBinding,
};
use crate::approval::{PendingConfirmation, TASK_BUDGET_CHECKPOINT_TYPE};

use super::transition::{NamespaceAgentFiles, RuntimeLoopState};
use crate::agent_machine::PendingToolReplayBatch;
use crate::agent_machine::task_budget::waiting::{WAIT_EVENT, Wait};
use crate::rollout::{EventRecord, RolloutItem};

mod auto_extend;
pub(crate) use auto_extend::{MAX_SMART_AUTO_EXTENSIONS, try_auto_extend};

mod inherited;
pub(crate) use inherited::pause_inherited;
#[cfg(test)]
pub(crate) use inherited::preserve_inherited_wait;
pub(crate) use inherited::publish_inherited_wait;
pub(crate) use inherited::resolve_inherited;

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
    if !starts_new
        && state.machine.inherited_budget_wait.is_some()
        && !inherited::lift_inherited(state).await?
    {
        anyhow::bail!(
            "child task is waiting for its original ancestor budget; ordinary continuation is forbidden"
        );
    }
    if starts_new {
        state.machine.inherited_budget_wait = None;
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
    publish_state(state).await;
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
    publish_state(state).await;
    Ok(())
}

pub(crate) fn to_ui_snapshot(
    snapshot: &crate::agent_machine::task_budget::Snapshot,
    smart_auto_extensions_enabled: bool,
) -> UiBudgetSnapshot {
    UiBudgetSnapshot {
        version: alan_agent_protocol::UI_SURFACE_VERSION,
        elapsed_secs: snapshot.elapsed_millis / 1000,
        max_secs: snapshot.limits.execution_seconds.get(),
        tool_calls: snapshot.tool_calls,
        max_tool_calls: snapshot.limits.tool_calls.get(),
        model_calls: snapshot.model_calls,
        max_model_calls: snapshot.limits.model_calls.get(),
        auto_extensions_used: smart_auto_extensions_enabled
            .then_some(snapshot.auto_extensions_used),
        max_auto_extensions: smart_auto_extensions_enabled.then_some(MAX_SMART_AUTO_EXTENSIONS),
    }
}

pub(crate) fn default_ui_snapshot(
    limits: &crate::TaskBudgetLimits,
    smart_auto_extensions_enabled: bool,
) -> UiBudgetSnapshot {
    UiBudgetSnapshot {
        version: alan_agent_protocol::UI_SURFACE_VERSION,
        elapsed_secs: 0,
        max_secs: limits.execution_seconds.get(),
        tool_calls: 0,
        max_tool_calls: limits.tool_calls.get(),
        model_calls: 0,
        max_model_calls: limits.model_calls.get(),
        auto_extensions_used: smart_auto_extensions_enabled.then_some(0),
        max_auto_extensions: smart_auto_extensions_enabled.then_some(MAX_SMART_AUTO_EXTENSIONS),
    }
}

/// Advisory publication of the live task budget snapshot to `machine/ui/budget`.
pub(crate) async fn publish(
    files: &NamespaceAgentFiles,
    budget: Option<&TaskBudget>,
    limits: &crate::TaskBudgetLimits,
    smart_auto_extensions_enabled: bool,
) {
    let snapshot = match budget {
        Some(budget) => to_ui_snapshot(&budget.snapshot().await, smart_auto_extensions_enabled),
        None => default_ui_snapshot(limits, smart_auto_extensions_enabled),
    };
    if let Err(error) = files.write_ui_budget_snapshot(&snapshot).await {
        tracing::warn!(error = %error, "Failed to publish budget snapshot");
    }
}

/// Advisory publication of the live task budget snapshot when an active budget exists.
pub(crate) async fn publish_active(
    files: &NamespaceAgentFiles,
    budget: Option<&TaskBudget>,
    smart_auto_extensions_enabled: bool,
) {
    let Some(budget) = budget else {
        return;
    };
    let snapshot = to_ui_snapshot(&budget.snapshot().await, smart_auto_extensions_enabled);
    if let Err(error) = files.write_ui_budget_snapshot(&snapshot).await {
        tracing::warn!(error = %error, "Failed to publish budget snapshot");
    }
}

pub(crate) async fn publish_state(state: &RuntimeLoopState) {
    let smart_enabled =
        state.runtime_config.trust_posture == crate::policy::TrustPosture::ConfinedSmart;
    publish(
        &state.agent_files(),
        state.machine.task_budget.as_ref(),
        &state.core_config.task_budget,
        smart_enabled,
    )
    .await;
}

/// Change-filtered live budget publisher for the submission observer loop.
pub(crate) struct BudgetObserver {
    files: NamespaceAgentFiles,
    binding: TaskBudgetBinding,
    limits: crate::TaskBudgetLimits,
    smart_auto_extensions_enabled: Arc<AtomicBool>,
    last: Option<UiBudgetSnapshot>,
}

impl BudgetObserver {
    pub(crate) fn new(state: &RuntimeLoopState) -> Self {
        Self {
            files: state.agent_files(),
            binding: state.environment.task_budget_binding.clone(),
            limits: state.core_config.task_budget.clone(),
            smart_auto_extensions_enabled: state.environment.smart_auto_extensions_enabled(),
            last: None,
        }
    }

    pub(crate) async fn observe(&mut self) {
        let smart_enabled = self.smart_auto_extensions_enabled.load(Ordering::Acquire);
        let snapshot = match self.binding.current() {
            Some(budget) => to_ui_snapshot(&budget.snapshot().await, smart_enabled),
            None => default_ui_snapshot(&self.limits, smart_enabled),
        };
        if self.last.as_ref() == Some(&snapshot) {
            return;
        }
        if let Err(error) = self.files.write_ui_budget_snapshot(&snapshot).await {
            tracing::warn!(error = %error, "Failed to publish budget snapshot");
        } else {
            self.last = Some(snapshot);
        }
    }
}

pub(crate) async fn handle_tool_batch_exhaustion<E, F>(
    state: &mut RuntimeLoopState,
    exhausted: &Exhausted,
    calls: &[crate::agent_machine::NormalizedToolCall],
    resume_with_generation: bool,
    cancel: &tokio_util::sync::CancellationToken,
    emit: &mut E,
) -> Result<Option<crate::runtime::tool_batch::ToolBatchOrchestratorOutcome>>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if state.machine.task_budget_wait.is_some() {
        return Ok(Some(
            crate::runtime::tool_batch::ToolBatchOrchestratorOutcome::PauseTurn,
        ));
    }
    let tool_calls = calls
        .iter()
        .filter(|call| state.machine.tool_payload_by_call_id(&call.id).is_none())
        .cloned()
        .collect::<Vec<_>>();
    if tool_calls.is_empty() && !resume_with_generation {
        return Ok(Some(
            crate::runtime::tool_batch::ToolBatchOrchestratorOutcome::EndTurn {
                surfaces_refreshed: false,
            },
        ));
    }
    if resume_with_generation && try_auto_extend(state, exhausted, cancel, emit).await? {
        return Ok(None);
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
    Ok(Some(
        crate::runtime::tool_batch::ToolBatchOrchestratorOutcome::PauseTurn,
    ))
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
    // Only an existing budget decision makes this pause redundant. Some other
    // pending interaction must not swallow the stop, which would leave the turn
    // paused with nothing for the operator to answer.
    if state.machine.task_budget_wait.is_some() {
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

pub(super) fn compute_extension(
    state: &RuntimeLoopState,
    snapshot: &crate::agent_machine::task_budget::Snapshot,
    exhausted: &Exhausted,
) -> Result<Extension> {
    let limits = &state.core_config.task_budget;
    let owner = state
        .machine
        .owner_work
        .as_ref()
        .filter(|work| work.outcome == crate::agent_machine::owner_work::Outcome::BudgetPaused);
    let mut additional = extension(snapshot, limits, owner)?;
    if additional == Extension::default() {
        // The stop has no allowance left for what it must do; an empty proposal
        // could not describe it and would leave the operator nothing to answer.
        additional = match exhausted.dimension {
            crate::agent_machine::task_budget::Dimension::ModelCalls => Extension {
                model_calls: limits.model_calls.get(),
                ..Extension::default()
            },
            crate::agent_machine::task_budget::Dimension::ToolCalls => Extension {
                tool_calls: limits.tool_calls.get(),
                ..Extension::default()
            },
            crate::agent_machine::task_budget::Dimension::Time => Extension {
                execution_seconds: limits.execution_seconds.get(),
                ..Extension::default()
            },
        };
    }
    Ok(additional)
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
    let additional = compute_extension(state, &snapshot, exhausted)?;
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
    // A rejected wait must not leave the ledger paused with no decision to answer:
    // settle/validate in that order and roll the pause back on any failure.
    if let Err(error) = wait
        .budget()
        .and_then(|_| crate::agent_machine::owner_work::checkpoint_work(&wait.checkpoint))
    {
        budget.resume().await?;
        return Err(error);
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
    let id = register_wait(state, wait.checkpoint, wait.continuation).await?;
    Ok(Some(id))
}

/// Record a paused decision and publish it as this invocation's pending yield.
///
/// Shared by the local `Exhausted` wait and the inherited-wait pause so both
/// reach the same UI, replay-batch and turn-activity bookkeeping.
pub(super) async fn register_wait(
    state: &mut RuntimeLoopState,
    pending: PendingConfirmation,
    continuation: Option<PendingToolReplayBatch>,
) -> Result<String> {
    let id = state
        .agent_files()
        .write_confirmation_request(&pending)
        .await?;
    state
        .machine
        .set_confirmation_for_request(id.clone(), pending.clone());
    if let Some(batch) = continuation {
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
    publish_state(state).await;
    Ok(id)
}

pub(crate) async fn resolve(
    machine: &mut crate::agent_machine::AgentMachine,
    pending: &PendingConfirmation,
    approved: bool,
) -> Result<()> {
    if pending.checkpoint_type == crate::approval::INHERITED_BUDGET_CHECKPOINT_TYPE {
        return inherited::resolve_inherited(machine, pending, approved).await;
    }
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

#[cfg(test)]
mod ui_budget_tests {
    use super::*;
    use std::num::NonZeroU64;
    use std::sync::Arc;

    use alan_agentfs::{AgentFs, AgentRootFs};
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace, ProcFs};

    #[test]
    fn default_and_active_ui_snapshots_project_task_budget_fields() {
        let limits = crate::TaskBudgetLimits {
            execution_seconds: NonZeroU64::new(1800).unwrap(),
            tool_calls: NonZeroU64::new(256).unwrap(),
            model_calls: NonZeroU64::new(64).unwrap(),
        };
        let default_snap = default_ui_snapshot(&limits, false);
        assert!(default_snap.is_valid());
        assert!(default_snap.has_limits());
        assert_eq!(default_snap.elapsed_secs, 0);
        assert_eq!(default_snap.max_secs, 1800);
        assert_eq!(default_snap.max_tool_calls, 256);
        assert_eq!(default_snap.max_model_calls, 64);
        assert_eq!(default_snap.auto_extensions_used, None);
        assert_eq!(default_snap.max_auto_extensions, None);

        let default_smart = default_ui_snapshot(&limits, true);
        assert_eq!(default_smart.auto_extensions_used, Some(0));
        assert_eq!(
            default_smart.max_auto_extensions,
            Some(MAX_SMART_AUTO_EXTENSIONS)
        );

        let active = crate::agent_machine::task_budget::Snapshot {
            version: 1,
            task_id: "task-1".into(),
            parent_tasks: Vec::new(),
            descendants_active: false,
            limits,
            elapsed_millis: 1_080_500,
            tool_calls: 84,
            model_calls: 43,
            auto_extensions_used: 1,
            phase: Phase::Running,
            foreground_complete: false,
            recovered_time_upper_bound: false,
        };
        let ui = to_ui_snapshot(&active, false);
        assert_eq!(ui.elapsed_secs, 1080);
        assert_eq!(ui.tool_calls, 84);
        assert_eq!(ui.model_calls, 43);
        assert_eq!(ui.max_secs, 1800);
        assert_eq!(ui.max_tool_calls, 256);
        assert_eq!(ui.max_model_calls, 64);
        assert_eq!(ui.auto_extensions_used, None);
        assert_eq!(ui.max_auto_extensions, None);

        let ui_smart = to_ui_snapshot(&active, true);
        assert_eq!(ui_smart.auto_extensions_used, Some(1));
        assert_eq!(
            ui_smart.max_auto_extensions,
            Some(MAX_SMART_AUTO_EXTENSIONS)
        );
    }

    #[tokio::test]
    async fn publish_writes_ui_budget_document_to_agentfs() {
        let procfs = Arc::new(ProcFs::new());
        let agent_root = Arc::new(AgentRootFs::new_with_process_events(
            procfs.clone(),
            procfs.clone(),
        ));
        let mount_fs = Arc::new(MountFs::new(Namespace::new()));
        let live = mount_fs.live_namespace();
        live.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
        live.mount(
            "/agent",
            InProcessTransport::new(agent_root.clone()),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(mount_fs);
        let shell = alan_shell::Shell::new(root.clone());
        let pid = shell
            .spawn(
                r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#,
            )
            .await
            .unwrap();
        agent_root
            .bind_process(pid.clone(), Arc::new(AgentFs::new()))
            .await;
        let owner = format!("/agent/{pid}");
        let env = super::super::transition::NamespaceRuntimeEnvironment::new(
            root.clone(),
            owner.clone(),
            "/mnt/llm/default",
        );
        let files = env.agent_files();
        let limits = crate::TaskBudgetLimits {
            execution_seconds: NonZeroU64::new(900).unwrap(),
            tool_calls: NonZeroU64::new(120).unwrap(),
            model_calls: NonZeroU64::new(30).unwrap(),
        };
        let snapshot = crate::agent_machine::task_budget::Snapshot {
            version: 1,
            task_id: "task-publish".into(),
            parent_tasks: Vec::new(),
            descendants_active: false,
            limits: limits.clone(),
            tool_calls: 3,
            model_calls: 1,
            elapsed_millis: 45_000,
            auto_extensions_used: 0,
            phase: Phase::Paused,
            foreground_complete: false,
            recovered_time_upper_bound: false,
        };
        let budget = TaskBudget::restore(snapshot, None).unwrap();

        publish(&files, Some(&budget), &limits, true).await;

        let raw = shell
            .cat(&format!("{owner}/machine/ui/budget"))
            .await
            .unwrap();
        let parsed: UiBudgetSnapshot = serde_json::from_slice(&raw).unwrap();
        assert!(parsed.is_valid());
        assert_eq!(parsed.elapsed_secs, 45);
        assert_eq!(parsed.max_secs, 900);
        assert_eq!(parsed.tool_calls, 3);
        assert_eq!(parsed.max_tool_calls, 120);
        assert_eq!(parsed.model_calls, 1);
        assert_eq!(parsed.max_model_calls, 30);
        assert_eq!(parsed.auto_extensions_used, Some(0));
        assert_eq!(parsed.max_auto_extensions, Some(MAX_SMART_AUTO_EXTENSIONS));
    }
}
