//! Named workstream lifecycle, live grant/budget revalidation, and checkpoint tracking.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use alan_agent_protocol::{SpawnHostMount, SpawnMountAccess};
use tracing::warn;

use crate::agent_machine::pending_resume::ResumeTarget;
use crate::agent_machine::task_budget::Phase;
use crate::rollout::RolloutItem;
use crate::skills::DelegatedSkillOutputRef;

use super::super::delegated_child_run::{ChildRuntimeResult, ChildRuntimeStatus};
use super::super::delegated_skill_tool::DelegatedSkillRuntime;
use super::super::project_startup::ProjectWorkstreamStoreRef;
use super::super::project_workstreams::{
    StreamLeaseAcquireResult, StreamLeaseGuard, StreamLeaseHolderKind, WorkstreamPersistence,
    WorkstreamRecord, WorkstreamState, WorkstreamWriteMode, archive_workstream,
    complete_workstream, idle_workstream, load_workstream_record, pause_workstream_error,
    resume_workstream, upsert_workstream,
};
use super::super::write_isolation::{
    IsolatedWorkspaceAllocation, IsolatedWorkspaceRequest, SharedWriteQueueRegistry,
    WorkspaceIsolationPort, WorkspaceMergeHandoff, resolve_effective_write_mode,
};
use super::SubagentSpawnRequest;
use super::batch::PrelaunchFailure;

/// Workstream metadata carried on a prepared/executed `spawn_subagent` slot.
#[derive(Debug, Clone)]
pub(in crate::runtime) struct TrackedWorkstream {
    pub(in crate::runtime) store: ProjectWorkstreamStoreRef,
    pub(in crate::runtime) name: String,
    pub(in crate::runtime) persistence: WorkstreamPersistence,
    #[allow(
        dead_code,
        reason = "RAII per-stream lease guard held alive for the duration of the tracked workstream"
    )]
    pub(in crate::runtime) lease: Option<Arc<StreamLeaseGuard>>,
}

/// Allocated isolated workspace handle held during child execution and finalized on completion.
#[derive(Debug, Clone)]
pub(in crate::runtime) struct IsolatedWorkspaceContext {
    pub(in crate::runtime) port: Arc<dyn WorkspaceIsolationPort>,
    pub(in crate::runtime) allocation: IsolatedWorkspaceAllocation,
    pub(in crate::runtime) workstream_name: String,
    pub(in crate::runtime) why: String,
    pub(in crate::runtime) replaced_mount_target: PathBuf,
}

impl IsolatedWorkspaceContext {
    pub(in crate::runtime) async fn collect_handoff(
        &self,
        rollout_ref: Option<&str>,
    ) -> Option<WorkspaceMergeHandoff> {
        match self
            .port
            .collect_handoff(
                &self.allocation,
                &self.workstream_name,
                rollout_ref,
                &self.why,
            )
            .await
        {
            Ok(handoff) => Some(handoff),
            Err(error) => {
                warn!(
                    workspace_ref = %self.allocation.workspace_ref,
                    %error,
                    "Failed to collect isolated workspace merge handoff"
                );
                None
            }
        }
    }
}

/// Tiered write-isolation plan resolved before spawning a child Agent Process.
#[derive(Debug, Clone)]
pub(in crate::runtime) struct PreparedWriteIsolation {
    pub(in crate::runtime) write_mode: WorkstreamWriteMode,
    pub(in crate::runtime) queue: SharedWriteQueueRegistry,
    pub(in crate::runtime) shared_write_grants: Vec<String>,
    pub(in crate::runtime) isolated_workspace: Option<IsolatedWorkspaceContext>,
}

/// Prepared workstream resolution before spawning a child Agent Process.
#[derive(Debug, Clone)]
pub(in crate::runtime) struct PreparedWorkstreamLaunch {
    pub(in crate::runtime) tracked: Option<TrackedWorkstream>,
    pub(in crate::runtime) checkpoint_resume: Option<ResumeTarget>,
    pub(in crate::runtime) effective_model: Option<String>,
    pub(in crate::runtime) write_isolation: PreparedWriteIsolation,
}

fn prelaunch_err(code: &str, message: impl Into<String>) -> Box<PrelaunchFailure> {
    Box::new(PrelaunchFailure {
        status: code.to_string(),
        error_kind: code.to_string(),
        message: message.into(),
        delegated_result: None,
    })
}

