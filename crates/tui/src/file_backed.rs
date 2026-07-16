use std::path::PathBuf;

#[cfg(test)]
use alan_agent_protocol::{
    ToolResultPresentation, UiActivitySnapshot, UiEvent, UiNoticeKind, UiNoticeSnapshot,
    UiPlanSnapshot, UiThinkingSnapshot, YieldKind,
};
use alan_ap::InProcessTransport;
use anyhow::{Result, bail};
#[cfg(test)]
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
mod app;
mod file_surface;

use app::{FileBackedAction, FileBackedApp, FileBackedEvent};

#[cfg(test)]
use file_surface::{
    ActionSnapshot, RequestSnapshot, TapeRecordV1, agent_output_path, parse_tape_history,
    request_snapshot_to_pending_yield, sync_actions_from_snapshots,
};
use file_surface::{
    hydrate_and_open_tails, spawn_action_watch, spawn_output_tail, spawn_request_watch,
    spawn_tape_watch, spawn_terminal_events, spawn_ui_watch, sync_actions_from_files,
    sync_requests_from_files, write_agent_input, write_interrupt, write_machine_ctl,
    write_request_response,
};

use crate::completion::{self, CompletionCandidate};
use crate::composer::{Composer, load_history};
#[cfg(test)]
use crate::history::{HistoryCell, PendingYieldCell, RenderOpts, RunningTool, ToolStatus};
use crate::terminal::{TerminalSession, terminal_capability_error};
use crate::transcript_ui::style_transcript_line;

const MAX_COMPOSER_LINES: usize = 10;
const MAX_COMPLETION_ROWS: usize = 6;
const SPINNER: [&str; 10] = ["|", "/", "-", "\\", "|", "/", "-", "\\", "|", "/"];

#[derive(Clone)]
pub struct FileBackedRunConfig {
    /// Mounted namespace surface for the local renderer host.
    pub root_transport: InProcessTransport,
    /// Concrete launched agent path, for example `/agent/1`.
    pub agent_path: String,
    /// Optional explicitly authorized Host directory used for local `@` file completion.
    pub host_file_completion_root: Option<PathBuf>,
    /// Whether stdin/stdout must be interactive before entering the UI.
    pub require_interactive_terminal: bool,
    /// Optional file used to persist composer input history across launches.
    pub history_path: Option<PathBuf>,
    /// Optional local skill candidates used for `$` completion.
    pub skill_candidates: Vec<CompletionCandidate>,
}

impl FileBackedRunConfig {
    pub fn new(root_transport: InProcessTransport, agent_path: impl Into<String>) -> Self {
        Self {
            root_transport,
            agent_path: agent_path.into(),
            host_file_completion_root: None,
            require_interactive_terminal: true,
            history_path: None,
            skill_candidates: Vec::new(),
        }
    }
}

