//! A refused budget pause must not leave the ledger paused with no decision.

use super::*;

#[tokio::test]
async fn a_rejected_wait_leaves_the_ledger_running() {
    use crate::agent_machine::task_budget::{Exhausted, Phase};
    let directory = tempfile::TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/proc/wait", "test", directory.path())
        .await
        .unwrap();
    let mut source = state(machine);
    let budget = TaskBudget::start("task".into(), source.core_config.task_budget.clone(), None)
        .await
        .unwrap();
    source.machine.task_budget = Some(budget);

    let mut emit = |_event: Event| async {};
    // The task has spent nothing, so the proposed tranche is empty and the wait
    // cannot describe it. The pause must be rolled back instead of leaving the
    // ledger paused with nothing for the operator to answer.
    let outcome = crate::runtime::task_budget::pause(
        &mut source,
        &Exhausted {
            dimension: crate::agent_machine::task_budget::Dimension::ModelCalls,
        },
        None,
        &mut emit,
    )
    .await;

    assert!(
        outcome.is_err(),
        "a wait that cannot describe itself is refused"
    );
    assert_eq!(
        source
            .machine
            .task_budget
            .as_ref()
            .unwrap()
            .snapshot()
            .await
            .phase,
        Phase::Running,
        "the refused pause is rolled back"
    );
    assert!(source.machine.task_budget_wait.is_none());
}
