//! Process-bound access to an existing Machine ledger, never a new task manager.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, ensure};

use super::{Admission, AtomicUsize, Ordering, Phase, TaskBudget};

pub(super) struct DescendantLease {
    active: Vec<Arc<AtomicUsize>>,
    changed: Arc<tokio::sync::Notify>,
}

impl Drop for DescendantLease {
    fn drop(&mut self) {
        for active in &self.active {
            active.fetch_sub(1, Ordering::AcqRel);
        }
        self.changed.notify_waiters();
    }
}

/// Trusted runtime binding carried by Agent Runtime Service across Process launch.
/// Agent arguments and configuration cannot select or replace the parent ledger.
#[derive(Clone, Debug, Default)]
pub struct TaskBudgetBinding {
    current: Arc<Mutex<Option<TaskBudget>>>,
    parent: Option<TaskBudget>,
}

impl TaskBudgetBinding {
    /// Capture the current parent task; subsequent parent tasks cannot replace it.
    pub fn child(&self) -> Result<Self> {
        let parent = self
            .current
            .lock()
            .expect("task budget binding poisoned")
            .clone()
            .context("parent Agent Process has no admitted task budget")?;
        Ok(Self {
            current: Default::default(),
            parent: Some(parent),
        })
    }

    pub(crate) fn parent(&self) -> Option<TaskBudget> {
        self.parent.clone()
    }

    pub(crate) async fn bind(&self, budget: &mut TaskBudget) -> Result<()> {
        budget.attach_ancestors(parent_chain(self.parent())).await?;
        *self.current.lock().expect("task budget binding poisoned") = Some(budget.clone());
        Ok(())
    }
}

pub(super) fn parent_chain(parent: Option<TaskBudget>) -> Vec<TaskBudget> {
    let Some(mut parent) = parent else {
        return Vec::new();
    };
    let mut ancestors = std::mem::take(&mut parent.ancestors);
    ancestors.push(parent);
    ancestors
}

pub(super) async fn task_ids(ancestors: &[TaskBudget]) -> Vec<String> {
    let mut ids = Vec::with_capacity(ancestors.len());
    for ancestor in ancestors {
        ids.push(ancestor.snapshot().await.task_id);
    }
    ids
}

fn inherited_error(error: anyhow::Error) -> anyhow::Error {
    // Do not offer a child-local extension for an exhausted ancestor ledger.
    anyhow!("Parent task budget prevents continuation: {error}")
}

impl TaskBudget {
    pub(super) async fn acquire_ancestors(&self) -> Result<DescendantLease> {
        self.validate_ancestry().await?;
        let mut lease = DescendantLease {
            active: Vec::new(),
            changed: self.changed.clone(),
        };
        for ancestor in &self.ancestors {
            let state = ancestor.state.lock().await;
            ensure!(
                !state.poisoned
                    && state.snapshot.phase == Phase::Running
                    && !state.snapshot.foreground_complete,
                "Parent task budget stopped before child execution"
            );
            state
                .active_descendants
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    count.checked_add(1)
                })
                .map_err(|_| anyhow!("too many active descendants"))?;
            lease.active.push(state.active_descendants.clone());
        }
        Ok(lease)
    }

    pub(super) async fn await_descendants(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self
                .state
                .lock()
                .await
                .active_descendants
                .load(Ordering::Acquire)
                == 0
            {
                return;
            }
            changed.await;
        }
    }

    pub(super) async fn attach_ancestors(&mut self, ancestors: Vec<TaskBudget>) -> Result<()> {
        ensure!(
            self.snapshot().await.parent_tasks == task_ids(&ancestors).await,
            "task budget recovery requires its original ancestor bindings"
        );
        if let Some(parent) = ancestors.last() {
            self.changed = parent.changed.clone();
        }
        self.ancestors = ancestors;
        Ok(())
    }

    pub(super) async fn validate_ancestry(&self) -> Result<()> {
        ensure!(
            self.snapshot().await.parent_tasks == task_ids(&self.ancestors).await,
            "task budget ancestor binding is unavailable"
        );
        Ok(())
    }

    pub(crate) async fn reserve(&self, admission: Admission) -> Result<()> {
        self.validate_ancestry().await?;
        // Check local time/phase before charging ancestors. Recheck on admission:
        // failed or racing reservations remain charged rather than being refunded.
        self.remaining_time_local(false).await?;
        for ancestor in &self.ancestors {
            ancestor
                .reserve_local(admission, true)
                .await
                .map_err(inherited_error)?;
        }
        self.reserve_local(admission, false).await
    }

    pub(crate) async fn remaining_time(&self) -> Result<std::time::Duration> {
        self.validate_ancestry().await?;
        let mut remaining = self.remaining_time_local(false).await?;
        for ancestor in &self.ancestors {
            remaining = remaining.min(
                ancestor
                    .remaining_time_local(true)
                    .await
                    .map_err(inherited_error)?,
            );
        }
        Ok(remaining)
    }

    pub(super) async fn ancestor_time_error(&self) -> Option<anyhow::Error> {
        for ancestor in &self.ancestors {
            if let Err(error) = ancestor.remaining_time_local(true).await {
                return Some(inherited_error(error));
            }
        }
        None
    }

    pub(super) async fn ancestor_stopped(&self) -> anyhow::Error {
        if self.ancestors.is_empty() {
            return std::future::pending().await;
        }
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            for ancestor in &self.ancestors {
                let state = ancestor.state.lock().await;
                if state.poisoned
                    || state.snapshot.phase != Phase::Running
                    || state.snapshot.foreground_complete
                {
                    return anyhow!("Parent task budget stopped; child work must settle");
                }
            }
            changed.await;
        }
    }
}

#[cfg(test)]
mod tests;
