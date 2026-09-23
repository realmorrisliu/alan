use super::tail::tail_with_history;
use super::*;
mod faulting_agentfs;
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::ProcessEventSource;
use alan_kernel::{Access, LiveNamespace, MountFs, Namespace, ProcFs};
pub(crate) use faulting_agentfs::FaultingFileServer;
use std::sync::Arc;

pub(super) const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";
pub(super) const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;

#[test]
fn interactive_task_lock_is_shared_and_released_after_the_turn() {
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("task.lock");

    let interactive = acquire_task_submission_lock(&path).unwrap();
    let competing = acquire_task_submission_lock(&path).unwrap_err();
    assert!(
        competing
            .to_string()
            .contains("another Alan task is already running")
    );

    drop(interactive);
    assert!(acquire_task_submission_lock(&path).is_ok());
}

#[test]
fn root_agent_interrupt_waits_until_the_submitted_turn_is_accepted() {
    let mut pending = Some(PendingRootAgentTurn {
        input: "current task".to_string(),
        observed_active: false,
        interrupt_requested: false,
        submitted_at_ms: 20,
        prior_matching_turns: 0,
    });

    assert!(!request_pending_root_interrupt(&mut pending));
    assert!(!observe_root_agent_activity(
        &mut pending,
        UiActivityState::Idle
    ));
    assert!(pending.as_ref().unwrap().interrupt_requested);

    assert!(observe_root_agent_activity(
        &mut pending,
        UiActivityState::Running
    ));
    assert!(!pending.as_ref().unwrap().interrupt_requested);
    assert!(!observe_root_agent_activity(
        &mut pending,
        UiActivityState::Idle
    ));
    assert_eq!(pending, None);
}

#[test]
fn pending_root_agent_interrupt_is_discarded_if_task_settles_before_activation() {
    let mut pending = Some(PendingRootAgentTurn {
        input: "current task".to_string(),
        observed_active: false,
        interrupt_requested: false,
        submitted_at_ms: 20,
        prior_matching_turns: 0,
    });

    assert!(!request_pending_root_interrupt(&mut pending));
    pending.as_mut().unwrap().observed_active = true;
    assert!(!observe_root_agent_activity(
        &mut pending,
        UiActivityState::Idle
    ));
    assert_eq!(pending, None);
}

#[test]
fn only_a_plain_enter_submits_a_new_agent_task() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("do work");
    assert!(app.enter_submits_agent_task());

    app.composer.set_text("  ");
    assert!(!app.enter_submits_agent_task());
    app.composer.set_text("/help");
    assert!(!app.enter_submits_agent_task());
    app.composer.set_text("do work");
    app.completion = Some(crate::completion::CompletionState {
        kind: crate::completion::CompletionKind::Command,
        token_start: 0,
        query: String::new(),
        matches: Vec::new(),
        selected: 0,
    });
    assert!(!app.enter_submits_agent_task());
}

#[test]
fn actionless_slash_commands_never_become_agent_tasks() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.composer.set_text("/help");

    assert!(app.handle_submit().is_none());
    assert!(app.transcript.is_empty());
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|notice| notice.contains("/compact"))
    );

    app.transcript
        .push(HistoryCell::User("old transcript".to_string()));
    app.composer.set_text("/clear");
    assert!(app.handle_submit().is_none());
    assert!(app.transcript.is_empty());
}

pub(super) async fn live_root_agent() -> (alan_shell::Shell, Arc<AgentRootFs>, LiveNamespace, String)
{
    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn alan_ap::FileServer> = proc.clone();
    let proc_events: Arc<dyn ProcessEventSource> = proc.clone();
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        proc_server,
        proc_events,
    ));
    let mut namespace = Namespace::new();
    namespace.mount("/proc", InProcessTransport::new(proc), Access::ReadWrite);
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    let live_namespace = LiveNamespace::new(namespace);
    let root_transport = InProcessTransport::new(Arc::new(MountFs::from_live_namespace(
        live_namespace.clone(),
    )));
    let shell = alan_shell::Shell::new(root_transport);

    let pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );

    (shell, agent_root, live_namespace, pid)
}

