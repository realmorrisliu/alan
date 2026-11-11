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

#[tokio::test]
async fn a_relaunched_child_republishes_the_same_inherited_request_identity() {
    use crate::agent_machine::task_budget::{Admission, InheritedExhaustion};
    use alan_agent_protocol::Event;
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
        Some(parent),
    )
    .await
    .unwrap();
    let error = child.reserve(Admission::Model).await.unwrap_err();
    let typed = error.downcast_ref::<InheritedExhaustion>().unwrap();
    let exhausted = InheritedExhaustion {
        ancestor_task_id: typed.ancestor_task_id.clone(),
        dimension: typed.dimension,
    };
    source.machine.task_budget = Some(child);
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
    let sink = std::sync::Arc::clone(&events);
    let mut emit = move |event: Event| {
        sink.lock().unwrap().push(event);
        async {}
    };
    pause_inherited(&mut source, &exhausted, None, &mut emit)
        .await
        .unwrap();
    drop(emit);
    let published_id = events
        .lock()
        .unwrap()
        .iter()
        .find_map(|event| match event {
            Event::Yield { request_id, .. } => Some(request_id.clone()),
            _ => None,
        })
        .expect("the pause publishes a yield");
    let first = source
        .machine
        .inherited_budget_wait
        .as_ref()
        .expect("the pause records the wait")
        .checkpoint
        .clone();
    source.machine.flush_recorder().await.unwrap();

    // A new invocation must expose the same request, or the ancestor's resume
    // would target something that does not exist.
    let recovered = AgentMachine::load_from_rollout_in_dir(
        source.machine.rollout_path().unwrap(),
        "/proc/relaunched",
        "test",
        directory.path(),
    )
    .await
    .unwrap();
    let mut relaunched = state(recovered);
    assert!(relaunched.machine.inherited_budget_wait.is_some());
    let republished = publish_inherited_wait(&mut relaunched)
        .await
        .unwrap()
        .expect("the inherited wait is republished");
    assert_eq!(
        republished, published_id,
        "the request identity must survive a relaunch"
    );
    assert_eq!(
        relaunched.machine.pending_confirmation(),
        Some(first),
        "the republished decision must be the recorded one"
    );
}

#[tokio::test]
async fn child_never_extends_an_inherited_wait_and_settles_it_instead() {
    use alan_agent_protocol::Event;
    let (_directory, mut source, exhausted) = inherited_exhausted_child().await;
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };
    pause_inherited(&mut source, &exhausted, None, &mut emit)
        .await
        .unwrap();
    drop(emit);

    let pending = source
        .machine
        .pending_confirmation()
        .expect("the inherited pause is a pending confirmation");
    assert_eq!(
        pending.checkpoint_type,
        crate::approval::INHERITED_BUDGET_CHECKPOINT_TYPE
    );

    let kept = source.machine.inherited_budget_wait.clone();
    // The child never extends: an approval only lets its own preserved work
    // continue, and the ancestor's ledger is untouched.
    let before = source
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .snapshot()
        .await;
    resolve(&mut source.machine, &pending, true).await.unwrap();
    assert!(source.machine.inherited_budget_wait.is_none());
    let after = source
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .snapshot()
        .await;
    assert_eq!(
        (
            after.model_calls,
            after.tool_calls,
            after.limits.model_calls
        ),
        (
            before.model_calls,
            before.tool_calls,
            before.limits.model_calls
        ),
        "resuming must not add allowance or refund calls"
    );

    // A rejection settles the child subtree instead.
    source.machine.inherited_budget_wait = kept;
    resolve_inherited(&mut source.machine, &pending, false)
        .await
        .unwrap();
    assert!(source.machine.inherited_budget_wait.is_none());
    let after = source
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .snapshot()
        .await;
    assert_eq!(
        (after.model_calls, after.tool_calls),
        (before.model_calls, before.tool_calls),
        "settling must not refund or add allowance"
    );
    // The inherited block is gone, so ordinary continuation is no longer refused.
    assert!(begin(&mut source).await.is_ok());
}
