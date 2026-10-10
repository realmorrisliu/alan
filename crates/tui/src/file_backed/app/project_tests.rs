use super::*;

#[path = "project_retry_tests.rs"]
mod retry_tests;

fn receipt() -> ProjectMountReceipt {
    ProjectMountReceipt {
        grant_id: "new".into(),
        namespace_path: "/mnt/new".into(),
        label: "new".into(),
        access: ProjectAccess::ReadOnly,
    }
}
fn action(id: &str, status: &str, exit: i32, cwd: &str) -> ActionSnapshot {
    ActionSnapshot { id: "action".into(), name: "cd".into(), status: status.into(), output: String::new(), result: serde_json::json!({"call_id": id, "title": "Select Process directory", "exit_code": exit, "outcome": {"success": exit == 0, "cwd": cwd}}).to_string() }
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
    app.queue.apply(
        "/agent/1",
        Some(alan_agent_protocol::UiQueueSnapshot::default()),
    );
    assert!(app.retained_project_mount.is_none());
    assert!(app.pending_project_control.is_none());
    assert!(app.project_boundary_available(false));
    assert!(app.handle_command("/project").is_none());
    assert_eq!(
        app.notice.as_deref(),
        Some("revoke the active project before selecting another")
    );
    assert!(
        matches!(app.handle_command("/project revoke"), Some(FileBackedAction::Project(ProjectControl::Revoke { grant_id })) if grant_id == "new")
    );
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
        assert_eq!(
            app.pending_project_control.is_none(),
            snapshot.status == "failed"
        );
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
    let pending = app
        .pending_project_control
        .as_ref()
        .expect("retain sole candidate receipt");
    assert_eq!(pending.owner, "/agent/1");
    assert_eq!(pending.id, "selector");
    assert_eq!(
        app.retained_project_mount.as_ref().unwrap().0.grant_id,
        "new"
    );
    assert!(!app.project_boundary_available(false));
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

#[test]
fn explicit_current_owner_recovery_retains_grant_until_confirmed_host_revoke() {
    let mut app = staged();
    app.fail_project_control("lost ack".into());
    app.reset_for_root_process_change();
    app.queue.apply(
        "/agent/2",
        Some(alan_agent_protocol::UiQueueSnapshot::default()),
    );
    assert!(
        matches!(app.handle_command("/project revoke"), Some(FileBackedAction::Project(ProjectControl::Revoke { grant_id })) if grant_id == "new")
    );
    app.stage_project_control(
        "/agent/2".into(),
        "recover".into(),
        None,
        Some("new".into()),
    );
    app.observe_project_action("/agent/1", &action("selector", "completed", 0, "/mnt/new"));
    assert!(app.take_ready_project_revoke().is_none());
    app.observe_project_action("/agent/2", &action("recover", "completed", 0, "/"));
    assert_eq!(app.take_ready_project_revoke().as_deref(), Some("new"));
    assert_eq!(app.retained_project_grant().as_deref(), Some("new"));
    assert!(!app.project_boundary_available(false));
    assert!(app.take_ready_project_revoke().is_none());
    app.project_revoked();
    assert!(app.retained_project_grant().is_none());
    assert!(app.project_boundary_available(false));
}

#[test]
fn candidate_cleanup_retains_failed_authority_and_requires_explicit_retry() {
    for rejected in [true, false] {
        let mut app = if rejected {
            staged()
        } else {
            FileBackedApp::new("/agent/root".into())
        };
        if rejected {
            app.observe_project_action("/agent/1", &action("selector", "rejected", 1, "/"));
        } else {
            app.project_cleanup = Some("new".into());
        }
        app.queue.apply(
            "/agent/1",
            Some(alan_agent_protocol::UiQueueSnapshot::default()),
        );
        let grant = app.take_project_cleanup().unwrap();
        app.finish_project_cleanup(&grant, false);
        assert_eq!(app.project_cleanup.as_deref(), Some("new"));
        assert!(app.take_project_cleanup().is_none());
        assert!(!app.project_boundary_available(false));
        assert!(app.handle_command("/project revoke").is_none());
        assert_eq!(app.take_project_cleanup().as_deref(), Some("new"));
        app.finish_project_cleanup("new", true);
        assert!(app.project_cleanup.is_none());
        assert!(app.project_boundary_available(false));
    }
}

#[test]
fn ordinary_cd_context_uses_observed_namespace_cwd_without_a_picker_receipt() {
    let mut app = FileBackedApp::new("/agent/1".into());
    crate::file_backed::model_tests::install_header_model(&mut app, "gpt-6.1-sol");
    assert!(app.context_line(120).to_string().contains("no project"));
    let mut snapshot = ActionSnapshot {
        id: "external-project-cd".into(),
        name: "cd".into(),
        status: "running".into(),
        output: String::new(),
        result: serde_json::json!({"outcome": {"cwd": "/mnt/project-request-1/src"}}).to_string(),
    };
    for status in ["running", "failed", "rejected"] {
        snapshot.status = status.into();
        app.observe_action_cwd(&snapshot);
        assert!(app.context_line(120).to_string().contains("no project"));
    }
    snapshot.status = "completed".into();
    app.observe_action_cwd(&snapshot);
    let context = app.context_line(120).to_string();
    assert!(context.contains("/mnt/project-request-1/src"), "{context}");
    assert!(!context.contains("no project") && !context.contains("read-only"));
    assert!(
        app.project.is_none(),
        "cwd does not create a project receipt"
    );
    for width in [32, 40, 48, 80] {
        let line = app.context_line(width);
        let text = line.to_string();
        assert!(line.width() <= width, "{width}: {text}");
        assert!(
            text.contains("gpt-6.1-sol") && text.contains("ready"),
            "{text}"
        );
    }
    snapshot.result = serde_json::json!({"outcome": {"cwd": "/"}}).to_string();
    app.observe_action_cwd(&snapshot);
    assert!(app.context_line(120).to_string().contains("no project"));
}

#[test]
fn root_change_discards_previous_ordinary_cd_context() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.observe_action_cwd(&ActionSnapshot {
        id: "old-cd".into(),
        name: "cd".into(),
        status: "completed".into(),
        output: String::new(),
        result: serde_json::json!({"outcome": {"cwd": "/mnt/old"}}).to_string(),
    });
    app.reset_for_root_process_change();
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
    assert!(app.context_line(120).to_string().contains("no project"));
}
