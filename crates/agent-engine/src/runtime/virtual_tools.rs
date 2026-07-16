use alan_agent_protocol::{
    AdaptivePresentationHint, ConfirmationYieldPayload, DelegatedSpawnContext, Event, SpawnHandle,
    SpawnLaunchInputs, SpawnRuntimeOverrides, SpawnSpec, StructuredInputKind,
    StructuredInputOption, StructuredInputQuestion, StructuredInputYieldPayload,
};
use anyhow::Result;
use serde_json::json;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

use crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE;
use crate::approval::{PendingConfirmation, append_skill_permission_hints};
use crate::llm::ToolDefinition;
use crate::skills::{DelegatedSkillResult, DelegatedSkillResultStatus};

use super::agent_loop::{NormalizedToolCall, RuntimeLoopState};
use super::child_agents::spawn_child_runtime_cancellable;
use super::child_run_termination_tool::{
    handle_terminate_child_run, terminate_child_run_tool_definition,
};
use super::delegated_child_run::ChildRuntimeResult;
#[cfg(test)]
use super::delegated_child_run::{ChildRuntimeStatus, MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS};
use super::delegated_skill_evidence::{
    build_bounded_delegated_invocation_persistence, persist_delegated_child_evidence,
};
use super::delegated_skill_tool::{
    DEFAULT_DELEGATED_TIMEOUT_SECS, DelegatedSkillInvocationRequest,
    invoke_delegated_skill_tool_definition, parse_delegated_skill_invocation_request,
};
#[cfg(test)]
use super::delegated_skill_tool::{
    MAX_DELEGATED_SKILL_ID_CHARS, MAX_DELEGATED_TARGET_CHARS, MAX_DELEGATED_TASK_CHARS,
};
use super::delegation_capabilities::{
    DelegatedSpawnRejected, classify_delegated_task_requirements,
};
#[cfg(test)]
use super::mount_request_tool::MountRequestAccess;
pub(super) use super::mount_request_tool::parse_mount_request;
use super::mount_request_tool::{handle_request_mount, request_mount_tool_definition};
use super::turn_support::{check_turn_cancelled, tool_result_preview};
pub(super) use super::virtual_tool::VirtualToolOutcome;

type DelegatedSkillSpawnResult<T> = std::result::Result<T, Box<DelegatedSkillResult>>;

pub(super) fn virtual_tool_definitions(include_delegated_skill: bool) -> Vec<ToolDefinition> {
    let mut defs = vec![
        request_confirmation_tool_definition(),
        request_mount_tool_definition(),
        request_user_input_tool_definition(),
        update_plan_tool_definition(),
    ];
    if include_delegated_skill {
        defs.push(invoke_delegated_skill_tool_definition());
        defs.push(terminate_child_run_tool_definition());
    }
    defs
}

