use std::{collections::BTreeMap, path::PathBuf};

use alan_agent_protocol::{
    ContentPart, StructuredInputQuestion, ToolResultPresentation, UiActivitySnapshot,
    UiActivityState, UiEvent, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
    UiThinkingState, YieldKind,
};
use alan_ap::InProcessTransport;
use anyhow::{Context, Result, anyhow, bail};
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::completion::{self, CompletionCandidate, CompletionSources, CompletionState};
use crate::composer::{Composer, ComposerKeyOutcome, load_history};
use crate::form::FormState;
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
    /// Optional workspace directory used for local `@` file completion.
    pub workspace_dir: Option<PathBuf>,
    /// Whether stdin/stdout must be interactive before entering the UI.
    pub require_interactive_terminal: bool,
    /// Optional file used to persist composer input history across sessions.
    pub history_path: Option<PathBuf>,
    /// Optional local skill candidates used for `$` completion.
    pub skill_candidates: Vec<CompletionCandidate>,
}

impl FileBackedRunConfig {
    pub fn new(root_transport: InProcessTransport, agent_path: impl Into<String>) -> Self {
        Self {
            root_transport,
            agent_path: agent_path.into(),
            workspace_dir: None,
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
    if let Some(workspace_dir) = &config.workspace_dir {
        app.set_file_candidates(super::build_file_index(
            workspace_dir,
            crate::FILE_INDEX_LIMIT,
        ));
    }
    if let Some(history_path) = &config.history_path {
        let history = load_history(history_path, crate::HISTORY_LIMIT);
        app.composer = Composer::with_history(history, Some(history_path.clone()));
    }
    // Open the watch tails BEFORE hydrating: each tail pins its live edge at
    // open time, and hydration afterwards reads everything up to now — so a
    // write landing between the two is covered by hydration instead of being
    // silently skipped as "pre-existing". The tiny overlap window (a write
    // both hydrated and later tail-delivered) is safe: request/action watches
    // only trigger idempotent re-syncs, and UI snapshot application is
    // change-guarded.
    let watch_tails = open_watch_tails(&shell, &config.agent_path).await?;
    hydrate_app_from_files(&shell, &config.agent_path, &mut app).await?;

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
    let ui_task = tokio::spawn(spawn_ui_watch(watch_tails.ui, tx, shutdown_rx));

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

    Ok(())
}

async fn hydrate_app_from_files(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<()> {
    app.transcript = read_tape_history(shell, agent_path).await?;
    sync_actions_from_files(shell, agent_path, app).await?;
    sync_requests_from_files(shell, agent_path, app).await?;
    sync_ui_from_files(shell, agent_path, app).await
}

async fn sync_requests_from_files(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<()> {
    let pending = read_latest_pending_request(shell, agent_path).await?;
    match pending {
        Some(snapshot) => app.set_pending_yield(request_snapshot_to_pending_yield(snapshot)?),
        None => app.clear_pending_yield(),
    }
    Ok(())
}

async fn sync_actions_from_files(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<()> {
    let snapshots = read_action_snapshots(shell, agent_path).await?;
    sync_actions_from_snapshots(app, snapshots);
    Ok(())
}

async fn sync_ui_from_files(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<()> {
    for event in read_ui_event_history(shell, agent_path).await? {
        app.apply_ui_event(event);
    }
    app.apply_ui_activity_snapshot(read_json_file(shell, &ui_activity_path(agent_path)).await?);
    app.apply_ui_plan_snapshot(read_json_file(shell, &ui_plan_path(agent_path)).await?);
    app.apply_ui_thinking_snapshot(read_json_file(shell, &ui_thinking_path(agent_path)).await?);
    app.apply_ui_notice_snapshot(read_json_file(shell, &ui_notice_path(agent_path)).await?);
    Ok(())
}

/// The four live watch tails, opened together before hydration so nothing
/// written between snapshot and tail startup is lost (see `run`).
struct WatchTails {
    output: alan_shell::Tail,
    requests: alan_shell::Tail,
    actions: alan_shell::Tail,
    ui: alan_shell::Tail,
}

async fn open_watch_tails(shell: &alan_shell::Shell, agent_path: &str) -> Result<WatchTails> {
    Ok(WatchTails {
        output: tail_from_live_edge(shell, &agent_output_path(agent_path)).await?,
        requests: tail_from_live_edge(shell, &request_events_path(agent_path)).await?,
        actions: tail_from_live_edge(shell, &action_events_path(agent_path)).await?,
        ui: tail_from_live_edge(shell, &ui_events_path(agent_path)).await?,
    })
}

async fn spawn_output_tail(
    mut tail: alan_shell::Tail,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            result = tail.read(4096) => {
                match result {
                    Ok(bytes) if bytes.is_empty() => break,
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).to_string();
                        if tx.send(FileBackedEvent::Output(text)).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(FileBackedEvent::Error(format!(
                            "output tail failed: {err:?}"
                        ))).await;
                        break;
                    }
                }
            }
        }
    }

    tail.close()
        .await
        .map_err(|err| anyhow!("failed to close output tail: {err:?}"))?;
    Ok(())
}

async fn spawn_request_watch(
    mut tail: alan_shell::Tail,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            result = tail.read(4096) => {
                match result {
                    Ok(bytes) if bytes.is_empty() => break,
                    Ok(_) => {
                        if tx.send(FileBackedEvent::RequestsChanged).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(FileBackedEvent::Error(format!(
                            "request watch failed: {err:?}"
                        ))).await;
                        break;
                    }
                }
            }
        }
    }

