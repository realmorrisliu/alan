//! Ad-hoc `spawn_subagent` virtual tool and shared child-batch helpers.

mod batch;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use alan_agent_protocol::{
    Event, SpawnHandle, SpawnHostMount, SpawnLaunchInputs, SpawnMountAccess, SpawnRuntimeOverrides,
    SpawnSpec, SpawnTarget,
};
use anyhow::Result;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::agent_machine::NormalizedToolCall;
use crate::agent_machine::pending_resume::ResumeTarget;
use crate::llm::ToolDefinition;

use super::child_agents::{ChildLaunchRuntime, spawn_child_runtime_cancellable};
use super::delegated_child_run::ChildRuntimeResult;
use super::delegated_skill_evidence::build_subagent_action_metadata;
use super::delegated_skill_tool::{
    DEFAULT_DELEGATED_TIMEOUT_SECS, DelegatedSkillRuntime, MAX_DELEGATED_HOST_MOUNTS,
    MAX_DELEGATED_PATH_CHARS, MAX_DELEGATED_SKILL_ID_CHARS, MAX_DELEGATED_TASK_CHARS,
    MAX_DELEGATED_TIMEOUT_SECS, parse_host_mounts, parse_optional_path_argument,
    parse_optional_timeout_secs_argument, resolve_delegated_launch_path,
};
use super::transition::NamespaceAgentFiles;
use super::virtual_tool::VirtualToolOutcome;

pub(in crate::runtime) use batch::{
    ExecutedChildSlot, PrelaunchFailure, SlotExecutionError, commit_executed_slot,
    execute_prepared_slot, finalize_action_record, prepare_delegated_skill_slot,
    prepare_subagent_slot,
};

pub(super) const MAX_SUBAGENT_ROLE_CHARS: usize = 120;
pub(super) const MAX_SUBAGENT_MODEL_CHARS: usize = 120;

/// Spawn closure signature shared between `spawn_subagent`, `invoke_delegated_skill`,
/// and concurrent subagent batch execution.
pub(in crate::runtime) type SubagentSpawnFn = Arc<
    dyn Fn(
            ChildLaunchRuntime,
            SpawnSpec,
            CancellationToken,
            Option<ResumeTarget>,
            Option<ActionProgressTracker>,
        ) -> Pin<Box<dyn Future<Output = Result<ChildRuntimeResult>> + Send>>
        + Send
        + Sync,
>;

/// Tracks a live `actions/<id>` entry while its child Process starts and runs.
#[derive(Clone)]
pub(in crate::runtime) struct ActionProgressTracker {
    agent_files: NamespaceAgentFiles,
    action_id: String,
    label: String,
}

impl ActionProgressTracker {
    pub(in crate::runtime) fn new(
        agent_files: NamespaceAgentFiles,
        action_id: String,
        label: String,
    ) -> Self {
        Self {
            agent_files,
            action_id,
            label,
        }
    }

    pub(in crate::runtime) async fn mark_started(
        &self,
        child_pid: &str,
        child_run_id: &str,
        process_path: &str,
    ) {
        let doc = build_subagent_action_metadata(
            &self.label,
            Some(child_pid),
            Some(child_run_id),
            "running",
            Some(process_path),
            None,
        );
        let _ = self
            .agent_files
            .update_running_action(&self.action_id, &doc.to_string(), Some(process_path))
            .await;
    }
}

/// Parsed arguments for an ad-hoc `spawn_subagent` virtual tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runtime) struct SubagentSpawnRequest {
    pub(in crate::runtime) role: String,
    pub(in crate::runtime) task: String,
    pub(in crate::runtime) model: Option<String>,
    pub(in crate::runtime) read_only: bool,
    pub(in crate::runtime) cwd: Option<PathBuf>,
    pub(in crate::runtime) host_mounts: Vec<SpawnHostMount>,
    pub(in crate::runtime) timeout_secs: Option<u64>,
}

