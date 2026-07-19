mod context;
mod pressure;

use alan_agent_protocol::{
    AppliedCompactionOutcome, CompactionAttemptSnapshot, CompactionMode, CompactionOutcome,
    CompactionPressureLevel, CompactionReason, CompactionRequestMetadata, CompactionResult,
    CompactionSkipReason, CompactionTrigger, Event, FailedCompactionOutcome,
    MemoryFlushAttemptSnapshot, MemoryFlushResult, SkippedCompactionOutcome,
};
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{
    agent_machine::AgentMachine, llm::build_generation_request, prompts, rollout::CompactedItem,
};

use super::{
    memory_flush,
    transition::{NamespaceAgentFiles, RuntimeLoopState},
};

#[cfg(test)]
pub(crate) use self::context::{
    COMPACTION_TOOL_OUTPUT_CHAR_LIMIT, DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS,
    DEGRADED_COMPACTION_SUMMARY_MAX_CHARS, sanitize_tool_text_for_compaction,
};
pub(crate) use self::context::{
    build_degraded_compaction_summary, sanitize_messages_for_compaction,
};
use self::pressure::{CompactionPressure, evaluate_compaction_pressure};

#[derive(Debug, Clone)]
pub(crate) struct CompactionRequest {
    mode: CompactionMode,
    trigger: CompactionTrigger,
    reason: CompactionReason,
    focus: Option<String>,
    additional_prompt_tokens: usize,
}

impl CompactionRequest {
    pub(crate) fn manual(focus: Option<String>) -> Self {
        Self {
            mode: CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: normalize_compaction_focus(focus),
            additional_prompt_tokens: 0,
        }
    }

    pub(crate) fn automatic_pre_turn() -> Self {
        Self {
            mode: CompactionMode::AutoPreTurn,
            trigger: CompactionTrigger::Auto,
            reason: CompactionReason::WindowPressure,
            focus: None,
            additional_prompt_tokens: 0,
        }
    }

    pub(crate) fn automatic_mid_turn() -> Self {
        Self {
            mode: CompactionMode::AutoMidTurn,
            trigger: CompactionTrigger::Auto,
            reason: CompactionReason::ContinuationPressure,
            focus: None,
            additional_prompt_tokens: 0,
        }
    }

    pub(crate) fn with_additional_prompt_tokens(mut self, additional_prompt_tokens: usize) -> Self {
        self.additional_prompt_tokens = additional_prompt_tokens;
        self
    }

    pub(crate) fn mode(&self) -> CompactionMode {
        self.mode
    }

    pub(crate) fn trigger(&self) -> CompactionTrigger {
        self.trigger
    }

    pub(crate) fn reason(&self) -> CompactionReason {
        self.reason
    }

    pub(crate) fn focus(&self) -> Option<&str> {
        self.focus.as_deref()
    }

    pub(crate) fn additional_prompt_tokens(&self) -> usize {
        self.additional_prompt_tokens
    }

    pub(crate) fn metadata(&self) -> CompactionRequestMetadata {
        CompactionRequestMetadata {
            mode: self.mode,
            trigger: self.trigger,
            reason: self.reason,
            focus: self.focus.clone(),
        }
    }
}

