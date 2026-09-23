use std::path::{Path, PathBuf};
use std::{
    fs::OpenOptions,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
};

#[cfg(test)]
use alan_agent_protocol::{
    ToolResultPresentation, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
    YieldKind,
};
use alan_agent_protocol::{UiActivitySnapshot, UiActivityState, UiEvent};
use alan_ap::InProcessTransport;
use anyhow::{Context, Result, bail};
#[cfg(test)]
use crossterm::event::KeyEvent;
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
mod app;
mod file_surface;
mod history_merge;
mod interrupt;
mod stdio_completion;
mod submission;
mod tail;

use app::{FileBackedAction, FileBackedApp, FileBackedEvent};
use interrupt::{
    PendingRootAgentTurn, observe_root_agent_activity, request_pending_root_interrupt,
    send_interrupt,
};
use submission::{prepare_root_agent_submission, require_root_agent_idle};

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
use tail::{
    StdioTailAttachment, close_stdio_tails, current_root_agent_pid,
    open_stdio_tail_attachment_when_idle, root_agent_path_for_pid,
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
    /// Shared lock path for serializing Root Agent submissions across clients.
    pub task_submission_lock_path: Option<PathBuf>,
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
            task_submission_lock_path: None,
        }
    }
}

/// Acquire the channel-scoped lock shared by interactive and redirected tasks.
pub fn acquire_task_submission_lock(path: &Path) -> Result<std::fs::File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open task submission lock {}", path.display()))?;
    let metadata = file.metadata()?;
    // SAFETY: geteuid has no memory-safety preconditions.
    let current_uid = unsafe { libc::geteuid() };
    anyhow::ensure!(metadata.file_type().is_file(), "task lock is not a file");
    anyhow::ensure!(
        metadata.uid() == current_uid,
        "task lock has a foreign owner"
    );
    anyhow::ensure!(metadata.mode() & 0o077 == 0, "task lock is not private");

    // SAFETY: flock acts on the live descriptor and does not retain the pointer.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
            bail!("another Alan task is already running for this channel");
        }
        return Err(error).context("acquire task submission lock");
    }
    Ok(file)
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
    let mut _active_task_lock = None;
    let watch_tails = hydrate_and_open_tails(&shell, &config.agent_path, &mut app).await?;

    let mut terminal = TerminalSession::enter()?;
    terminal.draw_with(|frame| draw(frame, &app))?;

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
                        let mut submission_lock = None;
                        let submission_requested = follows_root_agent
                            && matches!(
                                &other,
                                FileBackedEvent::Terminal(TerminalEvent::Key(key))
                                    if key.code == KeyCode::Enter
                                        && !key.modifiers.contains(KeyModifiers::SHIFT)
                            )
                            && app.enter_submits_agent_task();
                        let blocked_submission = if submission_requested {
                            match prepare_root_agent_submission(
                                &shell,
                                &config.agent_path,
                                config.task_submission_lock_path.as_deref(),
                            )
                            .await
                            {
                                Ok(lock) => {
                                    submission_lock = lock;
                                    false
                                }
                                Err(err) => {
                                    app.push_error(format!("submit blocked: {err:#}"));
                                    true
                                }
                            }
                        } else {
                            false
                        };
                        if !blocked_submission
                            && let Some(action) = app.dispatch(other)
                        {
                            match action {
                                FileBackedAction::Submit(text) => {
                                    let submitted_at_ms = unix_time_ms();
                                    let prior_matching_turns =
                                        app.tape_user_prompt_count(&text);
                                    match write_agent_input(&shell, &app.agent_path, &text).await {
                                        Ok(()) => {
                                            app.notice = None;
                                            if follows_root_agent {
                                                _active_task_lock = submission_lock.take();
                                                pending_root_agent_turn = Some(PendingRootAgentTurn {
                                                    input: text.clone(),
                                                    observed_active: false,
                                                    interrupt_requested: false,
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
                                    if request_pending_root_interrupt(&mut pending_root_agent_turn) {
                                        send_interrupt(&shell, &mut app).await;
                                    } else {
                                        app.notice = Some("interrupt queued".to_string());
                                    }
                                }
                                FileBackedAction::Quit => break,
                            }
                        }
                    }
                }
                if follows_root_agent {
                    if observe_root_agent_activity(
                        &mut pending_root_agent_turn,
                        app.activity.state,
                    ) {
                        send_interrupt(&shell, &mut app).await;
                    }
                    if pending_root_agent_turn.is_none() {
                        _active_task_lock = None;
                    }
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
                if observe_root_agent_activity(
                    &mut pending_root_agent_turn,
                    app.activity.state,
                ) {
                    send_interrupt(&shell, &mut app).await;
                }
                if pending_root_agent_turn.is_none() {
                    _active_task_lock = None;
                }
                if previous_pid != watchers.root_agent_pid
                    || previous_refresh_failed != watchers.pid_refresh_failed
                    || previous_turn != pending_root_agent_turn
                {
                    dirty = true;
                }
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
    drop(rx);
    terminal_reader
        .await
        .context("terminal reader task failed")?;

    Ok(())
}

struct AgentWatchers {
    shutdown: tokio::sync::watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<Result<()>>>,
    root_agent_pid: Option<u64>,
    pid_refresh_failed: bool,
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
                self.stop().await;
                match reattach_to_current_agent(shell, agent_path, app, submitted_task).await {
                    Ok((tails, submitted_task_settled)) => {
                        self.pid_refresh_failed = false;
                        *self = Self::start(tails, agent_path, tx.clone());
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
    let mut attachment = open_stdio_tail_attachment_when_idle(&shell, &agent_path).await?;

    // ponytail: the CLI lock excludes concurrent one-shot clients; add IDs if
    // one-shot and interactive submissions need concurrent task correlation.
    let task = StdioTaskWaitContext::new(input, std::mem::take(&mut attachment.tape_history));

    let result = wait_for_stdio_answer(&shell, &agent_path, task, &mut attachment, async {
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
    root_agent_path: &str,
    task: StdioTaskWaitContext<'_>,
    attachment: &mut StdioTailAttachment,
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

    if current_root_agent_pid(shell).await? != Some(attachment.root_agent_pid) {
        bail!("Root Agent changed before the task could be submitted; retry")
    }
    write_agent_input(shell, &attachment.agent_process_path, task.input).await?;

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
                        match tail::recover_stdio_task_after_tail_close(
                            shell, root_agent_path, &task, attachment, &mut snapshot, interrupt_requested,
                        ).await? {
                            tail::StdioTaskRecovery::Complete(answer) => return Ok(answer),
                            tail::StdioTaskRecovery::Reattached => {
                                tape_pending.clear();
                                ui_pending.clear();
                                continue;
                            }
                            tail::StdioTaskRecovery::Unavailable => {
                                root_agent_pid_tick.tick().await;
                                continue;
                            }
                            tail::StdioTaskRecovery::Unchanged => return Err(error),
                        }
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
            bytes = attachment.ui_tail.read(4096) => {
                let bytes = match bytes {
                    Ok(bytes) if !bytes.is_empty() => bytes,
                    result => {
                        let error = match result {
                            Ok(_) => anyhow::anyhow!("Agent UI event stream closed before the task completed"),
                            Err(err) => anyhow::anyhow!("read Agent UI events failed: {err:?}"),
                        };
                        match tail::recover_stdio_task_after_tail_close(
                            shell, root_agent_path, &task, attachment, &mut snapshot, interrupt_requested,
                        ).await? {
                            tail::StdioTaskRecovery::Complete(answer) => return Ok(answer),
                            tail::StdioTaskRecovery::Reattached => {
                                tape_pending.clear();
                                ui_pending.clear();
                                continue;
                            }
                            tail::StdioTaskRecovery::Unavailable => {
                                root_agent_pid_tick.tick().await;
                                continue;
                            }
                            tail::StdioTaskRecovery::Unchanged => return Err(error),
                        }
                    }
                };
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
                                snapshot.activity_state = Some(UiActivityState::Running);
                                snapshot.task_error = None;
                            } else if activity.state == UiActivityState::Paused
                                && snapshot.activity_state == Some(UiActivityState::Running)
                            {
                                snapshot.activity_state = Some(UiActivityState::Paused);
                            } else if activity.state == UiActivityState::Idle
                                && matches!(
                                    snapshot.activity_state,
                                    Some(UiActivityState::Running | UiActivityState::Paused)
                                )
                            {
                                snapshot.activity_state = Some(UiActivityState::Idle);
                            }
                        }
                        UiEvent::Error { message, .. }
                            if matches!(
                                snapshot.activity_state,
                                Some(UiActivityState::Running | UiActivityState::Paused)
                            ) =>
                        {
                            snapshot.task_error = Some(message)
                        }
                        UiEvent::Error { .. } => {}
                        UiEvent::Plan { .. } | UiEvent::Thinking { .. } | UiEvent::Notice { .. } => {}
                    }
                }
                if interrupt_requested
                    && interrupt_stdio_task_if_active(shell, &attachment.agent_process_path, &snapshot).await?
                {
                    bail!("Agent task interrupted");
                }
                if snapshot.activity_state == Some(UiActivityState::Paused) {
                    bail!("Agent task needs interactive input; attach with the TTY renderer");
                }
                if let Err(error) = stdio_completion::refresh_answer_after_idle(
                    shell,
                    &attachment.agent_process_path,
                    &task,
                    &mut snapshot,
                )
                .await
                {
                    match tail::recover_stdio_task_after_tail_close(
                        shell,
                        root_agent_path,
                        &task,
                        attachment,
                        &mut snapshot,
                        interrupt_requested,
                    )
                    .await?
                    {
                        tail::StdioTaskRecovery::Complete(answer) => return Ok(answer),
                        tail::StdioTaskRecovery::Reattached => {
                            tape_pending.clear();
                            ui_pending.clear();
                            continue;
                        }
                        tail::StdioTaskRecovery::Unavailable => {
                            root_agent_pid_tick.tick().await;
                            continue;
                        }
                        tail::StdioTaskRecovery::Unchanged => return Err(error),
                    }
                }
                if let Some(answer) = finish_stdio_task_if_ready(&mut snapshot)? {
                    return Ok(answer);
                }
            }
            _ = root_agent_pid_tick.tick() => {
                match tail::recover_stdio_task_after_root_change(
                    shell, root_agent_path, &task, attachment, &mut snapshot, interrupt_requested,
                ).await? {
                    tail::StdioTaskRecovery::Complete(answer) => return Ok(answer),
                    tail::StdioTaskRecovery::Reattached => {
                        tape_pending.clear();
                        ui_pending.clear();
                    }
                    tail::StdioTaskRecovery::Unavailable | tail::StdioTaskRecovery::Unchanged => {}
                }
            }
            signal = &mut interrupt, if !interrupt_requested => {
                signal?;
                interrupt_requested = true;
                if interrupt_stdio_task_if_active(shell, &attachment.agent_process_path, &snapshot).await? {
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

struct StdioTaskWaitContext<'a> {
    input: &'a str,
    baseline_tape_history: Vec<u8>,
    submitted_at_ms: u64,
}

impl<'a> StdioTaskWaitContext<'a> {
    fn new(input: &'a str, baseline_tape_history: Vec<u8>) -> Self {
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
    task: &StdioTaskWaitContext<'_>,
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
    stdio_completion::refresh_answer_after_idle(shell, agent_path, task, &mut snapshot).await?;
    Ok(snapshot)
}

fn stdio_task_snapshot_from_history(
    task: &StdioTaskWaitContext<'_>,
    tape_history: &[u8],
    ui_history: &[u8],
) -> Result<StdioTaskSnapshot> {
    let (tape_task_started, assistant_answer) =
        stdio_completion::tape_outcome(task.input, &task.baseline_tape_history, tape_history)?;
    let ui_task = file_surface::correlated_ui_task(ui_history, task.submitted_at_ms)?;
    Ok(StdioTaskSnapshot {
        task_started: tape_task_started || ui_task.started,
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
#[path = "file_backed/stdio_idle_tests.rs"]
mod stdio_idle_tests;
#[cfg(test)]
mod stdio_tests;
#[cfg(test)]
mod tests;