fn stdio_attachment(
    pid: &str,
    tape_tail: alan_shell::Tail,
    tape_history: Vec<u8>,
    ui_tail: alan_shell::Tail,
) -> StdioTailAttachment {
    StdioTailAttachment {
        root_agent_pid: pid.parse().unwrap(),
        agent_process_path: format!("/agent/{pid}"),
        tape_tail,
        tape_history,
        ui_tail,
        ui_history: Vec::new(),
    }
}

#[test]
fn line_drain_keeps_partial_records_until_newline() {
    let mut pending = b"{\"role\":\"user\"}\n{\"role\"".to_vec();

    assert_eq!(
        drain_lines(&mut pending),
        vec![br#"{"role":"user"}"#.to_vec()]
    );
    assert_eq!(pending, b"{\"role\"");

    pending.extend_from_slice(b": \"assistant\"}\n");
    assert_eq!(
        drain_lines(&mut pending),
        vec![br#"{"role": "assistant"}"#.to_vec()]
    );
    assert!(pending.is_empty());
}

#[test]
fn snapshot_restores_the_latest_matching_turn_after_root_process_change() {
    let snapshot = stdio_task_snapshot_from_history(
        &StdioTaskWaitContext {
            input: "same task",
            baseline_tape_history: Vec::new(),
            submitted_at_ms: 2,
        },
        br#"{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"old answer"}
{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"intermediate response"}
{"version":1,"kind":"message","role":"assistant","content":"current answer"}
"#,
        br#"{"type":"activity","snapshot":{"version":1,"state":"running","started_at_ms":1}}
{"type":"error","message":"previous provider failure","recoverable":true}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
{"type":"activity","snapshot":{"version":1,"state":"running","started_at_ms":2}}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
    )
    .unwrap();

    assert!(snapshot.task_started);
    assert_eq!(snapshot.assistant_answer.as_deref(), Some("current answer"));
    assert_eq!(snapshot.activity_state, Some(UiActivityState::Idle));
    assert!(snapshot.task_error.is_none());
}

#[test]
fn one_shot_reports_failure_before_the_user_record_reaches_tape() {
    let mut snapshot = stdio_task_snapshot_from_history(
        &StdioTaskWaitContext {
            input: "task with no tape record",
            baseline_tape_history: Vec::new(),
            submitted_at_ms: 0,
        },
        b"",
        br#"{"type":"activity","snapshot":{"version":1,"state":"running","started_at_ms":1}}
{"type":"error","message":"provider unavailable","recoverable":true}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
    )
    .unwrap();

    assert!(
        snapshot.task_started,
        "Running establishes this submitted task"
    );
    assert_eq!(
        finish_stdio_task_if_ready(&mut snapshot)
            .unwrap_err()
            .to_string(),
        "Agent task failed: provider unavailable"
    );
}

#[test]
fn recovery_does_not_reuse_an_identical_prompt_from_the_baseline_tape() {
    let baseline_tape = br#"{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"old answer"}
"#;
    let baseline_ui =
        br#"{"type":"activity","snapshot":{"version":1,"state":"running","started_at_ms":10}}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#;
    let snapshot = stdio_task_snapshot_from_history(
        &StdioTaskWaitContext {
            input: "same task",
            baseline_tape_history: baseline_tape.to_vec(),
            submitted_at_ms: 20,
        },
        baseline_tape,
        baseline_ui,
    )
    .unwrap();

    assert!(!snapshot.task_started);
    assert!(snapshot.assistant_answer.is_none());
    assert_eq!(snapshot.activity_state, None);
}

#[test]
fn recovery_rejects_a_reset_tape_with_only_an_old_identical_prompt() {
    let baseline_tape = br#"{"version":1,"kind":"message","role":"user","content":"earlier task"}
{"version":1,"kind":"message","role":"assistant","content":"earlier answer"}
"#;
    let replacement_tape = br#"{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"old answer"}
"#;
    let snapshot = stdio_task_snapshot_from_history(
        &StdioTaskWaitContext {
            input: "same task",
            baseline_tape_history: baseline_tape.to_vec(),
            submitted_at_ms: 20,
        },
        replacement_tape,
        br#"{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
    )
    .unwrap();

    assert!(!snapshot.task_started);
    assert!(snapshot.assistant_answer.is_none());
}

#[test]
fn reattachment_does_not_treat_a_pre_submission_ui_error_as_current() {
    let task = file_surface::correlated_ui_task(
        br#"{"type":"activity","snapshot":{"version":1,"state":"running","started_at_ms":10}}
{"type":"error","message":"old provider failure","recoverable":true}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
        20,
    )
    .unwrap();

    assert!(!task.started);
    assert_eq!(task.state, None);
    assert_eq!(task.error, None);
}

#[test]
fn renderer_reconnect_does_not_reuse_an_older_identical_prompt() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript = vec![
        HistoryCell::User("same task".to_string()),
        HistoryCell::Assistant("old answer".to_string()),
        HistoryCell::User("same task".to_string()),
    ];
    let previous = app.transcript.clone();

    assert!(!app.merge_reconnected_history(
        vec![
            HistoryCell::User("same task".to_string()),
            HistoryCell::Assistant("old answer".to_string()),
        ],
        "same task",
        1,
    ));
    assert_eq!(app.transcript, previous);
}

#[tokio::test]
async fn full_watcher_queue_does_not_block_shutdown() {
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    tx.send(FileBackedEvent::RequestsChanged).await.unwrap();
    let (shutdown, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let send = tokio::spawn(async move {
        file_surface::send_event_or_shutdown(&tx, &mut shutdown_rx, FileBackedEvent::ActionsChanged)
            .await
    });

    tokio::task::yield_now().await;
    shutdown.send(true).unwrap();
    assert!(
        !tokio::time::timeout(std::time::Duration::from_secs(1), send)
            .await
            .expect("watcher send must unblock on shutdown")
            .unwrap()
    );
}

#[tokio::test]
async fn renderer_reconnect_hydrates_the_current_turn_and_keeps_prior_transcript() {
    let (shell, agent_root, live_namespace, _old_pid) = live_root_agent().await;
    shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"previous task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"previous answer\"}\n",
        )
        .await
        .unwrap();

    let mut app = FileBackedApp::new("/agent/root".to_string());
    let old_tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut watchers = AgentWatchers::start(old_tails, tx.clone());
    app.transcript
        .push(HistoryCell::User("current task".to_string()));
    app.reconciler.on_local_submit("current task");

    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{new_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"current task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"current answer\"}\n",
        )
        .await
        .unwrap();
    shell
        .write(
            "/agent/root/machine/ui/events",
            b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":10}}\n{\"type\":\"error\",\"message\":\"old provider failure\",\"recoverable\":true}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":30}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
        )
        .await
        .unwrap();

    assert!(
        watchers
            .refresh_root_agent_attachment(
                &shell,
                "/agent/root",
                &mut app,
                Some(("current task", 20, 0)),
                &tx,
            )
            .await
    );
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("previous task".to_string()),
            HistoryCell::Assistant("previous answer".to_string()),
            HistoryCell::User("current task".to_string()),
            HistoryCell::Assistant("current answer".to_string()),
        ]
    );

    watchers.stop().await;
}

