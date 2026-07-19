use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use alan_agent_protocol::{
    DelegatedSpawnContext, Event, SpawnHandle, SpawnLaunchInputs, SpawnRuntimeOverrides, SpawnSpec,
};
use anyhow::Result;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::llm::ToolDefinition;
use crate::skills::{DelegatedSkillResult, DelegatedSkillResultStatus};

use super::child_agents::spawn_child_runtime_cancellable;
use super::delegated_child_run::ChildRuntimeResult;
use super::delegated_skill_evidence::{
    build_bounded_delegated_invocation_persistence, persist_delegated_child_evidence,
};
use super::delegation_capabilities::{
    DelegatedSpawnRejected, classify_delegated_task_requirements,
};
use super::transition::RuntimeLoopState;
use super::turn_support::{check_turn_cancelled, tool_result_preview};
use super::virtual_tool::VirtualToolOutcome;
use crate::agent_machine::NormalizedToolCall;

pub(super) const MAX_DELEGATED_SKILL_ID_CHARS: usize = 120;
pub(super) const MAX_DELEGATED_TARGET_CHARS: usize = 120;
pub(super) const MAX_DELEGATED_TASK_CHARS: usize = 1_000;
pub(super) const MAX_DELEGATED_PATH_CHARS: usize = 1_000;
pub(super) const DEFAULT_DELEGATED_TIMEOUT_SECS: u64 = 900;
pub(super) const MAX_DELEGATED_TIMEOUT_SECS: u64 = 86_400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DelegatedSkillInvocationRequest {
    pub(super) skill_id: String,
    pub(super) target: String,
    pub(super) task: String,
    pub(super) cwd: Option<PathBuf>,
    pub(super) timeout_secs: Option<u64>,
}

impl DelegatedSkillInvocationRequest {
    pub(super) fn with_effective_launch_inputs(
        &self,
        cwd: Option<PathBuf>,
        timeout_secs: Option<u64>,
    ) -> Self {
        Self {
            skill_id: self.skill_id.clone(),
            target: self.target.clone(),
            task: self.task.clone(),
            cwd,
            timeout_secs,
        }
    }
}

pub(super) fn parse_delegated_skill_invocation_request(
    arguments: &serde_json::Value,
) -> Option<DelegatedSkillInvocationRequest> {
    let skill_id = arguments.get("skill_id")?.as_str()?.trim().to_string();
    let target = arguments.get("target")?.as_str()?.trim().to_string();
    let task = arguments.get("task")?.as_str()?.trim().to_string();
    let cwd = parse_optional_path_argument(arguments, "cwd")?;
    let timeout_secs = parse_optional_timeout_secs_argument(arguments, "timeout_secs")?;
    if skill_id.is_empty() || target.is_empty() || task.is_empty() {
        return None;
    }
    Some(DelegatedSkillInvocationRequest {
        skill_id,
        target,
        task,
        cwd,
        timeout_secs,
    })
}

fn parse_optional_path_argument(
    arguments: &serde_json::Value,
    key: &str,
) -> Option<Option<PathBuf>> {
    match arguments.get(key) {
        None => Some(None),
        Some(value) => {
            let path = value.as_str()?.trim();
            if path.is_empty() {
                return Some(None);
            }
            Some(Some(PathBuf::from(path)))
        }
    }
}

fn parse_optional_timeout_secs_argument(
    arguments: &serde_json::Value,
    key: &str,
) -> Option<Option<u64>> {
    match arguments.get(key) {
        None => Some(None),
        Some(value) => {
            let timeout_secs = value.as_u64()?;
            if timeout_secs == 0 || timeout_secs > MAX_DELEGATED_TIMEOUT_SECS {
                return None;
            }
            Some(Some(timeout_secs))
        }
    }
}

pub(super) fn invoke_delegated_skill_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "invoke_delegated_skill".to_string(),
        description: "Invoke a delegated skill through alan's runtime-owned delegated launch path. Use this for delegated skills listed in the skills catalog or in active-skill runtime context.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "skill_id": {
                    "type": "string",
                    "description": "Resolved delegated skill id exposed in the skills catalog or active-skill runtime context.",
                    "maxLength": MAX_DELEGATED_SKILL_ID_CHARS
                },
                "target": {
                    "type": "string",
                    "description": "Resolved package-local launch target for this delegated skill.",
                    "maxLength": MAX_DELEGATED_TARGET_CHARS
                },
                "task": {
                    "type": "string",
                    "description": "A concise bounded task for the delegated runtime.",
                    "maxLength": MAX_DELEGATED_TASK_CHARS
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional Alan OS namespace cwd for the delegated Process. When omitted, the child inherits the parent Process cwd.",
                    "maxLength": MAX_DELEGATED_PATH_CHARS
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional bounded runtime timeout for the delegated child. When omitted, alan applies a default bounded child timeout.",
                    "minimum": 1,
                    "maximum": MAX_DELEGATED_TIMEOUT_SECS
                }
            },
            "required": ["skill_id", "target", "task"]
        }),
    }
}

