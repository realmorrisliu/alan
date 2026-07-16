use super::*;

#[test]
fn lifecycle_exists_only_for_effectful_tool_calls() {
    let machine = AgentMachine::new();
    let arguments = json!({});

    assert!(
        ToolEffectLifecycle::for_call(
            &machine,
            "/proc/1",
            "call-read",
            "read_file",
            &arguments,
            ToolCapability::Read,
        )
        .is_none()
    );
    assert_eq!(
        ToolEffectLifecycle::for_call(
            &machine,
            "/proc/1",
            "call-file",
            "write_file",
            &arguments,
            ToolCapability::Write,
        )
        .unwrap()
        .effect_type(),
        "file"
    );
    assert_eq!(
        ToolEffectLifecycle::for_call(
            &machine,
            "/proc/1",
            "call-network",
            "bash",
            &arguments,
            ToolCapability::Network,
        )
        .unwrap()
        .effect_type(),
        "network"
    );
    assert_eq!(
        ToolEffectLifecycle::for_call(
            &machine,
            "/proc/1",
            "call-process",
            "bash",
            &arguments,
            ToolCapability::Unknown,
        )
        .unwrap()
        .effect_type(),
        "process"
    );
}

#[test]
fn identity_canonicalizes_arguments() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("write once");
    let left = json!({"path": "notes.txt", "nested": {"b": 2, "a": 1}});
    let right = json!({"nested": {"a": 1, "b": 2}, "path": "notes.txt"});

    let left = build_effect_identity(&machine, "write_file", &left, EffectCategory::File);
    let right = build_effect_identity(&machine, "write_file", &right, EffectCategory::File);

    assert_eq!(left.request_fingerprint, right.request_fingerprint);
    assert_eq!(left.idempotency_key, right.idempotency_key);
}

#[test]
fn identity_turn_component_remains_monotonic_across_rollback() {
    let mut machine = AgentMachine::new();
    let arguments = json!({"path":"notes.txt","payload":"hello"});

    machine.add_user_message("turn-1");
    let first = build_effect_identity(&machine, "write_file", &arguments, EffectCategory::File);
    machine.add_user_message("turn-2");
    let second = build_effect_identity(&machine, "write_file", &arguments, EffectCategory::File);

    let removed = machine.rollback_last_turns(1);
    assert!(removed.removed_messages > 0);
    machine.add_user_message("turn-3");
    let third = build_effect_identity(&machine, "write_file", &arguments, EffectCategory::File);

    assert_ne!(first.idempotency_key, second.idempotency_key);
    assert_ne!(second.idempotency_key, third.idempotency_key);
}

#[test]
fn identity_ignores_confirmation_control_messages() {
    let mut machine = AgentMachine::new();
    let arguments = json!({"path":"notes.txt","payload":"hello"});
    machine.add_user_message("write once");
    let first = build_effect_identity(&machine, "write_file", &arguments, EffectCategory::File);

    machine.add_user_control_message_parts(vec![crate::tape::ContentPart::structured(json!({
        "checkpoint_type":"effect_replay_confirmation",
        "choice":"approve"
    }))]);
    let replayed = build_effect_identity(&machine, "write_file", &arguments, EffectCategory::File);

    assert_eq!(first.idempotency_key, replayed.idempotency_key);
}

#[tokio::test]
async fn lifecycle_commits_unknown_applied_and_replayed_records() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("write once");
    let arguments = json!({"path":"notes.txt","payload":"hello"});
    let lifecycle = ToolEffectLifecycle::for_call(
        &machine,
        "/proc/1",
        "call-1",
        "write_file",
        &arguments,
        ToolCapability::Write,
    )
    .unwrap();
    assert!(matches!(
        lifecycle.plan(&machine, false),
        ToolEffectPlan::Execute
    ));

    let started = lifecycle.begin(&mut machine).await.unwrap();
    assert_eq!(
        machine
            .effect_by_idempotency_key(lifecycle.idempotency_key())
            .unwrap()
            .status,
        EffectStatus::Unknown
    );

    let payload = json!({"success": true, "value": "done"});
    lifecycle.complete(&mut machine, &started, &payload, true, None);
    let applied = machine
        .effect_by_idempotency_key(lifecycle.idempotency_key())
        .unwrap();
    assert_eq!(applied.status, EffectStatus::Applied);
    assert!(applied.result_digest.is_some());
    assert!(applied.applied_at.is_some());

    let replay = ToolEffectLifecycle::for_call(
        &machine,
        "/proc/2",
        "call-2",
        "write_file",
        &arguments,
        ToolCapability::Write,
    )
    .unwrap();
    let ToolEffectPlan::ReplayApplied { payload } = replay.plan(&machine, false) else {
        panic!("applied effect should produce replay plan");
    };
    replay.commit_replay(&mut machine, &payload, "dedupe");

    let replayed = machine
        .effect_by_idempotency_key(replay.idempotency_key())
        .unwrap();
    assert_eq!(replayed.status, EffectStatus::Applied);
    assert_eq!(replayed.process_path, "/proc/2");
    assert!(replayed.dedupe_hit);
}

#[tokio::test]
async fn lifecycle_commits_failed_terminal_status() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("write once");
    let arguments = json!({"path":"notes.txt"});
    let lifecycle = ToolEffectLifecycle::for_call(
        &machine,
        "/proc/1",
        "call-1",
        "write_file",
        &arguments,
        ToolCapability::Write,
    )
    .unwrap();

    let started = lifecycle.begin(&mut machine).await.unwrap();
    lifecycle.complete(
        &mut machine,
        &started,
        &json!({"success": false}),
        false,
        Some("tool reported failure in payload".to_string()),
    );

    let failed = machine
        .effect_by_idempotency_key(lifecycle.idempotency_key())
        .unwrap();
    assert_eq!(failed.status, EffectStatus::Failed);
    assert!(failed.applied_at.is_none());
    assert_eq!(
        failed.reason.as_deref(),
        Some("tool reported failure in payload")
    );
}
