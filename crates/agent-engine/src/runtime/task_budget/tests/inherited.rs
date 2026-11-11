//! Lifting an inherited stop must follow the owning ancestor, never the child.
use super::*;
use crate::runtime::task_budget::inherited::lift_inherited;

#[tokio::test]
async fn inherited_wait_lifts_only_after_the_owning_ancestor_extends() {
    use crate::agent_machine::task_budget::{Admission, Extension, InheritedExhaustion};
    let directory = tempfile::TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/proc/child", "test", directory.path())
        .await
        .unwrap();
    let mut source = state(machine);
    let parent = TaskBudget::start(
        "ancestor".into(),
        source.core_config.task_budget.clone(),
        None,
    )
    .await
    .unwrap();
    parent.reserve(Admission::Model).await.unwrap();
    let child = TaskBudget::start_with_parent(
        "child".into(),
        source.core_config.task_budget.clone(),
        source.machine.recorder(),
        Some(parent.clone()),
    )
    .await
    .unwrap();
    let error = child.reserve(Admission::Model).await.unwrap_err();
    let typed = error
        .downcast_ref::<InheritedExhaustion>()
        .expect("ancestor exhaustion stays typed");
    let exhausted = InheritedExhaustion {
        ancestor_task_id: typed.ancestor_task_id.clone(),
        dimension: typed.dimension,
    };
    source.machine.task_budget = Some(child);
    preserve_inherited_wait(
        &mut source,
        &exhausted,
        Some(PendingToolReplayBatch {
            tool_calls: vec![],
            resume_with_generation: true,
        }),
    )
    .await
    .unwrap();

    // The ancestor has not extended yet, so the stop stands.
    assert!(!lift_inherited(&mut source).await.unwrap());
    assert!(source.machine.inherited_budget_wait.is_some());

    parent
        .extend(&Extension {
            model_calls: 2,
            ..Extension::default()
        })
        .await
        .unwrap();
    parent.resume().await.unwrap();
    assert!(lift_inherited(&mut source).await.unwrap());
    assert!(source.machine.inherited_budget_wait.is_none());
}
