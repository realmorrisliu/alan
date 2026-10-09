use super::*;

#[tokio::test]
async fn unresolved_root_details_schedule_no_io_and_retry_with_resolved_owner() {
    use crate::file_backed::stdio_tests::FaultingFileServer;
    use alan_kernel::Access;
    use std::sync::Arc;
    let (shell, root, namespace, pid) = live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let id = super::async_tests::allocate(&shell, &root, &owner, "pinned retry evidence").await;
    let counted = Arc::new(FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(counted.clone()),
        Access::ReadOnly,
    );
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.set_text_with_cursor("界 draft 🦀", 4);
    let open = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    app.handle_key(open);
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    action_detail_io::start_pending_for_pid(&shell, &mut app, &tx, None);
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        counted.walk_count(),
        0,
        "unresolved Root scheduled AgentFS IO"
    );
    assert!(
        rx.try_recv().is_err(),
        "unresolved Root returned async details"
    );
    let text = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Root unavailable") && text.contains("retry"),
        "{text}"
    );
    assert!(app.modal.active && !app.modal.pending);
    assert_eq!(app.composer.text(), "界 draft 🦀");
    assert_eq!(app.composer.cursor(), 4);
    // A resolved watcher does not silently replace the unavailable modal.
    action_detail_io::start_pending_for_pid(&shell, &mut app, &tx, Some(pid.parse().unwrap()));
    assert_eq!(counted.walk_count(), 0);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.modal.active);
    app.handle_key(open);
    action_detail_io::start_pending_for_pid(&shell, &mut app, &tx, Some(pid.parse().unwrap()));
    for _ in 0..2 {
        app.dispatch(
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(app.modal.owner_path, owner);
    assert!(
        app.modal
            .actions
            .iter()
            .any(|entry| entry.owner == owner && entry.id == id)
    );
    assert!(
        app.modal
            .rows
            .iter()
            .any(|row| row.to_string().contains("pinned retry evidence"))
    );
    assert_eq!(app.composer.text(), "界 draft 🦀");
    assert_eq!(app.composer.cursor(), 4);
    // Concrete non-Root owners need no resolved Root PID.
    let mut concrete = FileBackedApp::new(owner.clone());
    concrete.handle_key(open);
    action_detail_io::start_pending_for_pid(&shell, &mut concrete, &tx, None);
    for _ in 0..2 {
        concrete.dispatch(
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(concrete.modal.owner_path, owner);
    assert!(
        concrete
            .modal
            .rows
            .iter()
            .any(|row| row.to_string().contains("pinned retry evidence"))
    );
}

#[tokio::test]
async fn real_root_alias_change_during_details_never_mixes_same_id_evidence() {
    use crate::file_backed::stdio_tests::{EXEC_SPEC, FaultingFileServer};
    use alan_agentfs::AgentFs;
    use alan_kernel::Access;
    use std::sync::Arc;
    let (shell, root, namespace, pid) = live_root_agent().await;
    let old_path = format!("/agent/{pid}");
    let id = super::async_tests::allocate(&shell, &root, &old_path, "OLD owner original").await;
    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    root.bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    let new_path = format!("/agent/{new_pid}");
    assert_eq!(
        super::async_tests::allocate(&shell, &root, &new_path, "NEW wrong original").await,
        id
    );
    let projection = serde_json::json!({"type":"evidence_projection","preview":"OLD","reference":{"path":format!("/agent/root/actions/{id}/output"),"length":18},"truncation":{"full_content_recoverable":true,"original_bytes":18,"preview_bytes":3}}).to_string();
    shell
        .write(
            &format!("{old_path}/actions/{id}/result"),
            projection.as_bytes(),
        )
        .await
        .unwrap();
    let slow = Arc::new(FaultingFileServer::new(root.clone()));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(slow.clone()),
        Access::ReadOnly,
    );
    let mut app = FileBackedApp::new("/agent/root".into());
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let (reached, resume) = slow.pause_next_walk_with_suffix(&format!("/actions/{id}/output"));
    action_detail_io::start_pending_at(&shell, &mut app, &tx, &old_path);
    app.dispatch(rx.recv().await.unwrap());
    reached.await.unwrap();
    root.set_root_process(new_pid).await;
    resume.send(()).unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    if let FileBackedEvent::ActionDetails { rows, .. } = &event {
        let text = rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.contains("NEW wrong original"), "{text}");
        assert!(
            text.contains("OLD owner original") && text.contains("Recovered reference"),
            "{text}"
        );
    } else {
        panic!("wrong event");
    }
    app.dispatch(event);
    assert!(
        app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("OLD owner original"))
    );
}

#[tokio::test]
async fn generic_metadata_preview_beats_raw_fallback_in_real_action_files() {
    for (status, presentation) in [
        ("completed", serde_json::Value::Null),
        ("failed", serde_json::json!({"form":"invalid"})),
        ("rejected", serde_json::Value::Null),
        ("cancelled", serde_json::Value::Null),
    ] {
        let result = serde_json::json!({"result_preview":"Readable cause: policy denied", "presentation":presentation}).to_string();
        let (shell, path, id) = action_fixture("RAW_ESCAPED_original_sentinel", &result).await;
        shell
            .write(&format!("{path}/actions/{id}/status"), status.as_bytes())
            .await
            .unwrap();
        let mut app = FileBackedApp::new(path.clone());
        file_surface::sync_action_from_file(&shell, &path, &id, &mut app)
            .await
            .unwrap();
        for width in [40, 60, 80, 120] {
            let rows = app.styled_history_lines(width);
            let text = rows
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                text.contains("Readable cause") && !text.contains("RAW_ESCAPED"),
                "{status}: {text}"
            );
            assert!(rows.len() <= 2 && text.len() <= 1024);
            if status != "completed" {
                assert!(text.contains(status), "{status}: {text}");
            }
        }
        let detail = action_detail_io::read_detail(&shell, &path, &id)
            .await
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            detail.contains("RAW_ESCAPED_original_sentinel")
                && detail.contains(&result)
                && detail.contains(status)
        );
    }
}