fn normalize_compaction_focus(focus: Option<String>) -> Option<String> {
    focus.and_then(|focus| {
        let trimmed = focus.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn compaction_warning_message(
    result: CompactionResult,
    error: &str,
    retry_count: u32,
    failure_streak: u32,
) -> String {
    let mut message = match result {
        CompactionResult::Degraded => format!(
            "Context compaction degraded after {retry_count} retry attempt(s): {error}. Used deterministic fallback summary."
        ),
        CompactionResult::Failure => format!(
            "Context compaction failed after {retry_count} retry attempt(s): {error}. Preserving existing context."
        ),
        _ => format!("Context compaction result {result:?}: {error}"),
    };

    if failure_streak >= 2 {
        message.push_str(
            " Repeated compaction degradation/failure detected; consider starting a new machine.",
        );
    }

    message
}

fn compaction_success_result(trimmed_count: usize) -> CompactionResult {
    if trimmed_count > 0 {
        CompactionResult::Retry
    } else {
        CompactionResult::Success
    }
}

struct CompactionFailureContext<'a> {
    request: &'a CompactionRequest,
    sanitized_to_summarize: &'a [crate::tape::Message],
    keep_last: usize,
    input_prompt_tokens: usize,
    pressure_level: Option<CompactionPressureLevel>,
    memory_flush_attempt_id: Option<String>,
    retry_count: u32,
    error_message: String,
    started_at: std::time::Instant,
}

fn skipped_outcome(
    request: &CompactionRequest,
    input_prompt_tokens: usize,
    reason: CompactionSkipReason,
) -> CompactionOutcome {
    CompactionOutcome::Skipped(SkippedCompactionOutcome {
        request: request.metadata(),
        input_prompt_tokens,
        reason,
    })
}

fn applied_outcome(
    request: &CompactionRequest,
    input_prompt_tokens: usize,
    output_prompt_tokens: usize,
    retry_count: u32,
    result: CompactionResult,
) -> CompactionOutcome {
    CompactionOutcome::Applied(AppliedCompactionOutcome {
        request: request.metadata(),
        input_prompt_tokens,
        output_prompt_tokens,
        retry_count,
        result,
    })
}

fn failed_outcome(
    request: &CompactionRequest,
    input_prompt_tokens: usize,
    retry_count: u32,
) -> CompactionOutcome {
    CompactionOutcome::Failed(FailedCompactionOutcome {
        request: request.metadata(),
        input_prompt_tokens,
        retry_count,
        result: CompactionResult::Failure,
    })
}

fn duration_ms_since(started_at: std::time::Instant) -> u64 {
    started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

struct CompactionAttemptDetails {
    result: CompactionResult,
    pressure_level: Option<CompactionPressureLevel>,
    memory_flush_attempt_id: Option<String>,
    input_messages: Option<usize>,
    output_messages: Option<usize>,
    input_prompt_tokens: Option<usize>,
    output_prompt_tokens: Option<usize>,
    retry_count: u32,
    tape_mutated: bool,
    warning_message: Option<String>,
    error_message: Option<String>,
    failure_streak: Option<u32>,
    reference_context_revision_before: Option<u64>,
    reference_context_revision_after: Option<u64>,
    timestamp: String,
}

fn build_compaction_attempt_snapshot(
    attempt_id: String,
    submission_id: Option<String>,
    request: &CompactionRequest,
    details: CompactionAttemptDetails,
) -> CompactionAttemptSnapshot {
    let CompactionAttemptDetails {
        result,
        pressure_level,
        memory_flush_attempt_id,
        input_messages,
        output_messages,
        input_prompt_tokens,
        output_prompt_tokens,
        retry_count,
        tape_mutated,
        warning_message,
        error_message,
        failure_streak,
        reference_context_revision_before,
        reference_context_revision_after,
        timestamp,
    } = details;

    CompactionAttemptSnapshot {
        attempt_id,
        submission_id,
        request: request.metadata(),
        result,
        pressure_level,
        memory_flush_attempt_id,
        input_messages,
        output_messages,
        input_prompt_tokens,
        output_prompt_tokens,
        retry_count,
        tape_mutated,
        warning_message,
        error_message,
        failure_streak,
        reference_context_revision_before,
        reference_context_revision_after,
        timestamp,
    }
}

fn compaction_submission_id(machine: &AgentMachine, request: &CompactionRequest) -> Option<String> {
    matches!(request.mode(), CompactionMode::Manual)
        .then(|| machine.current_submission_id().map(str::to_owned))
        .flatten()
}

async fn record_and_emit_compaction_attempt<E, F>(
    machine: &mut AgentMachine,
    agent_files: &NamespaceAgentFiles,
    emit: &mut E,
    attempt: CompactionAttemptSnapshot,
    compacted: Option<CompactedItem>,
) where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if let Err(err) = machine
        .persist_compaction_observation(attempt.clone(), compacted)
        .await
    {
        error!(error = %err, "Failed to persist compaction observation batch");
        return;
    }
    if let Err(err) = super::ui_surfaces::compaction(agent_files, &attempt).await {
        error!(error = %err, "Failed to write compaction UI state");
    }
    emit(Event::CompactionObserved { attempt }).await;
}

async fn record_and_emit_memory_flush_attempt<E, F>(
    machine: &mut AgentMachine,
    agent_files: &NamespaceAgentFiles,
    emit: &mut E,
    attempt: MemoryFlushAttemptSnapshot,
) -> Option<String>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if let Err(err) = machine.persist_memory_flush_attempt(attempt.clone()).await {
        error!(error = %err, "Failed to persist memory flush attempt");
        return None;
    }
    let attempt_id = attempt.attempt_id.clone();
    if let Err(err) = super::ui_surfaces::memory_flush(agent_files, &attempt).await {
        error!(error = %err, "Failed to write memory flush UI state");
    }
    emit(Event::MemoryFlushObserved { attempt }).await;
    Some(attempt_id)
}

async fn maybe_flush_memory_before_compaction<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    request: &CompactionRequest,
    pressure: CompactionPressure,
    sanitized_to_summarize: &[crate::tape::Message],
    cancel: &CancellationToken,
) -> Option<String>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let agent_files = state.agent_files();
    if !matches!(request.mode(), CompactionMode::AutoPreTurn)
        || !matches!(pressure.level, CompactionPressureLevel::Soft)
    {
        return None;
    }

    if state.machine.auto_memory_flush_attempted_in_cycle() {
        let attempt = memory_flush::skipped_memory_flush_attempt(
            request.mode(),
            pressure.level,
            alan_agent_protocol::MemoryFlushSkipReason::AlreadyFlushedThisCycle,
            Some(sanitized_to_summarize.len()),
        );
        return record_and_emit_memory_flush_attempt(
            &mut state.machine,
            &agent_files,
            emit,
            attempt,
        )
        .await;
    }

    let generation = state.namespace_generation();
    let process_path = state.process_path();
    let attempt = memory_flush::perform_memory_flush_attempt(
        memory_flush::MemoryFlushInputs::new(
            &state.machine,
            &generation,
            state.core_config.memory.enabled,
            state.core_config.memory.store_dir.as_deref(),
            &process_path,
        ),
        request.mode(),
        pressure.level,
        sanitized_to_summarize,
        cancel,
    )
    .await;

    if !matches!(
        (attempt.result, attempt.skip_reason),
        (
            MemoryFlushResult::Skipped,
            Some(alan_agent_protocol::MemoryFlushSkipReason::Cancelled)
        )
    ) {
        state.machine.note_auto_memory_flush_attempt();
    }

    if let Some(message) = attempt.warning_message.clone() {
        if let Err(err) = super::ui_surfaces::warning(&agent_files, message.clone()).await {
            error!(error = %err, "Failed to write memory warning UI state");
        }
        emit(Event::Warning { message }).await;
    }

    record_and_emit_memory_flush_attempt(&mut state.machine, &agent_files, emit, attempt).await
}

