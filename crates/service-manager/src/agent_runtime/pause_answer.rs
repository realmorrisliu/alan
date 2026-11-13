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