    tail.close()
        .await
        .map_err(|err| anyhow!("failed to close request watch: {err:?}"))?;
    Ok(())
}

async fn spawn_action_watch(
    mut tail: alan_shell::Tail,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            result = tail.read(4096) => {
                match result {
                    Ok(bytes) if bytes.is_empty() => break,
                    Ok(_) => {
                        if tx.send(FileBackedEvent::ActionsChanged).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(FileBackedEvent::Error(format!(
                            "action watch failed: {err:?}"
                        ))).await;
                        break;
                    }
                }
            }
        }
    }

    tail.close()
        .await
        .map_err(|err| anyhow!("failed to close action tail: {err:?}"))?;
    Ok(())
}

async fn spawn_ui_watch(
    mut tail: alan_shell::Tail,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let mut pending = Vec::new();
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            result = tail.read(4096) => {
                match result {
                    Ok(bytes) if bytes.is_empty() => break,
                    Ok(bytes) => {
                        pending.extend_from_slice(&bytes);
                        while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                            let line = pending.drain(..=newline).collect::<Vec<_>>();
                            let line = &line[..line.len().saturating_sub(1)];
                            if line.is_empty() {
                                continue;
                            }
                            match serde_json::from_slice::<UiEvent>(line) {
                                Ok(event) => {
                                    if tx.send(FileBackedEvent::Ui(event)).await.is_err() {
                                        pending.clear();
                                        break;
                                    }
                                }
                                Err(err) => {
                                    let _ = tx.send(FileBackedEvent::Error(format!(
                                        "ui watch parse failed: {err}"
                                    ))).await;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(FileBackedEvent::Error(format!(
                            "ui watch failed: {err:?}"
                        ))).await;
                        break;
                    }
                }
            }
        }
    }

    tail.close()
        .await
        .map_err(|err| anyhow!("failed to close ui watch: {err:?}"))?;
    Ok(())
}

async fn tail_from_live_edge(shell: &alan_shell::Shell, path: &str) -> Result<alan_shell::Tail> {
    let existing = shell
        .cat(path)
        .await
        .map_err(|err| anyhow!("failed to snapshot {path}: {err:?}"))?;
    let mut tail = shell
        .tail(path)
        .await
        .map_err(|err| anyhow!("failed to tail {path}: {err:?}"))?;
    let mut skipped = 0usize;
    while skipped < existing.len() {
        let remaining = existing.len() - skipped;
        let chunk = tail
            .read(remaining.min(64 * 1024) as u32)
            .await
            .map_err(|err| anyhow!("failed to skip existing {path} bytes: {err:?}"))?;
        if chunk.is_empty() {
            bail!("tail for {path} closed before existing bytes were skipped");
        }
        skipped += chunk.len();
    }
    Ok(tail)
}

fn spawn_terminal_events(tx: tokio::sync::mpsc::Sender<FileBackedEvent>) {
    tokio::task::spawn_blocking(move || {
        loop {
            match crossterm::event::poll(std::time::Duration::from_millis(100)) {
                Ok(true) => match crossterm::event::read() {
                    Ok(event) => {
                        let should_quit = matches!(
                            event,
                            TerminalEvent::Key(KeyEvent {
                                code: KeyCode::Char('q'),
                                modifiers,
                                ..
                            }) if modifiers.contains(KeyModifiers::CONTROL)
                        );
                        if tx.blocking_send(FileBackedEvent::Terminal(event)).is_err()
                            || should_quit
                        {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.blocking_send(FileBackedEvent::Error(format!(
                            "terminal input failed: {err}"
                        )));
                        break;
                    }
                },
                Ok(false) => {}
                Err(err) => {
                    let _ = tx.blocking_send(FileBackedEvent::Error(format!(
                        "terminal polling failed: {err}"
                    )));
                    break;
                }
            }
        }
    });
}

fn agent_input_path(agent_path: &str) -> String {
    format!("{agent_path}/io/input")
}

fn agent_output_path(agent_path: &str) -> String {
    format!("{agent_path}/io/output")
}

fn request_events_path(agent_path: &str) -> String {
    format!("{agent_path}/requests/events")
}

fn action_events_path(agent_path: &str) -> String {
    format!("{agent_path}/actions/events")
}

fn ui_activity_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/activity")
}

fn ui_plan_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/plan")
}

fn ui_thinking_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/thinking")
}

fn ui_notice_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/notice")
}

fn ui_events_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/events")
}

fn machine_ctl_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ctl")
}

fn request_response_path(agent_path: &str, request_id: &str) -> String {
    format!("{agent_path}/requests/{request_id}/response")
}

fn proc_ctl_path(agent_path: &str) -> Result<String> {
    let segments = agent_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    match segments.as_slice() {
        ["agent", "root"] => {
            bail!("agent path must resolve to a concrete pid, not /agent/root")
        }
        ["agent", pid] => Ok(format!("/proc/{pid}/ctl")),
        _ => bail!("agent path must be a concrete /agent/<pid> path: {agent_path}"),
    }
}

