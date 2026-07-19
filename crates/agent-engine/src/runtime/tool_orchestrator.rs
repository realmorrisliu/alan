use alan_agent_protocol::Event;
use anyhow::Result;
#[cfg(test)]
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::approval::replays_tool_calls;

use super::loop_guard::ToolLoopGuard;
use super::steering_queue::handle_queued_steering_inputs;
use super::tool_authorization::{
    ToolAuthorizationOutcome, ToolAuthorizationRequest, authorize_tool_call,
};
#[cfg(test)]
use super::tool_effect_lifecycle::{EffectCategory, build_effect_identity};
use super::tool_execution::{
    ToolExecutionOutcome, ToolExecutionRequest, execute_allowed_tool_call,
};
#[cfg(test)]
use super::tool_execution::{execute_tool_effect, tool_payload_for_tape};
use super::tool_resolution::{ToolResolutionOutcome, ToolResolutionRequest, resolve_tool_call};
use super::transition::{RuntimeLoopState, dispatch_virtual_tool_call};
use super::turn_driver::TurnInputBroker;
use super::virtual_tool::VirtualToolOutcome;
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

pub(super) struct ToolTurnOrchestrator {
    loop_guard: ToolLoopGuard,
}

impl ToolTurnOrchestrator {
    pub(super) fn new(max_tool_loops: Option<usize>, tool_repeat_limit: usize) -> Self {
        Self {
            loop_guard: ToolLoopGuard::new(max_tool_loops, tool_repeat_limit),
        }
    }

    pub(super) async fn orchestrate_tool_batch<E, F>(
        &mut self,
        state: &mut RuntimeLoopState,
        tool_calls: &[NormalizedToolCall],
        inputs: ToolOrchestratorInputs<'_>,
        emit: &mut E,
    ) -> Result<ToolBatchOrchestratorOutcome>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        self.orchestrate_tool_batch_internal(state, tool_calls, inputs, None, None, emit)
            .await
    }

    async fn orchestrate_tool_batch_internal<E, F>(
        &mut self,
        state: &mut RuntimeLoopState,
        tool_calls: &[NormalizedToolCall],
        inputs: ToolOrchestratorInputs<'_>,
        approved_unknown_effect_call_index: Option<usize>,
        approved_tool_escalation_call_index: Option<usize>,
        emit: &mut E,
    ) -> Result<ToolBatchOrchestratorOutcome>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        orchestrate_tool_batch_with_guard(
            state,
            &mut self.loop_guard,
            tool_calls,
            inputs,
            approved_unknown_effect_call_index,
            approved_tool_escalation_call_index,
            emit,
        )
        .await
    }
}

pub(super) async fn replay_approved_tool_call_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    approved_unknown_effect_call_id: Option<&str>,
    approved_tool_escalation_call_id: Option<&str>,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    replay_approved_tool_batch_with_cancel(
        state,
        std::slice::from_ref(tool_call),
        approved_unknown_effect_call_id,
        approved_tool_escalation_call_id,
        inputs,
        emit,
    )
    .await
}

pub(super) async fn replay_approved_tool_batch_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    approved_unknown_effect_call_id: Option<&str>,
    approved_tool_escalation_call_id: Option<&str>,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let max_tool_loops = if state.runtime_config.max_tool_loops == 0 {
        None
    } else {
        Some(state.runtime_config.max_tool_loops)
    };
    let approved_unknown_effect_call_index = approved_unknown_effect_call_id.and_then(|call_id| {
        tool_calls
            .first()
            .filter(|call| call.id == call_id)
            .map(|_| 0)
    });
    let approved_tool_escalation_call_index =
        approved_tool_escalation_call_id.and_then(|call_id| {
            tool_calls
                .first()
                .filter(|call| call.id == call_id)
                .map(|_| 0)
        });
    let mut orchestrator =
        ToolTurnOrchestrator::new(max_tool_loops, state.runtime_config.tool_repeat_limit);
    orchestrator
        .orchestrate_tool_batch_internal(
            state,
            tool_calls,
            inputs,
            approved_unknown_effect_call_index,
            approved_tool_escalation_call_index,
            emit,
        )
        .await
}

#[derive(Clone, Copy)]
pub(super) struct ToolOrchestratorInputs<'a> {
    pub cancel: &'a CancellationToken,
    pub steering_broker: Option<&'a TurnInputBroker>,
}

