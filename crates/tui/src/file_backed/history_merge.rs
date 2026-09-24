use std::mem;

use crate::history::{HistoryCell, RenderOpts};

use super::app::FileBackedApp;

pub(super) fn merge_reconnected_history(
    app: &mut FileBackedApp,
    current: Vec<HistoryCell>,
    submitted_input: &str,
    prior_matching_turns: usize,
) -> bool {
    let Some(boundary) = current
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| {
            matches!(cell, HistoryCell::User(text) if text == submitted_input).then_some(index)
        })
        .nth(prior_matching_turns)
    else {
        return false;
    };

    let mut omitted_current_cell = None;
    if let Some(previous_boundary) = app
        .transcript
        .iter()
        .rposition(|cell| matches!(cell, HistoryCell::User(text) if text == submitted_input))
        && let Some((previous_answer_index, previous_answer)) = app
            .transcript
            .iter()
            .enumerate()
            .skip(previous_boundary + 1)
            .rev()
            .find_map(|(index, cell)| match cell {
                HistoryCell::Assistant(text) => Some((index, text.as_str())),
                _ => None,
            })
        && let Some((current_answer_index, current_answer)) = current
            .iter()
            .enumerate()
            .skip(boundary + 1)
            .rev()
            .find_map(|(index, cell)| match cell {
                HistoryCell::Assistant(text) => Some((index, text.as_str())),
                _ => None,
            })
        && !previous_answer.is_empty()
        && !current_answer.is_empty()
        && (current_answer.starts_with(previous_answer)
            || previous_answer.starts_with(current_answer))
    {
        if current_answer.starts_with(previous_answer) {
            app.transcript[previous_answer_index] = current[current_answer_index].clone();
        }
        omitted_current_cell = Some(current_answer_index);
    }

    let previous_len = app.transcript.len();
    app.action_cells = std::mem::take(&mut app.action_cells)
        .into_iter()
        .filter_map(|(action_id, index)| {
            if index <= boundary {
                return None;
            }
            let omitted_before_action =
                usize::from(omitted_current_cell.is_some_and(|omitted| omitted < index));
            Some((
                action_id,
                previous_len + index - boundary - 1 - omitted_before_action,
            ))
        })
        .collect();
    app.transcript
        .extend(current.into_iter().enumerate().filter_map(|(index, cell)| {
            (index > boundary && Some(index) != omitted_current_cell).then_some(cell)
        }));
    true
}

pub(super) fn merge_idle_history(app: &mut FileBackedApp, current: Vec<HistoryCell>) {
    // ponytail: O(current * retained^2), bounded by scrollback; tape IDs would remove text ambiguity.
    let max_overlap_len = app.transcript.len().min(current.len());
    let retained_suffix = (1..=max_overlap_len).rev().find_map(|overlap_len| {
        let suffix = &app.transcript[app.transcript.len() - overlap_len..];
        let allow_partial_front =
            overlap_len == app.transcript.len() && app.scrollback_front_is_partial;
        current
            .windows(overlap_len)
            .position(|window| retained_history_matches(window, suffix, allow_partial_front))
            .map(|offset| (offset, overlap_len))
    });
    if let Some((offset, overlap_len)) = retained_suffix {
        let suffix_start = app.transcript.len() - overlap_len;
        let append_from = offset + overlap_len;
        app.action_cells = mem::take(&mut app.action_cells)
            .into_iter()
            .filter_map(|(action_id, index)| {
                let merged_index = if index >= offset && index < append_from {
                    suffix_start + index - offset
                } else if index >= append_from {
                    app.transcript.len() + index - append_from
                } else {
                    return None;
                };
                Some((action_id, merged_index))
            })
            .collect();
        app.transcript.extend(current.into_iter().skip(append_from));
        return;
    }

    let shared_prefix_len = app
        .transcript
        .iter()
        .zip(&current)
        .take_while(|(previous, replacement)| previous == replacement)
        .count();
    let previous_len = app.transcript.len();
    app.action_cells = mem::take(&mut app.action_cells)
        .into_iter()
        .map(|(action_id, index)| {
            let merged_index = if index < shared_prefix_len {
                index
            } else {
                previous_len + index.saturating_sub(shared_prefix_len)
            };
            (action_id, merged_index)
        })
        .collect();
    app.transcript
        .extend(current.into_iter().skip(shared_prefix_len));
}

fn retained_history_matches(
    replacement: &[HistoryCell],
    retained: &[HistoryCell],
    allow_partial_front: bool,
) -> bool {
    replacement.len() == retained.len()
        && replacement
            .iter()
            .zip(retained)
            .enumerate()
            .all(|(index, (replacement, retained))| {
                replacement == retained
                    || (index == 0
                        && allow_partial_front
                        && rendered_history_suffix_matches(replacement, retained))
            })
}

fn rendered_history_suffix_matches(replacement: &HistoryCell, retained: &HistoryCell) -> bool {
    let replacement = rendered_history_tokens(replacement);
    let retained = rendered_history_tokens(retained);
    !retained.is_empty() && replacement.len() >= retained.len() && replacement.ends_with(&retained)
}

fn rendered_history_tokens(cell: &HistoryCell) -> Vec<String> {
    let mut lines = cell.render_lines(RenderOpts::new(16_384, false));
    if let Some(first) = lines.first_mut() {
        for prefix in [
            "alan > ",
            "you> ",
            "alan> ",
            "tool> ",
            "input> ",
            "error> ",
            "thinking> ",
            "plan> ",
        ] {
            if let Some(body) = first.strip_prefix(prefix) {
                *first = body.to_string();
                break;
            }
        }
    }
    lines
        .iter()
        .flat_map(|line| line.split_whitespace().map(str::to_string))
        .collect()
}
