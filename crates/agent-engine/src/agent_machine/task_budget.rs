//! Machine-owned admission accounting shared by one task's execution adapters.

use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::rollout::{EventRecord, RolloutItem, RolloutRecorder};

pub(crate) const EVENT_TYPE: &str = "agent_task_budget_v1";

pub(crate) mod waiting;

/// Finite limits for one logical task, including retries and reviewer requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TaskBudgetLimits {
    /// Active execution time; waiting for a user does not consume this allowance.
    pub execution_seconds: NonZeroU64,
    /// Maximum admitted Tool calls, including rejected attempts.
    pub tool_calls: NonZeroU64,
    /// Maximum admitted model requests, including retries and review.
    pub model_calls: NonZeroU64,
}

impl Default for TaskBudgetLimits {
    fn default() -> Self {
        Self {
            execution_seconds: NonZeroU64::new(1800).expect("positive default"),
            tool_calls: NonZeroU64::new(256).expect("positive default"),
            model_calls: NonZeroU64::new(64).expect("positive default"),
        }
    }
}

impl TaskBudgetLimits {
    fn milliseconds(&self) -> Result<u64> {
        self.execution_seconds
            .get()
            .checked_mul(1000)
            .context("task execution budget is too large")
    }
}

/// Additional allowance explicitly approved without changing spent counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Extension {
    pub execution_seconds: u64,
    pub tool_calls: u64,
    pub model_calls: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Running,
    Paused,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub version: u16,
    pub task_id: String,
    pub limits: TaskBudgetLimits,
    pub tool_calls: u64,
    pub model_calls: u64,
    pub elapsed_millis: u64,
    pub phase: Phase,
    /// Foreground work settled; captured best-effort jobs retain this same ledger.
    #[serde(default)]
    pub foreground_complete: bool,
    /// A crash left an active interval without a settled elapsed-time record.
    pub recovered_time_upper_bound: bool,
}

