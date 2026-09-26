use std::path::{Path, PathBuf};
use std::{
    collections::VecDeque,
    fs::OpenOptions,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
};

use alan_agent_protocol::{
    InputIntent, InputMode, UiActivitySnapshot, UiActivityState, UiEvent, UserInputRecord,
    parse_input_prefix,
};
#[cfg(test)]
use alan_agent_protocol::{
    ToolResultPresentation, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
    YieldKind,
};
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
use file_surface::write_agent_input;
#[cfg(test)]
use file_surface::{
    ActionSnapshot, RequestSnapshot, agent_output_path, hydrate_tape_history, parse_tape_history,
    request_snapshot_to_pending_yield, sync_action_snapshot,
};
use file_surface::{
    TapeRecordV1, hydrate_and_open_tails, reattach_to_current_agent, spawn_action_watch,
    spawn_output_tail, spawn_request_watch, spawn_tape_watch, spawn_terminal_events,
    spawn_ui_watch, sync_action_from_file, sync_requests_from_files, write_agent_submission,
    write_machine_ctl, write_request_response,
};
use layout::{draw, history_prefix_to_drain, inline_viewport_height, live_region_height};
use tail::{
    StdioTailAttachment, close_stdio_tails, current_root_agent_pid,
    open_stdio_tail_attachment_when_idle, root_agent_path_for_pid,
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
                                FileBackedAction::Submit(input) => {
                                    let text = input.body.clone();
                                    let submitted_at_ms = unix_time_ms();
                                    let prior_matching_turns =
                                        app.tape_user_prompt_count(&text);
                                    match write_agent_submission(
                                        &shell,
                                        &app.agent_path,
                                        input.intent,
                                        &text,
                                    )
                                        .await
                                    {
                                        Ok(submission_id) => {
                                            app.accept_submission(&input);
                                            if input.intent == InputIntent::Command {
                                                app.mark_command_submission(submission_id);
                                            }
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
                                                    &mut rx,
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
                                        Err(err) => {
                                            app.restore_rejected_submission(&input);
                                            app.push_error(format!(
                                                "submit rejected; draft restored: {err:#}"
                                            ));
                                        }
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
                    &mut rx,
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

fn discard_superseded_attachment_events(
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
    pending_terminal_events: &mut VecDeque<FileBackedEvent>,
) {
    for _ in 0..rx.len() {
        let Ok(event) = rx.try_recv() else {
            break;
        };
        if matches!(
            event,
            FileBackedEvent::Terminal(_) | FileBackedEvent::TerminalError(_)
        ) {
            pending_terminal_events.push_back(event);
        }
    }
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
        submitted_task: Option<(&str, u64, usize)>,
        tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    ) -> bool {
        match current_root_agent_pid(shell).await {
            Ok(Some(pid)) if self.root_agent_pid != Some(pid) => {
                self.stop().await;
                discard_superseded_attachment_events(rx, &mut self.pending_terminal_events);
                match reattach_to_current_agent(shell, agent_path, app, submitted_task).await {
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

/// Run one task from redirected stdin and write its result to the standard streams.
pub async fn run_stdio_task(
    root_transport: InProcessTransport,
    agent_path: impl Into<String>,
    input: &str,
) -> Result<i32> {
    use tokio::io::AsyncWriteExt;

    let agent_path = agent_path.into();
    let shell = alan_shell::Shell::new(root_transport);
    let mut attachment = open_stdio_tail_attachment_when_idle(&shell, &agent_path).await?;

    // ponytail: a channel-wide lock serializes client results until queued
    // outcomes are fully correlated.
    let task = StdioTaskWaitContext::new(input);
    if task.record.body.trim().is_empty() {
        bail!(if task.record.intent == InputIntent::Agent {
            "stdin did not contain an Agent task"
        } else {
            "missing content after input prefix"
        });
    }

    let result = wait_for_stdio_answer(&shell, &agent_path, task, &mut attachment, async {
        tokio::signal::ctrl_c().await.map_err(anyhow::Error::from)
    })
    .await;
    let close_result = close_stdio_tails(attachment.tape_tail, attachment.ui_tail).await;
    let completion = result?;
    close_result?;

    let mut stdout = tokio::io::stdout();
    let exit_code = match completion {
        StdioTaskCompletion::AgentAnswer(answer) => {
            stdout.write_all(answer.as_bytes()).await?;
            if !answer.ends_with('\n') {
                stdout.write_all(b"\n").await?;
            }
            0
        }
        StdioTaskCompletion::Command(result) => {
            stdout.write_all(result.stdout.as_bytes()).await?;
            stdout.flush().await?;
            let mut stderr = tokio::io::stderr();
            stderr.write_all(result.stderr.as_bytes()).await?;
            stderr.flush().await?;
            result.exit_code
        }
    };
    stdout.flush().await?;
    Ok(exit_code)
}

async fn wait_for_stdio_answer(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
    task: StdioTaskWaitContext,
    attachment: &mut StdioTailAttachment,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<StdioTaskCompletion> {
    submit_stdio_task(shell, &task, attachment).await?;
    wait_for_stdio_answer_after_submit(shell, root_agent_path, task, attachment, interrupt).await
}

async fn submit_stdio_task(
    shell: &alan_shell::Shell,
    task: &StdioTaskWaitContext,
    attachment: &StdioTailAttachment,
) -> Result<()> {
    if current_root_agent_pid(shell).await? != Some(attachment.root_agent_pid) {
        bail!("Root Agent changed before the task could be submitted; retry")
    }
    let payload = task.record.encode_payload()?;
    shell
        .write(
            &format!("{}/io/input", attachment.agent_process_path),
            &payload,
        )
        .await
        .map_err(|err| anyhow::anyhow!("write Agent submission failed: {err:?}"))
}

async fn wait_for_stdio_answer_after_submit(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
    task: StdioTaskWaitContext,
    attachment: &mut StdioTailAttachment,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<StdioTaskCompletion> {
    tokio::pin!(interrupt);
    let mut tape_pending = Vec::new();
    let mut ui_pending = Vec::new();
    let mut interrupt_requested = false;
    let mut snapshot = StdioTaskSnapshot {
        task_started: false,
        assistant_answer: None,
        command_result: None,
        activity_state: None,
        task_error: None,
    };
    let mut root_agent_pid_tick = tokio::time::interval(std::time::Duration::from_millis(250));
    root_agent_pid_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    'wait: loop {
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
                            tail::StdioTaskRecovery::Complete(completion) => return Ok(completion),
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
                        snapshot.task_started = record.role == "user"
                            && record.submission_id.as_deref()
                                == Some(task.record.submission_id.as_str());
                    } else if record.role == "assistant"
                        && record.submission_id.as_deref()
                            == Some(task.record.submission_id.as_str())
                    {
                        snapshot.assistant_answer = Some(record.content);
                    }
                }
                if let Some(completion) = finish_stdio_task_if_ready(&mut snapshot, task.record.intent)? {
                    return Ok(completion);
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
                            tail::StdioTaskRecovery::Complete(completion) => return Ok(completion),
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
                            stdio_completion::observe_activity(&task, &mut snapshot, &activity);
                        }
                        UiEvent::Error { message, submission_id: Some(id), .. }
                            if id == task.record.submission_id =>
                        {
                            snapshot.task_started = true;
                            snapshot.activity_state = Some(UiActivityState::Idle);
                            snapshot.task_error = Some(message);
                            break;
                        }
                        UiEvent::Error { .. } => {}
                        UiEvent::Plan { .. } | UiEvent::Thinking { .. } | UiEvent::Notice { .. } => {}
                    }
                }
                if interrupt_requested
                    && interrupt_stdio_task_if_accepted(shell, &attachment.agent_process_path, &task.record.submission_id, &snapshot).await?
                {
                    bail!("Agent task interruption requested");
                }
                if snapshot.activity_state == Some(UiActivityState::Paused) {
                    bail!("Agent task needs interactive input; attach with the TTY renderer");
                }
                let refresh_result = {
                    let refresh = stdio_completion::refresh_answer_after_idle(
                        shell,
                        &attachment.agent_process_path,
                        &task,
                        &mut snapshot,
                    );
                    tokio::pin!(refresh);
                    loop {
                        tokio::select! {
                            result = &mut refresh => break Some(result),
                            _ = root_agent_pid_tick.tick() => {
                                if tail::current_root_agent_pid(shell).await? != Some(attachment.root_agent_pid) {
                                    break None;
                                }
                            }
                        }
                    }
                };
                let Some(refresh_result) = refresh_result else {
                    tape_pending.clear();
                    ui_pending.clear();
                    continue 'wait;
                };
                if let Err(error) = refresh_result {
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
                        tail::StdioTaskRecovery::Complete(completion) => return Ok(completion),
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
                if let Some(completion) = finish_stdio_task_if_ready(&mut snapshot, task.record.intent)? {
                    return Ok(completion);
                }
            }
            _ = root_agent_pid_tick.tick() => {
                match tail::recover_stdio_task_after_root_change(
                    shell, root_agent_path, &task, attachment, &mut snapshot, interrupt_requested,
                ).await? {
                    tail::StdioTaskRecovery::Complete(completion) => return Ok(completion),
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
                if interrupt_stdio_task_if_accepted(shell, &attachment.agent_process_path, &task.record.submission_id, &snapshot).await? {
                    bail!("Agent task interruption requested");
                }
            }
        }
    }
}

async fn interrupt_stdio_task_if_accepted(
    shell: &alan_shell::Shell,
    agent_path: &str,
    submission_id: &str,
    snapshot: &StdioTaskSnapshot,
) -> Result<bool> {
    if !snapshot.task_started {
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

fn finish_stdio_task_if_ready(
    snapshot: &mut StdioTaskSnapshot,
    intent: InputIntent,
) -> Result<Option<StdioTaskCompletion>> {
    if !snapshot.task_started || snapshot.activity_state != Some(UiActivityState::Idle) {
        return Ok(None);
    }
    if intent == InputIntent::Command
        && let Some(result) = snapshot.command_result.take()
    {
        return Ok(Some(StdioTaskCompletion::Command(result)));
    }
    if let Some(message) = snapshot.task_error.take() {
        bail!("Agent task failed: {message}");
    }
    if intent != InputIntent::Command
        && let Some(answer) = snapshot.assistant_answer.take()
    {
        return Ok(Some(StdioTaskCompletion::AgentAnswer(answer)));
    }
    Ok(None)
}

#[derive(Debug, PartialEq, Eq)]
enum StdioTaskCompletion {
    AgentAnswer(String),
    Command(stdio_completion::CommandResult),
}

struct StdioTaskSnapshot {
    task_started: bool,
    assistant_answer: Option<String>,
    command_result: Option<stdio_completion::CommandResult>,
    activity_state: Option<UiActivityState>,
    task_error: Option<String>,
}

struct StdioTaskWaitContext {
    record: UserInputRecord,
    submitted_at_ms: u64,
}

impl StdioTaskWaitContext {
    fn new(input: &str) -> Self {
        let input = strip_one_stdin_line_ending(input);
        let (intent, input) = parse_input_prefix(input);
        let record = UserInputRecord::new(intent, InputMode::FollowUp, input);
        Self {
            record,
            submitted_at_ms: unix_time_ms(),
        }
    }
}

fn strip_one_stdin_line_ending(input: &str) -> &str {
    input
        .strip_suffix("\r\n")
        .or_else(|| input.strip_suffix('\n'))
        .unwrap_or(input)
}

async fn stdio_task_snapshot(
    shell: &alan_shell::Shell,
    agent_path: &str,
    task: &StdioTaskWaitContext,
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
        stdio_completion::observe_activity(task, &mut snapshot, &activity);
    }
    stdio_completion::refresh_answer_after_idle(shell, agent_path, task, &mut snapshot).await?;
    Ok(snapshot)
}

fn stdio_task_snapshot_from_history(
    task: &StdioTaskWaitContext,
    tape_history: &[u8],
    ui_history: &[u8],
) -> Result<StdioTaskSnapshot> {
    let (tape_task_started, assistant_answer) =
        stdio_completion::tape_outcome(&task.record.submission_id, tape_history)?;
    let task_error = stdio_completion::submission_error(&task.record.submission_id, ui_history)?;
    let failed = task_error.is_some();
    let mut snapshot = StdioTaskSnapshot {
        task_started: failed || tape_task_started,
        assistant_answer,
        command_result: None,
        activity_state: failed.then_some(UiActivityState::Idle),
        task_error,
    };
    for line in std::str::from_utf8(ui_history)
        .context("ui events are not utf8")?
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        if let UiEvent::Activity { snapshot: activity } =
            serde_json::from_str(line).context("parse Agent UI event")?
        {
            stdio_completion::observe_activity(task, &mut snapshot, &activity);
        }
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