/// Revalidate live Host Mount grants against `/mnt/host-mount/grants` and `ToolExecutionAuthority`.
pub(in crate::runtime) async fn revalidate_live_host_mounts(
    runtime: &DelegatedSkillRuntime<'_>,
    host_mounts: &[SpawnHostMount],
) -> std::result::Result<(), Box<PrelaunchFailure>> {
    if host_mounts.is_empty() {
        return Ok(());
    }

    match runtime
        .host_mount_requests
        .validate_live_grants(host_mounts)
        .await
    {
        Ok(Some(reason)) => return Err(prelaunch_err("grant_revoked", reason)),
        Ok(None) => {}
        Err(err) => {
            return Err(prelaunch_err(
                "grant_validation_failed",
                format!("Failed to revalidate live Host Mount grants: {err}"),
            ));
        }
    }

    if let Some(reason) = runtime
        .tool_execution()
        .validate_live_host_mounts(host_mounts)
    {
        return Err(prelaunch_err("grant_revoked", reason));
    }

    Ok(())
}

/// Revalidate that the live `task_budget` has remaining allowance before resuming or starting
/// a persistent workstream.
pub(in crate::runtime) async fn revalidate_live_task_budget(
    runtime: &DelegatedSkillRuntime<'_>,
) -> std::result::Result<(), Box<PrelaunchFailure>> {
    if runtime.machine.task_budget_wait.is_some() || runtime.machine.inherited_budget_wait.is_some()
    {
        return Err(prelaunch_err(
            "task_budget_exhausted",
            "Cannot resume workstream while task_budget is paused awaiting extension.",
        ));
    }

    let Some(budget) = runtime.machine.task_budget.as_ref() else {
        return Ok(());
    };
    let snapshot = budget.snapshot().await;
    let time_limit_ms = snapshot.limits.execution_seconds.get().saturating_mul(1000);
    if matches!(snapshot.phase, Phase::Paused | Phase::Completed)
        || snapshot.elapsed_millis >= time_limit_ms
        || snapshot.tool_calls >= snapshot.limits.tool_calls.get()
        || snapshot.model_calls >= snapshot.limits.model_calls.get()
    {
        return Err(prelaunch_err(
            "task_budget_exhausted",
            "Cannot resume workstream because live task_budget has no remaining allowance.",
        ));
    }

    Ok(())
}

