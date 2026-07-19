#[test]
fn submission_handling_uses_only_explicit_runtime_inputs() {
    let transition = super::read_runtime_source("transition.rs");
    let handlers = super::read_runtime_source("submission_handlers.rs");
    let runtime = super::read_runtime_source("submission_handlers/runtime_inputs.rs");

    for forbidden in [
        "RuntimeLoopState",
        "RuntimeConfig",
        "NamespaceRuntimeEnvironment",
    ] {
        assert!(
            !runtime.contains(forbidden),
            "submission runtime must not regain {forbidden}"
        );
    }
    assert!(runtime.contains("SubmissionRuntime"));
    assert!(transition.contains("fn submission_runtime("));
    assert!(transition.contains("async fn handle_runtime_op"));
    assert!(transition.contains("Op::CompactWithOptions"));
    assert!(handlers.contains("handle_non_compaction_runtime_op"));
    assert!(handlers.contains("runtime.mount_control"));
    assert!(!handlers.contains("state.mount_control()"));
    assert!(!handlers.contains("state.environment"));
}