#[tokio::test]
async fn renderer_reconnects_after_root_pid_changes_without_a_pending_turn() {
    let (shell, agent_root, live_namespace, _old_pid) = live_root_agent().await;
    shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"previous task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"previous answer\"}\n",
        )
        .await
        .unwrap();
    let mut app = FileBackedApp::new("/agent/root".to_string());
    let old_tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut watchers = AgentWatchers::start(old_tails, tx.clone());

    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{new_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"previous task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"previous answer\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"remote task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"remote answer\"}\n",
        )
        .await
        .unwrap();

    assert!(
        !watchers
            .refresh_root_agent_attachment(&shell, "/agent/root", &mut app, None, &tx)
            .await
    );
    assert_eq!(watchers.root_agent_pid, Some(new_pid.parse().unwrap()));
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("previous task".to_string()),
            HistoryCell::Assistant("previous answer".to_string()),
            HistoryCell::User("remote task".to_string()),
            HistoryCell::Assistant("remote answer".to_string()),
        ]
    );

    watchers.stop().await;
}

#[tokio::test]
async fn failed_root_reattach_preserves_state_and_retries_the_new_pid() {
    let (shell, agent_root, live_namespace, _old_pid) = live_root_agent().await;
    let old_tails = hydrate_and_open_tails(
        &shell,
        "/agent/root",
        &mut FileBackedApp::new("/agent/root".to_string()),
    )
    .await
    .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut watchers = AgentWatchers::start(old_tails, tx.clone());

    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript = vec![
        HistoryCell::User("prior task".to_string()),
        HistoryCell::Assistant("prior answer".to_string()),
    ];
    app.action_cells.insert("prior-action".to_string(), 1);
    app.activity = UiActivitySnapshot::running(10);
    app.composer.set_text("unsent draft");
    let original_transcript = app.transcript.clone();
    let original_activity = app.activity.clone();
    let original_actions = app.action_cells.clone();

    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{new_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    shell
        .write("/agent/root/machine/ui/events", b"not valid json\n")
        .await
        .unwrap();

    assert!(
        !watchers
            .refresh_root_agent_attachment(&shell, "/agent/root", &mut app, None, &tx)
            .await
    );
    assert_eq!(watchers.root_agent_pid, None);
    assert_eq!(
        app.transcript[..original_transcript.len()],
        original_transcript
    );
    assert_eq!(app.activity, original_activity);
    assert_eq!(app.action_cells, original_actions);
    assert_eq!(app.composer.text(), "unsent draft");
    assert!(watchers.pid_refresh_failed);

    let recovered_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(recovered_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(recovered_pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{recovered_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    watchers
        .refresh_root_agent_attachment(&shell, "/agent/root", &mut app, None, &tx)
        .await;
    assert_eq!(
        watchers.root_agent_pid,
        Some(recovered_pid.parse().unwrap())
    );
    assert!(!watchers.pid_refresh_failed);
    assert_eq!(
        app.transcript[..original_transcript.len()],
        original_transcript
    );
    assert!(matches!(
        app.transcript.last(),
        Some(HistoryCell::Error(message)) if message.starts_with("Root Agent reattach failed:")
    ));
    assert_eq!(app.composer.text(), "unsent draft");

    watchers.stop().await;
}

#[tokio::test]
async fn renderer_does_not_reuse_a_tape_turn_hidden_by_clear() {
    let (shell, agent_root, live_namespace, _old_pid) = live_root_agent().await;
    shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"same task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"old answer\"}\n",
        )
        .await
        .unwrap();

    let mut app = FileBackedApp::new("/agent/root".to_string());
    let old_tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let prior_matching_turns = app.tape_user_prompt_count("same task");
    app.transcript.clear();
    app.transcript
        .push(HistoryCell::User("same task".to_string()));
    app.reconciler.on_local_submit("same task");
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut watchers = AgentWatchers::start(old_tails, tx.clone());

    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{new_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"same task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"old answer\"}\n",
        )
        .await
        .unwrap();
    shell
        .write(
            "/agent/root/machine/ui/events",
            b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":30}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
        )
        .await
        .unwrap();

    assert!(
        watchers
            .refresh_root_agent_attachment(
                &shell,
                "/agent/root",
                &mut app,
                Some(("same task", 20, prior_matching_turns)),
                &tx,
            )
            .await
    );
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("same task".to_string()),
            HistoryCell::Error(
                "Root Agent changed before the submitted turn could be recovered; outcome is unknown"
                    .to_string(),
            ),
        ]
    );

    watchers.stop().await;
}

#[tokio::test]
async fn renderer_reattach_keeps_a_tape_less_terminal_error() {
    let (shell, agent_root, live_namespace, _old_pid) = live_root_agent().await;
    let old_tails = hydrate_and_open_tails(
        &shell,
        "/agent/root",
        &mut FileBackedApp::new("/agent/root".to_string()),
    )
    .await
    .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript.push(crate::history::HistoryCell::User(
        "current task".to_string(),
    ));
    let mut watchers = AgentWatchers::start(old_tails, tx.clone());

    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{new_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    shell
        .write(
            "/agent/root/machine/ui/events",
            b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":30}}\n{\"type\":\"error\",\"message\":\"provider unavailable\",\"recoverable\":true}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
        )
        .await
        .unwrap();

    assert!(
        watchers
            .refresh_root_agent_attachment(
                &shell,
                "/agent/root",
                &mut app,
                Some(("current task", 20, 0)),
                &tx,
            )
            .await
    );
    assert!(matches!(
        app.transcript.last(),
        Some(crate::history::HistoryCell::Error(message)) if message == "provider unavailable"
    ));

    watchers.stop().await;
}

#[test]
fn one_shot_result_waits_for_seen_task_and_idle_activity() {
    let mut task = StdioTaskSnapshot {
        task_started: false,
        assistant_answer: Some("answer".to_string()),
        activity_state: Some(UiActivityState::Idle),
        task_error: None,
    };

    assert_eq!(finish_stdio_task_if_ready(&mut task).unwrap(), None);
    task.task_started = true;
    task.activity_state = Some(UiActivityState::Running);
    assert_eq!(finish_stdio_task_if_ready(&mut task).unwrap(), None);
    task.activity_state = Some(UiActivityState::Idle);
    assert_eq!(
        finish_stdio_task_if_ready(&mut task).unwrap().as_deref(),
        Some("answer")
    );

    let mut failed_task = StdioTaskSnapshot {
        task_started: true,
        assistant_answer: Some("intermediate response".to_string()),
        activity_state: Some(UiActivityState::Idle),
        task_error: Some("provider failed".to_string()),
    };
    let result = finish_stdio_task_if_ready(&mut failed_task);
    assert_eq!(
        result.unwrap_err().to_string(),
        "Agent task failed: provider failed"
    );
}

#[tokio::test(start_paused = true)]
async fn one_shot_waits_without_timeout_and_rebinds_when_a_tail_closes_before_pid_poll() {
    let (shell, agent_root, live_namespace, old_pid) = live_root_agent().await;
    let tail_closer = Arc::new(FaultingFileServer::new(agent_root.clone()));
    live_namespace.replace_mount(
        "/agent",
        InProcessTransport::new(tail_closer.clone()),
        Access::ReadWrite,
    );

    let (tape_tail, baseline_tape_history) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut attachment = stdio_attachment(&old_pid, tape_tail, baseline_tape_history, ui_tail);
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let (input_seen_tx, input_seen_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let (closed_tx, closed_rx) = tokio::sync::oneshot::channel();
    let (clear_pid_tx, clear_pid_rx) = tokio::sync::oneshot::channel();
    let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
    let controller_shell = shell.clone();
    let controller_agent_root = agent_root.clone();
    let controller_namespace = live_namespace.clone();
    let controller_tail_closer = tail_closer.clone();
    let old_agent_pid = old_pid.parse::<u64>().unwrap();
    let controller = tokio::spawn(async move {
        assert!(!input_tail.read(4096).await.unwrap().is_empty());
        input_seen_tx.send(()).unwrap();
        release_rx.await.unwrap();
        controller_tail_closer.close(old_agent_pid);
        closed_tx.send(()).unwrap();
        clear_pid_rx.await.unwrap();
        controller_namespace.replace_mount(
            PID_MOUNT,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                "pid",
                b"0\n".to_vec(),
            ))),
            Access::ReadOnly,
        );
        resume_rx.await.unwrap();
        let new_pid = controller_shell.spawn(EXEC_SPEC).await.unwrap();
        controller_agent_root
            .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
            .await;
        controller_agent_root
            .set_root_process(new_pid.clone())
            .await;
        controller_namespace.replace_mount(
            PID_MOUNT,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                "pid",
                format!("{new_pid}\n").into_bytes(),
            ))),
            Access::ReadOnly,
        );
        controller_shell
            .write(
                "/agent/root/machine/tape",
                b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"rebind me\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"one answer\"}\n",
            )
            .await
            .unwrap();
        controller_shell
            .write(
                "/agent/root/machine/ui/events",
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
            )
            .await
            .unwrap();
        input_tail.close().await.unwrap();
    });

    let answer = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            "/agent/root",
            StdioTaskWaitContext::new("rebind me", std::mem::take(&mut attachment.tape_history)),
            &mut attachment,
            std::future::pending::<anyhow::Result<()>>(),
        );
        tokio::pin!(wait_for_answer);
        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot completed before its AgentFS result: {result:?}"),
            result = input_seen_rx => result.unwrap(),
        }

        tokio::time::advance(std::time::Duration::from_secs(301)).await;
        let still_waiting =
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut wait_for_answer).await;
        assert!(
            still_waiting.is_err(),
            "a valid one-shot task must not time out after five minutes"
        );

        release_tx.send(()).unwrap();
        closed_rx.await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut wait_for_answer)
                .await
                .is_err(),
            "one-shot must wait while the supervised Root Agent is restarting"
        );
        clear_pid_tx.send(()).unwrap();
        resume_tx.send(()).unwrap();
        tokio::time::advance(std::time::Duration::from_millis(250)).await;
        wait_for_answer.await.unwrap()
    };
    controller.await.unwrap();

    assert_eq!(answer, "one answer");
    let current_pid = current_root_agent_pid(&shell).await.unwrap().unwrap();
    assert_eq!(attachment.root_agent_pid, current_pid);
    assert_eq!(
        attachment.agent_process_path,
        format!("/agent/{current_pid}")
    );
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
}

