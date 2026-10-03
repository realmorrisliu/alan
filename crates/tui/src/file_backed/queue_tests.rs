use super::*;
use alan_agent_protocol::UiQueueSnapshot;

async fn put_queue(shell: &alan_shell::Shell, pid: &str, snapshot: &UiQueueSnapshot) {
    shell
        .write(
            &format!("/agent/{pid}/machine/ui/queue"),
            &serde_json::to_vec(snapshot).unwrap(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn queue_initial_paused_hydration_is_not_ready() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let snapshot = UiQueueSnapshot {
        known: true,
        revision: 7,
        paused: true,
        pending_submission_ids: vec![stdio_tests::INPUT_ID.into()],
        ..Default::default()
    };
    put_queue(&shell, &pid, &snapshot).await;
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let status = app.context_line(120).to_string();
    assert!(
        status.contains("paused") && status.contains("queued 1"),
        "initial authoritative queue must precede Ready: {status}"
    );
    assert!(app.project_boundary_available(false));
    tails.close().await;
}

#[tokio::test]
async fn queue_root_replacement_resets_revision_and_draft_receipt_owner() {
    use alan_agentfs::AgentFs;
    use alan_ap::InProcessTransport;
    use alan_kernel::Access;
    use std::sync::Arc;
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let old = UiQueueSnapshot {
        known: true,
        revision: 90,
        paused: true,
        pending_submission_ids: vec!["mine".into()],
        ..Default::default()
    };
    put_queue(&shell, &pid, &old).await;
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.set_text("draft");
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    tails.close().await;
    let next = shell.spawn(stdio_tests::EXEC_SPEC).await.unwrap();
    root.bind_process(next.clone(), Arc::new(AgentFs::new()))
        .await;
    root.set_root_process(next.clone()).await;
    namespace.replace_mount(
        stdio_tests::PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{next}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    let pending = VecDeque::from([PendingRootAgentTurn {
        input: "draft".into(),
        submission_id: "mine".into(),
        submitted_process: Some(pid.parse().unwrap()),
        submitted_at_ms: 0,
    }]);
    let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
        .await
        .unwrap();
    assert_eq!(app.queue.owner, format!("/agent/{next}"));
    assert!(app.queue.label().contains("unknown"));
    let new = UiQueueSnapshot {
        known: true,
        revision: 1,
        paused: true,
        pending_submission_ids: vec!["mine".into()],
        ..Default::default()
    };
    put_queue(&shell, &next, &new).await;
    app.queue.apply(
        &format!("/agent/{next}"),
        queue::read_queue(&shell, &format!("/agent/{next}")).await,
    );
    queue::confirm_local_receipts(&mut app, &pending);
    assert_eq!(
        app.composer.text(),
        "draft",
        "replacement receipt cannot confirm old owner submission"
    );
    assert!(app.queue.label().contains("queued 1"));
    tails.close().await;
}

#[tokio::test]
async fn queue_subscribe_before_read_catches_racing_publication() {
    use alan_ap::InProcessTransport;
    use alan_kernel::Access;
    use std::sync::Arc;
    let (shell, _, namespace, pid) = stdio_tests::live_root_agent().await;
    let server = Arc::new(stdio_tests::FaultingFileServer::new(Arc::new(
        alan_agentfs::AgentFs::new(),
    )));
    namespace.replace_mount(
        &format!("/agent/{pid}"),
        InProcessTransport::new(server.clone()),
        Access::ReadWrite,
    );
    let (reached, resume) = server.pause_next_walk_with_suffix("machine/ui/queue");
    let hydration_shell = shell.clone();
    let task = tokio::spawn(async move {
        let mut app = FileBackedApp::new("/agent/root".into());
        let tails = hydrate_and_open_tails(&hydration_shell, "/agent/root", &mut app)
            .await
            .unwrap();
        (app, tails)
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), reached)
        .await
        .unwrap()
        .unwrap();
    put_queue(
        &shell,
        &pid,
        &UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: true,
            pending_submission_ids: vec!["race".into()],
            ..Default::default()
        },
    )
    .await;
    resume.send(()).unwrap();
    let (app, tails) = task.await.unwrap();
    assert!(app.queue.label().contains("queued 1"));
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(event, FileBackedEvent::QueueChanged { .. }) {
            break;
        }
    }
    watchers.stop().await;
}

#[tokio::test]
async fn queue_fresh_unknown_allows_authorization_not_settled_claim() {
    let (shell, _, _, _) = stdio_tests::live_root_agent().await;
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    assert!(app.context_line(120).to_string().contains("queue unknown"));
    assert!(
        app.project_boundary_available(false),
        "valid no-admission snapshot permits authorization/control request, not settled-queue guarantee"
    );
    assert!(!app.project_boundary_available(true));
    tails.close().await;
}

#[tokio::test]
async fn queue_live_subscription_receipt_and_stale_revision() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.set_text("draft");
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    let pending = VecDeque::from([PendingRootAgentTurn {
        input: "draft".into(),
        submission_id: "mine".into(),
        submitted_process: Some(pid.parse().unwrap()),
        submitted_at_ms: 0,
    }]);
    queue::confirm_local_receipts(&mut app, &pending);
    assert_eq!(app.composer.text(), "draft");
    app.track_local_input(
        "mine",
        format!("/agent/{pid}"),
        "draft".into(),
        alan_agent_protocol::InputIntent::Agent,
    );
    let mut completion_pending = pending.clone();
    interrupt::observe_root_agent_completion(
        &mut completion_pending,
        &UiEvent::InputCompleted {
            submission_ids: vec!["mine".into()],
            status: alan_agent_protocol::UiInputStatus::Completed,
            error: None,
        },
        &mut app,
    );
    assert_eq!(
        app.composer.text(),
        "",
        "durable completion can precede queue refresh"
    );
    app.composer.set_text("draft");
    let mut snapshot = UiQueueSnapshot {
        known: true,
        revision: 2,
        paused: true,
        pending_submission_ids: vec!["other".into()],
        ..Default::default()
    };
    put_queue(&shell, &pid, &snapshot).await;
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        if let FileBackedEvent::QueueChanged { owner } = event {
            assert_eq!(owner, format!("/agent/{pid}"));
            app.queue
                .apply(&owner, queue::read_queue(&shell, &owner).await);
            break;
        }
    }
    queue::confirm_local_receipts(&mut app, &pending);
    assert_eq!(
        app.composer.text(),
        "draft",
        "foreign receipt cannot consume draft"
    );
    snapshot.revision = 3;
    snapshot.pending_submission_ids = vec!["mine".into()];
    put_queue(&shell, &pid, &snapshot).await;
    app.queue.apply(
        &format!("/agent/{pid}"),
        queue::read_queue(&shell, &format!("/agent/{pid}")).await,
    );
    queue::confirm_local_receipts(&mut app, &pending);
    assert_eq!(
        app.composer.text(),
        "draft",
        "a later receipt cannot consume an identical new draft"
    );
    snapshot.revision = 2;
    snapshot.pending_submission_ids.clear();
    put_queue(&shell, &pid, &snapshot).await;
    app.queue.apply(
        &format!("/agent/{pid}"),
        queue::read_queue(&shell, &format!("/agent/{pid}")).await,
    );
    assert!(app.queue.label().contains("queued 1"));
    watchers.stop().await;
}

