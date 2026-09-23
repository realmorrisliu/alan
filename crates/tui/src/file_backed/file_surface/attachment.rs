use super::super::tail::{current_root_agent_pid, root_agent_path_for_pid, tail_with_history};
use super::{
    FileBackedApp, WatchTails, action_events_path, agent_output_path, correlated_ui_task,
    parse_tape_history, read_activity_snapshot, read_json_file, request_events_path,
    sync_actions_from_files, sync_requests_from_files, tail_from_live_edge, ui_events_path,
    ui_notice_path, ui_plan_path, ui_thinking_path,
};
use crate::history::HistoryCell;
use alan_agent_protocol::{UiActivityState, UiEvent};
use anyhow::{Context, Result, anyhow, bail};

/// Pin every renderer stream and snapshot read to one Root Agent Process.
/// If the supervisor replaces it during hydration, discard the whole set and
/// retry rather than combining observations from different Processes.
pub(in crate::file_backed) async fn hydrate_and_open_tails(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
) -> Result<WatchTails> {
    let follows_root_agent = agent_path == "/agent/root";
    for _ in 0..if follows_root_agent { 3 } else { 1 } {
        let root_agent_pid = if follows_root_agent {
            Some(
                current_root_agent_pid(shell)
                    .await?
                    .ok_or_else(|| anyhow!("Root Agent PID is unavailable while attaching"))?,
            )
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
        app.transcript = parse_tape_history(&tape_history);
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
            for event in ui_events {
                app.apply_ui_event(event);
            }
        }

        sync_actions_from_files(shell, agent_path, app).await?;
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
            remove_error_cells_and_remap_actions(current_transcript, &mut reattached.action_cells);
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

fn remove_error_cells_and_remap_actions(
    current: Vec<HistoryCell>,
    action_cells: &mut std::collections::BTreeMap<String, usize>,
) -> Vec<HistoryCell> {
    let mut action_indices = Vec::with_capacity(current.len());
    let mut next_index = 0;
    let current = current
        .into_iter()
        .filter_map(|cell| {
            if matches!(cell, HistoryCell::Error(_)) {
                action_indices.push(None);
                None
            } else {
                action_indices.push(Some(next_index));
                next_index += 1;
                Some(cell)
            }
        })
        .collect();
    *action_cells = std::mem::take(action_cells)
        .into_iter()
        .filter_map(|(id, index)| {
            action_indices
                .get(index)
                .copied()
                .flatten()
                .map(|index| (id, index))
        })
        .collect();
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
