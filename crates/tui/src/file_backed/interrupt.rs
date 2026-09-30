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

pub(super) fn render_input_completion(event: &UiEvent, app: &mut FileBackedApp) {
    if let UiEvent::InputCompleted { status, error, .. } = event
        && *status != alan_agent_protocol::UiInputStatus::Completed
    {
        app.push_error(
            error
                .clone()
                .unwrap_or_else(|| format!("Input ended: {status:?}")),
        );
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
mod tests {
    use super::*;
    #[tokio::test]
    async fn pending_input_interrupt_is_targeted_before_activity_is_observed() {
        let (shell, root, _, pid) = super::super::stdio_tests::live_root_agent().await;
        let pending = PendingRootAgentTurn {
            input: "queued task".into(),
            submission_id: "00000000-0000-4000-8000-000000000001".into(),
            submitted_process: Some(pid.parse().unwrap()),
            submitted_at_ms: 20,
        };
        let pending_turns = VecDeque::from([
            pending,
            PendingRootAgentTurn {
                input: "later task".into(),
                submission_id: "00000000-0000-4000-8000-000000000002".into(),
                submitted_process: Some(pid.parse().unwrap()),
                submitted_at_ms: 21,
            },
        ]);
        root.set_root_process("99999").await;
        for refreshed_pid in [Some(99999), None] {
            let (agent_path, command) =
                interrupt_control("/agent/root", &pending_turns, refreshed_pid).unwrap();
            assert_eq!(agent_path, format!("/agent/{pid}"));
            super::super::file_surface::write_machine_ctl(&shell, &agent_path, &command)
                .await
                .unwrap();
        }
        let events =
            String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap()).unwrap();
        assert!(events.contains(&format!(
            "ctl:queue-v1 interrupt {}",
            pending_turns.front().unwrap().submission_id
        )));
        assert!(!events.contains("ctl:interrupt"));
        assert!(!events.contains(&pending_turns.back().unwrap().submission_id));
    }

    #[test]
    fn replacement_that_later_becomes_idle_reports_unknown_and_releases_pending() {
        let mut pending = VecDeque::from([
            PendingRootAgentTurn {
                input: "task".into(),
                submission_id: "input-one".into(),
                submitted_process: Some(1),
                submitted_at_ms: 20,
            },
            PendingRootAgentTurn {
                input: "task two".into(),
                submission_id: "input-two".into(),
                submitted_process: Some(2),
                submitted_at_ms: 21,
            },
        ]);
        let mut app = FileBackedApp::new("/agent/root".into());
        app.activity.state = UiActivityState::Running;
        settle_unknown_replaced_input(&mut pending, Some(1), &mut app);
        assert!(
            !pending.is_empty(),
            "a running turn is not settled by Root identity refresh"
        );
        settle_unknown_replaced_input(&mut pending, Some(2), &mut app);
        assert!(!pending.is_empty());
        app.activity.state = UiActivityState::Idle;
        settle_unknown_replaced_input(&mut pending, Some(2), &mut app);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending.front().unwrap().submission_id, "input-two");
        settle_unknown_replaced_input(&mut pending, Some(3), &mut app);
        assert!(pending.is_empty());
        assert!(
            matches!(app.transcript.last(), Some(crate::history::HistoryCell::Error(message))
            if message.contains("outcome is unknown"))
        );
    }
    #[test]
    fn matching_non_success_completion_is_visible_before_pending_is_released() {
        for status in [
            alan_agent_protocol::UiInputStatus::Failed,
            alan_agent_protocol::UiInputStatus::Cancelled,
        ] {
            let mut pending = VecDeque::from([PendingRootAgentTurn {
                input: "task".into(),
                submission_id: "mine".into(),
                submitted_process: Some(1),
                submitted_at_ms: 20,
            }]);
            let mut app = FileBackedApp::new("/agent/root".into());
            let mut event = UiEvent::InputCompleted {
                submission_ids: vec!["other".into()],
                status,
                error: Some("reason".into()),
            };
            observe_root_agent_completion(&mut pending, &event, &mut app);
            assert_eq!(pending.len(), 1);
            assert!(app.transcript.is_empty());
            if let UiEvent::InputCompleted { submission_ids, .. } = &mut event {
                *submission_ids = vec!["mine".into()];
            }
            observe_root_agent_completion(&mut pending, &event, &mut app);
            assert!(pending.is_empty());
            assert!(
                matches!(app.transcript.last(), Some(crate::history::HistoryCell::Error(message)) if message == "reason")
            );
            app.apply_ui_event(event);
            if status == alan_agent_protocol::UiInputStatus::Failed {
                app.apply_ui_event(UiEvent::Notice {
                    snapshot: alan_agent_protocol::UiNoticeSnapshot::new(
                        alan_agent_protocol::UiNoticeKind::Error,
                        "Error handling submission: reason",
                    ),
                });
                app.apply_ui_event(UiEvent::Error {
                    message: "Error handling submission: reason".into(),
                    recoverable: true,
                });
                assert_eq!(app.transcript.len(), 1);
                // Only the immediately paired terminal error is suppressed.
                app.apply_ui_event(UiEvent::Error {
                    message: "Error handling submission: reason".into(),
                    recoverable: true,
                });
                assert_eq!(app.transcript.len(), 2);
            }
        }
    }
}
