//! AgentFS file observation, command writes, and snapshot projection.

use alan_agent_protocol::{
    ContentPart, StructuredInputQuestion, ToolResultPresentation, UiActivitySnapshot,
    UiActivityState, UiEvent, YieldKind,
};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::history::{HistoryCell, PendingYieldCell, RunningTool, ToolStatus};

use super::app::{FileBackedApp, FileBackedEvent};
use super::tail::tail_with_history;
mod action_projection;
mod attachment;
pub(in crate::file_backed) use action_projection::{
    ActionSnapshot, RequestSnapshot, TapeRecordV1, action_snapshot_to_history_cell, action_title,
};

pub(super) use attachment::{hydrate_and_open_tails, reattach_to_current_agent};

pub(super) async fn sync_requests_from_files(
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

pub(super) async fn hydrate_actions_from_files(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<()> {
    let snapshots = read_action_snapshots(shell, agent_path).await?;
    hydrate_actions_from_snapshots(app, snapshots);
    Ok(())
}

pub(super) async fn sync_action_from_file(
    shell: &alan_shell::Shell,
    agent_path: &str,
    action_id: &str,
    app: &mut FileBackedApp,
) -> Result<()> {
    if app.action_owner() != agent_path {
        return Ok(());
    }
    let snapshot = read_action_snapshot(shell, agent_path, action_id).await?;
    app.observe_project_action(agent_path, &snapshot);
    sync_action_snapshot(app, snapshot);
    Ok(())
}

/// Live streams and history from one file-backed renderer attachment.
pub(super) struct WatchTails {
    pub(super) root_agent_pid: Option<u64>,
    pub(super) queue_events: alan_shell::Tail,
    pub(super) output: alan_shell::Tail,
    pub(super) requests: alan_shell::Tail,
    pub(super) actions: alan_shell::Tail,
    pub(super) ui: alan_shell::Tail,
    pub(super) tape: alan_shell::Tail,
    pub(super) recovery_ui: alan_shell::Tail,
    pub(super) recovery_tape: alan_shell::Tail,
    pub(super) ui_history: Vec<u8>,
    pub(super) tape_history: Vec<u8>,
}

#[derive(Default)]
pub(super) struct CorrelatedUiTask {
    pub(super) started: bool,
    pub(super) state: Option<UiActivityState>,
    pub(super) error: Option<String>,
}

pub(super) fn correlated_ui_task(
    ui_history: &[u8],
    submitted_at_ms: u64,
) -> Result<CorrelatedUiTask> {
    let ui_history = std::str::from_utf8(ui_history).context("ui events are not utf8")?;
    let mut task = CorrelatedUiTask::default();
    for line in ui_history.lines().filter(|line| !line.trim().is_empty()) {
        let event = serde_json::from_str::<UiEvent>(line).context("parse Agent UI event")?;
        if !task.started {
            if let UiEvent::Activity { snapshot } = event
                && snapshot.state == UiActivityState::Running
                && snapshot
                    .started_at_ms
                    .is_some_and(|started_at| started_at >= submitted_at_ms)
            {
                task.started = true;
                task.state = Some(UiActivityState::Running);
            }
            continue;
        }
        match event {
            UiEvent::Activity { snapshot } => {
                task.state = Some(snapshot.state);
                if snapshot.state == UiActivityState::Idle {
                    break;
                }
            }
            UiEvent::Error { message, .. } => task.error = Some(message),
            UiEvent::InputCompleted { .. }
            | UiEvent::Plan { .. }
            | UiEvent::Thinking { .. }
            | UiEvent::Notice { .. } => {}
        }
    }
    Ok(task)
}

pub(super) async fn send_event_or_shutdown(
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
    event: FileBackedEvent,
) -> bool {
    tokio::select! {
        result = tx.send(event) => result.is_ok(),
        changed = shutdown_rx.changed() => {
            let _ = changed;
            false
        }
    }
}

pub(super) async fn spawn_output_tail(
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
                        if !send_event_or_shutdown(
                            &tx,
                            &mut shutdown_rx,
                            FileBackedEvent::Output(text),
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = send_event_or_shutdown(
                            &tx,
                            &mut shutdown_rx,
                            FileBackedEvent::Error(format!("output tail failed: {err:?}")),
                        )
                        .await;
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

pub(super) async fn spawn_request_watch(
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
                        if !send_event_or_shutdown(
                            &tx,
                            &mut shutdown_rx,
                            FileBackedEvent::RequestsChanged,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = send_event_or_shutdown(
                            &tx,
                            &mut shutdown_rx,
                            FileBackedEvent::Error(format!("request watch failed: {err:?}")),
                        )
                        .await;
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

pub(super) async fn spawn_action_watch(
    mut tail: alan_shell::Tail,
    agent_path: String,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let mut pending = Vec::new();
    'watch: loop {
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
                        pending.extend(bytes);
                        for action_id in action_ids_from_events(&mut pending) {
                            if !send_event_or_shutdown(
                                &tx,
                                &mut shutdown_rx,
                                FileBackedEvent::ActionsChanged {
                                    agent_path: agent_path.clone(),
                                    action_id,
                                },
                            )
                            .await
                            {
                                break 'watch;
                            }
                        }
                    }
                    Err(err) => {
                        let _ = send_event_or_shutdown(
                            &tx,
                            &mut shutdown_rx,
                            FileBackedEvent::Error(format!("action watch failed: {err:?}")),
                        )
                        .await;
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

pub(super) async fn spawn_ui_watch(
    mut tail: alan_shell::Tail,
    owner: String,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let mut pending = Vec::new();
    'watch: loop {
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
                                    if matches!(event, UiEvent::InputCompleted { .. }) && !send_event_or_shutdown(&tx, &mut shutdown_rx, FileBackedEvent::ModelReceipt { owner: owner.clone(), event: event.clone() }).await { break 'watch; }
                                    if !send_event_or_shutdown(
                                        &tx,
                                        &mut shutdown_rx,
                                        FileBackedEvent::Ui(event),
                                    )
                                    .await
                                    {
                                        pending.clear();
                                        break 'watch;
                                    }
                                }
                                Err(err) => {
                                    if !send_event_or_shutdown(&tx, &mut shutdown_rx, FileBackedEvent::ModelReceiptsUnavailable { owner: owner.clone() }).await { break 'watch; }
                                    if !send_event_or_shutdown(
                                        &tx,
                                        &mut shutdown_rx,
                                        FileBackedEvent::Error(format!("ui watch parse failed: {err}")),
                                    )
                                    .await
                                    {
                                        break 'watch;
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let _ = send_event_or_shutdown(
                            &tx,
                            &mut shutdown_rx,
                            FileBackedEvent::Error(format!("ui watch failed: {err:?}")),
                        )
                        .await;
                        break;
                    }
                }
            }
        }
    }

    if !*shutdown_rx.borrow() {
        let _ = send_event_or_shutdown(
            &tx,
            &mut shutdown_rx,
            FileBackedEvent::ModelReceiptsUnavailable { owner },
        )
        .await;
    }
    tail.close()
        .await
        .map_err(|err| anyhow!("failed to close ui watch: {err:?}"))?;
    Ok(())
}

pub(super) async fn spawn_tape_watch(
    mut tail: alan_shell::Tail,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let mut pending = Vec::new();
    'watch: loop {
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
                            // Non-message records (tool calls, checkpoints…)
                            // are not rendered; skip quietly like hydration.
                            let Ok(mut record) = serde_json::from_slice::<TapeRecordV1>(line) else {
                                continue;
                            };
                            record.end_offset = tail.offset() as usize - pending.len();
                            if !send_event_or_shutdown(
                                &tx,
                                &mut shutdown_rx,
                                FileBackedEvent::Tape(record),
                            )
                            .await
                            {
                                pending.clear();
                                break 'watch;
                            }
                        }
                    }
                    Err(err) => {
                        let _ = send_event_or_shutdown(
                            &tx,
                            &mut shutdown_rx,
                            FileBackedEvent::Error(format!("tape watch failed: {err:?}")),
                        )
                        .await;
                        break;
                    }
                }
            }
        }
    }

    tail.close()
        .await
        .map_err(|err| anyhow!("failed to close tape watch: {err:?}"))?;
    Ok(())
}