impl Snapshot {
    fn validate(&self) -> Result<()> {
        if self.version != 1 || self.task_id.trim().is_empty() {
            bail!("invalid task budget identity or version");
        }
        self.limits.milliseconds()?;
        if self.tool_calls > self.limits.tool_calls.get()
            || self.model_calls > self.limits.model_calls.get()
        {
            bail!("task budget consumption exceeds its recorded allowance");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Admission {
    Tool,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dimension {
    Time,
    ToolCalls,
    ModelCalls,
}

impl std::fmt::Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Time => "execution time",
            Self::ToolCalls => "Tool calls",
            Self::ModelCalls => "model calls",
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Task budget exhausted: {dimension}; explicit additional allowance is required")]
pub(crate) struct Exhausted {
    pub dimension: Dimension,
}

#[derive(Debug)]
struct State {
    snapshot: Snapshot,
    running_since: Option<Instant>,
    poisoned: bool,
}

impl State {
    fn observed(&self) -> Snapshot {
        let mut next = self.snapshot.clone();
        if let Some(started) = self.running_since {
            next.elapsed_millis = next
                .elapsed_millis
                .saturating_add(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
        }
        next
    }
}

/// One task's state; clones share accounting, not a new execution identity.
#[derive(Clone, Debug)]
pub(crate) struct TaskBudget {
    state: Arc<Mutex<State>>,
    recorder: Option<RolloutRecorder>,
}

impl TaskBudget {
    pub(crate) async fn start(
        task_id: String,
        limits: TaskBudgetLimits,
        recorder: Option<RolloutRecorder>,
    ) -> Result<Self> {
        let snapshot = Snapshot {
            version: 1,
            task_id,
            limits,
            tool_calls: 0,
            model_calls: 0,
            elapsed_millis: 0,
            phase: Phase::Paused,
            foreground_complete: false,
            recovered_time_upper_bound: false,
        };
        snapshot.validate()?;
        let budget = Self::restore(snapshot, recorder)?;
        budget.resume().await?;
        Ok(budget)
    }

    pub(crate) fn restore(
        mut snapshot: Snapshot,
        recorder: Option<RolloutRecorder>,
    ) -> Result<Self> {
        snapshot.validate()?;
        if snapshot.phase == Phase::Running {
            // A wall clock cannot reconstruct a lost monotonic interval. Charge
            // the remaining time rather than silently reset it after a crash.
            snapshot.elapsed_millis = snapshot.elapsed_millis.max(snapshot.limits.milliseconds()?);
            snapshot.recovered_time_upper_bound = true;
            snapshot.phase = Phase::Paused;
        }
        Ok(Self {
            state: Arc::new(Mutex::new(State {
                snapshot,
                running_since: None,
                poisoned: false,
            })),
            recorder,
        })
    }

    pub(crate) async fn snapshot(&self) -> Snapshot {
        self.state.lock().await.observed()
    }

    async fn persist(&self, state: &mut State, next: Snapshot) -> Result<()> {
        if state.poisoned {
            bail!("task budget evidence is uncertain; execution remains paused");
        }
        let checkpoint_started = Instant::now();
        // Dropping this future while awaiting the acknowledgement must also
        // fail closed: the writer can still commit the admission afterward.
        state.poisoned = true;
        if let Some(recorder) = &self.recorder {
            let item = RolloutItem::Event(EventRecord {
                event_type: EVENT_TYPE.into(),
                payload: serde_json::to_value(&next)?,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
            if let Err(error) = recorder.persist_batch(vec![item]).await {
                // The writer may have accepted bytes before returning an error.
                // Neither refund nor another attempt can prove a free allowance.
                state.poisoned = true;
                return Err(error.context("persist task budget admission"));
            }
        }
        state.snapshot = next;
        state.poisoned = false;
        state.running_since =
            (state.snapshot.phase == Phase::Running).then_some(checkpoint_started);
        Ok(())
    }

    pub(crate) async fn reserve(&self, admission: Admission) -> Result<()> {
        let mut state = self.state.lock().await;
        if state.poisoned {
            bail!("task budget evidence is uncertain; execution remains paused");
        }
        let mut next = state.observed();
        if next.phase != Phase::Running {
            bail!("task budget is not running");
        }
        if next.elapsed_millis >= next.limits.milliseconds()? {
            return Err(Exhausted {
                dimension: Dimension::Time,
            }
            .into());
        }
        if next.tool_calls >= next.limits.tool_calls.get() {
            return Err(Exhausted {
                dimension: Dimension::ToolCalls,
            }
            .into());
        }
        if next.model_calls >= next.limits.model_calls.get() {
            return Err(Exhausted {
                dimension: Dimension::ModelCalls,
            }
            .into());
        }
        match admission {
            Admission::Tool => next.tool_calls += 1,
            Admission::Model => next.model_calls += 1,
        }
        self.persist(&mut state, next).await
    }

    pub(crate) async fn remaining_time(&self) -> Result<Duration> {
        let state = self.state.lock().await;
        if state.poisoned {
            bail!("task budget evidence is uncertain; execution remains paused");
        }
        let next = state.observed();
        if next.phase != Phase::Running {
            bail!("task budget is not running");
        }
        let remaining = next
            .limits
            .milliseconds()?
            .saturating_sub(next.elapsed_millis);
        if remaining == 0 {
            return Err(Exhausted {
                dimension: Dimension::Time,
            }
            .into());
        }
        Ok(Duration::from_millis(remaining))
    }

    pub(crate) async fn resume(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        let mut next = state.observed();
        if next.phase == Phase::Completed {
            bail!("completed task budget cannot be resumed");
        }
        next.phase = Phase::Running;
        self.persist(&mut state, next).await
    }

    pub(crate) async fn settle(&self, completed: bool) -> Result<()> {
        let mut state = self.state.lock().await;
        anyhow::ensure!(
            !state.poisoned,
            "task budget evidence is uncertain; execution remains paused"
        );
        let mut next = state.observed();
        if !completed && next.phase == Phase::Paused {
            return Ok(());
        }
        if next.phase == Phase::Completed {
            anyhow::ensure!(completed, "completed task budget cannot be paused");
            return Ok(());
        }
        next.phase = if completed {
            Phase::Completed
        } else {
            Phase::Paused
        };
        self.persist(&mut state, next).await
    }

    pub(crate) async fn finish_foreground(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        anyhow::ensure!(
            !state.poisoned,
            "task budget evidence is uncertain; execution remains paused"
        );
        let mut next = state.observed();
        if next.phase == Phase::Completed {
            return Ok(());
        }
        if next.foreground_complete && next.phase == Phase::Paused {
            return Ok(());
        }
        next.foreground_complete = true;
        next.phase = Phase::Paused;
        self.persist(&mut state, next).await
    }

    /// Add an explicitly approved tranche; existing consumption never changes.
    pub(crate) async fn extend(&self, additional: &Extension) -> Result<()> {
        anyhow::ensure!(
            additional.execution_seconds > 0
                || additional.tool_calls > 0
                || additional.model_calls > 0,
            "task budget extension must add a positive allowance"
        );
        let mut state = self.state.lock().await;
        let mut next = state.observed();
        if next.phase == Phase::Completed {
            bail!("completed task budget cannot be extended");
        }
        let add = |old: NonZeroU64, more: u64| {
            old.get()
                .checked_add(more)
                .and_then(NonZeroU64::new)
                .context("task budget extension overflow")
        };
        next.limits.execution_seconds =
            add(next.limits.execution_seconds, additional.execution_seconds)?;
        next.limits.tool_calls = add(next.limits.tool_calls, additional.tool_calls)?;
        next.limits.model_calls = add(next.limits.model_calls, additional.model_calls)?;
        next.limits.milliseconds()?;
        next.phase = Phase::Paused;
        self.persist(&mut state, next).await
    }

    /// Run an admitted operation until the shared time allowance expires. Await
    /// cancellation cleanup before reporting exhaustion; never detach an effect.
    pub(crate) async fn run<T, F, Fut>(&self, parent: &CancellationToken, operation: F) -> Result<T>
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        anyhow::ensure!(
            !parent.is_cancelled(),
            "task operation cancelled before admission"
        );
        let remaining = self.remaining_time().await?;
        let cancel = parent.child_token();
        let future = operation(cancel.clone());
        tokio::pin!(future);
        tokio::select! {
            result = &mut future => result,
            _ = tokio::time::sleep(remaining) => {
                cancel.cancel();
                let exhausted = Exhausted { dimension: Dimension::Time };
                match future.await {
                    // Keep uncertain cleanup evidence in the error chain. A
                    // deadline must never turn an unconfirmed abort into a
                    // safely settled operation that can be resumed implicitly.
                    Err(error) => Err(error.context(exhausted)),
                    Ok(_) => Err(exhausted.into()),
                }
            }
        }
    }
}

pub(crate) fn recover(events: &[EventRecord]) -> Result<Option<Snapshot>> {
    let mut tasks = std::collections::BTreeMap::<String, Snapshot>::new();
    let mut foreground = None;
    for event in events.iter().filter(|event| event.event_type == EVENT_TYPE) {
        let next: Snapshot =
            serde_json::from_value(event.payload.clone()).context("invalid durable task budget")?;
        next.validate()?;
        if let Some(prior) = tasks.get(&next.task_id) {
            anyhow::ensure!(
                prior.phase != Phase::Completed || next == *prior,
                "completed task budget changed after settlement"
            );
            if next.tool_calls < prior.tool_calls
                || (prior.foreground_complete && !next.foreground_complete)
                || next.model_calls < prior.model_calls
                || next.elapsed_millis < prior.elapsed_millis
                || next.limits.execution_seconds < prior.limits.execution_seconds
                || next.limits.tool_calls < prior.limits.tool_calls
                || next.limits.model_calls < prior.limits.model_calls
            {
                bail!("durable task budget consumption or allowance went backwards");
            }
        } else {
            if let Some(prior) = foreground.as_ref().and_then(|id| tasks.get(id)) {
                anyhow::ensure!(
                    prior.phase == Phase::Completed || prior.foreground_complete,
                    "durable task budget changed identity before foreground completion"
                );
            }
            foreground = Some(next.task_id.clone());
        }
        tasks.insert(next.task_id.clone(), next);
    }
    // Deferred work may settle an older task after newer foreground work has
    // started. That update must never select the old task for recovery again.
    Ok(foreground
        .and_then(|id| tasks.remove(&id))
        .filter(|snapshot| snapshot.phase != Phase::Completed))
}

#[cfg(test)]
mod tests;
