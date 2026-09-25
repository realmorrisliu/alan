use super::super::history_merge::remap_transcript_indices;
use super::super::tail::{current_root_agent_pid, root_agent_path_for_pid, tail_with_history};
use super::{
    ActionSnapshot, FileBackedApp, TapeRecordV1, WatchTails, action_events_path, agent_output_path,
    command_action_history_cell, correlated_ui_task, hydrate_actions_from_snapshots,
    read_action_snapshots, read_activity_snapshot, read_json_file, request_events_path,
    sync_requests_from_files, tail_from_live_edge, ui_events_path, ui_notice_path, ui_plan_path,
    ui_thinking_path,
};
use crate::history::HistoryCell;
use alan_agent_protocol::{UiActivityState, UiEvent};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

pub(in crate::file_backed) fn hydrate_tape_history(
    app: &mut FileBackedApp,
    raw: &str,
    actions: &[ActionSnapshot],
) {
    app.tape_user_cells.clear();
    app.pending_command_actions.clear();
    let mut actions_by_submission = std::collections::HashMap::new();
    for action in actions.iter().rev() {
        if !matches!(action.name.as_str(), "bash" | "cd") {
            continue;
        }
        let Ok(result) = serde_json::from_str::<Value>(&action.result) else {
            continue;
        };
        if let Some(submission_id) = result.get("call_id").and_then(Value::as_str) {
            actions_by_submission
                .entry(submission_id.to_string())
                .or_insert(action);
        }
    }

    let mut cells = Vec::new();
    let mut action_cells = std::collections::BTreeMap::new();
    let mut command_submission_ids = Vec::new();
    for line in raw.lines() {
        let Ok(record) = serde_json::from_str::<TapeRecordV1>(line) else {
            continue;
        };
        if record.kind != "message" {
            continue;
        }
        match record.role.as_str() {
            "user" => {
                if let Some(submission_id) = record.submission_id {
                    if let Some(action) = actions_by_submission.get(&submission_id).copied() {
                        command_submission_ids.push(submission_id);
                        cells.push(HistoryCell::Command(record.content));
                        if matches!(action.status.trim(), "completed" | "failed") {
                            let cell = command_action_history_cell(action).unwrap_or_else(|| {
                                HistoryCell::Error("command result is unavailable".to_string())
                            });
                            action_cells.insert(action.id.clone(), cells.len());
                            cells.push(cell);
                        }
                    } else {
                        app.tape_user_cells.insert(submission_id, cells.len());
                        cells.push(HistoryCell::User(record.content));
                    }
                } else {
                    cells.push(HistoryCell::User(record.content));
                }
            }
            "assistant" => match cells.last_mut() {
                Some(HistoryCell::Assistant(text)) => text.push_str(&record.content),
                _ => cells.push(HistoryCell::Assistant(record.content)),
            },
            _ => {}
        }
    }
    app.transcript = cells;
    app.action_cells = action_cells;
    for submission_id in command_submission_ids {
        app.mark_command_submission(submission_id);
    }
}