#[tokio::test]
async fn one_shot_returns_runtime_failure_without_a_tape_user_record() {
    let (shell, _agent_root, _live_namespace, pid) = live_root_agent().await;
    let (tape_tail, baseline_tape_history) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let mut attachment = stdio_attachment(&pid, tape_tail, baseline_tape_history, ui_tail);

    let result = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            "/agent/root",
            StdioTaskWaitContext::new(
                "fail before tape persistence",
                std::mem::take(&mut attachment.tape_history),
            ),
            &mut attachment,
            std::future::pending::<anyhow::Result<()>>(),
        );
        tokio::pin!(wait_for_answer);
        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot returned before input was observed: {result:?}"),
            input = input_tail.read(4096) => assert!(!input.unwrap().is_empty()),
        }

        shell
            .write(
                "/agent/root/machine/ui/events",
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":18446744073709551615}}\n{\"type\":\"error\",\"message\":\"provider unavailable\",\"recoverable\":true}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
            )
            .await
            .unwrap();

        wait_for_answer.await
    };

    assert_eq!(
        result.unwrap_err().to_string(),
        "Agent task failed: provider unavailable"
    );
    let tape = shell.cat("/agent/root/machine/tape").await.unwrap();
    assert!(
        tape.is_empty(),
        "this failure path must not depend on tape data"
    );
    let process_status =
        String::from_utf8(shell.cat(&format!("/proc/{pid}/status")).await.unwrap()).unwrap();
    assert_eq!(process_status.trim(), "running");

    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
    input_tail.close().await.unwrap();
}

