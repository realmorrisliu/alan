//! Ordinary submissions retained by an Agent Machine across transition cancellation.
use std::collections::VecDeque;

use alan_agent_protocol::Submission;

#[derive(Debug)]
pub(crate) enum QueuedRuntimeItem {
    Submission(Submission),
    Deferred(super::DeferredRuntimeAction),
}

#[derive(Debug, Default)]
pub(crate) struct MachineInputQueue {
    pub(crate) pending: VecDeque<QueuedRuntimeItem>,
    pub(crate) paused: bool,
}

impl super::AgentMachine {
    pub(crate) fn input_queue(&self) -> std::sync::Arc<std::sync::Mutex<MachineInputQueue>> {
        self.transition_state.input_queue.clone()
    }
}