async fn write_agent_input(shell: &alan_shell::Shell, agent_path: &str, text: &str) -> Result<()> {
    shell
        .write(&agent_input_path(agent_path), text.as_bytes())
        .await
        .map_err(|err| anyhow!("write agent input failed: {err:?}"))
}

async fn write_request_response(
    shell: &alan_shell::Shell,
    agent_path: &str,
    request_id: &str,
    response: &str,
) -> Result<()> {
    shell
        .write(
            &request_response_path(agent_path, request_id),
            response.as_bytes(),
        )
        .await
        .map_err(|err| anyhow!("write request response failed: {err:?}"))
}

async fn write_machine_ctl(
    shell: &alan_shell::Shell,
    agent_path: &str,
    command: &str,
) -> Result<()> {
    shell
        .write(&machine_ctl_path(agent_path), command.as_bytes())
        .await
        .map_err(|err| anyhow!("write machine ctl failed: {err:?}"))
}

async fn write_interrupt(shell: &alan_shell::Shell, agent_path: &str) -> Result<()> {
    let ctl_path = proc_ctl_path(agent_path)?;
    shell
        .write(&ctl_path, b"interrupt")
        .await
        .map_err(|err| anyhow!("write process interrupt failed: {err:?}"))
}

async fn read_tape_history(
    shell: &alan_shell::Shell,
    agent_path: &str,
) -> Result<Vec<HistoryCell>> {
    let raw = shell
        .cat(&format!("{agent_path}/machine/tape"))
        .await
        .map_err(|err| anyhow!("read tape failed: {err:?}"))?;
    Ok(parse_tape_history(
        &String::from_utf8(raw).context("machine/tape is not utf8")?,
    ))
}

async fn read_latest_pending_request(
    shell: &alan_shell::Shell,
    agent_path: &str,
) -> Result<Option<RequestSnapshot>> {
    let mut ids = shell
        .ls(&format!("{agent_path}/requests"))
        .await
        .map_err(|err| anyhow!("list requests failed: {err:?}"))?
        .into_iter()
        .filter(|entry| entry != "clone" && entry != "events")
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| request_sort_key(id));
    ids.reverse();

    for id in ids {
        let request = read_request_snapshot(shell, agent_path, &id).await?;
        if request.status == "pending" {
            return Ok(Some(request));
        }
    }
    Ok(None)
}

async fn read_request_snapshot(
    shell: &alan_shell::Shell,
    agent_path: &str,
    request_id: &str,
) -> Result<RequestSnapshot> {
    let request_path = format!("{agent_path}/requests/{request_id}");
    Ok(RequestSnapshot {
        id: request_id.to_string(),
        kind: read_utf8(shell, &format!("{request_path}/kind")).await?,
        prompt: read_utf8(shell, &format!("{request_path}/prompt")).await?,
        options: read_utf8(shell, &format!("{request_path}/options"))
            .await
            .unwrap_or_default(),
        status: read_utf8(shell, &format!("{request_path}/status")).await?,
    })
}

async fn read_action_snapshots(
    shell: &alan_shell::Shell,
    agent_path: &str,
) -> Result<Vec<ActionSnapshot>> {
    let mut ids = shell
        .ls(&format!("{agent_path}/actions"))
        .await
        .map_err(|err| anyhow!("list actions failed: {err:?}"))?
        .into_iter()
        .filter(|entry| entry != "clone" && entry != "events")
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| request_sort_key(id));

    let mut snapshots = Vec::with_capacity(ids.len());
    for id in ids {
        snapshots.push(read_action_snapshot(shell, agent_path, &id).await?);
    }
    Ok(snapshots)
}

async fn read_action_snapshot(
    shell: &alan_shell::Shell,
    agent_path: &str,
    action_id: &str,
) -> Result<ActionSnapshot> {
    let action_path = format!("{agent_path}/actions/{action_id}");
    Ok(ActionSnapshot {
        id: action_id.to_string(),
        name: read_utf8(shell, &format!("{action_path}/name"))
            .await
            .unwrap_or_default(),
        status: read_utf8(shell, &format!("{action_path}/status")).await?,
        output: read_utf8(shell, &format!("{action_path}/output"))
            .await
            .unwrap_or_default(),
        result: read_utf8(shell, &format!("{action_path}/result"))
            .await
            .unwrap_or_default(),
    })
}

async fn read_utf8(shell: &alan_shell::Shell, path: &str) -> Result<String> {
    let bytes = shell
        .cat(path)
        .await
        .map_err(|err| anyhow!("read {path} failed: {err:?}"))?;
    String::from_utf8(bytes).with_context(|| format!("{path} is not utf8"))
}

