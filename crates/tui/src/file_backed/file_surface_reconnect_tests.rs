use super::*;
use crate::file_backed::stdio_tests::{EXEC_SPEC, FaultingFileServer, PID_MOUNT, live_root_agent};
use crate::history::ToolStatus;
use alan_agentfs::AgentFs;
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::Access;
use std::sync::Arc;

#[tokio::test]
async fn superseded_attachment_events_are_dropped_but_terminal_input_survives() {
    use crate::file_backed::app::FileBackedEvent;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    tx.send(FileBackedEvent::Output("stale output".to_string()))
        .await
        .unwrap();
    tx.send(FileBackedEvent::Terminal(Event::Key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE,
    ))))
    .await
    .unwrap();
    tx.send(FileBackedEvent::Error("stale watcher error".to_string()))
        .await
        .unwrap();
    tx.send(FileBackedEvent::TerminalError(
        "terminal input error".to_string(),
    ))
    .await
    .unwrap();

    let mut pending_terminal_events = std::collections::VecDeque::new();
    crate::file_backed::discard_superseded_attachment_events(&mut rx, &mut pending_terminal_events);

    assert!(matches!(
        pending_terminal_events.pop_front(),
        Some(FileBackedEvent::Terminal(_))
    ));
    assert!(matches!(
        pending_terminal_events.pop_front(),
        Some(FileBackedEvent::TerminalError(message)) if message == "terminal input error"
    ));
    assert!(pending_terminal_events.is_empty());
    assert!(rx.try_recv().is_err());
}

#[test]
fn hydration_omits_unplaced_completed_actions_but_keeps_live_tool_updates() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript = vec![
        HistoryCell::User("first task".to_string()),
        HistoryCell::Assistant("first answer".to_string()),
        HistoryCell::User("second task".to_string()),
        HistoryCell::Assistant("second answer".to_string()),
    ];
    let action = |id: &str, name: &str, status: &str, output: &str| super::super::ActionSnapshot {
        id: id.to_string(),
        name: name.to_string(),
        status: status.to_string(),
        output: output.to_string(),
        result: String::new(),
    };

    super::super::hydrate_actions_from_snapshots(
        &mut app,
        vec![
            action("a0", "first tool", "completed", "first result"),
            action("a1", "second tool", "completed", "second result"),
            action("a2", "active tool", "running", ""),
        ],
    );

    assert_eq!(app.transcript.len(), 4);
    assert_eq!(
        app.running_tools,
        vec![crate::history::RunningTool {
            id: "a2".to_string(),
            title: "active tool".to_string(),
        }]
    );

    super::super::sync_action_snapshot(
        &mut app,
        action("a2", "active tool", "completed", "live result"),
    );
    assert!(app.running_tools.is_empty());
    assert!(matches!(
        app.transcript.last(),
        Some(HistoryCell::Tool {
            title,
            status: ToolStatus::Complete,
            ..
        }) if title == "active tool"
    ));
}

#[test]
fn submitted_turn_reconnect_reconciles_its_streamed_assistant_preview() {
    for (preview, recovered, expected) in [
        ("same answer", "same answer", "same answer"),
        (
            "streamed prefix",
            "streamed prefix completed",
            "streamed prefix completed",
        ),
        (
            "streamed answer completed",
            "streamed answer",
            "streamed answer completed",
        ),
    ] {
        let mut app = FileBackedApp::new("/agent/root".to_string());
        app.transcript = vec![
            HistoryCell::User("previous task".to_string()),
            HistoryCell::Assistant("previous answer".to_string()),
            HistoryCell::User("current task".to_string()),
            HistoryCell::Assistant(preview.to_string()),
        ];
        let tool = HistoryCell::Tool {
            title: "after answer".to_string(),
            status: ToolStatus::Complete,
            preview: None,
            presentation: None,
        };
        app.action_cells.insert("current-tool".to_string(), 4);
        let current = vec![
            HistoryCell::User("previous task".to_string()),
            HistoryCell::Assistant("previous answer".to_string()),
            HistoryCell::User("current task".to_string()),
            HistoryCell::Assistant(recovered.to_string()),
            tool.clone(),
        ];

        assert!(app.merge_reconnected_history(current, "current task", 0));
        assert_eq!(
            app.transcript,
            vec![
                HistoryCell::User("previous task".to_string()),
                HistoryCell::Assistant("previous answer".to_string()),
                HistoryCell::User("current task".to_string()),
                HistoryCell::Assistant(expected.to_string()),
                tool,
            ]
        );
        assert_eq!(app.action_cells.get("current-tool"), Some(&4));
    }
}

