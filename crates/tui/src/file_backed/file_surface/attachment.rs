use super::super::interrupt::PendingRootAgentTurn;
use super::super::tail::{current_root_agent_pid, root_agent_path_for_pid, tail_with_history};
use super::{
    FileBackedApp, WatchTails, action_events_path, agent_output_path, correlated_ui_task,
    hydrate_actions_from_files, parse_tape_history, read_activity_snapshot, read_json_file,
    request_events_path, sync_requests_from_files, tail_from_live_edge, ui_events_path,
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
    // Failed attachment must never leave an acceptable detached Skill owner.
    app.invalidate_skill_owner();
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
    let mut opened = Vec::with_capacity(7);
    let histories = async {
        opened.push(shell.tail(&format!("{agent_path}/events")).await?);
        opened.push(tail_from_live_edge(shell, &request_events_path(agent_path)).await?);
        opened.push(tail_from_live_edge(shell, &action_events_path(agent_path)).await?);
        let (ui, ui_history) = tail_with_history(shell, &ui_events_path(agent_path)).await?;
        opened.push(ui);
        let (tape, tape_history) =
            tail_with_history(shell, &format!("{agent_path}/machine/tape")).await?;
        opened.push(tape);
        opened.push(tail_from_live_edge(shell, &agent_output_path(agent_path)).await?);
        opened.push(shell.tail(&ui_events_path(agent_path)).await?);
        opened.push(shell.tail(&format!("{agent_path}/machine/tape")).await?);
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
    let queue_events = opened.next().expect("Agent events tail was opened");
    let requests = opened.next().expect("request tail was opened");
    let actions = opened.next().expect("action tail was opened");
    let ui = opened.next().expect("UI tail was opened");
    let tape = opened.next().expect("tape tail was opened");
    let output = opened.next().expect("output tail was opened");
    let mut tails = WatchTails {
        queue_events,
        root_agent_pid: None,
        output,
        requests,
        actions,
        ui,
        tape,
        recovery_ui: opened.next().expect("recovery UI was opened"),
        recovery_tape: opened.next().expect("recovery Tape was opened"),
        ui_history: Vec::new(),
        tape_history: Vec::new(),
    };

    let hydrate = async {
        app.apply_skills(
            agent_path,
            super::super::skills::read_skills(shell, agent_path).await,
        );
        app.model.apply(
            agent_path,
            super::super::model::read_model(shell, agent_path).await,
        );
        app.queue.apply(
            agent_path,
            super::super::queue::read_queue(shell, agent_path).await,
        );
        let tape_history =
            std::str::from_utf8(&tape_history).context("machine/tape is not utf8")?;
        let receipt_history = projected_receipt_history(app, tape_history);
        app.transcript = parse_tape_history(&receipt_history);
        app.tape_consumed_offset = tape_history.len();
        app.seed_reconciler_from_tape_history(&receipt_history);
        app.restore_receipts_after_hydration(&receipt_history);

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

        hydrate_actions_from_files(shell, agent_path, app).await?;
        sync_requests_from_files(shell, agent_path, app).await
    }
    .await;
    if let Err(error) = hydrate {
        tails.close().await;
        return Err(error);
    }
    tails.ui_history = ui_history;
    tails.tape_history = tape_history;
    Ok(tails)
}

pub(in crate::file_backed) async fn reattach_to_current_agent(
    shell: &alan_shell::Shell,
    agent_path: &str,
    app: &mut FileBackedApp,
    submitted_tasks: &[PendingRootAgentTurn],
) -> Result<(WatchTails, Vec<String>)> {
    let mut reattached = app.clone();
    let mut retained_inputs = reattached.local_inputs.clone();
    let previous_inputs = retained_inputs.clone();
    let previous_transcript = std::mem::take(&mut reattached.transcript);
    reattached.reset_for_root_process_change();
    let tails = hydrate_and_open_tails(shell, agent_path, &mut reattached).await?;
    let mut current_transcript = std::mem::take(&mut reattached.transcript);
    let receipt_history =
        projected_receipt_history(&reattached, std::str::from_utf8(&tails.tape_history)?);
    let records = receipt_history
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<super::TapeRecordV1>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let prompt = submitted_tasks.iter().find_map(|task| {
        let ordinal = records
            .iter()
            .filter(|record| {
                record.kind == "message" && record.role == "user" && record.content == task.input
            })
            .position(|record| record.belongs_to(&task.submission_id));
        ordinal.map(|ordinal| (task, ordinal))
    });
    let prompt_cell = prompt.and_then(|(task, ordinal)| {
        current_transcript
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| {
                (cell
                    .input_source()
                    .is_some_and(|(text, _, _)| text == task.input)
                    || reattached.local_inputs.values().any(|input| {
                        input.tape_seen && input.cell == Some(index) && input.body == task.input
                    }))
                .then_some(index)
            })
            .nth(ordinal)
    });
    // Receipt identity is (owner, submission ID), never rendered/body overlap.
    // Keep the retained source cell out of text-only history matching and omit
    // its hydrated counterpart. Hydration still owns Tape/terminal evidence.
    let mut retained_receipts = Vec::new();
    reattached.transcript = previous_transcript;
    let mut indices = previous_inputs
        .iter()
        .filter_map(|(id, previous)| {
            let hydrated = reattached.local_inputs.get(id)?;
            (previous.owner == hydrated.owner)
                .then_some(previous.cell.map(|index| (index, id.clone())))
                .flatten()
        })
        .collect::<Vec<_>>();
    indices.sort_by_key(|(index, _)| *index);
    for (index, id) in indices.into_iter().rev() {
        if index >= reattached.transcript.len() {
            continue;
        }
        let input = reattached
            .local_inputs
            .get_mut(&id)
            .expect("retained receipt");
        // A cloned receipt's owner/tape_seen flag is retained evidence, not
        // authority over this Process's hydrated source. Old-owner turns stay
        // visible and unknown; they are neither transferred nor resubmitted.
        if input.owner != reattached.queue.owner {
            continue;
        }
        let canonical = input.tape_seen
            && input
                .cell
                .is_some_and(|cell| cell < current_transcript.len())
            && records.iter().any(|record| {
                record.kind == "message" && record.role == "user" && record.belongs_to(&id)
            });
        if canonical {
            if let Some(current_index) = input.cell {
                current_transcript[current_index] = reattached.transcript[index].clone();
            }
            // Only an exact submitted prompt transfers the retained turn to
            // hydration. No-prompt recovery keeps the whole visible turn in place.
            if prompt_cell.is_some_and(|boundary| input.cell.is_some_and(|cell| cell >= boundary)) {
                // The following canonical answer has the same hydrated owner.
                // Keeping it in the old prefix would also confuse submitted-A
                // answer reconciliation on the next reattach.
                let end = reattached.transcript[index + 1..]
                    .iter()
                    .position(|cell| cell.input_source().is_some())
                    .map_or(reattached.transcript.len(), |next| index + 1 + next);
                for next in (index + 1..end).rev() {
                    if reattached.transcript[next].assistant_source().is_some() {
                        remove_retained_cell(
                            &mut reattached.transcript,
                            &mut retained_inputs,
                            next,
                        );
                    }
                }
                remove_retained_cell(&mut reattached.transcript, &mut retained_inputs, index);
            }
            continue;
        } else if let Some(current_index) = input.cell.take()
            && current_index < current_transcript.len()
        {
            current_transcript.remove(current_index);
            reattached.pending_remote_turn_start = reattached
                .pending_remote_turn_start
                .map(|boundary| boundary - usize::from(boundary > current_index));
            for other in reattached.local_inputs.values_mut() {
                other.cell = other.cell.and_then(|i| {
                    if i == current_index {
                        None
                    } else {
                        Some(i - usize::from(i > current_index))
                    }
                });
            }
            reattached.action_cells.retain(|_, i| {
                if *i == current_index {
                    return false;
                }
                *i -= usize::from(*i > current_index);
                true
            });
        }
        retained_receipts.push((
            id,
            remove_retained_cell(&mut reattached.transcript, &mut retained_inputs, index),
        ));
    }
    retained_receipts.reverse();
    if !submitted_tasks.is_empty() {
        // This recovery notice is regenerated below from the current evidence.
        // It is not a retained turn and must not split that turn on reattach.
        for index in (0..reattached.transcript.len()).rev() {
            if matches!(&reattached.transcript[index], HistoryCell::Error(message)
                if message == "Root Agent changed without correlated completion evidence; outcome is unknown")
            {
                remove_retained_cell(&mut reattached.transcript, &mut retained_inputs, index);
            }
        }
    }
    let mut settled_ids = Vec::new();
    if let Some(first_task) = submitted_tasks.first() {
        let ui_task = correlated_ui_task(&tails.ui_history, first_task.submitted_at_ms)?;
        let ui_events = std::str::from_utf8(&tails.ui_history)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str::<UiEvent>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let completions = ui_events
            .iter()
            .filter(|event| {
                matches!(event, UiEvent::InputCompleted { submission_ids, .. }
                    if submitted_tasks.iter().any(|task| submission_ids.contains(&task.submission_id)))
            })
            .collect::<Vec<_>>();
        if reattached.notice.as_ref().is_some_and(|notice| {
            current_transcript
                .iter()
                .any(|cell| matches!(cell, HistoryCell::Error(message) if message == notice))
                && ui_task.error.as_ref() != Some(notice)
        }) {
            reattached.notice = None;
        }
        reattached.pending_remote_turn_start =
            reattached.pending_remote_turn_start.map(|boundary| {
                boundary
                    - current_transcript
                        .iter()
                        .take(boundary)
                        .filter(|cell| matches!(cell, HistoryCell::Error(_)))
                        .count()
            });
        let current_transcript = remove_error_cells_and_remap_actions(
            current_transcript,
            &mut reattached.action_cells,
            &mut reattached.local_inputs,
        );
        if let Some((task, ordinal)) = prompt {
            reattached.merge_reconnected_history(current_transcript, &task.input, ordinal);
            // Prefix receipts were not transferred to the appended hydrated
            // suffix. Their retained coordinates still own the visible turn.
            for (id, input) in &mut reattached.local_inputs {
                if input.cell.is_none() {
                    input.cell = retained_inputs.get(id).and_then(|retained| {
                        (retained.owner == input.owner)
                            .then_some(retained.cell)
                            .flatten()
                    });
                }
            }
            // Indices from an omitted hydrated prefix cannot address retained
            // cells. Retained receipts are restored below by exact ID.
        } else {
            // No exact submitted user record: clear/unknown-outcome recovery
            // must not import unrelated canonical turns, even equal-body ones.
            reattached.action_cells.clear();
            reattached.pending_remote_turn_start = None;
            let safe_cells = reattached
                .local_inputs
                .iter()
                .filter_map(|(id, input)| {
                    (input.owner == reattached.queue.owner).then_some(())?;
                    let cell = current_transcript.get(input.cell?)?.clone();
                    Some((id.clone(), cell))
                })
                .collect::<Vec<_>>();
            for (id, input) in &mut reattached.local_inputs {
                input.cell = retained_inputs.get(id).and_then(|retained| {
                    (retained.owner == input.owner)
                        .then_some(retained.cell)
                        .flatten()
                });
            }
            for (id, cell) in safe_cells {
                if reattached.local_inputs[&id].cell.is_some() {
                    continue;
                }
                let index = reattached.transcript.len();
                let input = reattached.local_inputs.get_mut(&id).expect("safe receipt");
                input.cell = Some(index);
                reattached.transcript.insert(index, cell);
            }
        }
        for event in completions {
            if let UiEvent::InputCompleted {
                submission_ids,
                status,
                error: _,
            } = event
            {
                settled_ids.extend(
                    submitted_tasks
                        .iter()
                        .filter(|task| submission_ids.contains(&task.submission_id))
                        .map(|task| task.submission_id.clone()),
                );
                if *status != alan_agent_protocol::UiInputStatus::Completed {
                    let message =
                        super::super::interrupt::render_input_completion(event, &mut reattached)
                            .expect("non-completed status has a completion message");
                    reattached.notice = Some(message);
                }
            }
        }
        let unknown_ids = submitted_tasks
            .iter()
            .filter(|task| !settled_ids.contains(&task.submission_id))
            .map(|task| task.submission_id.clone())
            .collect::<Vec<_>>();
        if reattached.activity.state == UiActivityState::Idle && !unknown_ids.is_empty() {
            settled_ids.extend(unknown_ids);
            let message =
                "Root Agent changed without correlated completion evidence; outcome is unknown"
                    .to_string();
            reattached.notice = Some(message.clone());
            reattached.transcript.push(HistoryCell::Error(message));
        }
    } else {
        reattached.merge_reconnected_idle_history(current_transcript);
    }
    // Hydration coordinates have now been merged/remapped. Receipts from an
    // older Process never participated: restore their retained anchors, adjusted
    // only by actual retained-cell removals.
    for (id, input) in &mut reattached.local_inputs {
        if input.owner != reattached.queue.owner {
            input.cell = retained_inputs.get(id).and_then(|retained| retained.cell);
        }
    }
    for (id, cell) in retained_receipts {
        if let Some(input) = reattached.local_inputs.get_mut(&id)
            && input.cell.is_none()
        {
            input.cell = Some(reattached.transcript.len());
            reattached.transcript.push(cell);
        }
    }
    *app = reattached;
    Ok((tails, settled_ids))
}

