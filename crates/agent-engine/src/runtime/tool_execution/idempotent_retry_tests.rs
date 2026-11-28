use super::*;
use crate::agent_machine::AgentMachine;

#[test]
fn idempotent_retry_skips_effect_lifecycle_only_for_write() {
    assert!(ToolEffectLifecycle::skips_idempotent_write(
        ToolCapability::Write,
        true
    ));
    assert!(!ToolEffectLifecycle::skips_idempotent_write(
        ToolCapability::Write,
        false
    ));
    assert!(!ToolEffectLifecycle::skips_idempotent_write(
        ToolCapability::Read,
        true
    ));
    assert!(!ToolEffectLifecycle::skips_idempotent_write(
        ToolCapability::Network,
        true
    ));
    // Unknown mutating bash commands keep duplicate-effect protection.
    assert!(!ToolEffectLifecycle::skips_idempotent_write(
        ToolCapability::Unknown,
        true
    ));
}

#[test]
fn non_idempotent_write_still_creates_an_effect_lifecycle() {
    let machine = AgentMachine::new();
    let arguments = json!({"command": "cargo fmt"});

    let lifecycle = ToolEffectLifecycle::for_call(
        &machine,
        "/proc/1",
        "call-1",
        "bash",
        &arguments,
        ToolCapability::Write,
    )
    .expect("non-idempotent write must keep lifecycle tracking");
    assert_eq!(lifecycle.effect_type(), "process");
    assert!(!ToolEffectLifecycle::skips_idempotent_write(
        ToolCapability::Write,
        false
    ));
}
