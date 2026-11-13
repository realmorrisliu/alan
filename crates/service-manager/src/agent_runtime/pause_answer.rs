//! Whether a paused Agent Process may still be answered by its own submissions.

/// A Process that is here to answer a stop must outlive that stop's publication;
/// every other pause is that Process's outcome.
pub(super) fn pause_is_terminal(resuming: bool, within_grace: bool) -> bool {
    !resuming || !within_grace
}

#[cfg(test)]
mod tests {
    use super::pause_is_terminal;

    #[test]
    fn a_resuming_process_waits_for_its_answer() {
        assert!(
            !pause_is_terminal(true, true),
            "the answer is still on its way"
        );
    }

    #[test]
    fn a_resuming_process_stops_waiting_after_the_grace_period() {
        assert!(
            pause_is_terminal(true, false),
            "a Process that never answers is paused"
        );
    }

    #[test]
    fn an_ordinary_process_publishes_its_pause_immediately() {
        assert!(pause_is_terminal(false, true));
    }
}
