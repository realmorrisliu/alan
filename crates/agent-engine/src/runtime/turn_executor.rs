use alan_agent_protocol::{CompactionOutcome, Event};
use anyhow::{Context, Result};
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::llm::build_generation_request;

use super::agent_loop::{DeferredRuntimeAction, RuntimeLoopState};
use super::compaction::{CompactionRequest, maybe_compact_context_with_cancel};
use super::response_guardrails::{
    AssistantDraft, GuardrailDecision, ResponseGuardrailContext, ResponseGuardrails,
};
use super::tool_orchestrator::{
    ToolBatchOrchestratorOutcome, ToolOrchestratorInputs, ToolTurnOrchestrator,
};
use super::turn_driver::TurnInputBroker;
use super::turn_support::{
    check_turn_cancelled, emit_streaming_chunks, emit_task_completed_success, emit_thinking_chunks,
    normalize_tool_calls,
};
use super::virtual_tools::virtual_tool_definitions;

mod namespace_generation;

use namespace_generation::NamespaceTurnGeneration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnRunKind {
    NewTurn,
    ResumeTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnExecutionOutcome {
    Finished,
    Paused,
}

const COMPACTION_TIMEOUT_SECS: u64 = 30;

struct TimedCompactionResult {
    result: Result<CompactionOutcome>,
    timed_out: bool,
}

fn append_system_instruction(request: &mut crate::llm::GenerationRequest, instruction: &str) {
    if let Some(system_prompt) = &mut request.system_prompt {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(instruction);
    } else {
        request.system_prompt = Some(instruction.to_string());
    }
}

fn estimate_runtime_system_instruction_tokens(instruction: &str) -> usize {
    crate::tape::estimate_text_tokens(instruction).saturating_add(1)
}

fn estimate_request_prompt_overhead_tokens(
    turn_recall_bundle: Option<&str>,
    pending_guardrail_instruction: Option<&str>,
) -> usize {
    turn_recall_bundle
        .into_iter()
        .chain(pending_guardrail_instruction)
        .map(estimate_runtime_system_instruction_tokens)
        .sum()
}

fn estimate_pending_turn_prompt_tokens(
    pending_user_input: Option<&[crate::tape::ContentPart]>,
    turn_recall_bundle: Option<&str>,
) -> usize {
    pending_user_input
        .map(crate::tape::estimate_user_message_tokens)
        .unwrap_or(0)
        .saturating_add(estimate_request_prompt_overhead_tokens(
            turn_recall_bundle,
            None,
        ))
}

async fn finalize_turn_memory_best_effort(
    state: &mut RuntimeLoopState,
    surfaces_refreshed: bool,
    surfaces_context: &'static str,
    promotion_context: &'static str,
) {
    if !surfaces_refreshed {
        super::memory_surfaces::refresh_turn_memory_surfaces_best_effort(state, surfaces_context)
            .await;
    }

    if let Some(job) =
        super::memory_promotion::build_turn_memory_promotion_job(state, promotion_context)
    {
        state
            .turn_state
            .push_deferred_runtime_action(DeferredRuntimeAction::TurnMemoryPromotion(job));
    }
}

async fn turn_tool_definitions(
    state: &RuntimeLoopState,
) -> anyhow::Result<(
    Vec<super::ToolPackageManifest>,
    Vec<crate::llm::ToolDefinition>,
)> {
    let include_runtime_delegated_tool = state.prompt_cache.supports_delegated_skill_invocation();

    let tool_packages = state
        .namespace_environment()
        .discover_tool_packages()
        .await?;
    let mut tools = tool_packages
        .iter()
        .map(|package| package.model_definition())
        .collect::<Vec<_>>();
    tools.extend(virtual_tool_definitions(include_runtime_delegated_tool));
    Ok((tool_packages, tools))
}

fn log_generation_failure(request_start: Instant, error: &anyhow::Error) {
    error!(
        elapsed_ms = request_start.elapsed().as_millis(),
        error = %error,
        "Namespace LLM failed"
    );
}

fn generation_error_message(error: &anyhow::Error) -> String {
    format!("Namespace LLM request failed: {error}")
}

fn should_skip_auto_compaction_for_responses_continuation(_state: &mut RuntimeLoopState) -> bool {
    false
}

async fn maybe_compact_context_with_turn_timeout<E, F>(
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
    let compaction = maybe_compact_context_with_cancel(state, emit, request, &compaction_cancel);
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

fn resolve_definition_persona_dirs(state: &RuntimeLoopState) -> Vec<std::path::PathBuf> {
    state.definition_persona_dirs.clone()
}

fn build_domain_prompt_with_skills(
    state: &mut RuntimeLoopState,
    user_input: Option<&[crate::tape::ContentPart]>,
    active_skills: Option<&[crate::skills::ActiveSkillEnvelope]>,
) -> super::prompt_cache::PromptAssemblyResult {
    state
        .prompt_cache
        .rebind_paths(resolve_definition_persona_dirs(state));
    state.prompt_cache.set_memory_store_dir(
        state
            .core_config
            .memory
            .enabled
            .then(|| state.core_config.memory.store_dir.clone())
            .flatten(),
    );
    match active_skills {
        Some(active_skills) => state
            .prompt_cache
            .build_with_active_skills(active_skills, user_input),
        None => state.prompt_cache.build(user_input),
    }
}

/// Run a single agent turn
pub(super) async fn run_turn_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    turn_kind: TurnRunKind,
    mut user_input: Option<Vec<crate::tape::ContentPart>>,
    emit: &mut E,
    cancel: &CancellationToken,
    steering_broker: Option<&TurnInputBroker>,
) -> Result<TurnExecutionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if matches!(turn_kind, TurnRunKind::NewTurn) {
        state.turn_state.reset_auto_mid_turn_compaction_state();
        super::ui_surfaces::turn_started(state.namespace_environment())
            .await
            .context("write turn-start UI state")?;
        emit(Event::TurnStarted {}).await;
    } else {
        super::ui_surfaces::resumed(state.namespace_environment())
            .await
            .context("write resumed turn UI state")?;
    }

    let namespace_generation = state.namespace_environment().clone();
    if matches!(turn_kind, TurnRunKind::NewTurn) && user_input.is_none() {
        let input = namespace_generation
            .read_next_input()
            .await
            .context("read next namespace agent input")?;
        user_input = Some(vec![crate::tape::ContentPart::text(input)]);
    }

    let user_input_for_skills = user_input.clone();
    let mut namespace_user_input_for_tape = user_input_for_skills
        .as_deref()
        .map(crate::tape::parts_to_text)
        .filter(|input| !input.trim().is_empty());
    let turn_recall_bundle = if state.core_config.memory.enabled {
        super::memory_recall::build_turn_recall_bundle(
            state.core_config.memory.store_dir.as_deref(),
            user_input_for_skills.as_deref(),
        )
    } else {
        None
    };

    if !should_skip_auto_compaction_for_responses_continuation(state) {
        let compaction_request = CompactionRequest::automatic_pre_turn()
            .with_additional_prompt_tokens(estimate_pending_turn_prompt_tokens(
                user_input_for_skills.as_deref(),
                turn_recall_bundle.as_deref(),
            ));
        let compaction = maybe_compact_context_with_turn_timeout(
            state,
            emit,
            &compaction_request,
            cancel,
            tokio::time::Duration::from_secs(COMPACTION_TIMEOUT_SECS),
        )
        .await;
        match compaction.result {
            Ok(_) => {}
            Err(e) if compaction.timed_out => {
                debug!(error = %e, "Context compaction stopped after timeout cancellation");
            }
            Err(e) => {
                warn!(error = %e, "Context compaction failed");
            }
        }
        if compaction.timed_out {
            warn!("Context compaction timeout - continuing without compaction");
        }
    }
    if check_turn_cancelled(state, emit, cancel).await? {
        return Ok(TurnExecutionOutcome::Finished);
    }

    if matches!(turn_kind, TurnRunKind::NewTurn) {
        state
            .turn_state
            .begin_turn(state.machine.tape.messages().len());
    } else if user_input.is_some() {
        state.turn_state.note_resumed_user_input();
    }
    if let Some(user_input) = user_input {
        state.machine.add_user_message_parts(user_input);
    }

    // Resume turns keep the same active skill envelopes for the logical turn.
    // Current user input can still add new skills via prompt assembly merge logic.
    let resumed_active_skills = matches!(turn_kind, TurnRunKind::ResumeTurn)
        .then(|| state.turn_state.active_skills().to_vec())
        .filter(|active_skills| !active_skills.is_empty());
    let prompt_build = build_domain_prompt_with_skills(
        state,
        user_input_for_skills.as_deref(),
        resumed_active_skills.as_deref(),
    );
    debug!(
        elapsed_ms = prompt_build.elapsed_ms,
        skills_cache_hit = prompt_build.skills_cache_hit,
        persona_cache_hit = prompt_build.persona_cache_hit,
        active_skills = prompt_build.active_skills.len(),
        cache_builds = prompt_build.metrics.builds,
        cache_hits = prompt_build.metrics.hits,
        "Prepared prompt assembly inputs"
    );
    state
        .turn_state
        .set_active_skills(prompt_build.active_skills.clone());
    let active_skill_ids = prompt_build
        .active_skills
        .iter()
        .map(|skill| skill.metadata.id.clone())
        .collect::<Vec<_>>();
    let _domain_prompt = prompt_build.domain_prompt;
    let system_prompt = prompt_build.system_prompt;

    let (tool_packages, tools) = turn_tool_definitions(state).await?;
    let tool_names = tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect::<Vec<_>>();
    let generation = NamespaceTurnGeneration::load(state).await;
    let initial_provider_capabilities = generation.capabilities();
    let turn_request_controls = crate::resolve_turn_request_controls(
        &state.core_config,
        initial_provider_capabilities,
        state.runtime_config.request_control_intent,
        state.turn_state.active_turn_request_control_intent(),
    )?;
    let model = state.core_config.effective_model().to_string();
    let memory_enabled = state.core_config.memory.enabled;
    let context_items = state.machine.tape.context_items().to_vec();
    let context_delta = state.machine.tape.last_context_delta().clone();
    state.machine.record_turn_context_if_changed(
        &model,
        turn_request_controls.reasoning_effort(),
        &system_prompt,
        &context_items,
        &tool_names,
        memory_enabled,
        &active_skill_ids,
        &context_delta,
    );

    let max_tool_loops = if state.runtime_config.max_tool_loops == 0 {
        None
    } else {
        Some(state.runtime_config.max_tool_loops)
    };
    let mut tool_orchestrator =
        ToolTurnOrchestrator::new(max_tool_loops, state.runtime_config.tool_repeat_limit);
    let mut response_guardrails = ResponseGuardrails::default();
    let mut pending_guardrail_instruction: Option<String> = None;
    loop {
        if check_turn_cancelled(state, emit, cancel).await? {
            return Ok(TurnExecutionOutcome::Finished);
        }
        let provider = generation.provider();
        if state
            .machine
            .responses_continuation()
            .is_some_and(|continuation| continuation.provider == provider)
        {
            state
                .machine
                .clear_responses_continuation("provider_capability_unavailable");
        }

        let prompt_view = state.machine.tape.prompt_view();
        let estimated_prompt_tokens =
            prompt_view
                .estimated_tokens
                .saturating_add(estimate_request_prompt_overhead_tokens(
                    turn_recall_bundle.as_deref(),
                    pending_guardrail_instruction.as_deref(),
                ));
        let context_revision = prompt_view.reference_context.revision;
        let messages = prompt_view.messages;
        let llm_messages = crate::llm::project_messages(&messages, true);
        let llm_tools: Vec<crate::llm::ToolDefinition> = tools
            .iter()
            .map(|t| {
                crate::llm::ToolDefinition::new(&t.name, &t.description)
                    .with_parameters(t.parameters.clone())
            })
            .collect();

        let mut request = build_generation_request(
            Some(system_prompt.clone()),
            llm_messages,
            llm_tools,
            Some(state.runtime_config.temperature),
            Some(state.runtime_config.max_tokens as i32),
        );
        if let Some(recall_bundle) = turn_recall_bundle.as_deref() {
            append_system_instruction(&mut request, recall_bundle);
        }
        if let Some(instruction) = pending_guardrail_instruction.as_deref() {
            append_system_instruction(&mut request, instruction);
        }
        let request_controls = turn_request_controls.clone();
        request = request.with_reasoning_controls(request_controls.reasoning);

        let request_start = Instant::now();
        let reasoning_effort_log = request_controls
            .reasoning_effort()
            .map(|effort| effort.to_string());
        info!(
            messages = messages.len(),
            estimated_prompt_tokens,
            context_revision,
            tools = tools.len(),
            provider,
            reasoning_effort = reasoning_effort_log.as_deref(),
            request_control_source = ?request_controls.source,
            "LLM request"
        );

        let streaming_requested = match state.runtime_config.streaming_mode {
            crate::config::StreamingMode::Off => false,
            crate::config::StreamingMode::On | crate::config::StreamingMode::Auto => true,
        };
        if streaming_requested {
            debug!("Streaming mode requested; generation uses request/response file semantics");
        }
        let llm_request_timeout_secs = state.runtime_config.llm_request_timeout_secs;
        let (response, live_text_chunks) = match generation
            .generate(state, request, llm_request_timeout_secs, cancel)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                if cancel.is_cancelled() && check_turn_cancelled(state, emit, cancel).await? {
                    return Ok(TurnExecutionOutcome::Finished);
                }
                log_generation_failure(request_start, &error);
                emit(Event::Error {
                    message: generation_error_message(&error),
                    recoverable: true,
                })
                .await;
                return Ok(TurnExecutionOutcome::Finished);
            }
        };

        if let Some(usage) = response.usage {
            info!(
                prompt_tokens = usage.prompt_tokens,
                completion_tokens = usage.completion_tokens,
                total_tokens = usage.total_tokens,
                reasoning_tokens = ?usage.reasoning_tokens,
                "LLM usage"
            );
        }

        for warning in &response.warnings {
            super::ui_surfaces::warning(state.namespace_environment(), warning.clone())
                .await
                .context("write provider warning UI state")?;
            emit(Event::Warning {
                message: warning.clone(),
            })
            .await;
        }

        let tool_calls = normalize_tool_calls(response.tool_calls);

        let guardrail_context = ResponseGuardrailContext::from_state(state, &tool_packages);
        let guardrail_draft = AssistantDraft::new(&response.content, !tool_calls.is_empty());
        match response_guardrails.evaluate(&guardrail_context, &guardrail_draft) {
            GuardrailDecision::Accept => {
                pending_guardrail_instruction = None;
            }
            GuardrailDecision::Recover {
                rule_id,
                reason,
                instruction,
            } => {
                warn!(
                    rule_id,
                    reason = %reason,
                    "Response guardrail triggered for assistant output"
                );
                let message =
                    format!("Guardrail recovered ({rule_id}): {reason}. Retrying before output.");
                super::ui_surfaces::warning(state.namespace_environment(), message.clone())
                    .await
                    .context("write guardrail warning UI state")?;
                emit(Event::Warning { message }).await;
                pending_guardrail_instruction = Some(instruction);
                maybe_compact_mid_turn_if_needed(
                    state,
                    emit,
                    cancel,
                    estimate_request_prompt_overhead_tokens(
                        turn_recall_bundle.as_deref(),
                        pending_guardrail_instruction.as_deref(),
                    ),
                )
                .await?;
                if check_turn_cancelled(state, emit, cancel).await? {
                    return Ok(TurnExecutionOutcome::Finished);
                }
                continue;
            }
        }

        if let Some(ref thinking) = response.thinking
            && !thinking.is_empty()
        {
            super::ui_surfaces::thinking(state.namespace_environment(), thinking)
                .await
                .context("write thinking UI state")?;
            emit_thinking_chunks(emit, thinking).await;
        }

        if !response.content.is_empty() {
            if live_text_chunks.is_empty() {
                emit_streaming_chunks(emit, &response.content).await;
            } else {
                for chunk in live_text_chunks {
                    emit(Event::TextDelta {
                        chunk,
                        is_final: false,
                    })
                    .await;
                }
                emit(Event::TextDelta {
                    chunk: String::new(),
                    is_final: true,
                })
                .await;
            }
        }

        let assistant_message_persisted = if !tool_calls.is_empty() {
            let machine_tool_calls: Vec<crate::tape::ToolRequest> = tool_calls
                .iter()
                .map(|tc| crate::tape::ToolRequest {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                })
                .collect();
            state
                .machine
                .add_assistant_message_with_tool_calls_and_reasoning(
                    &response.content,
                    machine_tool_calls,
                    response.thinking.as_deref(),
                    response.thinking_signature.as_deref(),
                    &response.redacted_thinking,
                );
            true
        } else if !response.content.is_empty() {
            state.machine.add_assistant_message_with_reasoning(
                &response.content,
                response.thinking.as_deref(),
                response.thinking_signature.as_deref(),
                &response.redacted_thinking,
            );
            true
        } else {
            false
        };

        if assistant_message_persisted && !response.content.is_empty() {
            let namespace_input_text = namespace_user_input_for_tape.take();
            namespace_generation
                .write_assistant_output(&response.content)
                .await
                .context("write namespace assistant output")?;
            namespace_generation
                .write_turn_tape_state(namespace_input_text.as_deref(), &response.content)
                .await
                .context("write namespace turn tape state")?;
        }

        if !tool_calls.is_empty() {
            match tool_orchestrator
                .orchestrate_tool_batch(
                    state,
                    &tool_calls,
                    ToolOrchestratorInputs {
                        cancel,
                        steering_broker,
                    },
                    emit,
                )
                .await?
            {
                ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. } => {
                    maybe_compact_mid_turn_if_needed(
                        state,
                        emit,
                        cancel,
                        estimate_request_prompt_overhead_tokens(
                            turn_recall_bundle.as_deref(),
                            pending_guardrail_instruction.as_deref(),
                        ),
                    )
                    .await?;
                    if check_turn_cancelled(state, emit, cancel).await? {
                        return Ok(TurnExecutionOutcome::Finished);
                    }
                }
                ToolBatchOrchestratorOutcome::PauseTurn => return Ok(TurnExecutionOutcome::Paused),
                ToolBatchOrchestratorOutcome::EndTurn { surfaces_refreshed } => {
                    if !cancel.is_cancelled() {
                        finalize_turn_memory_best_effort(
                            state,
                            surfaces_refreshed,
                            "turn-ended-after-tool-batch",
                            "after tool-driven end turn",
                        )
                        .await;
                    }
                    return Ok(TurnExecutionOutcome::Finished);
                }
            }
            continue;
        }

        if response.content.is_empty() {
            let fallback_text = "I apologize, but I couldn't generate a response.";
            // Persist fallback output (and any reasoning metadata) to tape so
            // subsequent turns can reference what the assistant actually emitted.
            state.machine.add_assistant_message_with_reasoning(
                fallback_text,
                response.thinking.as_deref(),
                response.thinking_signature.as_deref(),
                &response.redacted_thinking,
            );
            let namespace_input_text = namespace_user_input_for_tape.take();
            namespace_generation
                .write_assistant_output(fallback_text)
                .await
                .context("write namespace fallback assistant output")?;
            namespace_generation
                .write_turn_tape_state(namespace_input_text.as_deref(), fallback_text)
                .await
                .context("write namespace fallback turn tape state")?;
            finalize_turn_memory_best_effort(
                state,
                false,
                "fallback-turn-completed",
                "after fallback turn",
            )
            .await;
            emit(Event::TextDelta {
                chunk: fallback_text.to_string(),
                is_final: true,
            })
            .await;
            emit(Event::TurnCompleted {
                summary: Some("Turn completed with empty response fallback".to_string()),
            })
            .await;
            super::ui_surfaces::turn_completed(state.namespace_environment(), false)
                .await
                .context("write fallback turn completion UI state")?;
            return Ok(TurnExecutionOutcome::Finished);
        }

        finalize_turn_memory_best_effort(state, false, "turn-completed", "after completed turn")
            .await;
        emit_task_completed_success(state, emit, "Task completed").await?;
        return Ok(TurnExecutionOutcome::Finished);
    }
}

