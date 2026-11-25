//! Host-side workstream ledger & `/agent/<pid>` focus adapter for the foreground TUI.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use alan_agent_engine::{
    InboxDigestStatus, WorkstreamPersistence, WorkstreamState, archive_workstream,
    list_unread_inbox_digests, load_project_ledger, load_workstream_record,
    mark_all_inbox_digests_digested, resolve_bound_project, upsert_workstream,
};
use alan_tui::{
    WorkstreamControl, WorkstreamControlHandler, WorkstreamControlResult, WorkstreamCountsDto,
    WorkstreamEntryDto, WorkstreamFocusTarget, WorkstreamInboxEntryDto, WorkstreamOverviewDto,
    WorkstreamPersistenceKind, WorkstreamStatusKind,
};
use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Load initial read-only workstream & inbox overview for a bound project directory.
///
/// Returns `None` when `ephemeral` is true, when `candidate_cwd` is absent, or when
/// the directory is not explicitly bound in System Store.
pub(super) fn load_initial_workstream_overview(
    store: &alan_os_host::SystemStorePaths,
    candidate_cwd: Option<&Path>,
    ephemeral: bool,
) -> Option<WorkstreamOverviewDto> {
    if ephemeral {
        return None;
    }
    let dir = candidate_cwd?;
    let projects_root = store.projects().ok()?;
    let binding = resolve_bound_project(&projects_root, dir).ok()??;
    build_overview(&projects_root, &binding.project_key, &BTreeMap::new()).ok()
}

/// Build a `WorkstreamControlHandler` backed by System Store Continuity + `/agent` namespace inspection.
pub(super) fn build_workstream_control_handler(
    projects_root: Option<PathBuf>,
    candidate_cwd: Option<PathBuf>,
    root_transport: alan_ap::InProcessTransport,
    ephemeral: bool,
) -> WorkstreamControlHandler {
    Arc::new(move |control| {
        let projects_root = projects_root.clone();
        let candidate_cwd = candidate_cwd.clone();
        let root_transport = root_transport.clone();
        Box::pin(async move {
            let shell = alan_shell::Shell::new(root_transport);
            let live_pids = discover_live_stream_pids(&shell).await;
            let bound = if !ephemeral
                && let (Some(root), Some(cwd)) =
                    (projects_root.as_deref(), candidate_cwd.as_deref())
            {
                resolve_bound_project(root, cwd)?
                    .map(|binding| (root.to_path_buf(), binding.project_key))
            } else {
                None
            };

            match control {
                WorkstreamControl::Inspect => {
                    if let Some((root, project_key)) = bound {
                        let overview = build_overview(&root, &project_key, &live_pids)?;
                        Ok(WorkstreamControlResult::Overview(overview))
                    } else {
                        Ok(WorkstreamControlResult::Overview(
                            build_ephemeral_overview(&shell, &live_pids).await,
                        ))
                    }
                }
                WorkstreamControl::ResolveFocus { name } => {
                    if let Some(&pid) = live_pids.get(&name) {
                        let agent_path = format!("/agent/{pid}");
                        if shell.ls(&agent_path).await.is_ok() {
                            return Ok(WorkstreamControlResult::FocusResolved(
                                WorkstreamFocusTarget {
                                    stream_name: name,
                                    agent_path,
                                    pid: Some(pid),
                                },
                            ));
                        }
                    }
                    if let Some(target) = scan_live_children_by_name(&shell, &name).await {
                        return Ok(WorkstreamControlResult::FocusResolved(target));
                    }
                    if let Some((root, project_key)) = bound
                        && let Some(record) = load_workstream_record(&root, &project_key, &name)?
                    {
                        let message = match record.state {
                            WorkstreamState::Running => format!(
                                "Stream '{name}' is marked running without an active /agent/<pid> process."
                            ),
                            WorkstreamState::Idle => format!(
                                "Stream '{name}' is idle; ask Root to continue '{name}' before focusing."
                            ),
                            WorkstreamState::PausedError => format!(
                                "Stream '{name}' is paused after errors; run `/streams resume {name}` first."
                            ),
                            WorkstreamState::Archived => format!(
                                "Stream '{name}' is archived and has no live /agent/<pid> process."
                            ),
                        };
                        return Ok(WorkstreamControlResult::FocusUnavailable { name, message });
                    }
                    Ok(WorkstreamControlResult::FocusUnavailable {
                        message: format!(
                            "Unknown stream '{name}'. Run /streams to inspect available workstreams."
                        ),
                        name,
                    })
                }
                WorkstreamControl::Archive { name } => {
                    let Some((root, project_key)) = bound else {
                        bail!("archiving workstreams requires an explicitly bound project");
                    };
                    archive_workstream(&root, &project_key, &name, None)?;
                    let overview = build_overview(&root, &project_key, &live_pids)?;
                    Ok(WorkstreamControlResult::ActionCompleted {
                        message: format!("Archived workstream '{name}'."),
                        overview,
                    })
                }
                WorkstreamControl::ClearInbox => {
                    let Some((root, project_key)) = bound else {
                        bail!("clearing inbox digests requires an explicitly bound project");
                    };
                    let cleared = mark_all_inbox_digests_digested(&root, &project_key)?;
                    let overview = build_overview(&root, &project_key, &live_pids)?;
                    Ok(WorkstreamControlResult::ActionCompleted {
                        message: format!("Acknowledged {} inbox digest(s).", cleared.len()),
                        overview,
                    })
                }
                WorkstreamControl::Resume { name } => {
                    let Some((root, project_key)) = bound else {
                        bail!("resuming workstreams requires an explicitly bound project");
                    };
                    let mut record = load_workstream_record(&root, &project_key, &name)?
                        .with_context(|| format!("workstream '{name}' not found"))?;
                    let message = match record.state {
                        WorkstreamState::PausedError => {
                            record.state = WorkstreamState::Idle;
                            if let Some(policy) = record.cron_binding.as_mut() {
                                policy.consecutive_failures = 0;
                            }
                            record.updated_at = chrono::Utc::now().to_rfc3339();
                            upsert_workstream(&root, &project_key, record)?;
                            format!(
                                "Cleared paused error on '{name}'; stream is now idle and ready for Root."
                            )
                        }
                        WorkstreamState::Idle => {
                            format!("Stream '{name}' is already idle and ready.")
                        }
                        WorkstreamState::Running => {
                            format!("Stream '{name}' is currently running; use `/focus {name}`.")
                        }
                        WorkstreamState::Archived => {
                            format!("Stream '{name}' is archived.")
                        }
                    };
                    let overview = build_overview(&root, &project_key, &live_pids)?;
                    Ok(WorkstreamControlResult::ActionCompleted { message, overview })
                }
            }
        })
    })
}

