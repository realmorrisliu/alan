use alan_agent_protocol::{UiActivityState, UiEvent};
use std::collections::VecDeque;

use super::app::FileBackedApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingRootAgentTurn {
    pub(super) input: String,
    pub(super) submission_id: String,
    pub(super) submitted_process: Option<u64>,
    pub(super) submitted_at_ms: u64,
}

pub(super) fn observe_root_agent_completion(
    pending_turns: &mut VecDeque<PendingRootAgentTurn>,
    event: &UiEvent,
    app: &mut FileBackedApp,
) {
    if let UiEvent::InputCompleted { submission_ids, .. } = event {
        let matched = pending_turns
            .iter()
            .any(|turn| submission_ids.contains(&turn.submission_id));
        if !matched {
            return;
        }
        render_input_completion(event, app);
        if let UiEvent::InputCompleted {
            status: alan_agent_protocol::UiInputStatus::Failed,
            error: Some(error),
            ..
        } = event
        {
            // Process emits this terminal error immediately after failed settlement.
            app.expected_terminal_error = Some(format!("Error handling submission: {error}"));
        }
        pending_turns.retain(|turn| !submission_ids.contains(&turn.submission_id));
    }
}

pub(super) fn render_input_completion(event: &UiEvent, app: &mut FileBackedApp) -> Option<String> {
    if let UiEvent::InputCompleted { status, error, .. } = event {
        match status {
            alan_agent_protocol::UiInputStatus::Completed => None,
            alan_agent_protocol::UiInputStatus::Cancelled => {
                let message = match error {
                    Some(reason) if !reason.trim().is_empty() => {
                        format!("Input cancelled: {reason}")
                    }
                    _ => "Input cancelled".to_string(),
                };
                app.transcript
                    .push(crate::history::HistoryCell::Rendered(vec![message.clone()]));
                Some(message)
            }
            alan_agent_protocol::UiInputStatus::Failed => {
                let message = error
                    .clone()
                    .unwrap_or_else(|| "Input ended: Failed".into());
                app.push_error(message.clone());
                Some(message)
            }
        }
    } else {
        None
    }
}

pub(super) fn settle_unknown_replaced_input(
    pending_turns: &mut VecDeque<PendingRootAgentTurn>,
    current_process: Option<u64>,
    app: &mut FileBackedApp,
) {
    if app.activity.state == UiActivityState::Idle {
        let before = pending_turns.len();
        pending_turns.retain(|turn| {
            !matches!((turn.submitted_process, current_process),
                (Some(submitted), Some(current)) if submitted != current)
        });
        if pending_turns.len() == before {
            return;
        }
        app.push_error(
            "Root Agent changed without correlated completion evidence; outcome is unknown".into(),
        );
    }
}

pub(super) fn interrupt_control(
    agent_path: &str,
    pending_turns: &VecDeque<PendingRootAgentTurn>,
    root_pid: Option<u64>,
) -> Result<(String, String), String> {
    let pending = pending_turns.front();
    let target = if agent_path == "/agent/root" {
        let Some(pid) = pending.map_or(root_pid, |turn| turn.submitted_process) else {
            return Err("Root Agent is not attached; retry interrupt".into());
        };
        format!("/agent/{pid}")
    } else {
        agent_path.to_string()
    };
    let command = pending.map_or_else(
        || "interrupt".to_string(),
        |turn| format!("queue-v1 interrupt {}", turn.submission_id),
    );
    Ok((target, command))
}

#[cfg(test)]
#[path = "interrupt_tests.rs"]
mod tests;
