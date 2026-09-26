use alan_agent_protocol::{UiActivitySnapshot, UiActivityState};

use super::app::FileBackedApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingRootAgentTurn {
    pub(super) input: String,
    pub(super) submission_id: String,
    pub(super) observed_active: bool,
    pub(super) interrupt_requested: bool,
    pub(super) submitted_at_ms: u64,
    pub(super) prior_matching_turns: usize,
}

pub(super) fn observe_root_agent_activity(
    pending_turn: &mut Option<PendingRootAgentTurn>,
    activity: &UiActivitySnapshot,
) -> bool {
    if let Some(turn) = pending_turn {
        if activity.version >= 2 {
            let accepted = activity
                .active_submission
                .as_ref()
                .is_some_and(|input| input.submission_id == turn.submission_id)
                || activity
                    .pending_submissions
                    .iter()
                    .any(|input| input.submission_id == turn.submission_id);
            if accepted {
                turn.observed_active = true;
                return std::mem::take(&mut turn.interrupt_requested);
            }
            if activity.active_submission.is_none()
                && activity.state == UiActivityState::Idle
                && turn.observed_active
            {
                *pending_turn = None;
            }
            return false;
        }
        match activity.state {
            UiActivityState::Running | UiActivityState::Paused => {
                turn.observed_active = true;
                std::mem::take(&mut turn.interrupt_requested)
            }
            UiActivityState::Idle if turn.observed_active => {
                *pending_turn = None;
                false
            }
            UiActivityState::Idle => false,
        }
    } else {
        false
    }
}

pub(super) fn settle_failed_input(pending: &mut Option<PendingRootAgentTurn>, id: &str) {
    if pending
        .as_ref()
        .is_some_and(|turn| turn.submission_id == id)
    {
        *pending = None;
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

pub(super) async fn send_interrupt(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    pending: Option<&PendingRootAgentTurn>,
) {
    let agent_path = app.agent_path.clone();
    let target = pending
        .map(|input| input.submission_id.as_str())
        .or_else(|| {
            app.activity
                .active_submission
                .as_ref()
                .map(|input| input.submission_id.as_str())
        });
    let result = if let Some(id) = target {
        super::file_surface::write_machine_ctl(
            shell,
            &agent_path,
            &format!("queue-v1 interrupt {id}"),
        )
        .await
    } else if app.activity.version < 2 {
        super::file_surface::write_interrupt(shell, &agent_path).await
    } else {
        app.notice = Some("no active input to interrupt".into());
        return;
    };
    match result {
        Ok(()) => app.notice = Some("interruption requested".to_string()),
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
            submission_id: "current-id".into(),
            observed_active: false,
            interrupt_requested: false,
            submitted_at_ms: 20,
            prior_matching_turns: 0,
        });

        assert!(!request_pending_root_interrupt(&mut pending));
        assert!(!observe_root_agent_activity(
            &mut pending,
            &UiActivitySnapshot {
                version: 1,
                state: UiActivityState::Idle,
                ..UiActivitySnapshot::idle()
            }
        ));
        assert!(pending.as_ref().unwrap().interrupt_requested);

        assert!(observe_root_agent_activity(
            &mut pending,
            &UiActivitySnapshot {
                version: 1,
                state: UiActivityState::Running,
                ..UiActivitySnapshot::idle()
            }
        ));
        assert!(!pending.as_ref().unwrap().interrupt_requested);
        assert!(!observe_root_agent_activity(
            &mut pending,
            &UiActivitySnapshot {
                version: 1,
                state: UiActivityState::Idle,
                ..UiActivitySnapshot::idle()
            }
        ));
        assert_eq!(pending, None);
    }

    #[test]
    fn pending_root_agent_interrupt_is_discarded_if_task_settles_before_activation() {
        let mut pending = Some(PendingRootAgentTurn {
            input: "current task".to_string(),
            submission_id: "current-id".into(),
            observed_active: false,
            interrupt_requested: false,
            submitted_at_ms: 20,
            prior_matching_turns: 0,
        });

        assert!(!request_pending_root_interrupt(&mut pending));
        pending.as_mut().unwrap().observed_active = true;
        assert!(!observe_root_agent_activity(
            &mut pending,
            &UiActivitySnapshot {
                version: 1,
                state: UiActivityState::Idle,
                ..UiActivitySnapshot::idle()
            }
        ));
        assert_eq!(pending, None);
    }
    #[tokio::test]
    async fn queued_local_interrupt_ignores_foreign_activity_and_targets_its_own_id() {
        use alan_agent_protocol::{InputIntent, UiSubmission};
        let (shell, _root, _namespace, _pid) = super::super::stdio_tests::live_root_agent().await;
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let mut pending = Some(PendingRootAgentTurn {
            input: "same prompt".into(),
            submission_id: id.into(),
            observed_active: false,
            interrupt_requested: false,
            submitted_at_ms: 20,
            prior_matching_turns: 0,
        });
        let mut app = FileBackedApp::new("/agent/root".into());
        app.activity = UiActivitySnapshot::running(21);
        app.activity.active_submission = Some(UiSubmission {
            submission_id: "550e8400-e29b-41d4-a716-446655440001".into(),
            intent: InputIntent::Agent,
        });
        assert!(!request_pending_root_interrupt(&mut pending));
        assert!(!observe_root_agent_activity(&mut pending, &app.activity));
        assert!(!pending.as_ref().unwrap().observed_active);
        app.activity.pending_submissions.push(UiSubmission {
            submission_id: id.into(),
            intent: InputIntent::Command,
        });
        assert!(observe_root_agent_activity(&mut pending, &app.activity));
        send_interrupt(&shell, &mut app, pending.as_ref()).await;
        let events = String::from_utf8(shell.cat("/agent/root/events").await.unwrap()).unwrap();
        assert!(events.contains(&format!("ctl:queue-v1 interrupt {id}")));
        assert!(!events.contains("ctl:queue-v1 interrupt 550e8400-e29b-41d4-a716-446655440001"));
        app.activity.active_submission = None;
        app.activity.state = UiActivityState::Idle;
        assert!(!observe_root_agent_activity(&mut pending, &app.activity));
        assert!(
            pending.is_some(),
            "idle does not settle an accepted queued input"
        );
        app.activity.pending_submissions.clear();
        assert!(!observe_root_agent_activity(&mut pending, &app.activity));
        assert!(pending.is_none());
    }

    #[test]
    fn a_correlated_failure_clears_the_pending_interrupt_target_without_adopting_other_errors() {
        let mut pending = Some(PendingRootAgentTurn {
            input: "task".into(),
            submission_id: "mine".into(),
            observed_active: true,
            interrupt_requested: true,
            submitted_at_ms: 1,
            prior_matching_turns: 0,
        });
        settle_failed_input(&mut pending, "other");
        assert!(pending.is_some());
        settle_failed_input(&mut pending, "mine");
        assert!(pending.is_none());
    }
}
