use std::collections::VecDeque;
use std::path::PathBuf;

#[cfg(test)]
use alan_agent_protocol::UiActivitySnapshot;
#[cfg(test)]
use alan_agent_protocol::{
    ToolResultPresentation, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
    YieldKind,
};
use alan_agent_protocol::{UiActivityState, UiEvent};
use alan_ap::InProcessTransport;
use anyhow::{Context, Result, bail};
#[cfg(test)]
use crossterm::event::KeyEvent;
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyModifiers};
#[cfg(test)]
use ratatui::style::Color;
mod app;
mod file_surface;
mod history_merge;
mod interrupt;
mod layout;
mod previous_input;
mod stdio_completion;
mod tail;

use app::{FileBackedAction, FileBackedApp, FileBackedEvent};
use interrupt::{
    PendingRootAgentTurn, observe_root_agent_completion, send_interrupt,
    settle_unknown_replaced_input,
};
use previous_input::discard_superseded_attachment_events;

#[cfg(test)]
use file_surface::{
    ActionSnapshot, RequestSnapshot, agent_output_path, parse_tape_history,
    request_snapshot_to_pending_yield, sync_action_snapshot,
};
use file_surface::{
    TapeRecordV1, hydrate_and_open_tails, reattach_to_current_agent, spawn_action_watch,
    spawn_output_tail, spawn_request_watch, spawn_tape_watch, spawn_terminal_events,
    spawn_ui_watch, sync_action_from_file, sync_requests_from_files, write_agent_input,
    write_machine_ctl, write_request_response,
};
use layout::{draw, history_prefix_to_drain, inline_viewport_height, live_region_height};
use tail::{
    StdioTailAttachment, close_stdio_tails, current_root_agent_pid,
    open_stdio_tail_attachment_for_submit, root_agent_path_for_pid,
};

use crate::completion::CompletionCandidate;
use crate::composer::{Composer, load_history};
#[cfg(test)]
use crate::history::HistoryCell;
#[cfg(test)]
use crate::history::{PendingYieldCell, RenderOpts, RunningTool, ToolStatus};
use crate::terminal::{TerminalSession, terminal_capability_error};
const MAX_COMPOSER_LINES: usize = 10;
const MAX_COMPLETION_ROWS: usize = 6;
const SPINNER: [&str; 10] = ["|", "/", "-", "\\", "|", "/", "-", "\\", "|", "/"];

/// Configuration for rendering an Agent Process through a mounted file-backed surface.
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
    /// Create a renderer configuration for a mounted Agent Process.
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

