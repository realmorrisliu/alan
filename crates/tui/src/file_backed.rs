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
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
#[cfg(test)]
use ratatui::style::Color;
mod app;
mod file_surface;
mod history_merge;
mod interrupt;
mod layout;
mod previous_input;
mod project;
pub use project::{
    ProjectAccess, ProjectControl, ProjectControlFuture, ProjectControlHandler,
    ProjectControlResult, ProjectMountReceipt,
};
mod stdio_completion;
mod tail;

use app::{FileBackedAction, FileBackedApp, FileBackedEvent};
use interrupt::{
    PendingRootAgentTurn, observe_root_agent_completion, settle_unknown_replaced_input,
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
    StdioTailAttachment, close_stdio_tails, current_root_agent_pid, open_stdio_tail_attachment,
    root_agent_path_for_pid, spawn_root_agent_pid_refresh,
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
    /// Effective model selected for this invocation, when supplied by the Host.
    pub effective_model: Option<String>,
    /// Optional explicitly authorized Host directory used for local `@` file completion.
    pub host_file_completion_root: Option<PathBuf>,
    /// Whether stdin/stdout must be interactive before entering the UI.
    pub require_interactive_terminal: bool,
    /// Optional file used to persist composer input history across launches.
    pub history_path: Option<PathBuf>,
    /// Optional local skill candidates used for `$` completion.
    pub skill_candidates: Vec<CompletionCandidate>,
    /// Host cwd offered as the initial path in `/project`.
    pub project_candidate: Option<PathBuf>,
    /// Host-local project grant/revoke control for this Alan invocation.
    pub project_control: Option<ProjectControlHandler>,
}

