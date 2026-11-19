//! Turn-bounded context compaction helpers for generation transitions.

use alan_agent_protocol::{CompactionOutcome, Event};
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::runtime::compaction::{CompactionRequest, maybe_compact_context_with_cancel};

use super::{RuntimeLoopState, compaction_runtime};

pub(super) const COMPACTION_TIMEOUT_SECS: u64 = 30;

pub(super) struct TimedCompactionResult {
    pub(super) result: Result<CompactionOutcome>,
    pub(super) timed_out: bool,
}

pub(super) fn should_skip_auto_compaction_for_responses_continuation(
    _state: &mut RuntimeLoopState,
) -> bool {
    false
}

pub(super) async fn maybe_compact_context_with_turn_timeout<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    request: &CompactionRequest,
    parent_cancel: &CancellationToken,
    timeout: std::time::Duration,
) -> TimedCompactionResult
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let compaction_cancel = CancellationToken::new();
    let timeout = tokio::time::sleep(timeout);
    tokio::pin!(timeout);
    let runtime = compaction_runtime(state);
    let compaction = maybe_compact_context_with_cancel(runtime, emit, request, &compaction_cancel);
    tokio::pin!(compaction);

    tokio::select! {
        result = &mut compaction => TimedCompactionResult {
            result,
            timed_out: false,
        },
        _ = parent_cancel.cancelled() => {
            compaction_cancel.cancel();
            TimedCompactionResult {
                result: compaction.await,
                timed_out: false,
            }
        }
        _ = &mut timeout => {
            compaction_cancel.cancel();
            TimedCompactionResult {
                result: compaction.await,
                timed_out: true,
            }
        }
    }
}

pub(super) async fn maybe_compact_mid_turn_if_needed<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    cancel: &CancellationToken,
    additional_prompt_tokens: usize,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if should_skip_auto_compaction_for_responses_continuation(state) {
        return Ok(());
    }

    let estimated_prompt_tokens = state
        .machine
        .estimated_prompt_tokens()
        .saturating_add(additional_prompt_tokens);
    let context_window_tokens = state.runtime_config.context_window_tokens as usize;
    if !state
        .machine
        .can_auto_mid_turn_compact(estimated_prompt_tokens, context_window_tokens)
    {
        return Ok(());
    }

    let compaction_request = CompactionRequest::automatic_mid_turn()
        .with_additional_prompt_tokens(additional_prompt_tokens);
    let compaction = maybe_compact_context_with_turn_timeout(
        state,
        emit,
        &compaction_request,
        cancel,
        tokio::time::Duration::from_secs(COMPACTION_TIMEOUT_SECS),
    )
    .await;
    match compaction.result {
        Ok(CompactionOutcome::Applied(outcome)) => {
            state
                .machine
                .record_auto_mid_turn_compaction(outcome.output_prompt_tokens);
        }
        Ok(CompactionOutcome::Skipped(_)) => {}
        Ok(CompactionOutcome::Failed(_)) => {}
        Err(e) if compaction.timed_out => {
            debug!(error = %e, "Mid-turn context compaction stopped after timeout cancellation");
        }
        Err(e) => {
            warn!(error = %e, "Mid-turn context compaction failed");
        }
    }
    if compaction.timed_out {
        warn!("Mid-turn context compaction timeout - continuing without compaction");
    }

    Ok(())
}
