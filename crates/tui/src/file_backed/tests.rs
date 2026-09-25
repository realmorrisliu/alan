use super::file_surface::write_interrupt;
use super::*;
use std::sync::Arc;

use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::{FileServer, ProcessEventSource};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn press(
    app: &mut FileBackedApp,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Option<FileBackedAction> {
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Key(
        KeyEvent::new(code, modifiers),
    )))
}

fn render(app: &FileBackedApp) -> TestBackend {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    terminal.backend().clone()
}

#[test]
fn parse_tape_history_restores_user_and_assistant_messages() {
    let cells = parse_tape_history(
        r#"{"version":1,"kind":"message","role":"user","content":"hello"}
{"version":1,"kind":"message","role":"assistant","content":"world"}
"#,
    );
    assert_eq!(
        cells,
        vec![
            HistoryCell::User("hello".to_string()),
            HistoryCell::Assistant("world".to_string()),
        ]
    );
}

#[test]
fn root_process_reattach_preserves_prior_transcript_and_adds_current_turn() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.transcript = vec![
        HistoryCell::User("previous task".to_string()),
        HistoryCell::Assistant("previous answer".to_string()),
        HistoryCell::User("current task".to_string()),
    ];
    app.action_cells.insert("a-current".to_string(), 1);

    let attached = app.merge_reconnected_history(
        vec![
            HistoryCell::User("current task".to_string()),
            HistoryCell::Tool {
                title: "bash".to_string(),
                status: ToolStatus::Complete,
                preview: Some("read complete".to_string()),
                presentation: None,
            },
            HistoryCell::Assistant("current answer".to_string()),
        ],
        "current task",
        0,
    );

    assert!(attached);
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("previous task".to_string()),
            HistoryCell::Assistant("previous answer".to_string()),
            HistoryCell::User("current task".to_string()),
            HistoryCell::Tool {
                title: "bash".to_string(),
                status: ToolStatus::Complete,
                preview: Some("read complete".to_string()),
                presentation: None,
            },
            HistoryCell::Assistant("current answer".to_string()),
        ]
    );
    assert_eq!(app.action_cells.get("a-current"), Some(&3));
}

#[test]
fn root_agent_file_paths_pin_to_a_process_id() {
    assert_eq!(
        super::tail::root_agent_path_for_pid("/agent/root/machine/tape", 42),
        Some("/agent/42/machine/tape".to_string())
    );
    assert_eq!(
        super::tail::root_agent_path_for_pid("/agent/root", 42),
        Some("/agent/42".to_string())
    );
    assert_eq!(
        super::tail::root_agent_path_for_pid("/agent/rooted/machine/tape", 42),
        None
    );
}

#[test]
fn root_agent_pid_polling_requires_an_active_to_idle_transition() {
    let mut pending = Some(PendingRootAgentTurn {
        input: "current task".to_string(),
        observed_active: false,
        interrupt_requested: false,
        submitted_at_ms: 20,
        prior_matching_turns: 0,
    });
    observe_root_agent_activity(&mut pending, UiActivityState::Idle);
    assert_eq!(
        pending,
        Some(PendingRootAgentTurn {
            input: "current task".to_string(),
            observed_active: false,
            interrupt_requested: false,
            submitted_at_ms: 20,
            prior_matching_turns: 0,
        }),
        "streamed assistant output is not proof that the turn completed"
    );

    observe_root_agent_activity(&mut pending, UiActivityState::Running);
    assert_eq!(
        pending,
        Some(PendingRootAgentTurn {
            input: "current task".to_string(),
            observed_active: true,
            interrupt_requested: false,
            submitted_at_ms: 20,
            prior_matching_turns: 0,
        })
    );

    observe_root_agent_activity(&mut pending, UiActivityState::Idle);

    assert_eq!(pending, None);
}