async fn orchestrate_tool_call_with_guard<E, F>(
    state: &mut RuntimeLoopState,
    loop_guard: &mut ToolLoopGuard,
    tool_call: &NormalizedToolCall,
    inputs: ToolOrchestratorInputs<'_>,
    allow_approved_unknown_effect_execution: bool,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<ToolOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let tool_arguments = tool_call.arguments.clone();

    if let Some(msg) = loop_guard.before_tool_call(&tool_call.name, &tool_arguments) {
        emit(Event::Error {
            message: msg.clone(),
            recoverable: true,
        })
        .await;
        emit(Event::TextDelta {
            chunk: msg,
            is_final: true,
        })
        .await;
        return Ok(ToolOrchestratorOutcome::EndTurn);
    }

    match dispatch_virtual_tool_call(
        state,
        tool_call,
        &tool_arguments,
        inputs.cancel,
        allow_approved_tool_escalation_execution,
        emit,
    )
    .await?
    {
        VirtualToolOutcome::NotVirtual => {}
        VirtualToolOutcome::Continue { refresh_context } => {
            return Ok(ToolOrchestratorOutcome::ContinueToolBatch { refresh_context });
        }
        VirtualToolOutcome::PauseTurn => return Ok(ToolOrchestratorOutcome::PauseTurn),
        VirtualToolOutcome::EndTurn => return Ok(ToolOrchestratorOutcome::EndTurn),
    }

    let resolution_runtime = super::transition::tool_resolution_runtime(state);
    let resolution_request = ToolResolutionRequest {
        tool_call,
        tool_arguments: &tool_arguments,
    };
    let resolved_tool =
        match resolve_tool_call(resolution_runtime, resolution_request, emit).await? {
            ToolResolutionOutcome::Resolved(resolved) => resolved,
            ToolResolutionOutcome::Unavailable => {
                return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                    refresh_context: false,
                });
            }
        };
    let authorization_runtime = super::transition::tool_authorization_runtime(state);
    let authorization_request = ToolAuthorizationRequest {
        tool_call,
        tool_arguments: &tool_arguments,
        tool_capability: resolved_tool.capability,
        current_tool_cwd: resolved_tool.current_cwd.as_deref(),
        allow_approved_tool_escalation_execution,
        cancel: inputs.cancel,
    };
    let tool_audit =
        match authorize_tool_call(authorization_runtime, authorization_request, emit).await? {
            ToolAuthorizationOutcome::Authorized { audit } => Some(audit),
            ToolAuthorizationOutcome::Completed => {
                return Ok(ToolOrchestratorOutcome::ContinueToolBatch {
                    refresh_context: false,
                });
            }
            ToolAuthorizationOutcome::PauseTurn => {
                return Ok(ToolOrchestratorOutcome::PauseTurn);
            }
        };

    let runtime = super::transition::tool_execution_runtime(state);
    let request = ToolExecutionRequest {
        tool_call,
        tool_arguments: &tool_arguments,
        tool_timeout_secs: resolved_tool.timeout_secs,
        tool_capability: resolved_tool.capability,
        tool_audit,
        allow_approved_unknown_effect_execution,
        cancel: inputs.cancel,
    };
    match execute_allowed_tool_call(runtime, request, emit).await? {
        ToolExecutionOutcome::Completed => Ok(ToolOrchestratorOutcome::ContinueToolBatch {
            refresh_context: false,
        }),
        ToolExecutionOutcome::PauseTurn => Ok(ToolOrchestratorOutcome::PauseTurn),
        ToolExecutionOutcome::EndTurn => Ok(ToolOrchestratorOutcome::EndTurn),
    }
}

async fn orchestrate_tool_batch_with_guard<E, F>(
    state: &mut RuntimeLoopState,
    loop_guard: &mut ToolLoopGuard,
    tool_calls: &[NormalizedToolCall],
    inputs: ToolOrchestratorInputs<'_>,
    approved_unknown_effect_call_index: Option<usize>,
    approved_tool_escalation_call_index: Option<usize>,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let mut refresh_context = false;

    for (idx, tool_call) in tool_calls.iter().enumerate() {
        let allow_approved_unknown_effect_execution =
            approved_unknown_effect_call_index.is_some_and(|approved_index| approved_index == idx);
        let allow_approved_tool_escalation_execution =
            approved_tool_escalation_call_index.is_some_and(|approved_index| approved_index == idx);
        match orchestrate_tool_call_with_guard(
            state,
            loop_guard,
            tool_call,
            inputs,
            allow_approved_unknown_effect_execution,
            allow_approved_tool_escalation_execution,
            emit,
        )
        .await?
        {
            ToolOrchestratorOutcome::ContinueToolBatch {
                refresh_context: call_refresh,
            } => {
                refresh_context |= call_refresh;
                if handle_queued_steering_inputs(
                    &mut state.machine,
                    tool_calls,
                    idx + 1,
                    inputs.steering_broker,
                    emit,
                )
                .await?
                {
                    return Ok(ToolBatchOrchestratorOutcome::ContinueTurnLoop {
                        refresh_context: true,
                    });
                }
            }
            ToolOrchestratorOutcome::PauseTurn => {
                if let Some(pending) = state.machine.pending_confirmation()
                    && replays_tool_calls(&pending.checkpoint_type)
                {
                    state
                        .machine
                        .set_tool_replay_batch(pending.checkpoint_id, tool_calls[idx..].to_vec());
                }
                return Ok(ToolBatchOrchestratorOutcome::PauseTurn);
            }
            ToolOrchestratorOutcome::EndTurn => {
                return Ok(ToolBatchOrchestratorOutcome::EndTurn {
                    surfaces_refreshed: false,
                });
            }
        }
    }

    if let Some(msg) = loop_guard.after_tool_batch() {
        emit(Event::Error {
            message: msg.clone(),
            recoverable: true,
        })
        .await;
        emit(Event::TextDelta {
            chunk: msg,
            is_final: true,
        })
        .await;
        let memory_dir = state
            .core_config
            .memory
            .enabled
            .then_some(state.core_config.memory.store_dir.as_deref())
            .flatten();
        let process_path = state.process_path();
        super::memory_surfaces::refresh_active_turn_memory_surfaces_best_effort(
            &state.machine,
            memory_dir,
            &process_path,
            "tool-loop-guard-ended-turn",
        )
        .await;
        emit(Event::TurnCompleted {
            summary: Some("Tool loop stopped by loop guard".to_string()),
        })
        .await;
        return Ok(ToolBatchOrchestratorOutcome::EndTurn {
            surfaces_refreshed: true,
        });
    }

    Ok(ToolBatchOrchestratorOutcome::ContinueTurnLoop { refresh_context })
}

#[cfg(test)]
mod tests;