async fn read_json_file<T: DeserializeOwned>(shell: &alan_shell::Shell, path: &str) -> Result<T> {
    let bytes = shell
        .cat(path)
        .await
        .map_err(|err| anyhow!("read {path} failed: {err:?}"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {path} failed"))
}

async fn read_ui_event_history(
    shell: &alan_shell::Shell,
    agent_path: &str,
) -> Result<Vec<UiEvent>> {
    let raw = read_utf8(shell, &ui_events_path(agent_path)).await?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<UiEvent>(line).context("parse ui event"))
        .collect()
}

fn request_sort_key(request_id: &str) -> u64 {
    request_id
        .trim_start_matches(|ch: char| !ch.is_ascii_digit())
        .parse::<u64>()
        .unwrap_or(0)
}

fn parse_tape_history(raw: &str) -> Vec<HistoryCell> {
    let mut cells = Vec::new();
    for line in raw.lines() {
        let Ok(record) = serde_json::from_str::<TapeRecordV1>(line) else {
            continue;
        };
        if record.kind != "message" {
            continue;
        }
        match record.role.as_str() {
            "user" => cells.push(HistoryCell::User(record.content)),
            "assistant" => match cells.last_mut() {
                Some(HistoryCell::Assistant(text)) => text.push_str(&record.content),
                _ => cells.push(HistoryCell::Assistant(record.content)),
            },
            _ => {}
        }
    }
    cells
}

fn request_snapshot_to_pending_yield(snapshot: RequestSnapshot) -> Result<PendingYieldCell> {
    let payload = if snapshot.options.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str::<Value>(&snapshot.options).unwrap_or(Value::Null)
    };
    let kind = request_yield_kind(&snapshot.kind);
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(&snapshot.prompt)
        .to_string();
    let prompt = match &kind {
        YieldKind::Confirmation => payload
            .get("details")
            .and_then(Value::as_str)
            .map(str::to_string),
        YieldKind::StructuredInput | YieldKind::DynamicTool | YieldKind::Custom(_) => {
            Some(snapshot.prompt.clone())
        }
    };
    let options = payload
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let questions = payload
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| serde_json::from_value::<StructuredInputQuestion>(value.clone()).ok())
        .collect::<Vec<_>>();

    Ok(PendingYieldCell {
        request_id: snapshot.id,
        kind,
        title,
        prompt,
        options,
        default_option: payload
            .get("default_option")
            .and_then(Value::as_str)
            .map(str::to_string),
        questions,
        capability: request_capability(&payload),
        reason: request_reason(&payload),
        presentation: request_presentation(&payload),
    })
}

fn request_yield_kind(kind: &str) -> YieldKind {
    match kind {
        "structured_input" => YieldKind::StructuredInput,
        "dynamic_tool" => YieldKind::DynamicTool,
        "confirmation" => YieldKind::Confirmation,
        other if other.ends_with("_confirmation") => YieldKind::Confirmation,
        other => YieldKind::Custom(other.to_string()),
    }
}

