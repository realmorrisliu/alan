//! Ordinary submissions retained by an Agent Machine across transition cancellation.
use std::collections::VecDeque;

use alan_agent_protocol::{ContentPart, Submission};

#[derive(Debug)]
pub(crate) enum QueuedRuntimeItem {
    Submission(Submission),
    Deferred(super::DeferredRuntimeAction),
}

#[derive(Debug, Default)]
pub(crate) struct MachineInputQueue {
    pub(crate) pending: VecDeque<QueuedRuntimeItem>,
    pub(crate) paused: bool,
    pub(crate) inband: VecDeque<Submission>,
    pub(crate) buffered_inband_submissions: VecDeque<Submission>,
    pub(crate) queued_next_turn_inputs: VecDeque<(Option<String>, Vec<ContentPart>)>,
    pub(crate) notify: std::sync::Arc<tokio::sync::Notify>,
}

impl super::AgentMachine {
    pub(crate) fn input_queue(&self) -> std::sync::Arc<std::sync::Mutex<MachineInputQueue>> {
        self.transition_state.input_queue.clone()
    }
}