#[test]
fn request_snapshot_maps_confirmation_payload() {
    let pending = request_snapshot_to_pending_yield(RequestSnapshot {
        id: "r1".to_string(),
        kind: "confirmation".to_string(),
        prompt: "Approve?".to_string(),
        options: serde_json::json!({
            "options": ["approve", "reject"],
            "details": {
                "capability": "write",
                "policy": { "reason": "needs approval" }
            }
        })
        .to_string(),
        status: "pending".to_string(),
    })
    .unwrap();

    assert_eq!(pending.kind, YieldKind::Confirmation);
    assert_eq!(pending.options, vec!["approve", "reject"]);
    assert_eq!(pending.capability.as_deref(), Some("write"));
    assert_eq!(pending.reason.as_deref(), Some("needs approval"));
}

#[test]
fn slash_opens_command_completion_and_tab_accepts() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('o'), KeyModifiers::NONE);
    let state = app.completion.as_ref().expect("command completion open");
    assert_eq!(state.matches[0].value, "compact");
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.composer.text(), "/compact ");
    assert!(app.completion.is_none());
}

#[test]
fn esc_interrupts_during_turn_even_with_completion_open() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.apply_ui_activity_snapshot(UiActivitySnapshot::running(1));
    press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
    assert!(app.completion.is_some());

    let action = press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(matches!(action, Some(FileBackedAction::Interrupt)));
    assert!(
        app.completion.is_some(),
        "interrupt should not dismiss popup first"
    );
}

#[test]
fn ctrl_c_interrupts_during_turn_even_with_completion_open() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.apply_ui_activity_snapshot(UiActivitySnapshot::running(1));
    press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
    assert!(app.completion.is_some());

    let action = press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

    assert!(matches!(action, Some(FileBackedAction::Interrupt)));
    assert!(
        app.completion.is_some(),
        "interrupt should not dismiss popup first"
    );
}

#[test]
fn ctrl_c_interrupts_instead_of_entering_a_structured_input_form() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.activity = UiActivitySnapshot::paused(Some(1));
    app.set_pending_yield(PendingYieldCell {
        request_id: "r1".to_string(),
        kind: YieldKind::StructuredInput,
        title: "Answer these questions".to_string(),
        prompt: None,
        options: Vec::new(),
        default_option: None,
        questions: ["first", "second"]
            .into_iter()
            .map(|id| alan_agent_protocol::StructuredInputQuestion {
                id: id.to_string(),
                label: id.to_string(),
                prompt: format!("{id} answer"),
                kind: alan_agent_protocol::StructuredInputKind::Text,
                required: false,
                placeholder: None,
                help_text: None,
                default_value: None,
                default_values: Vec::new(),
                min_selected: None,
                max_selected: None,
                options: Vec::new(),
                presentation_hints: Vec::new(),
            })
            .collect(),
        capability: None,
        reason: None,
        presentation: None,
    });
    let form = app.form.as_ref().expect("multi-question form");
    let initial_value = form.fields[form.focus].value.clone();

    let action = press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

    assert!(matches!(action, Some(FileBackedAction::Interrupt)));
    assert_eq!(
        app.form.as_ref().unwrap().fields[0].value,
        initial_value,
        "Ctrl-C must not be inserted as form text"
    );
}

#[tokio::test]
async fn terminal_reader_exits_after_its_event_receiver_is_dropped() {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx);

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        super::file_surface::spawn_terminal_events(tx),
    )
    .await
    .expect("terminal reader should observe shutdown")
    .expect("terminal reader task should exit cleanly");
}

#[test]
fn ctrl_r_toggles_thinking_expansion() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.apply_ui_thinking_snapshot(UiThinkingSnapshot::complete(
        "step one\nstep two".to_string(),
        3,
    ));

    let collapsed = app.rendered_history_lines(80).join("\n");
    assert!(collapsed.contains("ctrl+r to expand"));
    assert!(!app.expand_thinking);

    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);

    assert!(app.expand_thinking);
    let expanded = app.rendered_history_lines(80).join("\n");
    assert!(expanded.contains("step one"));
    assert!(!expanded.contains("ctrl+r to expand"));
}

#[test]
fn dollar_skill_completion_uses_local_candidates() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.set_skill_candidates(vec![CompletionCandidate::new("code-review", None)]);
    app.composer.set_text("use $co");
    app.refresh_completion();
    let state = app.completion.as_ref().expect("skill completion open");
    assert_eq!(state.matches[0].value, "code-review");
}

