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

pub(super) const INPUT_ID: &str = "00000000-0000-4000-8000-000000000001";
pub(super) fn task(input: &str) -> StdioTaskWaitContext {
    let mut task = StdioTaskWaitContext::new(input).unwrap();
    task.record.submission_id = INPUT_ID.into();
    task
}

pub(super) fn correlated_records(records: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for line in records
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let mut record: serde_json::Value = serde_json::from_slice(line).unwrap();
        record["submission_id"] = INPUT_ID.into();
        serde_json::to_writer(&mut output, &record).unwrap();
        output.push(b'\n');
    }
    output
}

pub(super) fn completion(
    status: alan_agent_protocol::UiInputStatus,
    error: Option<&str>,
) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&UiEvent::InputCompleted {
        submission_ids: vec![INPUT_ID.into()],
        status,
        error: error.map(str::to_owned),
    })
    .unwrap();
    bytes.push(b'\n');
    bytes
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
    ui_tail: alan_shell::Tail,
) -> StdioTailAttachment {
    StdioTailAttachment {
        root_agent_pid: pid.parse().unwrap(),
        agent_process_path: format!("/agent/{pid}"),
        tape_tail,
        ui_tail,
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
        &StdioTaskWaitContext { submitted_at_ms: 2, ..task("same task") },
        br#"{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"old answer"}
{"submission_id":"00000000-0000-4000-8000-000000000001","version":1,"kind":"message","role":"user","content":"same task"}
{"submission_id":"00000000-0000-4000-8000-000000000001","version":1,"kind":"message","role":"assistant","content":"intermediate response"}
{"submission_id":"00000000-0000-4000-8000-000000000001","version":1,"kind":"message","role":"assistant","content":"current answer"}
"#,
        br#"{"type":"activity","snapshot":{"version":1,"state":"running","started_at_ms":1}}
{"type":"error","message":"previous provider failure","recoverable":true}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
{"type":"input_completed","submission_ids":["00000000-0000-4000-8000-000000000001"],"status":"completed"}
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
        &StdioTaskWaitContext { submitted_at_ms: 0, ..task("task with no tape record") },
        b"",
        br#"{"type":"activity","snapshot":{"version":1,"state":"running","started_at_ms":1}}
{"type":"input_completed","submission_ids":["00000000-0000-4000-8000-000000000001"],"status":"failed","error":"provider unavailable"}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
    )
    .unwrap();

    assert!(
        snapshot.task_started,
        "The correlated failure identifies this submitted task"
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
            submitted_at_ms: 20,
            ..task("same task")
        },
        baseline_tape,
        baseline_ui,
    )
    .unwrap();

    assert!(!snapshot.task_started);
    assert!(snapshot.assistant_answer.is_none());
    assert_eq!(snapshot.activity_state, Some(UiActivityState::Idle));
    assert!(!snapshot.waiting_for_response);
}

