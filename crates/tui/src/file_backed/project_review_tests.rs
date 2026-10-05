use super::*;
use alan_ap::{Fid, FileServer, OpenMode};
use std::sync::Arc;

#[test]
fn actual_project_producer_envelope_accepts_uuid_contract() {
    // Engine Selector requires a UUID. UserInputRecord::validate applies the
    // same uuid::Uuid::parse_str contract without a new TUI dependency.
    for path in ["/mnt/candidate", "/"] {
        let (id, command) = project_dispatch::project_selector(path);
        let envelope: serde_json::Value =
            serde_json::from_str(command.strip_prefix("project-cwd-v1 ").unwrap()).unwrap();
        assert_eq!(envelope["id"], id);
        assert_eq!(envelope["path"], path);
        assert_eq!(envelope.as_object().unwrap().len(), 2);
        let mut record = alan_agent_protocol::UserInputRecord::new(
            alan_agent_protocol::InputIntent::Command,
            alan_agent_protocol::InputMode::FollowUp,
            "UUID contract probe",
        );
        record.submission_id = envelope["id"].as_str().unwrap().to_owned();
        record
            .validate()
            .expect("actual Mount/Revoke envelope ID must satisfy Engine UUID parser");
    }
}

#[tokio::test]
async fn mounted_receipt_survives_actual_ctl_lost_ack_and_late_file_settlement() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let fault = Arc::new(stdio_tests::FaultingFileServer::new(root.clone()));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    fault.lose_write_ack("/machine/ctl");
    let (reached, resume) = fault.pause_next_write_with_suffix("/machine/ctl");
    let owner = format!("/agent/{pid}");
    let receipt = ProjectMountReceipt {
        grant_id: "active-host-grant".into(),
        namespace_path: "/mnt/candidate".into(),
        label: "candidate".into(),
        access: ProjectAccess::ReadWrite,
    };
    let active = Arc::new(std::sync::Mutex::new(None::<String>));
    let handler: ProjectControlHandler = {
        let active = active.clone();
        let receipt = receipt.clone();
        Arc::new(move |control| {
            let active = active.clone();
            let receipt = receipt.clone();
            Box::pin(async move {
                match control {
                    ProjectControl::Mount { .. } => {
                        *active.lock().unwrap() = Some(receipt.grant_id.clone());
                        Ok(ProjectControlResult::Mounted {
                            receipt,
                            completion_root: "/unused".into(),
                        })
                    }
                    ProjectControl::Revoke { grant_id } => {
                        assert_eq!(active.lock().unwrap().as_deref(), Some(grant_id.as_str()));
                        anyhow::bail!("injected Host revoke failure; grant remains active")
                    }
                }
            })
        })
    };
    let mounted = handler(ProjectControl::Mount {
        host_path: "/unused".into(),
        access: ProjectAccess::ReadWrite,
    })
    .await
    .unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    app.activity.state = UiActivityState::Paused;
    let ProjectControlResult::Mounted {
        receipt,
        completion_root,
    } = mounted
    else {
        unreachable!()
    };
    let (selector_id, command) = project_dispatch::project_selector("/mnt/candidate");
    app.stage_project_control(
        owner.clone(),
        selector_id.clone(),
        Some((receipt.clone(), completion_root)),
        None,
    );
    let write = write_machine_ctl(&shell, &owner, &command);
    tokio::pin!(write);
    tokio::select! { result = &mut write => panic!("write completed before pause: {result:?}"), result = reached => result.unwrap() }
    resume.send(()).unwrap();
    let error = write.await.unwrap_err();
    app.fail_project_control(format!("effects uncertain: {error:#}"));
    assert_eq!(active.lock().unwrap().as_deref(), Some("active-host-grant"));
    assert!(app.project.is_none());
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
    assert!(!app.project_boundary_available(false));
    let pending = app
        .pending_project_control
        .as_ref()
        .expect("sole Mounted receipt must survive actual ctl error");
    assert_eq!(
        app.retained_project_mount.as_ref().unwrap().0.grant_id,
        "active-host-grant"
    );
    assert_eq!(pending.owner, owner);
    assert_eq!(pending.id, selector_id);
    let fid = Fid(1900);
    root.walk(
        Fid::ROOT,
        fid,
        &[pid.clone(), "actions".into(), "clone".into()],
    )
    .await
    .unwrap();
    root.open(fid, OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(root.read(fid, 0, 64).await.unwrap()).unwrap();
    root.clunk(fid).await.unwrap();
    shell
        .write(&format!("{owner}/actions/{id}/status"), b"completed")
        .await
        .unwrap();
    shell
        .write(&format!("{owner}/actions/{id}/name"), b"cd")
        .await
        .unwrap();
    shell.write(&format!("{owner}/actions/{id}/result"), &serde_json::to_vec(&serde_json::json!({"call_id":selector_id,"title":"Select Process directory","exit_code":0,"outcome":{"success":true,"cwd":"/mnt/candidate"}})).unwrap()).await.unwrap();
    sync_action_from_file(&shell, &owner, &id, &mut app)
        .await
        .unwrap();
    assert_eq!(app.project.as_ref().unwrap().grant_id, "active-host-grant");
    assert_eq!(app.activity.state, UiActivityState::Paused);
    app.queue.apply(
        &owner,
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
        matches!(app.handle_command("/project revoke"), Some(FileBackedAction::Project(ProjectControl::Revoke { grant_id })) if grant_id == "active-host-grant")
    );

    // A confirmed leave makes Host revoke eligible; a failed Host response
    // must not clear the handler-owned grant or the UI's only receipt.
    app.stage_project_control(
        owner.clone(),
        "leave".into(),
        None,
        Some("active-host-grant".into()),
    );
    shell
        .write(
            &format!("{owner}/actions/{id}/result"),
            br#"{"call_id":"leave","exit_code":0,"outcome":{"success":true,"cwd":"/"}}"#,
        )
        .await
        .unwrap();
    sync_action_from_file(&shell, &owner, &id, &mut app)
        .await
        .unwrap();
    let grant_id = app
        .take_ready_project_revoke()
        .expect("confirmed leave enables revoke");
    let error = handler(ProjectControl::Revoke { grant_id })
        .await
        .unwrap_err();
    app.push_error(format!("project revoke failed: {error:#}"));
    assert_eq!(active.lock().unwrap().as_deref(), Some("active-host-grant"));
    assert_eq!(
        app.retained_project_grant().as_deref(),
        Some("active-host-grant")
    );
    assert_eq!(app.project.as_ref().unwrap().grant_id, "active-host-grant");
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));

    // Replacement fencing refuses late successful old-owner evidence even
    // with the same call id, while preserving authority for explicit recovery.
    app.stage_project_control(
        owner.clone(),
        "lost-ack".into(),
        Some((receipt.clone(), "/unused".into())),
        None,
    );
    let next = model_transport_tests::replace(&shell, &root, &namespace).await;
    app.reset_for_root_process_change();
    app.agent_path = format!("/agent/{next}");
    assert_ne!(app.agent_path, owner);
    shell.write(&format!("{owner}/actions/{id}/result"), br#"{"call_id":"lost-ack","exit_code":0,"outcome":{"success":true,"cwd":"/mnt/candidate"}}"#).await.unwrap();
    sync_action_from_file(&shell, &owner, &id, &mut app)
        .await
        .unwrap();
    assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
    assert!(app.pending_project_control.as_ref().unwrap().fenced);
    assert_eq!(
        app.retained_project_grant().as_deref(),
        Some("active-host-grant")
    );
    assert_eq!(active.lock().unwrap().as_deref(), Some("active-host-grant"));
}