pub(super) async fn try_handle_virtual_tool_call<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    if cancel.is_cancelled() && check_turn_cancelled(state, emit, cancel).await? {
        return Ok(VirtualToolOutcome::EndTurn);
    }

    match tool_call.name.as_str() {
        "request_confirmation" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;

            if let Some(mut pending) = parse_confirmation_request(&tool_call.id, tool_arguments) {
                pending.details = append_skill_permission_hints(
                    pending.details,
                    state.turn_state.active_skills(),
                );
                let request_id = state
                    .write_namespace_confirmation_request(&pending)
                    .await?
                    .unwrap_or_else(|| pending.checkpoint_id.clone());
                let pending_payload = json!({
                    "status": "pending_confirmation",
                    "request_id": request_id.clone()
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(true),
                    result_preview: tool_result_preview(&pending_payload),
                    audit: None,
                })
                .await;
                state.machine.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    pending_payload,
                    true,
                );
                state
                    .turn_state
                    .set_confirmation_for_request(request_id.clone(), pending.clone());
                super::ui_surfaces::paused(state.namespace_environment()).await?;
                emit(Event::Yield {
                    request_id,
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    payload: serde_json::to_value(ConfirmationYieldPayload {
                        checkpoint_type: pending.checkpoint_type.clone(),
                        summary: pending.summary.clone(),
                        details: Some(pending.details.clone()),
                        default_option: pending
                            .options
                            .iter()
                            .find(|option| option.as_str() == "approve")
                            .cloned()
                            .or_else(|| pending.options.first().cloned()),
                        options: pending.options.clone(),
                        presentation_hints: vec![],
                    })
                    .unwrap_or_else(|_| json!({})),
                })
                .await;
            } else {
                let error_payload = json!({
                    "status": "invalid_request",
                    "error": "Invalid confirmation request."
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&error_payload),
                    audit: None,
                })
                .await;
                state.machine.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    error_payload,
                    false,
                );
                emit(Event::Error {
                    message: "Invalid confirmation request.".to_string(),
                    recoverable: true,
                })
                .await;
                return Ok(VirtualToolOutcome::EndTurn);
            }
            Ok(VirtualToolOutcome::PauseTurn)
        }
        "request_mount" => handle_request_mount(state, tool_call, tool_arguments, emit).await,
        "request_user_input" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;

            if let Some(request) =
                parse_structured_user_input_request(&tool_call.id, tool_arguments)
            {
                let request_id = state
                    .write_namespace_structured_input_request(&request)
                    .await?
                    .unwrap_or_else(|| request.request_id.clone());
                let pending_payload =
                    json!({"status": "pending_structured_input", "request_id": request_id.clone()});
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(true),
                    result_preview: tool_result_preview(&pending_payload),
                    audit: None,
                })
                .await;
                state.machine.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    pending_payload,
                    true,
                );
                state
                    .turn_state
                    .set_structured_input_for_request(request_id.clone(), request.clone());
                super::ui_surfaces::paused(state.namespace_environment()).await?;
                emit(Event::Yield {
                    request_id,
                    kind: alan_agent_protocol::YieldKind::StructuredInput,
                    payload: serde_json::to_value(structured_input_yield_payload(
                        request.title.clone(),
                        request.prompt.clone(),
                        request.questions.clone(),
                    ))
                    .unwrap_or_else(|_| json!({})),
                })
                .await;
            } else {
                let error_payload = json!({
                    "status": "invalid_request",
                    "error": "Invalid structured user input request."
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&error_payload),
                    audit: None,
                })
                .await;
                state.machine.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    error_payload,
                    false,
                );
                emit(Event::Error {
                    message: "Invalid structured user input request.".to_string(),
                    recoverable: true,
                })
                .await;
                return Ok(VirtualToolOutcome::EndTurn);
            }
            Ok(VirtualToolOutcome::PauseTurn)
        }
        "update_plan" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;
            match parse_plan_update(tool_arguments) {
                Some((explanation, items)) => {
                    state.turn_state.set_plan_snapshot_at_message_count(
                        explanation.clone(),
                        items.clone(),
                        state.machine.tape.messages().len(),
                    );
                    super::ui_surfaces::plan_updated(
                        state.namespace_environment(),
                        explanation.clone(),
                        items.clone(),
                    )
                    .await?;
                    let payload = json!({
                        "status": "plan_updated",
                        "explanation": explanation,
                        "items": items.clone(),
                        "items_count": items.len()
                    });
                    emit(Event::ToolCallCompleted {
                        presentation: None,
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        success: Some(true),
                        result_preview: tool_result_preview(&payload),
                        audit: None,
                    })
                    .await;
                    emit(Event::PlanUpdated {
                        explanation: explanation.clone(),
                        items: items.clone(),
                    })
                    .await;
                    state.machine.record_tool_call(
                        &tool_call.name,
                        tool_arguments.clone(),
                        payload.clone(),
                        true,
                    );
                    state
                        .machine
                        .add_tool_message(&tool_call.id, &tool_call.name, payload);
                    Ok(VirtualToolOutcome::Continue {
                        refresh_context: true,
                    })
                }
                None => {
                    let error_payload = json!({
                        "status": "invalid_request",
                        "error": "Invalid plan update payload."
                    });
                    emit(Event::ToolCallCompleted {
                        presentation: None,
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        success: Some(false),
                        result_preview: tool_result_preview(&error_payload),
                        audit: None,
                    })
                    .await;
                    state.machine.record_tool_call(
                        &tool_call.name,
                        tool_arguments.clone(),
                        error_payload,
                        false,
                    );
                    emit(Event::Error {
                        message: "Invalid plan update payload.".to_string(),
                        recoverable: true,
                    })
                    .await;
                    Ok(VirtualToolOutcome::Continue {
                        refresh_context: false,
                    })
                }
            }
        }
        "invoke_delegated_skill" => {
            handle_invoke_delegated_skill(
                state,
                tool_call,
                tool_arguments,
                cancel,
                emit,
                |state, spec, cancel| Box::pin(spawn_and_join_delegated_child(state, spec, cancel)),
            )
            .await
        }
        "terminate_child_run" => {
            handle_terminate_child_run(
                state,
                tool_call,
                tool_arguments,
                allow_approved_tool_escalation_execution,
                emit,
            )
            .await
        }
        _ => Ok(VirtualToolOutcome::NotVirtual),
    }
}

