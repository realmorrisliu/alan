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
    let status = app.attention_labels().join(" · ").to_lowercase();
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
    assert!(
        !app.attention_labels()
            .join(" · ")
            .to_lowercase()
            .contains("queue unavailable")
    );
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
    assert!(
        app.attention_labels()
            .join(" · ")
            .to_lowercase()
            .contains("queue unavailable")
    );
    assert!(!app.project_boundary_available(false));
    tails.close().await;
}

#[test]
fn queued_input_behind_active_turn_is_not_interleaved_and_survives_pruned_yield() {
    use crate::history::{HistoryCell, PendingYieldCell, RenderOpts, RunningTool, ToolStatus};
    use alan_agent_protocol::{InputIntent, UiActivitySnapshot, UiEvent, YieldKind};

    let mut app = FileBackedApp::new("/agent/root".into());
    let owner = "/agent/1";
    app.queue.apply(
        owner,
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: false,
            active_submission_ids: vec!["turn-1".into()],
            ..Default::default()
        }),
    );

    // Turn 1 begins and runs enough lines to prune its User cell into scrollback.
    app.apply_tape_record(serde_json::from_str(
        r#"{"version":1,"kind":"message","role":"user","content":"turn one prompt\nline2\nline3","submission_id":"turn-1"}"#,
    ).unwrap());
    app.upsert_action_cell(
        "act-1".into(),
        HistoryCell::Tool {
            action: None,
            title: "Bash ls".into(),
            status: ToolStatus::Complete,
            preview: Some("ok".into()),
            presentation: None,
        },
    );
    app.prune_rendered_prefix(RenderOpts::new(40, false), 10);

    // User queues Turn 2 while Turn 1 is actively running.
    app.composer.set_text("queued turn two");
    app.track_local_input(
        "turn-2",
        owner.into(),
        "queued turn two".into(),
        InputIntent::Agent,
    );
    app.queue.apply(
        owner,
        Some(UiQueueSnapshot {
            known: true,
            revision: 2,
            paused: false,
            active_submission_ids: vec!["turn-1".into()],
            pending_submission_ids: vec!["turn-2".into()],
            ..Default::default()
        }),
    );
    app.acknowledge_local_input("turn-2", owner);
    assert!(
        app.composer.text().is_empty(),
        "composer should clear on queue acknowledgement"
    );
    assert!(
        !app.transcript.iter().any(|cell| cell
            .input_source()
            .is_some_and(|(body, ..)| body == "queued turn two")),
        "queued turn two must stay out of transcript while Turn 1 is still active: {:?}",
        app.transcript
    );

    // Stale waiting_submission_ids while a tool is running must not report "waiting for input".
    app.activity = UiActivitySnapshot::running(1);
    app.activity.waiting_submission_ids.push("req-old".into());
    app.running_tools.push(RunningTool {
        id: "act-2".into(),
        title: "Bash python3".into(),
        tail_lines: Vec::new(),
    });
    assert!(!app.has_pending_response());
    app.running_tools.clear();
    app.activity.waiting_submission_ids.clear();

    // Turn 1 yields for confirmation and then resumes another Bash tool call.
    app.set_pending_yield(PendingYieldCell {
        request_id: "req-1".into(),
        kind: YieldKind::Confirmation,
        title: "Escalate tool call 'bash'?".into(),
        prompt: None,
        options: vec!["approve".into(), "reject".into()],
        default_option: Some("approve".into()),
        questions: Vec::new(),
        capability: Some("network".into()),
        reason: Some("needs human judgment".into()),
        presentation: None,
    });
    app.clear_pending_yield();
    app.upsert_action_cell(
        "act-2".into(),
        HistoryCell::Tool {
            action: None,
            title: "Bash python3".into(),
            status: ToolStatus::Complete,
            preview: Some("resumed ok".into()),
            presentation: None,
        },
    );
    assert_eq!(
        app.pending_remote_turn_start, None,
        "resumed Turn 1 tool after pruned user cell + yield must not arm remote turn splicing"
    );

    // Turn 1 finishes and Turn 2 begins on Tape.
    app.apply_ui_event(UiEvent::InputCompleted {
        submission_ids: vec!["turn-1".into()],
        status: alan_agent_protocol::UiInputStatus::Completed,
        error: None,
    });
    app.apply_tape_record(serde_json::from_str(
        r#"{"version":1,"kind":"message","role":"user","content":"queued turn two","submission_id":"turn-2"}"#,
    ).unwrap());

    let act2_idx = app
        .transcript
        .iter()
        .position(|c| matches!(c, HistoryCell::Tool { title, .. } if title == "Bash python3"))
        .unwrap();
    let turn2_idx = app
        .transcript
        .iter()
        .position(|c| {
            c.input_source()
                .is_some_and(|(body, ..)| body == "queued turn two")
        })
        .unwrap();
    assert!(
        turn2_idx > act2_idx,
        "Turn 2 user boundary must appear AFTER Turn 1 resumed actions, not spliced above them: {:?}",
        app.transcript
    );
}

#[test]
fn provisional_receipt_before_tape_preserves_user_before_early_action_and_output() {
    use crate::history::{HistoryCell, ToolStatus};
    use alan_agent_protocol::InputIntent;

    let mut app = FileBackedApp::new("/agent/root".into());
    let owner = "/agent/1";
    app.queue.owner = owner.into();
    app.queue.apply(
        owner,
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: false,
            pending_submission_ids: vec!["sub-race".into()],
            ..Default::default()
        }),
    );

    app.track_local_input(
        "sub-race",
        owner.into(),
        "race prompt".into(),
        InputIntent::Agent,
    );
    app.acknowledge_local_input("sub-race", owner);
    assert!(
        !app.active_turn_has_user_boundary,
        "untaped provisional receipt must not pretend an authoritative tape boundary was seen"
    );

    app.upsert_action_cell(
        "act-early".into(),
        HistoryCell::Tool {
            action: None,
            title: "Bash ls".into(),
            status: ToolStatus::Complete,
            preview: Some("ok".into()),
            presentation: None,
        },
    );
    app.push_output("early output".into());

    app.apply_tape_record(
        serde_json::from_str(
            r#"{"version":1,"kind":"message","role":"user","content":"race prompt","submission_id":"sub-race"}"#,
        )
        .unwrap(),
    );

    assert_eq!(app.transcript.len(), 3, "transcript: {:?}", app.transcript);
    assert!(
        app.transcript[0]
            .input_source()
            .is_some_and(|(body, ..)| body == "race prompt"),
        "authoritative user boundary must precede early tool/output cells: {:?}",
        app.transcript
    );
    assert!(matches!(app.transcript[1], HistoryCell::Tool { .. }));
    assert!(matches!(app.transcript[2], HistoryCell::Assistant(ref s) if s == "early output"));
}