/// Validate and transition/prepare a named workstream before spawning its child Agent Process.
pub(in crate::runtime) async fn prepare_workstream_launch(
    runtime: &DelegatedSkillRuntime<'_>,
    request: &SubagentSpawnRequest,
) -> std::result::Result<PreparedWorkstreamLaunch, Box<PrelaunchFailure>> {
    revalidate_live_host_mounts(runtime, &request.host_mounts).await?;

    let write_isolation = prepare_write_isolation(runtime, request, None).await?;

    let Some(stream_name) = request.stream_name.as_deref() else {
        if request.persistence == Some(WorkstreamPersistence::Persistent) {
            return Err(prelaunch_err(
                "invalid_request",
                "Persistent workstreams require a non-empty stream_name.",
            ));
        }
        return Ok(PreparedWorkstreamLaunch {
            tracked: None,
            checkpoint_resume: None,
            effective_model: request.model.clone(),
            write_isolation,
        });
    };

    if runtime.async_workstreams().is_stream_active(stream_name) {
        return Err(prelaunch_err(
            "workstream_running",
            format!("Workstream '{stream_name}' is already running."),
        ));
    }

    let Some(store) = runtime.workstream_store().cloned() else {
        if request.persistence == Some(WorkstreamPersistence::Persistent) {
            return Err(prelaunch_err(
                "project_not_bound",
                "Persistent named workstreams require an explicitly bound non-ephemeral project.",
            ));
        }
        // Ephemeral or unbound sessions never load or mutate ledger.json for one-off runs.
        return Ok(PreparedWorkstreamLaunch {
            tracked: None,
            checkpoint_resume: None,
            effective_model: request.model.clone(),
            write_isolation,
        });
    };

    let existing = load_workstream_record(&store.projects_root, &store.project_key, stream_name)
        .map_err(|err| {
            prelaunch_err(
                "workstream_store_error",
                format!("Failed to load workstream '{stream_name}': {err}"),
            )
        })?;

    if let Some(existing) = existing {
        match existing.state {
            WorkstreamState::Running => {
                return Err(prelaunch_err(
                    "workstream_running",
                    format!("Workstream '{stream_name}' is already running."),
                ));
            }
            WorkstreamState::PausedError => {
                return Err(prelaunch_err(
                    "workstream_paused_error",
                    format!("Workstream '{stream_name}' is paused due to previous errors."),
                ));
            }
            WorkstreamState::Idle => {
                if existing.persistence != WorkstreamPersistence::Persistent {
                    return Err(prelaunch_err(
                        "workstream_invalid_state",
                        format!(
                            "Non-persistent workstream '{stream_name}' cannot be resumed from idle."
                        ),
                    ));
                }
                if request.persistence == Some(WorkstreamPersistence::OneOff) {
                    return Err(prelaunch_err(
                        "workstream_persistence_mismatch",
                        format!(
                            "Workstream '{stream_name}' is persistent and cannot be launched as one_off."
                        ),
                    ));
                }

                revalidate_live_task_budget(runtime).await?;
                let lease = acquire_interactive_stream_lease(&store, stream_name)?;

                let checkpoint_resume =
                    existing
                        .last_checkpoint_ref
                        .as_ref()
                        .map(|ckpt| ResumeTarget {
                            request_id: String::new(),
                            child_pid: None,
                            rollout_path: Some(ckpt.clone()),
                        });
                let effective_model = request
                    .model
                    .clone()
                    .or_else(|| existing.model_override.clone());
                let mut resumed =
                    resume_workstream(&store.projects_root, &store.project_key, stream_name)
                        .map_err(|err| {
                            prelaunch_err(
                                "workstream_store_error",
                                format!("Failed to resume workstream '{stream_name}': {err}"),
                            )
                        })?;

                let mut metadata_updated = false;
                if let Some(scope) = request.scope.as_deref()
                    && resumed.scope != scope
                {
                    resumed.scope = scope.to_string();
                    metadata_updated = true;
                }
                if effective_model != resumed.model_override {
                    resumed.model_override.clone_from(&effective_model);
                    metadata_updated = true;
                }
                if resumed.write_mode != write_isolation.write_mode {
                    resumed.write_mode = write_isolation.write_mode;
                    metadata_updated = true;
                }
                let next_worktree_ref = write_isolation
                    .isolated_workspace
                    .as_ref()
                    .map(|ctx| ctx.allocation.workspace_ref.clone());
                if next_worktree_ref.is_some() && resumed.worktree_ref != next_worktree_ref {
                    resumed.worktree_ref = next_worktree_ref;
                    metadata_updated = true;
                }
                if metadata_updated {
                    resumed = upsert_workstream(&store.projects_root, &store.project_key, resumed)
                        .map_err(|err| {
                            prelaunch_err(
                                "workstream_store_error",
                                format!(
                                    "Failed to update resumed workstream '{stream_name}': {err}"
                                ),
                            )
                        })?;
                }

                return Ok(PreparedWorkstreamLaunch {
                    tracked: Some(TrackedWorkstream {
                        store,
                        name: resumed.name,
                        persistence: WorkstreamPersistence::Persistent,
                        lease: Some(lease),
                    }),
                    checkpoint_resume,
                    effective_model,
                    write_isolation,
                });
            }
            WorkstreamState::Archived => {
                if existing.persistence == WorkstreamPersistence::Persistent
                    && request.persistence == Some(WorkstreamPersistence::Persistent)
                {
                    return Err(prelaunch_err(
                        "workstream_archived",
                        format!(
                            "Archived persistent workstream '{stream_name}' cannot be resumed implicitly."
                        ),
                    ));
                }
                // Reusing an archived one_off stream name starts a fresh record below.
            }
        }
    }

    let persistence = request.effective_persistence();
    let lease = if persistence == WorkstreamPersistence::Persistent {
        revalidate_live_task_budget(runtime).await?;
        Some(acquire_interactive_stream_lease(&store, stream_name)?)
    } else {
        None
    };

    let scope = request
        .scope
        .clone()
        .unwrap_or_else(|| format!("{}: {}", request.role, request.task));
    let mut record = WorkstreamRecord::new(stream_name, scope, persistence);
    record.write_mode = write_isolation.write_mode;
    record.worktree_ref = write_isolation
        .isolated_workspace
        .as_ref()
        .map(|ctx| ctx.allocation.workspace_ref.clone());
    record.model_override.clone_from(&request.model);

    let persisted =
        upsert_workstream(&store.projects_root, &store.project_key, record).map_err(|err| {
            prelaunch_err(
                "workstream_store_error",
                format!("Failed to persist workstream '{stream_name}': {err}"),
            )
        })?;

    Ok(PreparedWorkstreamLaunch {
        tracked: Some(TrackedWorkstream {
            store,
            name: persisted.name,
            persistence,
            lease,
        }),
        checkpoint_resume: None,
        effective_model: request.model.clone(),
        write_isolation,
    })
}

