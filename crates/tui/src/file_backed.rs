use std::path::PathBuf;

#[cfg(test)]
use alan_agent_protocol::{
    ToolResultPresentation, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
    YieldKind,
};
use alan_agent_protocol::{UiActivitySnapshot, UiActivityState, UiEvent};
use alan_ap::InProcessTransport;
use anyhow::{Context, Result, bail};
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
    ActionSnapshot, RequestSnapshot, agent_output_path, parse_tape_history,
    request_snapshot_to_pending_yield, sync_actions_from_snapshots,
};
use file_surface::{
    TapeRecordV1, current_root_agent_pid, hydrate_and_open_tails, reattach_to_current_agent,
    spawn_action_watch, spawn_output_tail, spawn_request_watch, spawn_tape_watch,
    spawn_terminal_events, spawn_ui_watch, sync_actions_from_files, sync_requests_from_files,
    tail_with_history, write_agent_input, write_interrupt, write_machine_ctl,
    write_request_response,
};

use crate::completion::{self, CompletionCandidate};
use crate::composer::{Composer, load_history};
#[cfg(test)]
use crate::history::HistoryCell;
#[cfg(test)]
use crate::history::{PendingYieldCell, RenderOpts, RunningTool, ToolStatus};
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
    let follows_root_agent = config.agent_path == "/agent/root";
    let root_agent_pid = if follows_root_agent {
        current_root_agent_pid(&shell).await?
    } else {
        None
    };
    let mut pending_root_agent_turn: Option<PendingRootAgentTurn> = None;
    let watch_tails = hydrate_and_open_tails(&shell, &config.agent_path, &mut app).await?;

    let mut terminal = TerminalSession::enter()?;
    terminal.draw_with(|frame| draw(frame, &app))?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<FileBackedEvent>(128);
    spawn_terminal_events(tx.clone());

    let mut watchers = AgentWatchers::start(watch_tails, tx.clone(), root_agent_pid);

    let mut frame_tick = tokio::time::interval(std::time::Duration::from_millis(33));
    frame_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut root_agent_pid_tick = tokio::time::interval(std::time::Duration::from_millis(250));
    root_agent_pid_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
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
                                    let submitted_at_ms = unix_time_ms();
                                    let prior_matching_turns =
                                        app.tape_user_prompt_count(&text);
                                    match write_agent_input(&shell, &app.agent_path, &text).await {
                                        Ok(()) => {
                                            app.notice = None;
                                            if follows_root_agent {
                                                pending_root_agent_turn = Some(PendingRootAgentTurn {
                                                    input: text.clone(),
                                                    observed_active: false,
                                                    submitted_at_ms,
                                                    prior_matching_turns,
                                                });
                                                let submitted_task_settled = watchers
                                                    .refresh_root_agent_attachment(
                                                    &shell,
                                                    &config.agent_path,
                                                    &mut app,
                                                    Some((&text, submitted_at_ms, prior_matching_turns)),
                                                    &tx,
                                                    )
                                                    .await;
                                                if submitted_task_settled
                                                    && let Some(turn) =
                                                        pending_root_agent_turn.as_mut()
                                                {
                                                    turn.observed_active = true;
                                                }
                                            }
                                        }
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
                if follows_root_agent {
                    observe_root_agent_activity(
                        &mut pending_root_agent_turn,
                        app.activity.state,
                    );
                }
                dirty = true;
            }
            _ = root_agent_pid_tick.tick(), if follows_root_agent && pending_root_agent_turn.is_some() => {
                let submitted_input = pending_root_agent_turn
                    .as_ref()
                    .map(|turn| {
                        (
                            turn.input.as_str(),
                            turn.submitted_at_ms,
                            turn.prior_matching_turns,
                        )
                    });
                let submitted_task_settled = watchers
                    .refresh_root_agent_attachment(
                    &shell,
                    &config.agent_path,
                    &mut app,
                    submitted_input,
                    &tx,
                    )
                    .await;
                if submitted_task_settled
                    && let Some(turn) = pending_root_agent_turn.as_mut()
                {
                    turn.observed_active = true;
                }
                observe_root_agent_activity(
                    &mut pending_root_agent_turn,
                    app.activity.state,
                );
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

    watchers.stop().await;

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct PendingRootAgentTurn {
    input: String,
    observed_active: bool,
    submitted_at_ms: u64,
    prior_matching_turns: usize,
}

struct AgentWatchers {
    shutdown: tokio::sync::watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<Result<()>>>,
    root_agent_pid: Option<u64>,
    pid_refresh_failed: bool,
}

fn observe_root_agent_activity(
    pending_turn: &mut Option<PendingRootAgentTurn>,
    activity: UiActivityState,
) {
    if let Some(turn) = pending_turn {
        match activity {
            UiActivityState::Running | UiActivityState::Paused => turn.observed_active = true,
            UiActivityState::Idle if turn.observed_active => *pending_turn = None,
            UiActivityState::Idle => {}
        }
    }
}

impl AgentWatchers {
    fn start(
        tails: file_surface::WatchTails,
        tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
        root_agent_pid: Option<u64>,
    ) -> Self {
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
        let tasks = vec![
            tokio::spawn(spawn_output_tail(
                tails.output,
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(spawn_request_watch(
                tails.requests,
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(spawn_action_watch(
                tails.actions,
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(spawn_ui_watch(tails.ui, tx.clone(), shutdown_rx.clone())),
            tokio::spawn(spawn_tape_watch(tails.tape, tx, shutdown_rx)),
        ];
        Self {
            shutdown,
            tasks,
            root_agent_pid,
            pid_refresh_failed: false,
        }
    }

    async fn refresh_root_agent_attachment(
        &mut self,
        shell: &alan_shell::Shell,
        agent_path: &str,
        app: &mut FileBackedApp,
        submitted_task: Option<(&str, u64, usize)>,
        tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    ) -> bool {
        match current_root_agent_pid(shell).await {
            Ok(Some(pid)) if self.root_agent_pid != Some(pid) => {
                self.pid_refresh_failed = false;
                self.stop().await;
                match reattach_to_current_agent(shell, agent_path, app, submitted_task).await {
                    Ok((tails, submitted_task_settled)) => {
                        *self = Self::start(tails, tx.clone(), Some(pid));
                        submitted_task_settled
                    }
                    Err(err) => {
                        self.root_agent_pid = None;
                        app.push_error(format!("Root Agent reattach failed: {err:#}"));
                        false
                    }
                }
            }
            Ok(pid) => {
                self.root_agent_pid = pid;
                self.pid_refresh_failed = false;
                false
            }
            Err(err) if !self.pid_refresh_failed => {
                self.pid_refresh_failed = true;
                app.push_error(format!("Root Agent identity refresh failed: {err:#}"));
                false
            }
            Err(_) => false,
        }
    }

    async fn stop(&mut self) {
        let _ = self.shutdown.send(true);
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
    }
}

/// Run one task from redirected stdin and write its final Agent answer to stdout.
pub async fn run_stdio_task(
    root_transport: InProcessTransport,
    agent_path: impl Into<String>,
    input: &str,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    if input.trim().is_empty() {
        bail!("stdin did not contain an Agent task");
    }

    let agent_path = agent_path.into();
    let shell = alan_shell::Shell::new(root_transport);
    let mut root_agent_pid = current_root_agent_pid(&shell).await?;
    let activity_path = format!("{agent_path}/machine/ui/activity");
    let activity: UiActivitySnapshot = serde_json::from_slice(
        &shell
            .cat(&activity_path)
            .await
            .map_err(|err| anyhow::anyhow!("read Agent activity failed: {err:?}"))?,
    )
    .map_err(|err| anyhow::anyhow!("parse Agent activity failed: {err}"))?;
    match activity.state {
        UiActivityState::Idle => {}
        UiActivityState::Running => {
            bail!("Root Agent is already working; retry after it finishes")
        }
        UiActivityState::Paused => {
            bail!("Root Agent is waiting for interactive input; use the TTY renderer")
        }
    }

    // ponytail: the CLI lock excludes concurrent one-shot clients; add IDs if
    // one-shot and interactive submissions need concurrent task correlation.
    let tape_path = format!("{agent_path}/machine/tape");
    let ui_events_path = format!("{agent_path}/machine/ui/events");
    let (mut tape_tail, baseline_tape_history) = tail_with_history(&shell, &tape_path).await?;
    let (mut ui_tail, _) = tail_with_history(&shell, &ui_events_path).await?;
    let task = StdioTaskWaitContext::new(input, &baseline_tape_history);

    let result = wait_for_stdio_answer(
        &shell,
        &agent_path,
        task,
        &mut tape_tail,
        &mut ui_tail,
        &mut root_agent_pid,
        async { tokio::signal::ctrl_c().await.map_err(anyhow::Error::from) },
    )
    .await;
    let tape_close = tape_tail.close().await;
    let ui_close = ui_tail.close().await;
    let answer = result?;
    tape_close.map_err(|err| anyhow::anyhow!("close Agent tape tail failed: {err:?}"))?;
    ui_close.map_err(|err| anyhow::anyhow!("close Agent UI tail failed: {err:?}"))?;

    let mut stdout = tokio::io::stdout();
    stdout.write_all(answer.as_bytes()).await?;
    if !answer.ends_with('\n') {
        stdout.write_all(b"\n").await?;
    }
    stdout.flush().await?;
    Ok(())
}

async fn wait_for_stdio_answer(
    shell: &alan_shell::Shell,
    agent_path: &str,
    task: StdioTaskWaitContext<'_>,
    tape_tail: &mut alan_shell::Tail,
    ui_tail: &mut alan_shell::Tail,
    root_agent_pid: &mut Option<u64>,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<String> {
    tokio::pin!(interrupt);
    let mut tape_pending = Vec::new();
    let mut ui_pending = Vec::new();
    let mut interrupt_requested = false;
    let mut snapshot = StdioTaskSnapshot {
        task_started: false,
        assistant_answer: None,
        activity_state: None,
        task_error: None,
    };
    let mut root_agent_pid_tick = tokio::time::interval(std::time::Duration::from_millis(250));
    root_agent_pid_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    write_agent_input(shell, agent_path, task.input).await?;

    loop {
        tokio::select! {
            bytes = tape_tail.read(4096) => {
                let bytes = bytes.map_err(|err| anyhow::anyhow!("read Agent tape failed: {err:?}"))?;
                if bytes.is_empty() {
                    bail!("Agent tape closed before the task completed");
                }
                tape_pending.extend_from_slice(&bytes);
                for line in drain_lines(&mut tape_pending) {
                    let Ok(record) = serde_json::from_slice::<TapeRecordV1>(&line) else {
                        continue;
                    };
                    if record.kind != "message" {
                        continue;
                    }
                    if !snapshot.task_started {
                        snapshot.task_started =
                            record.role == "user" && record.content == task.input;
                    } else if record.role == "assistant" {
                        snapshot.assistant_answer = Some(record.content);
                    }
                }
                if let Some(answer) = finish_stdio_task_if_ready(&mut snapshot)? {
                    return Ok(answer);
                }
            }
            bytes = ui_tail.read(4096) => {
                let bytes = bytes.map_err(|err| anyhow::anyhow!("read Agent UI events failed: {err:?}"))?;
                if bytes.is_empty() {
                    bail!("Agent UI event stream closed before the task completed");
                }
                ui_pending.extend_from_slice(&bytes);
                for line in drain_lines(&mut ui_pending) {
                    let event = serde_json::from_slice::<UiEvent>(&line)
                        .map_err(|err| anyhow::anyhow!("parse Agent UI event failed: {err}"))?;
                    match event {
                        UiEvent::Activity { snapshot: activity } => {
                            if activity.state == UiActivityState::Running
                                && activity
                                    .started_at_ms
                                    .is_none_or(|started_at| started_at >= task.submitted_at_ms)
                            {
                                snapshot.task_started = true;
                                snapshot.task_error = None;
                            }
                            snapshot.activity_state = Some(activity.state)
                        }
                        UiEvent::Error { message, .. } if snapshot.task_started => {
                            snapshot.task_error = Some(message)
                        }
                        UiEvent::Error { .. } => {}
                        UiEvent::Plan { .. } | UiEvent::Thinking { .. } | UiEvent::Notice { .. } => {}
                    }
                }
                if interrupt_requested
                    && interrupt_stdio_task_if_active(shell, agent_path, &snapshot).await?
                {
                    bail!("Agent task interrupted");
                }
                if snapshot.activity_state == Some(UiActivityState::Paused) {
                    bail!("Agent task needs interactive input; attach with the TTY renderer");
                }
                if let Some(answer) = finish_stdio_task_if_ready(&mut snapshot)? {
                    return Ok(answer);
                }
            }
            _ = root_agent_pid_tick.tick() => {
                if let Some(pid) = current_root_agent_pid(shell).await?
                    && *root_agent_pid != Some(pid)
                {
                    let (new_tape_tail, tape_history) =
                        tail_with_history(shell, &format!("{agent_path}/machine/tape")).await?;
                    let (new_ui_tail, ui_history) =
                        tail_with_history(shell, &format!("{agent_path}/machine/ui/events")).await?;
                    std::mem::replace(tape_tail, new_tape_tail).close().await
                        .map_err(|err| anyhow::anyhow!("close old Agent tape tail failed: {err:?}"))?;
                    std::mem::replace(ui_tail, new_ui_tail).close().await
                        .map_err(|err| anyhow::anyhow!("close old Agent UI tail failed: {err:?}"))?;

                    let recovered = stdio_task_snapshot(
                        shell,
                        agent_path,
                        task,
                        &tape_history,
                        &ui_history,
                    )
                    .await?;
                    tape_pending.clear();
                    ui_pending.clear();
                    *root_agent_pid = Some(pid);
                    snapshot = recovered;

                    if interrupt_requested
                        && interrupt_stdio_task_if_active(shell, agent_path, &snapshot).await?
                    {
                        bail!("Agent task interrupted");
                    }
                    if snapshot.activity_state == Some(UiActivityState::Paused) {
                        bail!("Agent task needs interactive input; attach with the TTY renderer");
                    }
                    if let Some(answer) = finish_stdio_task_if_ready(&mut snapshot)? {
                        return Ok(answer);
                    }
                    if snapshot.activity_state == Some(UiActivityState::Idle) {
                        bail!("Root Agent changed before the submitted task outcome could be recovered; outcome is unknown");
                    }
                }
            }
            signal = &mut interrupt, if !interrupt_requested => {
                signal?;
                interrupt_requested = true;
                if interrupt_stdio_task_if_active(shell, agent_path, &snapshot).await? {
                    bail!("Agent task interrupted");
                }
            }
        }
    }
}

async fn interrupt_stdio_task_if_active(
    shell: &alan_shell::Shell,
    agent_path: &str,
    snapshot: &StdioTaskSnapshot,
) -> Result<bool> {
    if !matches!(
        snapshot.activity_state,
        Some(UiActivityState::Running | UiActivityState::Paused)
    ) {
        return Ok(false);
    }
    write_machine_ctl(shell, agent_path, "interrupt")
        .await
        .context("failed to send Agent task interrupt")?;
    Ok(true)
}

fn finish_stdio_task_if_ready(snapshot: &mut StdioTaskSnapshot) -> Result<Option<String>> {
    if !snapshot.task_started || snapshot.activity_state != Some(UiActivityState::Idle) {
        return Ok(None);
    }
    if let Some(message) = snapshot.task_error.take() {
        bail!("Agent task failed: {message}");
    }
    if let Some(answer) = snapshot.assistant_answer.take() {
        return Ok(Some(answer));
    }
    Ok(None)
}

struct StdioTaskSnapshot {
    task_started: bool,
    assistant_answer: Option<String>,
    activity_state: Option<UiActivityState>,
    task_error: Option<String>,
}

#[derive(Clone, Copy)]
struct StdioTaskWaitContext<'a> {
    input: &'a str,
    baseline_tape_history: &'a [u8],
    submitted_at_ms: u64,
}

impl<'a> StdioTaskWaitContext<'a> {
    fn new(input: &'a str, baseline_tape_history: &'a [u8]) -> Self {
        Self {
            input,
            baseline_tape_history,
            submitted_at_ms: unix_time_ms(),
        }
    }
}

async fn stdio_task_snapshot(
    shell: &alan_shell::Shell,
    agent_path: &str,
    task: StdioTaskWaitContext<'_>,
    tape_history: &[u8],
    ui_history: &[u8],
) -> Result<StdioTaskSnapshot> {
    let mut snapshot = stdio_task_snapshot_from_history(task, tape_history, ui_history)?;
    if snapshot.activity_state.is_none() {
        let raw = shell
            .cat(&format!("{agent_path}/machine/ui/activity"))
            .await
            .map_err(|err| anyhow::anyhow!("read Agent activity failed: {err:?}"))?;
        let activity =
            serde_json::from_slice::<UiActivitySnapshot>(&raw).context("parse Agent activity")?;
        if matches!(
            activity.state,
            UiActivityState::Running | UiActivityState::Paused
        ) && activity
            .started_at_ms
            .is_some_and(|started_at| started_at >= task.submitted_at_ms)
        {
            snapshot.task_started = true;
            snapshot.task_error = None;
        }
        snapshot.activity_state = Some(activity.state);
    }
    Ok(snapshot)
}

fn stdio_task_snapshot_from_history(
    task: StdioTaskWaitContext<'_>,
    tape_history: &[u8],
    ui_history: &[u8],
) -> Result<StdioTaskSnapshot> {
    let tape_history = std::str::from_utf8(tape_history).context("machine/tape is not utf8")?;
    let records = tape_history
        .lines()
        .filter_map(|line| serde_json::from_str::<TapeRecordV1>(line).ok())
        .filter(|record| record.kind == "message")
        .collect::<Vec<_>>();
    let matching_task_indices = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            (record.role == "user" && record.content == task.input).then_some(index)
        })
        .collect::<Vec<_>>();
    let baseline_record_count = std::str::from_utf8(task.baseline_tape_history)
        .context("baseline machine/tape is not utf8")?
        .lines()
        .filter_map(|line| serde_json::from_str::<TapeRecordV1>(line).ok())
        .filter(|record| record.kind == "message")
        .count();
    let task_index = tape_history
        .as_bytes()
        .starts_with(task.baseline_tape_history)
        .then(|| {
            matching_task_indices
                .iter()
                .copied()
                .rfind(|index| *index >= baseline_record_count)
        })
        .flatten();
    let assistant_answer = task_index.and_then(|index| {
        let following = &records[index + 1..];
        let turn_end = following
            .iter()
            .position(|record| record.role == "user")
            .unwrap_or(following.len());
        following[..turn_end]
            .iter()
            .rev()
            .find(|record| record.role == "assistant")
            .map(|record| record.content.clone())
    });

    let ui_task = file_surface::correlated_ui_task(ui_history, task.submitted_at_ms)?;
    Ok(StdioTaskSnapshot {
        task_started: task_index.is_some() || ui_task.started,
        assistant_answer,
        activity_state: ui_task.state,
        task_error: ui_task.error,
    })
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn drain_lines(pending: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
        let mut line = pending.drain(..=newline).collect::<Vec<_>>();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
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
            format!(" · ctrl+c/esc interrupt · {elapsed}s"),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

#[cfg(test)]
mod stdio_tests;
#[cfg(test)]
mod tests;