pub async fn run(config: FileBackedRunConfig) -> Result<()> {
    if config.require_interactive_terminal && !crate::terminal::is_interactive_terminal() {
        bail!("{}", terminal_capability_error());
    }

    let shell = alan_shell::Shell::new(config.root_transport.clone());
    let mut app = FileBackedApp::new(config.agent_path.clone());
    app.set_skill_candidates(config.skill_candidates.clone());
    if let Some(host_root) = &config.host_file_completion_root {
        app.set_file_candidates(super::build_file_index(host_root, crate::FILE_INDEX_LIMIT));
    }
    if let Some(history_path) = &config.history_path {
        let history = load_history(history_path, crate::HISTORY_LIMIT);
        app.composer = Composer::with_history(history, Some(history_path.clone()));
    }
    let watch_tails = hydrate_and_open_tails(&shell, &config.agent_path, &mut app).await?;

    let mut terminal = TerminalSession::enter()?;
    terminal.draw_with(|frame| draw(frame, &app))?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<FileBackedEvent>(128);
    spawn_terminal_events(tx.clone());

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let output_task = tokio::spawn(spawn_output_tail(
        watch_tails.output,
        tx.clone(),
        shutdown_rx.clone(),
    ));
    let request_task = tokio::spawn(spawn_request_watch(
        watch_tails.requests,
        tx.clone(),
        shutdown_rx.clone(),
    ));
    let action_task = tokio::spawn(spawn_action_watch(
        watch_tails.actions,
        tx.clone(),
        shutdown_rx.clone(),
    ));
    let ui_task = tokio::spawn(spawn_ui_watch(
        watch_tails.ui,
        tx.clone(),
        shutdown_rx.clone(),
    ));
    let tape_task = tokio::spawn(spawn_tape_watch(watch_tails.tape, tx, shutdown_rx));

    let mut frame_tick = tokio::time::interval(std::time::Duration::from_millis(33));
    frame_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = true;

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else {
                    break;
                };
                match event {
                    FileBackedEvent::RequestsChanged => {
                        if let Err(err) = sync_requests_from_files(&shell, &app.agent_path.clone(), &mut app).await {
                            app.push_error(format!("request refresh failed: {err:#}"));
                        }
                    }
                    FileBackedEvent::ActionsChanged => {
                        if let Err(err) = sync_actions_from_files(&shell, &app.agent_path.clone(), &mut app).await {
                            app.push_error(format!("action refresh failed: {err:#}"));
                        }
                    }
                    other => {
                        if let Some(action) = app.dispatch(other) {
                            match action {
                                FileBackedAction::Submit(text) => {
                                    match write_agent_input(&shell, &app.agent_path, &text).await {
                                        Ok(()) => app.notice = None,
                                        Err(err) => app.push_error(format!("submit failed: {err:#}")),
                                    }
                                }
                                FileBackedAction::Resume { request_id, response } => {
                                    match write_request_response(&shell, &app.agent_path, &request_id, &response).await {
                                        Ok(()) => {
                                            app.notice = Some("response sent".to_string());
                                            if let Err(err) = sync_requests_from_files(&shell, &app.agent_path.clone(), &mut app).await {
                                                app.push_error(format!("request refresh failed: {err:#}"));
                                            }
                                        }
                                        Err(err) => app.push_error(format!("resume failed: {err:#}")),
                                    }
                                }
                                FileBackedAction::MachineCtl { command, success_notice } => {
                                    match write_machine_ctl(&shell, &app.agent_path, &command).await {
                                        Ok(()) => app.notice = Some(success_notice),
                                        Err(err) => app.push_error(format!("control failed: {err:#}")),
                                    }
                                }
                                FileBackedAction::Interrupt => {
                                    match write_interrupt(&shell, &app.agent_path).await {
                                        Ok(()) => app.notice = Some("interrupt sent".to_string()),
                                        Err(err) => app.push_error(format!("interrupt failed: {err:#}")),
                                    }
                                }
                                FileBackedAction::Quit => break,
                            }
                        }
                    }
                }
                dirty = true;
            }
            _ = frame_tick.tick() => {
                if dirty {
                    let (viewport_width, viewport_height) = terminal.viewport_size();
                    let committed = app.drain_committed_scrollback(viewport_width, viewport_height);
                    terminal.write_scrollback(&committed)?;
                    terminal.draw_with(|frame| draw(frame, &app))?;
                    dirty = false;
                }
                if app.should_quit {
                    break;
                }
            }
        }
    }

    let _ = shutdown_tx.send(true);
    let _ = output_task.await;
    let _ = request_task.await;
    let _ = action_task.await;
    let _ = ui_task.await;
    let _ = tape_task.await;

    Ok(())
}

fn draw(frame: &mut Frame<'_>, app: &FileBackedApp) {
    let area = frame.area();
    let width = area.width as usize;
    let live_height = app.live_region_height(width).max(2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(live_height)])
        .split(area);

    draw_transcript(frame, app, chunks[0]);
    draw_live_region(frame, app, chunks[1]);
}

fn draw_transcript(frame: &mut Frame<'_>, app: &FileBackedApp, area: Rect) {
    let rendered = app.rendered_history_lines(area.width as usize);
    let lines = if rendered.is_empty() {
        vec![Line::from(vec![
            Span::styled("alan", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(" ready", Style::default().fg(Color::DarkGray)),
        ])]
    } else {
        rendered.into_iter().map(style_transcript_line).collect()
    };

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default()),
        area,
    );
}

