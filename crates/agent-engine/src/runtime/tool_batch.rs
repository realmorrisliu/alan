//! Tool-batch inputs, outcomes, and replay selection.

use tokio_util::sync::CancellationToken;

use super::turn_input::TurnInputBroker;
use crate::agent_machine::NormalizedToolCall;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolOrchestratorOutcome {
    ContinueToolBatch { refresh_context: bool },
    PauseTurn,
    EndTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolBatchOrchestratorOutcome {
    ContinueTurnLoop { refresh_context: bool },
    PauseTurn,
    EndTurn { surfaces_refreshed: bool },
}

pub(super) fn approved_replay_call_index(
    tool_calls: &[NormalizedToolCall],
    approved_call_id: Option<&str>,
) -> Option<usize> {
    approved_call_id.and_then(|call_id| {
        tool_calls
            .first()
            .filter(|call| call.id == call_id)
            .map(|_| 0)
    })
}

#[derive(Clone, Copy)]
pub(super) struct ToolOrchestratorInputs<'a> {
    pub cancel: &'a CancellationToken,
    pub steering_broker: Option<&'a TurnInputBroker>,
}