/// Build the `ToolDefinition` for `spawn_subagent` exposed on Root Agent.
pub(super) fn spawn_subagent_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "spawn_subagent".to_string(),
        description: "Spawn an independent bounded ad-hoc subagent Process with an explicit role, task, optional model, optional read-only posture, and optional equal-or-narrower Host Mounts."
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "role": {
                    "type": "string",
                    "description": "Concise single-line role name for the subagent (for example researcher, reviewer, verifier).",
                    "minLength": 1,
                    "maxLength": MAX_SUBAGENT_ROLE_CHARS
                },
                "task": {
                    "type": "string",
                    "description": "A concise bounded task for the ad-hoc subagent Process.",
                    "minLength": 1,
                    "maxLength": MAX_DELEGATED_TASK_CHARS
                },
                "model": {
                    "type": "string",
                    "description": "Optional effective model override for this subagent.",
                    "minLength": 1,
                    "maxLength": MAX_SUBAGENT_MODEL_CHARS
                },
                "read_only": {
                    "type": "boolean",
                    "description": "When true, restricts the child Process to read-only mounts and read-only tools."
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional normalized Alan OS namespace cwd for the subagent. Defaults to parent cwd if mounted or '/' otherwise.",
                    "maxLength": MAX_DELEGATED_PATH_CHARS
                },
                "host_mounts": {
                    "type": "array",
                    "description": "Explicit parent-held Host Mount grants for the child (equal-or-narrower than parent posture; read_write rejected when read_only=true).",
                    "maxItems": MAX_DELEGATED_HOST_MOUNTS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "grant": {"type": "string", "minLength": 1, "maxLength": MAX_DELEGATED_SKILL_ID_CHARS},
                            "target": {"type": "string", "minLength": 1, "maxLength": MAX_DELEGATED_PATH_CHARS},
                            "access": {"type": "string", "enum": ["read_only", "read_write"]}
                        },
                        "required": ["grant", "target", "access"],
                        "additionalProperties": false
                    }
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional bounded timeout in seconds for the subagent run.",
                    "minimum": 1,
                    "maximum": MAX_DELEGATED_TIMEOUT_SECS
                }
            },
            "required": ["role", "task"]
        }),
    }
}

pub(in crate::runtime) fn default_subagent_spawn_fn() -> SubagentSpawnFn {
    Arc::new(|runtime, spec, cancel, resume, tracker| {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Ok(ChildRuntimeResult::cancelled_before_launch());
            }
            let controller =
                spawn_child_runtime_cancellable(runtime, spec, &cancel, resume.as_ref()).await?;
            if let Some(tracker) = tracker {
                let pid = controller.process_pid().to_string();
                let run_id = controller.child_run_id().to_string();
                let process_path = format!("/proc/{pid}");
                tracker.mark_started(&pid, &run_id, &process_path).await;
            }
            controller.join_until_cancelled(&cancel).await
        })
    })
}

pub(in crate::runtime) fn parse_subagent_spawn_request(
    arguments: &Value,
) -> Option<SubagentSpawnRequest> {
    let role = arguments.get("role")?.as_str()?.trim().to_string();
    let task = arguments.get("task")?.as_str()?.trim().to_string();
    if role.is_empty()
        || role.chars().count() > MAX_SUBAGENT_ROLE_CHARS
        || role.chars().any(char::is_control)
        || task.is_empty()
        || task.chars().count() > MAX_DELEGATED_TASK_CHARS
    {
        return None;
    }
    let model = match arguments.get("model") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let trimmed = value.as_str()?.trim();
            if trimmed.is_empty()
                || trimmed.chars().count() > MAX_SUBAGENT_MODEL_CHARS
                || trimmed.chars().any(char::is_control)
            {
                return None;
            }
            Some(trimmed.to_string())
        }
    };
    let read_only = match arguments.get("read_only") {
        None | Some(Value::Null) => false,
        Some(value) => value.as_bool()?,
    };
    let cwd = parse_optional_path_argument(arguments, "cwd")?;
    let host_mounts = parse_host_mounts(arguments)?;
    let timeout_secs = parse_optional_timeout_secs_argument(arguments, "timeout_secs")?;
    Some(SubagentSpawnRequest {
        role,
        task,
        model,
        read_only,
        cwd,
        host_mounts,
        timeout_secs,
    })
}