fn build_overview(
    projects_root: &Path,
    project_key: &str,
    live_pids: &BTreeMap<String, u64>,
) -> Result<WorkstreamOverviewDto> {
    let ledger = load_project_ledger(projects_root, project_key)?;
    let unread = list_unread_inbox_digests(projects_root, project_key)?;
    let counts = ledger.counts();

    let streams = ledger
        .streams
        .iter()
        .map(|record| WorkstreamEntryDto {
            name: record.name.clone(),
            scope: record.scope.clone(),
            persistence: match record.persistence {
                WorkstreamPersistence::Persistent => WorkstreamPersistenceKind::Persistent,
                WorkstreamPersistence::OneOff => WorkstreamPersistenceKind::OneOff,
            },
            state: match record.state {
                WorkstreamState::Running => WorkstreamStatusKind::Running,
                WorkstreamState::Idle => WorkstreamStatusKind::Idle,
                WorkstreamState::Archived => WorkstreamStatusKind::Archived,
                WorkstreamState::PausedError => WorkstreamStatusKind::PausedError,
            },
            child_pid: live_pids.get(&record.name).copied(),
            model_override: record.model_override.clone(),
            rollout_ref: record.rollout_ref.clone(),
            updated_at: record.updated_at.clone(),
        })
        .collect();

    let unread_inbox = unread
        .into_iter()
        .map(|entry| WorkstreamInboxEntryDto {
            run_id: entry.run_id,
            workflow_id: entry.workflow_id,
            stream_name: entry.stream_name,
            status: match entry.status {
                InboxDigestStatus::Ok => "ok",
                InboxDigestStatus::Warning => "warning",
                InboxDigestStatus::PausedError => "paused_error",
            }
            .to_string(),
            summary: entry.summary,
            created_at: entry.created_at,
        })
        .collect::<Vec<_>>();

    Ok(WorkstreamOverviewDto {
        project_key: Some(project_key.to_string()),
        counts: WorkstreamCountsDto {
            running: counts.running,
            idle: counts.idle,
            paused_error: counts.paused_error,
            archived: counts.archived,
            unread_inbox: unread_inbox.len(),
        },
        streams,
        unread_inbox,
    })
}