#[test]
fn action_event_ids_are_parsed_across_partial_records() {
    let mut pending = b"a0:status\ncreated:a1\na1:output\na1:sta".to_vec();
    assert_eq!(
        super::super::action_ids_from_events(&mut pending),
        ["a0", "a1"]
    );
    assert_eq!(pending, b"a1:sta");

    pending.extend_from_slice(b"tus\n");
    assert_eq!(super::super::action_ids_from_events(&mut pending), ["a1"]);
    assert!(pending.is_empty());
}

#[test]
fn reattached_action_indices_follow_removed_error_cells() {
    let mut reattached = FileBackedApp::new("/agent/root".to_string());
    reattached.transcript = vec![
        HistoryCell::User("previous task".to_string()),
        HistoryCell::Assistant("previous answer".to_string()),
        HistoryCell::User("current task".to_string()),
    ];
    let previous_transcript = std::mem::take(&mut reattached.transcript);
    reattached.reset_for_root_process_change();

    reattached.transcript = vec![
        HistoryCell::User("current task".to_string()),
        HistoryCell::Error("recoverable provider failure".to_string()),
        HistoryCell::Tool {
            title: "bash".to_string(),
            status: ToolStatus::Complete,
            preview: Some("first result".to_string()),
            presentation: None,
        },
        HistoryCell::User("later task".to_string()),
        HistoryCell::Assistant("current answer".to_string()),
    ];
    reattached.action_cells.insert("action-1".to_string(), 2);
    reattached
        .tape_user_cells
        .insert("current-submission".to_string(), 0);
    reattached
        .tape_user_cells
        .insert("later-submission".to_string(), 3);

    let current_transcript = std::mem::take(&mut reattached.transcript);
    reattached.transcript = previous_transcript;
    let current_transcript =
        remove_error_cells_and_remap_indices(current_transcript, &mut reattached);

    assert!(reattached.merge_reconnected_history(current_transcript, "current task", 0));
    assert_eq!(reattached.action_cells.get("action-1"), Some(&3));
    assert_eq!(
        reattached.tape_user_cells.get("current-submission"),
        Some(&2)
    );
    assert_eq!(reattached.tape_user_cells.get("later-submission"), Some(&4));
    assert!(reattached.classify_command_submission("current-submission"));
    assert!(reattached.classify_command_submission("later-submission"));
    assert!(matches!(
        reattached.transcript.get(2),
        Some(HistoryCell::Command(text)) if text == "current task"
    ));
    assert!(matches!(
        reattached.transcript.get(4),
        Some(HistoryCell::Command(text)) if text == "later task"
    ));

    reattached.upsert_action_cell(
        "action-1".to_string(),
        HistoryCell::Tool {
            title: "bash".to_string(),
            status: ToolStatus::Failed,
            preview: Some("updated result".to_string()),
            presentation: None,
        },
    );
    assert!(matches!(
        reattached.transcript.get(3),
        Some(HistoryCell::Tool {
            status: ToolStatus::Failed,
            ..
        })
    ));
    assert_eq!(
        reattached.transcript.last(),
        Some(&HistoryCell::Assistant("current answer".to_string()))
    );
}

#[test]
fn reattached_pending_tape_indices_follow_both_idle_merge_strategies() {
    let tool = || HistoryCell::Tool {
        title: "old tool".to_string(),
        status: ToolStatus::Complete,
        preview: None,
        presentation: None,
    };
    let cases = [
        (
            vec![
                HistoryCell::User("before".to_string()),
                HistoryCell::Assistant("before answer".to_string()),
                tool(),
                HistoryCell::User("pending".to_string()),
            ],
            vec![tool(), HistoryCell::User("pending".to_string())],
            1,
            3,
        ),
        (
            vec![
                HistoryCell::User("prefix".to_string()),
                HistoryCell::Assistant("old answer".to_string()),
                tool(),
            ],
            vec![
                HistoryCell::User("prefix".to_string()),
                HistoryCell::User("pending".to_string()),
            ],
            1,
            3,
        ),
    ];

    let outcomes = cases
        .into_iter()
        .map(|(previous, current, source_index, expected_index)| {
            let mut app = FileBackedApp::new("/agent/root".to_string());
            app.transcript = previous;
            app.tape_user_cells
                .insert("submission".to_string(), source_index);

            app.merge_reconnected_idle_history(current);

            let mapped_index = app.tape_user_cells.get("submission").copied();
            let classified = app.classify_command_submission("submission");
            let command_cell = matches!(
                app.transcript.get(expected_index),
                Some(HistoryCell::Command(text)) if text == "pending"
            );
            (mapped_index, classified, command_cell)
        })
        .collect::<Vec<_>>();

    assert_eq!(outcomes, [(Some(3), true, true), (Some(3), true, true)]);
}