pub(in crate::runtime) fn resolve_subagent_spawn_spec(
    parent_cwd: &Path,
    request: &SubagentSpawnRequest,
) -> std::result::Result<SpawnSpec, Box<PrelaunchFailure>> {
    if request.read_only
        && request
            .host_mounts
            .iter()
            .any(|mount| mount.access == SpawnMountAccess::ReadWrite)
    {
        return Err(Box::new(PrelaunchFailure {
            status: "read_only_mount_conflict".to_string(),
            error_kind: "read_only_mount_conflict".to_string(),
            message: "Read-only subagent cannot request read_write host_mounts.".to_string(),
            delegated_result: None,
        }));
    }

    let default_cwd = if request
        .host_mounts
        .iter()
        .any(|mount| parent_cwd.starts_with(&mount.target))
    {
        parent_cwd.to_path_buf()
    } else if let Some(first_mount) = request.host_mounts.first() {
        first_mount.target.clone()
    } else {
        PathBuf::from("/")
    };
    let requested_cwd = request.cwd.as_deref().unwrap_or(&default_cwd);
    let cwd = match resolve_delegated_launch_path(requested_cwd, Some(parent_cwd), "cwd") {
        Ok(path) => path,
        Err(delegated_err) => {
            return Err(Box::new(PrelaunchFailure {
                status: "invalid_request".to_string(),
                error_kind: delegated_err
                    .error_kind
                    .clone()
                    .unwrap_or_else(|| "invalid_cwd".to_string()),
                message: delegated_err.summary.clone(),
                delegated_result: Some(*delegated_err),
            }));
        }
    };

    let spec = SpawnSpec {
        target: SpawnTarget::AdHocSubagent {
            role: request.role.clone(),
        },
        launch: SpawnLaunchInputs {
            task: request.task.clone(),
            cwd: Some(cwd),
            timeout_secs: Some(
                request
                    .timeout_secs
                    .unwrap_or(DEFAULT_DELEGATED_TIMEOUT_SECS),
            ),
            output_dir: None,
        },
        handles: vec![SpawnHandle::ApprovalScope],
        host_mounts: request.host_mounts.clone(),
        runtime_overrides: SpawnRuntimeOverrides {
            model: request.model.clone(),
            read_only: request.read_only,
            ..SpawnRuntimeOverrides::default()
        },
        delegated: None,
    };
    if let Err(err) = spec.validate_agent_process_launch() {
        return Err(Box::new(PrelaunchFailure {
            status: "invalid_request".to_string(),
            error_kind: "invalid_spawn_spec".to_string(),
            message: err.to_string(),
            delegated_result: None,
        }));
    }
    Ok(spec)
}

