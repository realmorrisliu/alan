use std::collections::VecDeque;
use std::sync::Arc;

use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::InProcessTransport;
use alan_ap::reference::MemFs;
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;
use tokio::sync::mpsc;

use crate::file_backed::app::{FileBackedAction, FileBackedEvent};
use crate::file_backed::file_surface::ActionSnapshot;
use crate::file_backed::observation_io;
use crate::file_backed::streams::{
    WorkstreamControl, WorkstreamControlResult, WorkstreamCountsDto, WorkstreamEntryDto,
    WorkstreamInboxEntryDto, WorkstreamOverviewDto, WorkstreamPersistenceKind,
    WorkstreamStatusKind, footer_status_line, parse_route_log_line,
};
use crate::file_backed::streams_io::switch_agent_target;
use crate::file_backed::watchers::AgentWatchers;
use crate::file_backed::{FileBackedApp, hydrate_and_open_tails};
use crate::history::{HistoryCell, RenderOpts};
use crate::theme::Theme;
use crate::transcript_ui::wrapped_line_count;

// Helper trait only for concise test construction of ActionSnapshot.
trait TestActionSnapshotExt {
    fn test_subagent(id: &str, status: &str, stream_name: &str, child_pid: u64) -> Self;
}

impl TestActionSnapshotExt for ActionSnapshot {
    fn test_subagent(id: &str, status: &str, stream_name: &str, child_pid: u64) -> Self {
        Self {
            id: id.to_string(),
            name: "spawn_subagent".to_string(),
            status: status.to_string(),
            output: String::new(),
            result: serde_json::json!({
                "stream_name": stream_name,
                "child_pid": child_pid,
                "child_state": status,
            })
            .to_string(),
        }
    }
}

fn sample_overview() -> WorkstreamOverviewDto {
    WorkstreamOverviewDto {
        project_key: Some("proj-alan".to_string()),
        counts: WorkstreamCountsDto {
            running: 2,
            idle: 1,
            paused_error: 0,
            archived: 3,
            unread_inbox: 4,
        },
        streams: vec![
            WorkstreamEntryDto {
                name: "ui-polish".to_string(),
                scope: "Refine footer status bar".to_string(),
                persistence: WorkstreamPersistenceKind::Persistent,
                state: WorkstreamStatusKind::Running,
                child_pid: Some(42),
                model_override: None,
                rollout_ref: None,
                updated_at: "2026-11-25T08:00:00Z".to_string(),
            },
            WorkstreamEntryDto {
                name: "docs-sync".to_string(),
                scope: "Update OpenSpec deltas".to_string(),
                persistence: WorkstreamPersistenceKind::Persistent,
                state: WorkstreamStatusKind::Idle,
                child_pid: None,
                model_override: None,
                rollout_ref: Some("rollouts/1.jsonl".to_string()),
                updated_at: "2026-11-25T07:00:00Z".to_string(),
            },
            WorkstreamEntryDto {
                name: "quick-lint".to_string(),
                scope: "Run clippy check".to_string(),
                persistence: WorkstreamPersistenceKind::OneOff,
                state: WorkstreamStatusKind::Archived,
                child_pid: None,
                model_override: None,
                rollout_ref: None,
                updated_at: "2026-11-25T06:00:00Z".to_string(),
            },
        ],
        unread_inbox: vec![WorkstreamInboxEntryDto {
            run_id: "run-1".to_string(),
            workflow_id: "nightly-audit".to_string(),
            stream_name: Some("docs-sync".to_string()),
            status: "ok".to_string(),
            summary: "All checks passed cleanly.".to_string(),
            created_at: "2026-11-25T05:00:00Z".to_string(),
        }],
    }
}

#[test]
fn routing_receipt_renders_reroute_handoff_dead_letter_and_dumb_fallback() {
    let cell = HistoryCell::RoutingReceipt {
        source: "stream-a".to_string(),
        target: "stream-b".to_string(),
        kind: "reroute_request".to_string(),
        summary: None,
    };
    let wide = cell.render_lines(RenderOpts::new(80, false));
    assert_eq!(wide, vec!["↳ rerouted from [stream-a] → [stream-b]"]);

    let dumb = cell.render_lines(RenderOpts::new(80, false).with_theme(Theme::dumb()));
    assert_eq!(dumb, vec!["> rerouted from [stream-a] -> [stream-b]"]);

    let dead = HistoryCell::RoutingReceipt {
        source: "stream-a".to_string(),
        target: "root".to_string(),
        kind: "dead_letter".to_string(),
        summary: Some("hop limit (3) escalated to root".to_string()),
    };
    let dead_lines = dead.render_lines(RenderOpts::new(80, false));
    assert_eq!(
        dead_lines,
        vec!["↳ dead-letter from [stream-a] → [root] · hop limit (3) escalated to root"]
    );

    // Narrow pane wrapping never overflows width.
    for width in [16usize, 24, 32] {
        let wrapped = dead.render_styled_lines(RenderOpts::new(width, false));
        assert!(!wrapped.is_empty());
        for line in wrapped {
            assert!(
                line.width() <= width,
                "wrapped line exceeded width {width}: {:?}",
                line.to_string()
            );
        }
    }
}

