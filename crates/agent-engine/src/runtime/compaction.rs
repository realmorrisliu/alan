use alan_agent_protocol::{
    AppliedCompactionOutcome, CompactionAttemptSnapshot, CompactionMode, CompactionOutcome,
    CompactionPressureLevel, CompactionReason, CompactionRequestMetadata, CompactionResult,
    CompactionSkipReason, CompactionTrigger, Event, FailedCompactionOutcome,
    MemoryFlushAttemptSnapshot, MemoryFlushResult, SkippedCompactionOutcome,
};
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{llm::build_generation_request, prompts, rollout::CompactedItem};

use super::{agent_loop::RuntimeLoopState, memory_flush};

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

pub(crate) const COMPACTION_TOOL_OUTPUT_CHAR_LIMIT: usize = 4_000;
const COMPACTION_TOOL_OUTPUT_HEAD_LINES: usize = 12;
const COMPACTION_TOOL_OUTPUT_TAIL_LINES: usize = 12;
const COMPACTION_TOOL_OUTPUT_IDENTIFIER_LINES: usize = 24;
const COMPACTION_TOOL_OUTPUT_INLINE_LINE_LIMIT: usize = 80;
const COMPACTION_TOOL_OUTPUT_RENDER_LINE_MAX_CHARS: usize = 240;
const COMPACTION_TOOL_OUTPUT_RENDER_LINE_MIN_CHARS: usize = 32;
const DEGRADED_COMPACTION_SNIPPET_CHARS: usize = 240;
const DEGRADED_COMPACTION_SUMMARY_MESSAGES: usize = 6;
pub(crate) const DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS: usize = 800;
pub(crate) const DEGRADED_COMPACTION_SUMMARY_MAX_CHARS: usize = 2_400;

#[derive(Debug, Clone, Copy)]
struct CompactionPressure {
    level: CompactionPressureLevel,
    soft_trigger_ratio: f32,
    hard_trigger_ratio: f32,
    soft_token_trigger_threshold: usize,
    hard_token_trigger_threshold: usize,
    context_window_utilization: f64,
    over_message_threshold: bool,
    emergency_mid_turn_compaction: bool,
}

fn derived_soft_trigger_ratio(hard_trigger_ratio: f32) -> f32 {
    hard_trigger_ratio * 0.9
}

fn effective_hard_trigger_ratio(runtime_config: &super::RuntimeConfig) -> f32 {
    runtime_config.compaction_hard_trigger_ratio.clamp(0.0, 1.0)
}

fn effective_soft_trigger_ratio(
    runtime_config: &super::RuntimeConfig,
    hard_trigger_ratio: f32,
) -> f32 {
    let soft_trigger_ratio = runtime_config.compaction_soft_trigger_ratio.clamp(0.0, 1.0);
    if soft_trigger_ratio < hard_trigger_ratio {
        soft_trigger_ratio
    } else {
        derived_soft_trigger_ratio(hard_trigger_ratio)
    }
}

fn token_trigger_threshold(context_window_tokens: usize, ratio: f32) -> usize {
    if context_window_tokens == 0 {
        0
    } else {
        ((context_window_tokens as f64) * (ratio as f64)).ceil() as usize
    }
}

