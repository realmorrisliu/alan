use super::read_runtime_source;

#[test]
fn turn_memory_finalization_uses_only_explicit_runtime_inputs() {
    let runtime_inputs = read_runtime_source("turn_memory/runtime_inputs.rs");
    for forbidden in [
        "RuntimeLoopState",
        "RuntimeConfig",
        "NamespaceRuntimeEnvironment",
    ] {
        assert!(
            !runtime_inputs.contains(forbidden),
            "turn memory runtime must not regain {forbidden}"
        );
    }
    assert!(runtime_inputs.contains("TurnMemoryRuntime"));

    let transition = read_runtime_source("transition.rs");
    assert!(transition.contains("fn turn_memory_runtime("));
    let owner = read_runtime_source("turn_memory.rs");
    assert!(owner.contains("finalize_turn_memory_best_effort"));

    let executor = read_runtime_source("transition/turn_execution.rs");
    for (name, source) in [("turn executor", executor), ("transition", transition)] {
        for displaced_operation in [
            "build_turn_memory_promotion_job(",
            "refresh_turn_memory_surfaces_best_effort(",
        ] {
            assert!(
                !source.contains(displaced_operation),
                "{name} must leave {displaced_operation} to the memory owner"
            );
        }
    }
}