async fn handle_invoke_delegated_skill<E, F, S>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    emit: &mut E,
    spawn_child: S,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
    S: for<'a> FnOnce(
        &'a RuntimeLoopState,
        SpawnSpec,
        &'a CancellationToken,
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<ChildRuntimeResult>> + Send + 'a>,
    >,
{
    emit(Event::ToolCallStarted {
        title: None,
        id: tool_call.id.clone(),
        name: tool_call.name.clone(),
        audit: None,
    })
    .await;

    let Some(request) = parse_delegated_skill_invocation_request(tool_arguments) else {
        let error_payload = json!({
            "status": "invalid_request",
            "error": "Invalid delegated skill invocation payload."
        });
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&error_payload),
            audit: None,
        })
        .await;
        state.machine.record_tool_call(
            &tool_call.name,
            tool_arguments.clone(),
            error_payload.clone(),
            false,
        );
        state
            .machine
            .add_tool_message(&tool_call.id, &tool_call.name, error_payload.clone());
        emit(Event::Error {
            message: "Invalid delegated skill invocation payload.".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(VirtualToolOutcome::Continue {
            refresh_context: true,
        });
    };

    if !state.prompt_cache.supports_delegated_skill_invocation() {
        let error_payload = json!({
            "status": "delegated_invocation_unavailable",
            "error": "Delegated skill invocation is not available in this runtime."
        });
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&error_payload),
            audit: None,
        })
        .await;
        state.machine.record_tool_call(
            &tool_call.name,
            tool_arguments.clone(),
            error_payload.clone(),
            false,
        );
        state
            .machine
            .add_tool_message(&tool_call.id, &tool_call.name, error_payload);
        emit(Event::Error {
            message: "Delegated skill invocation is not available in this runtime.".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(VirtualToolOutcome::Continue {
            refresh_context: true,
        });
    }

    let (persisted_request, result, child_run) =
        match resolve_delegated_skill_invocation(state, &request) {
            Ok(spec) => {
                let persisted_request = request.with_effective_launch_inputs(
                    spec.launch.cwd.clone(),
                    spec.launch.timeout_secs,
                );
                match spawn_child(state, spec, cancel).await {
                    Ok(mut child_result) => {
                        if cancel.is_cancelled()
                            && child_result.is_cancelled()
                            && check_turn_cancelled(state, emit, cancel).await?
                        {
                            return Ok(VirtualToolOutcome::EndTurn);
                        }

                        let output_reference =
                            persist_delegated_child_evidence(state, &request, &child_result).await;
                        if let (Some(child_run_id), Some(reference)) = (
                            child_result.child_run_id.as_deref(),
                            output_reference.as_ref(),
                        ) {
                            state
                                .child_run_registry()
                                .set_state_ref(child_run_id, reference.clone());
                            child_result.child_run = state.child_run_registry().get(child_run_id);
                        }
                        (
                            persisted_request,
                            child_result.delegated_result(output_reference),
                            Some(child_result.reference()),
                        )
                    }
                    Err(err) => {
                        if cancel.is_cancelled()
                            && check_turn_cancelled(state, emit, cancel).await?
                        {
                            return Ok(VirtualToolOutcome::EndTurn);
                        }

                        let capability_decision = err
                            .downcast_ref::<DelegatedSpawnRejected>()
                            .map(|rejection| rejection.decision.clone());
                        let error_kind = if capability_decision.is_some() {
                            "delegated_capability_mismatch"
                        } else {
                            "child_launch_failed"
                        };
                        let mut result = DelegatedSkillResult::failed(
                            format!(
                                "Failed to launch delegated runtime for skill '{}': {err}",
                                request.skill_id
                            ),
                            Some(json!({
                                "error_kind": error_kind
                            })),
                        );
                        result.error_kind = Some(error_kind.to_string());
                        result.capability_decision = capability_decision;
                        (persisted_request, result, None)
                    }
                }
            }
            Err(result) => (request.clone(), *result, None),
        };

    let (persisted_arguments, tape_record, rollout_record) =
        build_bounded_delegated_invocation_persistence(&persisted_request, result, child_run);
    let preview = tool_result_preview(&json!(tape_record.result.summary.clone()));
    let tape_payload = serde_json::to_value(&tape_record).unwrap_or_else(|_| {
        json!({
            "status": "invalid_result_encoding",
            "error": "Failed to serialize delegated skill result."
        })
    });
    let rollout_payload =
        serde_json::to_value(&rollout_record).unwrap_or_else(|_| tape_payload.clone());
    let invocation_succeeded = matches!(
        tape_record.result.status,
        DelegatedSkillResultStatus::Completed
    );
    emit(Event::ToolCallCompleted {
        presentation: None,
        id: tool_call.id.clone(),
        name: Some(tool_call.name.clone()),
        success: Some(invocation_succeeded),
        result_preview: preview,
        audit: None,
    })
    .await;
    state.machine.record_tool_call(
        &tool_call.name,
        persisted_arguments,
        rollout_payload,
        invocation_succeeded,
    );
    state
        .machine
        .add_tool_message(&tool_call.id, &tool_call.name, tape_payload);
    Ok(VirtualToolOutcome::Continue {
        refresh_context: true,
    })
}