#[test]
fn action_snapshots_track_running_and_commit_completed_tool() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a0".to_string(),
            name: "edit".to_string(),
            status: "running".to_string(),
            output: String::new(),
            result: String::new(),
        },
    );
    assert_eq!(
        app.running_tools,
        vec![RunningTool {
            id: "a0".to_string(),
            title: "edit".to_string(),
        }]
    );
    assert!(app.transcript.is_empty());

    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a0".to_string(),
            name: "edit".to_string(),
            status: "completed".to_string(),
            output: "updated file".to_string(),
            result: r#"{"exit_code":0}"#.to_string(),
        },
    );
    assert!(app.running_tools.is_empty());
    assert_eq!(
        app.transcript,
        vec![HistoryCell::Tool {
            title: "edit".to_string(),
            status: ToolStatus::Complete,
            preview: None,
            presentation: Some(ToolResultPresentation::PlainText {
                body: "updated file".to_string(),
            }),
        }]
    );
}

#[test]
fn ui_plan_event_appends_plan_cell() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.apply_ui_event(UiEvent::Plan {
        snapshot: UiPlanSnapshot::new(
            Some("ship parity".to_string()),
            vec![alan_agent_protocol::PlanItem {
                id: "1".to_string(),
                content: "wire ui files".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        ),
    });

    assert!(matches!(app.transcript.last(), Some(HistoryCell::Plan(items)) if items.len() == 1));
}

#[test]
fn completed_ui_thinking_snapshot_appends_once() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    let snapshot = UiThinkingSnapshot::complete("reasoning".to_string(), 3);
    app.apply_ui_event(UiEvent::Thinking {
        snapshot: snapshot.clone(),
    });
    app.apply_ui_event(UiEvent::Thinking { snapshot });

    let thinking_cells = app
        .transcript
        .iter()
        .filter(|cell| matches!(cell, HistoryCell::Thinking { .. }))
        .count();
    assert_eq!(thinking_cells, 1);
}

#[test]
fn paused_activity_prefers_waiting_label_and_notice_none_clears() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.apply_ui_event(UiEvent::Notice {
        snapshot: UiNoticeSnapshot::new(UiNoticeKind::Warning, "retrying"),
    });
    assert_eq!(app.notice.as_deref(), Some("retrying"));

    app.apply_ui_event(UiEvent::Activity {
        snapshot: UiActivitySnapshot::paused(Some(1)),
    });
    assert_eq!(app.activity_label(), Some("waiting for input"));

    app.apply_ui_event(UiEvent::Notice {
        snapshot: UiNoticeSnapshot::none(),
    });
    assert!(app.notice.is_none());
}

#[test]
fn transcript_renders_error_style() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.transcript.push(HistoryCell::Error("boom".to_string()));

    let backend = render(&app);
    let cell = backend.buffer().cell((0, 0)).unwrap();

    assert_eq!(cell.symbol(), "e");
    assert_eq!(cell.fg, Color::Red);
}

#[test]
fn recoverable_error_is_kept_in_the_transcript() {
    let mut app = FileBackedApp::new("/agent/1".to_string());

    app.apply_ui_event(UiEvent::Error {
        message: "provider request failed".to_string(),
        recoverable: true,
    });

    assert_eq!(
        app.transcript,
        vec![HistoryCell::Error("provider request failed".to_string())]
    );
    assert!(
        app.notice.is_none(),
        "the transcript is the single visible error"
    );
    assert!(app.rendered_history_lines(80)[0].contains("provider request failed"));
}

#[test]
fn compact_command_routes_to_machine_ctl() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.composer.set_text("/compact");
    let action = app.handle_submit();
    match action {
        Some(FileBackedAction::MachineCtl { command, .. }) => {
            assert_eq!(command, "compact");
        }
        other => panic!("expected machine ctl action, got {other:?}"),
    }
}

