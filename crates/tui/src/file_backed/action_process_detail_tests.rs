use super::*;
use crate::file_backed::stdio_tests::EXEC_SPEC;
use alan_agentfs::AgentFs;
use std::sync::Arc;

async fn load(shell: &alan_shell::Shell, app: &mut FileBackedApp) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    action_detail_io::start_pending(shell, app, &tx);
    for _ in 0..2 {
        app.dispatch(
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
}

#[tokio::test]
async fn previous_process_member_with_same_id_remains_selectable_without_substitution() {
    let (shell, root, _, pid) = live_root_agent().await;
    let old = format!("/agent/{pid}");
    let id = super::async_tests::allocate(&shell, &root, &old, "OLD retained original").await;
    let mut app = FileBackedApp::new(old.clone());
    file_surface::sync_action_from_file(&shell, &old, &id, &mut app)
        .await
        .unwrap();
    app.drain_committed_scrollback(80, 1);
    let next = shell.spawn(EXEC_SPEC).await.unwrap();
    root.bind_process(next.clone(), Arc::new(AgentFs::new()))
        .await;
    let current = format!("/agent/{next}");
    assert_eq!(
        super::async_tests::allocate(&shell, &root, &current, "NEW different original").await,
        id
    );
    app.reset_for_root_process_change();
    app.agent_path = current.clone();
    app.composer.set_text_with_cursor("草稿 😀 保存", 7);
    let draft = (
        app.composer.text().to_string(),
        app.composer.cursor(),
        app.input_intent,
    );
    let open = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    app.handle_key(open);
    load(&shell, &mut app).await;
    assert!(
        app.modal
            .rows
            .iter()
            .any(|row| row.to_string().contains("NEW different original"))
    );
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert!(
        app.modal.pending,
        "previous Process member has no selectable detail entry"
    );
    load(&shell, &mut app).await;
    let rows = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rows.contains("OLD retained original"), "{rows}");
    assert!(!rows.contains("NEW different original"));
    assert!(
        rows.contains(&old),
        "selected concrete Process must be visible: {rows}"
    );
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(
        (
            app.composer.text().to_string(),
            app.composer.cursor(),
            app.input_intent
        ),
        draft
    );
    assert!(root.unbind_process(&pid).await);
    app.handle_key(open);
    load(&shell, &mut app).await;
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert!(
        app.modal.pending,
        "previous Process member has no selectable detail entry"
    );
    load(&shell, &mut app).await;
    let rows = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rows.contains(&old) && rows.contains("unavailable"),
        "{rows}"
    );
    assert!(!rows.contains("NEW different original") && !rows.contains("OLD retained original"));
    assert_eq!(
        file_surface::read_action_ids(&shell, &current)
            .await
            .unwrap(),
        vec![id]
    );
}

#[test]
fn detail_owner_and_controls_remain_visible_while_paging_at_supported_widths() {
    use ratatui::{Terminal, backend::TestBackend};
    for width in [48, 80, 120] {
        let mut app = FileBackedApp::new("/agent/9".into());
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        app.modal.actions = vec![action_detail_io::ActionEntry {
            owner: "/agent/8".into(),
            id: "a0".into(),
        }];
        app.modal.rows = (0..100)
            .map(|i| ratatui::text::Line::from(format!("line {i}")))
            .collect();
        app.modal.scroll = 30;
        let mut terminal = Terminal::new(TestBackend::new(width, 12)).unwrap();
        terminal
            .draw(|frame| super::super::layout::draw(frame, &app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let row = |y| {
            (0..width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        };
        assert!(row(0).contains("1/1 · Action · /agent/8"));
        assert!(row(1).contains("Space/b page · Esc · p Plans"));
        assert!(row(2).contains("line 30"));
    }
}

#[tokio::test]
async fn same_id_cross_process_late_read_cannot_replace_current_selection() {
    use crate::file_backed::stdio_tests::FaultingFileServer;
    use alan_kernel::Access;
    let (shell, root, namespace, pid) = live_root_agent().await;
    let old = format!("/agent/{pid}");
    let id = super::async_tests::allocate(&shell, &root, &old, "OLD held original").await;
    let mut app = FileBackedApp::new(old.clone());
    file_surface::sync_action_from_file(&shell, &old, &id, &mut app)
        .await
        .unwrap();
    let next = shell.spawn(EXEC_SPEC).await.unwrap();
    root.bind_process(next.clone(), Arc::new(AgentFs::new()))
        .await;
    let current = format!("/agent/{next}");
    assert_eq!(
        super::async_tests::allocate(&shell, &root, &current, "NEW selected original").await,
        id
    );
    let slow = Arc::new(FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(slow.clone()),
        Access::ReadOnly,
    );
    app.reset_for_root_process_change();
    app.agent_path = current.clone();
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    load(&shell, &mut app).await;
    let (reached, resume) = slow.pause_next_walk_with_suffix(&format!("{pid}/actions/{id}/output"));
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(2), reached)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(app.modal.actions[app.modal.selected].owner, old);
    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    load(&shell, &mut app).await;
    resume.send(()).unwrap();
    app.dispatch(
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    assert_eq!(app.modal.actions[app.modal.selected].owner, current);
    let rows = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rows.contains("NEW selected original") && !rows.contains("OLD held original"),
        "{rows}"
    );
}

#[tokio::test]
async fn missing_current_catalog_keeps_explicit_old_reference_and_warns_without_cached_results() {
    let (shell, root, _, pid) = live_root_agent().await;
    let old = format!("/agent/{pid}");
    let id = super::async_tests::allocate(&shell, &root, &old, "OLD available original").await;
    let mut app = FileBackedApp::new(old.clone());
    file_surface::sync_action_from_file(&shell, &old, &id, &mut app)
        .await
        .unwrap();
    let next = shell.spawn(EXEC_SPEC).await.unwrap();
    root.bind_process(next.clone(), Arc::new(AgentFs::new()))
        .await;
    let current = format!("/agent/{next}");
    super::async_tests::allocate(&shell, &root, &current, "NEW removed original").await;
    app.reset_for_root_process_change();
    app.agent_path = current;
    let open = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    app.handle_key(open);
    load(&shell, &mut app).await;
    assert_eq!(app.modal.actions[app.modal.selected].owner, app.agent_path);
    assert!(root.unbind_process(&next).await);
    app.modal.pending = true;
    load(&shell, &mut app).await;
    assert_eq!(app.modal.actions[app.modal.selected].owner, app.agent_path);
    let text = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Current Process Action catalog unavailable") && text.contains("unavailable")
    );
    assert!(!text.contains("NEW removed original") && !text.contains("OLD available original"));
    app.handle_key(open);
    app.handle_key(open);
    load(&shell, &mut app).await;
    assert_eq!(app.modal.actions[app.modal.selected].owner, old);
    let text = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Current Process Action catalog unavailable")
            && text.contains("OLD available original")
    );
    assert!(!text.contains("NEW removed original"));
}
