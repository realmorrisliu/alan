//! Generation and Tool-loop execution owned by the accepted-submission transition.

use alan_agent_protocol::Event;
use anyhow::{Context, Result};
use std::{sync::Arc, time::Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::llm::build_generation_request;
use crate::runtime::compaction::CompactionRequest;
use crate::runtime::loop_guard::ToolLoopGuard;
use crate::runtime::response_guardrails::{
    AssistantDraft, GuardrailDecision, ResponseGuardrailContext, ResponseGuardrails,
};
use crate::runtime::tool_batch::{ToolBatchOrchestratorOutcome, ToolOrchestratorInputs};
use crate::runtime::turn_input::TurnInputBroker;
use crate::runtime::turn_memory::{FinalizeTurnMemoryRequest, finalize_turn_memory_best_effort};
use crate::runtime::turn_support::{
    check_turn_cancelled, emit_task_completed_success, normalize_tool_calls,
};
use crate::runtime::virtual_tools::virtual_tool_definitions;

use super::{
    NamespaceTapeWriter, NamespaceToolExecution, NormalizedToolCall, RuntimeLoopState,
    TurnActivityState, TurnExecutionOutcome, TurnRunKind, compaction_runtime,
    orchestrate_tool_batch_internal, turn_memory_runtime,
};

mod accepted_output;
mod live_stream;
mod mid_turn_compaction;
mod namespace_generation;
mod response_schema;
mod usage_surface;

#[cfg(test)]
pub(super) use alan_agent_protocol::CompactionOutcome;
pub use response_schema::validate_response_schema;

use live_stream::{LiveTurnStream, guardrail_probe};
use mid_turn_compaction::{
    COMPACTION_TIMEOUT_SECS, maybe_compact_context_with_turn_timeout,
    maybe_compact_mid_turn_if_needed, should_skip_auto_compaction_for_responses_continuation,
};
use namespace_generation::NamespaceTurnGeneration;

fn process_directory_instruction(execution: &NamespaceToolExecution) -> Option<String> {
    let cwd = execution.default_cwd()?;
    let path = serde_json::json!(cwd.to_str()?);
    Some(format!(
        "Current selected Process directory: {path}\nResolve relative Tool paths here; historical directories may be obsolete. This selection is context, not a grant; Tools still enforce live authority."
    ))
}

fn service_activation_instruction(
    authority: Option<&Arc<dyn crate::runtime::service_activation::ServiceActivationAuthority>>,
) -> Option<String> {
    let packages = authority?.list_available_fileservers();
    crate::runtime::service_activation::render_service_activation_context(&packages)
}

fn estimate_runtime_system_instruction_tokens(instruction: &str) -> usize {
    crate::tape::estimate_text_tokens(instruction).saturating_add(1)
}

fn estimate_request_prompt_overhead_tokens(
    turn_recall_bundle: Option<&str>,
    pending_guardrail_instruction: Option<&str>,
    directory_instruction: Option<&str>,
    service_activation_instruction: Option<&str>,
) -> usize {
    turn_recall_bundle
        .into_iter()
        .chain(pending_guardrail_instruction)
        .chain(directory_instruction)
        .chain(service_activation_instruction)
        .map(estimate_runtime_system_instruction_tokens)
        .sum()
}

fn estimate_pending_turn_prompt_tokens(
    pending_user_input: Option<&[crate::tape::ContentPart]>,
    turn_recall_bundle: Option<&str>,
    directory_instruction: Option<&str>,
    service_activation_instruction: Option<&str>,
) -> usize {
    pending_user_input
        .map(crate::tape::estimate_user_message_tokens)
        .unwrap_or(0)
        .saturating_add(estimate_request_prompt_overhead_tokens(
            turn_recall_bundle,
            None,
            directory_instruction,
            service_activation_instruction,
        ))
}

async fn turn_tool_definitions(
    include_runtime_delegated_tool: bool,
    include_subagent_spawn: bool,
    include_service_activation: bool,
    tool_execution: &NamespaceToolExecution,
) -> anyhow::Result<(
    Vec<crate::runtime::ToolPackageManifest>,
    Vec<crate::llm::ToolDefinition>,
)> {
    let tool_packages = tool_execution.discover_packages().await?;
    let mut tools = tool_packages
        .iter()
        .map(|package| package.model_definition())
        .collect::<Vec<_>>();
    tools.extend(virtual_tool_definitions(
        include_runtime_delegated_tool,
        include_subagent_spawn,
        include_service_activation,
    ));
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
    if let Some(cause) = error.downcast_ref::<crate::retry::GenerationCause>() {
        return format!("Namespace LLM request failed: {cause}");
    }
    format!("Namespace LLM request failed: {error}")
}

fn build_domain_prompt_with_skills(
    prompt_cache: &mut crate::runtime::prompt_cache::PromptAssemblyCache,
    user_input: Option<&[crate::tape::ContentPart]>,
    active_skills: Option<&[crate::skills::ActiveSkillEnvelope]>,
) -> crate::runtime::prompt_cache::PromptAssemblyResult {
    match active_skills {
        Some(active_skills) => prompt_cache.build_with_active_skills(active_skills, user_input),
        None => prompt_cache.build(user_input),
    }
}

/// Run a single agent turn
#[cfg(test)]
pub(in crate::runtime) async fn run_turn_with_cancel<E, F>(
    state: &mut RuntimeLoopState,
    turn_kind: TurnRunKind,
    user_input: Option<Vec<crate::tape::ContentPart>>,
    emit: &mut E,
    cancel: &CancellationToken,
    steering_broker: Option<&TurnInputBroker>,
) -> Result<TurnExecutionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let writer = state.agent_files().begin_tape_generation().await?;
    let result = run_turn_with_writer(
        state,
        turn_kind,
        user_input,
        emit,
        cancel,
        steering_broker,
        &writer,
    )
    .await;
    let closed = writer.finish().await;
    result.and_then(|outcome| closed.map(|()| outcome))
}

