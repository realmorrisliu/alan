use alan_agent_protocol::UiActivityState;

use super::app::FileBackedApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingRootAgentTurn {
    pub(super) input: String,
    pub(super) observed_active: bool,
    pub(super) interrupt_requested: bool,
    pub(super) submitted_at_ms: u64,
    pub(super) prior_matching_turns: usize,
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
