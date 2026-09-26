use alan_agent_protocol::{UiActivityState, UiEvent};

use super::app::FileBackedApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingRootAgentTurn {
    pub(super) input: String,
    pub(super) submission_id: String,
    pub(super) submitted_process: Option<u64>,
    pub(super) observed_active: bool,
    pub(super) interrupt_requested: bool,
    pub(super) submitted_at_ms: u64,
}

pub(super) fn observe_root_agent_activity(
    pending_turn: &mut Option<PendingRootAgentTurn>,
    activity: UiActivityState,
) -> bool {
    if let Some(turn) = pending_turn {
        match activity {
            UiActivityState::Running | UiActivityState::Paused => {
                turn.observed_active = true;
                std::mem::take(&mut turn.interrupt_requested)
            }
            UiActivityState::Idle => false,
        }
    } else {
        false
    }
}

pub(super) fn observe_root_agent_completion(
    pending_turn: &mut Option<PendingRootAgentTurn>,
    event: &UiEvent,
    app: &mut FileBackedApp,
) {
    if let UiEvent::InputCompleted { submission_ids, .. } = event
        && pending_turn
            .as_ref()
            .is_some_and(|turn| submission_ids.contains(&turn.submission_id))
    {
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
        *pending_turn = None;
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
    pending_turn: &mut Option<PendingRootAgentTurn>,
    current_process: Option<u64>,
    app: &mut FileBackedApp,
) {
    if app.activity.state == UiActivityState::Idle
        && pending_turn.as_ref().is_some_and(|turn| {
            matches!((turn.submitted_process, current_process),
                (Some(submitted), Some(current)) if submitted != current)
        })
    {
        *pending_turn = None;
        app.push_error(
            "Root Agent changed without correlated completion evidence; outcome is unknown".into(),
        );
    }
}

pub(super) fn request_pending_root_interrupt(
    pending_turn: &mut Option<PendingRootAgentTurn>,
) -> bool {
    let Some(turn) = pending_turn else {
        return true;
    };
    if turn.observed_active {
        true
    } else {
        turn.interrupt_requested = true;
        false
    }
}

pub(super) async fn send_interrupt(shell: &alan_shell::Shell, app: &mut FileBackedApp) {
    let agent_path = app.agent_path.clone();
    match super::file_surface::write_interrupt(shell, &agent_path).await {
        Ok(()) => app.notice = Some("interrupt sent".to_string()),
        Err(err) => app.push_error(format!("interrupt failed: {err:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn root_agent_interrupt_waits_until_the_submitted_turn_is_accepted() {
        let mut pending = Some(PendingRootAgentTurn {
            input: "current task".to_string(),
            submission_id: "input-one".into(),
            submitted_process: Some(1),
            observed_active: false,
            interrupt_requested: false,
            submitted_at_ms: 20,
        });

        assert!(!request_pending_root_interrupt(&mut pending));
        assert!(!observe_root_agent_activity(
            &mut pending,
            UiActivityState::Idle
        ));
        assert!(pending.as_ref().unwrap().interrupt_requested);

        assert!(observe_root_agent_activity(
            &mut pending,
            UiActivityState::Running
        ));
        assert!(!pending.as_ref().unwrap().interrupt_requested);
        assert!(!observe_root_agent_activity(
            &mut pending,
            UiActivityState::Idle
        ));
        assert!(pending.is_some());
    }

    #[test]
    fn pending_root_agent_interrupt_is_discarded_if_task_settles_before_activation() {
        let mut pending = Some(PendingRootAgentTurn {
            input: "current task".to_string(),
            submission_id: "input-one".into(),
            submitted_process: Some(1),
            observed_active: false,
            interrupt_requested: false,
            submitted_at_ms: 20,
        });

        assert!(!request_pending_root_interrupt(&mut pending));
        observe_root_agent_completion(
            &mut pending,
            &UiEvent::InputCompleted {
                submission_ids: vec!["input-one".into()],
                status: alan_agent_protocol::UiInputStatus::Completed,
                error: None,
            },
            &mut FileBackedApp::new("/agent/root".into()),
        );
        assert!(!observe_root_agent_activity(
            &mut pending,
            UiActivityState::Idle
        ));
        assert_eq!(pending, None);
    }
    #[test]
    fn replacement_that_later_becomes_idle_reports_unknown_and_releases_pending() {
        let mut pending = Some(PendingRootAgentTurn {
            input: "task".into(),
            submission_id: "input-one".into(),
            submitted_process: Some(1),
            observed_active: true,
            interrupt_requested: false,
            submitted_at_ms: 20,
        });
        let mut app = FileBackedApp::new("/agent/root".into());
        settle_unknown_replaced_input(&mut pending, Some(1), &mut app);
        assert!(
            pending.is_some(),
            "the original Process idle is not settlement"
        );
        app.activity.state = UiActivityState::Running;
        settle_unknown_replaced_input(&mut pending, Some(2), &mut app);
        assert!(pending.is_some());
        app.activity.state = UiActivityState::Idle;
        settle_unknown_replaced_input(&mut pending, Some(2), &mut app);
        assert!(pending.is_none());
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
            let mut pending = Some(PendingRootAgentTurn {
                input: "task".into(),
                submission_id: "mine".into(),
                submitted_process: Some(1),
                observed_active: false,
                interrupt_requested: false,
                submitted_at_ms: 20,
            });
            let mut app = FileBackedApp::new("/agent/root".into());
            let mut event = UiEvent::InputCompleted {
                submission_ids: vec!["other".into()],
                status,
                error: Some("reason".into()),
            };
            observe_root_agent_completion(&mut pending, &event, &mut app);
            assert!(pending.is_some());
            assert!(app.transcript.is_empty());
            if let UiEvent::InputCompleted { submission_ids, .. } = &mut event {
                *submission_ids = vec!["mine".into()];
            }
            observe_root_agent_completion(&mut pending, &event, &mut app);
            assert!(pending.is_none());
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