async fn spawn_and_join_delegated_child(
    state: &RuntimeLoopState,
    spec: SpawnSpec,
    cancel: &CancellationToken,
) -> Result<ChildRuntimeResult> {
    if cancel.is_cancelled() {
        return Ok(ChildRuntimeResult::cancelled_before_launch());
    }

    let controller = spawn_child_runtime_cancellable(state, spec, cancel).await?;
    controller.join_until_cancelled(cancel).await
}

fn resolve_delegated_skill_invocation(
    state: &mut RuntimeLoopState,
    request: &DelegatedSkillInvocationRequest,
) -> DelegatedSkillSpawnResult<SpawnSpec> {
    let active_skill = state
        .turn_state
        .active_skills()
        .iter()
        .find(|skill| skill.metadata.id == request.skill_id)
        .cloned();

    let skill_metadata = if let Some(skill) = active_skill {
        if !skill.availability.is_available() {
            return Err(Box::new(DelegatedSkillResult::failed(
                format!(
                    "Delegated skill '{}' is {}.",
                    request.skill_id,
                    skill.availability.render_label()
                ),
                Some(json!({
                    "error_kind": "skill_unavailable"
                })),
            )));
        }
        skill.metadata
    } else {
        match state
            .prompt_cache
            .resolve_listed_skill_metadata(request.skill_id.as_str())
        {
            Ok(Some(metadata)) => metadata,
            Ok(None) => {
                return Err(Box::new(DelegatedSkillResult::failed(
                    format!(
                        "Delegated skill '{}' is not active and is not listed for implicit use in the current runtime.",
                        request.skill_id
                    ),
                    Some(json!({
                        "error_kind": "skill_not_visible"
                    })),
                )));
            }
            Err(err) => {
                return Err(Box::new(DelegatedSkillResult::failed(
                    format!(
                        "Failed to resolve delegated skill '{}' from the runtime catalog: {err}",
                        request.skill_id
                    ),
                    Some(json!({
                        "error_kind": "skill_resolution_failed"
                    })),
                )));
            }
        }
    };

    let Some(resolved_target) = skill_metadata.execution.delegate_target() else {
        return Err(Box::new(DelegatedSkillResult::failed(
            format!(
                "Skill '{}' is not resolved for delegated execution.",
                request.skill_id
            ),
            Some(json!({
                "error_kind": "skill_not_delegated"
            })),
        )));
    };

    if resolved_target != request.target {
        return Err(Box::new(DelegatedSkillResult::failed(
            format!(
                "Delegated skill '{}' resolves to delegated target '{}' rather than '{}'.",
                request.skill_id, resolved_target, request.target
            ),
            Some(json!({
                "error_kind": "delegate_target_mismatch",
                "resolved_target": resolved_target
            })),
        )));
    }

    let Some(spawn_target) = skill_metadata.delegated_spawn_target() else {
        return Err(Box::new(DelegatedSkillResult::failed(
            format!(
                "Delegated skill '{}' does not expose a package-local launch target.",
                request.skill_id
            ),
            Some(json!({
                "error_kind": "delegate_target_missing"
            })),
        )));
    };

    build_delegated_spawn_spec(state, request, spawn_target)
}

