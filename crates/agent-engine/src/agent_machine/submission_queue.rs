//! Deferred inputs retain command identity until an explicit turn releases them.
use super::*;
use alan_agent_protocol::{InputIntent, InputMode, Op};

impl AgentMachine {
    pub(crate) fn take_steering_command(&mut self) -> Option<Submission> {
        let queue = &mut self.transition_state.buffered_inband_submissions;
        let index = queue.iter().position(|submission| {
            submission.intent == InputIntent::Command
                && matches!(
                    submission.op,
                    Op::Input {
                        mode: InputMode::Steer,
                        ..
                    }
                )
        })?;
        queue.remove(index)
    }

    pub(crate) fn queue_next_turn_input(&mut self, parts: Vec<ContentPart>) -> Option<usize> {
        let mut submission = Submission::new(Op::Input {
            parts,
            mode: InputMode::NextTurn,
        });
        if let Some(id) = self.current_submission_id() {
            submission.id = id.to_owned();
        }
        self.queue_next_turn_submission(submission)
    }

    pub(crate) fn queue_next_turn_submission(&mut self, submission: Submission) -> Option<usize> {
        let queue = &mut self.transition_state.queued_next_turn_inputs;
        if queue.len() >= MAX_QUEUED_NEXT_TURN_INPUTS {
            return None;
        }
        queue.push_back(submission);
        Some(queue.len())
    }

    /// Release command submissions separately from context; never feed scripts to generation.
    pub(crate) fn take_next_turn_commands(&mut self) -> VecDeque<Submission> {
        let pending = std::mem::take(&mut self.transition_state.queued_next_turn_inputs);
        let (commands, context) = pending
            .into_iter()
            .partition(|submission| submission.intent == InputIntent::Command);
        self.transition_state.queued_next_turn_inputs = context;
        commands
    }

    pub(crate) fn drain_next_turn_inputs(&mut self) -> VecDeque<Vec<ContentPart>> {
        let pending = std::mem::take(&mut self.transition_state.queued_next_turn_inputs);
        let mut context = VecDeque::new();
        for submission in pending {
            if submission.intent == InputIntent::Command {
                self.transition_state
                    .queued_next_turn_inputs
                    .push_back(submission);
            } else if let Op::Input { parts, .. } = submission.op {
                context.push_back(parts);
            }
        }
        context
    }

    #[cfg(test)]
    pub(crate) fn queued_next_turn_input_count(&self) -> usize {
        self.transition_state.queued_next_turn_inputs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_queue_next_turn_inputs_fifo_and_drain() {
        let mut state = AgentMachine::new();
        assert_eq!(
            state.queue_next_turn_input(vec![ContentPart::text("ctx-1")]),
            Some(1)
        );
        assert_eq!(
            state.queue_next_turn_input(vec![ContentPart::text("ctx-2")]),
            Some(2)
        );
        assert_eq!(state.queued_next_turn_input_count(), 2);

        let drained = state.drain_next_turn_inputs();
        assert_eq!(drained.len(), 2);
        assert_eq!(alan_agent_protocol::parts_to_text(&drained[0]), "ctx-1");
        assert_eq!(alan_agent_protocol::parts_to_text(&drained[1]), "ctx-2");
        assert_eq!(state.queued_next_turn_input_count(), 0);
    }

    #[test]
    fn test_queue_next_turn_inputs_overflow_is_rejected() {
        let mut state = AgentMachine::new();
        for _ in 0..MAX_QUEUED_NEXT_TURN_INPUTS {
            assert!(
                state
                    .queue_next_turn_input(vec![ContentPart::text("queued")])
                    .is_some()
            );
        }
        assert!(
            state
                .queue_next_turn_input(vec![ContentPart::text("overflow")])
                .is_none()
        );
    }
}
