use super::*;
use alan_ap::reference::MemFs;
use alan_kernel::Access;
use std::sync::Arc;

async fn host_grants(
    grants: &[(&str, &str, u64, bool)],
) -> (alan_shell::Shell, alan_kernel::LiveNamespace, String) {
    let (shell, _, namespace, pid) = stdio_tests::live_root_agent().await;
    namespace.replace_mount(
        "/mnt/host-mount/grants",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_files(
            grants
                .iter()
                .map(|(id, _, _, _)| ((*id).into(), Vec::new())),
        ))),
        Access::ReadOnly,
    );
    for (id, path, requester, active) in grants {
        for (prefix, field, value) in [
            (
                "grants",
                "record",
                serde_json::json!({
                    "id": id, "namespace_path": path, "active": active,
                }),
            ),
            (
                "requests",
                "request",
                serde_json::json!({
                    "id": id, "namespace_path": path, "requesting_pid": requester,
                }),
            ),
        ] {
            namespace.replace_mount(
                &format!("/mnt/host-mount/{prefix}/{id}"),
                InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
                    field,
                    serde_json::to_vec(&value).unwrap(),
                ))),
                Access::ReadOnly,
            );
        }
    }
    (shell, namespace, format!("/agent/{pid}"))
}

#[tokio::test]
async fn discovery_uses_actual_root_request_and_longest_component_prefix() {
    let (shell, _, owner) = host_grants(&[
        ("a-parent", "/mnt/project", 1, true),
        ("b-same-parent", "/mnt/project", 1, true),
        ("c-nearest", "/mnt/project/src", 1, true),
        ("d-other-root", "/mnt/project/src/deep", 99, true),
        ("e-inactive", "/mnt/project/src/deep", 1, false),
    ])
    .await;
    assert_eq!(
        discover_grant(
            &shell,
            &owner,
            std::path::Path::new("/mnt/project/src/deep")
        )
        .await
        .unwrap(),
        Some("c-nearest".into())
    );
    for cwd in ["/", "/mnt/project-other", "/mnt/unrelated"] {
        assert!(
            discover_grant(&shell, &owner, std::path::Path::new(cwd))
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        discover_grant(&shell, &owner, std::path::Path::new("/mnt/project"))
            .await
            .unwrap_err()
            .contains("ambiguous")
    );
}

#[tokio::test]
async fn discovery_refuses_unavailable_malformed_and_uncorrelated_records() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    assert!(
        discover_grant(&shell, &owner, std::path::Path::new("/mnt/project"))
            .await
            .is_err()
    );
    for text in [
        "{broken",
        r#"{"id":"wrong","namespace_path":"/mnt/project","active":true}"#,
        r#"{"id":"grant","namespace_path":"/mnt/project/../escape","active":true}"#,
        r#"{"id":"grant","namespace_path":"/mnt/project"}"#,
        &"x".repeat(4097),
    ] {
        let (shell, namespace, owner) = host_grants(&[("grant", "/mnt/project", 1, true)]).await;
        namespace.replace_mount(
            "/mnt/host-mount/grants/grant",
            InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
                "record",
                text.as_bytes(),
            ))),
            Access::ReadOnly,
        );
        assert!(
            discover_grant(&shell, &owner, std::path::Path::new("/mnt/project"))
                .await
                .is_err(),
            "{text}"
        );
    }
    let (shell, namespace, owner) = host_grants(&[("grant", "/mnt/project", 1, true)]).await;
    namespace.replace_mount(
        "/mnt/host-mount/requests/grant",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
            "request",
            br#"{"id":"grant","namespace_path":"/mnt/replaced","requesting_pid":1}"#,
        ))),
        Access::ReadOnly,
    );
    assert!(
        discover_grant(&shell, &owner, std::path::Path::new("/mnt/project"))
            .await
            .is_err()
    );
}

fn lookup_app() -> FileBackedApp {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.queue.apply(
        "/agent/1",
        Some(alan_agent_protocol::UiQueueSnapshot::default()),
    );
    app.activity.state = UiActivityState::Paused;
    app.namespace_cwd = "/mnt/external".into();
    app.project_host_pending = true;
    app
}

#[test]
fn discovery_result_never_mutates_replaced_root_cwd_or_unsettled_work() {
    for case in 0..3 {
        let mut app = lookup_app();
        let owner = if case == 0 { "/agent/2" } else { "/agent/1" };
        if case == 1 {
            app.namespace_cwd = "/mnt/changed".into();
        }
        assert!(
            finish_discovery(
                &mut app,
                "/agent/1".into(),
                Some(owner.into()),
                "/mnt/external".into(),
                case == 2,
                Ok(Some("grant".into()))
            )
            .is_none()
        );
        assert!(app.pending_project_control.is_none());
        assert!(app.retained_project_grant().is_none());
        assert!(!app.project_host_pending);
    }
}