#[tokio::test]
async fn one_shot_cancellation_interrupts_before_running_is_observed() {
    let (shell, _agent_root, _live_namespace, pid) = live_root_agent().await;
    let (tape_tail, baseline_tape_history) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let mut attachment = stdio_attachment(&pid, tape_tail, baseline_tape_history, ui_tail);
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();

    let result = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            "/agent/root",
            StdioTaskWaitContext::new(
                "cancel this turn",
                std::mem::take(&mut attachment.tape_history),
            ),
            &mut attachment,
            async move { cancel_rx.await.map_err(anyhow::Error::from) },
        );
        tokio::pin!(wait_for_answer);

        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot returned before input was observed: {result:?}"),
            input = input_tail.read(4096) => assert!(!input.unwrap().is_empty()),
        }
        cancel_tx.send(()).unwrap();

        tokio::time::timeout(std::time::Duration::from_millis(10), &mut wait_for_answer)
            .await
            .expect_err("cancellation waits until the task is accepted");
        let events = shell.cat("/agent/root/events").await.unwrap();
        assert!(
            !String::from_utf8_lossy(&events).contains("ctl:interrupt"),
            "do not let an idle Runtime consume the interrupt before input"
        );

        shell
            .write(
                "/agent/root/machine/ui/events",
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\"}}\n",
            )
            .await
            .unwrap();
        wait_for_answer.await
    };

    assert_eq!(result.unwrap_err().to_string(), "Agent task interrupted");
    let events = String::from_utf8(shell.cat("/agent/root/events").await.unwrap()).unwrap();
    assert!(
        events.contains("ctl:interrupt"),
        "one-shot cancellation must target the Agent Machine: {events:?}"
    );
    let process_status =
        String::from_utf8(shell.cat(&format!("/proc/{pid}/status")).await.unwrap()).unwrap();
    assert_eq!(process_status.trim(), "running");

    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
    input_tail.close().await.unwrap();
}
