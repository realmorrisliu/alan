//! Whether a paused Agent Process is still able to receive the answer it waits for.
//!
//! A Process that pauses has published a request; the answer arrives on its own
//! submission channel. Pausing therefore means waiting, not finishing. The wait ends
//! only when the Process is stopped (cancel/timeout) — never because a clock elapsed
//! while an answer could still arrive.

/// `paused_with_request`: the Process is paused and has a pending request to answer.
/// `stopped`: the run was cancelled, timed out, or its parent went away.
pub(super) fn pause_keeps_waiting(paused_with_request: bool, stopped: bool) -> bool {
    paused_with_request && !stopped
}

#[cfg(test)]
mod tests {
    use super::pause_keeps_waiting;

    #[test]
    fn a_process_paused_on_a_request_keeps_waiting_for_its_answer() {
        assert!(pause_keeps_waiting(true, false));
    }

    #[test]
    fn a_stopped_run_ends_even_while_a_request_is_pending() {
        assert!(!pause_keeps_waiting(true, true));
    }

    #[test]
    fn a_process_without_a_pending_request_is_not_waiting() {
        assert!(!pause_keeps_waiting(false, false));
    }
}

/// What the host should do with one observation of a child Process.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ChildTerminal {
    /// Keep reading: the answer can still arrive.
    Wait,
    /// A pause that can no longer be answered (`stopped`: cancelled or timed out).
    Paused,
    Completed,
    Failed,
}

/// The host's terminal decision, separated from the filesystem it reads.
pub(super) fn decide_child_terminal(
    activity: alan_agent_engine::UiActivityState,
    has_pending_request: bool,
    notice_is_error: bool,
    stopped: bool,
) -> ChildTerminal {
    use alan_agent_engine::UiActivityState;
    if notice_is_error {
        return ChildTerminal::Failed;
    }
    if activity == UiActivityState::Running {
        // Work in progress is the furthest thing from an outcome.
        return ChildTerminal::Wait;
    }
    if activity != UiActivityState::Paused {
        return ChildTerminal::Completed;
    }
    if !has_pending_request {
        return ChildTerminal::Wait;
    }
    if pause_keeps_waiting(true, stopped) {
        ChildTerminal::Wait
    } else {
        ChildTerminal::Paused
    }
}

#[cfg(test)]
mod terminal_tests {
    use super::{ChildTerminal, decide_child_terminal};

    #[test]
    fn a_pause_that_can_still_be_answered_keeps_waiting() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Paused,
                true,
                false,
                false
            ),
            ChildTerminal::Wait
        );
    }

    #[test]
    fn a_stopped_run_publishes_its_pause() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Paused,
                true,
                false,
                true
            ),
            ChildTerminal::Paused
        );
    }

    #[test]
    fn an_answered_run_completes() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Idle,
                false,
                false,
                false
            ),
            ChildTerminal::Completed
        );
    }

    #[test]
    fn a_running_process_is_not_finished() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Running,
                false,
                false,
                false
            ),
            ChildTerminal::Wait
        );
    }

    #[test]
    fn an_error_notice_fails_the_run() {
        assert_eq!(
            decide_child_terminal(alan_agent_engine::UiActivityState::Idle, false, true, false),
            ChildTerminal::Failed
        );
    }

    #[test]
    fn a_pause_without_a_request_keeps_reading() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Paused,
                false,
                false,
                false
            ),
            ChildTerminal::Wait
        );
    }
}
