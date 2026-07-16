//! AgentFS file observation, command writes, and snapshot projection.

use alan_agent_protocol::{
    ContentPart, StructuredInputQuestion, ToolResultPresentation, UiEvent, YieldKind,
};
use anyhow::{Context, Result, anyhow, bail};
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::history::{HistoryCell, PendingYieldCell, RunningTool, ToolStatus};

use super::app::{FileBackedApp, FileBackedEvent};

/// Hydrate startup state and open the live watch tails so that attach time
/// neither loses nor replays records, per channel:
///
/// - `machine/tape` and `machine/ui/events` hydrate FROM the byte snapshot
///   their own tail pinned at open (`tail_with_history`): the same file is
///   both history source and live stream, so delivery is exactly-once by
///   construction — no ordering race can exist between "what was hydrated"
///   and "what the tail will deliver".
/// - The request/action event tails only trigger idempotent directory
///   re-syncs, so overlap between their open point and the first sync is
///   harmless.
/// - `io/output` is an optimistic live preview: it is never hydrated, and the
///   tape watcher is the authority that reconciles it (`apply_tape_record`
///   dedupes fully-streamed responses, repairs a mid-turn attach that only
///   caught the suffix, and appends responses the stream missed entirely).
/// - UI snapshot files are read only when the ui event history is empty
///   (fresh log); otherwise replaying the pinned history is strictly more
///   consistent than mixing it with later point-in-time snapshot reads.
pub(super) async fn hydrate_and_open_tails(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<WatchTails> {
    let requests = tail_from_live_edge(shell, &request_events_path(agent_path)).await?;
    let actions = tail_from_live_edge(shell, &action_events_path(agent_path)).await?;
    let (ui, ui_history) = tail_with_history(shell, &ui_events_path(agent_path)).await?;
    let (tape, tape_history) =
        tail_with_history(shell, &format!("{agent_path}/machine/tape")).await?;
    let output = tail_from_live_edge(shell, &agent_output_path(agent_path)).await?;

    let tape_history = String::from_utf8(tape_history).context("machine/tape is not utf8")?;
    app.transcript = parse_tape_history(&tape_history);
    app.seed_reconciler_from_tape_history(&tape_history);

    let ui_history = String::from_utf8(ui_history).context("ui events are not utf8")?;
    let ui_events = ui_history
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<UiEvent>(line).context("parse ui event"))
        .collect::<Result<Vec<_>>>()?;
    if ui_events.is_empty() {
        app.apply_ui_activity_snapshot(read_json_file(shell, &ui_activity_path(agent_path)).await?);
        app.apply_ui_plan_snapshot(read_json_file(shell, &ui_plan_path(agent_path)).await?);
        app.apply_ui_thinking_snapshot(read_json_file(shell, &ui_thinking_path(agent_path)).await?);
        app.apply_ui_notice_snapshot(read_json_file(shell, &ui_notice_path(agent_path)).await?);
    } else {
        for event in ui_events {
            app.apply_ui_event(event);
        }
    }

    sync_actions_from_files(shell, agent_path, app).await?;
    sync_requests_from_files(shell, agent_path, app).await?;
    Ok(WatchTails {
        output,
        requests,
        actions,
        ui,
        tape,
    })
}

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

pub(super) async fn sync_actions_from_files(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<()> {
    let snapshots = read_action_snapshots(shell, agent_path).await?;
    sync_actions_from_snapshots(app, snapshots);
    Ok(())
}

/// The live watch tails, opened during `hydrate_and_open_tails` so that
/// attach time neither loses nor replays records (see that function's doc).
pub(super) struct WatchTails {
    pub(super) output: alan_shell::Tail,
    pub(super) requests: alan_shell::Tail,
    pub(super) actions: alan_shell::Tail,
    pub(super) ui: alan_shell::Tail,
    pub(super) tape: alan_shell::Tail,
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

pub(super) async fn spawn_action_watch(
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

pub(super) async fn spawn_ui_watch(
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

pub(super) async fn spawn_tape_watch(
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
                            // Non-message records (tool calls, checkpoints…)
                            // are not rendered; skip quietly like hydration.
                            let Ok(record) = serde_json::from_slice::<TapeRecordV1>(line) else {
                                continue;
                            };
                            if tx.send(FileBackedEvent::Tape(record)).await.is_err() {
                                pending.clear();
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(FileBackedEvent::Error(format!(
                            "tape watch failed: {err:?}"
                        ))).await;
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

/// Open a tail pinned at the file's current live edge and return the bytes
/// that existed at open time. Hydrating from these returned bytes — instead
/// of from a separate read of the same file — makes history + live delivery
/// exactly-once by construction.
async fn tail_with_history(
    shell: &alan_shell::Shell,
    path: &str,
) -> Result<(alan_shell::Tail, Vec<u8>)> {
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
    Ok((tail, existing))
}

pub(super) fn spawn_terminal_events(tx: tokio::sync::mpsc::Sender<FileBackedEvent>) {
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
    text: &str,
) -> Result<()> {
    shell
        .write(&agent_input_path(agent_path), text.as_bytes())
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

pub(super) async fn write_interrupt(shell: &alan_shell::Shell, agent_path: &str) -> Result<()> {
    // Turn interrupt is agent-runtime control: it must cancel the running
    // generation and leave the agent process alive. Writing "interrupt" to
    // the kernel `/proc/<pid>/ctl` would terminate the process instead.
    shell
        .write(&machine_ctl_path(agent_path), b"interrupt")
        .await
        .map_err(|err| anyhow!("write turn interrupt failed: {err:?}"))
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
        .filter(|entry| entry != "clone" && entry != "events" && entry != "help")
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

fn request_sort_key(request_id: &str) -> u64 {
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

pub(super) fn response_text_from_content(content: Vec<ContentPart>) -> String {
    content
        .iter()
        .map(ContentPart::to_text_lossy)
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn sync_actions_from_snapshots(app: &mut FileBackedApp, snapshots: Vec<ActionSnapshot>) {
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

#[derive(Debug, Clone)]
pub(super) struct RequestSnapshot {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) prompt: String,
    pub(super) options: String,
    pub(super) status: String,
}

#[derive(Debug, Clone)]
pub(super) struct ActionSnapshot {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) status: String,
    pub(super) output: String,
    pub(super) result: String,
}

#[derive(Deserialize)]
pub(super) struct TapeRecordV1 {
    #[allow(
        dead_code,
        reason = "version is part of the persisted tape schema even though deserialization validates it elsewhere"
    )]
    pub(super) version: u16,
    pub(super) kind: String,
    pub(super) role: String,
    pub(super) content: String,
}
