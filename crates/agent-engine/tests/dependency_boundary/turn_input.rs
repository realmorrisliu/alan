#[test]
fn turn_input_selection_does_not_own_submission_transitions() {
    let input = super::read_runtime_source("turn_input.rs");
    let transition = super::read_runtime_source("transition/accepted_submission.rs");
    let runtime_module = super::read_runtime_source("mod.rs");

    for forbidden in [
        "RuntimeLoopState",
        "handle_submission_with_cancel",
        "drive_turn_submission_with_cancel",
    ] {
        assert!(
            !input.contains(forbidden),
            "turn input selection must not regain transition authority through {forbidden}"
        );
    }

    assert!(input.contains("next_pending_interaction_submission"));
    assert!(input.contains("machine: &mut AgentMachine"));
    assert!(input.contains("agent_files: &NamespaceAgentFiles"));
    assert!(transition.contains("async fn drive_turn_submission_with_cancel"));
    assert!(transition.contains("handle_submission_with_cancel_and_steering"));
    assert!(runtime_module.contains("mod turn_input;"));
    assert!(!runtime_module.contains("mod turn_driver;"));
}