fn evaluate_compaction_pressure(
    runtime_config: &super::RuntimeConfig,
    request: &CompactionRequest,
    message_count: usize,
    estimated_prompt_tokens: usize,
) -> CompactionPressure {
    let context_window_tokens = runtime_config.context_window_tokens as usize;
    let hard_trigger_ratio = effective_hard_trigger_ratio(runtime_config);
    let soft_trigger_ratio = effective_soft_trigger_ratio(runtime_config, hard_trigger_ratio);
    let soft_token_trigger_threshold =
        token_trigger_threshold(context_window_tokens, soft_trigger_ratio);
    let hard_token_trigger_threshold =
        token_trigger_threshold(context_window_tokens, hard_trigger_ratio);
    let context_window_utilization = if context_window_tokens == 0 {
        0.0
    } else {
        estimated_prompt_tokens as f64 / context_window_tokens as f64
    };
    let over_message_threshold = message_count > runtime_config.compaction_trigger_messages;
    let over_hard_token_threshold =
        context_window_tokens > 0 && estimated_prompt_tokens >= hard_token_trigger_threshold;
    let over_soft_token_threshold =
        context_window_tokens > 0 && estimated_prompt_tokens >= soft_token_trigger_threshold;
    let emergency_mid_turn_compaction = matches!(request.mode(), CompactionMode::AutoMidTurn)
        && super::turn_state::is_auto_mid_turn_compaction_emergency(
            estimated_prompt_tokens,
            context_window_tokens,
        );
    let level = match request.mode() {
        CompactionMode::Manual => CompactionPressureLevel::Hard,
        CompactionMode::AutoMidTurn => {
            if emergency_mid_turn_compaction || over_message_threshold || over_hard_token_threshold
            {
                CompactionPressureLevel::Hard
            } else {
                CompactionPressureLevel::BelowSoft
            }
        }
        CompactionMode::AutoPreTurn => {
            if emergency_mid_turn_compaction || over_message_threshold || over_hard_token_threshold
            {
                CompactionPressureLevel::Hard
            } else if over_soft_token_threshold {
                CompactionPressureLevel::Soft
            } else {
                CompactionPressureLevel::BelowSoft
            }
        }
    };

    CompactionPressure {
        level,
        soft_trigger_ratio,
        hard_trigger_ratio,
        soft_token_trigger_threshold,
        hard_token_trigger_threshold,
        context_window_utilization,
        over_message_threshold,
        emergency_mid_turn_compaction,
    }
}

pub(crate) fn sanitize_messages_for_compaction(
    messages: &[crate::tape::Message],
) -> Vec<crate::tape::Message> {
    messages
        .iter()
        .map(sanitize_message_for_compaction)
        .collect()
}

fn sanitize_message_for_compaction(message: &crate::tape::Message) -> crate::tape::Message {
    match message {
        crate::tape::Message::Tool { responses } => crate::tape::Message::tool_multi(
            responses
                .iter()
                .map(sanitize_tool_response_for_compaction)
                .collect(),
        ),
        _ => message.clone(),
    }
}

fn sanitize_tool_response_for_compaction(
    response: &crate::tape::ToolResponse,
) -> crate::tape::ToolResponse {
    let text = response.text_content();
    if text.chars().count() <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT
        && text.lines().count() <= COMPACTION_TOOL_OUTPUT_INLINE_LINE_LIMIT
    {
        return response.clone();
    }

    crate::tape::ToolResponse::text(
        response.id.clone(),
        sanitize_tool_text_for_compaction(&text),
    )
}

pub(crate) fn sanitize_tool_text_for_compaction(text: &str) -> String {
    let line_count = text.lines().count();
    let char_count = text.chars().count();
    if char_count <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT
        && line_count <= COMPACTION_TOOL_OUTPUT_INLINE_LINE_LIMIT
    {
        return text.to_string();
    }

    let lines: Vec<&str> = text.lines().collect();
    let mut keep = std::collections::BTreeSet::new();
    let mut critical_lines = std::collections::BTreeSet::new();
    let tail_start = lines
        .len()
        .saturating_sub(COMPACTION_TOOL_OUTPUT_TAIL_LINES);

    for idx in 0..lines.len().min(COMPACTION_TOOL_OUTPUT_HEAD_LINES) {
        keep.insert(idx);
    }
    for idx in tail_start..lines.len() {
        keep.insert(idx);
    }

    let mut identifier_lines = 0usize;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || line_looks_like_compaction_noise(trimmed) {
            continue;
        }
        if line_contains_critical_identifier(trimmed) {
            keep.insert(idx);
            critical_lines.insert(idx);
            identifier_lines += 1;
            if identifier_lines >= COMPACTION_TOOL_OUTPUT_IDENTIFIER_LINES {
                break;
            }
        }
    }

    let header = format!(
        "[tool output trimmed for compaction; original {line_count} lines / {char_count} chars]"
    );
    let required: std::collections::BTreeSet<usize> = keep
        .iter()
        .copied()
        .filter(|idx| *idx >= tail_start || critical_lines.contains(idx))
        .collect();
    let optional: Vec<usize> = keep
        .iter()
        .copied()
        .filter(|idx| !required.contains(idx))
        .collect();

    render_tool_output_with_cap(&header, &lines, &required, &optional)
}