/// Pin every renderer stream and snapshot read to one Root Agent Process.
/// If the supervisor replaces it during hydration, discard the whole set and
/// retry rather than combining observations from different Processes.
pub(in crate::file_backed) async fn hydrate_and_open_tails(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<WatchTails> {
    let follows_root_agent = agent_path == "/agent/root";
    let attempts = if follows_root_agent { 3 } else { 1 };
    for attempt in 0..attempts {
        let root_agent_pid = if follows_root_agent {
            match current_root_agent_pid(shell).await? {
                Some(pid) => Some(pid),
                None if attempt + 1 < attempts => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    continue;
                }
                None => return Err(anyhow!("Root Agent PID is unavailable while attaching")),
            }
        } else {
            None
        };
        let pinned_agent_path = root_agent_pid
            .and_then(|pid| root_agent_path_for_pid(agent_path, pid))
            .unwrap_or_else(|| agent_path.to_string());
        let mut hydrated = app.clone();
        match hydrate_pinned_agent(shell, &pinned_agent_path, &mut hydrated).await {
            Ok(mut tails) => {
                let pid_changed = match root_agent_pid {
                    Some(pid) => match current_root_agent_pid(shell).await {
                        Ok(current) => current != Some(pid),
                        Err(error) => {
                            tails.close().await;
                            return Err(error);
                        }
                    },
                    None => false,
                };
                if pid_changed {
                    tails.close().await;
                    continue;
                }
                tails.root_agent_pid = root_agent_pid;
                *app = hydrated;
                return Ok(tails);
            }
            Err(error) => {
                if let Some(pid) = root_agent_pid
                    && current_root_agent_pid(shell).await? != Some(pid)
                {
                    continue;
                }
                // ponytail: two 250ms retries cover the detach-before-PID-clear window; persistent
                // hydration errors stay visible instead of making terminal startup wait forever.
                if follows_root_agent && attempt + 1 < attempts {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    continue;
                }
                return Err(error);
            }
        }
    }
    bail!("Root Agent kept changing while opening renderer streams; retry attach")
}

async fn hydrate_pinned_agent(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<WatchTails> {
    let mut opened = Vec::with_capacity(5);
    let histories = async {
        opened.push(tail_from_live_edge(shell, &request_events_path(agent_path)).await?);
        opened.push(tail_from_live_edge(shell, &action_events_path(agent_path)).await?);
        let (ui, ui_history) = tail_with_history(shell, &ui_events_path(agent_path)).await?;
        opened.push(ui);
        let (tape, tape_history) =
            tail_with_history(shell, &format!("{agent_path}/machine/tape")).await?;
        opened.push(tape);
        opened.push(tail_from_live_edge(shell, &agent_output_path(agent_path)).await?);
        Ok::<_, anyhow::Error>((ui_history, tape_history))
    }
    .await;
    let (ui_history, tape_history) = match histories {
        Ok(histories) => histories,
        Err(error) => {
            close_tails(opened).await;
            return Err(error);
        }
    };
    let mut opened = opened.into_iter();
    let requests = opened.next().expect("request tail was opened");
    let actions = opened.next().expect("action tail was opened");
    let ui = opened.next().expect("UI tail was opened");
    let tape = opened.next().expect("tape tail was opened");
    let output = opened.next().expect("output tail was opened");
    let mut tails = WatchTails {
        root_agent_pid: None,
        output,
        requests,
        actions,
        ui,
        tape,
        ui_history: Vec::new(),
    };

    let hydrate = async {
        let tape_history = String::from_utf8(tape_history).context("machine/tape is not utf8")?;
        let action_snapshots = read_action_snapshots(shell, agent_path).await?;
        hydrate_tape_history(app, &tape_history, &action_snapshots);
        app.seed_reconciler_from_tape_history(&tape_history);

        let ui_history_text = std::str::from_utf8(&ui_history).context("ui events are not utf8")?;
        let ui_events = ui_history_text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<UiEvent>(line).context("parse ui event"))
            .collect::<Result<Vec<_>>>()?;
        if ui_events.is_empty() {
            app.apply_ui_activity_snapshot(read_activity_snapshot(shell, agent_path).await?);
            app.apply_ui_plan_snapshot(read_json_file(shell, &ui_plan_path(agent_path)).await?);
            app.apply_ui_thinking_snapshot(
                read_json_file(shell, &ui_thinking_path(agent_path)).await?,
            );
            app.apply_ui_notice_snapshot(read_json_file(shell, &ui_notice_path(agent_path)).await?);
        } else {
            // ponytail: UI and tape lack shared event IDs, so replay only the latest-turn error;
            // add cross-log correlation metadata if exact older-error placement becomes required.
            let latest_running = ui_events.iter().rposition(|event| {
                matches!(
                    event,
                    UiEvent::Activity { snapshot }
                        if snapshot.state == UiActivityState::Running
                )
            });
            for (index, event) in ui_events.into_iter().enumerate() {
                if latest_running.is_some_and(|running| {
                    index < running && matches!(event, UiEvent::Error { .. })
                }) {
                    continue;
                }
                app.apply_ui_event(event);
            }
        }

        hydrate_actions_from_snapshots(app, action_snapshots);
        sync_requests_from_files(shell, agent_path, app).await
    }
    .await;
    if let Err(error) = hydrate {
        tails.close().await;
        return Err(error);
    }
    tails.ui_history = ui_history;
    Ok(tails)
}