async fn tail_from_live_edge(shell: &alan_shell::Shell, path: &str) -> Result<alan_shell::Tail> {
    Ok(tail_with_history(shell, path).await?.0)
}

#[cfg(test)]
pub(super) fn spawn_terminal_events(
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
) -> tokio::task::JoinHandle<()> {
    spawn_terminal_events_with_stop(
        tx,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}

pub(super) fn spawn_terminal_events_with_stop(
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        loop {
            if tx.is_closed() || stop.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            match crossterm::event::poll(std::time::Duration::from_millis(100)) {
                Ok(true) => match crossterm::event::read() {
                    Ok(event) => {
                        if tx.blocking_send(FileBackedEvent::Terminal(event)).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.blocking_send(FileBackedEvent::TerminalError(format!(
                            "terminal input failed: {err}"
                        )));
                        break;
                    }
                },
                Ok(false) => {}
                Err(err) => {
                    let _ = tx.blocking_send(FileBackedEvent::TerminalError(format!(
                        "terminal polling failed: {err}"
                    )));
                    break;
                }
            }
        }
    })
}

fn agent_input_path(agent_path: &str) -> String {
    format!("{agent_path}/io/input")
}

pub(super) fn agent_output_path(agent_path: &str) -> String {
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

pub(super) async fn read_activity_snapshot(
    shell: &alan_shell::Shell,
    agent_path: &str,
) -> Result<UiActivitySnapshot> {
    read_json_file(shell, &ui_activity_path(agent_path)).await
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

pub(super) async fn write_agent_input(
    shell: &alan_shell::Shell,
    agent_path: &str,
    expected_root_pid: Option<u64>,
    record: &alan_agent_protocol::UserInputRecord,
) -> Result<()> {
    let pinned_path;
    let agent_path = if agent_path == "/agent/root" {
        let pid = expected_root_pid.context("Root Agent is not attached; retry")?;
        anyhow::ensure!(
            super::tail::current_root_agent_pid(shell).await? == Some(pid),
            "Root Agent changed before the task could be submitted; retry"
        );
        pinned_path = format!("/agent/{pid}");
        &pinned_path
    } else {
        agent_path
    };
    shell
        .write(&agent_input_path(agent_path), &record.encode_payload()?)
        .await
        .map_err(|err| anyhow!("write agent input failed: {err:?}"))
}

pub(super) async fn write_request_response(
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

pub(super) async fn write_machine_ctl(
    shell: &alan_shell::Shell,
    agent_path: &str,
    command: &str,
) -> Result<()> {
    shell
        .write(&machine_ctl_path(agent_path), command.as_bytes())
        .await
        .map_err(|err| anyhow!("write machine ctl failed: {err:?}"))
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

pub(super) async fn read_action_ids(
    shell: &alan_shell::Shell,
    agent_path: &str,
) -> Result<Vec<String>> {
    let mut ids = shell
        .ls(&format!("{agent_path}/actions"))
        .await
        .map_err(|err| anyhow!("list actions failed: {err:?}"))?
        .into_iter()
        .filter(|entry| entry != "clone" && entry != "events" && entry != "help")
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| request_sort_key(id));

    Ok(ids)
}

async fn read_action_snapshots(
    shell: &alan_shell::Shell,
    agent_path: &str,
) -> Result<Vec<ActionSnapshot>> {
    let ids = read_action_ids(shell, agent_path).await?;
    let mut snapshots = Vec::with_capacity(ids.len());
    for id in ids {
        snapshots.push(read_action_snapshot(shell, agent_path, &id).await?);
    }
    Ok(snapshots)
}

pub(super) async fn read_action_snapshot(
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

pub(super) fn request_sort_key(request_id: &str) -> u64 {
    request_id
        .trim_start_matches(|ch: char| !ch.is_ascii_digit())
        .parse::<u64>()
        .unwrap_or(0)
}

pub(super) fn parse_tape_history(raw: &str) -> Vec<HistoryCell> {
    let mut cells = Vec::new();
    for line in raw.lines() {
        let Ok(record) = serde_json::from_str::<TapeRecordV1>(line) else {
            continue;
        };
        if record.kind != "message" {
            continue;
        }
        match record.role.as_str() {
            "user" => cells.push(record.into_user_cell()),
            "assistant" => match cells.last_mut() {
                Some(HistoryCell::Assistant(text)) => text.push_str(&record.content),
                _ => cells.push(HistoryCell::Assistant(record.content)),
            },
            _ => {}
        }
    }
    cells
}

pub(super) fn request_snapshot_to_pending_yield(
    snapshot: RequestSnapshot,
) -> Result<PendingYieldCell> {
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
        YieldKind::StructuredInput | YieldKind::Custom(_) => Some(snapshot.prompt.clone()),
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
        "confirmation" | "task_budget_extension" => YieldKind::Confirmation,
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

pub(super) fn response_text_from_content(content: Vec<ContentPart>) -> String {
    content
        .iter()
        .map(ContentPart::to_text_lossy)
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn hydrate_actions_from_snapshots(
    app: &mut FileBackedApp,
    snapshots: Vec<ActionSnapshot>,
) {
    for snapshot in &snapshots {
        app.observe_action_cwd(snapshot);
    }
    app.running_tools = snapshots.iter().filter_map(running_tool).collect();
}

pub(super) fn sync_action_snapshot(app: &mut FileBackedApp, snapshot: ActionSnapshot) {
    app.observe_action_cwd(&snapshot);
    app.running_tools.retain(|tool| tool.id != snapshot.id);
    if let Some(tool) = running_tool(&snapshot) {
        app.running_tools.push(tool);
    }
    if let Some(mut cell) = action_snapshot_to_history_cell(&snapshot) {
        use std::hash::{Hash, Hasher};
        let owner = app.action_owner().to_owned();
        let key = (owner.clone(), snapshot.id.clone());
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (
            &snapshot.name,
            &snapshot.status,
            &snapshot.output,
            &snapshot.result,
        )
            .hash(&mut hash);
        let fingerprint = hash.finish();
        let prior = app.projected_actions.get(&key).copied();
        let committed = prior.is_some_and(|projection| projection.1);
        let unchanged = prior.is_some_and(|projection| projection.0 == fingerprint);
        app.projected_actions.insert(key, (fingerprint, committed));
        if committed && unchanged {
            return;
        }
        if unchanged && !app.action_cells.contains_key(&snapshot.id) {
            return;
        }
        if let HistoryCell::Tool {
            action: Some(action),
            ..
        } = &mut cell
        {
            action.owner = owner.clone();
            action.read_only = action
                .read_only
                .take()
                .filter(|context| !committed && context.owner == owner);
        }
        if committed && let HistoryCell::Tool { title, .. } = &mut cell {
            title.push_str(" · updated");
        }
        app.upsert_action_cell(snapshot.id, cell);
    }
}

const LIVE_TOOL_TAIL_LINES: usize = 4;

fn running_tool(snapshot: &ActionSnapshot) -> Option<RunningTool> {
    if !action_status_is_running(&snapshot.status) {
        return None;
    }
    let metadata = serde_json::from_str::<Value>(&snapshot.result).ok();
    let title = format_running_subagent_title(snapshot, metadata.as_ref()).unwrap_or_else(|| {
        metadata
            .as_ref()
            .and_then(|v| v.get("title"))
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| action_title(snapshot))
    });
    let tail_lines = extract_running_tool_tail(metadata.as_ref(), &snapshot.output);
    Some(RunningTool {
        id: snapshot.id.clone(),
        title,
        tail_lines,
    })
}

fn format_running_subagent_title(
    snapshot: &ActionSnapshot,
    metadata: Option<&Value>,
) -> Option<String> {
    let role = metadata
        .and_then(|v| v.get("subagent_role"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| snapshot.name.trim().strip_prefix("subagent:"))?;
    let role = crate::history::clean_text(role);
    let pid = metadata
        .and_then(|v| v.get("child_pid"))
        .and_then(Value::as_u64);
    let state = metadata
        .and_then(|v| v.get("child_state"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("running"));
    Some(match (pid, state) {
        (Some(pid), Some(state)) if pid > 0 => format!("{role} · pid {pid} · {state}"),
        (Some(pid), None) if pid > 0 => format!("{role} · pid {pid}"),
        (_, Some(state)) => format!("{role} · {state}"),
        _ => role,
    })
}

fn extract_running_tool_tail(metadata: Option<&Value>, output: &str) -> Vec<String> {
    let raw = metadata
        .and_then(|v| v.get("presentation"))
        .and_then(|v| serde_json::from_value::<ToolResultPresentation>(v.clone()).ok())
        .and_then(|pres| match pres {
            ToolResultPresentation::Command { stdout, stderr, .. } => {
                let mut combined = String::new();
                if !stdout.is_empty() {
                    combined.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !combined.is_empty() && !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                    combined.push_str(&stderr);
                }
                (!combined.is_empty()).then_some(combined)
            }
            ToolResultPresentation::PlainText { body } if !body.is_empty() => Some(body),
            _ => None,
        })
        .or_else(|| {
            metadata
                .and_then(|v| v.get("result_preview"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        })
        .or_else(|| (!output.trim().is_empty()).then(|| output.to_string()));

    let Some(raw) = raw else {
        return Vec::new();
    };
    let non_empty: Vec<String> = raw
        .lines()
        .map(crate::history::clean_text)
        .filter(|line| !line.trim().is_empty())
        .collect();
    let start = non_empty.len().saturating_sub(LIVE_TOOL_TAIL_LINES);
    non_empty[start..].to_vec()
}

pub(super) fn action_ids_from_events(pending: &mut Vec<u8>) -> Vec<String> {
    let mut action_ids = Vec::new();
    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
        let record = pending.drain(..end).collect::<Vec<_>>();
        pending.drain(..1);
        let record = record.strip_suffix(b"\r").unwrap_or(&record);
        let Some(separator) = record.iter().position(|byte| *byte == b':') else {
            continue;
        };
        let action_id = if &record[..separator] == b"created" {
            &record[separator + 1..]
        } else {
            let event_kind = &record[separator + 1..];
            if event_kind != b"status" && event_kind != b"result" {
                continue;
            }
            &record[..separator]
        };
        if action_id.is_empty()
            || action_id == b"."
            || action_id == b".."
            || !action_id
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'.'))
        {
            continue;
        }
        if let Ok(action_id) = std::str::from_utf8(action_id) {
            action_ids.push(action_id.to_string());
        }
    }
    action_ids
}

fn action_status_is_running(status: &str) -> bool {
    matches!(status.trim(), "running" | "pending")
}
