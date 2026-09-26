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

#[test]
fn agent_machine_state_is_not_a_public_or_runtime_field_surface() {
    let public_api = super::read_crate_source("lib.rs");
    assert!(
        !public_api.contains("AgentMachine,"),
        "AgentMachine must not be a cross-crate integration surface"
    );

    let machine = super::read_crate_source("agent_machine.rs");
    let state = super::rust_item_body(&machine, "pub(crate) struct AgentMachine");
    for private_field in ["tape", "recorder", "has_active_task", "transition_state"] {
        assert!(state.contains(&format!("{private_field}:")));
        assert!(
            !state.contains(&format!("pub {private_field}:")),
            "AgentMachine field {private_field} must remain private"
        );
    }

    let transition_state = super::read_crate_source("agent_machine/transition_state.rs");
    let transition_fields = super::rust_item_body(
        &transition_state,
        "pub(super) struct MachineTransitionState",
    );
    assert!(transition_fields.contains("input_broker:"));
    assert!(
        !super::read_crate_source("agent_machine/input_queue.rs")
            .contains("pub current_submission_id:")
    );
    assert!(
        !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/runtime/turn_state.rs")
            .exists(),
        "turn-local state must not retain a second runtime owner"
    );

    let transition_source = super::read_runtime_source("transition.rs");
    let runtime_state =
        super::rust_item_body(&transition_source, "pub(super) struct RuntimeLoopState");
    for displaced_field in ["current_submission_id", "turn_state"] {
        assert!(
            !runtime_state.contains(&format!("{displaced_field}:")),
            "RuntimeLoopState must not retain Machine field {displaced_field}"
        );
    }

    for (path, source) in super::read_rust_sources_under("runtime") {
        for displaced_surface in ["TurnState", ".turn_state"] {
            assert!(
                !source.contains(displaced_surface),
                "{} retains displaced Machine state surface {displaced_surface}",
                path.display()
            );
        }
        for forbidden in [
            "machine.tape.",
            "machine.recorder.",
            "machine.has_active_task =",
            "state.machine.tape.",
            "state.machine.recorder.",
            "state.machine.has_active_task =",
        ] {
            assert!(
                !source.contains(forbidden),
                "{} reaches through AgentMachine via {forbidden}",
                path.display()
            );
        }
    }
}