pub(super) async fn run_turn_with_writer<E, F>(
    state: &mut RuntimeLoopState,
    turn_kind: TurnRunKind,
    mut user_input: Option<Vec<crate::tape::ContentPart>>,
    emit: &mut E,
    cancel: &CancellationToken,
    steering_broker: Option<&TurnInputBroker>,
    writer: &super::NamespaceTapeWriter,
) -> Result<TurnExecutionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if matches!(turn_kind, TurnRunKind::NewTurn) {
        state.machine.reset_auto_mid_turn_compaction_state();
        crate::runtime::ui_surfaces::turn_started(&state.agent_files())
            .await
            .context("write turn-start UI state")?;
        usage_surface::begin_turn(
            &state.agent_files(),
            u64::from(state.runtime_config.context_window_tokens),
        )
        .await;
        crate::runtime::smart_approval::publish_state_system1(state).await;
        emit(Event::TurnStarted {}).await;
    } else {
        crate::runtime::ui_surfaces::resumed(&state.agent_files())
            .await
            .context("write resumed turn UI state")?;
    }

    let agent_files = state.agent_files();
    let host_mount_requests = state.environment.host_mount_requests();
    if matches!(turn_kind, TurnRunKind::NewTurn) && user_input.is_none() {
        let input = agent_files
            .read_next_input()
            .await
            .context("read next namespace agent input")?;
        user_input = Some(vec![crate::tape::ContentPart::text(input)]);
    }

    let user_input_for_skills = user_input.clone();
    if let Some(input) = user_input_for_skills
        .as_deref()
        .map(crate::tape::parts_to_text)
        .filter(|input| !input.trim().is_empty())
    {
        writer
            .append_record(
                "user",
                &input,
                state.machine.current_submission_id(),
                state.machine.related_submission_ids(),
            )
            .await?;
    }
    let tool_execution = state.tool_execution();
    let turn_recall_bundle = if state.core_config.memory.enabled {
        let project_cwd = tool_execution
            .default_cwd()
            .map(|p| p.to_string_lossy().into_owned());
        let memory_bootstrap_section = state.prompt_cache.current_memory_bootstrap_section();
        crate::runtime::memory_recall::build_turn_recall_bundle_for_turn(
            state.core_config.memory.store_dir.as_deref(),
            user_input_for_skills.as_deref(),
            project_cwd.as_deref(),
            memory_bootstrap_section.as_deref(),
        )
    } else {
        None
    };

    let llm_generation = state.namespace_generation();
    let generation = NamespaceTurnGeneration::load(&llm_generation).await;
    generation.ensure_callable()?;

    let initial_directory_instruction = process_directory_instruction(&tool_execution);
    let initial_service_instruction =
        service_activation_instruction(state.environment.service_activation());

    if !should_skip_auto_compaction_for_responses_continuation(state) {
        let compaction_request = CompactionRequest::automatic_pre_turn()
            .with_additional_prompt_tokens(estimate_pending_turn_prompt_tokens(
                user_input_for_skills.as_deref(),
                turn_recall_bundle.as_deref(),
                initial_directory_instruction.as_deref(),
                initial_service_instruction.as_deref(),
            ));
        let compaction = Box::pin(maybe_compact_context_with_turn_timeout(
            state,
            emit,
            &compaction_request,
            cancel,
            tokio::time::Duration::from_secs(COMPACTION_TIMEOUT_SECS),
        ))
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
    if check_turn_cancelled(
        &mut state.machine,
        &agent_files,
        &host_mount_requests,
        emit,
        cancel,
    )
    .await?
    {
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
    let live_async_workstreams = state.environment.async_workstreams().list();
    let live_project_context = (!state.runtime_config.ephemeral_context)
        .then_some(state.runtime_config.project_startup_context.as_ref())
        .flatten();
    let refreshed_ledger = super::super::project_startup::render_live_project_and_async_context(
        live_project_context,
        &live_async_workstreams,
    );
    state
        .prompt_cache
        .set_project_ledger_section(refreshed_ledger);
    let prompt_build = build_domain_prompt_with_skills(
        &mut state.prompt_cache,
        user_input_for_skills.as_deref(),
        resumed_active_skills.as_deref(),
    );
    state.publish_ensured_skills().await?;
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
    let base_system_prompt = prompt_build.system_prompt;

    let include_runtime_delegated_tool = state.prompt_cache.supports_delegated_skill_invocation();
    let include_subagent_tool =
        include_runtime_delegated_tool && state.runtime_config.ad_hoc_subagent_spawn_enabled;
    let include_service_activation = state.environment.service_activation().is_some();
    let (tool_packages, tools) = turn_tool_definitions(
        include_runtime_delegated_tool,
        include_subagent_tool,
        include_service_activation,
        &tool_execution,
    )
    .await?;
    let tool_names = tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect::<Vec<_>>();
    let initial_provider_capabilities = generation.capabilities();
    let captured = state
        .environment
        .active_binding
        .read()
        .expect("active binding snapshot")
        .clone();
    let turn_request_controls = if let Some((binding, _)) = &captured {
        binding.request_controls.clone()
    } else {
        crate::resolve_turn_request_controls(
            &state.core_config,
            initial_provider_capabilities,
            state.runtime_config.request_control_intent,
            state.machine.active_turn_request_control_intent(),
        )?
    };
    let effective_provider = captured
        .as_ref()
        .map(|(_, callable)| callable.identity.provider.as_str())
        .unwrap_or_else(|| state.core_config.llm_provider.as_str());
    let model = captured
        .as_ref()
        .map(|(_, callable)| callable.identity.model.clone())
        .unwrap_or_else(|| state.core_config.effective_model().to_string());
    let system_prompt = crate::prompts::format_system_prompt_for_model(
        &base_system_prompt,
        effective_provider,
        &model,
    );
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
    // Mirrors the single regeneration `ResponseGuardrails::default()` allows per turn.
    let mut guardrail_regenerated = false;
    let mut schema_retried = false;
    let mut pending_guardrail_instruction: Option<String> = None;
    loop {
        if check_turn_cancelled(
            &mut state.machine,
            &agent_files,
            &host_mount_requests,
            emit,
            cancel,
        )
        .await?
        {
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

        let directory_instruction = process_directory_instruction(&tool_execution);
        let service_instruction =
            service_activation_instruction(state.environment.service_activation());
        let prompt_view = state.machine.prompt_view();
        let estimated_prompt_tokens =
            prompt_view
                .estimated_tokens
                .saturating_add(estimate_request_prompt_overhead_tokens(
                    turn_recall_bundle.as_deref(),
                    pending_guardrail_instruction.as_deref(),
                    directory_instruction.as_deref(),
                    service_instruction.as_deref(),
                ));
        let context_revision = prompt_view.reference_context.revision;
        let messages = prompt_view.messages;
        let mut llm_messages = crate::llm::project_messages(&messages, true);
        let mut trailing_context_parts = Vec::new();
        if let Some(instruction) = directory_instruction.as_deref() {
            trailing_context_parts.push(instruction);
        }
        if let Some(instruction) = service_instruction.as_deref() {
            trailing_context_parts.push(instruction);
        }
        if let Some(recall_bundle) = turn_recall_bundle.as_deref() {
            trailing_context_parts.push(recall_bundle);
        }
        if let Some(instruction) = pending_guardrail_instruction.as_deref() {
            trailing_context_parts.push(instruction);
        }
        if !trailing_context_parts.is_empty() {
            llm_messages.push(crate::llm::Message::context(
                trailing_context_parts.join("\n\n"),
            ));
        }
        let llm_tools: Vec<crate::llm::ToolDefinition> = tools
            .iter()
            .map(|t| {
                crate::llm::ToolDefinition::new(&t.name, &t.description)
                    .with_parameters(t.parameters.clone())
            })
            .collect();

        let active_response_schema = state
            .machine
            .active_turn_response_schema()
            .cloned()
            .or_else(|| state.runtime_config.response_schema.clone());
        let mut request = build_generation_request(
            Some(system_prompt.clone()),
            llm_messages,
            llm_tools,
            Some(state.runtime_config.temperature),
            Some(state.runtime_config.max_tokens as i32),
        );
        request.response_schema = active_response_schema.clone();
        let request_controls = turn_request_controls.clone();
        let session_key = state
            .machine
            .rollout_path()
            .and_then(|path| path.file_stem())
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| state.process_path());
        request = request
            .with_reasoning_controls(request_controls.reasoning)
            .with_session_key(session_key);

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
        let guardrail_context = Arc::new(ResponseGuardrailContext::from_machine(
            &state.machine,
            &tool_execution,
            &tool_packages,
        ));
        let live = LiveTurnStream::new(
            agent_files.clone(),
            guardrail_probe(Arc::clone(&guardrail_context), !guardrail_regenerated),
        );
        let response = match Box::pin(generation.generate(
            &llm_generation,
            request,
            llm_request_timeout_secs,
            cancel,
            &live,
            emit,
        ))
        .await
        {
            Ok(response) => response,
            Err(error) => {
                accepted_output::abandon(&live, emit).await;
                if let Some(exhausted) =
                    error.downcast_ref::<crate::agent_machine::task_budget::Exhausted>()
                {
                    if crate::runtime::task_budget::try_auto_extend(state, exhausted, cancel, emit)
                        .await?
                    {
                        continue;
                    }
                    return Err(error);
                }
                if error.is::<super::NamespaceOperationUncertainty>() {
                    return Err(error);
                }
                if cancel.is_cancelled()
                    && check_turn_cancelled(
                        &mut state.machine,
                        &agent_files,
                        &host_mount_requests,
                        emit,
                        cancel,
                    )
                    .await?
                {
                    return Ok(TurnExecutionOutcome::Finished);
                }
                log_generation_failure(request_start, &error);
                let message = generation_error_message(&error);
                emit(Event::Error {
                    message: message.clone(),
                    recoverable: true,
                })
                .await;
                return Err(anyhow::anyhow!(message));
            }
        };
        live.check()?;

        if let Some(usage) = response.usage {
            info!(
                prompt_tokens = usage.prompt_tokens,
                completion_tokens = usage.completion_tokens,
                total_tokens = usage.total_tokens,
                reasoning_tokens = ?usage.reasoning_tokens,
                "LLM usage"
            );
            usage_surface::record(
                &agent_files,
                &usage,
                u64::from(state.runtime_config.context_window_tokens),
            )
            .await;
        }
        crate::runtime::task_budget::publish_state(state).await;

        for warning in &response.warnings {
            crate::runtime::ui_surfaces::warning(&state.agent_files(), warning.clone())
                .await
                .context("write provider warning UI state")?;
            emit(Event::Warning {
                message: warning.clone(),
            })
            .await;
        }

        let tool_calls = normalize_tool_calls(response.tool_calls);

        let guardrail_draft = AssistantDraft::new(&response.content, !tool_calls.is_empty());
        match response_guardrails.evaluate(&guardrail_context, &guardrail_draft) {
            GuardrailDecision::Accept => {
                pending_guardrail_instruction = None;
            }
            GuardrailDecision::Recover {
                rule_id, reason, ..
            } if live.text_visible() => {
                // The text already reached `io/output`, which cannot be retracted.
                guardrail_regenerated = true;
                pending_guardrail_instruction = None;
                warn!(rule_id, reason = %reason, "Response guardrail fired after output streamed");
                let message = format!(
                    "Guardrail ({rule_id}): {reason}. The reply had already started streaming, so it was not retried."
                );
                crate::runtime::ui_surfaces::warning(&state.agent_files(), message.clone())
                    .await
                    .context("write guardrail warning UI state")?;
                emit(Event::Warning { message }).await;
            }
            GuardrailDecision::Recover {
                rule_id,
                reason,
                instruction,
            } => {
                guardrail_regenerated = true;
                warn!(
                    rule_id,
                    reason = %reason,
                    "Response guardrail triggered for assistant output"
                );
                let message =
                    format!("Guardrail recovered ({rule_id}): {reason}. Retrying before output.");
                accepted_output::abandon(&live, emit).await;
                crate::runtime::ui_surfaces::warning(&state.agent_files(), message.clone())
                    .await
                    .context("write guardrail warning UI state")?;
                emit(Event::Warning { message }).await;
                pending_guardrail_instruction = Some(instruction);
                Box::pin(maybe_compact_mid_turn_if_needed(
                    state,
                    emit,
                    cancel,
                    estimate_request_prompt_overhead_tokens(
                        turn_recall_bundle.as_deref(),
                        pending_guardrail_instruction.as_deref(),
                        directory_instruction.as_deref(),
                        service_instruction.as_deref(),
                    ),
                ))
                .await?;
                if check_turn_cancelled(
                    &mut state.machine,
                    &agent_files,
                    &host_mount_requests,
                    emit,
                    cancel,
                )
                .await?
                {
                    return Ok(TurnExecutionOutcome::Finished);
                }
                continue;
            }
        }

        if response_schema::enforce_or_recover(
            active_response_schema.as_ref(),
            !tool_calls.is_empty(),
            &response.content,
            &live,
            &mut pending_guardrail_instruction,
            &mut schema_retried,
            emit,
        )
        .await?
        {
            continue;
        }

        let streamed_live = accepted_output::publish_accepted(
            &live,
            &agent_files,
            &response.content,
            response.thinking.as_deref(),
            emit,
        )
        .await?;

        let assistant_message_persisted = if !tool_calls.is_empty() {
            let machine_tool_calls: Vec<crate::tape::ToolRequest> = tool_calls
                .iter()
                .map(|tc| crate::tape::ToolRequest {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: if tc.name == "request_mount" {
                        crate::runtime::mount_request_tool::durable_mount_request_arguments(
                            &tc.arguments,
                        )
                    } else {
                        tc.arguments.clone()
                    },
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
            if !streamed_live {
                agent_files
                    .write_assistant_output(&response.content)
                    .await
                    .context("write namespace assistant output")?;
            }
            writer
                .append_record(
                    "assistant",
                    &response.content,
                    state.machine.current_submission_id(),
                    state.machine.related_submission_ids(),
                )
                .await
                .context("write namespace turn tape state")?;
        }

        if !tool_calls.is_empty() {
            match Box::pin(orchestrate_tool_batch(
                &mut tool_loop_guard,
                state,
                &tool_calls,
                ToolOrchestratorInputs {
                    explicit_command: false,
                    cancel,
                    steering_broker,
                },
                writer,
                emit,
            ))
            .await?
            {
                ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. } => {
                    Box::pin(maybe_compact_mid_turn_if_needed(
                        state,
                        emit,
                        cancel,
                        estimate_request_prompt_overhead_tokens(
                            turn_recall_bundle.as_deref(),
                            pending_guardrail_instruction.as_deref(),
                            process_directory_instruction(&tool_execution).as_deref(),
                            service_activation_instruction(state.environment.service_activation())
                                .as_deref(),
                        ),
                    ))
                    .await?;
                    if check_turn_cancelled(
                        &mut state.machine,
                        &agent_files,
                        &host_mount_requests,
                        emit,
                        cancel,
                    )
                    .await?
                    {
                        return Ok(TurnExecutionOutcome::Finished);
                    }
                }
                ToolBatchOrchestratorOutcome::PauseTurn => return Ok(TurnExecutionOutcome::Paused),
                ToolBatchOrchestratorOutcome::EndTurn { surfaces_refreshed } => {
                    if !cancel.is_cancelled() {
                        let memory_runtime = turn_memory_runtime(state);
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
                    if !state.machine.submission_was_cancelled() {
                        check_turn_cancelled(
                            &mut state.machine,
                            &agent_files,
                            &host_mount_requests,
                            emit,
                            cancel,
                        )
                        .await?;
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
            agent_files
                .write_assistant_output(fallback_text)
                .await
                .context("write namespace fallback assistant output")?;
            writer
                .append_record(
                    "assistant",
                    fallback_text,
                    state.machine.current_submission_id(),
                    state.machine.related_submission_ids(),
                )
                .await
                .context("write namespace fallback turn tape state")?;
            let memory_runtime = turn_memory_runtime(state);
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
            if check_turn_cancelled(
                &mut state.machine,
                &agent_files,
                &host_mount_requests,
                emit,
                cancel,
            )
            .await?
            {
                return Ok(TurnExecutionOutcome::Finished);
            }
            emit(Event::TurnCompleted {
                summary: Some("Turn completed with empty response fallback".to_string()),
            })
            .await;
            return Ok(TurnExecutionOutcome::Finished);
        }

        let memory_runtime = turn_memory_runtime(state);
        finalize_turn_memory_best_effort(
            memory_runtime,
            FinalizeTurnMemoryRequest {
                surfaces_refreshed: false,
                surfaces_context: "turn-completed",
                promotion_context: "after completed turn",
            },
        )
        .await;
        if check_turn_cancelled(
            &mut state.machine,
            &agent_files,
            &host_mount_requests,
            emit,
            cancel,
        )
        .await?
        {
            return Ok(TurnExecutionOutcome::Finished);
        }
        emit_task_completed_success(emit, "Task completed").await?;
        return Ok(TurnExecutionOutcome::Finished);
    }
}

#[cfg(test)]
mod tests;

pub(in crate::runtime) async fn orchestrate_tool_batch<E, F>(
    loop_guard: &mut ToolLoopGuard,
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    inputs: ToolOrchestratorInputs<'_>,
    writer: &NamespaceTapeWriter,
    emit: &mut E,
) -> Result<ToolBatchOrchestratorOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    orchestrate_tool_batch_internal(
        loop_guard,
        state,
        tool_calls,
        inputs,
        (None, None),
        writer,
        emit,
    )
    .await
}
pub(super) async fn finalize_replayed_tool_end_turn_best_effort(
    state: &mut RuntimeLoopState,
    cancel: &CancellationToken,
    surfaces_refreshed: bool,
    surfaces_context: &'static str,
    promotion_context: &'static str,
) {
    if !cancel.is_cancelled() {
        let memory_runtime = turn_memory_runtime(state);
        finalize_turn_memory_best_effort(
            memory_runtime,
            FinalizeTurnMemoryRequest {
                surfaces_refreshed,
                surfaces_context,
                promotion_context,
            },
        )
        .await;
    }

    if cancel.is_cancelled() {
        state.machine.mark_submission_cancelled();
    }
    state.machine.set_turn_activity(TurnActivityState::Idle);
}