// One owner-scoped projection supplies all hydrated source coordinates.
// Durable evidence and tail byte offsets always remain the original raw Tape.
fn projected_receipt_history(app: &FileBackedApp, raw: &str) -> String {
    let mut seen = std::collections::BTreeSet::new();
    raw.lines()
        .filter(|line| {
            let Ok(record) = serde_json::from_str::<super::TapeRecordV1>(line) else {
                return true;
            };
            if record.kind != "message" || record.role != "user" {
                return true;
            }
            let ids = app
                .local_inputs
                .iter()
                .filter_map(|(id, input)| {
                    (input.owner == app.queue.owner && record.belongs_to(id)).then_some(id.clone())
                })
                .collect::<Vec<_>>();
            let duplicate = ids.iter().any(|id| seen.contains(id));
            seen.extend(ids);
            !duplicate
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn remove_retained_cell(
    transcript: &mut Vec<HistoryCell>,
    inputs: &mut std::collections::BTreeMap<String, super::super::queue::LocalInput>,
    index: usize,
) -> HistoryCell {
    for input in inputs.values_mut() {
        input.cell = input
            .cell
            .and_then(|old| (old != index).then(|| old - usize::from(old > index)));
    }
    transcript.remove(index)
}

fn remove_error_cells_and_remap_actions(
    current: Vec<HistoryCell>,
    action_cells: &mut std::collections::BTreeMap<String, usize>,
    local_inputs: &mut std::collections::BTreeMap<String, super::super::queue::LocalInput>,
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
    for input in local_inputs.values_mut() {
        input.cell = input
            .cell
            .and_then(|index| action_indices.get(index).copied().flatten());
    }
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
    pub(in crate::file_backed) async fn close(self) {
        close_tails(vec![
            self.queue_events,
            self.requests,
            self.actions,
            self.ui,
            self.tape,
            self.output,
            self.recovery_ui,
            self.recovery_tape,
        ])
        .await;
    }
}

#[cfg(test)]
#[path = "../file_surface_reconnect_tests.rs"]
mod reconnect_tests;