#[test]
fn footer_status_bar_counts_appear_only_when_active_and_fit_narrow_panes() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    assert!(footer_status_line(&app, 80).is_none());

    app.streams.apply_overview(sample_overview());
    let wide = footer_status_line(&app, 100)
        .expect("badge visible")
        .to_string();
    assert_eq!(wide, "streams: 2 active · 1 idle · 4 inbox");

    // Combined with Running hint on wide 120-col screen without losing Esc interrupt spacing.
    app.activity = alan_agent_protocol::UiActivitySnapshot::running(1);
    let running_wide = footer_status_line(&app, 120)
        .expect("running footer")
        .to_string();
    assert!(running_wide.contains("Esc interrupt"), "{running_wide}");
    assert!(
        running_wide.contains("streams: 2 active · 1 idle · 4 inbox"),
        "{running_wide}"
    );

    app.activity = alan_agent_protocol::UiActivitySnapshot::idle();
    for width in [16usize, 24, 36, 48, 80, 120] {
        let line = footer_status_line(&app, width).expect("footer line");
        assert_eq!(
            wrapped_line_count(std::slice::from_ref(&line), width),
            1,
            "footer must never wrap to multiple rows at width {width}: {:?}",
            line.to_string()
        );
        assert!(line.width() <= width);
    }
}

#[test]
fn streams_focus_and_root_commands_validate_and_render_inline() {
    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.viewport_size = (80, 24);

    assert!(matches!(
        app.handle_command("/streams"),
        Some(FileBackedAction::StreamsControl {
            control: WorkstreamControl::Inspect,
            show_overview: true
        })
    ));
    assert!(matches!(
        app.handle_command("/streams clear-inbox"),
        Some(FileBackedAction::StreamsControl {
            control: WorkstreamControl::ClearInbox,
            show_overview: false
        })
    ));
    assert!(matches!(
        app.handle_command("/streams archive docs-sync"),
        Some(FileBackedAction::StreamsControl {
            control: WorkstreamControl::Archive { ref name },
            show_overview: false
        }) if name == "docs-sync"
    ));
    assert!(matches!(
        app.handle_command("/streams resume docs-sync"),
        Some(FileBackedAction::StreamsControl {
            control: WorkstreamControl::Resume { ref name },
            show_overview: false
        }) if name == "docs-sync"
    ));
    assert!(matches!(
        app.handle_command("/focus ui-polish"),
        Some(FileBackedAction::FocusStream { ref name }) if name == "ui-polish"
    ));
    assert!(app.handle_command("/focus").is_none());
    assert!(app.handle_command("/root").is_none());

    app.dispatch(FileBackedEvent::WorkstreamControlCompleted {
        show_overview: true,
        result: Ok(WorkstreamControlResult::Overview(sample_overview())),
    });
    let rendered = app.rendered_history_lines(80).join("\n");
    assert!(rendered.contains("Workstreams · 2 active · 1 idle · 3 archived · 4 inbox"));
    assert!(rendered.contains("ui-polish"));
    assert!(rendered.contains("docs-sync"));
    assert!(rendered.contains("Archived: 3 collapsed"));
    assert!(rendered.contains("nightly-audit"));
    assert!(rendered.contains("/focus <name>"));
    assert_eq!(app.streams.live_pid_for("ui-polish"), Some(42));
}

#[test]
fn route_log_parser_handles_direct_and_embedded_handoffs_and_deduplicates() {
    let direct = serde_json::json!({
        "version": 1,
        "port": "root",
        "rule": "reroute",
        "message": {
            "version": 1,
            "type": "reroute_request",
            "kind": "reroute_request",
            "source_stream": "A",
            "suggested_target": "B",
            "hop_count": 1,
            "correlation_id": "corr-1",
            "content": "switch to B"
        }
    })
    .to_string();
    let parsed = parse_route_log_line(&direct).expect("direct parsed");
    assert_eq!(parsed.source, "A");
    assert_eq!(parsed.target, "B");
    assert_eq!(parsed.kind, "reroute_request");

    let embedded_payload = serde_json::json!({
        "version": 1,
        "kind": "cross_stream_dependency",
        "source_stream": "frontend",
        "suggested_target": "api",
        "hop_count": 1,
        "correlation_id": "corr-2",
        "payload": "need schema"
    })
    .to_string();
    let wrapped = serde_json::json!({
        "version": 1,
        "port": "root",
        "rule": "handoff",
        "message": {
            "version": 1,
            "type": "cross_stream_dependency",
            "content": embedded_payload
        }
    })
    .to_string();
    let parsed_wrapped = parse_route_log_line(&wrapped).expect("wrapped parsed");
    assert_eq!(parsed_wrapped.source, "frontend");
    assert_eq!(parsed_wrapped.target, "api");
    assert_eq!(parsed_wrapped.kind, "cross_stream_dependency");

    let dead = serde_json::json!({
        "version": 1,
        "port": "dead-letter",
        "rule": "dead-letter",
        "reason": "no matching rule",
        "message": {
            "version": 1,
            "type": "reroute_request",
            "kind": "reroute_request",
            "source_stream": "A",
            "suggested_target": "B",
            "hop_count": 3,
            "correlation_id": "corr-3",
            "content": "loop"
        }
    })
    .to_string();
    let parsed_dead = parse_route_log_line(&dead).expect("dead letter parsed");
    assert_eq!(parsed_dead.kind, "dead_letter");
    assert!(
        parsed_dead
            .summary
            .as_deref()
            .is_some_and(|s| s.contains("hop limit (3)"))
    );

    let mut app = FileBackedApp::new("/agent/root".to_string());
    app.apply_route_log_line(&direct);
    app.apply_route_log_line(&direct);
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(
        app.transcript[0].render_lines(RenderOpts::new(80, false)),
        vec!["↳ rerouted from [A] → [B]"]
    );

    let action = ActionSnapshot::test_subagent("a1", "running", "worker", 77);
    app.observe_workstream_action(&action);
    assert_eq!(app.streams.live_pid_for("worker"), Some(77));
}

