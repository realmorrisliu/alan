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
mod action_detail_io;
mod app;
mod doctor;
mod file_surface;
mod fork_picker;
mod history_merge;
mod interrupt;
mod layout;
mod login;
mod login_flow;
pub use login::{
    LoginControl, LoginControlFuture, LoginControlHandler, LoginInputField, LoginMethod,
    LoginProgress, LoginProgressSender, LoginProviderChoice, LoginResult, LoginSelectOption,
    LoginSubmission,
};
mod model;
#[cfg(test)]
mod model_layout_tests;
#[cfg(test)]
mod model_matrix_tests;
#[cfg(test)]
mod model_review_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod model_transport_tests;
mod plan_detail_io;
mod previous_input;
mod project;
mod queue;
#[cfg(test)]
mod queue_boundary_tests;
#[cfg(test)]
mod queue_coordinate_tests;
#[cfg(test)]
mod queue_hint_lifetime_tests;
#[cfg(test)]
mod queue_lifetime_tests;
#[cfg(test)]
mod queue_lineage_tests;
#[cfg(test)]
mod queue_removed_hint_tests;
#[cfg(test)]
mod queue_root_lifetime_tests;
mod resume_picker;
mod settings;
mod settings_flow;
pub use settings::{
    SettingsControl, SettingsControlFuture, SettingsControlHandler, SettingsResetScope,
    SettingsResult,
};
#[cfg(test)]
mod skill_descriptor_tests;
#[cfg(test)]
mod skill_review_tests;
#[cfg(test)]
mod skill_tests;
mod skills;
mod summary_view;
mod tape_view;
mod usage;
pub(crate) mod welcome;
pub use project::{
    ProjectAccess, ProjectControl, ProjectControlFuture, ProjectControlHandler,
    ProjectControlResult, ProjectMountReceipt,
};
mod attachment_io;
mod editor;
mod git_status;
mod help;
mod man_view;
mod stdio_completion;
mod stdio_json;
mod stdio_runner;
pub use stdio_json::{StdioTaskOptions, run_stdio_task_with_options};
pub use stdio_runner::run_stdio_task;
use stdio_runner::{CommandOutput, StdioTaskSnapshot, StdioTaskWaitContext};
#[cfg(test)]
use stdio_runner::{
    StdioTaskOutput, finish_stdio_task_if_ready, stdio_task_snapshot_from_history,
    submit_stdio_task, wait_for_stdio_answer, wait_for_stdio_answer_after_submit,
};
mod watchers;
use watchers::AgentWatchers;
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
    spawn_output_tail, spawn_request_watch, spawn_tape_watch, spawn_terminal_events_with_stop,
    spawn_ui_watch, sync_action_from_file, sync_requests_from_files, write_agent_input,
    write_machine_ctl, write_request_response,
};
use layout::{
    activity_elapsed_second, frame_needs_redraw, history_prefix_to_drain, inline_viewport_height,
};
#[cfg(test)]
use layout::{draw, live_region_height};
#[cfg(test)]
use tail::{StdioTailAttachment, close_stdio_tails, open_stdio_tail_attachment};
use tail::{current_root_agent_pid, root_agent_path_for_pid, spawn_root_agent_pid_refresh};