#[tokio::test]
async fn write_agent_input_targets_agent_surface() {
    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = proc.clone();
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
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root);
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    let agent_path = format!("/agent/{pid}");

    write_agent_input(&shell, &agent_path, "hello through files")
        .await
        .unwrap();

    let echoed =
        String::from_utf8(shell.cat(&format!("{agent_path}/io/input")).await.unwrap()).unwrap();
    assert_eq!(echoed, "19\nhello through files");

    // Esc interrupts through the agent-runtime surface (machine/ctl), not
    // kernel process lifecycle: /proc/<pid>/ctl interrupt would terminate
    // the agent process while the runtime keeps generating.
    write_interrupt(&shell, &agent_path).await.unwrap();
    let events =
        String::from_utf8(shell.cat(&format!("{agent_path}/events")).await.unwrap()).unwrap();
    assert!(
        events.contains("ctl:interrupt"),
        "interrupt must be recorded on machine/ctl: {events:?}"
    );
    let proc_status =
        String::from_utf8(shell.cat(&format!("/proc/{pid}/status")).await.unwrap()).unwrap();
    assert_eq!(
        proc_status.trim(),
        "running",
        "interrupt must not terminate the agent process"
    );
}

#[test]
fn pending_yield_cell_updates_in_place_when_fields_arrive() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    // The request watcher can observe `created:r1` before the runtime has
    // written kind/prompt/options, so the first sync inserts a sparse cell.
    let sparse = PendingYieldCell {
        request_id: "r1".to_string(),
        kind: YieldKind::Confirmation,
        title: String::new(),
        prompt: None,
        options: Vec::new(),
        default_option: None,
        questions: Vec::new(),
        capability: None,
        reason: None,
        presentation: None,
    };
    app.set_pending_yield(sparse);

    let populated = PendingYieldCell {
        title: "Approve tool".to_string(),
        prompt: Some("Run `ls`?".to_string()),
        options: vec!["yes".to_string(), "no".to_string()],
        default_option: Some("no".to_string()),
        ..app.pending_yield.clone().unwrap()
    };
    app.set_pending_yield(populated.clone());

    let cells: Vec<_> = app
        .transcript
        .iter()
        .filter_map(|cell| match cell {
            HistoryCell::PendingYield(pending) => Some(pending),
            _ => None,
        })
        .collect();
    assert_eq!(cells.len(), 1, "later sync must update the cell in place");
    assert_eq!(cells[0], &populated);
}

#[test]
fn post_yield_cells_do_not_arm_remote_boundary_insertion() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "run this".to_string(),
        submission_id: None,
    });
    app.set_pending_yield(PendingYieldCell {
        request_id: "r1".to_string(),
        kind: YieldKind::Confirmation,
        title: "Approve?".to_string(),
        prompt: None,
        options: vec!["yes".to_string(), "no".to_string()],
        default_option: None,
        questions: Vec::new(),
        capability: None,
        reason: None,
        presentation: None,
    });
    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a1".to_string(),
            name: "tool".to_string(),
            status: "completed".to_string(),
            output: "ran".to_string(),
            result: r#"{"exit_code":0}"#.to_string(),
        },
    );

    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "next remote turn".to_string(),
        submission_id: None,
    });

    assert!(matches!(app.transcript[0], HistoryCell::User(ref text) if text == "run this"));
    assert!(matches!(app.transcript[1], HistoryCell::PendingYield(_)));
    assert!(matches!(app.transcript[2], HistoryCell::Tool { .. }));
    assert!(matches!(app.transcript[3], HistoryCell::User(ref text) if text == "next remote turn"));
    assert_eq!(app.action_cells.get("a1"), Some(&2));
}