fn line_contains_critical_identifier(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    line.contains('/')
        || line.contains('\\')
        || lower.contains("call_")
        || lower.contains("tool_call")
        || lower.contains("id=")
        || lower.contains("id:")
        || lower.contains("uuid")
        || lower.contains("sha256:")
        || lower.contains("sha1:")
        || lower.contains("path:")
        || lower.contains("command:")
        || looks_like_shell_command(&lower)
}

fn looks_like_shell_command(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("$ ")
        || [
            "cargo ", "git ", "just ", "bash ", "sh ", "npm ", "pnpm ", "bun ", "make ",
        ]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

fn line_looks_like_compaction_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("debug")
        || lower.starts_with("[debug]")
        || lower.starts_with("trace")
        || lower.starts_with("[trace]")
        || lower.contains(" debug ")
        || lower.contains(" trace ")
}

pub(crate) fn build_degraded_compaction_summary(
    messages: &[crate::tape::Message],
    existing_summary: Option<&str>,
) -> Option<String> {
    let bounded_existing_summary = existing_summary
        .filter(|summary| !summary.trim().is_empty())
        .map(|summary| truncate_compaction_text(summary, DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS));

    let mut sections = Vec::new();
    if let Some(summary) = bounded_existing_summary.as_deref() {
        sections.push("Prior summary excerpt:".to_string());
        sections.push(summary.to_string());
    }

    let snippets: Vec<String> = messages
        .iter()
        .filter_map(degraded_compaction_snippet)
        .rev()
        .take(DEGRADED_COMPACTION_SUMMARY_MESSAGES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if snippets.is_empty() {
        return bounded_existing_summary;
    }

    sections.push("Deterministic fallback summary after compaction failure:".to_string());
    sections.push("Recent preserved context:".to_string());
    sections.extend(snippets.into_iter().map(|snippet| format!("- {snippet}")));
    Some(truncate_compaction_text(
        &sections.join("\n"),
        DEGRADED_COMPACTION_SUMMARY_MAX_CHARS,
    ))
}

fn degraded_compaction_snippet(message: &crate::tape::Message) -> Option<String> {
    match message {
        crate::tape::Message::User { .. } => {
            let text = message.text_content();
            if text.trim().is_empty() {
                None
            } else {
                Some(format!(
                    "user: {}",
                    truncate_compaction_text(&text, DEGRADED_COMPACTION_SNIPPET_CHARS)
                ))
            }
        }
        crate::tape::Message::Assistant { .. } => {
            let text = message.non_thinking_text_content();
            if text.trim().is_empty() {
                None
            } else {
                Some(format!(
                    "assistant: {}",
                    truncate_compaction_text(&text, DEGRADED_COMPACTION_SNIPPET_CHARS)
                ))
            }
        }
        crate::tape::Message::Tool { responses } => {
            let tool_summaries: Vec<String> = responses
                .iter()
                .filter_map(|response| {
                    let text = sanitize_tool_text_for_compaction(&response.text_content());
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(format!(
                            "tool[{}]: {}",
                            response.id,
                            truncate_compaction_text(trimmed, DEGRADED_COMPACTION_SNIPPET_CHARS)
                        ))
                    }
                })
                .collect();
            if tool_summaries.is_empty() {
                None
            } else {
                Some(tool_summaries.join(" | "))
            }
        }
        crate::tape::Message::System { .. } | crate::tape::Message::Context { .. } => None,
    }
}

fn truncate_compaction_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    truncate_text_with_suffix(trimmed, max_chars, "...")
}

fn render_tool_output_with_cap(
    header: &str,
    lines: &[&str],
    required: &std::collections::BTreeSet<usize>,
    optional: &[usize],
) -> String {
    let mut line_limit = COMPACTION_TOOL_OUTPUT_RENDER_LINE_MAX_CHARS;
    let mut rendered = render_tool_output_selection(header, lines, required, line_limit);

    while rendered.chars().count() > COMPACTION_TOOL_OUTPUT_CHAR_LIMIT
        && line_limit > COMPACTION_TOOL_OUTPUT_RENDER_LINE_MIN_CHARS
    {
        line_limit = line_limit.saturating_sub(16);
        rendered = render_tool_output_selection(header, lines, required, line_limit);
    }

    let mut included = required.clone();
    for idx in optional {
        let mut candidate = included.clone();
        candidate.insert(*idx);
        let candidate_rendered =
            render_tool_output_selection(header, lines, &candidate, line_limit);
        if candidate_rendered.chars().count() <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT {
            included = candidate;
            rendered = candidate_rendered;
        }
    }

    rendered
}

