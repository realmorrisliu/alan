#[test]
fn turn_execution_belongs_to_the_transition_owner() {
    let runtime_module = super::read_runtime_source("mod.rs");
    let transition = super::read_runtime_source("transition.rs");
    let turn_execution = super::read_runtime_source("transition/turn_execution.rs");

    assert!(!runtime_module.contains("mod turn_executor;"));
    assert!(transition.contains("mod turn_execution;"));
    assert!(transition.contains("pub(super) enum TurnRunKind"));
    assert!(transition.contains("pub(super) enum TurnExecutionOutcome"));
    assert!(turn_execution.contains("state: &mut RuntimeLoopState"));
    assert!(!turn_execution.contains("super::transition"));
}