#[tokio::test]
async fn focus_and_root_switch_over_agent_pids_preserves_coherent_scrollback_without_duplicates() {
    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let ns = Namespace::new();
    let mount_fs = Arc::new(MountFs::new(ns));
    let live = mount_fs.live_namespace();
    live.mount(
        "/proc",
        InProcessTransport::new(procfs.clone()),
        Access::ReadWrite,
    );
    live.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    let root_transport = InProcessTransport::new(mount_fs);
    let shell = Shell::new(root_transport);

    let spec =
        r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;
    let root_pid = shell.spawn(spec).await.unwrap();
    agent_root
        .bind_process(root_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(root_pid.clone()).await;
    live.mount(
        "/mnt/service-manager/units/root-agent",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
            "pid",
            format!("{root_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );

    let child_pid = shell.spawn(spec).await.unwrap();
    agent_root
        .bind_process(child_pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let root_agent = format!("/agent/{root_pid}");
    let child_agent = format!("/agent/{child_pid}");
    shell
        .write(
            &format!("{root_agent}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"root-initial-q\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"root-initial-a\"}\n",
        )
        .await
        .unwrap();
    shell
        .write(
            &format!("{child_agent}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"worker-initial-q\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"worker-initial-a\"}\n",
        )
        .await
        .unwrap();

    let mut app = FileBackedApp::new("/agent/root".to_string());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, mut rx) = mpsc::channel(64);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx.clone());
    let mut pending_turns = VecDeque::new();
    let mut jobs = tokio::task::JoinSet::new();
    let model_reads = observation_io::start(shell.clone(), true, tx.clone(), &mut jobs);
    let skill_reads = observation_io::start(shell.clone(), false, tx.clone(), &mut jobs);
    let usage_reads = observation_io::start_usage(shell.clone(), tx.clone(), &mut jobs);

    // Switch to child stream.
    let switched_to_child = switch_agent_target(
        &shell,
        &mut watchers,
        &mut app,
        &mut rx,
        &mut pending_turns,
        &tx,
        &model_reads,
        &skill_reads,
        &usage_reads,
        Some("worker".to_string()),
        child_agent.clone(),
        Some(child_pid.parse().unwrap()),
    )
    .await;
    assert!(switched_to_child);
    assert_eq!(app.agent_path, child_agent);
    assert_eq!(app.streams.focused_stream.as_deref(), Some("worker"));
    assert!(app.context_line(80).to_string().contains("focus:worker"));

    // Simulate a new turn on Root while focused on child.
    shell
        .write(
            &format!("{root_agent}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"root-later-a\"}\n",
        )
        .await
        .unwrap();

    // Switch back to Root.
    let switched_to_root = switch_agent_target(
        &shell,
        &mut watchers,
        &mut app,
        &mut rx,
        &mut pending_turns,
        &tx,
        &model_reads,
        &skill_reads,
        &usage_reads,
        None,
        "/agent/root".to_string(),
        None,
    )
    .await;
    assert!(switched_to_root);
    assert_eq!(app.agent_path, "/agent/root");
    assert!(app.streams.focused_stream.is_none());

    let rendered = app.rendered_history_lines(100).join("\n");
    assert_eq!(rendered.matches("root-initial-q").count(), 1, "{rendered}");
    assert_eq!(rendered.matches("root-initial-a").count(), 1, "{rendered}");
    assert_eq!(
        rendered.matches("worker-initial-q").count(),
        1,
        "{rendered}"
    );
    assert_eq!(
        rendered.matches("worker-initial-a").count(),
        1,
        "{rendered}"
    );
    assert_eq!(rendered.matches("root-later-a").count(), 1, "{rendered}");
    assert!(rendered.contains("focused [worker]"), "{rendered}");
    assert!(rendered.contains("focused [root]"), "{rendered}");

    watchers.stop().await;
    jobs.abort_all();
}