use crate::completion::CompletionCandidate;
use crate::composer::Composer;
#[cfg(test)]
use crate::history::HistoryCell;
#[cfg(test)]
use crate::history::{PendingYieldCell, RenderOpts, RunningTool, ToolStatus};
use crate::terminal::{TerminalSession, terminal_capability_error};
#[cfg(test)]
mod background_io_tests;
#[cfg(test)]
mod completion_anchor_tests;
#[cfg(test)]
mod completion_layout_tests;
#[cfg(test)]
mod completion_reference_tests;
mod observation_io;
mod project_dispatch;
mod project_io;
#[cfg(test)]
mod ux_polish_tests;
use project_dispatch::dispatch_with_pending_submissions;

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
    /// Legacy compatibility field; ignored by the file-backed renderer.
    /// `$` candidates come exclusively from the pinned Process skill snapshot.
    pub skill_candidates: Vec<CompletionCandidate>,
    /// Host cwd offered as the initial path in `/project`.
    pub project_candidate: Option<PathBuf>,
    /// Host-local project grant/revoke control for this Alan invocation.
    pub project_control: Option<ProjectControlHandler>,
    /// Active OS sandbox backend name displayed in the status bar when supplied.
    pub sandbox_backend: Option<String>,
    /// Optional explicit invocation trust posture badge (for example `"confined/trusted"`
    /// or `"UNSANDBOXED host-account"`). `None` keeps standard governed status display.
    pub trust_posture_label: Option<String>,
    /// Optional image/content attachments queued for the first user prompt (from `--image`).
    pub initial_attachments: Vec<alan_agent_protocol::ContentPart>,
    /// Host-local interactive login/connection handler for `/login`.
    pub login_control: Option<LoginControlHandler>,
    /// Host-local `/settings` reset handler supplied by the CLI to the TUI.
    pub settings_control: Option<SettingsControlHandler>,
    /// Automatically enter First-Run Setup Wizard on launch when unconfigured.
    pub auto_setup_on_unconfigured: bool,
    /// Pre-mounted project receipt + canonical Host root hydrated on frame 1.
    pub initial_project_mount: Option<(ProjectMountReceipt, PathBuf)>,
    /// Explicit Host report of whether any callable model connection is configured at boot.
    pub connection_configured: Option<bool>,
    /// Optional passive update notice shown in the initial welcome header.
    pub update_notice: Option<String>,
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
            sandbox_backend: None,
            trust_posture_label: None,
            initial_attachments: Vec::new(),
            login_control: None,
            settings_control: None,
            auto_setup_on_unconfigured: false,
            initial_project_mount: None,
            connection_configured: None,
            update_notice: None,
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
    app.theme = crate::theme::Theme::from_env();
    app.set_effective_model(config.effective_model.clone());
    app.set_project_candidate(config.project_candidate.clone());
    if let Some(sandbox) = &config.sandbox_backend {
        app.sandbox_backend = Some(sandbox.clone());
    }
    app.trust_posture_label = config.trust_posture_label.clone();
    app.login_available = config.login_control.is_some();
    app.settings_available = config.settings_control.is_some();
    app.connection_configured = config.connection_configured;
    app.update_notice = config.update_notice.clone();
    if !config.initial_attachments.is_empty() {
        app.pending_attachments = config.initial_attachments.clone();
    }
    if let Some((receipt, completion_root)) = &config.initial_project_mount {
        app.namespace_cwd = PathBuf::from(&receipt.namespace_path);
        app.project = Some(receipt.clone());
        app.project_host_root = Some(completion_root.clone());
        app.set_file_candidates(super::build_file_index(
            completion_root,
            crate::FILE_INDEX_LIMIT,
        ));
    } else if let Some(host_root) = &config.host_file_completion_root {
        app.set_file_candidates(super::build_file_index(host_root, crate::FILE_INDEX_LIMIT));
        app.project_host_root = Some(host_root.clone());
    }
    if let Some(history_path) = &config.history_path {
        app.composer = Composer::from_history_path(history_path.clone());
    }
    let follows_root_agent = config.agent_path == "/agent/root";
    let mut pending_root_agent_turns = VecDeque::<PendingRootAgentTurn>::new();
    let mut terminal = Some(TerminalSession::enter()?);
    let watch_tails = hydrate_and_open_tails(&shell, &config.agent_path, &mut app).await?;
    app.maybe_insert_welcome_header();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<FileBackedEvent>(128);
    let mut reader_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut terminal_reader = spawn_terminal_events_with_stop(tx.clone(), reader_stop.clone());
    let mut had_pending_yield = app.pending_yield.is_some();
    let mut last_osc_title = Some(app.window_title());
    if let Some(title) = last_osc_title.as_deref() {
        crate::terminal::set_terminal_title(title);
    }

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
    let mut last_drawn_second = None;

    let mut jobs = tokio::task::JoinSet::new();
    let model_reads = observation_io::start(shell.clone(), true, tx.clone(), &mut jobs);
    let skill_reads = observation_io::start(shell.clone(), false, tx.clone(), &mut jobs);
    let usage_reads = observation_io::start_usage(shell.clone(), tx.clone(), &mut jobs);
    if let Some(host_root) = app.project_host_root.clone() {
        git_status::spawn_refresh(host_root, &mut jobs, &tx);
    }
    let mut grant_tick = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut grant_read_pending = false;
    let mut active_login_task: Option<(u64, tokio::task::AbortHandle)> = None;
    let mut active_settings_task: Option<(u64, tokio::task::AbortHandle)> = None;
    settings::maybe_start_first_run_setup(&config, &mut app, &tx, &mut active_login_task);
    let mut draw_result = Ok(());
    loop {
        tokio::select! {
            event = receive_file_backed_event(&mut watchers.pending_terminal_events, &mut rx) => {
                let Some(event) = event else {
                    break;
                };
                let was_project_mounted = app.project_host_root.is_some();
                let is_input_completed = matches!(event, FileBackedEvent::Ui(UiEvent::InputCompleted { .. }));
                if let FileBackedEvent::Ui(ref event) = event {
                    observe_root_agent_completion(&mut pending_root_agent_turns, event, &mut app);
                }
                match event {
                    FileBackedEvent::ProjectHostCompleted { owner, command, result } => {
                        let current = watchers.root_agent_pid.and_then(|pid| root_agent_path_for_pid(&app.agent_path, pid));
                        if let Some((owner, id, command)) = project_io::finish(&mut app, owner, current, command, result) {
                            project_io::write_cwd(&mut app, &shell, owner, id, command, &mut jobs, &tx);
                        }
                    }
                    FileBackedEvent::ProjectCwdWritten { owner, id, result, owner_current } => {
                        if app.pending_project_control.as_ref().is_some_and(|p| p.owner == owner && p.id == id) {
                            if !owner_current { app.fence_project_control(); }
                            else if let Err(error) = result { app.fail_project_control(format!("project control write failed; effects uncertain: {error}")); }
                        }
                    }
                    FileBackedEvent::ProjectGrantObserved { grant_id, active } => {
                        grant_read_pending = false;
                        project_io::observe_grant(&mut app, &grant_id, active);
                    }
                    FileBackedEvent::ProjectGrantLocated { owner, cwd, result } => {
                        let current = watchers.root_agent_pid.and_then(|pid| root_agent_path_for_pid(&app.agent_path, pid));
                        let blocked = project_dispatch::unsettled_submissions(&app, &pending_root_agent_turns);
                        if let Some((owner, id, command)) = project_io::finish_discovery(&mut app, owner, current, cwd, blocked, result) {
                            project_io::write_cwd(&mut app, &shell, owner, id, command, &mut jobs, &tx);
                        }
                    }
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
                    event @ (FileBackedEvent::QueueChanged { .. }
                    | FileBackedEvent::QueueUnavailable { .. }) => {
                        queue::dispatch_queue_event(&shell, &mut app, &pending_root_agent_turns, event).await;
                    }
                    FileBackedEvent::ModelChanged { owner } => {
                        if owner == app.model.owner { observation_io::request(&model_reads, owner); }
                    }
                    FileBackedEvent::ModelUnavailable { owner } => {
                        if owner == app.model.owner {
                            observation_io::request(&model_reads, String::new());
                            app.model.apply(&owner, None);
                            app.reconcile_model_chooser();
                        }
                    }
                    FileBackedEvent::SkillsChanged { owner } => {
                        if owner == app.skills.owner { observation_io::request(&skill_reads, owner); }
                    }
                    FileBackedEvent::SkillsUnavailable { owner } => {
                        if owner == app.skills.owner {
                            observation_io::request(&skill_reads, String::new());
                            app.apply_skills(&owner, None);
                        }
                    }
                    FileBackedEvent::UsageChanged { owner } => {
                        if owner == app.usage.owner {
                            observation_io::request(&usage_reads, owner);
                        }
                    }
                    FileBackedEvent::UsageUnavailable { owner } => {
                        if owner == app.usage.owner {
                            observation_io::request(&usage_reads, String::new());
                            app.usage.apply(&owner, None);
                        }
                    }
                    FileBackedEvent::ObservationRead { revision, owner, snapshot } => {
                        let request = match &snapshot {
                            observation_io::Snapshot::Model(_) => &model_reads,
                            observation_io::Snapshot::Skills(_) => &skill_reads,
                            observation_io::Snapshot::Usage(_) => &usage_reads,
                        };
                        observation_io::apply(&mut app, request, revision, &owner, snapshot);
                    }
                    FileBackedEvent::HistorySwitchCompleted { rollout_id, result } => {
                        match result {
                            Ok(()) => {
                                pending_root_agent_turns.clear();
                                app.reset_for_rollout_switch(rollout_id.as_deref());
                                root_agent_pid_retry.send_replace(());
                            }
                            Err(error) => app.push_error(format!("rollout switch failed: {error}")),
                        }
                    }
                    FileBackedEvent::LoginStepCompleted { generation, result } => {
                        if active_login_task
                            .as_ref()
                            .is_some_and(|(task_gen, _)| *task_gen == generation)
                        {
                            active_login_task = None;
                        }
                        if let Some((connected_profile, connected_model)) =
                            app.apply_login_step_result(generation, result)
                        {
                            let owner = watchers
                                .root_agent_pid
                                .and_then(|pid| root_agent_path_for_pid(&app.agent_path, pid))
                                .unwrap_or_else(|| app.model.owner.clone());
                            login::spawn_post_login_model_sync(
                                config.root_transport.clone(),
                                owner,
                                connected_profile,
                                connected_model,
                                model_reads.clone(),
                            );
                        }
                    }
                    FileBackedEvent::SettingsCompleted { generation, result } => {
                        let owner = watchers
                            .root_agent_pid
                            .and_then(|pid| root_agent_path_for_pid(&app.agent_path, pid))
                            .unwrap_or_else(|| app.model.owner.clone());
                        settings::handle_settings_completed(
                            &config,
                            &mut app,
                            &tx,
                            owner,
                            generation,
                            result,
                            &mut active_settings_task,
                            &mut active_login_task,
                            &mut pending_root_agent_turns,
                            &model_reads,
                        );
                    }
                    FileBackedEvent::GitStatusUpdated { root, status } => {
                        if app.project_host_root.as_ref() == Some(&root) {
                            app.git_status = status;
                        }
                    }
                    FileBackedEvent::FileIndexUpdated { root, candidates } => {
                        if app.project_host_root.as_ref() == Some(&root) {
                            app.set_file_candidates(candidates);
                        }
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
                        } else {
                            if let Some(grant_id) = app.take_ready_project_revoke()
                                && let Some(handler) = config.project_control.as_ref()
                            {
                                let owner = app.pending_project_control.as_ref().map(|p| p.owner.clone()).unwrap_or_default();
                                project_io::start(&mut app, handler, ProjectControl::Revoke { grant_id }, owner, &mut jobs, &tx);
                            }
                            if let Some(root) = app.project_host_root.clone() {
                                git_status::spawn_refresh(root, &mut jobs, &tx);
                            }
                        }
                    }
                    other => {
                        if let Some(action) = dispatch_with_pending_submissions(
                            &mut app,
                            other,
                            &pending_root_agent_turns,
                        ) {
                            match action {
                                FileBackedAction::Submit(record) => {
                                    let submitted_at_ms = unix_time_ms();
                                    let text = record.body.clone();
                                    match write_agent_input(&shell, &app.agent_path, watchers.root_agent_pid, &record).await {
                                        Ok(()) => {
                                            let owner = if follows_root_agent { watchers.root_agent_pid.map(|pid| format!("/agent/{pid}")).unwrap_or_else(|| app.agent_path.clone()) } else { app.agent_path.clone() };
                                            app.track_local_input(&record.submission_id, owner, text.clone(), record.intent);
                                            app.show_submission_sent(&record.submission_id);
                                            pending_root_agent_turns.push_back(PendingRootAgentTurn {
                                                input: text.clone(),
                                                submission_id: record.submission_id.clone(),
                                                submitted_process: if follows_root_agent { watchers.root_agent_pid } else { app.agent_path.strip_prefix("/agent/").and_then(|pid| pid.parse().ok()) },
                                                submitted_at_ms,
                                            });
                                            if let Some(host_root) = app.project_host_root.clone()
                    && (is_input_completed || (!was_project_mounted))
                {
                    git_status::spawn_refresh(host_root.clone(), &mut jobs, &tx);
                    let refresh_tx = tx.clone();
                    jobs.spawn(async move {
                        let candidates = tokio::task::spawn_blocking(move || {
                            (
                                crate::build_file_index(&host_root, crate::FILE_INDEX_LIMIT),
                                host_root,
                            )
                        })
                        .await
                        .ok();
                        if let Some((candidates, root)) = candidates {
                            let _ = refresh_tx
                                .send(FileBackedEvent::FileIndexUpdated { root, candidates })
                                .await;
                        }
                    });
                }
                if follows_root_agent {
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
                                FileBackedAction::SelectModel { owner, id, op } => {
                                    model::start_selection(&shell, &mut app, owner, id, op, &mut jobs, &tx);
                                }
                                action @ (FileBackedAction::Project(_) | FileBackedAction::RevokeCurrentProject) => {
                                    let Some(handler) = config.project_control.as_ref() else {
                                        app.push_error("local project selection is unavailable in this Host".into());
                                        dirty = true;
                                        continue;
                                    };
                                    let Some(owner) = watchers.root_agent_pid.and_then(|pid| root_agent_path_for_pid(&app.agent_path, pid)) else {
                                        app.push_error("Root owner unavailable; project operation not sent".into());
                                        continue;
                                    };
                                    match action {
                                        FileBackedAction::Project(command @ ProjectControl::Mount { .. }) => {
                                            project_io::start(&mut app, handler, command, owner, &mut jobs, &tx);
                                        }
                                        FileBackedAction::Project(ProjectControl::Revoke { grant_id }) => {
                                            let (id, command) = project_dispatch::project_selector("/");
                                            app.stage_project_control(owner.clone(), id.clone(), None, Some(grant_id));
                                            project_io::write_cwd(&mut app, &shell, owner, id, command, &mut jobs, &tx);
                                        }
                                        FileBackedAction::RevokeCurrentProject => {
                                            project_io::start_discovery(&mut app, &shell, owner, &mut jobs, &tx);
                                        }
                                        _ => unreachable!(),
                                    }
                                }
                                FileBackedAction::OpenExternalEditor => {
                                    reader_stop.store(true, std::sync::atomic::Ordering::SeqCst);
                                    let _ = terminal_reader.await;
                                    drop(terminal.take());
                                    let draft = app.composer.text().to_owned();
                                    let edit_result = tokio::task::spawn_blocking(move || {
                                        editor::edit_text_in_external_editor(&draft)
                                    })
                                    .await;
                                    terminal = Some(TerminalSession::enter()?);
                                    last_osc_title = None;
                                    reader_stop = std::sync::Arc::new(
                                        std::sync::atomic::AtomicBool::new(false),
                                    );
                                    terminal_reader = spawn_terminal_events_with_stop(
                                        tx.clone(),
                                        reader_stop.clone(),
                                    );
                                    match edit_result {
                                        Ok(Ok(Some(updated))) => {
                                            app.composer.apply_external_edit(updated);
                                            app.refresh_completion();
                                        }
                                        Ok(Ok(None)) => {}
                                        Ok(Err(error)) => app
                                            .push_error(format!("external editor failed: {error:#}")),
                                        Err(error) => app.push_error(format!(
                                            "external editor task failed: {error}"
                                        )),
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
                                FileBackedAction::ReadTape => {
                                    let owner = watchers
                                        .root_agent_pid
                                        .and_then(|pid| root_agent_path_for_pid(&app.agent_path, pid))
                                        .unwrap_or_else(|| app.agent_path.clone());
                                    tape_view::start(&shell, &mut app, &tx, &owner);
                                }
                                FileBackedAction::ReadMan { topic, generation } => {
                                    man_view::spawn_read(&shell, &tx, topic, generation);
                                }
                                FileBackedAction::ReadHistoryCatalog { generation } => {
                                    resume_picker::spawn_load_catalog(&shell, &tx, generation);
                                }
                                FileBackedAction::SwitchRollout { rollout_id } => {
                                    resume_picker::spawn_switch_rollout(&shell, &tx, rollout_id);
                                }
                                FileBackedAction::InspectProjectSummary => {
                                    summary_view::spawn_inspect(&shell, &tx);
                                }
                                FileBackedAction::EditProjectSummary {
                                    text,
                                    expected_revision,
                                } => {
                                    summary_view::spawn_edit(&shell, &tx, text, expected_revision);
                                }
                                FileBackedAction::DeleteProjectSummary { expected_revision } => {
                                    summary_view::spawn_delete(&shell, &tx, expected_revision);
                                }
                                FileBackedAction::LoginListProviders { generation } => {
                                    if let Some((_, prev)) = active_login_task.take() {
                                        prev.abort();
                                    }
                                    if let Some(handler) = config.login_control.clone() {
                                        let abort = login::spawn_list_providers(
                                            handler,
                                            tx.clone(),
                                            generation,
                                        );
                                        active_login_task = Some((generation, abort));
                                    }
                                }
                                FileBackedAction::LoginSubmit {
                                    generation,
                                    submission,
                                    progress_rx,
                                } => {
                                    if let Some((_, prev)) = active_login_task.take() {
                                        prev.abort();
                                    }
                                    if let Some(handler) = config.login_control.clone() {
                                        let abort = login::spawn_login_submit(
                                            handler,
                                            tx.clone(),
                                            generation,
                                            submission,
                                            progress_rx,
                                        );
                                        active_login_task = Some((generation, abort));
                                    }
                                }
                                FileBackedAction::CancelLogin => {
                                    if let Some((_, prev)) = active_login_task.take() {
                                        prev.abort();
                                    }
                                }
                                FileBackedAction::SettingsControl { generation, control } => {
                                    settings::dispatch_settings_action(
                                        &config,
                                        &mut app,
                                        &tx,
                                        generation,
                                        control,
                                        &mut active_settings_task,
                                    );
                                }
                                FileBackedAction::Suspend => {
                                    reader_stop.store(true, std::sync::atomic::Ordering::SeqCst);
                                    let _ = terminal_reader.await;
                                    drop(terminal.take());
                                    if let Err(error) = crate::terminal::suspend_current_process() {
                                        app.push_error(format!("suspend failed: {error:#}"));
                                    }
                                    terminal = Some(TerminalSession::enter()?);
                                    last_osc_title = None;
                                    reader_stop = std::sync::Arc::new(
                                        std::sync::atomic::AtomicBool::new(false),
                                    );
                                    terminal_reader = spawn_terminal_events_with_stop(
                                        tx.clone(),
                                        reader_stop.clone(),
                                    );
                                    last_drawn_second = None;
                                }
                                FileBackedAction::Quit => break,
                            }
                        }
                    }
                }
                if let Some(host_root) = app.project_host_root.clone()
                    && (is_input_completed || (!was_project_mounted))
                {
                    git_status::spawn_refresh(host_root.clone(), &mut jobs, &tx);
                    let refresh_tx = tx.clone();
                    jobs.spawn(async move {
                        let candidates = tokio::task::spawn_blocking(move || {
                            (
                                crate::build_file_index(&host_root, crate::FILE_INDEX_LIMIT),
                                host_root,
                            )
                        })
                        .await
                        .ok();
                        if let Some((candidates, root)) = candidates {
                            let _ = refresh_tx
                                .send(FileBackedEvent::FileIndexUpdated { root, candidates })
                                .await;
                        }
                    });
                }
                if follows_root_agent {
                    settle_unknown_replaced_input(&mut pending_root_agent_turns, watchers.root_agent_pid, &mut app);
                }
                if app.take_ready_auto_continue_on_empty_cancel() {
                    let owner = watchers
                        .root_agent_pid
                        .and_then(|pid| root_agent_path_for_pid(&app.agent_path, pid))
                        .unwrap_or_else(|| app.agent_path.clone());
                    spawn_control_write(
                        config.root_transport.clone(),
                        tx.clone(),
                        owner,
                        "queue-v1 continue".to_string(),
                        "cancelled — ready".to_string(),
                        "auto-continue failed".to_string(),
                    );
                }
                if !app.project_host_pending && let Some(grant_id) = app.take_project_cleanup() {
                    if let Some(handler) = config.project_control.as_ref() {
                        let owner = watchers.root_agent_pid.and_then(|pid| root_agent_path_for_pid(&app.agent_path, pid)).unwrap_or_default();
                        project_io::start(&mut app, handler, ProjectControl::Revoke { grant_id }, owner, &mut jobs, &tx);
                    } else {
                        app.push_error(format!("candidate grant {grant_id} cleanup unavailable"));
                    }
                }
                action_detail_io::start_pending_for_pid(&shell, &mut app, &tx, watchers.root_agent_pid);
                let has_yield = app.pending_yield.is_some();
                if has_yield && !had_pending_yield {
                    crate::terminal::ring_terminal_bell();
                }
                had_pending_yield = has_yield;
                let osc_title = app.window_title();
                if last_osc_title.as_ref() != Some(&osc_title) {
                    crate::terminal::set_terminal_title(&osc_title);
                    last_osc_title = Some(osc_title);
                }
                dirty = true;
            }
            _ = jobs.join_next(), if !jobs.is_empty() => {}
            _ = grant_tick.tick(), if !grant_read_pending => {
                if let Some(project) = app.project.as_ref()
                    && app.pending_project_control.is_none() && !app.project_host_pending {
                    grant_read_pending = true;
                    let grant_id = project.grant_id.clone();
                    let shell = shell.clone();
                    let tx = tx.clone();
                    jobs.spawn(async move {
                        let active = project_io::grant_active(&shell, &grant_id).await;
                        let _ = tx.send(FileBackedEvent::ProjectGrantObserved { grant_id, active }).await;
                    });
                }
            }
            _ = frame_tick.tick() => {
                let now_ms = unix_time_ms();
                if frame_needs_redraw(dirty, &app, now_ms, last_drawn_second)
                    && let Some(active_terminal) = terminal.as_mut()
                {
                    let (viewport_width, terminal_height) = active_terminal.viewport_size();
                    let committed = app.drain_committed_scrollback(viewport_width, terminal_height);
                    draw_result = active_terminal.draw_inline_frame(&committed, inline_viewport_height(
                        &app,
                        viewport_width,
                        terminal_height,
                    ), layout::base_viewport_height(&app, viewport_width, terminal_height), |frame| layout::draw_at(frame, &app, now_ms));
                    if draw_result.is_err() { break; }
                    // Record only successfully drawn seconds on the existing frame tick.
                    last_drawn_second = Some(activity_elapsed_second(&app, now_ms));
                    dirty = false;
                }
                if app.should_quit {
                    break;
                }
            }
        }
    }

    if let Some((_, prev)) = active_login_task.take() {
        prev.abort();
    }
    if let Some((_, prev)) = active_settings_task.take() {
        prev.abort();
    }
    jobs.abort_all();
    while jobs.join_next().await.is_some() {}
    watchers.stop().await;
    drop(rx);
    if let Some(task) = root_agent_pid_refresh {
        let _ = task.await;
    }
    terminal_reader
        .await
        .context("terminal reader task failed")?;

    draw_result?;
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
#[path = "file_backed/accessibility_tests.rs"]
mod accessibility_tests;
#[cfg(test)]
mod action_details_tests;
#[cfg(test)]
#[path = "file_backed/grouping_tests.rs"]
mod grouping_tests;
#[cfg(test)]
mod project_review_tests;
#[cfg(test)]
mod queue_tests;

#[cfg(test)]
#[path = "file_backed/semantic_tests.rs"]
mod semantic_tests;
#[cfg(test)]
#[path = "file_backed/shadow_admission_tests.rs"]
mod shadow_admission_tests;

#[cfg(test)]
mod budget_confirmation_tests;
#[cfg(test)]
#[path = "file_backed/stdio_idle_tests.rs"]
mod stdio_idle_tests;
#[cfg(test)]
mod stdio_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "file_backed/ux_m2_tests.rs"]
mod ux_m2_tests;
#[cfg(test)]
#[path = "file_backed/ux_m3_tests.rs"]
mod ux_m3_tests;
#[cfg(test)]
#[path = "file_backed/ux_m6_tests.rs"]
mod ux_m6_tests;
#[cfg(test)]
#[path = "file_backed/ux_tests.rs"]
mod ux_tests;
