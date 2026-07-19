use alan_agent_protocol::{CompactionOutcome, Event};
use anyhow::{Context, Result};
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::llm::build_generation_request;

use super::compaction::{CompactionRequest, maybe_compact_context_with_cancel};
use super::loop_guard::ToolLoopGuard;
use super::response_guardrails::{
    AssistantDraft, GuardrailDecision, ResponseGuardrailContext, ResponseGuardrails,
};
use super::tool_batch::{ToolBatchOrchestratorOutcome, ToolOrchestratorInputs};
use super::transition::{RuntimeLoopState, orchestrate_tool_batch};
use super::turn_driver::TurnInputBroker;
use super::turn_memory::{FinalizeTurnMemoryRequest, finalize_turn_memory_best_effort};
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

async fn turn_tool_definitions(
    include_runtime_delegated_tool: bool,
    tool_execution: &super::transition::NamespaceToolExecution,
) -> anyhow::Result<(
    Vec<super::ToolPackageManifest>,
    Vec<crate::llm::ToolDefinition>,
)> {
    let tool_packages = tool_execution.discover_packages().await?;
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
    let runtime = super::transition::compaction_runtime(state);
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

fn build_domain_prompt_with_skills(
    prompt_cache: &mut super::prompt_cache::PromptAssemblyCache,
    user_input: Option<&[crate::tape::ContentPart]>,
    active_skills: Option<&[crate::skills::ActiveSkillEnvelope]>,
) -> super::prompt_cache::PromptAssemblyResult {
    match active_skills {
        Some(active_skills) => prompt_cache.build_with_active_skills(active_skills, user_input),
        None => prompt_cache.build(user_input),
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
        state.machine.reset_auto_mid_turn_compaction_state();
        super::ui_surfaces::turn_started(&state.agent_files())
            .await
            .context("write turn-start UI state")?;
        emit(Event::TurnStarted {}).await;
    } else {
        super::ui_surfaces::resumed(&state.agent_files())
            .await
            .context("write resumed turn UI state")?;
    }

    let agent_files = state.agent_files();
    if matches!(turn_kind, TurnRunKind::NewTurn) && user_input.is_none() {
        let input = agent_files
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
    if check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel).await? {
        return Ok(TurnExecutionOutcome::Finished);
    }

    if matches!(turn_kind, TurnRunKind::NewTurn) {
        state.machine.begin_turn(state.machine.messages().len());
    } else if user_input.is_some() {
        state.machine.note_resumed_user_input();
    }
    if let Some(user_input) = user_input {
        state.machine.add_user_message_parts(user_input);
    }

    // Resume turns keep the same active skill envelopes for the logical turn.
    // Current user input can still add new skills via prompt assembly merge logic.
    let resumed_active_skills = matches!(turn_kind, TurnRunKind::ResumeTurn)
        .then(|| state.machine.active_skills().to_vec())
        .filter(|active_skills| !active_skills.is_empty());
    let prompt_build = build_domain_prompt_with_skills(
        &mut state.prompt_cache,
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
        .machine
        .set_active_skills(prompt_build.active_skills.clone());
    let active_skill_ids = prompt_build
        .active_skills
        .iter()
        .map(|skill| skill.metadata.id.clone())
        .collect::<Vec<_>>();
    let _domain_prompt = prompt_build.domain_prompt;
    let system_prompt = prompt_build.system_prompt;

    let include_runtime_delegated_tool = state.prompt_cache.supports_delegated_skill_invocation();
    let tool_execution = state.tool_execution();
    let (tool_packages, tools) =
        turn_tool_definitions(include_runtime_delegated_tool, &tool_execution).await?;
    let tool_names = tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect::<Vec<_>>();
    let llm_generation = state.namespace_generation();
    let generation = NamespaceTurnGeneration::load(&llm_generation).await;
    let initial_provider_capabilities = generation.capabilities();
    let turn_request_controls = crate::resolve_turn_request_controls(
        &state.core_config,
        initial_provider_capabilities,
        state.runtime_config.request_control_intent,
        state.machine.active_turn_request_control_intent(),
    )?;
    let model = state.core_config.effective_model().to_string();
    let memory_enabled = state.core_config.memory.enabled;
    let context_items = state.machine.context_items().to_vec();
    let context_delta = state.machine.last_context_delta().clone();
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
    let mut tool_loop_guard =
        ToolLoopGuard::new(max_tool_loops, state.runtime_config.tool_repeat_limit);
    let mut response_guardrails = ResponseGuardrails::default();
    let mut pending_guardrail_instruction: Option<String> = None;
    loop {
        if check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel).await? {
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

        let prompt_view = state.machine.prompt_view();
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
            .generate(&llm_generation, request, llm_request_timeout_secs, cancel)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                if cancel.is_cancelled()
                    && check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel).await?
                {
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
            super::ui_surfaces::warning(&state.agent_files(), warning.clone())
                .await
                .context("write provider warning UI state")?;
            emit(Event::Warning {
                message: warning.clone(),
            })
            .await;
        }

        let tool_calls = normalize_tool_calls(response.tool_calls);

        let tool_execution = state.tool_execution();
        let guardrail_context =
            ResponseGuardrailContext::from_machine(&state.machine, &tool_execution, &tool_packages);
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
                super::ui_surfaces::warning(&state.agent_files(), message.clone())
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
                if check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel).await? {
                    return Ok(TurnExecutionOutcome::Finished);
                }
                continue;
            }
        }

        if let Some(ref thinking) = response.thinking
            && !thinking.is_empty()
        {
            super::ui_surfaces::thinking(&state.agent_files(), thinking)
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
            agent_files
                .write_assistant_output(&response.content)
                .await
                .context("write namespace assistant output")?;
            agent_files
                .write_turn_tape_state(namespace_input_text.as_deref(), &response.content)
                .await
                .context("write namespace turn tape state")?;
        }

        if !tool_calls.is_empty() {
            match orchestrate_tool_batch(
                &mut tool_loop_guard,
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
                    if check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel).await? {
                        return Ok(TurnExecutionOutcome::Finished);
                    }
                }
                ToolBatchOrchestratorOutcome::PauseTurn => return Ok(TurnExecutionOutcome::Paused),
                ToolBatchOrchestratorOutcome::EndTurn { surfaces_refreshed } => {
                    if !cancel.is_cancelled() {
                        let memory_runtime = super::transition::turn_memory_runtime(state);
                        finalize_turn_memory_best_effort(
                            memory_runtime,
                            FinalizeTurnMemoryRequest {
                                surfaces_refreshed,
                                surfaces_context: "turn-ended-after-tool-batch",
                                promotion_context: "after tool-driven end turn",
                            },
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
            agent_files
                .write_assistant_output(fallback_text)
                .await
                .context("write namespace fallback assistant output")?;
            agent_files
                .write_turn_tape_state(namespace_input_text.as_deref(), fallback_text)
                .await
                .context("write namespace fallback turn tape state")?;
            let memory_runtime = super::transition::turn_memory_runtime(state);
            finalize_turn_memory_best_effort(
                memory_runtime,
                FinalizeTurnMemoryRequest {
                    surfaces_refreshed: false,
                    surfaces_context: "fallback-turn-completed",
                    promotion_context: "after fallback turn",
                },
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
            super::ui_surfaces::turn_completed(&state.agent_files(), false)
                .await
                .context("write fallback turn completion UI state")?;
            return Ok(TurnExecutionOutcome::Finished);
        }

        let memory_runtime = super::transition::turn_memory_runtime(state);
        finalize_turn_memory_best_effort(
            memory_runtime,
            FinalizeTurnMemoryRequest {
                surfaces_refreshed: false,
                surfaces_context: "turn-completed",
                promotion_context: "after completed turn",
            },
        )
        .await;
        emit_task_completed_success(&agent_files, emit, "Task completed").await?;
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

#[cfg(test)]
mod tests;