fn draw_live_region(frame: &mut Frame<'_>, app: &FileBackedApp, area: Rect) {
    let mut lines = Vec::new();
    lines.push(Line::styled(
        format!("local renderer host · {}", app.agent_path),
        Style::default().fg(Color::DarkGray),
    ));
    if let Some(label) = app.activity_label() {
        lines.push(activity_line(app, label));
    }
    if let Some(notice) = &app.notice {
        lines.push(Line::styled(
            format!("· {notice}"),
            Style::default().fg(Color::Yellow),
        ));
    }
    for tool in &app.running_tools {
        lines.push(Line::styled(
            format!("· tool running: {}", tool.title),
            Style::default().fg(Color::Cyan),
        ));
    }

    if let Some(form) = &app.form {
        for (text, focused) in form.render_lines() {
            let style = if focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if text.trim_start().starts_with('!') {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            lines.push(Line::styled(text, style));
        }
    } else {
        if let Some(state) = &app.completion {
            for (idx, candidate) in state.matches.iter().take(MAX_COMPLETION_ROWS).enumerate() {
                let trigger = match state.kind {
                    completion::CompletionKind::Command => "/",
                    completion::CompletionKind::Skill => "$",
                    completion::CompletionKind::File => "@",
                };
                let mut label = format!("{trigger}{}", candidate.label);
                if let Some(detail) = &candidate.detail {
                    label.push_str(&format!("  - {detail}"));
                }
                let style = if idx == state.selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };
                lines.push(Line::styled(format!("  {label}"), style));
            }
        }
        lines.extend(app.composer_lines());
        lines.push(app.hint_line());
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn activity_line(app: &FileBackedApp, label: &str) -> Line<'static> {
    let elapsed = app
        .activity_started_at_ms()
        .and_then(|started_at_ms| {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis() as u64;
            Some(now_ms.saturating_sub(started_at_ms) / 1_000)
        })
        .unwrap_or(0);
    let frame_idx = (elapsed as usize) % SPINNER.len();
    Line::from(vec![
        Span::styled(
            format!("{} ", SPINNER[frame_idx]),
            Style::default().fg(Color::Green),
        ),
        Span::styled(
            label.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · esc interrupt · {elapsed}s"),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

#[cfg(test)]
mod tests {
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
    fn scrollback_drains_by_rendered_lines() {
        let mut app = FileBackedApp::new("/agent/1".to_string());
        app.transcript
            .push(HistoryCell::Assistant("long streamed output ".repeat(40)));

        let drained = app.drain_committed_scrollback(32, 10);

        assert!(!drained.is_empty());
        assert!(app.rendered_history_lines(32).len() <= 8);
        assert!(matches!(app.transcript[0], HistoryCell::Assistant(_)));
    }

    #[test]
    fn action_snapshots_track_running_and_commit_completed_tool() {
        let mut app = FileBackedApp::new("/agent/1".to_string());
        sync_actions_from_snapshots(
            &mut app,
            vec![ActionSnapshot {
                id: "a0".to_string(),
                name: "edit".to_string(),
                status: "running".to_string(),
                output: String::new(),
                result: String::new(),
            }],
        );
        assert_eq!(
            app.running_tools,
            vec![RunningTool {
                id: "a0".to_string(),
                title: "edit".to_string(),
            }]
        );
        assert!(app.transcript.is_empty());

        sync_actions_from_snapshots(
            &mut app,
            vec![ActionSnapshot {
                id: "a0".to_string(),
                name: "edit".to_string(),
                status: "completed".to_string(),
                output: "updated file".to_string(),
                result: r#"{"exit_code":0}"#.to_string(),
            }],
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

        assert!(
            matches!(app.transcript.last(), Some(HistoryCell::Plan(items)) if items.len() == 1)
        );
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

    #[test]
    fn confirmation_digit_builds_resume_response() {
        let mut app = FileBackedApp::new("/agent/1".to_string());
        app.set_pending_yield(PendingYieldCell {
            request_id: "r1".to_string(),
            kind: YieldKind::Confirmation,
            title: "Approve?".to_string(),
            prompt: None,
            options: vec!["approve".to_string(), "reject".to_string()],
            default_option: None,
            questions: Vec::new(),
            capability: None,
            reason: None,
            presentation: None,
        });

        let action = app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Key(
            KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
        )));
        match action {
            Some(FileBackedAction::Resume {
                request_id,
                response,
            }) => {
                assert_eq!(request_id, "r1");
                assert_eq!(response, r#"{"choice":"approve"}"#);
            }
            other => panic!("expected resume action, got {other:?}"),
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
            .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
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
        sync_actions_from_snapshots(
            &mut app,
            vec![ActionSnapshot {
                id: "a1".to_string(),
                name: "tool".to_string(),
                status: "completed".to_string(),
                output: "ran".to_string(),
                result: r#"{"exit_code":0}"#.to_string(),
            }],
        );

        app.apply_tape_record(TapeRecordV1 {
            version: 1,
            kind: "message".to_string(),
            role: "user".to_string(),
            content: "next remote turn".to_string(),
        });

        assert!(matches!(app.transcript[0], HistoryCell::User(ref text) if text == "run this"));
        assert!(matches!(app.transcript[1], HistoryCell::PendingYield(_)));
        assert!(matches!(app.transcript[2], HistoryCell::Tool { .. }));
        assert!(
            matches!(app.transcript[3], HistoryCell::User(ref text) if text == "next remote turn")
        );
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
            .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
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
        let bytes =
            tokio::time::timeout(std::time::Duration::from_secs(5), tails.output.read(4096))
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
        });
        app.apply_tape_record(TapeRecordV1 {
            version: 1,
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content: "done".to_string(),
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
        sync_actions_from_snapshots(
            &mut app,
            vec![ActionSnapshot {
                id: "a1".to_string(),
                name: "tool".to_string(),
                status: "completed".to_string(),
                output: "ran".to_string(),
                result: r#"{"exit_code":0}"#.to_string(),
            }],
        );
        app.push_output("wor".to_string());

        app.apply_tape_record(TapeRecordV1 {
            version: 1,
            kind: "message".to_string(),
            role: "user".to_string(),
            content: "second".to_string(),
        });
        app.apply_tape_record(TapeRecordV1 {
            version: 1,
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content: "world".to_string(),
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
        });
        app.apply_tape_record(TapeRecordV1 {
            version: 1,
            kind: "message".to_string(),
            role: "assistant".to_string(),
            content: "hello".to_string(),
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
        });
        sync_actions_from_snapshots(
            &mut app,
            vec![ActionSnapshot {
                id: "a1".to_string(),
                name: "tool".to_string(),
                status: "completed".to_string(),
                output: "ran".to_string(),
                result: r#"{"exit_code":0}"#.to_string(),
            }],
        );

        app.prune_rendered_prefix(RenderOpts::new(80, false), 1);
        app.apply_tape_record(TapeRecordV1 {
            version: 1,
            kind: "message".to_string(),
            role: "user".to_string(),
            content: "second".to_string(),
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
            .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
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
}