#[tokio::test]
async fn hydrated_output_is_not_replayed_by_the_live_tail() {
    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = proc.clone();
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
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root);
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    let agent_path = format!("/agent/{pid}");
    let output_path = agent_output_path(&agent_path);

    // A completed turn, written in engine order (io/output first, then
    // the tape record), landing entirely before the client attaches.
    shell.write(&output_path, b"hi").await.unwrap();
    shell
        .write(
            &format!("{agent_path}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"hi\"}\n",
        )
        .await
        .unwrap();

    let mut app = FileBackedApp::new(agent_path.clone());
    let mut tails = hydrate_and_open_tails(&shell, &agent_path, &mut app)
        .await
        .unwrap();

    // The response is hydrated from the tape exactly once...
    let assistant_cells: Vec<_> = app
        .transcript
        .iter()
        .filter_map(|cell| match cell {
            HistoryCell::Assistant(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(assistant_cells, vec!["hi"]);

    // ...and the live tail delivers only post-attach output: the first
    // read must be the new write, not a replay of the hydrated "hi".
    shell.write(&output_path, b"-next").await.unwrap();
    let bytes = tokio::time::timeout(std::time::Duration::from_secs(5), tails.output.read(4096))
        .await
        .expect("tail read timed out")
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        "-next",
        "hydrated output must not be replayed by the live tail"
    );
}

#[test]
fn app_wiring_streams_then_confirms_via_tape_record() {
    // A thin smoke test that the app wires push_output/apply_tape_record
    // to the reconciler; exhaustive reconciliation cases live in the
    // reconcile module's property test.
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.push_output("hel".to_string());
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content: "hello".to_string(),
        submission_id: None,
    });
    app.push_output("lo".to_string());
    let assistant_cells: Vec<_> = app
        .transcript
        .iter()
        .filter_map(|cell| match cell {
            HistoryCell::Assistant(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(assistant_cells, vec!["hello"]);
}

#[test]
fn raced_turn_preview_cells_move_behind_their_user_boundary() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "first".to_string(),
        submission_id: None,
    });
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content: "done".to_string(),
        submission_id: None,
    });

    // UI/action cells for the next turn can beat that turn's user tape
    // record because they are tailed from independent files.
    app.apply_ui_event(UiEvent::Plan {
        snapshot: UiPlanSnapshot::new(
            Some("next turn".to_string()),
            vec![alan_agent_protocol::PlanItem {
                id: "1".to_string(),
                content: "prepare".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        ),
    });
    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a1".to_string(),
            name: "tool".to_string(),
            status: "completed".to_string(),
            output: "ran".to_string(),
            result: r#"{"exit_code":0}"#.to_string(),
        },
    );
    app.push_output("wor".to_string());

    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "second".to_string(),
        submission_id: None,
    });
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content: "world".to_string(),
        submission_id: None,
    });

    assert!(matches!(app.transcript[0], HistoryCell::User(ref text) if text == "first"));
    assert!(matches!(app.transcript[1], HistoryCell::Assistant(ref text) if text == "done"));
    assert!(matches!(app.transcript[2], HistoryCell::User(ref text) if text == "second"));
    assert!(matches!(app.transcript[3], HistoryCell::Plan(_)));
    assert!(matches!(app.transcript[4], HistoryCell::Tool { .. }));
    assert!(matches!(app.transcript[5], HistoryCell::Assistant(ref text) if text == "world"));
    assert_eq!(app.action_cells.get("a1"), Some(&4));
}

#[test]
fn remote_first_stream_preview_moves_behind_user_boundary() {
    let mut app = FileBackedApp::new("/agent/1".to_string());

    app.push_output("hello".to_string());
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "remote".to_string(),
        submission_id: None,
    });
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content: "hello".to_string(),
        submission_id: None,
    });

    assert_eq!(app.transcript.len(), 2);
    assert!(matches!(app.transcript[0], HistoryCell::User(ref text) if text == "remote"));
    assert!(matches!(app.transcript[1], HistoryCell::Assistant(ref text) if text == "hello"));
}