fn build_delegated_spawn_spec(
    state: &RuntimeLoopState,
    request: &DelegatedSkillInvocationRequest,
    target: alan_agent_protocol::SpawnTarget,
) -> DelegatedSkillSpawnResult<SpawnSpec> {
    let parent_namespace_cwd = state
        .namespace_environment()
        .launch_context()
        .map(|context| Path::new(&context.cwd));
    let cwd = request
        .cwd
        .as_deref()
        .map(|path| resolve_delegated_launch_path(path, parent_namespace_cwd, "cwd"))
        .transpose()?
        .or_else(|| parent_namespace_cwd.map(lexically_normalize_path));
    let requirements = classify_delegated_task_requirements(&request.task, cwd.as_deref());
    let mut handles = vec![SpawnHandle::ApprovalScope];
    if cwd.as_deref().is_some_and(|cwd| {
        state
            .namespace_environment()
            .launch_context()
            .is_some_and(|context| {
                context
                    .host_mounts
                    .iter()
                    .any(|grant| grant.resolve_host_path(&cwd.to_string_lossy()).is_some())
            })
    }) {
        handles.push(SpawnHandle::HostMounts);
    }
    Ok(SpawnSpec {
        target,
        launch: SpawnLaunchInputs {
            task: request.task.clone(),
            cwd,
            timeout_secs: Some(
                request
                    .timeout_secs
                    .unwrap_or(DEFAULT_DELEGATED_TIMEOUT_SECS),
            ),
            output_dir: None,
        },
        handles,
        runtime_overrides: SpawnRuntimeOverrides::default(),
        delegated: Some(DelegatedSpawnContext { requirements }),
    })
}

fn resolve_delegated_launch_path(
    requested_path: &Path,
    base: Option<&Path>,
    field_name: &str,
) -> DelegatedSkillSpawnResult<PathBuf> {
    if requested_path.is_absolute() {
        return Ok(lexically_normalize_path(requested_path));
    }

    let Some(base) = base else {
        return Err(Box::new(DelegatedSkillResult::failed(
            format!(
                "Delegated skill invocation provided relative {field_name} '{}' but the parent runtime has no base path to resolve it.",
                requested_path.display()
            ),
            Some(json!({
                "error_kind": "relative_launch_path_unresolvable",
                "field": field_name
            })),
        )));
    };

    Ok(lexically_normalize_path(&base.join(requested_path)))
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

pub(super) fn parse_confirmation_request(
    tool_call_id: &str,
    args: &serde_json::Value,
) -> Option<PendingConfirmation> {
    let checkpoint_type = args
        .get("checkpoint_type")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or("confirmation")
        .to_string();
    if checkpoint_type == MOUNT_ESCALATION_CHECKPOINT_TYPE {
        return None;
    }
    let summary = args.get("summary")?.as_str()?.trim().to_string();
    if summary.is_empty() {
        return None;
    }
    let details = args.get("details").cloned().unwrap_or(json!({}));
    let options = args
        .get("options")
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                })
                .collect()
        })
        .filter(|opts: &Vec<String>| !opts.is_empty())
        .unwrap_or_else(|| {
            vec![
                "approve".to_string(),
                "modify".to_string(),
                "reject".to_string(),
            ]
        });

    Some(PendingConfirmation {
        checkpoint_id: tool_call_id.to_string(),
        checkpoint_type,
        summary,
        details,
        options,
    })
}