fn render_tool_output_selection(
    header: &str,
    lines: &[&str],
    included: &std::collections::BTreeSet<usize>,
    line_limit: usize,
) -> String {
    let mut output = vec![header.to_string()];
    let mut previous = None;
    let mut truncated_line = false;

    for idx in included {
        if let Some(prev) = previous
            && *idx > prev + 1
        {
            output.push(format!("[... {} lines omitted ...]", idx - prev - 1));
        }

        let rendered_line = truncate_text_with_suffix(lines[*idx], line_limit, "...");
        truncated_line |= rendered_line.chars().count() < lines[*idx].chars().count();
        output.push(rendered_line);
        previous = Some(*idx);
    }

    if let Some(prev) = previous
        && prev + 1 < lines.len()
    {
        output.push(format!(
            "[... {} lines omitted ...]",
            lines.len() - prev - 1
        ));
    }

    if truncated_line {
        output.push("[truncated for compaction]".to_string());
    }

    output.join("\n")
}

fn truncate_text_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    if max_chars == 0 {
        return String::new();
    }

    let suffix_chars = suffix.chars().count();
    if suffix_chars >= max_chars {
        return suffix.chars().take(max_chars).collect();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(suffix_chars))
        .collect::<String>();
    truncated.push_str(suffix);
    truncated
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

fn compaction_submission_id(
    state: &RuntimeLoopState,
    request: &CompactionRequest,
) -> Option<String> {
    matches!(request.mode(), CompactionMode::Manual)
        .then(|| state.current_submission_id.clone())
        .flatten()
}

async fn record_and_emit_compaction_attempt<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    attempt: CompactionAttemptSnapshot,
    compacted: Option<CompactedItem>,
) where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if let Err(err) = state
        .machine
        .persist_compaction_observation(attempt.clone(), compacted)
        .await
    {
        error!(error = %err, "Failed to persist compaction observation batch");
        return;
    }
    if let Err(err) = super::ui_surfaces::compaction(state.namespace_environment(), &attempt).await
    {
        error!(error = %err, "Failed to write compaction UI state");
    }
    emit(Event::CompactionObserved { attempt }).await;
}

async fn record_and_emit_memory_flush_attempt<E, F>(
    state: &mut RuntimeLoopState,
    emit: &mut E,
    attempt: MemoryFlushAttemptSnapshot,
) -> Option<String>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if let Err(err) = state
        .machine
        .persist_memory_flush_attempt(attempt.clone())
        .await
    {
        error!(error = %err, "Failed to persist memory flush attempt");
        return None;
    }
    let attempt_id = attempt.attempt_id.clone();
    if let Err(err) =
        super::ui_surfaces::memory_flush(state.namespace_environment(), &attempt).await
    {
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
        return record_and_emit_memory_flush_attempt(state, emit, attempt).await;
    }

    let attempt = memory_flush::perform_memory_flush_attempt(
        state,
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
        if let Err(err) =
            super::ui_surfaces::warning(state.namespace_environment(), message.clone()).await
        {
            error!(error = %err, "Failed to write memory warning UI state");
        }
        emit(Event::Warning { message }).await;
    }

    record_and_emit_memory_flush_attempt(state, emit, attempt).await
}