#[test]
fn stream_append_finds_open_preview_before_interposed_cells() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "remote".to_string(),
        submission_id: None,
    });

    app.push_output("hel".to_string());
    app.apply_ui_event(UiEvent::Plan {
        snapshot: UiPlanSnapshot::new(
            Some("same turn".to_string()),
            vec![alan_agent_protocol::PlanItem {
                id: "1".to_string(),
                content: "think".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        ),
    });
    app.push_output("lo".to_string());
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content: "hello".to_string(),
        submission_id: None,
    });

    let assistant_cells: Vec<_> = app
        .transcript
        .iter()
        .filter_map(|cell| match cell {
            HistoryCell::Assistant(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(assistant_cells, vec!["hello"]);
    assert!(matches!(app.transcript[2], HistoryCell::Plan(_)));
}

#[test]
fn hydrated_assistant_seeds_pending_boundary_state() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    let tape = r#"{"version":1,"kind":"message","role":"user","content":"first"}
{"version":1,"kind":"message","role":"assistant","content":"done"}
"#;
    app.transcript = parse_tape_history(tape);
    app.seed_reconciler_from_tape_history(tape);

    app.apply_ui_event(UiEvent::Plan {
        snapshot: UiPlanSnapshot::new(
            Some("next turn".to_string()),
            vec![alan_agent_protocol::PlanItem {
                id: "1".to_string(),
                content: "prepare".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        ),
    });
    app.push_output("wor".to_string());
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "second".to_string(),
        submission_id: None,
    });

    assert!(matches!(app.transcript[0], HistoryCell::User(ref text) if text == "first"));
    assert!(matches!(app.transcript[1], HistoryCell::Assistant(ref text) if text == "done"));
    assert!(matches!(app.transcript[2], HistoryCell::User(ref text) if text == "second"));
    assert!(matches!(app.transcript[3], HistoryCell::Plan(_)));
    assert!(matches!(app.transcript[4], HistoryCell::Assistant(ref text) if text == "wor"));
}

#[test]
fn pending_remote_turn_start_shifts_with_scrollback_prune() {
    let mut app = FileBackedApp::new("/agent/1".to_string());
    app.transcript
        .push(HistoryCell::Rendered(vec!["old".to_string()]));
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content: "done".to_string(),
        submission_id: None,
    });
    sync_action_snapshot(
        &mut app,
        ActionSnapshot {
            id: "a1".to_string(),
            name: "tool".to_string(),
            status: "completed".to_string(),
            output: "ran".to_string(),
            result: r#"{"exit_code":0}"#.to_string(),
        },
    );

    app.prune_rendered_prefix(RenderOpts::new(80, false), 1);
    app.apply_tape_record(TapeRecordV1 {
        version: 1,
        kind: "message".to_string(),
        role: "user".to_string(),
        content: "second".to_string(),
        submission_id: None,
    });

    assert!(matches!(app.transcript[0], HistoryCell::Assistant(ref text) if text == "done"));
    assert!(matches!(app.transcript[1], HistoryCell::User(ref text) if text == "second"));
    assert!(matches!(app.transcript[2], HistoryCell::Tool { .. }));
    assert_eq!(app.action_cells.get("a1"), Some(&2));
}

#[tokio::test]
async fn response_missed_at_attach_is_recovered_by_the_tape_watcher() {
    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = proc.clone();
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
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root);
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    let agent_path = format!("/agent/{pid}");

    // The engine wrote the response to io/output but its tape record has
    // not landed yet when the client attaches: the output bytes are
    // behind the live edge and the tape hydration comes up empty.
    shell
        .write(&agent_output_path(&agent_path), b"hi")
        .await
        .unwrap();
    let mut app = FileBackedApp::new(agent_path.clone());
    let mut tails = hydrate_and_open_tails(&shell, &agent_path, &mut app)
        .await
        .unwrap();
    assert!(
        !app.transcript
            .iter()
            .any(|cell| matches!(cell, HistoryCell::Assistant(_))),
        "nothing hydrated: the record has not landed yet"
    );

    // The tape record lands after attach; the tape watcher recovers it.
    shell
        .write(
            &format!("{agent_path}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"hi\"}\n",
        )
        .await
        .unwrap();
    let bytes = tokio::time::timeout(std::time::Duration::from_secs(5), tails.tape.read(4096))
        .await
        .expect("tape tail read timed out")
        .unwrap();
    let line = String::from_utf8(bytes).unwrap();
    let record: TapeRecordV1 = serde_json::from_str(line.trim()).unwrap();
    app.apply_tape_record(record);

    let assistant_cells: Vec<_> = app
        .transcript
        .iter()
        .filter_map(|cell| match cell {
            HistoryCell::Assistant(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        assistant_cells,
        vec!["hi"],
        "the tape watcher must recover a response the output tail missed"
    );
}

#[path = "inline_tests.rs"]
mod inline_tests;

#[path = "explicit_input_tests.rs"]
mod explicit_input_tests;