fn request_capability(payload: &Value) -> Option<String> {
    payload
        .get("details")
        .and_then(|details| details.get("capability"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn request_reason(payload: &Value) -> Option<String> {
    payload
        .get("details")
        .and_then(|details| details.get("policy"))
        .and_then(|policy| policy.get("reason"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn request_presentation(payload: &Value) -> Option<ToolResultPresentation> {
    payload
        .get("details")
        .and_then(|details| details.get("presentation"))
        .filter(|value| !value.is_null())
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

fn response_text_from_content(content: Vec<ContentPart>) -> String {
    content
        .iter()
        .map(ContentPart::to_text_lossy)
        .collect::<Vec<_>>()
        .join("")
}

fn sync_actions_from_snapshots(app: &mut FileBackedApp, snapshots: Vec<ActionSnapshot>) {
    let mut running_tools = Vec::new();
    for snapshot in snapshots {
        if action_status_is_running(&snapshot.status) {
            running_tools.push(RunningTool {
                id: snapshot.id.clone(),
                title: action_title(&snapshot),
            });
        }
        if let Some(cell) = action_snapshot_to_history_cell(&snapshot) {
            app.upsert_action_cell(snapshot.id.clone(), cell);
        }
    }
    app.running_tools = running_tools;
}

fn action_status_is_running(status: &str) -> bool {
    matches!(status.trim(), "running" | "pending")
}

fn action_snapshot_to_history_cell(snapshot: &ActionSnapshot) -> Option<HistoryCell> {
    let status = match snapshot.status.trim() {
        "completed" => ToolStatus::Complete,
        "failed" => ToolStatus::Failed,
        _ => return None,
    };
    let body = if !snapshot.output.trim().is_empty() {
        Some(snapshot.output.clone())
    } else if !snapshot.result.trim().is_empty() {
        Some(snapshot.result.clone())
    } else {
        None
    };

    Some(HistoryCell::Tool {
        title: action_title(snapshot),
        status,
        preview: None,
        presentation: body.map(|body| ToolResultPresentation::PlainText { body }),
    })
}

fn action_title(snapshot: &ActionSnapshot) -> String {
    let trimmed = snapshot.name.trim();
    if !trimmed.is_empty() {
        trimmed.to_string()
    } else {
        format!("tool {}", snapshot.id)
    }
}

fn default_commands() -> Vec<CompletionCandidate> {
    [
        ("compact", "summarize context"),
        ("rollback", "undo the last turn"),
        ("clear", "clear the transcript"),
        ("help", "show key bindings"),
        ("quit", "exit alan"),
    ]
    .into_iter()
    .map(|(value, detail)| CompletionCandidate::new(value, Some(detail.to_string())))
    .collect()
}

enum FileBackedEvent {
    Terminal(TerminalEvent),
    Output(String),
    RequestsChanged,
    ActionsChanged,
    Ui(UiEvent),
    Error(String),
}

#[derive(Debug)]
enum FileBackedAction {
    Submit(String),
    Resume {
        request_id: String,
        response: String,
    },
    MachineCtl {
        command: String,
        success_notice: String,
    },
    Interrupt,
    Quit,
}

#[derive(Debug, Clone)]
struct RequestSnapshot {
    id: String,
    kind: String,
    prompt: String,
    options: String,
    status: String,
}

#[derive(Debug, Clone)]
struct ActionSnapshot {
    id: String,
    name: String,
    status: String,
    output: String,
    result: String,
}

struct FileBackedApp {
    agent_path: String,
    composer: Composer,
    transcript: Vec<HistoryCell>,
    action_cells: BTreeMap<String, usize>,
    activity: UiActivitySnapshot,
    plan: UiPlanSnapshot,
    thinking: UiThinkingSnapshot,
    running_tools: Vec<RunningTool>,
    pending_yield: Option<PendingYieldCell>,
    form: Option<FormState>,
    completion: Option<CompletionState>,
    completion_sources: CompletionSources,
    expand_thinking: bool,
    notice: Option<String>,
    should_quit: bool,
}

impl FileBackedApp {
    fn new(agent_path: String) -> Self {
        Self {
            notice: Some(format!("local renderer host attached to {agent_path}")),
            agent_path,
            composer: Composer::default(),
            transcript: Vec::new(),
            action_cells: BTreeMap::new(),
            activity: UiActivitySnapshot::idle(),
            plan: UiPlanSnapshot::empty(),
            thinking: UiThinkingSnapshot::idle(),
            running_tools: Vec::new(),
            pending_yield: None,
            form: None,
            completion: None,
            completion_sources: CompletionSources {
                commands: default_commands(),
                ..CompletionSources::default()
            },
            expand_thinking: false,
            should_quit: false,
        }
    }

    fn set_skill_candidates(&mut self, skills: Vec<CompletionCandidate>) {
        self.completion_sources.skills = skills;
    }

    fn set_file_candidates(&mut self, files: Vec<CompletionCandidate>) {
        self.completion_sources.files = files;
    }

    fn dispatch(&mut self, event: FileBackedEvent) -> Option<FileBackedAction> {
        match event {
            FileBackedEvent::Terminal(TerminalEvent::Key(key)) => self.handle_key(key),
            FileBackedEvent::Terminal(TerminalEvent::Paste(text)) => {
                if let Some(form) = self.form.as_mut() {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        form.insert_char(ch);
                    }
                } else {
                    self.composer.insert_text(&text);
                    self.refresh_completion();
                }
                None
            }
            FileBackedEvent::Terminal(TerminalEvent::Resize(_, _)) => None,
            FileBackedEvent::Terminal(_) => None,
            FileBackedEvent::Output(text) => {
                self.push_output(text);
                None
            }
            FileBackedEvent::Ui(event) => {
                self.apply_ui_event(event);
                None
            }
            FileBackedEvent::RequestsChanged | FileBackedEvent::ActionsChanged => None,
            FileBackedEvent::Error(message) => {
                self.push_error(message);
                None
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        let pending_input = self.form.is_some() || self.pending_yield.is_some();
        if pending_input {
            self.completion = None;
        } else if self.completion.is_some() && self.consume_completion_key(key) {
            return None;
        }
        if self.form.is_some() {
            return self.handle_form_key(key);
        }
        if let Some(action) = self.confirmation_keypress(key) {
            return Some(action);
        }
        match key {
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                Some(FileBackedAction::Quit)
            }
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.expand_thinking = !self.expand_thinking;
                None
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => Some(FileBackedAction::Interrupt),
            KeyEvent {
                code: KeyCode::Char('/'),
                modifiers,
                ..
            } if modifiers.is_empty() && self.composer.text().is_empty() => {
                self.composer.set_text("/");
                self.refresh_completion();
                None
            }
            _ => {
                let outcome = self.composer.handle_key(key);
                self.refresh_completion();
                match outcome {
                    ComposerKeyOutcome::Submit => self.handle_submit(),
                    ComposerKeyOutcome::Interrupt => Some(FileBackedAction::Interrupt),
                    ComposerKeyOutcome::Changed | ComposerKeyOutcome::Ignored => None,
                }
            }
        }
    }

    fn consume_completion_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                if let Some(state) = self.completion.as_mut() {
                    state.move_up();
                }
                true
            }
            KeyCode::Down => {
                if let Some(state) = self.completion.as_mut() {
                    state.move_down();
                }
                true
            }
            KeyCode::Esc => {
                if self.turn_active() || self.pending_yield.is_some() {
                    false
                } else {
                    self.completion = None;
                    true
                }
            }
            KeyCode::Tab => {
                self.accept_completion();
                true
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.accept_completion();
                true
            }
            _ => false,
        }
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        match key.code {
            KeyCode::Esc => Some(FileBackedAction::Interrupt),
            KeyCode::Tab | KeyCode::Down => {
                if let Some(form) = self.form.as_mut() {
                    form.next_field();
                }
                None
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(form) = self.form.as_mut() {
                    form.prev_field();
                }
                None
            }
            KeyCode::Backspace => {
                if let Some(form) = self.form.as_mut() {
                    form.backspace();
                }
                None
            }
            KeyCode::Enter => self.submit_form(),
            KeyCode::Char(ch) => {
                if let Some(form) = self.form.as_mut() {
                    form.insert_char(ch);
                }
                None
            }
            _ => None,
        }
    }

    fn accept_completion(&mut self) {
        let Some(state) = self.completion.take() else {
            return;
        };
        if let Some(candidate) = state.selected_candidate() {
            let value = candidate.value.clone();
            let (new_text, cursor) = completion::apply(self.composer.text(), &state, &value);
            self.composer.set_text_with_cursor(new_text, cursor);
        }
        self.refresh_completion();
    }

    fn refresh_completion(&mut self) {
        if self.pending_yield.is_some() {
            self.completion = None;
            return;
        }
        self.completion = completion::detect(
            self.composer.text(),
            self.composer.cursor(),
            &self.completion_sources,
        );
    }

    fn submit_form(&mut self) -> Option<FileBackedAction> {
        let pending = self.pending_yield.clone()?;
        let form = self.form.as_mut()?;
        match pending.resume_content(&form.answers_json()) {
            Ok(content) => {
                self.form = None;
                Some(FileBackedAction::Resume {
                    request_id: pending.request_id,
                    response: response_text_from_content(content),
                })
            }
            Err(message) => {
                form.error = Some(message);
                None
            }
        }
    }

    fn confirmation_keypress(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
            return None;
        }
        let pending = self.pending_yield.as_ref()?;
        if !matches!(pending.kind, YieldKind::Confirmation) || !self.composer.text().is_empty() {
            return None;
        }
        let KeyCode::Char(ch) = key.code else {
            return None;
        };
        let index = ch.to_digit(10).filter(|digit| *digit >= 1)? as usize - 1;
        let option = pending.options.get(index)?.clone();
        let pending = pending.clone();
        match pending.resume_content(&option) {
            Ok(content) => Some(FileBackedAction::Resume {
                request_id: pending.request_id,
                response: response_text_from_content(content),
            }),
            Err(message) => {
                self.notice = Some(message);
                None
            }
        }
    }

    fn handle_submit(&mut self) -> Option<FileBackedAction> {
        if let Some(pending) = self.pending_yield.clone() {
            let text = self.composer.text().trim().to_string();
            self.composer.set_text("");
            self.completion = None;
            match pending.resume_content(&text) {
                Ok(content) => {
                    return Some(FileBackedAction::Resume {
                        request_id: pending.request_id,
                        response: response_text_from_content(content),
                    });
                }
                Err(message) => {
                    self.composer.set_text(text);
                    self.notice = Some(message);
                    return None;
                }
            }
        }

        let text = self.composer.take_submit()?;
        self.completion = None;
        self.composer.remember(&text);
        if let Some(action) = self.handle_command(&text) {
            return Some(action);
        }
        self.transcript.push(HistoryCell::User(text.clone()));
        Some(FileBackedAction::Submit(text))
    }

    fn handle_command(&mut self, text: &str) -> Option<FileBackedAction> {
        let command = text.strip_prefix('/')?;
        let name = command.split_whitespace().next().unwrap_or("");
        match name {
            "quit" => {
                self.should_quit = true;
                Some(FileBackedAction::Quit)
            }
            "compact" => Some(FileBackedAction::MachineCtl {
                command: "compact".to_string(),
                success_notice: "compact requested".to_string(),
            }),
            "rollback" => Some(FileBackedAction::MachineCtl {
                command: "rollback".to_string(),
                success_notice: "rollback requested".to_string(),
            }),
            "clear" => {
                self.transcript.clear();
                self.action_cells.clear();
                None
            }
            "help" => {
                self.notice = Some(
                    "/compact /rollback /clear /quit · ctrl+r toggle thinking · esc interrupt"
                        .to_string(),
                );
                None
            }
            _ => {
                self.notice = Some(format!("unknown command: /{name}"));
                None
            }
        }
    }

    fn set_pending_yield(&mut self, pending: PendingYieldCell) {
        self.pending_yield = Some(pending.clone());
        self.sync_form();
        self.completion = None;
        // The request watcher can fire on `created:<id>` before the runtime has
        // written kind/prompt/options, so the first sync may insert a sparse
        // cell; later syncs for the same request must update it in place.
        if let Some(existing) = self.transcript.iter_mut().find_map(|cell| match cell {
            HistoryCell::PendingYield(existing) if existing.request_id == pending.request_id => {
                Some(existing)
            }
            _ => None,
        }) {
            *existing = pending;
        } else {
            self.transcript.push(HistoryCell::PendingYield(pending));
        }
    }

    fn clear_pending_yield(&mut self) {
        self.pending_yield = None;
        self.form = None;
        self.refresh_completion();
    }

    fn sync_form(&mut self) {
        match &self.pending_yield {
            Some(pending)
                if matches!(pending.kind, YieldKind::StructuredInput)
                    && pending.questions.len() > 1 =>
            {
                // Rebuild when the question set itself changes (e.g. fields
                // arrived after the request-created event), not just on a new
                // request id — otherwise the form keeps stale questions.
                if self.form.as_ref().is_none_or(|form| {
                    form.request_id != pending.request_id
                        || form.fields.len() != pending.questions.len()
                        || form
                            .fields
                            .iter()
                            .zip(pending.questions.iter())
                            .any(|(field, question)| &field.question != question)
                }) {
                    self.form = Some(FormState::new(
                        pending.request_id.clone(),
                        pending.questions.clone(),
                    ));
                }
            }
            _ => self.form = None,
        }
    }

    fn push_output(&mut self, text: String) {
        if text.is_empty() {
            return;
        }

        match self.transcript.last_mut() {
            Some(HistoryCell::Assistant(existing)) => existing.push_str(&text),
            _ => self.transcript.push(HistoryCell::Assistant(text)),
        }
    }

    fn push_error(&mut self, message: String) {
        self.notice = Some(message.clone());
        self.transcript.push(HistoryCell::Error(message));
    }

    fn apply_ui_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Activity { snapshot } => self.apply_ui_activity_snapshot(snapshot),
            UiEvent::Plan { snapshot } => self.apply_ui_plan_snapshot(snapshot),
            UiEvent::Thinking { snapshot } => self.apply_ui_thinking_snapshot(snapshot),
            UiEvent::Notice { snapshot } => self.apply_ui_notice_snapshot(snapshot),
            UiEvent::Error {
                message,
                recoverable,
            } => {
                if recoverable {
                    self.notice = Some(message);
                } else {
                    self.push_error(message);
                }
            }
        }
    }

    fn apply_ui_activity_snapshot(&mut self, snapshot: UiActivitySnapshot) {
        self.activity = snapshot;
    }

    fn apply_ui_plan_snapshot(&mut self, snapshot: UiPlanSnapshot) {
        let changed = self.plan != snapshot;
        self.plan = snapshot.clone();
        if changed && !snapshot.items.is_empty() {
            self.transcript.push(HistoryCell::Plan(
                snapshot
                    .items
                    .into_iter()
                    .map(|item| crate::history::PlanLine {
                        status: item.status,
                        content: item.content,
                    })
                    .collect(),
            ));
        }
    }

    fn apply_ui_thinking_snapshot(&mut self, snapshot: UiThinkingSnapshot) {
        let changed = self.thinking != snapshot;
        self.thinking = snapshot.clone();
        if changed
            && matches!(snapshot.state, UiThinkingState::Complete)
            && !snapshot.text.trim().is_empty()
        {
            self.transcript.push(HistoryCell::Thinking {
                text: snapshot.text,
                duration_secs: snapshot.duration_secs.unwrap_or(0),
            });
        }
    }

    fn apply_ui_notice_snapshot(&mut self, snapshot: UiNoticeSnapshot) {
        self.notice = match snapshot.kind {
            UiNoticeKind::None => None,
            _ if snapshot.message.trim().is_empty() => None,
            _ => Some(snapshot.message),
        };
    }

    fn activity_label(&self) -> Option<&str> {
        match self.activity.state {
            UiActivityState::Idle => None,
            UiActivityState::Paused => Some("waiting for input"),
            UiActivityState::Running
                if matches!(self.thinking.state, UiThinkingState::Streaming) =>
            {
                Some("thinking")
            }
            UiActivityState::Running => Some("working"),
        }
    }

    fn turn_active(&self) -> bool {
        !matches!(self.activity.state, UiActivityState::Idle)
    }

    fn activity_started_at_ms(&self) -> Option<u64> {
        self.activity.started_at_ms
    }

    fn upsert_action_cell(&mut self, action_id: String, cell: HistoryCell) {
        if let Some(index) = self.action_cells.get(&action_id).copied()
            && let Some(existing) = self.transcript.get_mut(index)
        {
            *existing = cell;
            return;
        }
        let index = self.transcript.len();
        self.transcript.push(cell);
        self.action_cells.insert(action_id, index);
    }

    fn rendered_history_lines(&self, width: usize) -> Vec<String> {
        let opts = self.render_opts(width);
        self.transcript
            .iter()
            .flat_map(|cell| cell.render_lines(opts))
            .collect()
    }

    fn drain_committed_scrollback(
        &mut self,
        viewport_width: usize,
        viewport_height: usize,
    ) -> Vec<String> {
        let opts = self.render_opts(viewport_width);
        let reserved = self.live_region_height(viewport_width) as usize + 1;
        let max_lines = viewport_height.saturating_sub(reserved).max(2);
        let lines = self.rendered_history_lines(viewport_width);
        if lines.len() <= max_lines {
            return Vec::new();
        }
        let drain_count = lines.len() - max_lines;
        let pruned_count = self.prune_rendered_prefix(opts, drain_count);
        lines.into_iter().take(pruned_count).collect()
    }

    fn prune_rendered_prefix(&mut self, opts: RenderOpts, lines_to_prune: usize) -> usize {
        let mut remaining = lines_to_prune;
        let mut cells_to_remove = 0;
        let mut pruned = 0;

        while remaining > 0 && cells_to_remove < self.transcript.len() {
            let cell_lines = self.transcript[cells_to_remove].render_lines(opts).len();
            if cell_lines > remaining {
                break;
            }
            remaining -= cell_lines;
            pruned += cell_lines;
            cells_to_remove += 1;
        }

        if cells_to_remove > 0 {
            self.transcript.drain(0..cells_to_remove);
            self.shift_action_cells(cells_to_remove);
        }

        if remaining > 0
            && let Some(cell) = self.transcript.first_mut()
            && cell.trim_rendered_prefix(opts, remaining)
        {
            pruned += remaining;
        }

        pruned
    }

    fn shift_action_cells(&mut self, removed_prefix_len: usize) {
        self.action_cells = self
            .action_cells
            .iter()
            .filter_map(|(action_id, index)| {
                if *index < removed_prefix_len {
                    None
                } else {
                    Some((action_id.clone(), index - removed_prefix_len))
                }
            })
            .collect();
    }

    fn render_opts(&self, width: usize) -> RenderOpts {
        RenderOpts::new(width, self.expand_thinking)
    }

    fn live_region_height(&self, width: usize) -> u16 {
        let header_lines = 1usize;
        let activity_lines = usize::from(self.activity_label().is_some());
        let notice_lines = usize::from(self.notice.is_some());
        let tool_lines = self.running_tools.len();
        let body_lines = if let Some(form) = &self.form {
            form.render_lines().len()
        } else {
            self.composer_height(width) + self.completion_height() as usize + 1
        };
        (header_lines + activity_lines + notice_lines + tool_lines + body_lines) as u16
    }

    fn completion_height(&self) -> u16 {
        self.completion
            .as_ref()
            .map(|state| state.matches.len().min(MAX_COMPLETION_ROWS) as u16)
            .unwrap_or(0)
    }

    fn composer_height(&self, width: usize) -> usize {
        let width = width.max(1);
        let lines = self
            .composer
            .text()
            .split('\n')
            .map(|line| {
                let visual = unicode_width::UnicodeWidthStr::width(line);
                (visual / width) + 1
            })
            .sum::<usize>()
            .max(1);
        lines.min(MAX_COMPOSER_LINES)
    }

    fn composer_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let segments = self.composer.text().split('\n').collect::<Vec<_>>();
        for (idx, segment) in segments.iter().enumerate() {
            let prompt = if idx == 0 {
                if self.pending_yield.is_some() {
                    "» "
                } else {
                    "> "
                }
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                Span::styled(prompt, Style::default().fg(Color::Green)),
                Span::raw((*segment).to_string()),
            ]));
        }
        if lines.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                if self.pending_yield.is_some() {
                    "» "
                } else {
                    "> "
                },
                Style::default().fg(Color::Green),
            )]));
        }
        lines
    }

    fn hint_line(&self) -> Line<'static> {
        let hint = match &self.pending_yield {
            Some(pending)
                if matches!(pending.kind, YieldKind::Confirmation)
                    && !pending.options.is_empty() =>
            {
                let choices = pending
                    .options
                    .iter()
                    .enumerate()
                    .map(|(idx, option)| format!("{}={option}", idx + 1))
                    .collect::<Vec<_>>()
                    .join(" · ");
                format!("{choices}  · or type a reply and press Enter")
            }
            Some(_) => "reply and press Enter".to_string(),
            None => "enter send · shift+enter newline · / commands · ctrl+r thinking · ctrl+q quit"
                .to_string(),
        };
        Line::styled(hint, Style::default().fg(Color::DarkGray))
    }
}