fn parse_structured_user_input_request(
    tool_call_id: &str,
    arguments: &serde_json::Value,
) -> Option<crate::approval::PendingStructuredInputRequest> {
    let title = arguments.get("title")?.as_str()?.trim().to_string();
    let prompt = arguments.get("prompt")?.as_str()?.trim().to_string();
    if title.is_empty() || prompt.is_empty() {
        return None;
    }
    let request_id = tool_call_id.to_string();

    let questions = arguments
        .get("questions")?
        .as_array()?
        .iter()
        .filter_map(|raw| {
            let id = parse_non_empty_string(raw.get("id"))?;
            let label = parse_non_empty_string(raw.get("label"))?;
            let prompt = parse_non_empty_string(raw.get("prompt"))?;
            let required = raw
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let options = raw
                .get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|opt| {
                            Some(StructuredInputOption {
                                value: parse_non_empty_string(opt.get("value"))?,
                                label: parse_non_empty_string(opt.get("label"))?,
                                description: parse_optional_string(opt.get("description")),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let kind = parse_structured_input_kind(raw.get("kind"), !options.is_empty())?;
            let placeholder = parse_optional_string(raw.get("placeholder"));
            let help_text = parse_optional_string(raw.get("help_text"));
            let default_value = parse_optional_string(raw.get("default"));
            let default_values = parse_string_array(raw.get("defaults"));
            let min_selected = parse_optional_u32(raw.get("min_selected"));
            let max_selected = parse_optional_u32(raw.get("max_selected"));
            let presentation_hints = parse_presentation_hints(raw.get("presentation_hints"));
            let options = normalize_question_options(kind, options);

            if matches!(
                kind,
                StructuredInputKind::Boolean
                    | StructuredInputKind::SingleSelect
                    | StructuredInputKind::MultiSelect
            ) && options.is_empty()
            {
                return None;
            }

            let option_values = options
                .iter()
                .map(|opt| opt.value.as_str())
                .collect::<Vec<_>>();
            let normalized_default_value = match kind {
                StructuredInputKind::Text
                | StructuredInputKind::Number
                | StructuredInputKind::Integer => default_value.clone(),
                StructuredInputKind::Boolean | StructuredInputKind::SingleSelect => {
                    normalize_single_default(default_value.clone(), option_values.as_slice())
                }
                StructuredInputKind::MultiSelect => None,
            };
            let normalized_default_values = if matches!(kind, StructuredInputKind::MultiSelect) {
                normalize_multi_defaults(
                    default_value.as_deref(),
                    default_values,
                    option_values.as_slice(),
                )
            } else {
                Vec::new()
            };
            let (min_selected, max_selected) =
                normalize_selection_constraints(min_selected, max_selected, options.len());

            Some(StructuredInputQuestion {
                id,
                label,
                prompt,
                kind,
                required,
                placeholder,
                help_text,
                default_value: normalized_default_value,
                default_values: normalized_default_values,
                min_selected: if matches!(kind, StructuredInputKind::MultiSelect) {
                    min_selected
                } else {
                    None
                },
                max_selected: if matches!(kind, StructuredInputKind::MultiSelect) {
                    max_selected
                } else {
                    None
                },
                options,
                presentation_hints,
            })
        })
        .collect::<Vec<_>>();

    if questions.is_empty() {
        return None;
    }

    Some(crate::approval::PendingStructuredInputRequest {
        request_id,
        title,
        prompt,
        questions,
    })
}

fn parse_non_empty_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|raw| raw.as_str())
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(ToString::to_string)
}

fn parse_optional_string(value: Option<&serde_json::Value>) -> Option<String> {
    parse_non_empty_string(value)
}

fn parse_string_array(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|raw| raw.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| parse_non_empty_string(Some(item)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_optional_u32(value: Option<&serde_json::Value>) -> Option<u32> {
    value
        .and_then(|raw| raw.as_u64())
        .and_then(|raw| u32::try_from(raw).ok())
}

fn parse_structured_input_kind(
    value: Option<&serde_json::Value>,
    has_options: bool,
) -> Option<StructuredInputKind> {
    match value.and_then(|raw| raw.as_str()) {
        Some("text") => Some(StructuredInputKind::Text),
        Some("boolean") => Some(StructuredInputKind::Boolean),
        Some("number") => Some(StructuredInputKind::Number),
        Some("integer") => Some(StructuredInputKind::Integer),
        Some("single_select") => Some(StructuredInputKind::SingleSelect),
        Some("multi_select") => Some(StructuredInputKind::MultiSelect),
        Some(_) => None,
        None => Some(if has_options {
            StructuredInputKind::SingleSelect
        } else {
            StructuredInputKind::Text
        }),
    }
}

fn parse_presentation_hints(value: Option<&serde_json::Value>) -> Vec<AdaptivePresentationHint> {
    value
        .and_then(|raw| raw.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item.as_str() {
                    Some("radio") => Some(AdaptivePresentationHint::Radio),
                    Some("toggle") => Some(AdaptivePresentationHint::Toggle),
                    Some("searchable") => Some(AdaptivePresentationHint::Searchable),
                    Some("multiline") => Some(AdaptivePresentationHint::Multiline),
                    Some("compact") => Some(AdaptivePresentationHint::Compact),
                    Some("dangerous") => Some(AdaptivePresentationHint::Dangerous),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_question_options(
    kind: StructuredInputKind,
    options: Vec<StructuredInputOption>,
) -> Vec<StructuredInputOption> {
    if matches!(kind, StructuredInputKind::Boolean) && options.is_empty() {
        return boolean_options();
    }
    options
}

fn boolean_options() -> Vec<StructuredInputOption> {
    vec![
        StructuredInputOption {
            value: "true".to_string(),
            label: "Yes".to_string(),
            description: None,
        },
        StructuredInputOption {
            value: "false".to_string(),
            label: "No".to_string(),
            description: None,
        },
    ]
}

fn structured_input_yield_payload(
    title: String,
    prompt: String,
    questions: Vec<StructuredInputQuestion>,
) -> StructuredInputYieldPayload {
    StructuredInputYieldPayload {
        title,
        prompt: Some(prompt),
        questions,
    }
}

fn normalize_single_default(
    default_value: Option<String>,
    option_values: &[&str],
) -> Option<String> {
    default_value
        .filter(|value| option_values.is_empty() || option_values.contains(&value.as_str()))
}

fn normalize_multi_defaults(
    default_value: Option<&str>,
    default_values: Vec<String>,
    option_values: &[&str],
) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in default_values {
        if option_values.contains(&value.as_str()) && !normalized.contains(&value) {
            normalized.push(value);
        }
    }

    if normalized.is_empty()
        && let Some(value) = default_value
        && option_values.contains(&value)
    {
        normalized.push(value.to_string());
    }

    normalized
}

fn normalize_selection_constraints(
    min_selected: Option<u32>,
    max_selected: Option<u32>,
    option_count: usize,
) -> (Option<u32>, Option<u32>) {
    let option_limit = u32::try_from(option_count).ok();
    let min = min_selected.filter(|value| Some(*value) <= option_limit);
    let max = max_selected.filter(|value| Some(*value) <= option_limit);

    match (min, max) {
        (Some(min), Some(max)) if max < min => (Some(min), None),
        other => other,
    }
}

fn parse_plan_status(raw: &str) -> Option<alan_agent_protocol::PlanItemStatus> {
    match raw {
        "pending" | "blocked" => Some(alan_agent_protocol::PlanItemStatus::Pending),
        "in_progress" => Some(alan_agent_protocol::PlanItemStatus::InProgress),
        "completed" | "skipped" => Some(alan_agent_protocol::PlanItemStatus::Completed),
        _ => None,
    }
}

fn parse_plan_items(value: &serde_json::Value) -> Option<Vec<alan_agent_protocol::PlanItem>> {
    let items = value.as_array()?;
    let parsed = items
        .iter()
        .filter_map(|raw| {
            let id = raw.get("id")?.as_str()?.to_string();
            let content = raw
                .get("content")
                .or_else(|| raw.get("description"))?
                .as_str()?
                .to_string();
            let status_raw = raw.get("status")?.as_str()?;
            let status = parse_plan_status(status_raw)?;
            Some(alan_agent_protocol::PlanItem {
                id,
                content,
                status,
            })
        })
        .collect::<Vec<_>>();
    (!parsed.is_empty()).then_some(parsed)
}

fn parse_plan_update(
    arguments: &serde_json::Value,
) -> Option<(Option<String>, Vec<alan_agent_protocol::PlanItem>)> {
    let explanation = arguments
        .get("explanation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let items = parse_plan_items(arguments.get("items")?)?;
    Some((explanation, items))
}

fn request_confirmation_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_confirmation".to_string(),
        description: "Request user confirmation or approval before proceeding with a significant action. Use this when you need explicit user approval before making changes or proceeding with a recommendation.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "checkpoint_id": {
                    "type": "string",
                    "description": "Optional legacy field. Runtime uses the tool call id as request_id."
                },
                "checkpoint_type": {
                    "type": "string",
                    "description": "The type of checkpoint (e.g., 'business_understanding', 'supplier_recommendation', 'final_confirmation'). Defaults to 'confirmation'."
                },
                "summary": {
                    "type": "string",
                    "description": "A clear summary of what is being proposed or what the user should confirm"
                },
                "details": {
                    "type": "object",
                    "description": "Additional structured details relevant to the confirmation"
                }
            },
            "required": ["summary"]
        }),
    }
}