fn acquire_interactive_stream_lease(
    store: &ProjectWorkstreamStoreRef,
    stream_name: &str,
) -> std::result::Result<Arc<StreamLeaseGuard>, Box<PrelaunchFailure>> {
    match StreamLeaseGuard::try_acquire(
        &store.projects_root,
        &store.project_key,
        stream_name,
        StreamLeaseHolderKind::Interactive,
        None,
    ) {
        Ok(StreamLeaseAcquireResult::Acquired(guard)) => Ok(Arc::new(guard)),
        Ok(StreamLeaseAcquireResult::Contention { holder }) => {
            let holder_desc = holder
                .map(|h| format!("{:?} pid={}", h.holder_kind, h.pid))
                .unwrap_or_else(|| "another active session".to_string());
            Err(prelaunch_err(
                "workstream_locked",
                format!("Workstream '{stream_name}' is currently locked by {holder_desc}."),
            ))
        }
        Err(err) => Err(prelaunch_err(
            "workstream_lease_error",
            format!("Failed to acquire lease lock for workstream '{stream_name}': {err}"),
        )),
    }
}

async fn prepare_write_isolation(
    runtime: &DelegatedSkillRuntime<'_>,
    request: &SubagentSpawnRequest,
    rollout_ref: Option<&str>,
) -> std::result::Result<PreparedWriteIsolation, Box<PrelaunchFailure>> {
    let queue = runtime.write_isolation_queue().clone();
    let write_mode = resolve_effective_write_mode(
        request.read_only,
        request.write_mode,
        request.long_running,
        &request.host_mounts,
        &queue,
    )
    .map_err(|msg| prelaunch_err("write_mode_conflict", msg))?;

    let mut rw_grants: Vec<String> = request
        .host_mounts
        .iter()
        .filter(|m| m.access == SpawnMountAccess::ReadWrite)
        .map(|m| m.grant.clone())
        .collect();
    rw_grants.sort();
    rw_grants.dedup();

    match write_mode {
        WorkstreamWriteMode::ReadOnly => Ok(PreparedWriteIsolation {
            write_mode,
            queue,
            shared_write_grants: Vec::new(),
            isolated_workspace: None,
        }),
        WorkstreamWriteMode::SharedQueue => Ok(PreparedWriteIsolation {
            write_mode,
            queue,
            shared_write_grants: rw_grants,
            isolated_workspace: None,
        }),
        WorkstreamWriteMode::Worktree => {
            let Some(port) = runtime.workspace_isolation().cloned() else {
                return Err(prelaunch_err(
                    "workspace_isolation_unavailable",
                    "Requested `worktree` / `cow_workspace` write isolation, but no WorkspaceIsolationPort is configured.",
                ));
            };
            let primary_rw = request
                .host_mounts
                .iter()
                .find(|m| m.access == SpawnMountAccess::ReadWrite)
                .ok_or_else(|| {
                    prelaunch_err(
                        "write_mode_conflict",
                        "Worktree write isolation requires a read_write host_mount.",
                    )
                })?;
            let workstream_name = request
                .stream_name
                .clone()
                .unwrap_or_else(|| request.role.clone());
            let why = request
                .scope
                .clone()
                .unwrap_or_else(|| request.task.clone());
            let allocation = port
                .allocate_workspace(IsolatedWorkspaceRequest {
                    workstream_name: workstream_name.clone(),
                    source_grant_id: primary_rw.grant.clone(),
                    source_namespace_path: primary_rw.target.clone(),
                    rollout_ref: rollout_ref.map(str::to_string),
                    why: why.clone(),
                })
                .await
                .map_err(|err| {
                    prelaunch_err(
                        "workspace_isolation_failed",
                        format!("Failed to allocate isolated workspace: {err:#}"),
                    )
                })?;
            Ok(PreparedWriteIsolation {
                write_mode,
                queue,
                shared_write_grants: Vec::new(),
                isolated_workspace: Some(IsolatedWorkspaceContext {
                    port,
                    allocation,
                    workstream_name,
                    why,
                    replaced_mount_target: primary_rw.target.clone(),
                }),
            })
        }
    }
}

