//! Async workstream control, `/mnt/route/log` watcher, `/streams` & `/focus` & `/root` handlers,
//! and non-owning `/agent/<pid>` <-> `/agent/root` switching.

use std::collections::{BTreeMap, VecDeque};

use serde_json::Value;

use super::FileBackedApp;
use super::app::{FileBackedAction, FileBackedEvent, Notice};
use super::file_surface::{ActionSnapshot, hydrate_focus_target};
use super::interrupt::PendingRootAgentTurn;
use super::observation_io;
use super::streams::{
    WorkstreamControl, WorkstreamControlHandler, WorkstreamControlResult, WorkstreamCountsDto,
    WorkstreamEntryDto, WorkstreamFocusTarget, WorkstreamOverviewDto, WorkstreamPersistenceKind,
    WorkstreamStatusKind, format_streams_overview_lines, parse_route_log_line,
};
use super::watchers::AgentWatchers;
use crate::history::HistoryCell;

pub(in crate::file_backed) const ROUTE_LOG_PATH: &str = "/mnt/route/log";

impl FileBackedApp {
    pub(in crate::file_backed) fn streams_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        let trimmed = argument.trim();
        if trimmed.is_empty() || trimmed == "show" || trimmed == "list" {
            self.notice = Some("Loading workstreams…".into());
            return Some(FileBackedAction::StreamsControl {
                control: WorkstreamControl::Inspect,
                show_overview: true,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("focus ") {
            return self.focus_command(rest);
        }
        if let Some(rest) = trimmed.strip_prefix("archive ") {
            let name = rest.trim();
            if name.is_empty() {
                self.notice = Some(Notice::warning("usage: /streams archive <name>"));
                return None;
            }
            if self.streams.focused_stream.as_deref() == Some(name) {
                self.notice = Some(Notice::warning(format!(
                    "Cannot archive currently focused stream '{name}'; run /root first."
                )));
                return None;
            }
            self.notice = Some(format!("Archiving workstream '{name}'…").into());
            return Some(FileBackedAction::StreamsControl {
                control: WorkstreamControl::Archive {
                    name: name.to_string(),
                },
                show_overview: false,
            });
        }
        if matches!(trimmed, "clear-inbox" | "ack" | "ack-inbox") {
            self.notice = Some("Acknowledging unread inbox digests…".into());
            return Some(FileBackedAction::StreamsControl {
                control: WorkstreamControl::ClearInbox,
                show_overview: false,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("resume ") {
            let name = rest.trim();
            if name.is_empty() {
                self.notice = Some(Notice::warning("usage: /streams resume <name>"));
                return None;
            }
            self.notice = Some(format!("Resuming workstream '{name}'…").into());
            return Some(FileBackedAction::StreamsControl {
                control: WorkstreamControl::Resume {
                    name: name.to_string(),
                },
                show_overview: false,
            });
        }
        self.notice = Some(Notice::warning(
            "usage: /streams [show|focus <name>|archive <name>|clear-inbox|resume <name>]",
        ));
        None
    }

    pub(in crate::file_backed) fn focus_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        let name = argument.trim();
        if name.is_empty() {
            self.notice = Some(Notice::warning("usage: /focus <name>"));
            return None;
        }
        if name == "root" {
            return self.root_command("");
        }
        if !self.focus_switch_boundary_available() {
            self.notice = Some(Notice::warning(
                "Close the active modal or confirmation before switching focus.",
            ));
            return None;
        }
        if self.streams.focused_stream.as_deref() == Some(name) {
            self.notice = Some(format!("Already focused on stream '{name}'.").into());
            return None;
        }
        self.streams.pending_focus_name = Some(name.to_string());
        self.notice = Some(format!("Switching focus to stream '{name}'…").into());
        Some(FileBackedAction::FocusStream {
            name: name.to_string(),
        })
    }

    pub(in crate::file_backed) fn root_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        if !argument.trim().is_empty() {
            self.notice = Some(Notice::warning("usage: /root"));
            return None;
        }
        if !self.focus_switch_boundary_available() {
            self.notice = Some(Notice::warning(
                "Close the active modal or confirmation before returning to Root.",
            ));
            return None;
        }
        if self.agent_path == "/agent/root" && self.streams.focused_stream.is_none() {
            self.notice = Some("Already focused on Root (/agent/root).".into());
            return None;
        }
        self.streams.pending_focus_name = None;
        self.notice = Some("Returning focus to Root (/agent/root)…".into());
        Some(FileBackedAction::FocusRoot)
    }

    fn focus_switch_boundary_available(&self) -> bool {
        !self.modal.active
            && !self.resume_picker.active
            && !self.issue.is_active()
            && self.form.is_none()
            && self.confirmation.is_none()
            && self.pending_project_control.is_none()
            && self.response_in_flight.is_none()
    }

    pub(in crate::file_backed) fn apply_workstream_control_result(
        &mut self,
        show_overview: bool,
        result: Result<WorkstreamControlResult, String>,
    ) {
        match result {
            Ok(WorkstreamControlResult::Overview(overview)) => {
                self.streams.apply_overview(overview.clone());
                if show_overview {
                    let width = self.viewport_size.0.max(16) as usize;
                    let lines = format_streams_overview_lines(
                        &overview,
                        self.streams.focused_stream.as_deref(),
                        self.theme,
                        width,
                    );
                    self.transcript.push(HistoryCell::Styled(lines));
                    if self
                        .notice
                        .as_deref()
                        .is_some_and(|n| n == "Loading workstreams…")
                    {
                        self.notice = None;
                    }
                }
            }
            Ok(WorkstreamControlResult::ActionCompleted { message, overview }) => {
                self.streams.apply_overview(overview);
                self.notice = Some(message.into());
            }
            Ok(WorkstreamControlResult::FocusResolved(_)) => {}
            Ok(WorkstreamControlResult::FocusUnavailable { message, .. }) => {
                self.streams.pending_focus_name = None;
                self.notice = Some(Notice::warning(message));
            }
            Err(error) => {
                self.notice = Some(Notice::warning(format!("workstreams: {error}")));
            }
        }
    }

    pub(in crate::file_backed) fn apply_focus_target_resolved(
        &mut self,
        requested_name: String,
        result: Result<WorkstreamFocusTarget, String>,
    ) {
        if self.streams.pending_focus_name.as_deref() != Some(requested_name.as_str()) {
            return;
        }
        self.streams.pending_focus_name = None;
        match result {
            Ok(target) => {
                self.reset_for_focus_switch(
                    Some(target.stream_name),
                    target.agent_path,
                    target.pid,
                );
            }
            Err(error) => {
                self.notice = Some(Notice::warning(error));
            }
        }
    }

    pub(in crate::file_backed) fn apply_route_log_line(&mut self, raw_line: &str) {
        let Some(receipt) = parse_route_log_line(raw_line) else {
            return;
        };
        if !self.streams.record_route_key(receipt.dedup_key) {
            return;
        }
        self.push_turn_preview_cell(HistoryCell::RoutingReceipt {
            source: receipt.source,
            target: receipt.target,
            kind: receipt.kind,
            summary: receipt.summary,
        });
    }

    /// Track live `stream_name -> child_pid` mappings and emit calm dispatch receipts
    /// when `spawn_subagent` actions expose named workstream metadata.
    pub(in crate::file_backed) fn observe_workstream_action(&mut self, snapshot: &ActionSnapshot) {
        let Ok(metadata) = serde_json::from_str::<Value>(&snapshot.result) else {
            return;
        };
        let stream_name = metadata
            .get("stream_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let child_pid = parse_child_pid_value(metadata.get("child_pid"));
        let child_state = metadata
            .get("child_state")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let status = snapshot.status.trim();

        if let Some(name) = stream_name {
            if let Some(pid) = child_pid.filter(|pid| *pid > 0) {
                if matches!(status, "running" | "pending")
                    && !matches!(child_state, "completed" | "failed" | "cancelled")
                {
                    self.streams.observe_stream_pid(name, pid);
                } else if matches!(status, "completed" | "failed" | "rejected" | "cancelled") {
                    self.streams.remove_stream_pid(name);
                }
            }
            if matches!(status, "running" | "pending") {
                let key = format!("dispatch:{}:{name}", snapshot.id);
                if self.streams.record_route_key(key) {
                    self.push_turn_preview_cell(HistoryCell::RoutingReceipt {
                        source: "root".to_string(),
                        target: name.to_string(),
                        kind: "dispatch".to_string(),
                        summary: child_pid
                            .filter(|pid| *pid > 0)
                            .map(|pid| format!("pid {pid}")),
                    });
                }
            }
        }
    }

    pub(in crate::file_backed) fn reset_for_focus_switch(
        &mut self,
        target_name: Option<String>,
        target_agent_path: String,
        target_pid: Option<u64>,
    ) {
        let previous_label = self.streams.focus_label();
        let next_label = target_name.as_deref().unwrap_or("root").to_string();

        self.streams.pending_focus_name = None;
        self.streams.focused_stream = target_name;
        self.streams.focused_pid = target_pid;
        self.agent_path = target_agent_path.clone();

        self.lose_model_receipts(&self.model.owner.clone());
        self.model_chooser.active = false;
        self.model_chooser.catalog = None;
        self.model = super::model::ModelProjection::default();
        self.usage = super::usage::UsageProjection::default();
        self.skills = super::skills::SkillProjection::default();
        self.completion_sources.skills.clear();
        self.queue = super::queue::QueueProjection::default();
        for input in self.local_inputs.values_mut() {
            input.cell = None;
        }
        self.action_cells.clear();
        self.running_tools.clear();
        self.pending_yield = None;
        self.form = None;
        self.confirmation = None;
        self.staged_confirmation_outcome = None;
        self.completion = None;
        self.activity = alan_agent_protocol::UiActivitySnapshot::idle();
        self.plan = alan_agent_protocol::UiPlanSnapshot::empty();
        self.plan_revision = 0;
        self.thinking = alan_agent_protocol::UiThinkingSnapshot::idle();
        self.last_input_failed = false;
        self.auto_continue_on_empty_cancel = false;
        self.expected_terminal_error = None;
        self.reconciler = crate::reconcile::StreamReconciler::new();
        self.pending_remote_turn_start = None;
        self.clear_late_thinking_latch();
        self.active_turn_has_user_boundary = false;
        self.tape_consumed_offset = 0;

        self.transcript.push(HistoryCell::RoutingReceipt {
            source: previous_label,
            target: next_label.clone(),
            kind: "focus".to_string(),
            summary: Some(target_agent_path),
        });
        self.notice = Some(format!("Focused [{next_label}].").into());
    }
}

pub(in crate::file_backed) fn spawn_route_log_watch(
    shell: alan_shell::Shell,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut tail) = shell.tail(ROUTE_LOG_PATH).await else {
            return;
        };
        let mut pending = Vec::new();
        loop {
            tokio::select! {
                _ = tx.closed() => break,
                result = tail.read(4096) => {
                    match result {
                        Ok(bytes) if bytes.is_empty() => break,
                        Ok(bytes) => {
                            pending.extend_from_slice(&bytes);
                            for line in drain_lines(&mut pending) {
                                if let Ok(text) = String::from_utf8(line)
                                    && tx.send(FileBackedEvent::RouteLogLine(text)).await.is_err()
                                {
                                    let _ = tail.close().await;
                                    return;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        let _ = tail.close().await;
    })
}

pub(in crate::file_backed) fn spawn_workstream_control(
    shell: alan_shell::Shell,
    handler: Option<WorkstreamControlHandler>,
    fallback_overview: Option<WorkstreamOverviewDto>,
    live_pids: BTreeMap<String, u64>,
    control: WorkstreamControl,
    show_overview: bool,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    tokio::spawn(async move {
        let result = match handler {
            Some(handler) => handler(control).await.map_err(|err| format!("{err:#}")),
            None => {
                execute_namespace_fallback(&shell, fallback_overview, &live_pids, control).await
            }
        };
        let _ = tx
            .send(FileBackedEvent::WorkstreamControlCompleted {
                show_overview,
                result,
            })
            .await;
    });
}

pub(in crate::file_backed) fn spawn_focus_resolution(
    shell: alan_shell::Shell,
    handler: Option<WorkstreamControlHandler>,
    live_pids: BTreeMap<String, u64>,
    name: String,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    tokio::spawn(async move {
        let result = resolve_focus_target(&shell, handler.as_ref(), &live_pids, &name).await;
        let _ = tx
            .send(FileBackedEvent::FocusTargetResolved {
                requested_name: name,
                result,
            })
            .await;
    });
}

async fn resolve_focus_target(
    shell: &alan_shell::Shell,
    handler: Option<&WorkstreamControlHandler>,
    live_pids: &BTreeMap<String, u64>,
    name: &str,
) -> Result<WorkstreamFocusTarget, String> {
    if let Some(&pid) = live_pids.get(name) {
        let agent_path = format!("/agent/{pid}");
        if shell.ls(&agent_path).await.is_ok() {
            return Ok(WorkstreamFocusTarget {
                stream_name: name.to_string(),
                agent_path,
                pid: Some(pid),
            });
        }
    }

    if let Some(handler) = handler {
        match handler(WorkstreamControl::ResolveFocus {
            name: name.to_string(),
        })
        .await
        {
            Ok(WorkstreamControlResult::FocusResolved(target)) => return Ok(target),
            Ok(WorkstreamControlResult::FocusUnavailable { message, .. }) => {
                if let Some(target) = scan_children_for_stream(shell, name).await {
                    return Ok(target);
                }
                return Err(message);
            }
            Ok(_) => {}
            Err(err) => return Err(format!("{err:#}")),
        }
    }

    if let Some(target) = scan_children_for_stream(shell, name).await {
        return Ok(target);
    }
    Err(format!(
        "stream '{name}' has no live /agent/<pid> process to focus"
    ))
}

async fn scan_children_for_stream(
    shell: &alan_shell::Shell,
    name: &str,
) -> Option<WorkstreamFocusTarget> {
    if let Ok(pid) = name.parse::<u64>() {
        let path = format!("/agent/{pid}");
        if shell.ls(&path).await.is_ok() {
            return Some(WorkstreamFocusTarget {
                stream_name: name.to_string(),
                agent_path: path,
                pid: Some(pid),
            });
        }
    }
    let children = shell.ls("/agent/root/children").await.ok()?;
    for child in children {
        let Ok(pid) = child.trim().parse::<u64>() else {
            continue;
        };
        let agent_path = format!("/agent/{pid}");
        if shell.ls(&agent_path).await.is_err() {
            continue;
        }
        if let Ok(args_bytes) = shell.cat(&format!("/proc/{pid}/args")).await {
            let args = String::from_utf8_lossy(&args_bytes);
            if args.lines().any(|line| line.trim() == name) {
                return Some(WorkstreamFocusTarget {
                    stream_name: name.to_string(),
                    agent_path,
                    pid: Some(pid),
                });
            }
        }
    }
    None
}

async fn execute_namespace_fallback(
    shell: &alan_shell::Shell,
    fallback_overview: Option<WorkstreamOverviewDto>,
    live_pids: &BTreeMap<String, u64>,
    control: WorkstreamControl,
) -> Result<WorkstreamControlResult, String> {
    match control {
        WorkstreamControl::Inspect => {
            let mut overview = fallback_overview.unwrap_or_default();
            let children = shell.ls("/agent/root/children").await.unwrap_or_default();
            for (name, pid) in live_pids {
                if !overview.streams.iter().any(|entry| &entry.name == name) {
                    overview.streams.push(WorkstreamEntryDto {
                        name: name.clone(),
                        scope: String::new(),
                        persistence: WorkstreamPersistenceKind::OneOff,
                        state: WorkstreamStatusKind::Running,
                        child_pid: Some(*pid),
                        model_override: None,
                        rollout_ref: None,
                        updated_at: String::new(),
                    });
                }
            }
            for child in children {
                let Ok(pid) = child.trim().parse::<u64>() else {
                    continue;
                };
                if !overview
                    .streams
                    .iter()
                    .any(|entry| entry.child_pid == Some(pid))
                {
                    overview.streams.push(WorkstreamEntryDto {
                        name: format!("pid-{pid}"),
                        scope: format!("/agent/{pid}"),
                        persistence: WorkstreamPersistenceKind::OneOff,
                        state: WorkstreamStatusKind::Running,
                        child_pid: Some(pid),
                        model_override: None,
                        rollout_ref: None,
                        updated_at: String::new(),
                    });
                }
            }
            let running = overview
                .streams
                .iter()
                .filter(|entry| entry.state == WorkstreamStatusKind::Running)
                .count();
            let idle = overview
                .streams
                .iter()
                .filter(|entry| entry.state == WorkstreamStatusKind::Idle)
                .count();
            let paused_error = overview
                .streams
                .iter()
                .filter(|entry| entry.state == WorkstreamStatusKind::PausedError)
                .count();
            let archived = overview
                .streams
                .iter()
                .filter(|entry| entry.state == WorkstreamStatusKind::Archived)
                .count();
            overview.counts = WorkstreamCountsDto {
                running,
                idle,
                paused_error,
                archived,
                unread_inbox: overview.unread_inbox.len(),
            };
            Ok(WorkstreamControlResult::Overview(overview))
        }
        WorkstreamControl::ResolveFocus { name } => {
            resolve_focus_target(shell, None, live_pids, &name)
                .await
                .map(WorkstreamControlResult::FocusResolved)
        }
        WorkstreamControl::Archive { .. }
        | WorkstreamControl::ClearInbox
        | WorkstreamControl::Resume { .. } => {
            Err("workstream store actions require an active bound project".to_string())
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Focus switching coordinates watchers, app state, and observation channels explicitly"
)]
pub(in crate::file_backed) async fn switch_agent_target(
    shell: &alan_shell::Shell,
    watchers: &mut AgentWatchers,
    app: &mut FileBackedApp,
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
    pending_root_agent_turns: &mut VecDeque<PendingRootAgentTurn>,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    readers: &observation_io::Readers,
    target_name: Option<String>,
    target_agent_path: String,
    target_pid: Option<u64>,
) -> bool {
    let prev_key = super::streams::StreamsState::target_key(&app.agent_path);
    app.streams
        .tape_offsets_by_target
        .insert(prev_key, app.tape_consumed_offset);

    watchers.stop().await;
    drain_stale_attachment_events(rx, &mut watchers.pending_terminal_events);

    let mut next_app = app.clone();
    next_app.reset_for_focus_switch(target_name, target_agent_path.clone(), target_pid);

    match hydrate_focus_target(shell, &target_agent_path, &mut next_app).await {
        Ok(tails) => {
            *app = next_app;
            *watchers = AgentWatchers::start(tails, &target_agent_path, tx.clone());
            pending_root_agent_turns.clear();
            readers.refresh_all(app);
            true
        }
        Err(err) => {
            app.streams.pending_focus_name = None;
            app.push_error(format!(
                "focus switch to {target_agent_path} failed: {err:#}"
            ));
            false
        }
    }
}

pub(in crate::file_backed) async fn receive_file_backed_event(
    pending_terminal_events: &mut VecDeque<FileBackedEvent>,
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
) -> Option<FileBackedEvent> {
    match pending_terminal_events.pop_front() {
        Some(event) => Some(event),
        None => rx.recv().await,
    }
}

fn parse_child_pid_value(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        .filter(|pid| *pid > 0)
}

pub(in crate::file_backed) fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(in crate::file_backed) fn drain_lines(pending: &mut Vec<u8>) -> Vec<Vec<u8>> {
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

#[allow(
    clippy::too_many_arguments,
    reason = "Focus target resolution handler forwards runtime channels to switch_agent_target"
)]
pub(in crate::file_backed) async fn handle_focus_target_resolved(
    shell: &alan_shell::Shell,
    watchers: &mut AgentWatchers,
    app: &mut FileBackedApp,
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
    pending_root_agent_turns: &mut VecDeque<PendingRootAgentTurn>,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    readers: &observation_io::Readers,
    requested_name: String,
    result: Result<WorkstreamFocusTarget, String>,
) {
    if app.streams.pending_focus_name.as_deref() != Some(requested_name.as_str()) {
        return;
    }
    match result {
        Ok(target) => {
            switch_agent_target(
                shell,
                watchers,
                app,
                rx,
                pending_root_agent_turns,
                tx,
                readers,
                Some(target.stream_name),
                target.agent_path,
                target.pid,
            )
            .await;
        }
        Err(error) => {
            app.streams.pending_focus_name = None;
            app.notice = Some(Notice::warning(error));
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Workstream action dispatcher coordinates watchers, focus switching, and observation channels"
)]
pub(in crate::file_backed) async fn handle_streams_action(
    shell: &alan_shell::Shell,
    workstream_control: &Option<WorkstreamControlHandler>,
    watchers: &mut AgentWatchers,
    app: &mut FileBackedApp,
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
    pending_root_agent_turns: &mut VecDeque<PendingRootAgentTurn>,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    readers: &observation_io::Readers,
    root_agent_pid_retry: &tokio::sync::watch::Sender<()>,
    action: FileBackedAction,
) {
    match action {
        FileBackedAction::StreamsControl {
            control,
            show_overview,
        } => {
            spawn_workstream_control(
                shell.clone(),
                workstream_control.clone(),
                app.streams.overview.clone(),
                app.streams.live_stream_pids.clone(),
                control,
                show_overview,
                tx.clone(),
            );
        }
        FileBackedAction::FocusStream { name } => {
            spawn_focus_resolution(
                shell.clone(),
                workstream_control.clone(),
                app.streams.live_stream_pids.clone(),
                name,
                tx.clone(),
            );
        }
        FileBackedAction::FocusRoot => {
            let switched = switch_agent_target(
                shell,
                watchers,
                app,
                rx,
                pending_root_agent_turns,
                tx,
                readers,
                None,
                "/agent/root".to_string(),
                None,
            )
            .await;
            if switched {
                root_agent_pid_retry.send_replace(());
            }
        }
        _ => {}
    }
}

fn drain_stale_attachment_events(
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
    pending_terminal_events: &mut VecDeque<FileBackedEvent>,
) {
    while let Ok(event) = rx.try_recv() {
        match event {
            event @ FileBackedEvent::Terminal(_)
            | event @ FileBackedEvent::RouteLogLine(_)
            | event @ FileBackedEvent::WorkstreamControlCompleted { .. } => {
                pending_terminal_events.push_back(event);
            }
            _ => {}
        }
    }
}

pub(in crate::file_backed) fn spawn_control_write(
    transport: alan_ap::InProcessTransport,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    agent_path: String,
    command: String,
    success_notice: String,
    error_prefix: String,
) {
    tokio::spawn(async move {
        let shell = alan_shell::Shell::new(transport);
        let result = super::write_machine_ctl(&shell, &agent_path, &command)
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

pub(in crate::file_backed) fn spawn_host_workspace_refresh(
    host_root: std::path::PathBuf,
    jobs: &mut tokio::task::JoinSet<()>,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    super::git_status::spawn_refresh(host_root.clone(), jobs, tx);
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