pub(super) async fn handle_invoke_delegated_skill<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    handle_invoke_delegated_skill_with_spawn(
        state,
        tool_call,
        tool_arguments,
        cancel,
        emit,
        |state, spec, cancel| Box::pin(spawn_and_join_delegated_child(state, spec, cancel)),
    )
    .await
}

pub(super) async fn handle_invoke_delegated_skill_with_spawn<E, F, S>(
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

    let agent_files = state.agent_files();
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
                            && check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel)
                                .await?
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
                            && check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel)
                                .await?
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
        .machine
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
    let child_launch = state.child_launch();
    let launch_context = child_launch.launch_context();
    let parent_namespace_cwd = launch_context.map(|context| Path::new(&context.cwd));
    let cwd = request
        .cwd
        .as_deref()
        .map(|path| resolve_delegated_launch_path(path, parent_namespace_cwd, "cwd"))
        .transpose()?
        .or_else(|| parent_namespace_cwd.map(lexically_normalize_path));
    let requirements = classify_delegated_task_requirements(&request.task, cwd.as_deref());
    let mut handles = vec![SpawnHandle::ApprovalScope];
    if cwd.as_deref().is_some_and(|cwd| {
        launch_context.is_some_and(|context| {
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

type DelegatedSkillSpawnResult<T> = std::result::Result<T, Box<DelegatedSkillResult>>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_valid_invocation_request() {
        let args = json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        });

        let result = parse_delegated_skill_invocation_request(&args).unwrap();
        assert_eq!(result.skill_id, "repo-review");
        assert_eq!(result.target, "reviewer");
        assert_eq!(result.task, "Review the current diff and summarize risks.");
    }

    #[test]
    fn treats_an_empty_optional_cwd_as_absent() {
        let args = json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks.",
            "cwd": "   "
        });

        let result = parse_delegated_skill_invocation_request(&args).unwrap();
        assert_eq!(result.cwd, None);
    }

    #[test]
    fn rejects_missing_or_empty_required_fields() {
        let missing = json!({
            "skill_id": "repo-review",
            "target": "reviewer"
        });
        assert!(parse_delegated_skill_invocation_request(&missing).is_none());

        let empty = json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "   "
        });
        assert!(parse_delegated_skill_invocation_request(&empty).is_none());
    }

    #[test]
    fn accepts_a_bounded_timeout() {
        let request = parse_delegated_skill_invocation_request(&json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff.",
            "timeout_secs": 600
        }))
        .expect("expected delegated request");

        assert_eq!(request.timeout_secs, Some(600));
    }

    #[test]
    fn rejects_an_invalid_timeout() {
        assert!(
            parse_delegated_skill_invocation_request(&json!({
                "skill_id": "repo-review",
                "target": "reviewer",
                "task": "Review the current diff.",
                "timeout_secs": 0
            }))
            .is_none()
        );
        assert!(
            parse_delegated_skill_invocation_request(&json!({
                "skill_id": "repo-review",
                "target": "reviewer",
                "task": "Review the current diff.",
                "timeout_secs": (MAX_DELEGATED_TIMEOUT_SECS + 1)
            }))
            .is_none()
        );
    }

    #[test]
    fn tool_definition_exposes_bounded_invocation_contract() {
        let def = invoke_delegated_skill_tool_definition();
        assert_eq!(def.name, "invoke_delegated_skill");
        assert!(def.description.contains("delegated skill"));
        assert_eq!(def.parameters["type"], "object");
        assert_eq!(def.parameters["properties"]["skill_id"]["type"], "string");
        assert_eq!(
            def.parameters["properties"]["skill_id"]["maxLength"],
            MAX_DELEGATED_SKILL_ID_CHARS
        );
        assert_eq!(def.parameters["properties"]["target"]["type"], "string");
        assert_eq!(
            def.parameters["properties"]["target"]["maxLength"],
            MAX_DELEGATED_TARGET_CHARS
        );
        assert_eq!(def.parameters["properties"]["task"]["type"], "string");
        assert_eq!(
            def.parameters["properties"]["task"]["maxLength"],
            MAX_DELEGATED_TASK_CHARS
        );
        assert!(def.parameters["properties"].get("workspace_root").is_none());
        assert_eq!(def.parameters["properties"]["cwd"]["type"], "string");
    }
}