async fn handle_compaction_generation_failure<E, F>(
    state: &mut RuntimeLoopState,
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
    let reference_context_revision = state.machine.tape.context_revision();

    if let Some(summary) =
        build_degraded_compaction_summary(sanitized_to_summarize, state.machine.tape.summary())
    {
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let failure_streak = state.machine.note_compaction_failure();
        let warning_message = compaction_warning_message(
            CompactionResult::Degraded,
            &error_message,
            retry_count,
            failure_streak,
        );
        super::ui_surfaces::warning(state.namespace_environment(), warning_message.clone()).await?;
        emit(Event::Warning {
            message: warning_message.clone(),
        })
        .await;

        let retention_start = state.machine.tape.compaction_retention_start(keep_last);
        apply_tape_compaction(state, &summary, keep_last, retention_start);
        state.machine.clear_responses_continuation("compaction");
        let output_prompt_tokens = state.machine.tape.estimated_prompt_tokens();
        let output_messages = state.machine.tape.len();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let duration_ms = duration_ms_since(started_at);
        let attempt = build_compaction_attempt_snapshot(
            attempt_id.clone(),
            compaction_submission_id(state, request),
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
                reference_context_revision_after: Some(state.machine.tape.context_revision()),
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
        record_and_emit_compaction_attempt(state, emit, attempt, Some(compacted)).await;

        return Ok(applied_outcome(
            request,
            input_prompt_tokens,
            output_prompt_tokens,
            retry_count,
            CompactionResult::Degraded,
        ));
    }

    let failure_streak = state.machine.note_compaction_failure();
    let warning_message = compaction_warning_message(
        CompactionResult::Failure,
        &error_message,
        retry_count,
        failure_streak,
    );
    super::ui_surfaces::warning(state.namespace_environment(), warning_message.clone()).await?;
    emit(Event::Warning {
        message: warning_message.clone(),
    })
    .await;
    let attempt = build_compaction_attempt_snapshot(
        uuid::Uuid::new_v4().to_string(),
        compaction_submission_id(state, request),
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
    record_and_emit_compaction_attempt(state, emit, attempt, None).await;

    Ok(failed_outcome(request, input_prompt_tokens, retry_count))
}

fn apply_tape_compaction(
    state: &mut RuntimeLoopState,
    summary: &str,
    keep_last: usize,
    retention_start: usize,
) {
    state.machine.tape.compact(summary.to_string(), keep_last);
    state.turn_state.note_tape_compaction(retention_start);
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
    let message_count = state.machine.tape.len();
    let estimated_prompt_tokens = state
        .machine
        .tape
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

    let messages = state.machine.tape.messages().to_vec();
    let retention_start = state.machine.tape.compaction_retention_start(keep_last);
    let to_summarize = messages[..retention_start].to_vec();

    if to_summarize.is_empty() {
        return Ok(skipped_outcome(
            request,
            estimated_prompt_tokens,
            CompactionSkipReason::EmptySummarizeRegion,
        ));
    }

    let compaction_count = state.machine.tape.compaction_count();
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

    if let Some(existing_summary) = state.machine.tape.summary() {
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
    let summary = loop {
        let generation_request = build_generation_request(
            Some(prompts::COMPACT_PROMPT.to_string()),
            llm_messages.clone(),
            Vec::new(),
            Some(0.2),
            Some(2048),
        );

        match state
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
                    state,
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
            state,
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
    let reference_context_revision = state.machine.tape.context_revision();
    let attempt_id = uuid::Uuid::new_v4().to_string();
    apply_tape_compaction(state, &summary, keep_last, retention_start);
    state.machine.clear_responses_continuation("compaction");
    let output_prompt_tokens = state
        .machine
        .tape
        .estimated_prompt_tokens()
        .saturating_add(request.additional_prompt_tokens());
    let output_messages = state.machine.tape.len();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let duration_ms = duration_ms_since(started_at);
    state.machine.reset_compaction_failure_streak();
    let attempt = build_compaction_attempt_snapshot(
        attempt_id.clone(),
        compaction_submission_id(state, request),
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
            reference_context_revision_after: Some(state.machine.tape.context_revision()),
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
    record_and_emit_compaction_attempt(state, emit, attempt, Some(compacted)).await;

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
            NamespaceRuntimeEnvironment, RuntimeConfig, RuntimeLoopState, TurnState,
            prompt_cache::PromptAssemblyCache,
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
            current_submission_id: None,
            environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            core_config: Config::default(),
            runtime_config: RuntimeConfig {
                compaction_trigger_messages: 1,
                compaction_keep_last: 1,
                ..RuntimeConfig::default()
            },
            definition_persona_dirs: Vec::new(),
            prompt_cache: PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
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
            state.machine.tape.summary(),
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
