use super::*;

#[test]
fn root_reset_preserves_retained_project_recovery_notice() {
    let mut app = staged();
    app.reset_for_root_process_change();
    assert!(app.pending_project_control.as_ref().unwrap().fenced);
    assert_eq!(app.retained_project_grant().as_deref(), Some("new"));
    assert!(app.notice.as_deref().unwrap().contains("/project revoke"));
    assert!(app.notice.as_deref().unwrap().contains("grant retained"));

    let mut both = staged();
    both.model_chooser.uncertain = Some(("/agent/1".into(), "model-selection".into()));
    both.reset_for_root_process_change();
    let notice = both.notice.as_deref().unwrap();
    assert!(notice.contains("/project revoke"));
    assert!(notice.contains("model selection outcome uncertain"));

    let mut unknown = FileBackedApp::new("/agent/root".into());
    let command = ProjectControl::Mount {
        operation_id: project_dispatch::project_operation_id(),
        host_path: "/fixture".into(),
        access: ProjectAccess::ReadOnly,
    };
    unknown.uncertain_project_mount = Some(("/agent/1".into(), command.clone()));
    unknown.composer.set_text_with_cursor("界 draft 🦀", 4);
    unknown.input_intent = InputIntent::ForceAgent;
    unknown.reset_for_root_process_change();
    assert_eq!(
        unknown.uncertain_project_mount.as_ref().unwrap(),
        &("/agent/1".into(), command)
    );
    assert!(
        unknown
            .notice
            .as_deref()
            .unwrap()
            .contains("project outcome unknown")
    );
    assert!(unknown.notice.as_deref().unwrap().contains("original Root"));
    assert!(!unknown.project_boundary_available(false));
    assert_eq!(unknown.composer.text(), "界 draft 🦀");
    assert_eq!(unknown.composer.cursor(), 4);
    assert_eq!(unknown.input_intent, InputIntent::ForceAgent);

    let mut fresh = FileBackedApp::new("/agent/root".into());
    fresh.reset_for_root_process_change();
    assert!(fresh.notice.is_none());
}

#[test]
fn recovery_retries_replace_control_without_losing_candidate_receipt() {
    let mut app = staged();
    let candidate = app.retained_project_mount.clone().unwrap();
    app.fail_project_control("lost selector ack".into());
    for attempt in 0..128 {
        let id = format!("recover-{attempt}");
        app.stage_project_control("/agent/2".into(), id.clone(), None, Some("new".into()));
        let control = app.pending_project_control.as_ref().unwrap();
        assert_eq!(control.owner, "/agent/2");
        assert_eq!(control.id, id);
        assert_eq!(control.target, "/");
        assert!(!control.fenced);
        assert_eq!(app.retained_project_mount.as_ref(), Some(&candidate));
        app.observe_project_action("/agent/1", &action("selector", "failed", 1, "/"));
        app.observe_project_action("/agent/2", &action("recover-stale", "completed", 0, "/"));
        assert!(app.take_ready_project_revoke().is_none());
        // Failed leave is not rejection of the original candidate mount.
        app.observe_project_action("/agent/2", &action(&id, "failed", 1, "/"));
        assert!(app.project_cleanup.is_none());
        app.fail_project_control("lost recovery ack".into());
        assert_eq!(app.retained_project_mount.as_ref(), Some(&candidate));
        app.observe_project_action("/agent/2", &action(&id, "completed", 0, "/"));
        assert_eq!(app.take_ready_project_revoke().as_deref(), Some("new"));
        // Taking the revoke request or a failed Host revoke is not Revoked.
        assert_eq!(app.retained_project_mount.as_ref(), Some(&candidate));
        assert!(app.pending_project_control.as_ref().unwrap().fenced);
        assert!(
            app.project.is_none(),
            "leave must not install candidate as active project"
        );
        assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
    }
    app.project_revoked();
    assert!(app.retained_project_mount.is_none());
    assert!(app.pending_project_control.is_none());
    assert!(app.retained_project_grant().is_none());
}

#[test]
fn successful_selection_transfers_authority_until_actual_revoked() {
    let mut app = staged();
    let candidate = app.retained_project_mount.clone().unwrap();
    app.observe_project_action("/agent/1", &action("selector", "completed", 0, "/mnt/new"));
    assert!(app.retained_project_mount.is_none());
    assert_eq!(app.project.as_ref(), Some(&candidate.0));
    assert_eq!(app.retained_project_grant().as_deref(), Some("new"));
    app.stage_project_control("/agent/1".into(), "leave".into(), None, Some("new".into()));
    app.observe_project_action("/agent/1", &action("selector", "rejected", 1, "/"));
    assert!(app.retained_project_mount.is_none());
    assert_eq!(app.project.as_ref(), Some(&candidate.0));
    app.observe_project_action("/agent/1", &action("leave", "completed", 0, "/"));
    assert!(app.retained_project_mount.is_none());
    assert_eq!(app.project.as_ref(), Some(&candidate.0));
    assert_eq!(app.retained_project_grant().as_deref(), Some("new"));
    app.project_revoked();
    assert!(app.retained_project_mount.is_none());
    assert!(app.project.is_none());
}

#[test]
fn only_correlated_unfenced_candidate_rejection_clears_receipt() {
    let mut app = staged();
    app.fence_project_control();
    app.observe_project_action("/agent/1", &action("selector", "rejected", 1, "/"));
    assert!(app.retained_project_mount.is_some());
    let mut app = staged();
    app.observe_project_action("/agent/1", &action("selector", "rejected", 1, "/"));
    assert!(app.retained_project_mount.is_none());
    assert_eq!(app.project_cleanup.as_deref(), Some("new"));
}

#[test]
fn retired_project_actions_never_flow_through_generic_cwd_observer() {
    let mut app = staged();
    app.stage_project_control(
        "/agent/2".into(),
        "project-cwd-recovery".into(),
        None,
        Some("new".into()),
    );
    app.observe_project_action(
        "/agent/2",
        &action("project-cwd-recovery", "completed", 0, "/"),
    );
    app.project_revoked();
    app.observe_action_cwd(&action(
        "11111111-1111-4111-8111-111111111111",
        "completed",
        0,
        "/mnt/new",
    ));
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
    let mut ordinary = action("ordinary-cd", "completed", 0, "/other");
    let mut result: serde_json::Value = serde_json::from_str(&ordinary.result).unwrap();
    result["title"] = serde_json::json!("cd /other");
    ordinary.result = result.to_string();
    app.observe_action_cwd(&ordinary);
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/other"));
}