#[tokio::test]
async fn renderer_hydration_retries_all_streams_after_root_pid_changes() {
    let (shell, agent_root, namespace, old_pid) = live_root_agent().await;
    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;

    shell
        .write(
            &format!("/agent/{new_pid}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"new task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"new answer\"}\n",
        )
        .await
        .unwrap();

    let interceptor = Arc::new(FaultingFileServer::new(agent_root.clone()));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(interceptor.clone()),
        Access::ReadWrite,
    );
    let (walked, resume) = interceptor.pause_next_walk_with_suffix("/actions/events");
    let attach_shell = shell.clone();
    let attach = tokio::spawn(async move {
        let mut app = FileBackedApp::new("/agent/root".to_string());
        let tails = hydrate_and_open_tails(&attach_shell, "/agent/root", &mut app).await?;
        anyhow::Ok((app, tails))
    });

    tokio::time::timeout(std::time::Duration::from_secs(2), walked)
        .await
        .unwrap()
        .unwrap();
    agent_root.set_root_process(new_pid.clone()).await;
    namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{new_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    resume.send(()).unwrap();

    let (app, mut tails) = attach.await.unwrap().unwrap();
    assert_eq!(tails.root_agent_pid, Some(new_pid.parse().unwrap()));
    assert!(
        app.transcript
            .contains(&HistoryCell::Assistant("new answer".to_string()))
    );

    assert_eq!(
        create_request(&agent_root, &old_pid, Fid(50_001)).await,
        "r0"
    );
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            tails.requests.read(1024)
        )
        .await
        .is_err(),
        "renderer must not retain a request tail from the previous PID"
    );
    assert_eq!(
        create_request(&agent_root, &new_pid, Fid(50_002)).await,
        "r0"
    );
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            tails.requests.read(1024)
        )
        .await
        .unwrap()
        .unwrap(),
        b"created:r0\n"
    );

    tails.requests.close().await.unwrap();
    tails.actions.close().await.unwrap();
    tails.ui.close().await.unwrap();
    tails.tape.close().await.unwrap();
    tails.output.close().await.unwrap();
}

#[tokio::test]
async fn renderer_hydration_waits_for_a_stale_published_root_pid_to_change() {
    let (shell, agent_root, namespace, old_pid) = live_root_agent().await;
    assert!(agent_root.unbind_process(&old_pid).await);

    let attach_shell = shell.clone();
    let mut attach = tokio::spawn(async move {
        let mut app = FileBackedApp::new("/agent/root".to_string());
        let tails = hydrate_and_open_tails(&attach_shell, "/agent/root", &mut app).await?;
        anyhow::Ok((app, tails))
    });
    tokio::select! {
        _ = &mut attach => panic!("renderer returned while the old PID was still published"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {}
    }

    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{new_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );

    let (_, tails) = tokio::time::timeout(std::time::Duration::from_secs(2), &mut attach)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(tails.root_agent_pid, Some(new_pid.parse().unwrap()));
    tails.requests.close().await.unwrap();
    tails.actions.close().await.unwrap();
    tails.ui.close().await.unwrap();
    tails.tape.close().await.unwrap();
    tails.output.close().await.unwrap();
}

#[tokio::test]
async fn hydration_does_not_append_a_historical_error_after_later_tape_turns() {
    let (shell, _agent_root, _namespace, _pid) = live_root_agent().await;
    shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"first task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"first answer\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"later task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"later answer\"}\n",
        )
        .await
        .unwrap();
    shell
        .write(
            "/agent/root/machine/ui/events",
            b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":1}}\n{\"type\":\"error\",\"message\":\"earlier failure\",\"recoverable\":true}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":2}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
        )
        .await
        .unwrap();

    let mut app = FileBackedApp::new("/agent/root".to_string());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();

    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("first task".to_string()),
            HistoryCell::Assistant("first answer".to_string()),
            HistoryCell::User("later task".to_string()),
            HistoryCell::Assistant("later answer".to_string()),
        ]
    );
    tails.requests.close().await.unwrap();
    tails.actions.close().await.unwrap();
    tails.ui.close().await.unwrap();
    tails.tape.close().await.unwrap();
    tails.output.close().await.unwrap();
}

async fn create_request(agent_root: &alan_agentfs::AgentRootFs, pid: &str, fid: Fid) -> String {
    agent_root
        .walk(
            Fid::ROOT,
            fid,
            &[pid.to_string(), "requests".to_string(), "clone".to_string()],
        )
        .await
        .unwrap();
    agent_root.open(fid, OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(agent_root.read(fid, 0, 64).await.unwrap()).unwrap();
    agent_root.clunk(fid).await.unwrap();
    id
}
