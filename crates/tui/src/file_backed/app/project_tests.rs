use super::*;

fn receipt() -> ProjectMountReceipt {
    ProjectMountReceipt {
        grant_id: "new".into(),
        namespace_path: "/mnt/new".into(),
        label: "new".into(),
        access: ProjectAccess::ReadOnly,
    }
}
fn action(id: &str, status: &str, exit: i32, cwd: &str) -> ActionSnapshot {
    ActionSnapshot { id: "action".into(), name: "cd".into(), status: status.into(), output: String::new(), result: serde_json::json!({"call_id": id, "exit_code": exit, "outcome": {"success": exit == 0, "cwd": cwd}}).to_string() }
}
fn staged() -> FileBackedApp {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.activity.state = UiActivityState::Paused;
    app.stage_project_control(
        "/agent/1".into(),
        "selector".into(),
        Some((receipt(), "/nonexistent-fixture".into())),
        None,
    );
    app
}
#[test]
fn receipt_and_wrong_or_nonterminal_evidence_never_confirm() {
    let mut app = staged();
    assert!(app.project.is_none());
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
    app.observe_project_action("/agent/2", &action("selector", "completed", 0, "/mnt/new"));
    app.observe_project_action("/agent/1", &action("other", "completed", 0, "/mnt/new"));
    app.observe_project_action("/agent/1", &action("selector", "running", 0, "/mnt/new"));
    assert!(app.project.is_none());
    assert!(app.pending_project_control.is_some());
    app.observe_project_action("/agent/1", &action("selector", "completed", 0, "/mnt/new"));
    assert_eq!(app.project.as_ref().unwrap().grant_id, "new");
    assert_eq!(app.activity.state, UiActivityState::Paused);
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/mnt/new"));
}
#[test]
fn rejection_and_unexpected_cwd_preserve_binding_and_cleanup_only_new_grant() {
    for snapshot in [
        action("selector", "failed", 1, "/"),
        action("selector", "completed", 0, "/unexpected"),
    ] {
        let mut app = staged();
        let mut old = receipt();
        old.grant_id = "old".into();
        app.project = Some(old);
        app.namespace_cwd = "/mnt/old".into();
        app.observe_project_action("/agent/1", &snapshot);
        assert_eq!(app.project.as_ref().unwrap().grant_id, "old");
        assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/mnt/old"));
        assert_eq!(
            app.project_cleanup.as_deref(),
            if snapshot.status == "failed" {
                Some("new")
            } else {
                None
            }
        );
        assert!(app.pending_project_control.is_none());
    }
}
#[test]
fn reconnect_tombstones_selector_and_does_not_replay_or_confirm_late_action() {
    let mut app = staged();
    app.reset_for_root_process_change();
    let snapshot = action("selector", "completed", 0, "/mnt/new");
    app.observe_project_action("/agent/1", &snapshot);
    app.observe_action_cwd(&snapshot);
    assert!(app.project.is_none());
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
    assert!(
        app.project_cleanup.is_none(),
        "unknown effects must retain candidate authority"
    );
}
#[test]
fn revoke_requires_successful_root_cwd_action() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.project = Some(receipt());
    app.namespace_cwd = "/mnt/new".into();
    app.stage_project_control("/agent/1".into(), "revoke".into(), None, Some("new".into()));
    app.observe_project_action("/agent/1", &action("revoke", "failed", 1, "/"));
    assert!(app.take_ready_project_revoke().is_none());
    assert!(app.project.is_some());
    assert!(app.project_cleanup.is_none());
    app.stage_project_control(
        "/agent/1".into(),
        "revoke2".into(),
        None,
        Some("new".into()),
    );
    app.observe_project_action("/agent/1", &action("revoke2", "completed", 0, "/"));
    assert_eq!(app.take_ready_project_revoke().as_deref(), Some("new"));
    assert!(app.project.is_some());
}
#[test]
fn admission_yield_and_running_boundaries_are_blocked() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        "/agent/1",
        Some(alan_agent_protocol::UiQueueSnapshot::default()),
    );
    assert!(app.project_boundary_available(false));
    assert!(!app.project_boundary_available(true));
    app.activity.state = UiActivityState::Running;
    assert!(!app.project_boundary_available(false));
    app.activity.state = UiActivityState::Paused;
    assert!(app.project_boundary_available(false));
    app.activity.waiting_submission_ids.push("queued".into());
    assert!(!app.project_boundary_available(false));
    app.activity.waiting_submission_ids.clear();
    app.response_in_flight = Some("approval".into());
    assert!(!app.project_boundary_available(false));
}