async fn handle_compaction_generation_failure<E, F>(
    machine: &mut AgentMachine,
    agent_files: &NamespaceAgentFiles,
    emit: &mut E,
    failure: CompactionFailureContext<'_>,
) -> Result<CompactionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let CompactionFailureContext {
        request,
        sanitized_to_summarize,
        keep_last,
        input_prompt_tokens,
        pressure_level,
        memory_flush_attempt_id,
        retry_count,
        error_message,
        started_at,
    } = failure;
    let reference_context_revision = machine.context_revision();

    if let Some(summary) =
        build_degraded_compaction_summary(sanitized_to_summarize, machine.tape_summary())
    {
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let failure_streak = machine.note_compaction_failure();
        let warning_message = compaction_warning_message(
            CompactionResult::Degraded,
            &error_message,
            retry_count,
            failure_streak,
        );
        super::ui_surfaces::warning(agent_files, warning_message.clone()).await?;
        emit(Event::Warning {
            message: warning_message.clone(),
        })
        .await;

        let retention_start = machine.compaction_retention_start(keep_last);
        apply_tape_compaction(machine, &summary, keep_last, retention_start);
        machine.clear_responses_continuation("compaction");
        let output_prompt_tokens = machine.estimated_prompt_tokens();
        let output_messages = machine.tape_len();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let duration_ms = duration_ms_since(started_at);
        let attempt = build_compaction_attempt_snapshot(
            attempt_id.clone(),
            compaction_submission_id(machine, request),
            request,
            CompactionAttemptDetails {
                result: CompactionResult::Degraded,
                pressure_level,
                memory_flush_attempt_id: memory_flush_attempt_id.clone(),
                input_messages: Some(sanitized_to_summarize.len()),
                output_messages: Some(output_messages),
                input_prompt_tokens: Some(input_prompt_tokens),
                output_prompt_tokens: Some(output_prompt_tokens),
                retry_count,
                tape_mutated: true,
                warning_message: Some(warning_message),
                error_message: Some(error_message),
                failure_streak: Some(failure_streak),
                reference_context_revision_before: Some(reference_context_revision),
                reference_context_revision_after: Some(machine.context_revision()),
                timestamp: timestamp.clone(),
            },
        );
        let compacted = CompactedItem {
            message: summary,
            attempt_id: Some(attempt_id),
            trigger: Some(request.trigger()),
            reason: Some(request.reason()),
            focus: request.focus().map(str::to_string),
            input_messages: Some(sanitized_to_summarize.len()),
            output_messages: Some(output_messages),
            input_tokens: Some(input_prompt_tokens),
            output_tokens: Some(output_prompt_tokens),
            duration_ms: Some(duration_ms),
            retry_count: Some(retry_count),
            result: Some(CompactionResult::Degraded),
            reference_context_revision: Some(reference_context_revision),
            timestamp,
        };
        record_and_emit_compaction_attempt(machine, agent_files, emit, attempt, Some(compacted))
            .await;

        return Ok(applied_outcome(
            request,
            input_prompt_tokens,
            output_prompt_tokens,
            retry_count,
            CompactionResult::Degraded,
        ));
    }

    let failure_streak = machine.note_compaction_failure();
    let warning_message = compaction_warning_message(
        CompactionResult::Failure,
        &error_message,
        retry_count,
        failure_streak,
    );
    super::ui_surfaces::warning(agent_files, warning_message.clone()).await?;
    emit(Event::Warning {
        message: warning_message.clone(),
    })
    .await;
    let attempt = build_compaction_attempt_snapshot(
        uuid::Uuid::new_v4().to_string(),
        compaction_submission_id(machine, request),
        request,
        CompactionAttemptDetails {
            result: CompactionResult::Failure,
            pressure_level,
            memory_flush_attempt_id,
            input_messages: Some(sanitized_to_summarize.len()),
            output_messages: None,
            input_prompt_tokens: Some(input_prompt_tokens),
            output_prompt_tokens: None,
            retry_count,
            tape_mutated: false,
            warning_message: Some(warning_message),
            error_message: Some(error_message),
            failure_streak: Some(failure_streak),
            reference_context_revision_before: Some(reference_context_revision),
            reference_context_revision_after: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );
    record_and_emit_compaction_attempt(machine, agent_files, emit, attempt, None).await;

    Ok(failed_outcome(request, input_prompt_tokens, retry_count))
}