pub(in crate::file_backed) async fn reattach_to_current_agent(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
    submitted_task: Option<(&str, u64, usize)>,
) -> Result<(WatchTails, bool)> {
    let mut reattached = app.clone();
    let previous_transcript = std::mem::take(&mut reattached.transcript);
    reattached.reset_for_root_process_change();
    let tails = hydrate_and_open_tails(shell, agent_path, &mut reattached).await?;
    let current_transcript = std::mem::take(&mut reattached.transcript);
    reattached.transcript = previous_transcript;
    let mut submitted_task_settled = false;
    if let Some((submitted_input, submitted_at_ms, prior_matching_turns)) = submitted_task {
        let ui_task = correlated_ui_task(&tails.ui_history, submitted_at_ms)?;
        if reattached.notice.as_ref().is_some_and(|notice| {
            current_transcript
                .iter()
                .any(|cell| matches!(cell, HistoryCell::Error(message) if message == notice))
                && ui_task.error.as_ref() != Some(notice)
        }) {
            reattached.notice = None;
        }
        let current_transcript =
            remove_error_cells_and_remap_indices(current_transcript, &mut reattached);
        let recovered_current_turn = ui_task.started
            && reattached.merge_reconnected_history(
                current_transcript,
                submitted_input,
                prior_matching_turns,
            );
        if !recovered_current_turn {
            reattached.reconciler.on_local_submit(submitted_input);
        }
        if let Some(message) = ui_task.started.then_some(ui_task.error).flatten() {
            reattached.notice = Some(message.clone());
            reattached.transcript.push(HistoryCell::Error(message));
            submitted_task_settled = ui_task.state == Some(UiActivityState::Idle);
        } else if !recovered_current_turn && reattached.activity.state == UiActivityState::Idle {
            let message =
                "Root Agent changed before the submitted turn could be recovered; outcome is unknown"
                    .to_string();
            reattached.notice = Some(message.clone());
            reattached.transcript.push(HistoryCell::Error(message));
            submitted_task_settled = true;
        } else if recovered_current_turn {
            submitted_task_settled = ui_task.state == Some(UiActivityState::Idle);
        }
    } else {
        reattached.merge_reconnected_idle_history(current_transcript);
    }
    *app = reattached;
    Ok((tails, submitted_task_settled))
}

fn remove_error_cells_and_remap_indices(
    current: Vec<HistoryCell>,
    app: &mut FileBackedApp,
) -> Vec<HistoryCell> {
    let mut index_mapping = Vec::with_capacity(current.len());
    let mut next_index = 0;
    let current = current
        .into_iter()
        .filter_map(|cell| {
            if matches!(cell, HistoryCell::Error(_)) {
                index_mapping.push(None);
                None
            } else {
                index_mapping.push(Some(next_index));
                next_index += 1;
                Some(cell)
            }
        })
        .collect();
    remap_transcript_indices(&mut app.action_cells, &index_mapping);
    remap_transcript_indices(&mut app.tape_user_cells, &index_mapping);
    current
}

async fn close_tails(tails: Vec<alan_shell::Tail>) {
    for tail in tails {
        let _ = tail.close().await;
    }
}

impl WatchTails {
    async fn close(self) {
        close_tails(vec![
            self.requests,
            self.actions,
            self.ui,
            self.tape,
            self.output,
        ])
        .await;
    }
}

#[cfg(test)]
#[path = "../file_surface_reconnect_tests.rs"]
mod reconnect_tests;