/// Run the inline renderer for a mounted Agent Process.
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
    let mut pending_root_agent_turn: Option<PendingRootAgentTurn> = None;
    let watch_tails = hydrate_and_open_tails(&shell, &config.agent_path, &mut app).await?;

    let mut terminal = TerminalSession::enter()?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<FileBackedEvent>(128);
    let terminal_reader = spawn_terminal_events(tx.clone());

    let mut watchers = AgentWatchers::start(watch_tails, &config.agent_path, tx.clone());

    let mut frame_tick = tokio::time::interval(std::time::Duration::from_millis(33));
    frame_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut root_agent_pid_tick = tokio::time::interval(std::time::Duration::from_millis(250));
    root_agent_pid_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = true;

    loop {
        tokio::select! {
            event = receive_file_backed_event(&mut watchers.pending_terminal_events, &mut rx) => {
                let Some(event) = event else {
                    break;
                };
                if let FileBackedEvent::Ui(ref event) = event {
                    observe_root_agent_completion(&mut pending_root_agent_turn, event, &mut app);
                }
                match event {
                    FileBackedEvent::RequestsChanged => {
                        if let Err(err) = sync_requests_from_files(&shell, &app.agent_path.clone(), &mut app).await {
                            app.push_error(format!("request refresh failed: {err:#}"));
                        }
                    }
                    FileBackedEvent::ActionsChanged { agent_path, action_id } => {
                        if let Err(err) = sync_action_from_file(
                            &shell,
                            &agent_path,
                            &action_id,
                            &mut app,
                        )
                        .await
                        {
                            app.push_error(format!("action refresh failed: {err:#}"));
                        }
                    }
                    other => {
                        let submission_requested = follows_root_agent
                            && matches!(
                                &other,
                                FileBackedEvent::Terminal(TerminalEvent::Key(key))
                                    if key.code == KeyCode::Enter
                                        && !key.modifiers.contains(KeyModifiers::SHIFT)
                            )
                            && app.enter_submits_agent_task();
                        let blocked_submission = if submission_requested && pending_root_agent_turn.is_some() {
                            app.push_error("submit blocked: waiting for this input to complete".to_string());
                            true
                        } else {
                            false
                        };
                        if !blocked_submission
                            && let Some(action) = app.dispatch(other)
                        {
                            match action {
                                FileBackedAction::Submit(text) => {
                                    let submitted_at_ms = unix_time_ms();
                                    let record = alan_agent_protocol::UserInputRecord::new(
                                        alan_agent_protocol::InputIntent::Agent,
                                        alan_agent_protocol::InputMode::FollowUp,
                                        &text,
                                    );
                                    match write_agent_input(&shell, &app.agent_path, watchers.root_agent_pid, &record).await {
                                        Ok(()) => {
                                            app.notice = None;
                                            if follows_root_agent {
                                                pending_root_agent_turn = Some(PendingRootAgentTurn {
                                                    input: text.clone(),
                                                    submission_id: record.submission_id.clone(),
                                                    submitted_process: watchers.root_agent_pid,
                                                    submitted_at_ms,
                                                });
                                                let submitted_task_settled = watchers
                                                    .refresh_root_agent_attachment(
                                                    &shell,
                                                    &config.agent_path,
                                                    &mut app,
                                                    &mut rx,
                                                    Some((&text, submitted_at_ms, &record.submission_id)),
                                                    &tx,
                                                    )
                                                    .await;
                                                if submitted_task_settled {
                                                    pending_root_agent_turn = None;
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
                                    send_interrupt(&shell, &mut app, pending_root_agent_turn.as_ref(), watchers.root_agent_pid).await;
                                }
                                FileBackedAction::Quit => break,
                            }
                        }
                    }
                }
                if follows_root_agent {
                    settle_unknown_replaced_input(&mut pending_root_agent_turn, watchers.root_agent_pid, &mut app);
                }
                dirty = true;
            }
            _ = root_agent_pid_tick.tick(), if follows_root_agent => {
                let previous_pid = watchers.root_agent_pid;
                let previous_refresh_failed = watchers.pid_refresh_failed;
                let previous_turn = pending_root_agent_turn.clone();
                let submitted_input = pending_root_agent_turn
                    .as_ref()
                    .map(|turn| {
                        (
                            turn.input.as_str(),
                            turn.submitted_at_ms,
                            turn.submission_id.as_str(),
                        )
                    });
                let submitted_task_settled = watchers
                    .refresh_root_agent_attachment(
                    &shell,
                    &config.agent_path,
                    &mut app,
                    &mut rx,
                    submitted_input,
                    &tx,
                    )
                    .await;
                if submitted_task_settled {
                    pending_root_agent_turn = None;
                }
                settle_unknown_replaced_input(&mut pending_root_agent_turn, watchers.root_agent_pid, &mut app);
                if previous_pid != watchers.root_agent_pid
                    || previous_refresh_failed != watchers.pid_refresh_failed
                    || previous_turn != pending_root_agent_turn
                {
                    dirty = true;
                }
            }
            _ = frame_tick.tick() => {
                if dirty {
                    let (viewport_width, terminal_height) = terminal.viewport_size();
                    let committed = app.drain_committed_scrollback(viewport_width, terminal_height);
                    terminal.write_scrollback(&committed)?;
                    terminal.set_inline_height(inline_viewport_height(
                        &app,
                        viewport_width,
                        terminal_height,
                    ))?;
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
    drop(rx);
    terminal_reader
        .await
        .context("terminal reader task failed")?;

    Ok(())
}

async fn receive_file_backed_event(
    pending_terminal_events: &mut VecDeque<FileBackedEvent>,
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
) -> Option<FileBackedEvent> {
    match pending_terminal_events.pop_front() {
        Some(event) => Some(event),
        None => rx.recv().await,
    }
}

struct AgentWatchers {
    recovery: Option<(alan_shell::Tail, alan_shell::Tail)>,
    shutdown: tokio::sync::watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<Result<()>>>,
    root_agent_pid: Option<u64>,
    pid_refresh_failed: bool,
    pending_terminal_events: VecDeque<FileBackedEvent>,
}

impl AgentWatchers {
    fn start(
        tails: file_surface::WatchTails,
        agent_path: &str,
        tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    ) -> Self {
        let root_agent_pid = tails.root_agent_pid;
        let action_agent_path = root_agent_pid
            .and_then(|pid| root_agent_path_for_pid(agent_path, pid))
            .unwrap_or_else(|| agent_path.to_string());
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
                action_agent_path,
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(spawn_ui_watch(tails.ui, tx.clone(), shutdown_rx.clone())),
            tokio::spawn(spawn_tape_watch(tails.tape, tx, shutdown_rx)),
        ];
        Self {
            recovery: Some((tails.recovery_ui, tails.recovery_tape)),
            shutdown,
            tasks,
            root_agent_pid,
            pid_refresh_failed: false,
            pending_terminal_events: VecDeque::new(),
        }
    }

    async fn refresh_root_agent_attachment(
        &mut self,
        shell: &alan_shell::Shell,
        agent_path: &str,
        app: &mut FileBackedApp,
        rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
        submitted_task: Option<(&str, u64, &str)>,
        tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    ) -> bool {
        match current_root_agent_pid(shell).await {
            Ok(Some(pid)) if self.root_agent_pid != Some(pid) => {
                // Tape history outlives the local pending-input lock.
                let history_restored = if let Some((_, tape)) = &self.recovery {
                    previous_input::restore_tape_history(app, tape, app.tape_consumed_offset).await
                } else {
                    false
                };
                let retained = self.stop_for_input(submitted_task).await;
                app.expected_terminal_error = None;
                let queued = discard_superseded_attachment_events(
                    rx,
                    &mut self.pending_terminal_events,
                    submitted_task.map(|(_, _, id)| id),
                );
                let completion = retained.0.or(queued.0);
                if !history_restored
                    && let Some(answer) = retained.1.or(queued.1)
                    && let Some((input, _, _)) = submitted_task
                {
                    previous_input::restore_answer(app, input, answer);
                }
                let settled = match reattach_to_current_agent(
                    shell,
                    agent_path,
                    app,
                    submitted_task.filter(|_| completion.is_none()),
                )
                .await
                {
                    Ok((tails, submitted_task_settled)) => {
                        self.pid_refresh_failed = false;
                        let pending_terminal_events =
                            std::mem::take(&mut self.pending_terminal_events);
                        *self = Self::start(tails, agent_path, tx.clone());
                        self.pending_terminal_events = pending_terminal_events;
                        submitted_task_settled
                    }
                    Err(err) => {
                        self.root_agent_pid = None;
                        if !self.pid_refresh_failed {
                            app.push_error(format!("Root Agent reattach failed: {err:#}"));
                        }
                        self.pid_refresh_failed = true;
                        false
                    }
                };
                if let Some(event) = completion {
                    interrupt::render_input_completion(&event, app);
                    true
                } else {
                    settled
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
        self.stop_for_input(None).await;
    }

    async fn stop_for_input(
        &mut self,
        submitted: Option<(&str, u64, &str)>,
    ) -> (Option<UiEvent>, Option<String>) {
        let _ = self.shutdown.send(true);
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
        let Some((ui, tape)) = self.recovery.take() else {
            return (None, None);
        };
        let outcome = previous_input::snapshot(&ui, &tape, submitted).await;
        let _ = ui.close().await;
        let _ = tape.close().await;
        outcome
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
    let mut attachment = open_stdio_tail_attachment_for_submit(&shell, &agent_path).await?;

    let task = StdioTaskWaitContext::new(input);

    let result = wait_for_stdio_answer(&shell, task, &mut attachment, async {
        tokio::signal::ctrl_c().await.map_err(anyhow::Error::from)
    })
    .await;
    let close_result = close_stdio_tails(attachment.tape_tail, attachment.ui_tail).await;
    let answer = result?;
    close_result?;

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
    task: StdioTaskWaitContext,
    attachment: &mut StdioTailAttachment,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<String> {
    submit_stdio_task(shell, &task, attachment).await?;
    wait_for_stdio_answer_after_submit(shell, task, attachment, interrupt).await
}

async fn submit_stdio_task(
    shell: &alan_shell::Shell,
    task: &StdioTaskWaitContext,
    attachment: &StdioTailAttachment,
) -> Result<()> {
    write_agent_input(
        shell,
        "/agent/root",
        Some(attachment.root_agent_pid),
        &task.record,
    )
    .await
}

async fn wait_for_stdio_answer_after_submit(
    shell: &alan_shell::Shell,
    task: StdioTaskWaitContext,
    attachment: &mut StdioTailAttachment,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<String> {
    tokio::pin!(interrupt);
    let mut tape_pending = Vec::new();
    let mut ui_pending = Vec::new();
    let mut interrupt_requested = false;
    let mut snapshot = StdioTaskSnapshot {
        task_started: false,
        waiting_for_response: false,
        assistant_answer: None,
        activity_state: None,
        task_error: None,
        completion: None,
    };
    let mut root_agent_pid_tick = tokio::time::interval(std::time::Duration::from_millis(250));
    root_agent_pid_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            bytes = attachment.tape_tail.read(4096) => {
                let bytes = match bytes {
                    Ok(bytes) if !bytes.is_empty() => bytes,
                    result => {
                        let error = match result {
                            Ok(_) => anyhow::anyhow!("Agent tape closed before the task completed"),
                            Err(err) => anyhow::anyhow!("read Agent tape failed: {err:?}"),
                        };
                        return Err(error.context("task outcome is unknown"));
                    }
                };
                tape_pending.extend_from_slice(&bytes);
                for line in drain_lines(&mut tape_pending) {
                    let Ok(record) = serde_json::from_slice::<TapeRecordV1>(&line) else {
                        continue;
                    };
                    if record.kind != "message" {
                        continue;
                    }
                    if record.belongs_to(&task.record.submission_id) {
                        snapshot.task_started = true;
                        if record.role == "assistant" {
                            snapshot.assistant_answer = Some(record.content);
                        }
                    }
                }
                if let Some(answer) = finish_stdio_task_if_ready(&mut snapshot)? {
                    return Ok(answer);
                }
            }
            bytes = attachment.ui_tail.read(4096) => {
                let bytes = match bytes {
                    Ok(bytes) if !bytes.is_empty() => bytes,
                    result => {
                        let error = match result {
                            Ok(_) => anyhow::anyhow!("Agent UI event stream closed before the task completed"),
                            Err(err) => anyhow::anyhow!("read Agent UI events failed: {err:?}"),
                        };
                        return Err(error.context("task outcome is unknown"));
                    }
                };
                ui_pending.extend_from_slice(&bytes);
                for line in drain_lines(&mut ui_pending) {
                    let event = serde_json::from_slice::<UiEvent>(&line)
                        .map_err(|err| anyhow::anyhow!("parse Agent UI event failed: {err}"))?;
                    match event {
                        event @ UiEvent::Activity { .. } => {
                            stdio_completion::observe_event(&task.record.submission_id, &mut snapshot, event);
                        }
                        event @ UiEvent::InputCompleted { .. } => {
                            stdio_completion::observe_event(&task.record.submission_id, &mut snapshot, event);
                        }
                        UiEvent::Error { .. } | UiEvent::Plan { .. } | UiEvent::Thinking { .. } | UiEvent::Notice { .. } => {}
                    }
                }
                if interrupt_requested
                    && interrupt_stdio_task_if_pending(shell, &attachment.agent_process_path, &task.record.submission_id, &snapshot).await?
                {
                    bail!("Agent input cancellation requested");
                }
                if snapshot.completion.is_none() && snapshot.waiting_for_response {
                    bail!("Agent task needs interactive input; attach with the TTY renderer");
                }
                let refresh_result = {
                    let refresh = stdio_completion::refresh_answer_after_completion(
                        shell,
                        &attachment.agent_process_path,
                        &task,
                        &mut snapshot,
                    );
                    tokio::pin!(refresh);
                    loop {
                        tokio::select! {
                            result = &mut refresh => break result,
                            _ = root_agent_pid_tick.tick() => {
                                tail::require_stdio_attachment_current(shell, attachment).await?;
                            }
                        }
                    }
                };
                refresh_result.context("final task outcome is unknown")?;
                if let Some(answer) = finish_stdio_task_if_ready(&mut snapshot)? {
                    return Ok(answer);
                }
            }
            _ = root_agent_pid_tick.tick() => {
                tail::require_stdio_attachment_current(shell, attachment).await?;
            }
            signal = &mut interrupt, if !interrupt_requested => {
                signal?;
                interrupt_requested = true;
                if interrupt_stdio_task_if_pending(shell, &attachment.agent_process_path, &task.record.submission_id, &snapshot).await? {
                    bail!("Agent input cancellation requested");
                }
            }
        }
    }
}

async fn interrupt_stdio_task_if_pending(
    shell: &alan_shell::Shell,
    agent_path: &str,
    submission_id: &str,
    snapshot: &StdioTaskSnapshot,
) -> Result<bool> {
    if snapshot.completion.is_some() {
        return Ok(false);
    }
    write_machine_ctl(
        shell,
        agent_path,
        &format!("queue-v1 interrupt {submission_id}"),
    )
    .await
    .context("failed to send Agent task interrupt")?;
    Ok(true)
}

fn finish_stdio_task_if_ready(snapshot: &mut StdioTaskSnapshot) -> Result<Option<String>> {
    if snapshot.completion.is_none() {
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
    waiting_for_response: bool,
    assistant_answer: Option<String>,
    activity_state: Option<UiActivityState>,
    task_error: Option<String>,
    completion: Option<alan_agent_protocol::UiInputStatus>,
}

struct StdioTaskWaitContext {
    record: alan_agent_protocol::UserInputRecord,
    #[cfg(test)]
    submitted_at_ms: u64,
}

impl StdioTaskWaitContext {
    fn new(input: &str) -> Self {
        Self {
            record: alan_agent_protocol::UserInputRecord::new(
                alan_agent_protocol::InputIntent::Agent,
                alan_agent_protocol::InputMode::FollowUp,
                input,
            ),
            #[cfg(test)]
            submitted_at_ms: unix_time_ms(),
        }
    }
}

#[cfg(test)]
fn stdio_task_snapshot_from_history(
    task: &StdioTaskWaitContext,
    tape_history: &[u8],
    ui_history: &[u8],
) -> Result<StdioTaskSnapshot> {
    let (tape_task_started, assistant_answer) =
        stdio_completion::tape_outcome(&task.record.submission_id, tape_history)?;
    let ui_task = file_surface::correlated_ui_task(ui_history, task.submitted_at_ms)?;
    let mut snapshot = StdioTaskSnapshot {
        task_started: tape_task_started,
        waiting_for_response: false,
        assistant_answer,
        activity_state: ui_task.state,
        task_error: None,
        completion: None,
    };
    for line in std::str::from_utf8(ui_history)
        .context("UI history is not utf8")?
        .lines()
    {
        let event = serde_json::from_str::<UiEvent>(line).context("parse Agent UI history")?;
        stdio_completion::observe_event(&task.record.submission_id, &mut snapshot, event);
    }
    Ok(snapshot)
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

#[cfg(test)]
#[path = "file_backed/stdio_idle_tests.rs"]
mod stdio_idle_tests;
#[cfg(test)]
mod stdio_tests;
#[cfg(test)]
mod tests;