fn apply_tape_compaction(
    machine: &mut AgentMachine,
    summary: &str,
    keep_last: usize,
    retention_start: usize,
) {
    machine.compact_tape(summary.to_string(), keep_last);
    machine.note_tape_compaction(retention_start);
}

pub(crate) async fn maybe_compact_context_for_request<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    request: CompactionRequest,
) -> Result<CompactionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let cancel = CancellationToken::new();
    maybe_compact_context_with_cancel(state, emit, &request, &cancel).await
}

pub(crate) async fn maybe_compact_context_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    request: &CompactionRequest,
    cancel: &CancellationToken,
) -> Result<CompactionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let keep_last = state.runtime_config.compaction_keep_last;
    let message_count = state.machine.tape_len();
    let estimated_prompt_tokens = state
        .machine
        .estimated_prompt_tokens()
        .saturating_add(request.additional_prompt_tokens());
    let pressure = evaluate_compaction_pressure(
        &state.runtime_config,
        request,
        message_count,
        estimated_prompt_tokens,
    );
    let compaction_pressure_level =
        (!matches!(request.mode(), CompactionMode::Manual)).then_some(pressure.level);

    if matches!(request.mode(), CompactionMode::AutoPreTurn)
        && matches!(pressure.level, CompactionPressureLevel::BelowSoft)
    {
        state.machine.reset_auto_memory_flush_cycle();
    }

    if !matches!(request.mode(), CompactionMode::Manual)
        && matches!(pressure.level, CompactionPressureLevel::BelowSoft)
    {
        return Ok(skipped_outcome(
            request,
            estimated_prompt_tokens,
            CompactionSkipReason::UnderThreshold,
        ));
    }

    let messages = state.machine.messages().to_vec();
    let retention_start = state.machine.compaction_retention_start(keep_last);
    let to_summarize = messages[..retention_start].to_vec();

    if to_summarize.is_empty() {
        return Ok(skipped_outcome(
            request,
            estimated_prompt_tokens,
            CompactionSkipReason::EmptySummarizeRegion,
        ));
    }

    let compaction_count = state.machine.compaction_count();
    let sanitized_to_summarize = sanitize_messages_for_compaction(&to_summarize);
    let memory_flush_attempt_id = maybe_flush_memory_before_compaction(
        state,
        emit,
        request,
        pressure,
        &sanitized_to_summarize,
        cancel,
    )
    .await;

    if cancel.is_cancelled() {
        return Ok(skipped_outcome(
            request,
            estimated_prompt_tokens,
            CompactionSkipReason::Cancelled,
        ));
    }

    info!(
        total_messages = message_count,
        estimated_prompt_tokens,
        context_window_tokens = state.runtime_config.context_window_tokens,
        context_window_utilization = pressure.context_window_utilization,
        compaction_pressure_level = ?pressure.level,
        compaction_soft_trigger_ratio = pressure.soft_trigger_ratio,
        compaction_hard_trigger_ratio = pressure.hard_trigger_ratio,
        soft_token_trigger_threshold = pressure.soft_token_trigger_threshold,
        hard_token_trigger_threshold = pressure.hard_token_trigger_threshold,
        over_message_threshold = pressure.over_message_threshold,
        emergency_mid_turn_compaction = pressure.emergency_mid_turn_compaction,
        memory_flush_attempt_id = ?memory_flush_attempt_id,
        summarize = to_summarize.len(),
        keep_last,
        compaction_count,
        compaction_mode = ?request.mode(),
        "Compacting conversation history"
    );

    let started_at = std::time::Instant::now();
    let mut llm_messages = Vec::new();

    if let Some(existing_summary) = state.machine.tape_summary() {
        llm_messages.push(crate::llm::Message::context(format!(
            "[Previous compaction summary (compaction #{})]\n{}",
            compaction_count, existing_summary
        )));
    }

    if let Some(focus) = request.focus() {
        llm_messages.push(crate::llm::Message::context(format!(
            "[Compaction focus]\nPreserve and emphasize: {focus}"
        )));
    }

    llm_messages.extend(crate::llm::project_messages(&sanitized_to_summarize, true));

    let max_trim_retries = 5;
    let mut trimmed_count = 0usize;
    let agent_files = state.agent_files();
    let summary = loop {
        let generation_request = build_generation_request(
            Some(prompts::COMPACT_PROMPT.to_string()),
            llm_messages.clone(),
            Vec::new(),
            Some(0.2),
            Some(2048),
        );

        match state
            .namespace_generation()
            .generate_once_with_cancel(generation_request, cancel, "Compaction cancelled")
            .await
        {
            Ok(resp) => {
                let text = resp.content.trim().to_string();
                if trimmed_count > 0 {
                    info!(
                        trimmed_count,
                        "Trimmed oldest messages from compaction input to fit context window"
                    );
                }
                break text;
            }
            Err(err) => {
                if cancel.is_cancelled() {
                    return Ok(skipped_outcome(
                        request,
                        estimated_prompt_tokens,
                        CompactionSkipReason::Cancelled,
                    ));
                }

                let removable_count = llm_messages
                    .iter()
                    .filter(|m| !matches!(m.role, crate::llm::MessageRole::Context))
                    .count();

                if trimmed_count < max_trim_retries
                    && removable_count > 1
                    && let Some(idx) = llm_messages
                        .iter()
                        .position(|m| !matches!(m.role, crate::llm::MessageRole::Context))
                {
                    llm_messages.remove(idx);
                    trimmed_count += 1;
                    warn!(
                        error = %err,
                        trimmed_count,
                        remaining = llm_messages.len(),
                        "Compaction failed, trimming oldest message and retrying"
                    );
                    continue;
                }

                warn!(error = %err, "Failed to generate compaction summary after retries");
                return handle_compaction_generation_failure(
                    &mut state.machine,
                    &agent_files,
                    emit,
                    CompactionFailureContext {
                        request,
                        sanitized_to_summarize: &sanitized_to_summarize,
                        keep_last,
                        input_prompt_tokens: estimated_prompt_tokens,
                        pressure_level: compaction_pressure_level,
                        memory_flush_attempt_id: memory_flush_attempt_id.clone(),
                        retry_count: trimmed_count as u32,
                        error_message: err.to_string(),
                        started_at,
                    },
                )
                .await;
            }
        }
    };

    if summary.is_empty() {
        return handle_compaction_generation_failure(
            &mut state.machine,
            &agent_files,
            emit,
            CompactionFailureContext {
                request,
                sanitized_to_summarize: &sanitized_to_summarize,
                keep_last,
                input_prompt_tokens: estimated_prompt_tokens,
                pressure_level: compaction_pressure_level,
                memory_flush_attempt_id: memory_flush_attempt_id.clone(),
                retry_count: trimmed_count as u32,
                error_message: "compaction summary was empty".to_string(),
                started_at,
            },
        )
        .await;
    }

    let input_prompt_tokens = estimated_prompt_tokens;
    let success_result = compaction_success_result(trimmed_count);
    let reference_context_revision = state.machine.context_revision();
    let attempt_id = uuid::Uuid::new_v4().to_string();
    apply_tape_compaction(&mut state.machine, &summary, keep_last, retention_start);
    state.machine.clear_responses_continuation("compaction");
    let output_prompt_tokens = state
        .machine
        .estimated_prompt_tokens()
        .saturating_add(request.additional_prompt_tokens());
    let output_messages = state.machine.tape_len();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let duration_ms = duration_ms_since(started_at);
    state.machine.reset_compaction_failure_streak();
    let attempt = build_compaction_attempt_snapshot(
        attempt_id.clone(),
        compaction_submission_id(&state.machine, request),
        request,
        CompactionAttemptDetails {
            result: success_result,
            pressure_level: compaction_pressure_level,
            memory_flush_attempt_id,
            input_messages: Some(to_summarize.len()),
            output_messages: Some(output_messages),
            input_prompt_tokens: Some(input_prompt_tokens),
            output_prompt_tokens: Some(output_prompt_tokens),
            retry_count: trimmed_count as u32,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(reference_context_revision),
            reference_context_revision_after: Some(state.machine.context_revision()),
            timestamp: timestamp.clone(),
        },
    );
    let compacted = CompactedItem {
        message: summary,
        attempt_id: Some(attempt_id),
        trigger: Some(request.trigger()),
        reason: Some(request.reason()),
        focus: request.focus().map(str::to_string),
        input_messages: Some(to_summarize.len()),
        output_messages: Some(output_messages),
        input_tokens: Some(input_prompt_tokens),
        output_tokens: Some(output_prompt_tokens),
        duration_ms: Some(duration_ms),
        retry_count: Some(trimmed_count as u32),
        result: Some(success_result),
        reference_context_revision: Some(reference_context_revision),
        timestamp,
    };
    record_and_emit_compaction_attempt(
        &mut state.machine,
        &agent_files,
        emit,
        attempt,
        Some(compacted),
    )
    .await;

    Ok(applied_outcome(
        request,
        input_prompt_tokens,
        output_prompt_tokens,
        trimmed_count as u32,
        success_result,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use alan_agent_protocol::Event;
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};
    use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
    use alan_llmfs::LlmFs;
    use tokio::sync::mpsc;

    use crate::{
        agent_machine::AgentMachine,
        config::Config,
        runtime::{
            NamespaceRuntimeEnvironment, RuntimeConfig, prompt_cache::PromptAssemblyCache,
            transition::RuntimeLoopState,
        },
    };

    struct RecordingProvider {
        requests: Arc<Mutex<Vec<GenerationRequest>>>,
        response: String,
    }

    #[async_trait::async_trait]
    impl LlmProvider for RecordingProvider {
        async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
            unimplemented!()
        }

        async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
            unimplemented!()
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
            self.requests.lock().unwrap().push(request);
            let (tx, rx) = mpsc::channel(4);
            let response = self.response.clone();
            tokio::spawn(async move {
                let _ = tx
                    .send(StreamChunk {
                        text: Some(response),
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some("stop".to_string()),
                    })
                    .await;
            });
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "recording"
        }
    }

    fn namespace_state_with_provider(provider: impl LlmProvider + 'static) -> RuntimeLoopState {
        let llmfs = Arc::new(LlmFs::new());
        llmfs.register_connection("default", Box::new(provider));

        let mut namespace = Namespace::new();
        namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));

        RuntimeLoopState {
            machine: AgentMachine::new(),
            environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            core_config: Config::default(),
            runtime_config: RuntimeConfig {
                compaction_trigger_messages: 1,
                compaction_keep_last: 1,
                ..RuntimeConfig::default()
            },
            definition_persona_dirs: Vec::new(),
            prompt_cache: PromptAssemblyCache::new(Vec::new()),
        }
    }

    #[tokio::test]
    async fn compaction_generation_uses_namespace_llmfs() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut state = namespace_state_with_provider(RecordingProvider {
            requests: Arc::clone(&requests),
            response: "namespace compaction summary".to_string(),
        });
        state.core_config.memory.enabled = false;
        state.machine.add_user_message("first detail to compact");
        state.machine.add_assistant_message("first answer", None);
        state.machine.add_user_message("second detail to compact");
        state.machine.add_assistant_message("second answer", None);

        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = maybe_compact_context_with_cancel(
            &mut state,
            &mut emit,
            &CompactionRequest::manual(None),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, CompactionOutcome::Applied { .. }));
        assert_eq!(
            state.machine.tape_summary(),
            Some("namespace compaction summary")
        );
        let recorded = requests.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(
            recorded[0].system_prompt.as_deref(),
            Some(prompts::COMPACT_PROMPT)
        );
        assert!(
            recorded[0]
                .messages
                .iter()
                .any(|message| message.content.contains("first detail to compact"))
        );
    }
}
