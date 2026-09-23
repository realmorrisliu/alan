use std::mem;

use crate::history::{HistoryCell, RenderOpts};

use super::app::FileBackedApp;

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