#[test]
fn recovery_rejects_a_reset_tape_with_only_an_old_identical_prompt() {
    let replacement_tape = br#"{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"old answer"}
"#;
    let snapshot = stdio_task_snapshot_from_history(
        &StdioTaskWaitContext {
            submitted_at_ms: 20,
            ..task("same task")
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
        file_surface::send_event_or_shutdown(
            &tx,
            &mut shutdown_rx,
            FileBackedEvent::ActionsChanged {
                agent_path: "/agent/1".to_string(),
                action_id: "a0".to_string(),
            },
        )
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
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut watchers = AgentWatchers::start(old_tails, "/agent/root", tx.clone());
    app.transcript
        .push(HistoryCell::User("current task".to_string()));
    app.reconciler.on_user_record();

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
    shell.write(
        "/agent/root/machine/tape",
        b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"current task\",\"submission_id\":\"other-client\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"other client answer\",\"submission_id\":\"other-client\"}\n",
    ).await.unwrap();
    let mut tape = correlated_records(
        b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"current task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"current answer\"}\n",
    );
    tape.extend_from_slice(
        b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"queued task\",\"submission_id\":\"second-input\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"queued answer\",\"submission_id\":\"second-input\"}\n",
    );
    shell
        .write("/agent/root/machine/tape", &tape)
        .await
        .unwrap();
    shell
        .write(
            "/agent/root/machine/ui/events",
            b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":10}}\n{\"type\":\"error\",\"message\":\"old provider failure\",\"recoverable\":true}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":30}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
        )
        .await
        .unwrap();

    shell
        .write(
            "/agent/root/machine/ui/events",
            &completion(alan_agent_protocol::UiInputStatus::Completed, None),
        )
        .await
        .unwrap();
    shell
        .write(
            "/agent/root/machine/ui/events",
            &serde_json::to_vec(&UiEvent::InputCompleted {
                submission_ids: vec!["second-input".into()],
                status: alan_agent_protocol::UiInputStatus::Completed,
                error: None,
            })
            .map(|mut bytes| {
                bytes.push(b'\n');
                bytes
            })
            .unwrap(),
        )
        .await
        .unwrap();

    let mut pending = VecDeque::from([
        PendingRootAgentTurn {
            input: "current task".into(),
            submission_id: INPUT_ID.into(),
            submitted_process: Some(_old_pid.parse().unwrap()),
            submitted_at_ms: 20,
        },
        PendingRootAgentTurn {
            input: "queued task".into(),
            submission_id: "second-input".into(),
            submitted_process: Some(_old_pid.parse().unwrap()),
            submitted_at_ms: 21,
        },
    ]);
    assert!(
        watchers
            .refresh_root_agent_attachment(
                &shell,
                "/agent/root",
                &mut app,
                &mut rx,
                &mut pending,
                &tx,
            )
            .await
    );
    assert!(pending.is_empty());
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("previous task".to_string()),
            HistoryCell::Assistant("previous answer".to_string()),
            HistoryCell::User("current task".to_string()),
            HistoryCell::Assistant("current answer".to_string()),
            HistoryCell::User("queued task".to_string()),
            HistoryCell::Assistant("queued answer".to_string()),
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
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut watchers = AgentWatchers::start(old_tails, "/agent/root", tx.clone());

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

    let mut pending = VecDeque::new();
    assert!(
        !watchers
            .refresh_root_agent_attachment(
                &shell,
                "/agent/root",
                &mut app,
                &mut rx,
                &mut pending,
                &tx,
            )
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
        .refresh_root_agent_attachment(&shell, "/agent/root", &mut app, &mut rx, &mut pending, &tx)
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
    app.transcript.clear();
    app.transcript
        .push(HistoryCell::User("same task".to_string()));
    app.reconciler.on_user_record();
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut watchers = AgentWatchers::start(old_tails, "/agent/root", tx.clone());

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
                &mut rx,
                &mut VecDeque::from([PendingRootAgentTurn {
                    input: "same task".into(),
                    submission_id: INPUT_ID.into(),
                    submitted_process: Some(_old_pid.parse().unwrap()),
                    submitted_at_ms: 20,
                }]),
                &tx,
            )
            .await
    );
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("same task".to_string()),
            HistoryCell::Error(
                "Root Agent changed without correlated completion evidence; outcome is unknown"
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
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript.push(crate::history::HistoryCell::User(
        "current task".to_string(),
    ));
    let mut watchers = AgentWatchers::start(old_tails, "/agent/root", tx.clone());

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

    shell
        .write(
            "/agent/root/machine/ui/events",
            &completion(
                alan_agent_protocol::UiInputStatus::Failed,
                Some("provider unavailable"),
            ),
        )
        .await
        .unwrap();

    assert!(
        watchers
            .refresh_root_agent_attachment(
                &shell,
                "/agent/root",
                &mut app,
                &mut rx,
                &mut VecDeque::from([PendingRootAgentTurn {
                    input: "current task".into(),
                    submission_id: INPUT_ID.into(),
                    submitted_process: Some(_old_pid.parse().unwrap()),
                    submitted_at_ms: 20,
                }]),
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
fn one_shot_result_waits_for_correlated_settlement() {
    let mut task = StdioTaskSnapshot {
        task_started: false,
        waiting_for_response: false,
        command_output: None,
        assistant_answer: Some("answer".to_string()),
        activity_state: Some(UiActivityState::Idle),
        task_error: None,
        completion: None,
    };

    assert!(finish_stdio_task_if_ready(&mut task).unwrap().is_none());
    task.task_started = true;
    task.activity_state = Some(UiActivityState::Running);
    assert!(finish_stdio_task_if_ready(&mut task).unwrap().is_none());
    task.activity_state = Some(UiActivityState::Idle);
    assert!(finish_stdio_task_if_ready(&mut task).unwrap().is_none());
    task.completion = Some(alan_agent_protocol::UiInputStatus::Completed);
    assert_eq!(
        finish_stdio_task_if_ready(&mut task)
            .unwrap()
            .map(StdioTaskOutput::agent_answer)
            .as_deref(),
        Some("answer")
    );

    let mut failed_task = StdioTaskSnapshot {
        task_started: true,
        waiting_for_response: false,
        command_output: None,
        assistant_answer: Some("intermediate response".to_string()),
        activity_state: Some(UiActivityState::Idle),
        task_error: Some("provider failed".to_string()),
        completion: Some(alan_agent_protocol::UiInputStatus::Failed),
    };
    let result = finish_stdio_task_if_ready(&mut failed_task);
    assert_eq!(
        result.unwrap_err().to_string(),
        "Agent task failed: provider failed"
    );
}

#[tokio::test(start_paused = true)]
async fn one_shot_waits_without_timeout_but_fails_when_its_tail_closes() {
    let (shell, agent_root, namespace, pid) = live_root_agent().await;
    let fault = Arc::new(FaultingFileServer::new(agent_root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(fault.clone()),
        Access::ReadWrite,
    );
    let mut attachment = open_stdio_tail_attachment(&shell, "/agent/root")
        .await
        .unwrap();
    let mut input = shell.tail("/agent/root/io/input").await.unwrap();
    let error = {
        let waiting = wait_for_stdio_answer(
            &shell,
            task("long task"),
            &mut attachment,
            std::future::pending(),
        );
        tokio::pin!(waiting);
        tokio::select! {
            result = &mut waiting => panic!("finished before submission: {result:?}"),
            bytes = input.read(4096) => assert!(!bytes.unwrap().is_empty()),
        }
        tokio::time::advance(std::time::Duration::from_secs(301)).await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut waiting)
                .await
                .is_err()
        );
        fault.close(pid.parse().unwrap());
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap_err()
    };
    assert!(error.to_string().contains("outcome is unknown"));
    assert_eq!(attachment.root_agent_pid, pid.parse::<u64>().unwrap());
    input.close().await.unwrap();
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
}

#[tokio::test]
async fn one_shot_fails_on_missing_or_changed_root_without_replacing_tails() {
    for (published, input) in [
        (0, "pinned task"),
        (999, "pinned task"),
        (0, "!printf x"),
        (999, "!printf x"),
    ] {
        let (shell, _agent_root, namespace, pid) = live_root_agent().await;
        let mut attachment = open_stdio_tail_attachment(&shell, "/agent/root")
            .await
            .unwrap();
        let task = task(input);
        submit_stdio_task(&shell, &task, &attachment).await.unwrap();
        namespace.replace_mount(
            PID_MOUNT,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                "pid",
                format!("{published}\n").into_bytes(),
            ))),
            Access::ReadOnly,
        );
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            wait_for_stdio_answer_after_submit(
                &shell,
                task,
                &mut attachment,
                std::future::pending(),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("outcome is unknown"));
        assert_eq!(attachment.root_agent_pid, pid.parse::<u64>().unwrap());
        close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn one_shot_returns_runtime_failure_without_a_tape_user_record() {
    let (shell, _agent_root, _live_namespace, pid) = live_root_agent().await;
    let (tape_tail, _) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let mut attachment = stdio_attachment(&pid, tape_tail, ui_tail);

    let result = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            task("fail before tape persistence"),
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
                &completion(
                    alan_agent_protocol::UiInputStatus::Failed,
                    Some("provider unavailable"),
                ),
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
    let (tape_tail, _) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let mut attachment = stdio_attachment(&pid, tape_tail, ui_tail);
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();

    let input = task("cancel this turn");
    let submission_id = input.record.submission_id.clone();
    let result = {
        let wait_for_answer = wait_for_stdio_answer(&shell, input, &mut attachment, async move {
            cancel_rx.await.map_err(anyhow::Error::from)
        });
        tokio::pin!(wait_for_answer);

        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot returned before input was observed: {result:?}"),
            input = input_tail.read(4096) => assert!(!input.unwrap().is_empty()),
        }
        cancel_tx.send(()).unwrap();

        wait_for_answer.await
    };

    assert_eq!(
        result.unwrap_err().to_string(),
        "Agent input cancellation requested"
    );
    let events = String::from_utf8(shell.cat("/agent/root/events").await.unwrap()).unwrap();
    assert!(
        events.contains(&format!("ctl:queue-v1 interrupt {submission_id}")),
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