async fn maybe_compact_mid_turn_if_needed<E, F>(
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
        .tape
        .estimated_prompt_tokens()
        .saturating_add(additional_prompt_tokens);
    let context_window_tokens = state.runtime_config.context_window_tokens as usize;
    if !state
        .turn_state
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
                .turn_state
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::turn_state::TurnActivityState;
    use crate::{
        agent_machine::AgentMachine,
        config::Config,
        rollout::{RolloutItem, RolloutRecorder},
        runtime::{NamespaceRuntimeEnvironment, RuntimeConfig, TurnState},
        skills::{ResolvedCapabilityView, ScopedPackageDir, SkillScope},
        tape::{ContentPart, Message, ToolRequest},
        tools::{Tool, ToolContext, ToolRegistry, ToolResult},
    };
    use alan_llm::{
        GenerationRequest, GenerationResponse, LlmProvider, StreamChunk, ToolCall, ToolCallDelta,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    struct TestToolProcessRunner {
        tools: ToolRegistry,
    }

    impl TestToolProcessRunner {
        fn new(tools: ToolRegistry) -> Self {
            Self { tools }
        }
    }

    #[async_trait]
    impl alan_kernel::ProcessRunner for TestToolProcessRunner {
        async fn run(
            &self,
            invocation: alan_kernel::ProcessInvocation,
        ) -> alan_kernel::ProcessOutcome {
            if invocation
                .namespace
                .resolve(&invocation.exec.executable)
                .is_err()
            {
                return alan_kernel::ProcessOutcome::exited(
                    127,
                    b"executable is not mounted\n".to_vec(),
                );
            }
            let tool_name = invocation
                .exec
                .executable
                .rsplit('/')
                .next()
                .unwrap_or(invocation.exec.executable.as_str());
            let arguments = invocation
                .exec
                .args
                .first()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .unwrap_or(serde_json::Value::Null);

            match self.tools.execute(tool_name, arguments).await {
                Ok(output) => {
                    let mut bytes = serde_json::to_vec(&output)
                        .unwrap_or_else(|_| b"{\"success\":true}".to_vec());
                    bytes.push(b'\n');
                    alan_kernel::ProcessOutcome::exited(0, bytes)
                }
                Err(err) => {
                    let mut bytes = serde_json::to_vec(&serde_json::json!({
                        "success": false,
                        "error": format!("{err:#}"),
                    }))
                    .unwrap_or_else(|_| b"{\"success\":false}".to_vec());
                    bytes.push(b'\n');
                    alan_kernel::ProcessOutcome::exited(1, bytes)
                }
            }
        }
    }

    fn maybe_memory_promotion_response(request: &GenerationRequest) -> Option<GenerationResponse> {
        let system_prompt = request.system_prompt.as_deref()?;
        if system_prompt != crate::prompts::MEMORY_PROMOTION_PROMPT {
            return None;
        }

        let joined_user_text = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let content = if joined_user_text.contains("My name is Morris.") {
            serde_json::json!({
                "writes": [{
                    "kind": "user_identity",
                    "target": "USER.md",
                    "confidence": "high",
                    "disposition": "promote_now",
                    "observation": "Name: Morris",
                    "evidence": ["My name is Morris."],
                    "promotion_rationale": "Direct user-stated stable identity detail."
                }]
            })
            .to_string()
        } else {
            serde_json::json!({ "writes": [] }).to_string()
        };

        Some(GenerationResponse {
            content,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        })
    }

    fn response_stream(response: GenerationResponse) -> tokio::sync::mpsc::Receiver<StreamChunk> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            if !response.content.is_empty()
                || response
                    .thinking
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                || response
                    .thinking_signature
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                || !response.redacted_thinking.is_empty()
            {
                let mut redacted = response.redacted_thinking.into_iter();
                let _ = tx
                    .send(StreamChunk {
                        text: (!response.content.is_empty()).then_some(response.content),
                        thinking: response.thinking,
                        thinking_signature: response.thinking_signature,
                        redacted_thinking: redacted.next(),
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await;
                for redacted in redacted {
                    let _ = tx
                        .send(StreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: Some(redacted),
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: false,
                            finish_reason: None,
                        })
                        .await;
                }
            }

            let tool_calls = response.tool_calls;
            for (index, tool_call) in tool_calls.iter().enumerate() {
                let arguments =
                    serde_json::to_string(&tool_call.arguments).unwrap_or_else(|_| "{}".into());
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: Some(ToolCallDelta {
                            index,
                            id: tool_call.id.clone(),
                            name: Some(tool_call.name.clone()),
                            arguments_delta: Some(arguments.clone()),
                            arguments: Some(arguments),
                        }),
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await;
            }

            let finish_reason = response.finish_reason.unwrap_or_else(|| {
                if tool_calls.is_empty() {
                    "stop".to_string()
                } else {
                    "tool_calls".to_string()
                }
            });
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: response.usage,
                    provider_response_id: response.provider_response_id,
                    provider_response_status: response.provider_response_status,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some(finish_reason),
                })
                .await;
        });
        rx
    }

    // Mock provider that returns content without tool calls
    struct ContentMockProvider {
        content: String,
        thinking: Option<String>,
    }

    impl ContentMockProvider {
        fn new(content: impl Into<String>) -> Self {
            Self {
                content: content.into(),
                thinking: None,
            }
        }

        fn with_thinking(mut self, thinking: impl Into<String>) -> Self {
            self.thinking = Some(thinking.into());
            self
        }
    }

    #[async_trait]
    impl LlmProvider for ContentMockProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: self.thinking.clone(),
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.content.clone())
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response_stream(response));
            }
            Ok(response_stream(GenerationResponse {
                content: self.content.clone(),
                thinking: self.thinking.clone(),
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            }))
        }

        fn provider_name(&self) -> &'static str {
            "content_mock"
        }
    }

    struct BlockingStreamProvider {
        started: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl LlmProvider for BlockingStreamProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Err(anyhow::anyhow!("blocking provider uses streaming"))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("blocking stream provider".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.started.notify_one();
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            tokio::spawn(async move {
                let _hold = tx;
                std::future::pending::<()>().await;
            });
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "blocking_stream"
        }
    }

    struct PanicOnStreamProvider {
        content: String,
        generate_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for PanicOnStreamProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.content.clone())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            Ok(response_stream(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            }))
        }

        fn provider_name(&self) -> &'static str {
            "panic_on_stream"
        }
    }

    struct TransientStreamFailureProvider {
        generate_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmProvider for TransientStreamFailureProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Err(anyhow::anyhow!(
                "transient stream provider should use generate_stream"
            ))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("transient stream mock".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let call = self.generate_calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                return Err(anyhow::anyhow!("temporary 503 from stream"));
            }
            Ok(response_stream(GenerationResponse {
                content: "Recovered after retry.".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            }))
        }

        fn provider_name(&self) -> &'static str {
            "transient_stream_failure"
        }
    }

    struct PanicIfGeneratedProvider;

    struct NamedRecordingStreamProvider {
        provider_name: &'static str,
        chunks: Vec<String>,
        requests: Arc<Mutex<Vec<GenerationRequest>>>,
    }

    impl NamedRecordingStreamProvider {
        fn content(&self) -> String {
            self.chunks.concat()
        }
    }

    #[async_trait]
    impl LlmProvider for PanicIfGeneratedProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            panic!("namespace-backed turn must not call provider generate")
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            panic!("namespace-backed turn must not call provider chat")
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            panic!("namespace-backed turn must not call provider generate_stream")
        }

        fn provider_name(&self) -> &'static str {
            "content_mock"
        }
    }

    #[async_trait]
    impl LlmProvider for NamedRecordingStreamProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.requests.lock().unwrap().push(request);
            Ok(GenerationResponse {
                content: self.content(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.content())
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.requests.lock().unwrap().push(request);
            let (tx, rx) = tokio::sync::mpsc::channel(2);
            let chunks = self.chunks.clone();
            tokio::spawn(async move {
                if chunks.is_empty() {
                    let _ = tx
                        .send(StreamChunk {
                            text: None,
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
                    return;
                }

                let chunk_count = chunks.len();
                for (index, chunk) in chunks.into_iter().enumerate() {
                    let is_finished = index + 1 == chunk_count;
                    let _ = tx
                        .send(StreamChunk {
                            text: Some(chunk),
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: Some(index as u64),
                            tool_call_delta: None,
                            is_finished,
                            finish_reason: is_finished.then(|| "stop".to_string()),
                        })
                        .await;
                }
            });
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            self.provider_name
        }
    }

    struct FailOnMemoryPromotionProvider {
        content: String,
    }

    #[async_trait]
    impl LlmProvider for FailOnMemoryPromotionProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            if maybe_memory_promotion_response(&request).is_some() {
                panic!("turn execution should not synchronously call memory promotion");
            }

            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.content.clone())
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            if maybe_memory_promotion_response(&request).is_some() {
                panic!("turn execution should not synchronously call memory promotion");
            }

            Ok(response_stream(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            }))
        }

        fn provider_name(&self) -> &'static str {
            "fail_on_memory_promotion"
        }
    }

    // Mock provider that returns tool calls
    struct ToolCallMockProvider {
        tool_calls: Vec<ToolCall>,
        content: String,
    }

    impl ToolCallMockProvider {
        fn new(tool_calls: Vec<ToolCall>, content: impl Into<String>) -> Self {
            Self {
                tool_calls,
                content: content.into(),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for ToolCallMockProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: self.tool_calls.clone(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(format!("mock: {}", self.content))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response_stream(response));
            }
            Ok(response_stream(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: self.tool_calls.clone(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            }))
        }

        fn provider_name(&self) -> &'static str {
            "tool_mock"
        }
    }

    struct CapturingResponsesProvider {
        requests: Arc<Mutex<Vec<GenerationRequest>>>,
        response: GenerationResponse,
        provider_name: &'static str,
    }

    #[async_trait]
    impl LlmProvider for CapturingResponsesProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.requests.lock().unwrap().push(request);
            Ok(self.response.clone())
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(self.response.content.clone())
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.requests.lock().unwrap().push(request);
            Ok(response_stream(self.response.clone()))
        }

        fn provider_name(&self) -> &'static str {
            self.provider_name
        }
    }

    struct SequenceMockProvider {
        responses: VecDeque<GenerationResponse>,
        generate_calls: Arc<AtomicUsize>,
    }

    impl SequenceMockProvider {
        fn new(responses: Vec<GenerationResponse>, generate_calls: Arc<AtomicUsize>) -> Self {
            Self {
                responses: responses.into(),
                generate_calls,
            }
        }
    }

    #[async_trait]
    impl LlmProvider for SequenceMockProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            self.responses
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("No more scripted responses"))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("sequence mock".to_string())
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.generate_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response_stream(response));
            }
            let response = self
                .responses
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("No more scripted responses"))?;
            Ok(response_stream(response))
        }

        fn provider_name(&self) -> &'static str {
            "sequence_mock"
        }
    }

    struct NetworkCapabilityTool;

    impl Tool for NetworkCapabilityTool {
        fn name(&self) -> &str {
            "network_probe"
        }

        fn description(&self) -> &str {
            "Test tool classified as network capability."
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move { Ok(json!({"ok": true})) })
        }

        fn capability(
            &self,
            _arguments: &serde_json::Value,
        ) -> alan_agent_protocol::ToolCapability {
            alan_agent_protocol::ToolCapability::Network
        }
    }

    struct ReadCapabilityTool;

    impl Tool for ReadCapabilityTool {
        fn name(&self) -> &str {
            "local_probe"
        }

        fn description(&self) -> &str {
            "Test tool classified as read capability."
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move { Ok(json!({"ok": true})) })
        }

        fn capability(
            &self,
            _arguments: &serde_json::Value,
        ) -> alan_agent_protocol::ToolCapability {
            alan_agent_protocol::ToolCapability::Read
        }
    }

    struct LargeOutputTool {
        output: String,
    }

    impl LargeOutputTool {
        fn new(output: impl Into<String>) -> Self {
            Self {
                output: output.into(),
            }
        }
    }

    impl Tool for LargeOutputTool {
        fn name(&self) -> &str {
            "emit_large_output"
        }

        fn description(&self) -> &str {
            "Emit a large text payload for compaction tests."
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
            let payload = serde_json::to_value(ContentPart::text(self.output.clone())).unwrap();
            Box::pin(async move { Ok(payload) })
        }
    }

    fn create_test_state_with_provider<P: LlmProvider + 'static>(provider: P) -> RuntimeLoopState {
        create_test_state_with_provider_and_tools(provider, ToolRegistry::new())
    }

    fn create_test_state_with_provider_and_tools<P: LlmProvider + 'static>(
        provider: P,
        tools: ToolRegistry,
    ) -> RuntimeLoopState {
        create_test_state_with_provider_and_tools_and_shell(provider, tools).0
    }

    fn create_test_state_with_provider_and_tools_and_shell<P: LlmProvider + 'static>(
        provider: P,
        mut tools: ToolRegistry,
    ) -> (RuntimeLoopState, alan_shell::Shell) {
        let config = Config {
            openai_responses_model: "mock-model".to_string(),
            ..Default::default()
        };
        let machine = AgentMachine::new();
        let llmfs = std::sync::Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(provider));

        let mut process_namespace = alan_kernel::Namespace::new();
        process_namespace.mount(
            "/agent/1",
            alan_ap::InProcessTransport::new(std::sync::Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        process_namespace.mount(
            "/mnt/llm",
            alan_ap::InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        for tool_name in tools.list_tools() {
            process_namespace.mount(
                &format!("/bin/{tool_name}"),
                alan_ap::InProcessTransport::new(std::sync::Arc::new(
                    alan_ap::reference::MemFs::new(),
                )),
                alan_kernel::Access::ReadOnly,
            );
            let tool = tools.get(tool_name).unwrap();
            let manifest = crate::runtime::ToolPackageManifest::from_tool(
                tool.as_ref(),
                tools.execution_timeout_secs(tool_name).unwrap_or(30),
            )
            .unwrap();
            process_namespace.mount(
                &format!("/lib/exec/{tool_name}"),
                alan_ap::InProcessTransport::new(std::sync::Arc::new(
                    alan_ap::reference::MemFs::with_read_only_file(
                        "manifest",
                        serde_json::to_vec(&manifest).unwrap(),
                    ),
                )),
                alan_kernel::Access::ReadOnly,
            );
        }
        let launch_context = crate::ProcessLaunchContext::new(
            process_namespace.clone(),
            alan_kernel::Credentials::user("test-agent"),
            "/mnt/source",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new("/mnt/source", "/tmp", alan_kernel::Access::ReadWrite)
                .unwrap(),
        );
        tools.set_default_execution_binding(
            crate::tools::ToolExecutionBinding::from_launch_context(
                &launch_context,
                std::path::PathBuf::from("/tmp/alan-turn-executor-test-scratch"),
            )
            .unwrap(),
        );
        let procfs = alan_kernel::ProcFs::new().with_runner(std::sync::Arc::new(
            TestToolProcessRunner::new(tools.clone()),
        ));
        let process_procfs = procfs.for_spawner(
            None,
            process_namespace.clone(),
            alan_kernel::Credentials::user("root-agent"),
        );
        process_namespace.mount(
            "/proc",
            alan_ap::InProcessTransport::new(std::sync::Arc::new(process_procfs)),
            alan_kernel::Access::ReadWrite,
        );
        let root = alan_ap::InProcessTransport::new(std::sync::Arc::new(
            alan_kernel::MountFs::new(process_namespace),
        ));
        // Keep turn-executor tests deterministic by defaulting to non-streaming unless a test
        // explicitly opts into streaming semantics.
        let runtime_config = RuntimeConfig {
            streaming_mode: crate::config::StreamingMode::Off,
            ..RuntimeConfig::default()
        };

        let state = RuntimeLoopState {
            machine,
            current_submission_id: None,
            environment: NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default")
                .with_launch_context(launch_context),
            core_config: config,
            runtime_config,
            definition_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };
        (state, alan_shell::Shell::new(root))
    }

    async fn run_deferred_runtime_actions(state: &mut RuntimeLoopState) -> usize {
        let cancel = CancellationToken::new();
        let actions = state.turn_state.drain_deferred_runtime_actions();
        let count = actions.len();
        for action in actions {
            assert_eq!(
                super::super::agent_loop::run_deferred_runtime_action_with_cancel(
                    state, action, &cancel,
                )
                .await,
                super::super::agent_loop::DeferredRuntimeActionExit::Completed,
                "run deferred runtime action"
            );
        }
        count
    }

    fn prompt_cache_for_definition_root(
        definition_root: &std::path::Path,
        definition_persona_dirs: Vec<std::path::PathBuf>,
    ) -> crate::runtime::prompt_cache::PromptAssemblyCache {
        let capability_view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: definition_root.join("skills"),
            scope: SkillScope::Descriptor,
        }]);
        crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            capability_view,
            definition_persona_dirs,
            crate::skills::SkillHostCapabilities::default(),
        )
    }

    fn create_repo_skill(
        definition_root: &std::path::Path,
        dir_name: &str,
        skill_name: &str,
        description: &str,
        body: &str,
    ) {
        let skill_dir = definition_root.join("skills").join(dir_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {skill_name}
description: {description}
---

{body}
"#
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_turn_tool_definitions_include_runtime_delegated_schema_when_supported() {
        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.prompt_cache.set_host_capabilities(
            crate::skills::SkillHostCapabilities::default()
                .with_runtime_defaults()
                .with_delegated_skill_invocation(),
        );

        let (_, tools) = turn_tool_definitions(&state).await.unwrap();
        assert!(
            tools
                .iter()
                .any(|tool| tool.name == "invoke_delegated_skill")
        );
    }

    #[tokio::test]
    async fn unmounted_tool_is_not_model_callable() {
        let state = create_test_state_with_provider(ContentMockProvider::new("ok"));

        let (_, tools) = turn_tool_definitions(&state).await.unwrap();
        assert!(!tools.iter().any(|tool| tool.name == "network_probe"));
    }

    #[test]
    fn test_build_domain_prompt_with_skills_includes_mentioned_repo_skill_instructions() {
        let temp = TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_repo_skill(
            &definition_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let user_input = vec![ContentPart::text("please use $my-skill for this task")];
        let prompt = build_domain_prompt_with_skills(&mut state, Some(&user_input), None);

        assert!(prompt.system_prompt.contains("## Available Skills"));
        assert!(
            prompt
                .system_prompt
                .contains("## Active Skill Instructions")
        );
        assert!(prompt.system_prompt.contains("## Skill: My Skill"));
        assert!(prompt.system_prompt.contains("Use this skill when asked."));
    }

    #[test]
    fn test_build_domain_prompt_with_skills_uses_explicit_definition_persona() {
        let temp = TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let alan_dir = definition_root.join(".alan");
        let persona_dir = alan_dir.join("agents/default/persona");
        let memory_dir = alan_dir.join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        crate::prompts::ensure_definition_bootstrap_files_at(&persona_dir).unwrap();
        std::fs::write(persona_dir.join("SOUL.md"), "custom fallback persona").unwrap();

        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.core_config.memory.store_dir = Some(memory_dir);
        state.definition_persona_dirs = vec![persona_dir];
        state.prompt_cache = prompt_cache_for_definition_root(
            &definition_root,
            state.definition_persona_dirs.clone(),
        );

        let prompt = build_domain_prompt_with_skills(&mut state, None, None);

        assert!(prompt.system_prompt.contains("Agent Definition Persona"));
        assert!(prompt.system_prompt.contains("custom fallback persona"));
    }

    #[test]
    fn test_build_domain_prompt_with_skills_omits_memory_bootstrap_when_memory_disabled() {
        let temp = TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let alan_dir = definition_root.join(".alan");
        let memory_dir = alan_dir.join("memory");
        crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("USER.md"), "# User Memory\n- Morris\n").unwrap();

        let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
        state.core_config.memory.store_dir = Some(memory_dir);
        state.core_config.memory.enabled = false;
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let prompt = build_domain_prompt_with_skills(&mut state, None, None);

        assert!(!prompt.system_prompt.contains("Memory Store Bootstrap"));
        assert!(!prompt.system_prompt.contains("# User Memory"));
    }

    mod namespace_generation;

    mod turn_execution;

    #[tokio::test]
    async fn test_run_turn_refreshes_memory_surfaces_when_tool_batch_ends_turn() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join("memory-store");

        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({}),
            }],
            "",
        ));
        state.core_config.memory.store_dir = Some(memory_dir.clone());
        state
            .turn_state
            .set_turn_activity(TurnActivityState::Running);

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Error { message, .. } if message == "Invalid confirmation request."
        )));
        assert!(memory_dir.join("handoffs").join("LATEST.md").exists());
        assert!(memory_dir.join("episodic").exists());
        assert!(
            std::fs::read_dir(memory_dir.join("daily"))
                .unwrap()
                .next()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_run_turn_promotes_direct_user_fact_when_tool_batch_ends_turn() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join("memory-store");

        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({}),
            }],
            "",
        ));
        state.core_config.memory.store_dir = Some(memory_dir.clone());
        state.core_config.memory.enabled = true;
        state
            .turn_state
            .set_turn_activity(TurnActivityState::Running);

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("My name is Morris.")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        let user_memory_before =
            std::fs::read_to_string(memory_dir.join("USER.md")).unwrap_or_else(|_| String::new());
        assert!(!user_memory_before.contains("Name: Morris"));
        assert_eq!(run_deferred_runtime_actions(&mut state).await, 1);

        let user_memory = std::fs::read_to_string(memory_dir.join("USER.md")).unwrap();
        assert!(user_memory.contains("Name: Morris"));
    }

    #[tokio::test]
    async fn test_run_turn_defers_memory_promotion_until_after_completion() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join("memory-store");

        let mut state = create_test_state_with_provider(FailOnMemoryPromotionProvider {
            content: "Done.".to_string(),
        });
        state.core_config.memory.store_dir = Some(memory_dir);
        state.core_config.memory.enabled = true;

        let cancel = CancellationToken::new();
        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("My name is Morris.")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::TurnCompleted { .. }))
        );
        assert_eq!(state.turn_state.drain_deferred_runtime_actions().len(), 1);
    }

    struct SlowTool {
        delay: tokio::time::Duration,
    }

    impl Tool for SlowTool {
        fn name(&self) -> &str {
            "slow_tool"
        }

        fn description(&self) -> &str {
            "Slow tool used to test cancellation."
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
            let delay = self.delay;
            Box::pin(async move {
                tokio::time::sleep(delay).await;
                Ok(json!({ "ok": true }))
            })
        }
    }

    #[tokio::test]
    async fn test_run_turn_cancelled_tool_batch_does_not_refresh_memory_surfaces() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join("memory-store");

        let mut tools = ToolRegistry::new();
        tools.register(SlowTool {
            delay: tokio::time::Duration::from_millis(50),
        });
        let mut state = create_test_state_with_provider_and_tools(
            ToolCallMockProvider::new(
                vec![ToolCall {
                    id: Some("call_1".to_string()),
                    name: "slow_tool".to_string(),
                    arguments: json!({}),
                }],
                "",
            ),
            tools,
        );
        state.core_config.memory.store_dir = Some(memory_dir.clone());
        state
            .turn_state
            .set_turn_activity(TurnActivityState::Running);

        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            cancel_for_task.cancel();
        });

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::TurnCompleted { summary: Some(summary) }
                if summary == "Task cancelled by user"
        )));
        assert!(!memory_dir.join("handoffs").join("LATEST.md").exists());
        assert!(!memory_dir.join("episodic").exists());
        assert!(!memory_dir.join("daily").exists());
    }

    #[tokio::test]
    #[allow(
        clippy::field_reassign_with_default,
        reason = "the test highlights only the memory-refresh fields that define this scenario"
    )]
    async fn test_run_turn_tool_loop_guard_refreshes_memory_surfaces_before_completion_event() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join("memory-store");
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let provider = SequenceMockProvider::new(
            vec![
                GenerationResponse {
                    content: String::new(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![ToolCall {
                        id: Some("call-1".to_string()),
                        name: "update_plan".to_string(),
                        arguments: json!({
                            "explanation": "Loop 1",
                            "items": [{"id": "1", "content": "Step 1", "status": "in_progress"}]
                        }),
                    }],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
                GenerationResponse {
                    content: String::new(),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: Vec::new(),
                    tool_calls: vec![ToolCall {
                        id: Some("call-2".to_string()),
                        name: "update_plan".to_string(),
                        arguments: json!({
                            "explanation": "Loop 2",
                            "items": [{"id": "2", "content": "Step 2", "status": "in_progress"}]
                        }),
                    }],
                    usage: None,
                    finish_reason: None,
                    warnings: Vec::new(),
                    provider_response_id: None,
                    provider_response_status: None,
                },
            ],
            Arc::clone(&generate_calls),
        );
        let mut state = create_test_state_with_provider(provider);
        state.core_config.memory.store_dir = Some(memory_dir.clone());
        state.runtime_config.max_tool_loops = 2;

        let cancel = CancellationToken::new();
        let mut saw_handoff_before_completion = false;
        let mut emit = |event: Event| {
            if matches!(
                event,
                Event::TurnCompleted {
                    summary: Some(ref summary)
                } if summary == "Tool loop stopped by loop guard"
            ) {
                saw_handoff_before_completion = memory_dir.join("handoffs/LATEST.md").exists();
            }
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "Run until the loop guard stops you.",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(generate_calls.load(Ordering::SeqCst), 2);
        assert_eq!(state.turn_state.drain_deferred_runtime_actions().len(), 1);
        assert!(saw_handoff_before_completion);
    }

    #[tokio::test]
    async fn test_run_turn_confirmation_includes_active_skill_permission_hints() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let skill_dir = definition_root.join("skills/release-check");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let mut state = create_test_state_with_provider(ToolCallMockProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
        ));
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "please use $release-check for this task",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "release-check");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "May require write approval."
        );
    }

    struct RecordingToolCallProvider {
        tool_calls: Vec<ToolCall>,
        content: String,
        seen_system_prompts: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl RecordingToolCallProvider {
        fn new(
            tool_calls: Vec<ToolCall>,
            content: impl Into<String>,
            seen_system_prompts: Arc<std::sync::Mutex<Vec<String>>>,
        ) -> Self {
            Self {
                tool_calls,
                content: content.into(),
                seen_system_prompts,
            }
        }

        fn record_system_prompt(&self, request: &GenerationRequest) {
            if let Some(system_prompt) = request.system_prompt.as_ref() {
                self.seen_system_prompts
                    .lock()
                    .unwrap()
                    .push(system_prompt.clone());
            }
        }
    }

    #[async_trait]
    impl LlmProvider for RecordingToolCallProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.record_system_prompt(&request);
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response);
            }
            Ok(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: self.tool_calls.clone(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok(format!("mock: {}", self.content))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.record_system_prompt(&request);
            if let Some(response) = maybe_memory_promotion_response(&request) {
                return Ok(response_stream(response));
            }
            Ok(response_stream(GenerationResponse {
                content: self.content.clone(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: self.tool_calls.clone(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            }))
        }

        fn provider_name(&self) -> &'static str {
            "recording_tool_call_mock"
        }
    }

    #[tokio::test]
    async fn test_run_turn_includes_runtime_recall_bundle_for_identity_query() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let memory_dir = definition_root.join("memory-store");
        crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("USER.md"),
            "# User Memory\n- Favorite runtime marker: ALAN_IDENTITY_RECALL\n",
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_IDENTITY_RECALL",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.store_dir = Some(memory_dir);
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "What is my favorite runtime marker?",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let request_prompt = system_prompts
            .iter()
            .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
            .expect("expected runtime recall bundle prompt");
        assert!(request_prompt.contains("## Runtime Recall Bundle"));
        assert!(request_prompt.contains("/memory/USER.md"));
        assert!(request_prompt.contains("ALAN_IDENTITY_RECALL"));
    }

    #[tokio::test]
    async fn test_run_turn_includes_runtime_recall_bundle_for_continuity_query() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let memory_dir = definition_root.join("memory-store");
        crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("handoffs/LATEST.md"),
            "# Latest Handoff\n- Continuity marker: ALAN_CONTINUITY_RECALL\n",
        )
        .unwrap();
        std::fs::create_dir_all(memory_dir.join("episodic/2026/04/15")).unwrap();
        std::fs::write(
            memory_dir.join("episodic/2026/04/15/process-1.md"),
            "# Agent Process Activity\n- Continuity marker: ALAN_CONTINUITY_RECALL\n",
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_CONTINUITY_RECALL",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.store_dir = Some(memory_dir);
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "What was the previous Agent Process doing?",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let request_prompt = system_prompts
            .iter()
            .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
            .expect("expected runtime recall bundle prompt");
        assert!(request_prompt.contains("## Runtime Recall Bundle"));
        assert!(request_prompt.contains("/memory/handoffs/LATEST.md"));
        assert!(request_prompt.contains("/memory/episodic/2026/04/15/process-1.md"));
        assert!(request_prompt.contains("ALAN_CONTINUITY_RECALL"));
    }

    #[tokio::test]
    async fn test_run_turn_includes_runtime_recall_bundle_for_recent_query_fallback() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let memory_dir = definition_root.join("memory-store");
        crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
        std::fs::create_dir_all(memory_dir.join("episodic/2026/04/16")).unwrap();
        for index in 1..=4 {
            std::fs::write(
                memory_dir.join(format!("topics/recent-match-{index}.md")),
                format!("# Topic Note\nwe did document topic match {index}\n"),
            )
            .unwrap();
        }
        std::fs::write(
            memory_dir.join("daily/2026-04-16.md"),
            "## 2026-04-16\nALAN_RECENT_RECALL\n",
        )
        .unwrap();
        for index in 1..=4 {
            std::fs::write(
                memory_dir.join(format!("episodic/2026/04/16/process-{index}.md")),
                format!("# Agent Process Activity\nALAN_RECENT_RECALL_{index}\n"),
            )
            .unwrap();
        }

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "ALAN_RECENT_RECALL_4",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.store_dir = Some(memory_dir);
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("What did we do yesterday?")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let request_prompt = system_prompts
            .iter()
            .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
            .expect("expected runtime recall bundle prompt");
        assert!(request_prompt.contains("## Runtime Recall Bundle"));
        assert!(request_prompt.contains("/memory/daily/2026-04-16.md"));
        assert!(request_prompt.contains("/memory/episodic/2026/04/16/process-4.md"));
        assert!(request_prompt.contains("ALAN_RECENT_RECALL_4"));
        assert!(!request_prompt.contains("/memory/topics/recent-match-4.md"));
    }

    #[tokio::test]
    async fn test_run_turn_pre_turn_compaction_accounts_for_runtime_recall_budget() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let memory_dir = definition_root.join("memory-store");
        crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("USER.md"),
            format!(
                "# User Memory\n- Favorite runtime marker: {}\n",
                "ALAN_PRETURN_RECALL ".repeat(80)
            ),
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "COMPACTED_FOR_RECALL",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.store_dir = Some(memory_dir.clone());
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        state.runtime_config.compaction_keep_last = 2;
        state.runtime_config.compaction_trigger_messages = usize::MAX;
        state.runtime_config.compaction_soft_trigger_ratio = 1.0;
        state.runtime_config.compaction_hard_trigger_ratio = 1.0;
        for idx in 0..3 {
            state
                .machine
                .add_user_message(&format!("Earlier user context {idx} {}", "u".repeat(220)));
            state.machine.add_assistant_message(
                &format!("Earlier assistant context {idx} {}", "a".repeat(220)),
                None,
            );
        }

        let user_input = vec![ContentPart::text("What is my favorite runtime marker?")];
        let turn_recall_bundle = crate::runtime::memory_recall::build_turn_recall_bundle(
            Some(memory_dir.as_path()),
            Some(&user_input),
        );
        let pending_prompt_tokens =
            estimate_pending_turn_prompt_tokens(Some(&user_input), turn_recall_bundle.as_deref());
        assert!(pending_prompt_tokens > 0);

        let base_prompt_tokens = state.machine.tape.estimated_prompt_tokens();
        state.runtime_config.context_window_tokens =
            (base_prompt_tokens + pending_prompt_tokens - 1) as u32;

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(user_input),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(state.machine.tape.summary(), Some("COMPACTED_FOR_RECALL"));

        let system_prompts = seen_system_prompts.lock().unwrap();
        assert_eq!(system_prompts.len(), 2);
        let request_prompt = system_prompts
            .iter()
            .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
            .expect("expected runtime recall bundle prompt");
        assert!(request_prompt.contains("## Runtime Recall Bundle"));
        assert!(request_prompt.contains("ALAN_PRETURN_RECALL"));
        assert_eq!(state.turn_state.drain_deferred_runtime_actions().len(), 1);
    }

    #[tokio::test]
    async fn test_run_turn_omits_runtime_recall_bundle_when_memory_disabled() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let memory_dir = definition_root.join("memory-store");
        crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("USER.md"),
            "# User Memory\n- Favorite runtime marker: ALAN_DISABLED_RECALL\n",
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            Vec::new(),
            "ok",
            seen_system_prompts.clone(),
        ));
        state.core_config.memory.store_dir = Some(memory_dir);
        state.core_config.memory.enabled = false;
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text(
                "What is my favorite runtime marker?",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let request_prompt = system_prompts.last().expect("expected system prompt");
        assert!(!request_prompt.contains("## Runtime Recall Bundle"));
        assert!(!request_prompt.contains("ALAN_DISABLED_RECALL"));
    }

    #[tokio::test]
    async fn test_maybe_compact_mid_turn_accounts_for_runtime_prompt_overhead() {
        let mut state = create_test_state_with_provider(ContentMockProvider::new(
            "MID_TURN_COMPACTION_SUMMARY",
        ));
        state.runtime_config.compaction_keep_last = 2;
        state.runtime_config.compaction_trigger_messages = usize::MAX;
        state.runtime_config.compaction_soft_trigger_ratio = 1.0;
        state.runtime_config.compaction_hard_trigger_ratio = 1.0;
        for idx in 0..3 {
            state
                .machine
                .add_user_message(&format!("Mid-turn user context {idx} {}", "u".repeat(220)));
            state.machine.add_assistant_message(
                &format!("Mid-turn assistant context {idx} {}", "a".repeat(220)),
                None,
            );
        }

        let pending_guardrail_instruction = format!(
            "Retry with a corrected answer and preserve tool intent.\n{}",
            "guardrail-overhead ".repeat(80)
        );
        let additional_prompt_tokens = estimate_request_prompt_overhead_tokens(
            None,
            Some(pending_guardrail_instruction.as_str()),
        );
        assert!(additional_prompt_tokens > 0);

        let base_prompt_tokens = state.machine.tape.estimated_prompt_tokens();
        state.runtime_config.context_window_tokens =
            (base_prompt_tokens + additional_prompt_tokens - 1) as u32;

        let cancel = CancellationToken::new();
        let mut emit = |_event: Event| async {};
        let result = maybe_compact_mid_turn_if_needed(
            &mut state,
            &mut emit,
            &cancel,
            additional_prompt_tokens,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(
            state.machine.tape.summary(),
            Some("MID_TURN_COMPACTION_SUMMARY")
        );
        assert_eq!(state.turn_state.compactions_this_turn(), 1);
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_preserves_active_skill_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let skill_dir = definition_root.join("skills/release-check");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
            seen_system_prompts.clone(),
        ));
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
            "please use $release-check for this task",
        )]));
        state
            .turn_state
            .set_active_skills(prior_prompt.active_skills);
        state
            .machine
            .add_user_message("continue the prior approval flow");

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            None,
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let resumed_prompt = system_prompts.last().expect("expected system prompt");
        assert!(resumed_prompt.contains("## Skill: Release Check"));
        assert!(resumed_prompt.contains("Use this skill when asked."));

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "release-check");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "May require write approval."
        );
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_with_steer_preserves_active_skill_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let skill_dir = definition_root.join("skills/release-check");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
            seen_system_prompts.clone(),
        ));
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
            "please use $release-check for this task",
        )]));
        state
            .turn_state
            .set_active_skills(prior_prompt.active_skills);

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            Some(vec![ContentPart::text(
                "steer: tighten the approval explanation",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let resumed_prompt = system_prompts.last().expect("expected system prompt");
        assert!(resumed_prompt.contains("## Skill: Release Check"));
        assert!(resumed_prompt.contains("Use this skill when asked."));

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "release-check");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "May require write approval."
        );
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_without_prior_active_skills_can_activate_skill_from_steer() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let skill_dir = definition_root.join("skills/release-check");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
            seen_system_prompts.clone(),
        ));
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            Some(vec![ContentPart::text(
                "steer: please use $release-check for this task",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let resumed_prompt = system_prompts.last().expect("expected system prompt");
        assert!(resumed_prompt.contains("## Skill: Release Check"));
        assert!(resumed_prompt.contains("Use this skill when asked."));

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "release-check");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "May require write approval."
        );
    }

    #[tokio::test]
    async fn test_run_turn_resume_turn_with_steer_can_add_new_skill_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");

        let release_skill_dir = definition_root.join("skills/release-check");
        std::fs::create_dir_all(&release_skill_dir).unwrap();
        std::fs::write(
            release_skill_dir.join("SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this release skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            release_skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
        )
        .unwrap();

        let audit_skill_dir = definition_root.join("skills/safety-audit");
        std::fs::create_dir_all(&audit_skill_dir).unwrap();
        std::fs::write(
            audit_skill_dir.join("SKILL.md"),
            r#"---
name: Safety Audit
description: Review risky operations for safety concerns
---

# Instructions
Use this safety skill when asked.
"#,
        )
        .unwrap();
        std::fs::write(
            audit_skill_dir.join("skill.yaml"),
            r#"
runtime:
  permission_hints:
    - "May require network approval."
"#,
        )
        .unwrap();

        let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
            vec![ToolCall {
                id: Some("call_1".to_string()),
                name: "request_confirmation".to_string(),
                arguments: json!({
                    "checkpoint_type": "test",
                    "summary": "Confirm risky action"
                }),
            }],
            "",
            seen_system_prompts.clone(),
        ));
        state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

        let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
            "please use $release-check for this task",
        )]));
        state
            .turn_state
            .set_active_skills(prior_prompt.active_skills);

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::ResumeTurn,
            Some(vec![ContentPart::text(
                "steer: also use $safety-audit before approving this",
            )]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());

        let system_prompts = seen_system_prompts.lock().unwrap();
        let resumed_prompt = system_prompts.last().expect("expected system prompt");
        assert!(resumed_prompt.contains("## Skill: Release Check"));
        assert!(resumed_prompt.contains("Use this release skill when asked."));
        assert!(resumed_prompt.contains("## Skill: Safety Audit"));
        assert!(resumed_prompt.contains("Use this safety skill when asked."));

        let confirmation = events.into_iter().find_map(|event| match event {
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                payload,
                ..
            } => Some(payload),
            _ => None,
        });
        let confirmation = confirmation.expect("expected confirmation yield");
        let hints = confirmation["details"]["skill_permission_hints"]
            .as_array()
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 2);
        let skill_ids: std::collections::BTreeSet<String> = hints
            .iter()
            .filter_map(|hint| {
                hint.get("skill_id")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            })
            .collect();
        assert_eq!(
            skill_ids,
            std::collections::BTreeSet::from([
                "release-check".to_string(),
                "safety-audit".to_string(),
            ])
        );
    }

    #[tokio::test]
    async fn test_run_turn_llm_error() {
        // Use error provider
        struct ErrorMockProvider;

        #[async_trait]
        impl LlmProvider for ErrorMockProvider {
            async fn generate(
                &mut self,
                _request: GenerationRequest,
            ) -> anyhow::Result<GenerationResponse> {
                Err(anyhow::anyhow!("LLM error"))
            }

            async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
                Err(anyhow::anyhow!("LLM error"))
            }

            async fn generate_stream(
                &mut self,
                _request: GenerationRequest,
            ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
                Err(anyhow::anyhow!("LLM error"))
            }

            fn provider_name(&self) -> &'static str {
                "error_mock"
            }
        }

        let mut state = create_test_state_with_provider(ErrorMockProvider);
        let cancel = CancellationToken::new();

        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test input")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));

        // Should have error event
        let has_error = events.iter().any(
            |e| matches!(e, Event::Error { message, .. } if message.contains("LLM request failed")),
        );
        assert!(has_error, "Expected Error event for LLM failure");
    }

    #[tokio::test]
    async fn test_streaming_mode_uses_request_response_generation() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(PanicOnStreamProvider {
            content: "final response through request response".to_string(),
            generate_calls: Arc::clone(&generate_calls),
        });
        state.runtime_config.streaming_mode = crate::config::StreamingMode::On;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test streaming config")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(generate_calls.load(Ordering::SeqCst), 1);
        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(emitted_text, "final response through request response");
    }

    #[tokio::test]
    async fn test_namespace_live_turn_generation_retries_transient_stream_failure() {
        let generate_calls = Arc::new(AtomicUsize::new(0));
        let mut state = create_test_state_with_provider(TransientStreamFailureProvider {
            generate_calls: Arc::clone(&generate_calls),
        });
        state.runtime_config.llm_request_timeout_secs = 5;

        let cancel = CancellationToken::new();
        let mut events = vec![];
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let result = run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Test transient stream retry")]),
            &mut emit,
            &cancel,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), TurnExecutionOutcome::Finished));
        assert_eq!(generate_calls.load(Ordering::SeqCst), 2);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::Error { .. })),
            "transient stream failure should retry without surfacing an error: {events:?}"
        );
        let emitted_text = events
            .iter()
            .filter_map(|event| match event {
                Event::TextDelta { chunk, .. } if !chunk.is_empty() => Some(chunk.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(emitted_text, "Recovered after retry.");
    }
}