#[tokio::test]
async fn queue_invalid_and_reconnect_unknown_are_not_empty() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let snapshot = UiQueueSnapshot {
        known: true,
        revision: 8,
        active_submission_ids: vec!["active".into()],
        ..Default::default()
    };
    put_queue(&shell, &pid, &snapshot).await;
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    assert!(!app.project_boundary_available(false));
    for (revision, deferred, uncertain) in [(9, true, false), (10, false, true)] {
        let unsafe_queue = UiQueueSnapshot {
            known: true,
            revision,
            deferred,
            uncertain_submission_ids: if uncertain {
                vec!["uncertain".into()]
            } else {
                vec![]
            },
            ..Default::default()
        };
        put_queue(&shell, &pid, &unsafe_queue).await;
        app.queue.apply(
            &format!("/agent/{pid}"),
            queue::read_queue(&shell, &format!("/agent/{pid}")).await,
        );
        assert!(!app.project_boundary_available(false));
    }
    assert!(queue::read_queue(&shell, "/agent/missing").await.is_none());
    tails.close().await;
    shell
        .write(&format!("/agent/{pid}/machine/ui/queue"), b"invalid")
        .await
        .unwrap();
    let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
        .await
        .unwrap();
    assert!(app.context_line(120).to_string().contains("queue unknown"));
    assert!(!app.project_boundary_available(false));
    tails.close().await;
}