impl FileBackedRunConfig {
    /// Create a renderer configuration for a mounted Agent Process.
    pub fn new(root_transport: InProcessTransport, agent_path: impl Into<String>) -> Self {
        Self {
            root_transport,
            agent_path: agent_path.into(),
            effective_model: None,
            host_file_completion_root: None,
            require_interactive_terminal: true,
            history_path: None,
            skill_candidates: Vec::new(),
            project_candidate: None,
            project_control: None,
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
    app.set_effective_model(config.effective_model.clone());
    app.set_skill_candidates(config.skill_candidates.clone());
    app.set_project_candidate(config.project_candidate.clone());
    if let Some(host_root) = &config.host_file_completion_root {
        app.set_file_candidates(super::build_file_index(host_root, crate::FILE_INDEX_LIMIT));
    }
    if let Some(history_path) = &config.history_path {
        let history = load_history(history_path, crate::HISTORY_LIMIT);
        app.composer = Composer::with_history(history, Some(history_path.clone()));
    }
    let follows_root_agent = config.agent_path == "/agent/root";
    let mut pending_root_agent_turns = VecDeque::<PendingRootAgentTurn>::new();
    let watch_tails = hydrate_and_open_tails(&shell, &config.agent_path, &mut app).await?;

    let mut terminal = TerminalSession::enter()?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<FileBackedEvent>(128);
    let terminal_reader = spawn_terminal_events(tx.clone());

    let mut watchers = AgentWatchers::start(watch_tails, &config.agent_path, tx.clone());
    let (root_agent_pid_retry, root_agent_pid_retry_rx) = tokio::sync::watch::channel(());
    let root_agent_pid_refresh = if follows_root_agent {
        Some(spawn_root_agent_pid_refresh(
            shell.clone(),
            tx.clone(),
            root_agent_pid_retry_rx,
            watchers.root_agent_pid,
        ))
    } else {
        None
    };

    let mut frame_tick = tokio::time::interval(std::time::Duration::from_millis(33));
    frame_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = true;

    loop {
        tokio::select! {
            event = receive_file_backed_event(&mut watchers.pending_terminal_events, &mut rx) => {
                let Some(event) = event else {
                    break;
                };
                if let FileBackedEvent::Ui(ref event) = event {
                    observe_root_agent_completion(&mut pending_root_agent_turns, event, &mut app);
                }
                match event {
                    FileBackedEvent::RootAgentPidRefresh(result) => {
                        let retry = matches!(&result, Ok(Some(_)));
                        watchers.pending_root_agent_pid_result = Some(result);
                        watchers
                            .refresh_root_agent_attachment(
                                &shell,
                                &config.agent_path,
                                &mut app,
                                &mut rx,
                                &mut pending_root_agent_turns,
                                &tx,
                            )
                            .await;
                        if retry && watchers.pid_refresh_failed {
                            root_agent_pid_retry.send_replace(());
                        }
                        settle_unknown_replaced_input(
                            &mut pending_root_agent_turns,
                            watchers.root_agent_pid,
                            &mut app,
                        );
                    }
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
                        } else if let Some(grant_id) = app.take_ready_project_revoke() {
                            if let Some(handler) = config.project_control.as_ref() {
                                match handler(ProjectControl::Revoke { grant_id }).await {
                                    Ok(ProjectControlResult::Revoked) => {
                                        app.project_revoked();
                                        app.notice = Some("project grant revoked".into());
                                    }
                                    Ok(ProjectControlResult::Mounted { .. }) => {
                                        app.push_error("Host mounted a project while revoke was expected".into());
                                    }
                                    Err(err) => app.push_error(format!("project revoke failed: {err:#}")),
                                }
                            } else {
                                app.push_error("local project selection is unavailable in this Host".into());
                            }
                        }
                    }
                    other => {
                        if let Some(action) = app.dispatch(other)
                        {
                            match action {
                                FileBackedAction::Submit(record) => {
                                    let submitted_at_ms = unix_time_ms();
                                    let text = record.body.clone();
                                    match write_agent_input(&shell, &app.agent_path, watchers.root_agent_pid, &record).await {
                                        Ok(()) => {
                                            app.accept_input();
                                            app.notice = None;
                                            if follows_root_agent {
                                                pending_root_agent_turns.push_back(PendingRootAgentTurn {
                                                    input: text.clone(),
                                                    submission_id: record.submission_id.clone(),
                                                    submitted_process: watchers.root_agent_pid,
                                                    submitted_at_ms,
                                            });
                                                watchers
                                                    .refresh_root_agent_attachment(
                                                    &shell,
                                                    &config.agent_path,
                                                    &mut app,
                                                    &mut rx,
                                                    &mut pending_root_agent_turns,
                                                    &tx,
                                                    )
                                                    .await;
                                            }
                                        }
                                        Err(err) => app.push_error(format!("submit failed: {err:#}")),
                                    }
                                }
                                FileBackedAction::Resume {
                                    request_id,
                                    response,
                                    retry_input,
                                } => {
                                    let response_agent_path = if follows_root_agent {
                                        pending_root_agent_turns
                                            .front()
                                            .and_then(|turn| turn.submitted_process)
                                            .or(watchers.root_agent_pid)
                                            .map(|pid| format!("/agent/{pid}"))
                                    } else {
                                        Some(app.agent_path.clone())
                                    };
                                    if let Some(agent_path) = response_agent_path {
                                        app.begin_resume_write(request_id.clone());
                                        let operation_shell =
                                            alan_shell::Shell::new(config.root_transport.clone());
                                        let completion_tx = tx.clone();
                                        tokio::spawn(async move {
                                            let result = write_request_response(
                                                &operation_shell,
                                                &agent_path,
                                                &request_id,
                                                &response,
                                            )
                                            .await
                                            .map_err(|err| format!("{err:#}"));
                                            let _ = completion_tx
                                                .send(FileBackedEvent::ResumeWriteCompleted {
                                                    request_id,
                                                    retry_input,
                                                    result,
                                                })
                                                .await;
                                        });
                                    } else {
                                        app.push_error(
                                            "Root Agent is not attached; retry response".into(),
                                        );
                                    }
                                }
                                FileBackedAction::MachineCtl { command, success_notice } => {
                                    spawn_control_write(
                                        config.root_transport.clone(),
                                        tx.clone(),
                                        app.agent_path.clone(),
                                        command,
                                        success_notice,
                                        "control failed".into(),
                                    );
                                }
                                FileBackedAction::Project(command) => {
                                    let Some(handler) = config.project_control.as_ref() else {
                                        app.push_error("local project selection is unavailable in this Host".into());
                                        dirty = true;
                                        continue;
                                    };
                                    match command {
                                        ProjectControl::Mount { host_path, access } => {
                                            match handler(ProjectControl::Mount { host_path, access }).await {
                                                Ok(ProjectControlResult::Mounted { receipt, completion_root }) => {
                                                    let record = alan_agent_protocol::UserInputRecord::new(
                                                        alan_agent_protocol::InputIntent::Command,
                                                        alan_agent_protocol::InputMode::FollowUp,
                                                        format!("cd {}", receipt.namespace_path),
                                                    );
                                                    let submission_id = record.submission_id.clone();
                                                    match write_agent_input(&shell, &app.agent_path, watchers.root_agent_pid, &record).await {
                                                        Ok(()) => {
                                                            if follows_root_agent {
                                                                pending_root_agent_turns.push_back(PendingRootAgentTurn {
                                                                    input: record.body.clone(),
                                                                    submission_id: record.submission_id.clone(),
                                                                    submitted_process: watchers.root_agent_pid,
                                                                    submitted_at_ms: unix_time_ms(),
                                                                });
                                                            }
                                                            app.set_file_candidates(super::build_file_index(&completion_root, crate::FILE_INDEX_LIMIT));
                                                            app.project_mounted(receipt, submission_id);
                                                            if follows_root_agent {
                                                                watchers.refresh_root_agent_attachment(
                                                                    &shell,
                                                                    &config.agent_path,
                                                                    &mut app,
                                                                    &mut rx,
                                                                    &mut pending_root_agent_turns,
                                                                    &tx,
                                                                ).await;
                                                            }
                                                        }
                                                        Err(err) => {
                                                            let cleanup = handler(ProjectControl::Revoke {
                                                                grant_id: receipt.grant_id.clone(),
                                                            }).await;
                                                            app.push_error(match cleanup {
                                                                Ok(_) => format!("project mounted but cwd setup failed: {err:#}"),
                                                                Err(cleanup) => format!("project mount succeeded, cwd setup failed ({err:#}); grant cleanup also failed ({cleanup:#})"),
                                                            });
                                                        }
                                                    }
                                                }
                                                Ok(ProjectControlResult::Revoked) => {
                                                    app.push_error("Host revoked a project while a mount was expected".into());
                                                }
                                                Err(err) => app.push_error(format!("project mount failed: {err:#}")),
                                            }
                                        }
                                        ProjectControl::Revoke { grant_id } => {
                                            let record = alan_agent_protocol::UserInputRecord::new(
                                                alan_agent_protocol::InputIntent::Command,
                                                alan_agent_protocol::InputMode::FollowUp,
                                                "cd /",
                                            );
                                            match write_agent_input(&shell, &app.agent_path, watchers.root_agent_pid, &record).await {
                                                Ok(()) => {
                                                    app.begin_project_revoke(grant_id, record.submission_id.clone());
                                                    app.notice = Some("leaving project cwd before revoking its grant".into());
                                                    if follows_root_agent {
                                                        pending_root_agent_turns.push_back(PendingRootAgentTurn {
                                                            input: record.body.clone(),
                                                            submission_id: record.submission_id.clone(),
                                                            submitted_process: watchers.root_agent_pid,
                                                            submitted_at_ms: unix_time_ms(),
                                                        });
                                                        watchers.refresh_root_agent_attachment(
                                                            &shell,
                                                            &config.agent_path,
                                                            &mut app,
                                                            &mut rx,
                                                            &mut pending_root_agent_turns,
                                                            &tx,
                                                        ).await;
                                                    }
                                                }
                                                Err(err) => app.push_error(format!("could not leave project before revoke: {err:#}")),
                                            }
                                        }
                                    }
                                }
                                FileBackedAction::Interrupt => {
                                    match interrupt::interrupt_control(
                                        &app.agent_path,
                                        &pending_root_agent_turns,
                                        watchers.root_agent_pid,
                                    ) {
                                        Ok((agent_path, command)) => {
                                            spawn_control_write(
                                                config.root_transport.clone(),
                                                tx.clone(),
                                                agent_path,
                                                command,
                                                "interrupt requested".into(),
                                                "interrupt failed".into(),
                                            );
                                        }
                                        Err(error) => app.push_error(error),
                                    }
                                }
                                FileBackedAction::Quit => break,
                            }
                        }
                    }
                }
                if follows_root_agent {
                    settle_unknown_replaced_input(&mut pending_root_agent_turns, watchers.root_agent_pid, &mut app);
                }
                dirty = true;
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
    if let Some(task) = root_agent_pid_refresh {
        let _ = task.await;
    }
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

fn spawn_control_write(
    transport: InProcessTransport,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    agent_path: String,
    command: String,
    success_notice: String,
    error_prefix: String,
) {
    tokio::spawn(async move {
        let shell = alan_shell::Shell::new(transport);
        let result = write_machine_ctl(&shell, &agent_path, &command)
            .await
            .map_err(|err| format!("{err:#}"));
        let _ = tx
            .send(FileBackedEvent::ControlWriteCompleted {
                success_notice,
                error_prefix,
                result,
            })
            .await;
    });
}

struct AgentWatchers {
    recovery: Option<(alan_shell::Tail, alan_shell::Tail)>,
    shutdown: tokio::sync::watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<Result<()>>>,
    root_agent_pid: Option<u64>,
    pid_refresh_failed: bool,
    pending_root_agent_pid_result: Option<Result<Option<u64>, String>>,
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
            pending_root_agent_pid_result: None,
            pending_terminal_events: VecDeque::new(),
        }
    }

    async fn refresh_root_agent_attachment(
        &mut self,
        shell: &alan_shell::Shell,
        agent_path: &str,
        app: &mut FileBackedApp,
        rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
        pending_turns: &mut VecDeque<PendingRootAgentTurn>,
        tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    ) -> bool {
        let current_pid = match self.pending_root_agent_pid_result.take() {
            Some(result) => result,
            None => current_root_agent_pid(shell)
                .await
                .map_err(|error| format!("{error:#}")),
        };
        match current_pid {
            Ok(Some(pid)) if self.root_agent_pid != Some(pid) => {
                let pending_count = pending_turns.len();
                // Tape history outlives the local pending-input lock.
                let history_restored = if let Some((_, tape)) = &self.recovery {
                    previous_input::restore_tape_history(app, tape, app.tape_consumed_offset).await
                } else {
                    false
                };
                let submitted = pending_turns
                    .iter()
                    .map(|turn| (turn.submission_id.clone(), turn.input.clone()))
                    .collect::<Vec<_>>();
                let retained = self.stop_for_input(&submitted).await;
                app.expected_terminal_error = None;
                let queued = discard_superseded_attachment_events(
                    rx,
                    &mut self.pending_terminal_events,
                    &submitted,
                );
                for event in retained.0.iter().chain(&queued.0) {
                    interrupt::observe_root_agent_completion(pending_turns, event, app);
                }
                // The detached Process's companion terminal errors are discarded with its events.
                app.expected_terminal_error = None;
                if !history_restored {
                    for turn in &submitted {
                        if let Some((_, input, answer)) = retained
                            .1
                            .iter()
                            .chain(&queued.1)
                            .find(|(id, _, _)| id.as_str() == turn.0.as_str())
                        {
                            previous_input::restore_answer(app, input, answer.clone());
                        }
                    }
                }
                let submitted = pending_turns.iter().cloned().collect::<Vec<_>>();
                match reattach_to_current_agent(shell, agent_path, app, &submitted).await {
                    Ok((tails, settled_ids)) => {
                        pending_turns.retain(|turn| !settled_ids.contains(&turn.submission_id));
                        self.pid_refresh_failed = false;
                        let pending_terminal_events =
                            std::mem::take(&mut self.pending_terminal_events);
                        *self = Self::start(tails, agent_path, tx.clone());
                        self.pending_terminal_events = pending_terminal_events;
                        pending_turns.len() < pending_count
                    }
                    Err(err) => {
                        self.root_agent_pid = None;
                        if !self.pid_refresh_failed {
                            app.push_error(format!("Root Agent reattach failed: {err:#}"));
                        }
                        self.pid_refresh_failed = true;
                        pending_turns.len() < pending_count
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
                app.push_error(format!("Root Agent identity refresh failed: {err}"));
                false
            }
            Err(_) => false,
        }
    }

    async fn stop(&mut self) {
        self.stop_for_input(&[]).await;
    }

    async fn stop_for_input(
        &mut self,
        submitted: &[(String, String)],
    ) -> (Vec<UiEvent>, Vec<(String, String, String)>) {
        let _ = self.shutdown.send(true);
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
        let Some((ui, tape)) = self.recovery.take() else {
            return (Vec::new(), Vec::new());
        };
        let outcome = previous_input::snapshot(&ui, &tape, submitted).await;
        let _ = ui.close().await;
        let _ = tape.close().await;
        outcome
    }
}

/// Run one redirected input, write its output streams, and return its exit status.
pub async fn run_stdio_task(
    root_transport: InProcessTransport,
    agent_path: impl Into<String>,
    input: &str,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<i32> {
    use tokio::io::AsyncWriteExt;

    let task = StdioTaskWaitContext::new(input)?;

    let agent_path = agent_path.into();
    let shell = alan_shell::Shell::new(root_transport);
    let mut attachment = open_stdio_tail_attachment(&shell, &agent_path).await?;

    let result = wait_for_stdio_answer(&shell, task, &mut attachment, interrupt).await;
    let close_result = close_stdio_tails(attachment.tape_tail, attachment.ui_tail).await;
    let answer = result?;
    close_result?;

    let (out, err, exit_code, add_newline) = match answer {
        StdioTaskOutput::Agent(answer) => (answer, String::new(), 0, true),
        StdioTaskOutput::Command(output) => (output.stdout, output.stderr, output.exit_code, false),
    };
    let mut stdout = tokio::io::stdout();
    stdout.write_all(out.as_bytes()).await?;
    if add_newline && !out.ends_with('\n') {
        stdout.write_all(b"\n").await?;
    }
    stdout.flush().await?;
    let mut stderr = tokio::io::stderr();
    stderr.write_all(err.as_bytes()).await?;
    stderr.flush().await?;
    Ok(exit_code)
}

async fn wait_for_stdio_answer(
    shell: &alan_shell::Shell,
    task: StdioTaskWaitContext,
    attachment: &mut StdioTailAttachment,
    interrupt: impl std::future::Future<Output = Result<()>>,
) -> Result<StdioTaskOutput> {
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
) -> Result<StdioTaskOutput> {
    tokio::pin!(interrupt);
    let mut tape_pending = Vec::new();
    let mut ui_pending = Vec::new();
    let mut interrupt_requested = false;
    let mut snapshot = StdioTaskSnapshot {
        task_started: false,
        waiting_for_response: false,
        assistant_answer: None,
        command_output: None,
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

fn finish_stdio_task_if_ready(snapshot: &mut StdioTaskSnapshot) -> Result<Option<StdioTaskOutput>> {
    if snapshot.completion.is_none() {
        return Ok(None);
    }
    if let Some(output) = snapshot.command_output.take() {
        return Ok(Some(StdioTaskOutput::Command(output)));
    }
    if let Some(message) = snapshot.task_error.take() {
        bail!("Agent task failed: {message}");
    }
    if let Some(answer) = snapshot.assistant_answer.take() {
        return Ok(Some(StdioTaskOutput::Agent(answer)));
    }
    Ok(None)
}

#[derive(Debug)]
enum StdioTaskOutput {
    Agent(String),
    Command(CommandOutput),
}

#[derive(Debug, serde::Deserialize)]
struct CommandOutput {
    stdout: String,
    stderr: String,
    #[serde(default)]
    exit_code: i32,
}

#[cfg(test)]
impl StdioTaskOutput {
    fn agent_answer(self) -> String {
        match self {
            Self::Agent(answer) => answer,
            Self::Command(_) => panic!("expected Agent answer"),
        }
    }
}

struct StdioTaskSnapshot {
    task_started: bool,
    waiting_for_response: bool,
    assistant_answer: Option<String>,
    command_output: Option<CommandOutput>,
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
    fn new(input: &str) -> Result<Self> {
        let (intent, body) = alan_agent_protocol::parse_input_prefix(input);
        anyhow::ensure!(!body.trim().is_empty(), "stdin input body is empty");
        Ok(Self {
            record: alan_agent_protocol::UserInputRecord::new(
                intent,
                alan_agent_protocol::InputMode::FollowUp,
                body,
            ),
            #[cfg(test)]
            submitted_at_ms: unix_time_ms(),
        })
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
        command_output: None,
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