/// Handle one sequential `spawn_subagent` call through the shared prepare/execute/commit path.
pub(super) async fn handle_spawn_subagent<E, F>(
    runtime: DelegatedSkillRuntime<'_>,
    subagent_spawn_enabled: bool,
    tool_call: &NormalizedToolCall,
    tool_arguments: &Value,
    cancel: &CancellationToken,
    task_cancel: &CancellationToken,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: Future<Output = ()>,
{
    handle_spawn_subagent_with_spawn(
        runtime,
        subagent_spawn_enabled,
        tool_call,
        tool_arguments,
        cancel,
        task_cancel,
        emit,
        default_subagent_spawn_fn(),
    )
    .await
}

#[allow(
    clippy::too_many_arguments,
    reason = "Subagent dispatch takes explicit runtime, feature gate, call arguments, cancellation tokens, emitter, and spawn hook"
)]
pub(in crate::runtime) async fn handle_spawn_subagent_with_spawn<E, F>(
    mut runtime: DelegatedSkillRuntime<'_>,
    subagent_spawn_enabled: bool,
    tool_call: &NormalizedToolCall,
    tool_arguments: &Value,
    cancel: &CancellationToken,
    task_cancel: &CancellationToken,
    emit: &mut E,
    spawn_fn: SubagentSpawnFn,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: Future<Output = ()>,
{
    emit(Event::ToolCallStarted {
        title: None,
        id: tool_call.id.clone(),
        name: tool_call.name.clone(),
        audit: None,
    })
    .await;

    let slot = prepare_subagent_slot(
        &mut runtime,
        subagent_spawn_enabled,
        0,
        tool_call,
        tool_arguments,
    )
    .await?;

    let child_cancel = CancellationToken::new();
    let cancel_parent = cancel.clone();
    let cancel_task = task_cancel.clone();
    let cancel_forward = child_cancel.clone();
    let watcher = tokio::spawn(async move {
        tokio::select! {
            _ = cancel_parent.cancelled() => cancel_forward.cancel(),
            _ = cancel_task.cancelled() => cancel_forward.cancel(),
            _ = cancel_forward.cancelled() => {}
        }
    });

    let executed = execute_prepared_slot(
        slot,
        runtime.agent_files.clone(),
        child_cancel.clone(),
        spawn_fn,
    )
    .await;
    child_cancel.cancel();
    let _ = watcher.await;

    commit_executed_slot(&mut runtime, executed, cancel, task_cancel, emit).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_subagent_spawn_request_validates_fields_and_mounts() {
        let req = parse_subagent_spawn_request(&json!({
            "role": "researcher",
            "task": "Trace auth login flow",
            "model": "gpt-5.4",
            "read_only": true,
            "cwd": "/mnt/project",
            "host_mounts": [{"grant": "g1", "target": "/mnt/project", "access": "read_only"}],
            "timeout_secs": 45
        }))
        .unwrap();
        assert_eq!(req.role, "researcher");
        assert!(req.read_only);
        assert_eq!(req.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(req.timeout_secs, Some(45));

        assert!(parse_subagent_spawn_request(&json!({"role": "", "task": "x"})).is_none());
        assert!(parse_subagent_spawn_request(&json!({"role": "bad\nrole", "task": "x"})).is_none());
        assert!(
            parse_subagent_spawn_request(&json!({"role": "a", "task": "x", "read_only": "yes"}))
                .is_none()
        );
    }

    #[test]
    fn read_only_subagent_rejects_read_write_host_mounts() {
        let req = SubagentSpawnRequest {
            role: "reviewer".to_string(),
            task: "Inspect".to_string(),
            model: None,
            read_only: true,
            cwd: Some(PathBuf::from("/mnt/project")),
            host_mounts: vec![SpawnHostMount {
                grant: "grant-1".to_string(),
                target: PathBuf::from("/mnt/project"),
                access: SpawnMountAccess::ReadWrite,
            }],
            timeout_secs: None,
        };
        let err = resolve_subagent_spawn_spec(Path::new("/mnt/project"), &req).unwrap_err();
        assert_eq!(err.error_kind, "read_only_mount_conflict");
    }

    #[test]
    fn subagent_without_mounts_defaults_cwd_to_root() {
        let req = SubagentSpawnRequest {
            role: "planner".to_string(),
            task: "Draft checklist".to_string(),
            model: None,
            read_only: false,
            cwd: None,
            host_mounts: Vec::new(),
            timeout_secs: None,
        };
        let spec = resolve_subagent_spawn_spec(Path::new("/mnt/project"), &req).unwrap();
        assert_eq!(spec.launch.cwd.as_deref(), Some(Path::new("/")));
        assert!(matches!(
            spec.target,
            SpawnTarget::AdHocSubagent { ref role } if role == "planner"
        ));
    }
}
