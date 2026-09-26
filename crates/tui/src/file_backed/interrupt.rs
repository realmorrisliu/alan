use alan_agent_protocol::{UiActivityState, UiEvent};

use super::app::FileBackedApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingRootAgentTurn {
    pub(super) input: String,
    pub(super) submission_id: String,
    pub(super) submitted_process: Option<u64>,
    pub(super) submitted_at_ms: u64,
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

pub(super) async fn send_interrupt(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    pending: Option<&PendingRootAgentTurn>,
    root_pid: Option<u64>,
) {
    let agent_path = if app.agent_path == "/agent/root" {
        let Some(pid) = pending.map_or(root_pid, |turn| turn.submitted_process) else {
            app.push_error("Root Agent is not attached; retry interrupt".into());
            return;
        };
        format!("/agent/{pid}")
    } else {
        app.agent_path.clone()
    };
    let result = if let Some(turn) = pending {
        super::file_surface::write_machine_ctl(
            shell,
            &agent_path,
            &format!("queue-v1 interrupt {}", turn.submission_id),
        )
        .await
    } else {
        super::file_surface::write_interrupt(shell, &agent_path).await
    };
    match result {
        Ok(()) => app.notice = Some("interrupt requested".to_string()),
        Err(err) => app.push_error(format!("interrupt failed: {err:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn pending_input_interrupt_is_targeted_before_activity_is_observed() {
        let (shell, root, _, pid) = super::super::stdio_tests::live_root_agent().await;
        let mut app = FileBackedApp::new("/agent/root".into());
        let pending = PendingRootAgentTurn {
            input: "queued task".into(),
            submission_id: "00000000-0000-4000-8000-000000000001".into(),
            submitted_process: Some(pid.parse().unwrap()),
            submitted_at_ms: 20,
        };
        root.set_root_process("99999").await;
        for refreshed_pid in [Some(99999), None] {
            app.notice = None;
            send_interrupt(&shell, &mut app, Some(&pending), refreshed_pid).await;
            assert_eq!(app.notice.as_deref(), Some("interrupt requested"));
        }
        let events =
            String::from_utf8(shell.cat(&format!("/agent/{pid}/events")).await.unwrap()).unwrap();
        assert!(events.contains(&format!("ctl:queue-v1 interrupt {}", pending.submission_id)));
        assert!(!events.contains("ctl:interrupt"));
        assert_eq!(app.notice.as_deref(), Some("interrupt requested"));
    }

    #[test]
    fn replacement_that_later_becomes_idle_reports_unknown_and_releases_pending() {
        let mut pending = Some(PendingRootAgentTurn {
            input: "task".into(),
            submission_id: "input-one".into(),
            submitted_process: Some(1),
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