#[derive(Deserialize)]
struct TapeRecordV1 {
    #[allow(dead_code)]
    version: u16,
    kind: String,
    role: String,
    content: String,
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
    fn proc_ctl_path_requires_concrete_agent_pid() {
        assert_eq!(proc_ctl_path("/agent/7").unwrap(), "/proc/7/ctl");
        assert!(proc_ctl_path("/agent/root").is_err());
        assert!(proc_ctl_path("/agent").is_err());
        assert!(proc_ctl_path("/agent/7/io/output").is_err());
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
    fn transcript_render_matches_daemon_styles() {
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
        assert_eq!(echoed, "hello through files");
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

    #[tokio::test]
    async fn writes_between_tail_open_and_hydration_are_not_lost() {
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

        shell.write(&output_path, b"before").await.unwrap();

        // Startup order under test: tails open first and pin the live edge...
        let mut tails = open_watch_tails(&shell, &agent_path).await.unwrap();

        // ...so a write landing before hydration completes is tail-delivered
        // instead of being skipped as pre-existing.
        shell.write(&output_path, b"after").await.unwrap();
        let mut app = FileBackedApp::new(agent_path.clone());
        hydrate_app_from_files(&shell, &agent_path, &mut app)
            .await
            .unwrap();

        let bytes =
            tokio::time::timeout(std::time::Duration::from_secs(5), tails.output.read(4096))
                .await
                .expect("tail read timed out")
                .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "after",
            "the write between tail-open and hydration must reach the tail"
        );
    }
}
