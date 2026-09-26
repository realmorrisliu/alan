use alan_agent_protocol::{UiActivityState, UiEvent};

use super::app::FileBackedApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingRootAgentTurn {
    pub(super) input: String,
    pub(super) submission_id: String,
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
) {
    if let UiEvent::InputCompleted { submission_ids, .. } = event
        && pending_turn
            .as_ref()
            .is_some_and(|turn| submission_ids.contains(&turn.submission_id))
    {
        *pending_turn = None;
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
        );
        assert!(!observe_root_agent_activity(
            &mut pending,
            UiActivityState::Idle
        ));
        assert_eq!(pending, None);
    }
}