#[test]
fn discovered_authority_uses_existing_confirmed_leave_then_revoke_protocol() {
    let mut app = lookup_app();
    let (owner, id, _) = finish_discovery(
        &mut app,
        "/agent/1".into(),
        Some("/agent/1".into()),
        "/mnt/external".into(),
        false,
        Ok(Some("external".into())),
    )
    .unwrap();
    assert!(app.project.is_none());
    assert_eq!(app.retained_project_grant().as_deref(), Some("external"));
    assert!(app.take_ready_project_revoke().is_none());
    for (call, success, cwd) in [
        ("other", true, "/"),
        (id.as_str(), false, "/mnt/external"),
        (id.as_str(), true, "/unexpected"),
        (id.as_str(), true, "/"),
    ] {
        let snapshot = ActionSnapshot {
            id: "a1".into(), name: "cd".into(), status: if success { "completed" } else { "failed" }.into(), output: String::new(),
            result: serde_json::json!({"call_id":call,"exit_code":if success{0}else{1},"outcome":{"success":success,"cwd":cwd}}).to_string(),
        };
        app.observe_project_action(&owner, &snapshot);
        if success && call == id && cwd == "/" {
            break;
        }
        assert!(app.take_ready_project_revoke().is_none());
    }
    assert_eq!(app.take_ready_project_revoke().as_deref(), Some("external"));
    assert_eq!(app.activity.state, UiActivityState::Paused);
    let command = || ProjectControl::Revoke {
        grant_id: "external".into(),
    };
    finish(
        &mut app,
        owner.clone(),
        Some(owner.clone()),
        command(),
        Err("lost ack".into()),
    );
    assert_eq!(app.retained_project_grant().as_deref(), Some("external"));
    finish(
        &mut app,
        owner.clone(),
        Some(owner),
        command(),
        Ok(ProjectControlResult::Revoked),
    );
    assert!(app.retained_project_grant().is_none());
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
}

#[tokio::test]
async fn discovery_background_reply_preserves_draft_and_correlates_context() {
    let (shell, _, owner) = host_grants(&[("grant", "/mnt/external", 1, true)]).await;
    let mut app = lookup_app();
    app.project_host_pending = false;
    app.composer.set_text("保留 draft");
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mut jobs = tokio::task::JoinSet::new();
    start_discovery(&mut app, &shell, owner.clone(), &mut jobs, &tx);
    assert!(app.project_host_pending);
    assert_eq!(app.composer.text(), "保留 draft");
    let FileBackedEvent::ProjectGrantLocated {
        owner: observed,
        cwd,
        result,
    } = rx.recv().await.unwrap()
    else {
        panic!("wrong response")
    };
    assert_eq!(observed, owner);
    assert_eq!(cwd, app.namespace_cwd);
    assert_eq!(result.unwrap(), Some("grant".into()));
    jobs.join_next().await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn held_discovery_keeps_input_and_quit_responsive_then_reports_timeout() {
    let (shell, _, namespace, pid) = stdio_tests::live_root_agent().await;
    let parent = Arc::new(stdio_tests::FaultingFileServer::new(Arc::new(
        MemFs::with_read_only_file("grants", Vec::new()),
    )));
    let (reached, _release) = parent.pause_next_walk_with_suffix("grants");
    namespace.replace_mount(
        "/mnt/host-mount",
        InProcessTransport::new(parent),
        Access::ReadOnly,
    );
    let mut app = lookup_app();
    app.project_host_pending = false;
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mut jobs = tokio::task::JoinSet::new();
    start_discovery(&mut app, &shell, format!("/agent/{pid}"), &mut jobs, &tx);
    reached.await.unwrap();
    app.composer.set_text("保留 draft");
    let enter = || {
        FileBackedEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )))
    };
    assert!(
        project_dispatch::dispatch_with_pending_submissions(&mut app, enter(), &VecDeque::new())
            .is_none()
    );
    assert_eq!(app.composer.text(), "保留 draft");
    let mut quit = app.clone();
    quit.composer.set_text("/quit");
    assert!(matches!(
        project_dispatch::dispatch_with_pending_submissions(&mut quit, enter(), &VecDeque::new()),
        Some(FileBackedAction::Quit)
    ));
    tokio::time::advance(std::time::Duration::from_secs(11)).await;
    let FileBackedEvent::ProjectGrantLocated { result, .. } = rx.recv().await.unwrap() else {
        panic!("wrong response")
    };
    assert!(result.unwrap_err().contains("timed out"));
    assert!(app.pending_project_control.is_none());
    jobs.join_next().await.unwrap().unwrap();
}

#[tokio::test]
async fn discovery_refuses_oversized_listing_without_partial_selection() {
    let (shell, _, namespace, pid) = stdio_tests::live_root_agent().await;
    namespace.replace_mount(
        "/mnt/host-mount/grants",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_files(
            (0..1025).map(|i| (format!("grant-{i}"), Vec::new())),
        ))),
        Access::ReadOnly,
    );
    assert!(
        discover_grant(
            &shell,
            &format!("/agent/{pid}"),
            std::path::Path::new("/mnt/project")
        )
        .await
        .unwrap_err()
        .contains("LimitExceeded")
    );
}
