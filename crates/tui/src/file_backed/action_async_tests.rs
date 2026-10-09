use super::*;
use crate::file_backed::stdio_tests::FaultingFileServer;
use alan_agentfs::AgentFs;
use alan_kernel::Access;
use std::sync::Arc;

pub(super) async fn allocate(
    shell: &alan_shell::Shell,
    root: &alan_agentfs::AgentRootFs,
    path: &str,
    output: &str,
) -> String {
    let fid = Fid(1900);
    let pid = path.strip_prefix("/agent/").unwrap();
    root.walk(
        Fid::ROOT,
        fid,
        &[pid.into(), "actions".into(), "clone".into()],
    )
    .await
    .unwrap();
    root.open(fid, OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(root.read(fid, 0, 64).await.unwrap()).unwrap();
    root.clunk(fid).await.unwrap();
    for (field, text) in [
        ("output", output),
        ("name", "read"),
        ("status", "completed"),
    ] {
        shell
            .write(&format!("{path}/actions/{id}/{field}"), text.as_bytes())
            .await
            .unwrap();
    }
    id
}

async fn receive(rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>) -> FileBackedEvent {
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn real_retention_expiry_and_process_delete_reopen_without_cache() {
    let (shell, root, _, pid) = live_root_agent().await;
    let store = Arc::new(AgentFs::new());
    root.bind_process(pid.clone(), store.clone()).await;
    let path = format!("/agent/{pid}");
    let id = allocate(&shell, &root, &path, "retained original sentinel").await;
    let result = serde_json::json!({"type":"evidence_projection","preview":"retained","reference":{"path":format!("{path}/actions/{id}/output"),"offset":0,"length":26},"truncation":{"full_content_recoverable":true,"original_bytes":26,"preview_bytes":8}}).to_string();
    shell
        .write(&format!("{path}/actions/{id}/result"), result.as_bytes())
        .await
        .unwrap();
    let mut app = FileBackedApp::new(path.clone());
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let open = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    app.handle_key(open);
    action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(receive(&mut rx).await);
    app.dispatch(receive(&mut rx).await);
    assert!(
        app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("retained original sentinel"))
    );
    app.handle_key(open);
    store
        .expire_action_output_for_retention(&id, "test storing policy")
        .await
        .unwrap();
    app.handle_key(open);
    action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(receive(&mut rx).await);
    app.dispatch(receive(&mut rx).await);
    assert!(
        app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("Evidence retention expired"))
    );
    assert!(
        !app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("retained original sentinel"))
    );
    app.handle_key(open);
    assert!(root.unbind_process(&pid).await);
    app.handle_key(open);
    action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(receive(&mut rx).await);
    app.dispatch(receive(&mut rx).await);
    assert!(
        app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("list actions failed"))
    );
}

#[tokio::test]
async fn slow_read_allows_ticks_selection_escape_and_root_switch() {
    let (shell, root, namespace, pid) = live_root_agent().await;
    let path = format!("/agent/{pid}");
    let first = allocate(&shell, &root, &path, "first retained original").await;
    let second = allocate(&shell, &root, &path, "second retained original").await;
    let slow = Arc::new(FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(slow.clone()),
        Access::ReadOnly,
    );
    let mut app = FileBackedApp::new(path.clone());
    for snapshot_id in [&first, &second] {
        file_surface::sync_action_from_file(&shell, &path, snapshot_id, &mut app)
            .await
            .unwrap();
    }
    assert_eq!(app.drain_committed_scrollback(80, 1).len(), 4);
    assert!(app.action_cells.is_empty());
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let open = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    app.handle_key(open);
    action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(receive(&mut rx).await);
    app.dispatch(receive(&mut rx).await);
    assert_eq!(app.modal.ids[app.modal.selected], second);
    let (reached, resume) =
        slow.pause_read_after_matching_reads(&format!("/actions/{first}/output"), 1);
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(receive(&mut rx).await);
    tokio::time::timeout(Duration::from_secs(2), reached)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(app.modal.ids, vec![first.clone(), second.clone()]);
    let mut tick = tokio::time::interval(Duration::from_millis(1));
    for _ in 0..3 {
        tick.tick().await;
        app.dispatch(FileBackedEvent::Terminal(crossterm::event::Event::Paste(
            "blocked".into(),
        )));
    }
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(receive(&mut rx).await);
    app.dispatch(receive(&mut rx).await);
    assert!(
        app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("second retained original"))
    );
    resume.send(()).unwrap();
    app.dispatch(receive(&mut rx).await);
    assert_eq!(app.modal.ids[app.modal.selected], second);
    assert!(
        !app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("first retained original"))
    );
    for switch_root in [false, true] {
        let (reached, resume) =
            slow.pause_read_after_matching_reads(&format!("/actions/{first}/output"), 1);
        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        action_detail_io::start_pending(&shell, &mut app, &tx);
        app.dispatch(receive(&mut rx).await);
        reached.await.unwrap();
        if switch_root {
            app.reset_for_root_process_change();
            app.agent_path = "/agent/new".into();
        } else {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        }
        resume.send(()).unwrap();
        app.dispatch(receive(&mut rx).await);
        assert!(!app.modal.active);
        if !switch_root {
            app.handle_key(open);
            action_detail_io::start_pending(&shell, &mut app, &tx);
            app.dispatch(receive(&mut rx).await);
            app.dispatch(receive(&mut rx).await);
            assert_eq!(app.modal.ids[app.modal.selected], second);
        }
    }
}
