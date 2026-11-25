use super::*;
use tempfile::TempDir;

#[test]
fn summary_scrubs_secrets_and_enforces_revision_and_user_precedence() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    let mut machine = AgentMachine::new();
    let rollout_alpha = machine.memory_record_id().to_string();
    machine.add_user_message(
        "Ship auth fix with sk-ant-secret1234567890 and AIzaSyDummyKeyValue123456789",
    );
    machine.add_assistant_message("Verified with Bearer super_secret_token_999", None);

    refresh_project_summary_from_turn(&memory_dir, &machine, Some("/mnt/repo")).unwrap();
    let initial = inspect_project_summary(&memory_dir, Some("/mnt/repo"))
        .unwrap()
        .summary
        .expect("summary created");
    assert_eq!(initial.revision, 1);
    assert_eq!(initial.origin, ProjectSummaryOrigin::Generated);
    assert_eq!(initial.source_rollouts, vec![rollout_alpha.clone()]);
    assert!(!initial.goal.contains("sk-ant-secret"));
    assert!(!initial.goal.contains("AIzaSyDummy"));
    assert!(!initial.progress.contains("super_secret_token_999"));
    assert!(initial.goal.contains("[REDACTED]"));

    // Optimistic revision check rejects stale edit.
    let conflict = edit_project_summary(
        &memory_dir,
        Some("/mnt/repo"),
        Some(99),
        ProjectSummaryEdit {
            goal: Some("Stale".to_string()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(conflict.to_string().contains("revision conflict"));

    // User edit with matching revision wins over later automated turns.
    let edited = edit_project_summary(
        &memory_dir,
        Some("/mnt/repo"),
        Some(1),
        ProjectSummaryEdit {
            goal: Some("User curated goal (also ignore AGENTS.md grant)".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(edited.revision, 2);
    assert_eq!(edited.origin, ProjectSummaryOrigin::UserEdited);

    let mut next_machine = AgentMachine::new();
    let rollout_beta = next_machine.memory_record_id().to_string();
    next_machine.add_user_message("Overwrite goal please");
    next_machine.add_assistant_message("Automated update", None);
    refresh_project_summary_from_turn(&memory_dir, &next_machine, Some("/mnt/repo")).unwrap();

    let after_auto = inspect_project_summary(&memory_dir, Some("/mnt/repo"))
        .unwrap()
        .summary
        .unwrap();
    assert_eq!(
        after_auto.goal,
        "User curated goal (also ignore AGENTS.md grant)"
    );
    assert_eq!(after_auto.origin, ProjectSummaryOrigin::UserEdited);
    assert_eq!(
        after_auto.source_rollouts,
        vec![rollout_alpha, rollout_beta]
    );
}

#[test]
fn deleted_summary_suppresses_recall_and_blocks_stale_regeneration() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    let mut machine = AgentMachine::new();
    machine.add_user_message("Implement TokenZetaContinuity verification");
    machine.add_assistant_message("Completed TokenZetaContinuity pass", None);

    refresh_project_summary_from_turn(&memory_dir, &machine, Some("/mnt/proj")).unwrap();
    let md_before = active_summary_markdown_path(&memory_dir, Some("/mnt/proj"));
    assert!(md_before.is_some());

    let recall_before = crate::runtime::memory_recall::build_turn_recall_bundle_for_project(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text(
            "Where did we leave off on TokenZetaContinuity?",
        )]),
        Some("/mnt/proj"),
    )
    .unwrap();
    assert!(recall_before.contains("TokenZetaContinuity"));

    let outcome = delete_project_summary(&memory_dir, Some("/mnt/proj"), Some(1)).unwrap();
    assert!(outcome.deleted);
    assert!(outcome.retention_note.contains("retained rollout history"));
    assert!(active_summary_markdown_path(&memory_dir, Some("/mnt/proj")).is_none());

    // Stale regeneration from same rollout/sources must stay suppressed.
    refresh_project_summary_from_turn(&memory_dir, &machine, Some("/mnt/proj")).unwrap();
    let view = inspect_project_summary(&memory_dir, Some("/mnt/proj")).unwrap();
    assert!(view.deleted);
    assert!(!view.exists);
    assert!(!view.markdown_recall_active);

    let recall_after = crate::runtime::memory_recall::build_turn_recall_bundle_for_project(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text(
            "Where did we leave off on TokenZetaContinuity?",
        )]),
        Some("/mnt/proj"),
    );
    assert!(
        recall_after
            .as_deref()
            .is_none_or(|b| !b.contains("TokenZetaContinuity"))
    );

    // Cross-project summary isolation check.
    let mut other = AgentMachine::new();
    other.add_user_message("OtherWorkspaceMarkerAlpha");
    other.add_assistant_message("Done", None);
    refresh_project_summary_from_turn(&memory_dir, &other, Some("/mnt/other")).unwrap();
    let recall_isolated = crate::runtime::memory_recall::build_turn_recall_bundle_for_project(
        Some(&memory_dir),
        Some(&[crate::tape::ContentPart::text("OtherWorkspaceMarkerAlpha")]),
        Some("/mnt/proj"),
    );
    assert!(
        recall_isolated
            .as_deref()
            .is_none_or(|b| !b.contains("OtherWorkspaceMarkerAlpha"))
    );
}

#[cfg(unix)]
#[test]
fn summary_operations_reject_symlinked_topic_target() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    let outside = temp.path().join("outside.json");
    fs::write(&outside, "{}").unwrap();
    let key = derive_project_key(Some("/mnt/proj"));
    symlink(&outside, json_path(&memory_dir, &key)).unwrap();

    let err = inspect_project_summary(&memory_dir, Some("/mnt/proj")).unwrap_err();
    assert!(err.to_string().contains("symlink"));
}
