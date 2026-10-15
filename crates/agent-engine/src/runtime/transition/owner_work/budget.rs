//! Resume the retained source-owner program through its existing task budget.

use super::*;

pub(super) async fn run<E, F>(
    state: &mut RuntimeLoopState,
    mut work: Snapshot,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let result = advance(state, &mut work, emit, cancel).await;
    if cancel.is_cancelled() {
        state.machine.mark_submission_cancelled();
    }
    if let Err(error) = &result
        && (error.is::<NamespaceEvaluationUncertainty>()
            || error.is::<super::super::NamespaceOperationUncertainty>()
            || error.is::<FallbackUncertainty>())
    {
        return result;
    }
    if cancel.is_cancelled() {
        work.outcome = Outcome::Cancelled;
    } else if let Err(error) = result {
        if error.is::<crate::agent_machine::task_budget::Exhausted>() {
            work.outcome = Outcome::BudgetPaused;
            work.validate()?;
            // The budget checkpoint acknowledges this exact program and cursor together.
            state.machine.owner_work = Some(work);
            return Err(error);
        }
        work.outcome = Outcome::Failed {
            reason: error.to_string(),
        };
        settle(state, work, emit).await?;
        return Err(error);
    }
    settle(state, work, emit).await
}

pub(in super::super) async fn resume<E, F>(
    state: &mut RuntimeLoopState,
    work_id: &str,
    approved: bool,
    emit: &mut E,
    cancel: &CancellationToken,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let mut work = state
        .machine
        .owner_work
        .clone()
        .context("budget continuation lacks owner work")?;
    ensure!(
        work.work_id == work_id && work.outcome == Outcome::BudgetPaused,
        "budget continuation changed owner work"
    );
    state.machine.accept_submission(work.work_id.clone());
    if !approved {
        state.machine.mark_submission_cancelled();
        work.outcome = Outcome::Cancelled;
        return settle(state, work, emit).await;
    }
    work.outcome = Outcome::Started;
    work.budget_resumes = work
        .budget_resumes
        .checked_add(1)
        .context("owner program budget resume revision overflow")?;
    if work.selected_owner.is_some() {
        state.machine.retire_owner_read(
            &mut work,
            "revalidate current source authority after budget pause",
        );
        work.validated_sources = 0;
    }
    state.machine.persist_owner_work(work.clone()).await?;
    state.machine.set_turn_activity(TurnActivityState::Running);
    run(state, work, emit, cancel).await
}