/// Apply narrowed isolated workspace mount and cwd onto a resolved `SpawnSpec`.
pub(in crate::runtime) fn apply_isolated_workspace_to_spawn_spec(
    spec: &mut alan_agent_protocol::SpawnSpec,
    ctx: &IsolatedWorkspaceContext,
) {
    let mut replaced = false;
    for mount in &mut spec.host_mounts {
        if mount.access == SpawnMountAccess::ReadWrite && mount.target == ctx.replaced_mount_target
        {
            *mount = ctx.allocation.narrowed_grant.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        spec.host_mounts.push(ctx.allocation.narrowed_grant.clone());
    }
    if let Some(cwd) = spec.launch.cwd.as_ref()
        && cwd.starts_with(&ctx.replaced_mount_target)
    {
        spec.launch.cwd = Some(ctx.allocation.narrowed_grant.target.clone());
    }
}

/// Transition a tracked workstream to its terminal/idle state after child execution completes.
pub(in crate::runtime) async fn finalize_tracked_workstream(
    tracked: Option<&TrackedWorkstream>,
    rollouts_dir: Option<&Path>,
    result: &ChildRuntimeResult,
    output_reference: Option<&DelegatedSkillOutputRef>,
) -> Option<WorkstreamRecord> {
    let tracked = tracked?;
    let discovered_rollout = match result.rollout_path.clone() {
        Some(path) => Some(path),
        None => match result
            .pause
            .as_ref()
            .and_then(|pause| pause.inherited.as_ref())
            .map(|inherited| PathBuf::from(&inherited.rollout_path))
        {
            Some(path) => Some(path),
            None => find_recent_rollout_for_process(rollouts_dir, &result.process_path).await,
        },
    };
    let checkpoint_ref = discovered_rollout
        .as_ref()
        .map(|path| path.display().to_string());
    let rollout_ref = checkpoint_ref
        .clone()
        .or_else(|| output_reference.map(|reference| reference.path.clone()))
        .or_else(|| (!result.process_path.is_empty()).then(|| result.process_path.clone()));

    let outcome = match (tracked.persistence, &result.status) {
        (_, ChildRuntimeStatus::Completed) => complete_workstream(
            &tracked.store.projects_root,
            &tracked.store.project_key,
            &tracked.name,
            checkpoint_ref,
            rollout_ref,
        ),
        (WorkstreamPersistence::OneOff, _) => archive_workstream(
            &tracked.store.projects_root,
            &tracked.store.project_key,
            &tracked.name,
            rollout_ref,
        ),
        (WorkstreamPersistence::Persistent, ChildRuntimeStatus::Cancelled) => idle_workstream(
            &tracked.store.projects_root,
            &tracked.store.project_key,
            &tracked.name,
            checkpoint_ref,
            rollout_ref,
        ),
        (WorkstreamPersistence::Persistent, ChildRuntimeStatus::Paused)
            if result
                .pause
                .as_ref()
                .is_some_and(|pause| pause.inherited.is_some()) =>
        {
            idle_workstream(
                &tracked.store.projects_root,
                &tracked.store.project_key,
                &tracked.name,
                checkpoint_ref,
                rollout_ref,
            )
        }
        (WorkstreamPersistence::Persistent, _) => pause_workstream_error(
            &tracked.store.projects_root,
            &tracked.store.project_key,
            &tracked.name,
            checkpoint_ref,
            rollout_ref,
        ),
    };

    match outcome {
        Ok(record) => Some(record),
        Err(error) => {
            warn!(
                stream = %tracked.name,
                project = %tracked.store.project_key,
                %error,
                "Failed to update workstream record after child completion"
            );
            None
        }
    }
}

async fn find_recent_rollout_for_process(
    rollouts_dir: Option<&Path>,
    process_path: &str,
) -> Option<PathBuf> {
    let dir = rollouts_dir?;
    if process_path.trim().is_empty() || !dir.is_dir() {
        return None;
    }
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    let mut best: Option<(String, PathBuf)> = None;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(contents) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let Some(first_line) = contents.lines().next() else {
            continue;
        };
        let Ok(RolloutItem::AgentMachineMeta(meta)) =
            serde_json::from_str::<RolloutItem>(first_line)
        else {
            continue;
        };
        if meta.process_path == process_path
            && best
                .as_ref()
                .is_none_or(|(best_started, _)| meta.started_at > *best_started)
        {
            best = Some((meta.started_at, path));
        }
    }
    best.map(|(_, path)| path)
}

#[cfg(test)]
#[path = "workstream_tests.rs"]
mod tests;
