//! Memory Store surface finalization and deferred promotion for one completed turn.

use crate::agent_machine::DeferredRuntimeAction;

mod runtime_inputs;

pub(super) use runtime_inputs::TurnMemoryRuntime;

#[derive(Debug, Clone, Copy)]
pub(super) struct FinalizeTurnMemoryRequest {
    pub(super) surfaces_refreshed: bool,
    pub(super) surfaces_context: &'static str,
    pub(super) promotion_context: &'static str,
}

pub(super) async fn finalize_turn_memory_best_effort(
    runtime: TurnMemoryRuntime<'_>,
    request: FinalizeTurnMemoryRequest,
) {
    if !request.surfaces_refreshed {
        super::memory_surfaces::refresh_turn_memory_surfaces_best_effort(
            runtime.machine,
            runtime.memory_dir.as_deref(),
            &runtime.process_path,
            request.surfaces_context,
        )
        .await;
    }

    if let Some(job) = super::memory_promotion::build_turn_memory_promotion_job(
        runtime.machine,
        runtime.memory_dir,
        runtime.process_path,
        runtime.llm_request_timeout_secs,
        request.promotion_context,
    ) {
        runtime
            .machine
            .push_deferred_runtime_action(DeferredRuntimeAction::TurnMemoryPromotion(job));
    }
}