async fn build_ephemeral_overview(
    shell: &alan_shell::Shell,
    live_pids: &BTreeMap<String, u64>,
) -> WorkstreamOverviewDto {
    let mut streams = Vec::new();
    for (name, pid) in live_pids {
        streams.push(WorkstreamEntryDto {
            name: name.clone(),
            scope: String::new(),
            persistence: WorkstreamPersistenceKind::OneOff,
            state: WorkstreamStatusKind::Running,
            child_pid: Some(*pid),
            model_override: None,
            rollout_ref: None,
            updated_at: String::new(),
        });
    }
    if let Ok(children) = shell.ls("/agent/root/children").await {
        for child in children {
            let Ok(pid) = child.trim().parse::<u64>() else {
                continue;
            };
            if !streams.iter().any(|entry| entry.child_pid == Some(pid)) {
                streams.push(WorkstreamEntryDto {
                    name: format!("pid-{pid}"),
                    scope: format!("/agent/{pid}"),
                    persistence: WorkstreamPersistenceKind::OneOff,
                    state: WorkstreamStatusKind::Running,
                    child_pid: Some(pid),
                    model_override: None,
                    rollout_ref: None,
                    updated_at: String::new(),
                });
            }
        }
    }
    WorkstreamOverviewDto {
        project_key: None,
        counts: WorkstreamCountsDto {
            running: streams.len(),
            idle: 0,
            paused_error: 0,
            archived: 0,
            unread_inbox: 0,
        },
        streams,
        unread_inbox: Vec::new(),
    }
}

async fn discover_live_stream_pids(shell: &alan_shell::Shell) -> BTreeMap<String, u64> {
    let mut map = BTreeMap::new();
    let Ok(actions) = shell.ls("/agent/root/actions").await else {
        return map;
    };
    for id in actions {
        if id == "clone" || id == "events" || id == "help" {
            continue;
        }
        let Ok(status_bytes) = shell.cat(&format!("/agent/root/actions/{id}/status")).await else {
            continue;
        };
        let status = String::from_utf8_lossy(&status_bytes);
        if !matches!(status.trim(), "running" | "pending") {
            continue;
        }
        let Ok(result_bytes) = shell.cat(&format!("/agent/root/actions/{id}/result")).await else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<Value>(&result_bytes) else {
            continue;
        };
        if let (Some(name), Some(pid)) = (
            meta.get("stream_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty()),
            meta.get("child_pid")
                .and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
                })
                .filter(|pid| *pid > 0),
        ) {
            map.insert(name.to_string(), pid);
        }
    }
    map
}

async fn scan_live_children_by_name(
    shell: &alan_shell::Shell,
    name: &str,
) -> Option<WorkstreamFocusTarget> {
    if let Ok(pid) = name.parse::<u64>() {
        let path = format!("/agent/{pid}");
        if shell.ls(&path).await.is_ok() {
            return Some(WorkstreamFocusTarget {
                stream_name: name.to_string(),
                agent_path: path,
                pid: Some(pid),
            });
        }
    }
    let children = shell.ls("/agent/root/children").await.ok()?;
    for child in children {
        let Ok(pid) = child.trim().parse::<u64>() else {
            continue;
        };
        let agent_path = format!("/agent/{pid}");
        if shell.ls(&agent_path).await.is_err() {
            continue;
        }
        if let Ok(args_bytes) = shell.cat(&format!("/proc/{pid}/args")).await {
            let args = String::from_utf8_lossy(&args_bytes);
            if args.lines().any(|line| line.trim() == name) {
                return Some(WorkstreamFocusTarget {
                    stream_name: name.to_string(),
                    agent_path,
                    pid: Some(pid),
                });
            }
        }
    }
    None
}