fn request_user_input_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_user_input".to_string(),
        description: "Request structured user input (questions/options) from the client UI and wait for a structured response before continuing.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "request_id": {
                    "type": "string",
                    "description": "Optional legacy field. Runtime uses the tool call id as request_id."
                },
                "title": { "type": "string" },
                "prompt": { "type": "string" },
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "label": { "type": "string" },
                            "prompt": { "type": "string" },
                            "kind": {
                                "type": "string",
                                "enum": ["text", "boolean", "number", "integer", "single_select", "multi_select"]
                            },
                            "required": { "type": "boolean" },
                            "placeholder": { "type": "string" },
                            "help_text": { "type": "string" },
                            "presentation_hints": {
                                "type": "array",
                                "items": {
                                    "type": "string",
                                    "enum": ["radio", "toggle", "searchable", "multiline", "compact", "dangerous"]
                                }
                            },
                            "default": { "type": "string" },
                            "defaults": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "min_selected": { "type": "integer", "minimum": 0 },
                            "max_selected": { "type": "integer", "minimum": 0 },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "value": { "type": "string" },
                                        "label": { "type": "string" },
                                        "description": { "type": "string" }
                                    },
                                    "required": ["value", "label"]
                                }
                            }
                        },
                        "required": ["id", "label", "prompt"]
                    }
                }
            },
            "required": ["title", "prompt", "questions"]
        }),
    }
}

fn update_plan_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "update_plan".to_string(),
        description: "Publish a normalized plan/progress update to the client UI. Use this when the task plan changes or step status changes.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "explanation": { "type": "string" },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "content": { "type": "string" },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
                        },
                        "required": ["id", "content", "status"]
                    }
                }
            },
            "required": ["items"]
        }),
    }
}

#[cfg(test)]
#[path = "virtual_tools_tests.rs"]
mod tests;
